# VST3 Remaining Work - Phases 4 & 5

## Current Status: 3 of 5 Phases Complete ✅

**Completed:**
- ✅ Phase 1: Basic Audio Processing
- ✅ Phase 2: MIDI Support (API layer complete)
- ✅ Phase 3: State Save/Restore

**Remaining:**
- ⏳ Phase 4: Plugin GUI Integration
- ⏳ Phase 5: Engine Messages and UI Integration

---

## Phase 4: Plugin GUI Integration

### Overview
Enable VST3 plugin editors to display within Maolan's UI, allowing users to interact with native plugin GUIs.

### Files to Create
1. **`engine/src/vst3/gui.rs`** (~500 lines)
   - `PluginWindow` struct with platform-specific windowing
   - `PluginEditor` wrapping VST3's `IPlugView`
   - Window lifecycle management (create, attach, detach, resize, destroy)

### Platform-Specific Implementation

#### Linux/FreeBSD (X11)
```rust
pub struct PluginWindow {
    x11_window: u64,  // X11 Window ID
    display: *mut Display,
    width: u32,
    height: u32,
}

impl PluginWindow {
    pub fn new() -> Result<Self, String> {
        // XOpenDisplay, XCreateSimpleWindow
        // Return window ID for IPlugView::attached()
    }

    pub fn raw_handle(&self) -> RawWindowHandle {
        RawWindowHandle::Xlib(XlibWindowHandle {
            window: self.x11_window,
            display: self.display as *mut _,
            ..Default::default()
        })
    }
}
```

**Dependencies needed:**
```toml
[target.'cfg(any(target_os = "linux", target_os = "freebsd"))'.dependencies]
raw-window-handle = "0.6"
x11 = { version = "2.21", features = ["xlib"] }
```

#### Windows
```rust
pub struct PluginWindow {
    hwnd: windows::Win32::Foundation::HWND,
    width: u32,
    height: u32,
}

impl PluginWindow {
    pub fn new() -> Result<Self, String> {
        // CreateWindowExW with WS_CHILD style
        // Return HWND for IPlugView::attached()
    }
}
```

**Dependencies needed:**
```toml
[target.'cfg(target_os = "windows")'.dependencies]
raw-window-handle = "0.6"
windows = { version = "0.52", features = [
    "Win32_UI_WindowsAndMessaging",
    "Win32_Foundation",
    "Win32_Graphics_Gdi"
] }
```

#### macOS
```rust
pub struct PluginWindow {
    nsview: *mut objc::runtime::Object,  // NSView*
    width: u32,
    height: u32,
}

impl PluginWindow {
    pub fn new() -> Result<Self, String> {
        // Create NSView using objc
        // Return NSView* for IPlugView::attached()
    }
}
```

**Dependencies needed:**
```toml
[target.'cfg(target_os = "macos")'.dependencies]
raw-window-handle = "0.6"
cocoa = "0.25"
objc = "0.2"
```

### IPlugView Wrapper

```rust
pub struct PluginEditor {
    view: ComPtr<IPlugView>,
    window: PluginWindow,
    is_attached: bool,
}

impl PluginEditor {
    pub fn new(processor: &mut Vst3Processor) -> Result<Self, String> {
        // Get IEditController from processor
        let controller = processor.instance.edit_controller.as_ref()?;

        // Create IPlugView
        let view = unsafe {
            controller.createView(b"editor\0".as_ptr() as *const _)?
        };

        // Query preferred size
        let mut rect = ViewRect { left: 0, top: 0, right: 0, bottom: 0 };
        unsafe { view.getSize(&mut rect)?; }

        // Create platform window
        let window = PluginWindow::new()?;
        window.resize(
            (rect.right - rect.left) as u32,
            (rect.bottom - rect.top) as u32
        )?;

        Ok(Self { view, window, is_attached: false })
    }

    pub fn attach(&mut self) -> Result<(), String> {
        if self.is_attached { return Ok(()); }

        let handle = self.window.raw_handle();

        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        let parent_ptr = handle.window as *mut _;

        #[cfg(target_os = "windows")]
        let parent_ptr = handle.hwnd as *mut _;

        #[cfg(target_os = "macos")]
        let parent_ptr = handle.ns_view as *mut _;

        unsafe {
            self.view.attached(parent_ptr, b"X11EmbedNSView\0".as_ptr() as *const _)?;
        }

        self.is_attached = true;
        Ok(())
    }

    pub fn detach(&mut self) -> Result<(), String> {
        if !self.is_attached { return Ok(()); }
        unsafe { self.view.removed()?; }
        self.is_attached = false;
        Ok(())
    }

    pub fn on_size(&mut self, width: u32, height: u32) -> Result<(), String> {
        self.window.resize(width, height)?;

        if self.is_attached {
            let rect = ViewRect {
                left: 0,
                top: 0,
                right: width as i32,
                bottom: height as i32,
            };
            unsafe { self.view.onSize(&rect)?; }
        }

        Ok(())
    }
}

impl Drop for PluginEditor {
    fn drop(&mut self) {
        let _ = self.detach();
    }
}
```

