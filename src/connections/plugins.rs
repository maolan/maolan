#![cfg(all(unix, not(target_os = "macos")))]

use crate::{
    connections::colors::{audio_port_color, aux_port_color, midi_port_color},
    connections::port_kind::{can_connect_kinds, should_highlight_port},
    connections::ports::hover_radius,
    connections::selection::is_bezier_hit,
    consts::connections_plugins::*,
    message::Message,
    state::{MovingPlugin, PluginConnecting, State},
    ui_timing::DOUBLE_CLICK,
};
use iced::{
    Color, Point, Rectangle, Renderer, Theme,
    advanced::graphics::gradient,
    alignment::{Horizontal, Vertical},
    event::Event,
    mouse,
    widget::{
        canvas,
        canvas::{Action, Frame, Geometry, Path, Text},
    },
};
use maolan_engine::kind::Kind;
use maolan_engine::message::{Action as EngineAction, PluginGraphNode, PluginGraphPlugin};
use std::time::Instant;

pub struct Graph {
    state: State,
}

impl Graph {
    pub fn new(state: State) -> Self {
        Self { state }
    }

    fn required_height_for_ports(port_count: usize, radius: f32) -> f32 {
        if port_count == 0 {
            0.0
        } else {
            (port_count + 1) as f32 * (radius * 2.0 + MIN_PORT_GAP)
        }
    }

    fn plugin_height(plugin: &PluginGraphPlugin) -> f32 {
        MIN_PLUGIN_H
            .max(Self::required_height_for_ports(
                plugin.audio_inputs,
                AUDIO_PORT_RADIUS,
            ))
            .max(Self::required_height_for_ports(
                plugin.audio_outputs,
                AUDIO_PORT_RADIUS,
            ))
            .max(Self::required_height_for_ports(
                plugin.midi_inputs,
                MIDI_PORT_RADIUS,
            ))
            .max(Self::required_height_for_ports(
                plugin.midi_outputs,
                MIDI_PORT_RADIUS,
            ))
    }

    fn plugin_pos(
        data: &crate::state::StateData,
        plugin: &PluginGraphPlugin,
        idx: usize,
        bounds: Rectangle,
    ) -> Point {
        data.plugin_graph_plugin_positions
            .get(&plugin.instance_id)
            .copied()
            .unwrap_or_else(|| {
                let plugin_h = Self::plugin_height(plugin);
                let start_x = TRACK_IO_MARGIN_X + TRACK_IO_W + 60.0;
                let max_x = (bounds.width - TRACK_IO_MARGIN_X - TRACK_IO_W - PLUGIN_W).max(start_x);
                let x = (start_x + idx as f32 * (PLUGIN_W + 24.0)).min(max_x);
                Point::new(x, bounds.height / 2.0 - plugin_h / 2.0)
            })
    }

    fn track_input_rect(bounds: Rectangle) -> Rectangle {
        Rectangle::new(
            Point::new(TRACK_IO_MARGIN_X, bounds.height / 2.0 - TRACK_IO_H / 2.0),
            iced::Size::new(TRACK_IO_W, TRACK_IO_H),
        )
    }

    fn track_output_rect(bounds: Rectangle) -> Rectangle {
        Rectangle::new(
            Point::new(
                bounds.width - TRACK_IO_MARGIN_X - TRACK_IO_W,
                bounds.height / 2.0 - TRACK_IO_H / 2.0,
            ),
            iced::Size::new(TRACK_IO_W, TRACK_IO_H),
        )
    }

    fn trim_label_to_width(label: &str, width_px: f32) -> String {
        let max_chars = ((width_px - 12.0) / 7.2).floor() as i32;
        if max_chars <= 0 {
            return String::new();
        }
        label.chars().take(max_chars as usize).collect()
    }

    fn edge_port_y(
        y: f32,
        h: f32,
        audio_count: usize,
        midi_count: usize,
        kind: Kind,
        port: usize,
    ) -> Option<f32> {
        let total = audio_count + midi_count;
        if total == 0 {
            return None;
        }
        let slot = match kind {
            Kind::Audio => port,
            Kind::MIDI => audio_count + port,
        };
        (slot < total).then_some(y + (h / (total + 1) as f32) * (slot + 1) as f32)
    }

    fn track_input_port_y(
        track: &crate::state::Track,
        rect: Rectangle,
        kind: Kind,
        port: usize,
    ) -> Option<f32> {
        Self::edge_port_y(
            rect.y,
            rect.height,
            track.audio.ins,
            track.midi.ins,
            kind,
            port,
        )
    }

    fn track_output_port_y(
        track: &crate::state::Track,
        rect: Rectangle,
        kind: Kind,
        port: usize,
    ) -> Option<f32> {
        Self::edge_port_y(
            rect.y,
            rect.height,
            track.audio.outs,
            track.midi.outs,
            kind,
            port,
        )
    }

    fn plugin_input_port_y(
        plugin: &PluginGraphPlugin,
        plugin_h: f32,
        y: f32,
        kind: Kind,
        port: usize,
    ) -> Option<f32> {
        Self::edge_port_y(
            y,
            plugin_h,
            plugin.audio_inputs,
            plugin.midi_inputs,
            kind,
            port,
        )
    }

    fn plugin_output_port_y(
        plugin: &PluginGraphPlugin,
        plugin_h: f32,
        y: f32,
        kind: Kind,
        port: usize,
    ) -> Option<f32> {
        Self::edge_port_y(
            y,
            plugin_h,
            plugin.audio_outputs,
            plugin.midi_outputs,
            kind,
            port,
        )
    }

