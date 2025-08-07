use num_cpus;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::thread::{self, JoinHandle};

use imgui_glow_renderer::glow::HasContext;
use std::{num::NonZeroU32, time::Instant};

use glutin::{
    config::ConfigTemplateBuilder,
    context::{ContextAttributesBuilder, NotCurrentGlContext, PossiblyCurrentContext},
    display::{GetGlDisplay, GlDisplay},
    surface::{GlSurface, Surface, SurfaceAttributesBuilder, WindowSurface},
};
use imgui_winit_support::{
    WinitPlatform,
    winit::{
        dpi::LogicalSize,
        event_loop::EventLoop,
        window::{Window, WindowAttributes},
    },
};
use raw_window_handle::HasWindowHandle;

const TITLE: &str = "Maolan";

#[derive(Clone, Debug)]
struct Track {
    id: imnodes::NodeId,
    input: imnodes::InputPinId,
    output: imnodes::OutputPinId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TrackLink {
    id: imnodes::LinkId,
    start_pin: imnodes::OutputPinId,
    end_pin: imnodes::InputPinId,
}

#[derive(Debug)]
pub struct TrackState {
    pub editor_context: imnodes::EditorContext,
    id_gen: imnodes::IdentifierGenerator,
    nodes: Vec<Track>,
    links: Vec<TrackLink>,
    saved_state_string: Option<String>,
    status: String,
    last_selected_nodes: Vec<imnodes::NodeId>,
    last_selected_links: Vec<imnodes::LinkId>,
    add_node_at: Option<[f32; 2]>,
}

impl TrackState {
    pub fn new(context: &imnodes::Context) -> Self {
        let editor_context = context.create_editor();
        let id_gen = editor_context.new_identifier_generator();
        TrackState {
            editor_context,
            id_gen,
            nodes: vec![],
            links: vec![],
            saved_state_string: None,
            status: "Ready".to_string(),
            last_selected_nodes: vec![],
            last_selected_links: vec![],
            add_node_at: None,
        }
    }