### Add to Vst3Processor

```rust
impl Vst3Processor {
    pub fn has_editor(&self) -> bool {
        self.instance
            .as_ref()
            .and_then(|i| i.edit_controller.as_ref())
            .map(|c| unsafe {
                c.createView(b"editor\0".as_ptr() as *const _).is_ok()
            })
            .unwrap_or(false)
    }

    pub fn create_editor(&mut self) -> Result<PluginEditor, String> {
        PluginEditor::new(self)
    }
}
```

### Iced Integration

Create `src/plugin_editor_widget.rs`:

```rust
use iced::{widget, Element, Task};
use maolan_engine::vst3::PluginEditor;

pub struct PluginEditorWidget {
    editor: Option<PluginEditor>,
    track_name: String,
    instance_id: u64,
    width: u32,
    height: u32,
}

impl PluginEditorWidget {
    pub fn new(track_name: String, instance_id: u64) -> Self {
        Self {
            editor: None,
            track_name,
            instance_id,
            width: 800,
            height: 600,
        }
    }

    pub fn set_editor(&mut self, editor: PluginEditor) {
        self.width = editor.width();
        self.height = editor.height();
        self.editor = Some(editor);
    }

    pub fn view(&self) -> Element<Message> {
        if let Some(_editor) = &self.editor {
            // Native widget embedding - platform specific
            // On X11: use iced's native window handle
            // On Windows: embed HWND as child
            // On macOS: embed NSView

            widget::container(widget::text("VST3 Plugin Editor"))
                .width(self.width)
                .height(self.height)
                .into()
        } else {
            widget::text("Loading plugin editor...").into()
        }
    }
}
```

### Challenges
1. **Iced Native Widget Embedding**: Iced may not have direct support for embedding native windows
   - May need to use iced's `canvas` widget as a placeholder
   - Or use `winit` window handles directly

2. **Event Routing**: Plugin GUI needs mouse/keyboard events
   - Must route iced events to native window
   - Handle focus management

3. **Resize Handling**: Plugin GUIs can resize themselves
   - Need bidirectional size negotiation
   - Update iced layout on plugin resize

4. **Platform Differences**:
   - X11 window reparenting vs Windows child windows vs macOS NSView hierarchy
   - Different event models per platform

### Testing Plan
1. Load plugin with GUI
2. Call `create_editor()` and verify window appears
3. Click plugin controls, verify parameter changes
4. Resize editor window
5. Close and reopen editor
6. Test on each platform (Linux, FreeBSD, Windows, macOS)

### Estimated Effort
- Implementation: 1-2 weeks
- Testing: 3-5 days per platform
- Debugging platform-specific issues: 1 week

---

## Phase 5: Engine Messages and UI Integration

### Overview
Integrate VST3 into Maolan's engine message system and GUI, enabling full user interaction with VST3 plugins.

### Engine Message Changes

#### 1. Add Actions to `engine/src/message.rs`