    fn track_input_port_color(track: &crate::state::Track, port: usize) -> Color {
        if port >= track.primary_audio_ins() {
            aux_port_color()
        } else {
            audio_port_color()
        }
    }

    fn track_output_port_color(track: &crate::state::Track, port: usize) -> Color {
        if port >= track.primary_audio_outs() {
            aux_port_color()
        } else {
            audio_port_color()
        }
    }

    fn plugin_input_port_color(plugin: &PluginGraphPlugin, port: usize) -> Color {
        if port >= plugin.main_audio_inputs {
            aux_port_color()
        } else {
            audio_port_color()
        }
    }

    fn plugin_output_port_color(plugin: &PluginGraphPlugin, port: usize) -> Color {
        if port >= plugin.main_audio_outputs {
            aux_port_color()
        } else {
            audio_port_color()
        }
    }
}

#[derive(Clone)]
struct PortHit {
    node: PluginGraphNode,
    port: usize,
    is_input: bool,
    kind: Kind,
    pos: Point,
}

impl Graph {
    fn closest_port(ports: &[PortHit], cursor: Point, hit_radius: f32) -> Option<&PortHit> {
        ports
            .iter()
            .filter_map(|p| {
                let dist = cursor.distance(p.pos);
                (dist <= hit_radius).then_some((dist, p))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, p)| p)
    }
}

impl canvas::Program<Message> for Graph {
    type State = ();