    // Helper function to add a node
    fn add(&mut self, position: Option<[f32; 2]>) {
        let node_id = self.id_gen.next_node();
        let new_node = Track {
            id: node_id,
            input: self.id_gen.next_input_pin(),
            output: self.id_gen.next_output_pin(),
        };

        // Set position *before* pushing the node, so it's placed correctly on the first frame
        if let Some(pos) = position {
            let _ = node_id.set_position(pos[0], pos[1], imnodes::CoordinateSystem::ScreenSpace);
        } else {
            // Place the new node near the center of the screen, adjusted by panning.
            let pan = self.editor_context.get_panning();
            // Estimate center - requires access to window size, which we don't have here easily.
            // Place relative to pan for now. A better approach might involve passing window size.
            let node_x = pan.x + 100.0; // Offset from top-left visible corner
            let node_y = pan.y + 100.0;
            let _ = node_id.set_position(node_x, node_y, imnodes::CoordinateSystem::GridSpace);
        }

        self.nodes.push(new_node);
        self.status = format!("Added Node {node_id:?}");
    }
}

fn create_window() -> (
    EventLoop<()>,
    Window,
    Surface<WindowSurface>,
    PossiblyCurrentContext,
) {
    let event_loop = EventLoop::new().unwrap();

    let window_attributes = WindowAttributes::default()
        .with_title(TITLE)
        .with_inner_size(LogicalSize::new(1024, 768));
    let (window, cfg) = glutin_winit::DisplayBuilder::new()
        .with_window_attributes(Some(window_attributes))
        .build(&event_loop, ConfigTemplateBuilder::new(), |mut configs| {
            configs.next().unwrap()
        })
        .expect("Failed to create OpenGL window");

    let window = window.unwrap();

    let context_attribs =
        ContextAttributesBuilder::new().build(Some(window.window_handle().unwrap().as_raw()));
    let context = unsafe {
        cfg.display()
            .create_context(&cfg, &context_attribs)
            .expect("Failed to create OpenGL context")
    };

    let surface_attribs = SurfaceAttributesBuilder::<WindowSurface>::new()
        .with_srgb(Some(true))
        .build(
            window.window_handle().unwrap().as_raw(),
            NonZeroU32::new(1024).unwrap(),
            NonZeroU32::new(768).unwrap(),
        );
    let surface = unsafe {
        cfg.display()
            .create_window_surface(&cfg, &surface_attribs)
            .expect("Failed to create OpenGL surface")
    };

    let context = context
        .make_current(&surface)
        .expect("Failed to make OpenGL context current");

    (event_loop, window, surface, context)
}

fn glow_context(context: &PossiblyCurrentContext) -> glow::Context {
    unsafe {
        glow::Context::from_loader_function_cstr(|s| context.display().get_proc_address(s).cast())
    }
}

fn imgui_init(window: &Window) -> (WinitPlatform, imgui::Context) {
    let mut imgui_context = imgui::Context::create();
    imgui_context.set_ini_filename(None);

    let mut winit_platform = WinitPlatform::new(&mut imgui_context);
    winit_platform.attach_window(
        imgui_context.io_mut(),
        window,
        imgui_winit_support::HiDpiMode::Rounded,
    );

    imgui_context
        .fonts()
        .add_font(&[imgui::FontSource::DefaultFontData { config: None }]);

    imgui_context.io_mut().font_global_scale = (1.0 / winit_platform.hidpi_factor()) as f32;

    (winit_platform, imgui_context)
}

fn show_nodes(ui: &imgui::Ui, track_state: &mut TrackState) {
    let editor_id = ui.push_id_ptr(&track_state.editor_context);
    let _ = track_state.editor_context.set_as_current_editor();

    if ui.button("Add Node") {
        track_state.add_node_at = Some([-1.0, -1.0]); // Sentinel value
    }
    ui.same_line();
    if ui.button("Remove Selected Nodes") {
        if !track_state.last_selected_nodes.is_empty() {
            let mut removed_count = 0;
            let nodes_to_remove = track_state.last_selected_nodes.clone();
            track_state.nodes.retain(|node| {
                if nodes_to_remove.contains(&node.id) {
                    track_state
                        .links
                        .retain(|link| node.input != link.end_pin && node.output != link.start_pin);
                    removed_count += 1;
                    false
                } else {
                    true
                }
            });
            track_state.status = format!("Removed {removed_count} node(s)");
            track_state.editor_context.clear_node_selection();
            track_state.last_selected_nodes.clear();
        } else {
            track_state.status = "No nodes selected to remove".to_string();
        }
    }
    ui.same_line();
    ui.text("or press \"A\" / right-click");

    ui.separator();

    ui.text("Save/Load:");
    if ui.button("Save to String") {
        match track_state
            .editor_context
            .save_current_editor_state_to_string()
        {
            Some(saved_str) => {
                track_state.saved_state_string = Some(saved_str);
                track_state.status = "Saved state to internal string".to_string();
            }
            None => {
                track_state.status = "Failed to save state to string".to_string();
            }
        }
    }
    ui.same_line();
    if ui.button("Load from String") {
        if let Some(saved_str) = &track_state.saved_state_string {
            // Load the imnodes internal state
            track_state
                .editor_context
                .load_current_editor_state_from_string(saved_str);
            track_state.last_selected_nodes.clear();
            track_state.last_selected_links.clear();
            track_state.status =
                "Loaded imnodes state from string. App state assumed to match.".to_string();
        } else {
            track_state.status = "No saved string state to load".to_string();
        }
    }

    ui.separator();

    let current_panning = track_state.editor_context.get_panning();
    ui.text(format!(
        "Current Pan: {:.2}, {:.2}",
        current_panning.x, current_panning.y
    ));
    ui.same_line();
    if ui.button("Reset Pan") {
        track_state
            .editor_context
            .reset_panning(imnodes::ImVec2 { x: 0.0, y: 0.0 });
        track_state.status = "Panning reset".to_string();
    }

    ui.text(format!(
        "Selected Nodes: {}",
        track_state.last_selected_nodes.len()
    ));
    if track_state.last_selected_nodes.len() == 1 {
        let node_id = track_state.last_selected_nodes[0];
        // Check if the node still exists in our app state before getting position
        if track_state.nodes.iter().any(|n| n.id == node_id) {
            let screen_pos = node_id.get_position(imnodes::CoordinateSystem::ScreenSpace);
            let editor_pos = node_id.get_position(imnodes::CoordinateSystem::EditorSpace);
            let grid_pos = node_id.get_position(imnodes::CoordinateSystem::GridSpace);
            ui.text(format!("  Node {node_id:?} Pos:"));
            ui.text(format!(
                "    Screen: {:.1}, {:.1}",
                screen_pos.x, screen_pos.y
            ));
            ui.text(format!(
                "    Editor: {:.1}, {:.1}",
                editor_pos.x, editor_pos.y
            ));
            ui.text(format!("    Grid:   {:.1}, {:.1}", grid_pos.x, grid_pos.y));

            ui.same_line();
            if ui.button("Deselect Node") {
                let _ = node_id.deselect();
            }
            ui.same_line();
            if ui.button("Snap to Grid") {
                let _ = node_id.snap_to_grid();
            }
        } else {
            // Node was likely removed after selection but before redraw
            ui.text(format!("  Node {node_id:?} (removed)"));
        }
    } else if track_state.last_selected_nodes.len() > 1 {
        ui.same_line();
        if ui.button("Clear Node Selection") {
            track_state.editor_context.clear_node_selection();
            track_state.last_selected_nodes.clear();
        }
    }

    ui.text(format!(
        "Selected Links: {}",
        track_state.last_selected_links.len()
    ));
    if !track_state.last_selected_links.is_empty() {
        ui.same_line();
        if ui.button("Clear Link Selection") {
            track_state.editor_context.clear_link_selection();
            track_state.last_selected_links.clear();
        }
    }

    ui.separator();
    ui.text(format!("Status: {}", track_state.status));
    ui.separator();

    // Store context menu click position *outside* the editor closure
    let mut context_menu_pos = None;
    let outer_scope = imnodes::editor(&mut track_state.editor_context, |mut editor_scope| {
        // Detect context menu click *inside* the editor scope
        if editor_scope.is_hovered()
            && (ui.is_key_released(imgui::Key::A) || ui.is_mouse_clicked(imgui::MouseButton::Right))
        {
            // Store the position where the node should be added
            context_menu_pos = Some(ui.io().mouse_pos);
        }

        // Iterate using indices to allow mutable borrow inside slider closure
        for i in 0..track_state.nodes.len() {
            // Need to get these before the mutable borrow below
            let node_id = track_state.nodes[i].id;
            let input_pin = track_state.nodes[i].input;
            let output_pin = track_state.nodes[i].output;
            let attr_id = track_state.id_gen.next_attribute(); // Regenerate attribute ID each frame

            editor_scope.add_node(node_id, |mut node_scope| {
                node_scope.add_titlebar(|| {
                    ui.text(format!("Node {node_id:?}"));
                });

                node_scope.add_input(input_pin, imnodes::PinShape::CircleFilled, || {});

                ui.same_line();

                // Add a simple widget like in multi_editor
                node_scope.add_static_attribute(attr_id, || {
                    if let Some(_node_mut) = track_state.nodes.get_mut(i) {
                        ui.set_next_item_width(80.0);
                    }
                });

                ui.same_line();

                node_scope.add_output(output_pin, imnodes::PinShape::CircleFilled, || {});
            });
        }

        for link_data in &track_state.links {
            editor_scope.add_link(link_data.id, link_data.end_pin, link_data.start_pin);
        }
    });

    // Handle node addition request *after* the editor scope ends
    if let Some(pos) = context_menu_pos {
        track_state.add(Some(pos));
    } else if let Some(pos) = track_state.add_node_at {
        if pos[0] == -1.0 {
            // Check for sentinel value from button click
            track_state.add(None);
        }
        track_state.add_node_at = None; // Reset request
    }

    // Update stored selections for the *next* frame
    track_state.last_selected_nodes = outer_scope.selected_nodes();
    track_state.last_selected_links = outer_scope.selected_links();

    if let Some(new_link) = outer_scope.links_created() {
        let new_app_link = TrackLink {
            id: track_state.id_gen.next_link(),
            start_pin: new_link.start_pin,
            end_pin: new_link.end_pin,
        };
        if !track_state
            .links
            .iter()
            .any(|l| l.start_pin == new_app_link.start_pin && l.end_pin == new_app_link.end_pin)
        {
            track_state.links.push(new_app_link);
            track_state.status = format!(
                "Created Link from pin {:?} to {:?}",
                new_link.start_pin, new_link.end_pin
            );
        }
    }

    if let Some(destroyed_link_id) = outer_scope.get_destroyed_link() {
        let initial_len = track_state.links.len();
        track_state
            .links
            .retain(|link| link.id != destroyed_link_id);
        if track_state.links.len() < initial_len {
            track_state.status = format!("Removed Link {destroyed_link_id:?}");
        }
    }

    // Pop the editor's unique ID
    editor_id.pop();
}

pub fn run() {
    let (event_loop, window, surface, context) = create_window();
    let (mut winit_platform, mut imgui_context) = imgui_init(&window);
    let imnodes_ui = imnodes::Context::new();
    let mut track_state = TrackState::new(&imnodes_ui);

    // OpenGL context from glow
    let gl = glow_context(&context);

    // OpenGL renderer from this crate
    let mut ig_renderer = imgui_glow_renderer::AutoRenderer::new(gl, &mut imgui_context)
        .expect("failed to create renderer");

    let mut last_frame = Instant::now();

    // Standard winit event loop
    #[allow(deprecated)]
    event_loop
        .run(move |event, window_target| {
            match event {
                winit::event::Event::NewEvents(_) => {
                    let now = Instant::now();
                    imgui_context
                        .io_mut()
                        .update_delta_time(now.duration_since(last_frame));
                    last_frame = now;
                }
                winit::event::Event::AboutToWait => {
                    winit_platform
                        .prepare_frame(imgui_context.io_mut(), &window)
                        .unwrap();
                    window.request_redraw();
                }
                winit::event::Event::WindowEvent {
                    event: winit::event::WindowEvent::RedrawRequested,
                    ..
                } => {
                    // The renderer assumes you'll be clearing the buffer yourself
                    unsafe { ig_renderer.gl_context().clear(glow::COLOR_BUFFER_BIT) };

                    let ui = imgui_context.frame();
                    show_nodes(ui, &mut track_state);

                    winit_platform.prepare_render(ui, &window);
                    let draw_data = imgui_context.render();

                    // This is the only extra render step to add
                    ig_renderer
                        .render(draw_data)
                        .expect("error rendering imgui");

                    surface
                        .swap_buffers(&context)
                        .expect("Failed to swap buffers");
                }
                winit::event::Event::WindowEvent {
                    event: winit::event::WindowEvent::CloseRequested,
                    ..
                } => {
                    window_target.exit();
                }
                winit::event::Event::WindowEvent {
                    event: winit::event::WindowEvent::Resized(new_size),
                    ..
                } => {
                    if new_size.width > 0 && new_size.height > 0 {
                        surface.resize(
                            &context,
                            NonZeroU32::new(new_size.width).unwrap(),
                            NonZeroU32::new(new_size.height).unwrap(),
                        );
                    }
                    winit_platform.handle_event(imgui_context.io_mut(), &window, &event);
                }
                event => {
                    winit_platform.handle_event(imgui_context.io_mut(), &window, &event);
                }
            }
        })
        .expect("EventLoop error");
}

fn work(id: usize, tx: Sender<usize>, rx: Receiver<usize>) {
    // for _id in &rx {}
    tx.send(id).unwrap();
}

#[derive(Debug)]
pub struct Engine {
    rx: Receiver<usize>,
    threads: Vec<JoinHandle<()>>,
    txs: Vec<Sender<usize>>,
}

impl Engine {
    pub fn new() -> Self {
        let max_threads = num_cpus::get();
        let (tx, rx) = mpsc::channel();

        let mut engine = Engine {
            rx,
            threads: vec![],
            txs: vec![],
        };

        for id in 0..max_threads {
            let (engine_tx, thread_rx) = mpsc::channel();
            let thread_tx = tx.clone();
            let thread = thread::spawn(move || work(id, thread_tx, thread_rx));
            engine.txs.push(engine_tx);
            engine.threads.push(thread);
        }
        engine
    }

    pub fn read(&mut self) {
        for id in &self.rx {
            println!("received id {}", id);
        }
        self.join();
    }

    fn join(&mut self) {
        while self.threads.len() > 0 {
            let thread = self.threads.remove(0);
            let _ = thread.join().expect("Thread panicked");
        }
    }
}