```rust
pub enum Action {
    // ... existing actions ...

    // VST3 plugin lifecycle
    TrackLoadVst3Plugin {
        track_name: String,
        plugin_path: String,
    },
    TrackUnloadVst3PluginInstance {
        track_name: String,
        instance_id: u64,
    },

    // VST3 graph queries
    TrackGetVst3Graph {
        track_name: String,
    },
    TrackVst3Graph {
        track_name: String,
        plugins: Vec<Vst3GraphPlugin>,
        connections: Vec<Vst3GraphConnection>,
    },

    // VST3 parameters
    TrackSetVst3Parameter {
        track_name: String,
        instance_id: u64,
        param_id: u32,
        value: f32,
    },
    TrackGetVst3Parameters {
        track_name: String,
        instance_id: u64,
    },
    TrackVst3Parameters {
        track_name: String,
        instance_id: u64,
        parameters: Vec<ParameterInfo>,
    },

    // VST3 state
    TrackVst3SnapshotState {
        track_name: String,
        instance_id: u64,
    },
    TrackVst3RestoreState {
        track_name: String,
        instance_id: u64,
        state: Vst3PluginState,
    },

    // VST3 GUI
    TrackOpenVst3Editor {
        track_name: String,
        instance_id: u64,
    },
    TrackCloseVst3Editor {
        track_name: String,
        instance_id: u64,
    },
    TrackGetVst3Editor {
        track_name: String,
        instance_id: u64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Vst3GraphPlugin {
    pub id: u64,
    pub name: String,
    pub audio_inputs: usize,
    pub audio_outputs: usize,
    pub parameters: Vec<ParameterInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Vst3GraphConnection {
    pub from_plugin: Option<u64>,  // None = track input
    pub from_port: usize,
    pub to_plugin: Option<u64>,    // None = track output
    pub to_port: usize,
    pub connection_type: ConnectionType,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ConnectionType {
    Audio,
    Midi,
}
```

#### 2. Implement Handlers in `engine/src/engine.rs`

```rust
impl Engine {
    async fn handle_action(&mut self, action: Action) {
        match action {
            Action::TrackLoadVst3Plugin { track_name, plugin_path } => {
                let result = self.state.lock().tracks.get(&track_name)
                    .and_then(|track| {
                        track.lock().load_vst3_plugin(&plugin_path).ok()
                    });

                self.notify_clients(
                    result.map(|_| action).ok_or("Failed to load plugin".to_string())
                ).await;
            }

            Action::TrackUnloadVst3PluginInstance { track_name, instance_id } => {
                let result = self.state.lock().tracks.get(&track_name)
                    .and_then(|track| {
                        track.lock().unload_vst3_plugin_instance(instance_id).ok()
                    });

                self.notify_clients(
                    result.map(|_| action).ok_or("Failed to unload plugin".to_string())
                ).await;
            }

            Action::TrackGetVst3Graph { track_name } => {
                if let Some(track) = self.state.lock().tracks.get(&track_name) {
                    let t = track.lock();
                    let plugins = t.vst3_graph_plugins();
                    let connections = t.vst3_graph_connections();

                    self.notify_clients(Ok(Action::TrackVst3Graph {
                        track_name,
                        plugins,
                        connections,
                    })).await;
                }
            }

            Action::TrackSetVst3Parameter { track_name, instance_id, param_id, value } => {
                let result = self.state.lock().tracks.get(&track_name)
                    .and_then(|track| {
                        track.lock().set_vst3_parameter(instance_id, param_id, value).ok()
                    });

                self.notify_clients(
                    result.map(|_| action).ok_or("Failed to set parameter".to_string())
                ).await;
            }

            // ... implement other VST3 handlers ...
        }
    }
}
```

#### 3. Add Track Methods in `engine/src/track.rs`