    fn update(
        &self,
        _state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Message>> {
        let cursor_position = cursor.position_in(bounds)?;

        if let Ok(mut data) = self.state.try_write() {
            data.plugin_graph_track.as_ref()?;

            let mut audio_ports: Vec<PortHit> = vec![];
            let mut midi_ports: Vec<PortHit> = vec![];
            let in_rect = Self::track_input_rect(bounds);
            let out_rect = Self::track_output_rect(bounds);
            if let Some(track) = data
                .tracks
                .iter()
                .find(|t| Some(&t.name) == data.plugin_graph_track.as_ref())
            {
                for port in 0..track.audio.ins {
                    if let Some(py) = Self::track_input_port_y(track, in_rect, Kind::Audio, port) {
                        audio_ports.push(PortHit {
                            node: PluginGraphNode::TrackInput,
                            port,
                            is_input: false,
                            kind: Kind::Audio,
                            pos: Point::new(in_rect.x + in_rect.width, py),
                        });
                    }
                }
                for port in 0..track.audio.outs {
                    if let Some(py) = Self::track_output_port_y(track, out_rect, Kind::Audio, port)
                    {
                        audio_ports.push(PortHit {
                            node: PluginGraphNode::TrackOutput,
                            port,
                            is_input: true,
                            kind: Kind::Audio,
                            pos: Point::new(out_rect.x, py),
                        });
                    }
                }
                for port in 0..track.midi.ins {
                    if let Some(py) = Self::track_input_port_y(track, in_rect, Kind::MIDI, port) {
                        midi_ports.push(PortHit {
                            node: PluginGraphNode::TrackInput,
                            port,
                            is_input: false,
                            kind: Kind::MIDI,
                            pos: Point::new(in_rect.x + in_rect.width, py),
                        });
                    }
                }
                for port in 0..track.midi.outs {
                    if let Some(py) = Self::track_output_port_y(track, out_rect, Kind::MIDI, port) {
                        midi_ports.push(PortHit {
                            node: PluginGraphNode::TrackOutput,
                            port,
                            is_input: true,
                            kind: Kind::MIDI,
                            pos: Point::new(out_rect.x, py),
                        });
                    }
                }
            }
            for (idx, plugin) in data.plugin_graph_plugins.iter().enumerate() {
                let pos = Self::plugin_pos(&data, plugin, idx, bounds);
                let plugin_h = Self::plugin_height(plugin);
                for port in 0..plugin.audio_inputs {
                    if let Some(py) =
                        Self::plugin_input_port_y(plugin, plugin_h, pos.y, Kind::Audio, port)
                    {
                        audio_ports.push(PortHit {
                            node: plugin.node.clone(),
                            port,
                            is_input: true,
                            kind: Kind::Audio,
                            pos: Point::new(pos.x, py),
                        });
                    }
                }
                for port in 0..plugin.audio_outputs {
                    if let Some(py) =
                        Self::plugin_output_port_y(plugin, plugin_h, pos.y, Kind::Audio, port)
                    {
                        audio_ports.push(PortHit {
                            node: plugin.node.clone(),
                            port,
                            is_input: false,
                            kind: Kind::Audio,
                            pos: Point::new(pos.x + PLUGIN_W, py),
                        });
                    }
                }
            }
            for (idx, plugin) in data.plugin_graph_plugins.iter().enumerate() {
                let pos = Self::plugin_pos(&data, plugin, idx, bounds);
                let plugin_h = Self::plugin_height(plugin);
                for port in 0..plugin.midi_inputs {
                    if let Some(py) =
                        Self::plugin_input_port_y(plugin, plugin_h, pos.y, Kind::MIDI, port)
                    {
                        midi_ports.push(PortHit {
                            node: plugin.node.clone(),
                            port,
                            is_input: true,
                            kind: Kind::MIDI,
                            pos: Point::new(pos.x, py),
                        });
                    }
                }
                for port in 0..plugin.midi_outputs {
                    if let Some(py) =
                        Self::plugin_output_port_y(plugin, plugin_h, pos.y, Kind::MIDI, port)
                    {
                        midi_ports.push(PortHit {
                            node: plugin.node.clone(),
                            port,
                            is_input: false,
                            kind: Kind::MIDI,
                            pos: Point::new(pos.x + PLUGIN_W, py),
                        });
                    }
                }
            }

            match event {
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                    let port_hit =
                        Self::closest_port(&audio_ports, cursor_position, PORT_HIT_RADIUS)
                            .cloned()
                            .or_else(|| {
                                Self::closest_port(&midi_ports, cursor_position, MIDI_HIT_RADIUS)
                                    .cloned()
                            });
                    if let Some(hit) = port_hit {
                        data.plugin_graph_connecting = Some(PluginConnecting {
                            from_node: hit.node,
                            from_port: hit.port,
                            kind: hit.kind,
                            point: cursor_position,
                            is_input: hit.is_input,
                        });
                        return Some(Action::capture());
                    }

                    let mut clicked_connection = None;
                    for (idx, conn) in data.plugin_graph_connections.iter().enumerate() {
                        let start =
                            match &conn.from_node {
                                PluginGraphNode::TrackInput => {
                                    let Some(track) = data.tracks.iter().find(|t| {
                                        Some(&t.name) == data.plugin_graph_track.as_ref()
                                    }) else {
                                        continue;
                                    };
                                    let Some(py) = Self::track_input_port_y(
                                        track,
                                        in_rect,
                                        conn.kind,
                                        conn.from_port,
                                    ) else {
                                        continue;
                                    };
                                    Point::new(in_rect.x + in_rect.width, py)
                                }
                                PluginGraphNode::Lv2PluginInstance(id)
                                | PluginGraphNode::Vst3PluginInstance(id)
                                | PluginGraphNode::ClapPluginInstance(id) => {
                                    let Some((pidx, plugin)) = data
                                        .plugin_graph_plugins
                                        .iter()
                                        .enumerate()
                                        .find(|(_, p)| p.instance_id == *id)
                                    else {
                                        continue;
                                    };
                                    let pos = Self::plugin_pos(&data, plugin, pidx, bounds);
                                    let plugin_h = Self::plugin_height(plugin);
                                    let Some(py) = Self::plugin_output_port_y(
                                        plugin,
                                        plugin_h,
                                        pos.y,
                                        conn.kind,
                                        conn.from_port,
                                    ) else {
                                        continue;
                                    };
                                    Point::new(pos.x + PLUGIN_W, py)
                                }
                                PluginGraphNode::TrackOutput => continue,
                            };
                        let end =
                            match &conn.to_node {
                                PluginGraphNode::TrackOutput => {
                                    let Some(track) = data.tracks.iter().find(|t| {
                                        Some(&t.name) == data.plugin_graph_track.as_ref()
                                    }) else {
                                        continue;
                                    };
                                    let Some(py) = Self::track_output_port_y(
                                        track,
                                        out_rect,
                                        conn.kind,
                                        conn.to_port,
                                    ) else {
                                        continue;
                                    };
                                    Point::new(out_rect.x, py)
                                }
                                PluginGraphNode::Lv2PluginInstance(id)
                                | PluginGraphNode::Vst3PluginInstance(id)
                                | PluginGraphNode::ClapPluginInstance(id) => {
                                    let Some((pidx, plugin)) = data
                                        .plugin_graph_plugins
                                        .iter()
                                        .enumerate()
                                        .find(|(_, p)| p.instance_id == *id)
                                    else {
                                        continue;
                                    };
                                    let pos = Self::plugin_pos(&data, plugin, pidx, bounds);
                                    let plugin_h = Self::plugin_height(plugin);
                                    let Some(py) = Self::plugin_input_port_y(
                                        plugin,
                                        plugin_h,
                                        pos.y,
                                        conn.kind,
                                        conn.to_port,
                                    ) else {
                                        continue;
                                    };
                                    Point::new(pos.x, py)
                                }
                                PluginGraphNode::TrackInput => continue,
                            };

                        if is_bezier_hit(start, end, cursor_position, 100, 12.0) {
                            clicked_connection = Some(idx);
                            break;
                        }
                    }
                    if let Some(idx) = clicked_connection {
                        let ctrl = data.ctrl;
                        crate::connections::selection::select_connection_indices(
                            &mut data.plugin_graph_selected_connections,
                            idx,
                            ctrl,
                        );
                        data.plugin_graph_selected_plugin = None;
                        return Some(Action::request_redraw());
                    }

                    let mut clicked_plugin: Option<(usize, usize, Point)> = None;
                    for (idx, plugin) in data.plugin_graph_plugins.iter().enumerate().rev() {
                        let pos = Self::plugin_pos(&data, plugin, idx, bounds);
                        let rect = Rectangle::new(
                            pos,
                            iced::Size::new(PLUGIN_W, Self::plugin_height(plugin)),
                        );
                        if rect.contains(cursor_position) {
                            clicked_plugin = Some((idx, plugin.instance_id, pos));
                            break;
                        }
                    }
                    if let Some((plugin_idx, instance_id, pos)) = clicked_plugin {
                        data.plugin_graph_selected_plugin = Some(instance_id);
                        data.plugin_graph_selected_connections.clear();
                        let now = Instant::now();
                        let is_double_click = if let Some((last_instance, last_time)) =
                            data.plugin_graph_last_plugin_click
                        {
                            last_instance == instance_id
                                && now.duration_since(last_time) <= DOUBLE_CLICK
                        } else {
                            false
                        };
                        if is_double_click {
                            data.plugin_graph_last_plugin_click = None;
                            if data.plugin_graph_track.is_some() {
                                let Some(track_name) = data.plugin_graph_track.clone() else {
                                    return Some(Action::capture());
                                };
                                let plugin = &data.plugin_graph_plugins[plugin_idx];
                                return match &plugin.node {
                                    #[cfg(all(unix, not(target_os = "macos")))]
                                    PluginGraphNode::Lv2PluginInstance(_) => {
                                        Some(Action::publish(Message::OpenLv2PluginUi {
                                            track_name,
                                            clip_idx: data
                                                .plugin_graph_clip
                                                .as_ref()
                                                .map(|target| target.clip_idx),
                                            instance_id,
                                        }))
                                    }
                                    PluginGraphNode::ClapPluginInstance(_) => {
                                        Some(Action::publish(Message::ShowClapPluginUi {
                                            track_name,
                                            clip_idx: data
                                                .plugin_graph_clip
                                                .as_ref()
                                                .map(|target| target.clip_idx),
                                            instance_id,
                                            plugin_path: plugin.uri.clone(),
                                        }))
                                    }
                                    PluginGraphNode::Vst3PluginInstance(_) => {
                                        Some(Action::publish(Message::OpenVst3PluginUi {
                                            track_name,
                                            clip_idx: data
                                                .plugin_graph_clip
                                                .as_ref()
                                                .map(|target| target.clip_idx),
                                            instance_id,
                                            plugin_path: plugin.uri.clone(),
                                            plugin_name: plugin.name.clone(),
                                            plugin_id: plugin.plugin_id.clone(),
                                            audio_inputs: plugin.audio_inputs,
                                            audio_outputs: plugin.audio_outputs,
                                        }))
                                    }
                                    PluginGraphNode::TrackInput | PluginGraphNode::TrackOutput => {
                                        Some(Action::capture())
                                    }
                                };
                            }
                            return Some(Action::capture());
                        }
                        data.plugin_graph_last_plugin_click = Some((instance_id, now));
                        data.plugin_graph_moving_plugin = Some(MovingPlugin {
                            instance_id,
                            offset_x: cursor_position.x - pos.x,
                            offset_y: cursor_position.y - pos.y,
                        });
                        return Some(Action::capture());
                    }

                    data.plugin_graph_selected_connections.clear();
                    data.plugin_graph_selected_plugin = None;
                    return Some(Action::request_redraw());
                }
                Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                    let mut redraw = true;
                    if let Some(connecting) = data.plugin_graph_connecting.as_mut() {
                        connecting.point = cursor_position;
                        redraw = true;
                    }
                    if let Some(moving) = data.plugin_graph_moving_plugin.clone() {
                        data.plugin_graph_plugin_positions.insert(
                            moving.instance_id,
                            Point::new(
                                cursor_position.x - moving.offset_x,
                                cursor_position.y - moving.offset_y,
                            ),
                        );
                        redraw = true;
                    }
                    if redraw {
                        return Some(Action::request_redraw());
                    }
                }
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                    data.plugin_graph_moving_plugin = None;
                    if let Some(connecting) = data.plugin_graph_connecting.take() {
                        let opposite_ports: Vec<PortHit> = audio_ports
                            .iter()
                            .chain(midi_ports.iter())
                            .filter(|p| {
                                p.is_input != connecting.is_input
                                    && can_connect_kinds(p.kind, connecting.kind)
                            })
                            .cloned()
                            .collect();
                        let target = Self::closest_port(
                            &opposite_ports,
                            cursor_position,
                            if connecting.kind == Kind::Audio {
                                PORT_HIT_RADIUS
                            } else {
                                MIDI_HIT_RADIUS
                            },
                        )
                        .cloned();

                        if let Some(target) = target {
                            let Some(track_name) = data.plugin_graph_track.clone() else {
                                return Some(Action::request_redraw());
                            };

                            let (from_node, from_port, to_node, to_port) = if connecting.is_input {
                                (
                                    target.node,
                                    target.port,
                                    connecting.from_node,
                                    connecting.from_port,
                                )
                            } else {
                                (
                                    connecting.from_node,
                                    connecting.from_port,
                                    target.node,
                                    target.port,
                                )
                            };
                            if from_node != to_node || from_port != to_port {
                                let action = if data.plugin_graph_clip.is_some() {
                                    return Some(Action::publish(Message::ClipConnectPlugin {
                                        from_node,
                                        from_port,
                                        to_node,
                                        to_port,
                                        kind: connecting.kind,
                                    }));
                                } else if connecting.kind == Kind::Audio {
                                    EngineAction::TrackConnectPluginAudio {
                                        track_name,
                                        from_node,
                                        from_port,
                                        to_node,
                                        to_port,
                                    }
                                } else {
                                    EngineAction::TrackConnectPluginMidi {
                                        track_name,
                                        from_node,
                                        from_port,
                                        to_node,
                                        to_port,
                                    }
                                };
                                return Some(Action::publish(Message::Request(action)));
                            }
                        }
                        return Some(Action::request_redraw());
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let cursor_position = cursor.position_in(bounds);
        let rgb8 = |r: u8, g: u8, b: u8| Color::from_rgb8(r, g, b);
        let draw_true_gradient_box =
            |frame: &mut Frame, pos: Point, size: iced::Size, base: Color| {
                let path = Path::rectangle(pos, size);
                let brighten = |c: Color, amount: f32| Color {
                    r: (c.r + amount).min(1.0),
                    g: (c.g + amount).min(1.0),
                    b: (c.b + amount).min(1.0),
                    a: c.a,
                };
                let darken = |c: Color, amount: f32| Color {
                    r: (c.r - amount).max(0.0),
                    g: (c.g - amount).max(0.0),
                    b: (c.b - amount).max(0.0),
                    a: c.a,
                };
                let grad = gradient::Linear::new(
                    Point::new(pos.x + size.width * 0.5, pos.y),
                    Point::new(pos.x + size.width * 0.5, pos.y + size.height),
                )
                .add_stop(0.0, brighten(base, 0.07))
                .add_stop(0.55, base)
                .add_stop(1.0, darken(base, 0.08));
                frame.fill(&path, grad);
            };
        let draw_grid = |frame: &mut Frame, width: f32, height: f32| {
            let minor = 24.0;
            let major_every = 4usize;
            let minor_color = Color::from_rgba(0.78, 0.86, 1.0, 0.05);
            let major_color = Color::from_rgba(0.78, 0.86, 1.0, 0.10);

            let mut i = 0usize;
            let mut x = 0.0;
            while x <= width {
                let c = if i.is_multiple_of(major_every) {
                    major_color
                } else {
                    minor_color
                };
                frame.stroke(
                    &Path::line(Point::new(x, 0.0), Point::new(x, height)),
                    canvas::Stroke::default().with_color(c).with_width(1.0),
                );
                i += 1;
                x += minor;
            }

            let mut j = 0usize;
            let mut y = 0.0;
            while y <= height {
                let c = if j.is_multiple_of(major_every) {
                    major_color
                } else {
                    minor_color
                };
                frame.stroke(
                    &Path::line(Point::new(0.0, y), Point::new(width, y)),
                    canvas::Stroke::default().with_color(c).with_width(1.0),
                );
                j += 1;
                y += minor;
            }
        };
        let bg = rgb8(23, 31, 48);
        let edge_panel = rgb8(27, 35, 54);
        let edge_panel_border = rgb8(66, 78, 108);
        let node_fill = rgb8(36, 45, 68);
        let node_border = rgb8(78, 93, 130);
        let node_selected = rgb8(123, 173, 240);
        let conn_audio = Color::from_rgb(0.36, 0.66, 0.98);
        let conn_midi = Color::from_rgb(0.30, 0.82, 0.38);
        let conn_selected = Color::from_rgb(0.72, 0.90, 1.0);
        frame.fill(&Path::rectangle(Point::new(0.0, 0.0), bounds.size()), bg);
        draw_grid(&mut frame, bounds.width, bounds.height);
        if let Ok(data) = self.state.try_read() {
            let Some(track_name) = data.plugin_graph_track.as_ref() else {
                return vec![frame.into_geometry()];
            };
            let Some(track) = data.tracks.iter().find(|t| &t.name == track_name) else {
                return vec![frame.into_geometry()];
            };
            let active_connecting = data.plugin_graph_connecting.as_ref();

            let track_header = if let Some(target) = data.plugin_graph_clip.as_ref() {
                let clip_label = track
                    .audio
                    .clips
                    .get(target.clip_idx)
                    .map(|clip| clip.name.clone())
                    .unwrap_or_else(|| format!("clip {}", target.clip_idx));
                format!("Clip: {} / {}", track.name, clip_label)
            } else {
                format!("Track: {}", track.name)
            };
            frame.fill_text(Text {
                content: Self::trim_label_to_width(&track_header, bounds.width),
                position: Point::new(bounds.width / 2.0, 16.0),
                color: Color::WHITE,
                size: 16.0.into(),
                align_x: Horizontal::Center.into(),
                align_y: Vertical::Center,
                ..Default::default()
            });

            let in_rect = Self::track_input_rect(bounds);
            let out_rect = Self::track_output_rect(bounds);
            let left_box = Path::rectangle(in_rect.position(), in_rect.size());
            frame.fill(&left_box, edge_panel);
            frame.stroke(
                &left_box,
                canvas::Stroke::default()
                    .with_color(edge_panel_border)
                    .with_width(2.0),
            );
            frame.fill_text(Text {
                content: if data.plugin_graph_clip.is_some() {
                    "Clip In".into()
                } else {
                    "Track In".into()
                },
                position: Point::new(in_rect.center_x(), in_rect.y + 18.0),
                color: Color::WHITE,
                align_x: Horizontal::Center.into(),
                align_y: Vertical::Center,
                ..Default::default()
            });

            let right_box = Path::rectangle(out_rect.position(), out_rect.size());
            frame.fill(&right_box, edge_panel);
            frame.stroke(
                &right_box,
                canvas::Stroke::default()
                    .with_color(edge_panel_border)
                    .with_width(2.0),
            );
            frame.fill_text(Text {
                content: if data.plugin_graph_clip.is_some() {
                    "Clip Out".into()
                } else {
                    "Track Out".into()
                },
                position: Point::new(out_rect.center_x(), out_rect.y + 18.0),
                color: Color::WHITE,
                align_x: Horizontal::Center.into(),
                align_y: Vertical::Center,
                ..Default::default()
            });

            for port in 0..track.audio.ins {
                let Some(py) = Self::track_input_port_y(track, in_rect, Kind::Audio, port) else {
                    continue;
                };
                let is_hovered = cursor_position.is_some_and(|cursor| {
                    cursor.distance(Point::new(in_rect.x + in_rect.width, py)) <= PORT_HIT_RADIUS
                });
                let kind_ok = should_highlight_port(
                    is_hovered,
                    active_connecting.map(|c| c.kind),
                    Kind::Audio,
                );
                let can_highlight = if let Some(connecting) = active_connecting {
                    kind_ok && connecting.is_input
                } else {
                    kind_ok
                };
                frame.fill(
                    &Path::circle(
                        Point::new(in_rect.x + in_rect.width, py),
                        hover_radius(5.0, can_highlight),
                    ),
                    Self::track_input_port_color(track, port),
                );
            }
            for port in 0..track.midi.ins {
                let Some(py) = Self::track_input_port_y(track, in_rect, Kind::MIDI, port) else {
                    continue;
                };
                let is_hovered = cursor_position.is_some_and(|cursor| {
                    cursor.distance(Point::new(in_rect.x + in_rect.width, py)) <= MIDI_HIT_RADIUS
                });
                let kind_ok = should_highlight_port(
                    is_hovered,
                    active_connecting.map(|c| c.kind),
                    Kind::MIDI,
                );
                let can_highlight = if let Some(connecting) = active_connecting {
                    kind_ok && connecting.is_input
                } else {
                    kind_ok
                };
                frame.fill(
                    &Path::circle(
                        Point::new(in_rect.x + in_rect.width, py),
                        hover_radius(4.0, can_highlight),
                    ),
                    midi_port_color(),
                );
            }
            for port in 0..track.audio.outs {
                let Some(py) = Self::track_output_port_y(track, out_rect, Kind::Audio, port) else {
                    continue;
                };
                let is_hovered = cursor_position.is_some_and(|cursor| {
                    cursor.distance(Point::new(out_rect.x, py)) <= PORT_HIT_RADIUS
                });
                let kind_ok = should_highlight_port(
                    is_hovered,
                    active_connecting.map(|c| c.kind),
                    Kind::Audio,
                );
                let can_highlight = if let Some(connecting) = active_connecting {
                    kind_ok && !connecting.is_input
                } else {
                    kind_ok
                };
                frame.fill(
                    &Path::circle(Point::new(out_rect.x, py), hover_radius(5.0, can_highlight)),
                    Self::track_output_port_color(track, port),
                );
            }
            for port in 0..track.midi.outs {
                let Some(py) = Self::track_output_port_y(track, out_rect, Kind::MIDI, port) else {
                    continue;
                };
                let is_hovered = cursor_position.is_some_and(|cursor| {
                    cursor.distance(Point::new(out_rect.x, py)) <= MIDI_HIT_RADIUS
                });
                let kind_ok = should_highlight_port(
                    is_hovered,
                    active_connecting.map(|c| c.kind),
                    Kind::MIDI,
                );
                let can_highlight = if let Some(connecting) = active_connecting {
                    kind_ok && !connecting.is_input
                } else {
                    kind_ok
                };
                frame.fill(
                    &Path::circle(Point::new(out_rect.x, py), hover_radius(4.0, can_highlight)),
                    midi_port_color(),
                );
            }

            for (idx, plugin) in data.plugin_graph_plugins.iter().enumerate() {
                let pos = Self::plugin_pos(&data, plugin, idx, bounds);
                let plugin_h = Self::plugin_height(plugin);
                let rect = Path::rectangle(pos, iced::Size::new(PLUGIN_W, plugin_h));
                draw_true_gradient_box(
                    &mut frame,
                    pos,
                    iced::Size::new(PLUGIN_W, plugin_h),
                    node_fill,
                );
                let is_selected_plugin =
                    data.plugin_graph_selected_plugin == Some(plugin.instance_id);
                frame.stroke(
                    &rect,
                    canvas::Stroke::default()
                        .with_color(if is_selected_plugin {
                            node_selected
                        } else {
                            node_border
                        })
                        .with_width(if is_selected_plugin { 3.0 } else { 2.0 }),
                );
                frame.fill_text(Text {
                    content: plugin.name.clone(),
                    position: Point::new(pos.x + PLUGIN_W / 2.0, pos.y + 16.0),
                    color: Color::WHITE,
                    size: 14.0.into(),
                    align_x: Horizontal::Center.into(),
                    align_y: Vertical::Center,
                    ..Default::default()
                });
                for port in 0..plugin.audio_inputs {
                    let Some(py) =
                        Self::plugin_input_port_y(plugin, plugin_h, pos.y, Kind::Audio, port)
                    else {
                        continue;
                    };
                    let is_hovered = cursor_position.is_some_and(|cursor| {
                        cursor.distance(Point::new(pos.x, py)) <= PORT_HIT_RADIUS
                    });
                    let kind_ok = should_highlight_port(
                        is_hovered,
                        active_connecting.map(|c| c.kind),
                        Kind::Audio,
                    );
                    let can_highlight = if let Some(connecting) = active_connecting {
                        kind_ok && !connecting.is_input
                    } else {
                        kind_ok
                    };
                    frame.fill(
                        &Path::circle(
                            Point::new(pos.x, py),
                            hover_radius(AUDIO_PORT_RADIUS, can_highlight),
                        ),
                        Self::plugin_input_port_color(plugin, port),
                    );
                }
                for port in 0..plugin.audio_outputs {
                    let Some(py) =
                        Self::plugin_output_port_y(plugin, plugin_h, pos.y, Kind::Audio, port)
                    else {
                        continue;
                    };
                    let is_hovered = cursor_position.is_some_and(|cursor| {
                        cursor.distance(Point::new(pos.x + PLUGIN_W, py)) <= PORT_HIT_RADIUS
                    });
                    let kind_ok = should_highlight_port(
                        is_hovered,
                        active_connecting.map(|c| c.kind),
                        Kind::Audio,
                    );
                    let can_highlight = if let Some(connecting) = active_connecting {
                        kind_ok && connecting.is_input
                    } else {
                        kind_ok
                    };
                    frame.fill(
                        &Path::circle(
                            Point::new(pos.x + PLUGIN_W, py),
                            hover_radius(AUDIO_PORT_RADIUS, can_highlight),
                        ),
                        Self::plugin_output_port_color(plugin, port),
                    );
                }
                for port in 0..plugin.midi_inputs {
                    let Some(py) =
                        Self::plugin_input_port_y(plugin, plugin_h, pos.y, Kind::MIDI, port)
                    else {
                        continue;
                    };
                    let is_hovered = cursor_position.is_some_and(|cursor| {
                        cursor.distance(Point::new(pos.x, py)) <= MIDI_HIT_RADIUS
                    });
                    let kind_ok = should_highlight_port(
                        is_hovered,
                        active_connecting.map(|c| c.kind),
                        Kind::MIDI,
                    );
                    let can_highlight = if let Some(connecting) = active_connecting {
                        kind_ok && !connecting.is_input
                    } else {
                        kind_ok
                    };
                    frame.fill(
                        &Path::circle(
                            Point::new(pos.x, py),
                            hover_radius(MIDI_PORT_RADIUS, can_highlight),
                        ),
                        midi_port_color(),
                    );
                }
                for port in 0..plugin.midi_outputs {
                    let Some(py) =
                        Self::plugin_output_port_y(plugin, plugin_h, pos.y, Kind::MIDI, port)
                    else {
                        continue;
                    };
                    let is_hovered = cursor_position.is_some_and(|cursor| {
                        cursor.distance(Point::new(pos.x + PLUGIN_W, py)) <= MIDI_HIT_RADIUS
                    });
                    let kind_ok = should_highlight_port(
                        is_hovered,
                        active_connecting.map(|c| c.kind),
                        Kind::MIDI,
                    );
                    let can_highlight = if let Some(connecting) = active_connecting {
                        kind_ok && connecting.is_input
                    } else {
                        kind_ok
                    };
                    frame.fill(
                        &Path::circle(
                            Point::new(pos.x + PLUGIN_W, py),
                            hover_radius(MIDI_PORT_RADIUS, can_highlight),
                        ),
                        midi_port_color(),
                    );
                }
            }

            for (conn_idx, conn) in data.plugin_graph_connections.iter().enumerate() {
                let start = match &conn.from_node {
                    PluginGraphNode::TrackInput => {
                        let Some(py) =
                            Self::track_input_port_y(track, in_rect, conn.kind, conn.from_port)
                        else {
                            continue;
                        };
                        Point::new(in_rect.x + in_rect.width, py)
                    }
                    PluginGraphNode::Lv2PluginInstance(id)
                    | PluginGraphNode::Vst3PluginInstance(id)
                    | PluginGraphNode::ClapPluginInstance(id) => {
                        let Some((idx, plugin)) = data
                            .plugin_graph_plugins
                            .iter()
                            .enumerate()
                            .find(|(_, p)| p.instance_id == *id)
                        else {
                            continue;
                        };
                        let pos = Self::plugin_pos(&data, plugin, idx, bounds);
                        let plugin_h = Self::plugin_height(plugin);
                        let Some(py) = Self::plugin_output_port_y(
                            plugin,
                            plugin_h,
                            pos.y,
                            conn.kind,
                            conn.from_port,
                        ) else {
                            continue;
                        };
                        Point::new(pos.x + PLUGIN_W, py)
                    }
                    PluginGraphNode::TrackOutput => continue,
                };
                let end = match &conn.to_node {
                    PluginGraphNode::TrackOutput => {
                        let Some(py) =
                            Self::track_output_port_y(track, out_rect, conn.kind, conn.to_port)
                        else {
                            continue;
                        };
                        Point::new(out_rect.x, py)
                    }
                    PluginGraphNode::Lv2PluginInstance(id)
                    | PluginGraphNode::Vst3PluginInstance(id)
                    | PluginGraphNode::ClapPluginInstance(id) => {
                        let Some((idx, plugin)) = data
                            .plugin_graph_plugins
                            .iter()
                            .enumerate()
                            .find(|(_, p)| p.instance_id == *id)
                        else {
                            continue;
                        };
                        let pos = Self::plugin_pos(&data, plugin, idx, bounds);
                        let plugin_h = Self::plugin_height(plugin);
                        let Some(py) = Self::plugin_input_port_y(
                            plugin,
                            plugin_h,
                            pos.y,
                            conn.kind,
                            conn.to_port,
                        ) else {
                            continue;
                        };
                        Point::new(pos.x, py)
                    }
                    PluginGraphNode::TrackInput => continue,
                };
                let dist = (end.x - start.x).abs() / 2.0;
                let is_hovered = cursor_position
                    .is_some_and(|cursor| is_bezier_hit(start, end, cursor, 100, 12.0));
                let is_selected = data.plugin_graph_selected_connections.contains(&conn_idx);
                frame.stroke(
                    &Path::new(|p| {
                        p.move_to(start);
                        p.bezier_curve_to(
                            Point::new(start.x + dist, start.y),
                            Point::new(end.x - dist, end.y),
                            end,
                        );
                    }),
                    canvas::Stroke::default()
                        .with_color(if is_selected {
                            conn_selected
                        } else if conn.kind == Kind::MIDI {
                            conn_midi
                        } else {
                            conn_audio
                        })
                        .with_width(if is_selected {
                            4.0
                        } else if is_hovered {
                            3.0
                        } else {
                            2.0
                        }),
                );
            }

            if let Some(connecting) = &data.plugin_graph_connecting {
                let start = match &connecting.from_node {
                    PluginGraphNode::TrackInput => {
                        let Some(py) = Self::track_input_port_y(
                            track,
                            in_rect,
                            connecting.kind,
                            connecting.from_port,
                        ) else {
                            return vec![frame.into_geometry()];
                        };
                        Point::new(in_rect.x + in_rect.width, py)
                    }
                    PluginGraphNode::TrackOutput => {
                        let Some(py) = Self::track_output_port_y(
                            track,
                            out_rect,
                            connecting.kind,
                            connecting.from_port,
                        ) else {
                            return vec![frame.into_geometry()];
                        };
                        Point::new(out_rect.x, py)
                    }
                    PluginGraphNode::Lv2PluginInstance(id)
                    | PluginGraphNode::Vst3PluginInstance(id)
                    | PluginGraphNode::ClapPluginInstance(id) => {
                        let Some((idx, plugin)) = data
                            .plugin_graph_plugins
                            .iter()
                            .enumerate()
                            .find(|(_, p)| p.instance_id == *id)
                        else {
                            return vec![frame.into_geometry()];
                        };
                        let pos = Self::plugin_pos(&data, plugin, idx, bounds);
                        let plugin_h = Self::plugin_height(plugin);
                        if connecting.is_input {
                            let Some(py) = Self::plugin_input_port_y(
                                plugin,
                                plugin_h,
                                pos.y,
                                connecting.kind,
                                connecting.from_port,
                            ) else {
                                return vec![frame.into_geometry()];
                            };
                            Point::new(pos.x, py)
                        } else {
                            let Some(py) = Self::plugin_output_port_y(
                                plugin,
                                plugin_h,
                                pos.y,
                                connecting.kind,
                                connecting.from_port,
                            ) else {
                                return vec![frame.into_geometry()];
                            };
                            Point::new(pos.x + PLUGIN_W, py)
                        }
                    }
                };
                let end = connecting.point;
                let dist = (end.x - start.x).abs() / 2.0;
                let (c1, c2) = if connecting.is_input {
                    (
                        Point::new(start.x - dist, start.y),
                        Point::new(end.x + dist, end.y),
                    )
                } else {
                    (
                        Point::new(start.x + dist, start.y),
                        Point::new(end.x - dist, end.y),
                    )
                };
                frame.stroke(
                    &Path::new(|p| {
                        p.move_to(start);
                        p.bezier_curve_to(c1, c2, end);
                    }),
                    canvas::Stroke::default()
                        .with_color(Color::from_rgba(0.73, 0.84, 1.0, 0.6))
                        .with_width(2.0),
                );
            }
        }
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::widget::canvas::Program;
    use iced::{Point, Rectangle, Size, event, mouse};
    use std::sync::Arc;
    use tokio::sync::RwLock;

    fn action_message(action: Action<Message>) -> (Option<Message>, event::Status) {
        let (message, _redraw, status) = action.into_inner();
        (message, status)
    }

    #[test]
    fn update_clicking_plugin_selects_and_starts_move() {
        let state = Arc::new(RwLock::new(crate::state::StateData::default()));
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(700.0, 400.0));
        let plugin = PluginGraphPlugin {
            node: PluginGraphNode::Vst3PluginInstance(7),
            instance_id: 7,
            format: "VST3".to_string(),
            uri: "/plugins/test.vst3".to_string(),
            plugin_id: "plugin-id".to_string(),
            name: "Test".to_string(),
            main_audio_inputs: 1,
            main_audio_outputs: 1,
            audio_inputs: 1,
            audio_outputs: 1,
            midi_inputs: 0,
            midi_outputs: 0,
            state: None,
        };
        {
            let mut data = state.blocking_write();
            data.tracks.push(crate::state::Track::new(
                "Track".to_string(),
                0.0,
                1,
                1,
                0,
                0,
            ));
            data.plugin_graph_track = Some("Track".to_string());
            data.plugin_graph_plugins.push(plugin.clone());
        }
        let plugin_pos = {
            let data = state.blocking_read();
            Graph::plugin_pos(&data, &plugin, 0, bounds)
        };
        let cursor = mouse::Cursor::Available(Point::new(plugin_pos.x + 5.0, plugin_pos.y + 5.0));
        let graph = Graph::new(state.clone());

        let action = graph
            .update(
                &mut (),
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                cursor,
            )
            .expect("action");

        let (message, status) = action_message(action);
        assert!(message.is_none());
        assert_eq!(status, event::Status::Captured);
        let data = state.blocking_read();
        assert_eq!(data.plugin_graph_selected_plugin, Some(7));
        assert_eq!(
            data.plugin_graph_moving_plugin
                .as_ref()
                .map(|moving| moving.instance_id),
            Some(7)
        );
    }
}