```rust
impl Track {
    pub fn load_vst3_plugin(&mut self, plugin_path: &str) -> Result<u64, String> {
        let processor = Vst3Processor::new_with_sample_rate(
            self.sample_rate,
            self.buffer_size,
            plugin_path,
            2, // default 2 inputs
            2, // default 2 outputs
        )?;

        let id = self.next_vst3_instance_id;
        self.next_vst3_instance_id = self.next_vst3_instance_id.saturating_add(1);

        self.vst3_processors.push(Vst3Instance { id, processor });

        // Auto-wire into default audio graph
        self.rewire_vst3_default_audio_graph();

        Ok(id)
    }

    pub fn unload_vst3_plugin_instance(&mut self, instance_id: u64) -> Result<(), String> {
        let idx = self.vst3_processors
            .iter()
            .position(|i| i.id == instance_id)
            .ok_or("Instance not found")?;

        self.vst3_processors.remove(idx);
        self.rewire_vst3_default_audio_graph();

        Ok(())
    }

    pub fn set_vst3_parameter(
        &mut self,
        instance_id: u64,
        param_id: u32,
        value: f32,
    ) -> Result<(), String> {
        let instance = self.vst3_processors
            .iter_mut()
            .find(|i| i.id == instance_id)
            .ok_or("Instance not found")?;

        instance.processor.set_parameter_value(param_id, value)
    }

    pub fn vst3_graph_plugins(&self) -> Vec<Vst3GraphPlugin> {
        self.vst3_processors
            .iter()
            .map(|i| Vst3GraphPlugin {
                id: i.id,
                name: i.processor.name().to_string(),
                audio_inputs: i.processor.audio_inputs().len(),
                audio_outputs: i.processor.audio_outputs().len(),
                parameters: i.processor.parameters().to_vec(),
            })
            .collect()
    }

    pub fn vst3_graph_connections(&self) -> Vec<Vst3GraphConnection> {
        // Build connections list by inspecting AudioIO::connections
        // Similar to lv2_graph_connections() in track.rs:1332

        let mut connections = Vec::new();

        // Map plugin connections
        for (idx, instance) in self.vst3_processors.iter().enumerate() {
            // Check each audio input's connections
            for (port_idx, input) in instance.processor.audio_inputs().iter().enumerate() {
                for conn in &input.connections {
                    // Determine source
                    // ... (complex logic to trace back to source plugin or track)
                    connections.push(Vst3GraphConnection {
                        from_plugin: None,  // or Some(source_id)
                        from_port: 0,
                        to_plugin: Some(instance.id),
                        to_port: port_idx,
                        connection_type: ConnectionType::Audio,
                    });
                }
            }
        }

        connections
    }

    pub fn vst3_snapshot_states(&self) -> Vec<(u64, Vst3PluginState)> {
        self.vst3_processors
            .iter()
            .filter_map(|i| {
                i.processor.snapshot_state()
                    .ok()
                    .map(|state| (i.id, state))
            })
            .collect()
    }

    pub fn vst3_restore_states(&mut self, states: Vec<(u64, Vst3PluginState)>) -> Result<(), String> {
        for (id, state) in states {
            if let Some(instance) = self.vst3_processors.iter_mut().find(|i| i.id == id) {
                instance.processor.restore_state(&state)?;
            }
        }
        Ok(())
    }
}
```

#### 4. Update `Track::process()` for MIDI Routing

Currently (lines 227-242 in track.rs):
```rust
if !self.vst3_processors.is_empty() {
    for instance in &self.vst3_processors {
        let ready = instance.processor.audio_inputs()
            .iter().all(|audio_in| audio_in.ready());
        if ready {
            instance.processor.process_with_audio_io(frames);
        }
    }
}
```

Replace with MIDI-aware version (similar to LV2 topological sort):
```rust
if !self.vst3_processors.is_empty() {
    // Collect MIDI for each VST3 instance
    let mut vst3_node_events: HashMap<u64, Vec<MidiEvent>> = HashMap::new();

    for instance in &self.vst3_processors {
        // Check if all audio inputs ready
        let ready = instance.processor.audio_inputs()
            .iter().all(|audio_in| audio_in.ready());

        if !ready { continue; }

        // Get MIDI routed to this plugin from:
        // 1. Track MIDI input
        // 2. Previous VST3 plugin outputs
        let input_midi = self.get_vst3_plugin_input_events(
            instance.id,
            &track_input_midi_events,
            &vst3_node_events,
        );

        // Process with MIDI
        let output_midi = instance.processor.process_with_midi(frames, &input_midi);

        // Store output MIDI for routing to next plugin
        vst3_node_events.insert(instance.id, output_midi);
    }
}
```

### GUI Changes

#### 1. Add VST3 Messages to `src/gui/mod.rs`

```rust
#[derive(Clone, Debug)]
pub enum Message {
    // ... existing messages ...

    // VST3 plugin browser
    OpenVst3PluginBrowser,
    CloseVst3PluginBrowser,
    Vst3PluginSelected(String), // plugin path

    // VST3 plugin actions
    LoadVst3Plugin {
        track_name: String,
        plugin_path: String,
    },
    UnloadVst3Plugin {
        track_name: String,
        instance_id: u64,
    },

    // VST3 parameters
    Vst3ParameterChanged {
        track_name: String,
        instance_id: u64,
        param_id: u32,
        value: f32,
    },

    // VST3 editor
    OpenVst3Editor {
        track_name: String,
        instance_id: u64,
    },
    CloseVst3Editor {
        track_name: String,
        instance_id: u64,
    },
}
```

#### 2. Update `src/gui/update.rs`

```rust
impl Maolan {
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::LoadVst3Plugin { track_name, plugin_path } => {
                self.send(Action::TrackLoadVst3Plugin {
                    track_name,
                    plugin_path,
                })
            }

            Message::UnloadVst3Plugin { track_name, instance_id } => {
                self.send(Action::TrackUnloadVst3PluginInstance {
                    track_name,
                    instance_id,
                })
            }

            Message::Vst3ParameterChanged { track_name, instance_id, param_id, value } => {
                self.send(Action::TrackSetVst3Parameter {
                    track_name,
                    instance_id,
                    param_id,
                    value,
                })
            }

            Message::OpenVst3Editor { track_name, instance_id } => {
                // Request editor from engine
                // On response, create PluginEditorWidget
                self.send(Action::TrackGetVst3Editor {
                    track_name,
                    instance_id,
                })
            }

            // ... other VST3 message handlers ...
        }
    }
}
```

#### 3. Create VST3 Plugin Browser

New file: `src/vst3_plugin_browser.rs`

```rust
use iced::{widget, Element, Task};
use maolan_engine::vst3::{Vst3Host, Vst3PluginInfo};

pub struct Vst3PluginBrowser {
    host: Vst3Host,
    plugins: Vec<Vst3PluginInfo>,
    selected: Option<String>,
}

impl Vst3PluginBrowser {
    pub fn new() -> Self {
        let mut host = Vst3Host::new();
        let plugins = host.list_plugins();

        Self {
            host,
            plugins,
            selected: None,
        }
    }

    pub fn view(&self) -> Element<Message> {
        let plugin_list = widget::column(
            self.plugins.iter().map(|plugin| {
                widget::button(&plugin.name)
                    .on_press(Message::Vst3PluginSelected(plugin.path.clone()))
                    .into()
            })
        );

        widget::container(plugin_list)
            .width(400)
            .height(600)
            .into()
    }
}
```

#### 4. Add VST3 to Connections View

Modify `src/connections/canvas_host.rs`:

```rust
impl CanvasHost {
    fn draw_vst3_nodes(&self, frame: &mut Frame) {
        // Request VST3 graph from current track
        // Draw VST3 plugin nodes alongside LV2 nodes
        // Draw connections between VST3 plugins, LV2 plugins, and hardware
    }
}
```

#### 5. Session Format Updates

Modify session JSON to include VST3 state:

```json
{
  "tracks": [
    {
      "name": "Track 1",
      "vst3_plugins": [
        {
          "instance_id": 1,
          "plugin_path": "/usr/local/lib/vst3/SurgeXT.vst3",
          "state": {
            "plugin_id": "com.surge-synth-team.surge-xt",
            "component_state": [/* base64 */],
            "controller_state": [/* base64 */]
          },
          "connections": [
            {"from": "track_input", "from_port": 0, "to_port": 0},
            {"from_port": 0, "to": "track_output", "to_port": 0}
          ]
        }
      ]
    }
  ]
}
```

### Estimated Effort

**Engine Integration**: 1 week
- Message handlers: 2 days
- Track methods: 3 days
- MIDI routing in `Track::process()`: 2 days

**GUI Integration**: 1 week
- Plugin browser: 2 days
- Connections view updates: 2 days
- Parameter UI: 2 days
- Session serialization: 1 day

**Testing & Polish**: 3-5 days

**Total**: 2-3 weeks

---

## Summary

### What's Done ✅
- **Phase 1**: Complete audio processing pipeline
- **Phase 2**: MIDI API layer (simplified)
- **Phase 3**: Complete state save/restore with IBStream

### What Remains ⏳
- **Phase 4**: Plugin GUI embedding (~2 weeks, platform-specific)
- **Phase 5**: Engine/UI integration (~2-3 weeks)

### Recommendation
**Test Phases 1-3 thoroughly before proceeding:**

1. Install VST3 plugins on FreeBSD
2. Write small test programs to verify:
   - Plugin loading
   - Audio processing
   - Parameter control
   - State save/restore
3. Fix any bugs found
4. Document behavior and limitations

**Then choose:**
- **Option A**: Implement Phase 4 next (for plugin GUI support)
- **Option B**: Implement Phase 5 next (for full UI integration without GUIs)
- **Option C**: Ship Phases 1-3 and gather user feedback

Given the complexity of platform-specific GUI embedding, **Option B** (Phase 5 first) might be more practical - users can still use VST3 plugins via parameter automation without native GUIs.

---

**Date**: 2026-02-26
**Status**: 3/5 phases complete
**Next Decision**: Test current implementation, then choose Phase 4 or 5
