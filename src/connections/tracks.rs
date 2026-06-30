use crate::{
    connections::colors::{audio_port_color, aux_port_color, midi_port_color},
    connections::plugins::{Graph as PluginGraph, select_plugin_indices},
    connections::port_kind::{can_connect_kinds, should_highlight_port},
    connections::ports::hover_radius,
    connections::selection::{is_bezier_hit, select_connection_indices},
    consts::connections_plugins::{
        MIDI_HIT_RADIUS, PLUGIN_W, PORT_HIT_RADIUS, TRACK_IO_MARGIN_X, TRACK_IO_W,
    },
    message::Message,
    state::{
        Connecting, ConnectionViewSelection, HW_IN_ID, HW_OUT_ID, Hovering, MIDI_HW_IN_ID,
        MIDI_HW_OUT_ID, MovingPlugin, MovingTrack, PluginConnecting, State, StateData,
    },
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
use maolan_engine::{
    kind::Kind,
    message::{Action as EngineAction, ConnectableRef, PluginGraphNode, PluginGraphPlugin},
};
use std::time::Instant;

pub struct Graph {
    state: State,
}

#[derive(Clone, Copy)]
enum TrackPortEdge {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Clone)]
struct PluginPortHit {
    node: PluginGraphNode,
    port: usize,
    is_input: bool,
    kind: Kind,
    pos: Point,
}

impl Graph {
    pub fn new(state: State) -> Self {
        Self { state }
    }

    fn get_port_kind(data: &StateData, hovering_port: &Hovering) -> Option<Kind> {
        match hovering_port {
            Hovering::Port {
                track_idx,
                port_idx,
                is_input,
            } => {
                if track_idx == HW_IN_ID || track_idx == HW_OUT_ID {
                    if let Some(folder) = Self::folder_track(data) {
                        let is_input = track_idx == HW_IN_ID;
                        Some(Self::track_port_kind(folder, *port_idx, is_input))
                    } else {
                        Some(Kind::Audio)
                    }
                } else if track_idx.starts_with(MIDI_HW_IN_ID)
                    || track_idx.starts_with(MIDI_HW_OUT_ID)
                {
                    Some(Kind::MIDI)
                } else {
                    data.tracks
                        .iter()
                        .find(|t| t.name == *track_idx)
                        .map(|t| Self::track_port_kind(t, *port_idx, *is_input))
                }
            }
            _ => None,
        }
    }

    fn track_port_kind(track: &crate::state::Track, flat_port: usize, is_input: bool) -> Kind {
        if is_input {
            let primary_audio = track.primary_audio_ins();
            if flat_port < primary_audio {
                Kind::Audio
            } else if flat_port < primary_audio + track.midi.ins {
                Kind::MIDI
            } else {
                Kind::Audio
            }
        } else {
            let primary_audio = track.primary_audio_outs();
            if flat_port < primary_audio {
                Kind::Audio
            } else if flat_port < primary_audio + track.midi.outs {
                Kind::MIDI
            } else {
                Kind::Audio
            }
        }
    }

    fn connection_port_index(
        track: &crate::state::Track,
        kind: Kind,
        port: usize,
        is_input: bool,
    ) -> usize {
        if kind == Kind::MIDI {
            port + if is_input {
                track.primary_audio_ins()
            } else {
                track.primary_audio_outs()
            }
        } else if is_input {
            if port < track.primary_audio_ins() {
                port
            } else {
                track.primary_audio_ins() + track.midi.ins + (port - track.primary_audio_ins())
            }
        } else if port < track.primary_audio_outs() {
            port
        } else {
            track.primary_audio_outs() + track.midi.outs + (port - track.primary_audio_outs())
        }
    }

    fn midi_device_label(data: &StateData, path: &str) -> String {
        data.midi_hw_labels
            .get(path)
            .cloned()
            .unwrap_or_else(|| path.rsplit('/').next().unwrap_or(path).to_string())
    }

    fn midi_box_width(label: &str) -> f32 {
        let width = label.chars().count() as f32 * 7.2 + 13.0;
        width.clamp(90.0, 360.0)
    }

    fn trim_label_to_width(label: &str, width_px: f32) -> String {
        let max_chars = ((width_px - 10.0) / 7.2).floor() as i32;
        if max_chars <= 0 {
            return String::new();
        }
        label.chars().take(max_chars as usize).collect()
    }

    const TRACK_NODE_SIZE: f32 = 140.0;

    fn track_box_size(_track: &crate::state::Track) -> iced::Size {
        iced::Size::new(Self::TRACK_NODE_SIZE, Self::TRACK_NODE_SIZE)
    }

    fn plugin_box_size(plugin: &PluginGraphPlugin) -> iced::Size {
        iced::Size::new(PLUGIN_W, PluginGraph::plugin_height(plugin))
    }

    fn plugin_node_position(
        data: &StateData,
        plugin: &PluginGraphPlugin,
        idx: usize,
        bounds: Rectangle,
    ) -> Point {
        data.plugin_graph_plugin_positions
            .get(&plugin.instance_id)
            .copied()
            .unwrap_or_else(|| {
                let plugin_h = PluginGraph::plugin_height(plugin);
                let start_x = TRACK_IO_MARGIN_X + TRACK_IO_W + 200.0;
                let max_x = (bounds.width - PLUGIN_W - 20.0).max(start_x);
                let x = (start_x + (idx % 4) as f32 * (PLUGIN_W + 24.0)).min(max_x);
                let y = 80.0 + (idx / 4) as f32 * (plugin_h + 40.0);
                Point::new(x, y)
            })
    }

    fn plugin_port_position(
        plugin: &PluginGraphPlugin,
        pos: Point,
        port_idx: usize,
        is_input: bool,
    ) -> Option<Point> {
        let size = Self::plugin_box_size(plugin);
        let py = if is_input {
            PluginGraph::plugin_input_port_y(
                plugin,
                size.height,
                pos.y,
                Self::plugin_port_kind(plugin, port_idx, true),
                port_idx,
            )
        } else {
            PluginGraph::plugin_output_port_y(
                plugin,
                size.height,
                pos.y,
                Self::plugin_port_kind(plugin, port_idx, false),
                port_idx,
            )
        };
        py.map(|y| Point::new(if is_input { pos.x } else { pos.x + size.width }, y))
    }

    fn plugin_port_kind(plugin: &PluginGraphPlugin, flat_port: usize, is_input: bool) -> Kind {
        if is_input {
            if flat_port < plugin.audio_inputs {
                Kind::Audio
            } else {
                Kind::MIDI
            }
        } else if flat_port < plugin.audio_outputs {
            Kind::Audio
        } else {
            Kind::MIDI
        }
    }

    fn plugin_port_color(plugin: &PluginGraphPlugin, port_idx: usize, is_input: bool) -> Color {
        if is_input {
            PluginGraph::plugin_input_port_color(plugin, port_idx)
        } else {
            PluginGraph::plugin_output_port_color(plugin, port_idx)
        }
    }

    fn is_hw_node(name: &str) -> bool {
        name == HW_IN_ID
            || name == HW_OUT_ID
            || name.starts_with(MIDI_HW_IN_ID)
            || name.starts_with(MIDI_HW_OUT_ID)
    }

    fn is_track_view_hw_node(name: &str) -> bool {
        name == HW_IN_ID || name == HW_OUT_ID
    }

    fn visible_track_names(data: &StateData) -> std::collections::HashSet<String> {
        match &data.connections_folder {
            Some(folder) => {
                let mut names = std::collections::HashSet::new();
                for track in &data.tracks {
                    if track.parent_track.as_deref() == Some(folder) {
                        names.insert(track.name.clone());
                    }
                }
                names
            }
            None => data
                .tracks
                .iter()
                .filter(|t| t.parent_track.is_none())
                .map(|t| t.name.clone())
                .collect(),
        }
    }

    fn folder_track(data: &StateData) -> Option<&crate::state::Track> {
        data.connections_folder
            .as_ref()
            .and_then(|name| data.tracks.iter().find(|t| t.name == *name))
    }

    fn folder_input_count(track: &crate::state::Track) -> usize {
        track.primary_audio_ins() + track.midi.ins + track.return_count()
    }

    fn folder_output_count(track: &crate::state::Track) -> usize {
        track.primary_audio_outs() + track.midi.outs + track.send_count()
    }

    fn folder_input_port_y(port_idx: usize, count: usize, bounds_height: f32) -> f32 {
        if count == 0 {
            return 0.0;
        }
        50.0 + ((bounds_height - 60.0) / (count + 1) as f32) * (port_idx + 1) as f32
    }

    fn folder_output_port_y(port_idx: usize, count: usize, bounds_height: f32) -> f32 {
        Self::folder_input_port_y(port_idx, count, bounds_height)
    }

    fn folder_input_port_position(
        track: &crate::state::Track,
        port_idx: usize,
        bounds: Rectangle,
        hw_width: f32,
    ) -> Point {
        let count = Self::folder_input_count(track);
        Point::new(
            hw_width,
            Self::folder_input_port_y(port_idx, count, bounds.height),
        )
    }

    fn folder_output_port_position(
        track: &crate::state::Track,
        port_idx: usize,
        bounds: Rectangle,
        hw_width: f32,
    ) -> Point {
        let count = Self::folder_output_count(track);
        Point::new(
            bounds.width - hw_width,
            Self::folder_output_port_y(port_idx, count, bounds.height),
        )
    }

    fn plugin_graph_node_port_position(
        data: &StateData,
        node: &PluginGraphNode,
        port: usize,
        is_input: bool,
        bounds: Rectangle,
        hw_width: f32,
    ) -> Option<Point> {
        match node {
            PluginGraphNode::TrackInput => Self::folder_track(data)
                .map(|t| Self::folder_input_port_position(t, port, bounds, hw_width)),
            PluginGraphNode::TrackOutput => Self::folder_track(data)
                .map(|t| Self::folder_output_port_position(t, port, bounds, hw_width)),
            _ => {
                let id = PluginGraph::plugin_node_instance_id(node)?;
                let plugin = data
                    .plugin_graph_plugins
                    .iter()
                    .find(|p| p.instance_id == id)?;
                let idx = data
                    .plugin_graph_plugins
                    .iter()
                    .position(|p| p.instance_id == id)?;
                let pos = Self::plugin_node_position(data, plugin, idx, bounds);
                Self::plugin_port_position(plugin, pos, port, is_input)
            }
        }
    }

    fn plugin_graph_node_edge(node: &PluginGraphNode, is_input: bool) -> TrackPortEdge {
        match node {
            PluginGraphNode::TrackInput => TrackPortEdge::Right,
            PluginGraphNode::TrackOutput => TrackPortEdge::Left,
            _ => {
                if is_input {
                    TrackPortEdge::Left
                } else {
                    TrackPortEdge::Right
                }
            }
        }
    }

    fn connectable_ref_to_plugin_node(ref_: &ConnectableRef) -> Option<PluginGraphNode> {
        match ref_ {
            ConnectableRef::TrackInput => Some(PluginGraphNode::TrackInput),
            ConnectableRef::TrackOutput => Some(PluginGraphNode::TrackOutput),
            ConnectableRef::ClapPlugin(id) => Some(PluginGraphNode::ClapPluginInstance(*id)),
            ConnectableRef::Vst3Plugin(id) => Some(PluginGraphNode::Vst3PluginInstance(*id)),
            #[cfg(all(unix, not(target_os = "macos")))]
            ConnectableRef::Lv2Plugin(id) => Some(PluginGraphNode::Lv2PluginInstance(*id)),
            ConnectableRef::ChildTrack(_) => None,
        }
    }

    fn connectable_port_position(
        data: &StateData,
        ref_: &ConnectableRef,
        port: usize,
        is_input: bool,
        kind: Kind,
        bounds: Rectangle,
        hw_width: f32,
    ) -> Option<Point> {
        if let Some(node) = Self::connectable_ref_to_plugin_node(ref_) {
            return Self::plugin_graph_node_port_position(
                data, &node, port, is_input, bounds, hw_width,
            );
        }
        let ConnectableRef::ChildTrack(name) = ref_ else {
            return None;
        };
        let track = data.tracks.iter().find(|t| &t.name == name)?;
        let size = Self::track_box_size(track);
        if is_input {
            let flat_port = Self::connection_port_index(track, kind, port, true);
            Some(Self::track_port_position(
                track,
                flat_port,
                track.position,
                size,
            ))
        } else {
            let flat_port = Self::connection_port_index(track, kind, port, false);
            Some(Self::track_output_port_position(
                track,
                flat_port,
                track.position,
                size,
            ))
        }
    }

    fn connectable_port_edge(ref_: &ConnectableRef, is_input: bool) -> TrackPortEdge {
        match ref_ {
            ConnectableRef::ChildTrack(_) => {
                if is_input {
                    TrackPortEdge::Left
                } else {
                    TrackPortEdge::Right
                }
            }
            _ => {
                if let Some(node) = Self::connectable_ref_to_plugin_node(ref_) {
                    Self::plugin_graph_node_edge(&node, is_input)
                } else {
                    TrackPortEdge::Right
                }
            }
        }
    }

    fn folder_plugin_port_hits(
        data: &StateData,
        bounds: Rectangle,
        hw_width: f32,
    ) -> Vec<PluginPortHit> {
        let mut hits = Vec::new();
        let Some(folder) = Self::folder_track(data) else {
            return hits;
        };
        let in_count = Self::folder_input_count(folder);
        for port in 0..in_count {
            hits.push(PluginPortHit {
                node: PluginGraphNode::TrackInput,
                port,
                is_input: false,
                kind: Self::track_port_kind(folder, port, true),
                pos: Self::folder_input_port_position(folder, port, bounds, hw_width),
            });
        }
        let out_count = Self::folder_output_count(folder);
        for port in 0..out_count {
            hits.push(PluginPortHit {
                node: PluginGraphNode::TrackOutput,
                port,
                is_input: true,
                kind: Self::track_port_kind(folder, port, false),
                pos: Self::folder_output_port_position(folder, port, bounds, hw_width),
            });
        }
        hits
    }

    fn plugin_port_hits(data: &StateData, bounds: Rectangle, hw_width: f32) -> Vec<PluginPortHit> {
        let mut hits = Self::folder_plugin_port_hits(data, bounds, hw_width);
        for (idx, plugin) in data.plugin_graph_plugins.iter().enumerate() {
            let pos = Self::plugin_node_position(data, plugin, idx, bounds);
            for port in 0..plugin.audio_inputs {
                if let Some(point) = Self::plugin_port_position(plugin, pos, port, true) {
                    hits.push(PluginPortHit {
                        node: plugin.node.clone(),
                        port,
                        is_input: true,
                        kind: Kind::Audio,
                        pos: point,
                    });
                }
            }
            for port in 0..plugin.midi_inputs {
                let flat = plugin.audio_inputs + port;
                if let Some(point) = Self::plugin_port_position(plugin, pos, flat, true) {
                    hits.push(PluginPortHit {
                        node: plugin.node.clone(),
                        port,
                        is_input: true,
                        kind: Kind::MIDI,
                        pos: point,
                    });
                }
            }
            for port in 0..plugin.audio_outputs {
                if let Some(point) = Self::plugin_port_position(plugin, pos, port, false) {
                    hits.push(PluginPortHit {
                        node: plugin.node.clone(),
                        port,
                        is_input: false,
                        kind: Kind::Audio,
                        pos: point,
                    });
                }
            }
            for port in 0..plugin.midi_outputs {
                let flat = plugin.audio_outputs + port;
                if let Some(point) = Self::plugin_port_position(plugin, pos, flat, false) {
                    hits.push(PluginPortHit {
                        node: plugin.node.clone(),
                        port,
                        is_input: false,
                        kind: Kind::MIDI,
                        pos: point,
                    });
                }
            }
        }
        hits
    }

    fn plugin_only_port_hits(
        data: &StateData,
        bounds: Rectangle,
        hw_width: f32,
    ) -> Vec<PluginPortHit> {
        Self::plugin_port_hits(data, bounds, hw_width)
            .into_iter()
            .filter(|hit| {
                !matches!(
                    hit.node,
                    PluginGraphNode::TrackInput | PluginGraphNode::TrackOutput
                )
            })
            .collect()
    }

    fn closest_plugin_port_hit(
        hits: &[PluginPortHit],
        cursor: Point,
        radius: f32,
    ) -> Option<PluginPortHit> {
        hits.iter()
            .filter_map(|hit| {
                let dist = cursor.distance(hit.pos);
                (dist <= radius).then_some((dist, hit.clone()))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, hit)| hit)
    }

    fn plugin_graph_connection_actions(
        data: &StateData,
        from_node: PluginGraphNode,
        from_port: usize,
        to_node: PluginGraphNode,
        to_port: usize,
        kind: Kind,
    ) -> Option<Action<Message>> {
        if from_node == to_node && from_port == to_port {
            return None;
        }
        let track_name = data
            .plugin_graph_track
            .clone()
            .or_else(|| data.connections_folder.clone())?;
        let track = data
            .plugin_graph_track
            .as_ref()
            .and_then(|name| data.tracks.iter().find(|t| &t.name == name));

        let parallel_count = if data.shift {
            let from_count = match &from_node {
                PluginGraphNode::TrackInput => track
                    .map(|t| {
                        if kind == Kind::Audio {
                            t.primary_audio_ins()
                        } else {
                            t.midi.ins
                        }
                    })
                    .unwrap_or(0),
                PluginGraphNode::TrackOutput => 0,
                node => PluginGraph::plugin_node_instance_id(node)
                    .and_then(|id| {
                        data.plugin_graph_plugins
                            .iter()
                            .find(|p| p.instance_id == id)
                    })
                    .map(|p| {
                        if kind == Kind::Audio {
                            p.main_audio_outputs
                        } else {
                            p.midi_outputs
                        }
                    })
                    .unwrap_or(0),
            };
            let to_count = match &to_node {
                PluginGraphNode::TrackOutput => track
                    .map(|t| {
                        if kind == Kind::Audio {
                            t.primary_audio_outs()
                        } else {
                            t.midi.outs
                        }
                    })
                    .unwrap_or(0),
                PluginGraphNode::TrackInput => 0,
                node => PluginGraph::plugin_node_instance_id(node)
                    .and_then(|id| {
                        data.plugin_graph_plugins
                            .iter()
                            .find(|p| p.instance_id == id)
                    })
                    .map(|p| {
                        if kind == Kind::Audio {
                            p.main_audio_inputs
                        } else {
                            p.midi_inputs
                        }
                    })
                    .unwrap_or(0),
            };
            from_count
                .saturating_sub(from_port)
                .min(to_count.saturating_sub(to_port))
                .max(1)
        } else {
            1
        };

        if data.plugin_graph_clip.is_some() {
            let mut connections = Vec::with_capacity(parallel_count);
            for offset in 0..parallel_count {
                connections.push(maolan_engine::message::PluginGraphConnection {
                    from_node: from_node.clone(),
                    from_port: from_port + offset,
                    to_node: to_node.clone(),
                    to_port: to_port + offset,
                    kind,
                });
            }
            return Some(Action::publish(Message::ClipConnectPlugins(connections)));
        }

        let mut actions = Vec::with_capacity(parallel_count);
        for offset in 0..parallel_count {
            let action = if kind == Kind::Audio {
                EngineAction::TrackConnectPluginAudio {
                    track_name: track_name.clone(),
                    from_node: from_node.clone(),
                    from_port: from_port + offset,
                    to_node: to_node.clone(),
                    to_port: to_port + offset,
                }
            } else {
                EngineAction::TrackConnectPluginMidi {
                    track_name: track_name.clone(),
                    from_node: from_node.clone(),
                    from_port: from_port + offset,
                    to_node: to_node.clone(),
                    to_port: to_port + offset,
                }
            };
            actions.push(action);
        }
        if actions.len() == 1 {
            Some(Action::publish(Message::Request(
                actions.into_iter().next().unwrap(),
            )))
        } else {
            Some(Action::publish(Message::RequestBatch(actions)))
        }
    }

    fn plugin_node_to_connectable_ref(node: &PluginGraphNode) -> Option<ConnectableRef> {
        match node {
            PluginGraphNode::TrackInput => Some(ConnectableRef::TrackInput),
            PluginGraphNode::TrackOutput => Some(ConnectableRef::TrackOutput),
            PluginGraphNode::ClapPluginInstance(id) => Some(ConnectableRef::ClapPlugin(*id)),
            PluginGraphNode::Vst3PluginInstance(id) => Some(ConnectableRef::Vst3Plugin(*id)),
            #[cfg(all(unix, not(target_os = "macos")))]
            PluginGraphNode::Lv2PluginInstance(id) => Some(ConnectableRef::Lv2Plugin(*id)),
        }
    }

    fn connectable_port_count(
        data: &StateData,
        connectable: &ConnectableRef,
        kind: Kind,
        is_output: bool,
    ) -> usize {
        match connectable {
            ConnectableRef::TrackInput | ConnectableRef::TrackOutput => {
                let folder_name = data
                    .plugin_graph_track
                    .as_deref()
                    .or(data.connections_folder.as_deref());
                if let Some(folder) =
                    folder_name.and_then(|name| data.tracks.iter().find(|t| t.name == name))
                {
                    if kind == Kind::Audio {
                        if is_output {
                            folder.primary_audio_outs()
                        } else {
                            folder.primary_audio_ins()
                        }
                    } else if is_output {
                        folder.midi.outs
                    } else {
                        folder.midi.ins
                    }
                } else {
                    0
                }
            }
            ConnectableRef::ChildTrack(name) => data
                .tracks
                .iter()
                .find(|t| t.name == *name)
                .map(|track| {
                    if kind == Kind::Audio {
                        if is_output {
                            track.primary_audio_outs()
                        } else {
                            track.primary_audio_ins()
                        }
                    } else if is_output {
                        track.midi.outs
                    } else {
                        track.midi.ins
                    }
                })
                .unwrap_or(0),
            ConnectableRef::ClapPlugin(id)
            | ConnectableRef::Vst3Plugin(id)
            | ConnectableRef::Lv2Plugin(id) => data
                .plugin_graph_plugins
                .iter()
                .find(|p| p.instance_id == *id)
                .map(|plugin| {
                    if kind == Kind::Audio {
                        if is_output {
                            plugin.main_audio_outputs
                        } else {
                            plugin.main_audio_inputs
                        }
                    } else if is_output {
                        plugin.midi_outputs
                    } else {
                        plugin.midi_inputs
                    }
                })
                .unwrap_or(0),
        }
    }

    fn connectable_connection_actions(
        data: &StateData,
        from: ConnectableRef,
        from_port: usize,
        to: ConnectableRef,
        to_port: usize,
        kind: Kind,
    ) -> Option<Action<Message>> {
        if from == to && from_port == to_port {
            return None;
        }
        let track_name = data
            .plugin_graph_track
            .clone()
            .or_else(|| data.connections_folder.clone())?;

        let parallel_count = if data.shift {
            let from_count = Self::connectable_port_count(data, &from, kind, true);
            let to_count = Self::connectable_port_count(data, &to, kind, false);
            from_count
                .saturating_sub(from_port)
                .min(to_count.saturating_sub(to_port))
                .max(1)
        } else {
            1
        };

        let mut actions = Vec::with_capacity(parallel_count);
        for offset in 0..parallel_count {
            let action = if kind == Kind::Audio {
                EngineAction::TrackConnectAudio {
                    track_name: track_name.clone(),
                    from: from.clone(),
                    from_port: from_port + offset,
                    to: to.clone(),
                    to_port: to_port + offset,
                }
            } else {
                EngineAction::TrackConnectMidi {
                    track_name: track_name.clone(),
                    from: from.clone(),
                    from_port: from_port + offset,
                    to: to.clone(),
                    to_port: to_port + offset,
                }
            };
            actions.push(action);
        }

        if actions.len() == 1 {
            Some(Action::publish(Message::Request(
                actions.into_iter().next().unwrap(),
            )))
        } else {
            Some(Action::publish(Message::RequestBatch(actions)))
        }
    }

    fn draw_folder_side_panel(
        frame: &mut canvas::Frame,
        data: &StateData,
        bounds: Rectangle,
        hw_width: f32,
        is_input: bool,
        edge_panel: Color,
        edge_panel_border: Color,
    ) {
        let Some(track) = Self::folder_track(data) else {
            return;
        };
        let hovering = &data.hovering;
        let connecting_kind = data.connecting.as_ref().map(|c| c.kind);

        let count = if is_input {
            Self::folder_input_count(track)
        } else {
            Self::folder_output_count(track)
        };
        if count == 0 {
            return;
        }

        let pos = if is_input {
            Point::new(0.0, 0.0)
        } else {
            Point::new(bounds.width - hw_width, 0.0)
        };
        let rect = Path::rectangle(pos, iced::Size::new(hw_width, bounds.height));
        frame.fill(&rect, edge_panel);
        frame.stroke(
            &rect,
            canvas::Stroke::default()
                .with_color(edge_panel_border)
                .with_width(2.0),
        );

        frame.fill_text(canvas::Text {
            content: if is_input { "in".into() } else { "out".into() },
            position: Point::new(pos.x + hw_width / 2.0, pos.y + 20.0),
            color: Color::WHITE,
            align_x: Horizontal::Center.into(),
            ..Default::default()
        });

        for j in 0..count {
            let py = Self::folder_input_port_y(j, count, bounds.height);
            let port_pos = Point::new(if is_input { pos.x + hw_width } else { pos.x }, py);
            let track_idx = if is_input { HW_IN_ID } else { HW_OUT_ID };
            let h_port = Hovering::Port {
                track_idx: track_idx.to_string(),
                port_idx: j,
                is_input: !is_input,
            };
            let h = hovering.as_ref() == Some(&h_port);
            let kind = Self::track_port_kind(track, j, is_input);
            let can_highlight = should_highlight_port(h, connecting_kind, kind);

            frame.fill(
                &Path::circle(port_pos, hover_radius(5.0, can_highlight)),
                Self::track_port_color(track, j, is_input),
            );
        }
    }

    fn track_port_to_engine_index(
        track: &crate::state::Track,
        flat_port: usize,
        is_input: bool,
    ) -> (Kind, usize) {
        let kind = Self::track_port_kind(track, flat_port, is_input);
        let engine_port = if kind == Kind::MIDI {
            flat_port
                - if is_input {
                    track.primary_audio_ins()
                } else {
                    track.primary_audio_outs()
                }
        } else if is_input {
            if flat_port < track.primary_audio_ins() {
                flat_port
            } else {
                track.primary_audio_ins() + (flat_port - track.primary_audio_ins() - track.midi.ins)
            }
        } else if flat_port < track.primary_audio_outs() {
            flat_port
        } else {
            track.primary_audio_outs() + (flat_port - track.primary_audio_outs() - track.midi.outs)
        };
        (kind, engine_port)
    }

    fn track_port_edge(
        track: &crate::state::Track,
        flat_port: usize,
        is_input: bool,
    ) -> TrackPortEdge {
        let (kind, engine_port) = Self::track_port_to_engine_index(track, flat_port, is_input);
        match (is_input, kind) {
            (true, Kind::Audio) if engine_port >= track.primary_audio_ins() => {
                TrackPortEdge::Bottom
            }
            (false, Kind::Audio) if engine_port >= track.primary_audio_outs() => TrackPortEdge::Top,
            (true, _) => TrackPortEdge::Left,
            (false, _) => TrackPortEdge::Right,
        }
    }

    fn track_port_position(
        track: &crate::state::Track,
        flat_port: usize,
        pos: Point,
        size: iced::Size,
    ) -> Point {
        let edge = Self::track_port_edge(track, flat_port, true);
        let (kind, engine_port) = Self::track_port_to_engine_index(track, flat_port, true);
        match edge {
            TrackPortEdge::Bottom => {
                let returns = track.return_count().max(1);
                let slot = engine_port.saturating_sub(track.primary_audio_ins());
                let px = pos.x + (size.width / (returns + 1) as f32) * (slot + 1) as f32;
                Point::new(px, pos.y + size.height)
            }
            _ => {
                let count = track.primary_audio_ins() + track.midi.ins;
                let slot = if kind == Kind::MIDI {
                    track.primary_audio_ins() + engine_port
                } else {
                    engine_port
                };
                let py = pos.y + (size.height / (count.max(1) + 1) as f32) * (slot + 1) as f32;
                Point::new(pos.x, py)
            }
        }
    }

    fn track_output_port_position(
        track: &crate::state::Track,
        flat_port: usize,
        pos: Point,
        size: iced::Size,
    ) -> Point {
        let edge = Self::track_port_edge(track, flat_port, false);
        let (kind, engine_port) = Self::track_port_to_engine_index(track, flat_port, false);
        match edge {
            TrackPortEdge::Top => {
                let sends = track.send_count().max(1);
                let slot = engine_port.saturating_sub(track.primary_audio_outs());
                let px = pos.x + (size.width / (sends + 1) as f32) * (slot + 1) as f32;
                Point::new(px, pos.y)
            }
            _ => {
                let count = track.primary_audio_outs() + track.midi.outs;
                let slot = if kind == Kind::MIDI {
                    track.primary_audio_outs() + engine_port
                } else {
                    engine_port
                };
                let py = pos.y + (size.height / (count.max(1) + 1) as f32) * (slot + 1) as f32;
                Point::new(pos.x + size.width, py)
            }
        }
    }

    fn port_edge_vector(edge: TrackPortEdge) -> (f32, f32) {
        match edge {
            TrackPortEdge::Left => (-1.0, 0.0),
            TrackPortEdge::Right => (1.0, 0.0),
            TrackPortEdge::Top => (0.0, -1.0),
            TrackPortEdge::Bottom => (0.0, 1.0),
        }
    }

    fn bezier_controls(
        start: Point,
        start_edge: TrackPortEdge,
        end: Point,
        end_edge: TrackPortEdge,
    ) -> (Point, Point) {
        let dist = ((end.x - start.x).abs().max((end.y - start.y).abs()) * 0.5).max(28.0);
        let (sx, sy) = Self::port_edge_vector(start_edge);
        let (ex, ey) = Self::port_edge_vector(end_edge);
        (
            Point::new(start.x + sx * dist, start.y + sy * dist),
            Point::new(end.x + ex * dist, end.y + ey * dist),
        )
    }

    fn track_port_color(track: &crate::state::Track, flat_port: usize, is_input: bool) -> Color {
        match Self::track_port_edge(track, flat_port, is_input) {
            TrackPortEdge::Top | TrackPortEdge::Bottom => aux_port_color(),
            TrackPortEdge::Left | TrackPortEdge::Right => {
                match Self::track_port_kind(track, flat_port, is_input) {
                    Kind::Audio => audio_port_color(),
                    Kind::MIDI => midi_port_color(),
                }
            }
        }
    }

    fn default_midi_in_rect(index: usize, label: &str, box_h: f32, gap: f32) -> Rectangle {
        let box_w = Self::midi_box_width(label);
        Rectangle::new(
            Point::new(80.0, 10.0 + index as f32 * (box_h + gap)),
            iced::Size::new(box_w, box_h),
        )
    }

    fn default_midi_out_rect(
        index: usize,
        label: &str,
        bounds: Rectangle,
        hw_width: f32,
        box_h: f32,
        gap: f32,
    ) -> Rectangle {
        let box_w = Self::midi_box_width(label);
        Rectangle::new(
            Point::new(
                bounds.width - hw_width - 10.0 - box_w,
                10.0 + index as f32 * (box_h + gap),
            ),
            iced::Size::new(box_w, box_h),
        )
    }

    fn midi_hw_in_port_pos(
        data: &StateData,
        device: &str,
        index: usize,
        box_h: f32,
        gap: f32,
    ) -> Point {
        let label = Self::midi_device_label(data, device);
        let default_rect = Self::default_midi_in_rect(index, &label, box_h, gap);
        let pos = data
            .midi_hw_in_positions
            .get(device)
            .copied()
            .unwrap_or(Point::new(default_rect.x, default_rect.y));
        Point::new(
            pos.x + default_rect.width,
            pos.y + default_rect.height / 2.0,
        )
    }

    fn midi_hw_out_port_pos(
        data: &StateData,
        device: &str,
        index: usize,
        bounds: Rectangle,
        hw_width: f32,
        box_h: f32,
        gap: f32,
    ) -> Point {
        let label = Self::midi_device_label(data, device);
        let default_rect = Self::default_midi_out_rect(index, &label, bounds, hw_width, box_h, gap);
        let pos = data
            .midi_hw_out_positions
            .get(device)
            .copied()
            .unwrap_or(Point::new(default_rect.x, default_rect.y));
        Point::new(pos.x, pos.y + default_rect.height / 2.0)
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
        let hw_width = 70.0;
        let midi_hw_box_h = 24.0;
        let midi_hw_box_gap = 6.0;

        if let Ok(mut data) = self.state.try_write() {
            match event {
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                    let ctrl = data.ctrl;
                    let mut pending_action: Option<Action<Message>> = None;

                    let folder = Self::folder_track(&data);
                    let folder_view = folder.is_some();
                    if let Some(folder) = folder {
                        let in_count = Self::folder_input_count(folder);
                        for j in 0..in_count {
                            let pos = Self::folder_input_port_position(folder, j, bounds, hw_width);
                            if cursor_position.distance(pos) < 10.0 {
                                data.connecting = Some(Connecting {
                                    from_track: HW_IN_ID.to_string(),
                                    from_port: j,
                                    kind: Self::track_port_kind(folder, j, true),
                                    point: cursor_position,
                                    is_input: false,
                                });
                                return Some(Action::capture());
                            }
                        }

                        let out_count = Self::folder_output_count(folder);
                        for j in 0..out_count {
                            let pos =
                                Self::folder_output_port_position(folder, j, bounds, hw_width);
                            if cursor_position.distance(pos) < 10.0 {
                                data.connecting = Some(Connecting {
                                    from_track: HW_OUT_ID.to_string(),
                                    from_port: j,
                                    kind: Self::track_port_kind(folder, j, false),
                                    point: cursor_position,
                                    is_input: true,
                                });
                                return Some(Action::capture());
                            }
                        }
                    }

                    if !folder_view && let Some(hw_in) = &data.hw_in {
                        let pos = Point::new(0.0, 0.0);
                        for j in 0..hw_in.channels {
                            let py = pos.y
                                + 50.0
                                + ((bounds.height - 60.0) / (hw_in.channels + 1) as f32)
                                    * (j + 1) as f32;
                            if cursor_position.distance(Point::new(pos.x + hw_width, py)) < 10.0 {
                                data.connecting = Some(Connecting {
                                    from_track: HW_IN_ID.to_string(),
                                    from_port: j,
                                    kind: Kind::Audio,
                                    point: cursor_position,
                                    is_input: false,
                                });
                                return Some(Action::capture());
                            }
                        }
                        if Rectangle::new(pos, iced::Size::new(hw_width, bounds.height))
                            .contains(cursor_position)
                        {
                            let now = Instant::now();
                            if let Some((last_track, last_time)) =
                                &data.connections_last_track_click
                                && *last_track == HW_IN_ID
                                && now.duration_since(*last_time) <= DOUBLE_CLICK
                            {
                                data.connections_last_track_click = None;
                                return Some(Action::publish(Message::OpenHwPorts { input: true }));
                            }
                            data.connections_last_track_click = Some((HW_IN_ID.to_string(), now));
                            return Some(Action::capture());
                        }
                    }

                    if !folder_view && let Some(hw_out) = &data.hw_out {
                        let pos = Point::new(bounds.width - hw_width, 0.0);
                        for j in 0..hw_out.channels {
                            let py = pos.y
                                + 50.0
                                + ((bounds.height - 60.0) / (hw_out.channels + 1) as f32)
                                    * (j + 1) as f32;
                            if cursor_position.distance(Point::new(pos.x, py)) < 10.0 {
                                data.connecting = Some(Connecting {
                                    from_track: HW_OUT_ID.to_string(),
                                    from_port: j,
                                    kind: Kind::Audio,
                                    point: cursor_position,
                                    is_input: true,
                                });
                                return Some(Action::capture());
                            }
                        }
                        if Rectangle::new(pos, iced::Size::new(hw_width, bounds.height))
                            .contains(cursor_position)
                        {
                            let now = Instant::now();
                            if let Some((last_track, last_time)) =
                                &data.connections_last_track_click
                                && *last_track == HW_OUT_ID
                                && now.duration_since(*last_time) <= DOUBLE_CLICK
                            {
                                data.connections_last_track_click = None;
                                return Some(Action::publish(Message::OpenHwPorts {
                                    input: false,
                                }));
                            }
                            data.connections_last_track_click = Some((HW_OUT_ID.to_string(), now));
                            return Some(Action::capture());
                        }
                    }

                    if !folder_view {
                        for (idx, device) in data.opened_midi_in_hw.iter().enumerate() {
                            let label = Self::midi_device_label(&data, device);
                            let default_rect = Self::default_midi_in_rect(
                                idx,
                                &label,
                                midi_hw_box_h,
                                midi_hw_box_gap,
                            );
                            let port_pos = Self::midi_hw_in_port_pos(
                                &data,
                                device,
                                idx,
                                midi_hw_box_h,
                                midi_hw_box_gap,
                            );
                            if cursor_position.distance(port_pos) < 10.0 {
                                data.connecting = Some(Connecting {
                                    from_track: format!("{MIDI_HW_IN_ID}:{device}"),
                                    from_port: 0,
                                    kind: Kind::MIDI,
                                    point: cursor_position,
                                    is_input: false,
                                });
                                return Some(Action::capture());
                            }
                            let pos = data
                                .midi_hw_in_positions
                                .get(device)
                                .copied()
                                .unwrap_or(Point::new(default_rect.x, default_rect.y));
                            let rect = Rectangle::new(
                                pos,
                                iced::Size::new(default_rect.width, default_rect.height),
                            );
                            if rect.contains(cursor_position) {
                                data.moving_track = Some(MovingTrack {
                                    track_idx: format!("{MIDI_HW_IN_ID}:{device}"),
                                    offset_x: cursor_position.x - pos.x,
                                    offset_y: cursor_position.y - pos.y,
                                });
                                return Some(Action::capture());
                            }
                        }

                        for (idx, device) in data.opened_midi_out_hw.iter().enumerate() {
                            let label = Self::midi_device_label(&data, device);
                            let default_rect = Self::default_midi_out_rect(
                                idx,
                                &label,
                                bounds,
                                hw_width,
                                midi_hw_box_h,
                                midi_hw_box_gap,
                            );
                            let port_pos = Self::midi_hw_out_port_pos(
                                &data,
                                device,
                                idx,
                                bounds,
                                hw_width,
                                midi_hw_box_h,
                                midi_hw_box_gap,
                            );
                            if cursor_position.distance(port_pos) < 10.0 {
                                data.connecting = Some(Connecting {
                                    from_track: format!("{MIDI_HW_OUT_ID}:{device}"),
                                    from_port: 0,
                                    kind: Kind::MIDI,
                                    point: cursor_position,
                                    is_input: true,
                                });
                                return Some(Action::capture());
                            }
                            let pos = data
                                .midi_hw_out_positions
                                .get(device)
                                .copied()
                                .unwrap_or(Point::new(default_rect.x, default_rect.y));
                            let rect = Rectangle::new(
                                pos,
                                iced::Size::new(default_rect.width, default_rect.height),
                            );
                            if rect.contains(cursor_position) {
                                data.moving_track = Some(MovingTrack {
                                    track_idx: format!("{MIDI_HW_OUT_ID}:{device}"),
                                    offset_x: cursor_position.x - pos.x,
                                    offset_y: cursor_position.y - pos.y,
                                });
                                return Some(Action::capture());
                            }
                        }
                    }

                    let visible_names = Self::visible_track_names(&data);
                    for track in data.tracks.iter().rev() {
                        if !visible_names.contains(&track.name) {
                            continue;
                        }
                        let track_name = track.name.clone();
                        let track_pos = track.position;
                        let track_size = Self::track_box_size(track);
                        let t_ins =
                            track.primary_audio_ins() + track.midi.ins + track.return_count();
                        for j in 0..t_ins {
                            let port_pos =
                                Self::track_port_position(track, j, track_pos, track_size);
                            if cursor_position.distance(port_pos) < 10.0 {
                                data.connecting = Some(Connecting {
                                    from_track: track_name.clone(),
                                    from_port: j,
                                    kind: Self::track_port_kind(track, j, true),
                                    point: cursor_position,
                                    is_input: true,
                                });
                                return Some(Action::capture());
                            }
                        }

                        let t_outs =
                            track.primary_audio_outs() + track.midi.outs + track.send_count();
                        for j in 0..t_outs {
                            let port_pos =
                                Self::track_output_port_position(track, j, track_pos, track_size);
                            if cursor_position.distance(port_pos) < 10.0 {
                                data.connecting = Some(Connecting {
                                    from_track: track_name.clone(),
                                    from_port: j,
                                    kind: Self::track_port_kind(track, j, false),
                                    point: cursor_position,
                                    is_input: false,
                                });
                                return Some(Action::capture());
                            }
                        }

                        if Rectangle::new(track_pos, track_size).contains(cursor_position) {
                            let now = Instant::now();
                            if let Some((last_track, last_time)) =
                                &data.connections_last_track_click
                                && *last_track == track_name
                                && now.duration_since(*last_time) <= DOUBLE_CLICK
                            {
                                let is_folder = track.is_folder;
                                data.connections_last_track_click = None;
                                return Some(Action::publish(if is_folder {
                                    Message::OpenFolderConnections(track_name.clone())
                                } else {
                                    Message::OpenTrackPlugins(track_name.clone())
                                }));
                            }
                            data.connections_last_track_click = Some((track_name.clone(), now));

                            if ctrl {
                                pending_action = Some(Action::publish(
                                    Message::ConnectionViewSelectTrack(track_name.clone()),
                                ));
                            } else {
                                data.moving_track = Some(MovingTrack {
                                    track_idx: track_name.clone(),
                                    offset_x: cursor_position.x - track_pos.x,
                                    offset_y: cursor_position.y - track_pos.y,
                                });
                                let mut set = std::collections::HashSet::new();
                                set.insert(track_name.clone());
                                data.connection_view_selection =
                                    ConnectionViewSelection::Tracks(set);
                                data.selected.clear();
                                data.selected.insert(track_name.clone());
                                pending_action = Some(Action::capture());
                            }
                            break;
                        }
                    }

                    if folder_view {
                        for (idx, plugin) in data.plugin_graph_plugins.iter().enumerate().rev() {
                            let pos = Self::plugin_node_position(&data, plugin, idx, bounds);
                            let total_ins = plugin.audio_inputs + plugin.midi_inputs;
                            for j in 0..total_ins {
                                let Some(point) = Self::plugin_port_position(plugin, pos, j, true)
                                else {
                                    continue;
                                };
                                let radius =
                                    if Self::plugin_port_kind(plugin, j, true) == Kind::Audio {
                                        PORT_HIT_RADIUS
                                    } else {
                                        MIDI_HIT_RADIUS
                                    };
                                if cursor_position.distance(point) <= radius {
                                    data.plugin_graph_connecting = Some(PluginConnecting {
                                        from_node: plugin.node.clone(),
                                        from_port: j,
                                        kind: Self::plugin_port_kind(plugin, j, true),
                                        point: cursor_position,
                                        is_input: true,
                                    });
                                    return Some(Action::capture());
                                }
                            }
                            let total_outs = plugin.audio_outputs + plugin.midi_outputs;
                            for j in 0..total_outs {
                                let Some(point) = Self::plugin_port_position(plugin, pos, j, false)
                                else {
                                    continue;
                                };
                                let radius =
                                    if Self::plugin_port_kind(plugin, j, false) == Kind::Audio {
                                        PORT_HIT_RADIUS
                                    } else {
                                        MIDI_HIT_RADIUS
                                    };
                                if cursor_position.distance(point) <= radius {
                                    data.plugin_graph_connecting = Some(PluginConnecting {
                                        from_node: plugin.node.clone(),
                                        from_port: j,
                                        kind: Self::plugin_port_kind(plugin, j, false),
                                        point: cursor_position,
                                        is_input: false,
                                    });
                                    return Some(Action::capture());
                                }
                            }
                        }

                        for (idx, plugin) in data.plugin_graph_plugins.iter().enumerate().rev() {
                            let pos = Self::plugin_node_position(&data, plugin, idx, bounds);
                            let size = Self::plugin_box_size(plugin);
                            if Rectangle::new(pos, size).contains(cursor_position) {
                                let instance_id = plugin.instance_id;
                                let node = plugin.node.clone();
                                let uri = plugin.uri.clone();
                                select_plugin_indices(
                                    &mut data.plugin_graph_selected_plugins,
                                    instance_id,
                                    ctrl,
                                );
                                data.plugin_graph_selected_connections.clear();
                                data.connection_view_selection = ConnectionViewSelection::None;
                                let now = Instant::now();
                                let is_double_click = data
                                    .plugin_graph_last_plugin_click
                                    .as_ref()
                                    .is_some_and(|(last_id, last_time)| {
                                        *last_id == instance_id
                                            && now.duration_since(*last_time) <= DOUBLE_CLICK
                                    });
                                if is_double_click {
                                    data.plugin_graph_last_plugin_click = None;
                                    let track_name = data
                                        .plugin_graph_track
                                        .clone()
                                        .or_else(|| data.connections_folder.clone())
                                        .unwrap_or_default();
                                    let clip_idx = data
                                        .plugin_graph_clip
                                        .as_ref()
                                        .map(|target| target.clip_idx);
                                    return match &node {
                                        #[cfg(all(unix, not(target_os = "macos")))]
                                        PluginGraphNode::Lv2PluginInstance(_) => {
                                            Some(Action::publish(Message::OpenLv2PluginUi {
                                                track_name,
                                                clip_idx,
                                                instance_id,
                                            }))
                                        }
                                        PluginGraphNode::ClapPluginInstance(_) => {
                                            Some(Action::publish(Message::ShowClapPluginUi {
                                                track_name,
                                                clip_idx,
                                                instance_id,
                                                plugin_path: uri.clone(),
                                            }))
                                        }
                                        PluginGraphNode::Vst3PluginInstance(_) => {
                                            Some(Action::publish(Message::OpenVst3PluginUi {
                                                track_name,
                                                clip_idx,
                                                instance_id,
                                                plugin_path: uri.clone(),
                                            }))
                                        }
                                        PluginGraphNode::TrackInput
                                        | PluginGraphNode::TrackOutput => Some(Action::capture()),
                                    };
                                }
                                data.plugin_graph_last_plugin_click = Some((instance_id, now));
                                data.plugin_graph_moving_plugin = Some(MovingPlugin {
                                    instance_id,
                                    offset_x: cursor_position.x - pos.x,
                                    offset_y: cursor_position.y - pos.y,
                                });
                                return Some(Action::capture());
                            }
                        }

                        let mut clicked_plugin_connection = None;
                        for (idx, conn) in data.plugin_graph_connections.iter().enumerate() {
                            let start = Self::plugin_graph_node_port_position(
                                &data,
                                &conn.from_node,
                                conn.from_port,
                                false,
                                bounds,
                                hw_width,
                            );
                            let end = Self::plugin_graph_node_port_position(
                                &data,
                                &conn.to_node,
                                conn.to_port,
                                true,
                                bounds,
                                hw_width,
                            );
                            if let (Some(start), Some(end)) = (start, end)
                                && is_bezier_hit(start, end, cursor_position, 100, 12.0)
                            {
                                clicked_plugin_connection = Some(idx);
                                break;
                            }
                        }
                        if let Some(idx) = clicked_plugin_connection {
                            select_connection_indices(
                                &mut data.plugin_graph_selected_connections,
                                idx,
                                ctrl,
                            );
                            data.plugin_graph_selected_plugins.clear();
                            data.plugin_graph_selected_connectable_connections.clear();
                            return Some(Action::request_redraw());
                        }

                        let mut clicked_connectable_connection = None;
                        for (idx, conn) in data.connectable_connections.iter().enumerate() {
                            let involves_child = matches!(conn.from, ConnectableRef::ChildTrack(_))
                                || matches!(conn.to, ConnectableRef::ChildTrack(_));
                            // Folder/track inputs cannot be used as audio/MIDI sources, so the
                            // connection from folder input to child input is not selectable.
                            let from_is_track_input =
                                matches!(conn.from, ConnectableRef::TrackInput);
                            if !involves_child || from_is_track_input {
                                continue;
                            }
                            let start = Self::connectable_port_position(
                                &data,
                                &conn.from,
                                conn.from_port,
                                false,
                                conn.kind,
                                bounds,
                                hw_width,
                            );
                            let end = Self::connectable_port_position(
                                &data,
                                &conn.to,
                                conn.to_port,
                                true,
                                conn.kind,
                                bounds,
                                hw_width,
                            );
                            if let (Some(start), Some(end)) = (start, end)
                                && is_bezier_hit(start, end, cursor_position, 100, 12.0)
                            {
                                clicked_connectable_connection = Some(idx);
                                break;
                            }
                        }
                        if let Some(idx) = clicked_connectable_connection {
                            select_connection_indices(
                                &mut data.plugin_graph_selected_connectable_connections,
                                idx,
                                ctrl,
                            );
                            data.plugin_graph_selected_connections.clear();
                            data.plugin_graph_selected_plugins.clear();
                            data.connection_view_selection = ConnectionViewSelection::None;
                            return Some(Action::request_redraw());
                        }
                    }

                    let mut clicked_connection = None;
                    let folder_name_opt = data.connections_folder.as_ref();
                    for (idx, conn) in data.connections.iter().enumerate() {
                        // In folder view, hide track-level connections that involve the folder
                        // track itself; those are managed in the root connections view.
                        let from_visible = if data.connections_folder.is_some() {
                            Self::is_track_view_hw_node(&conn.from_track)
                                || visible_names.contains(&conn.from_track)
                        } else {
                            Self::is_hw_node(&conn.from_track)
                                || visible_names.contains(&conn.from_track)
                        };
                        let to_visible = if data.connections_folder.is_some() {
                            Self::is_track_view_hw_node(&conn.to_track)
                                || visible_names.contains(&conn.to_track)
                        } else {
                            Self::is_hw_node(&conn.to_track)
                                || visible_names.contains(&conn.to_track)
                        };
                        if !from_visible || !to_visible {
                            continue;
                        }
                        let start_track_option =
                            data.tracks.iter().find(|t| t.name == conn.from_track);
                        let end_track_option = data.tracks.iter().find(|t| t.name == conn.to_track);

                        let start_is_folder = folder_name_opt == Some(&conn.from_track);
                        let end_is_folder = folder_name_opt == Some(&conn.to_track);

                        let start_point = if conn.from_track == HW_IN_ID
                            || (data.connections_folder.is_some() && start_is_folder)
                        {
                            if let Some(folder) = Self::folder_track(&data) {
                                Some(Self::folder_input_port_position(
                                    folder,
                                    conn.from_port,
                                    bounds,
                                    hw_width,
                                ))
                            } else {
                                data.hw_in.as_ref().map(move |hw_in| {
                                    let py = 50.0
                                        + ((bounds.height - 60.0) / (hw_in.channels + 1) as f32)
                                            * (conn.from_port + 1) as f32;
                                    Point::new(hw_width, py)
                                })
                            }
                        } else if let Some(device) =
                            conn.from_track.strip_prefix(&format!("{MIDI_HW_IN_ID}:"))
                        {
                            data.opened_midi_in_hw
                                .iter()
                                .position(|d| d == device)
                                .map(|idx| {
                                    Self::midi_hw_in_port_pos(
                                        &data,
                                        device,
                                        idx,
                                        midi_hw_box_h,
                                        midi_hw_box_gap,
                                    )
                                })
                        } else {
                            start_track_option.map(|t| {
                                let track_size = Self::track_box_size(t);
                                let port_idx = Self::connection_port_index(
                                    t,
                                    conn.kind,
                                    conn.from_port,
                                    false,
                                );
                                Self::track_output_port_position(
                                    t, port_idx, t.position, track_size,
                                )
                            })
                        };

                        let end_point = if conn.to_track == HW_OUT_ID
                            || (data.connections_folder.is_some() && end_is_folder)
                        {
                            if let Some(folder) = Self::folder_track(&data) {
                                Some(Self::folder_output_port_position(
                                    folder,
                                    conn.to_port,
                                    bounds,
                                    hw_width,
                                ))
                            } else {
                                data.hw_out.as_ref().map(move |hw_out| {
                                    let py = 50.0
                                        + ((bounds.height - 60.0) / (hw_out.channels + 1) as f32)
                                            * (conn.to_port + 1) as f32;
                                    Point::new(bounds.width - hw_width, py)
                                })
                            }
                        } else if let Some(device) =
                            conn.to_track.strip_prefix(&format!("{MIDI_HW_OUT_ID}:"))
                        {
                            data.opened_midi_out_hw
                                .iter()
                                .position(|d| d == device)
                                .map(|idx| {
                                    Self::midi_hw_out_port_pos(
                                        &data,
                                        device,
                                        idx,
                                        bounds,
                                        hw_width,
                                        midi_hw_box_h,
                                        midi_hw_box_gap,
                                    )
                                })
                        } else {
                            end_track_option.map(|t| {
                                let track_size = Self::track_box_size(t);
                                let port_idx =
                                    Self::connection_port_index(t, conn.kind, conn.to_port, true);
                                Self::track_port_position(t, port_idx, t.position, track_size)
                            })
                        };

                        if let (Some(start), Some(end)) = (start_point, end_point)
                            && is_bezier_hit(start, end, cursor_position, 20, 10.0)
                        {
                            clicked_connection = Some(idx);
                            break;
                        }
                    }

                    if let Some(idx) = clicked_connection {
                        data.plugin_graph_selected_connections.clear();
                        data.plugin_graph_selected_connectable_connections.clear();
                        data.plugin_graph_selected_plugins.clear();
                        return Some(Action::publish(Message::ConnectionViewSelectConnection(
                            idx,
                        )));
                    }

                    if let Some(action) = pending_action {
                        return Some(action);
                    }

                    return Some(Action::publish(Message::DeselectAll));
                }

                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                    data.plugin_graph_moving_plugin = None;
                    if let Some(conn) = data.connecting.take() {
                        let from_t = conn.from_track;
                        let from_p = conn.from_port;
                        let kind = conn.kind;
                        let is_input = conn.is_input;
                        let mut target_port = None;
                        let visible_names = Self::visible_track_names(&data);

                        if is_input {
                            for track in data.tracks.iter() {
                                if !visible_names.contains(&track.name) {
                                    continue;
                                }
                                let track_size = Self::track_box_size(track);
                                let total_outs = track.primary_audio_outs()
                                    + track.midi.outs
                                    + track.send_count();
                                for j in 0..total_outs {
                                    let port_pos = Self::track_output_port_position(
                                        track,
                                        j,
                                        track.position,
                                        track_size,
                                    );
                                    if cursor_position.distance(port_pos) < 10.0 {
                                        target_port = Some((track.name.clone(), j));
                                        break;
                                    }
                                }
                                if target_port.is_some() {
                                    break;
                                }
                            }

                            if target_port.is_none() && from_t != HW_IN_ID {
                                if let Some(folder) = Self::folder_track(&data) {
                                    let count = Self::folder_input_count(folder);
                                    for j in 0..count {
                                        let pos = Self::folder_input_port_position(
                                            folder, j, bounds, hw_width,
                                        );
                                        if cursor_position.distance(pos) < 10.0 {
                                            target_port = Some((HW_IN_ID.to_string(), j));
                                            break;
                                        }
                                    }
                                } else if let Some(hw_in) = &data.hw_in {
                                    for j in 0..hw_in.channels {
                                        let py = 50.0
                                            + ((bounds.height - 60.0)
                                                / (hw_in.channels + 1) as f32)
                                                * (j + 1) as f32;
                                        if cursor_position.distance(Point::new(hw_width, py)) < 10.0
                                        {
                                            target_port = Some((HW_IN_ID.to_string(), j));
                                            break;
                                        }
                                    }
                                }
                            }

                            if kind == Kind::MIDI
                                && target_port.is_none()
                                && data.connections_folder.is_none()
                            {
                                for (idx, device) in data.opened_midi_in_hw.iter().enumerate() {
                                    let port_pos = Self::midi_hw_in_port_pos(
                                        &data,
                                        device,
                                        idx,
                                        midi_hw_box_h,
                                        midi_hw_box_gap,
                                    );
                                    if cursor_position.distance(port_pos) < 10.0 {
                                        target_port =
                                            Some((format!("{MIDI_HW_IN_ID}:{device}"), 0));
                                        break;
                                    }
                                }
                            }
                        } else {
                            for track in data.tracks.iter() {
                                if !visible_names.contains(&track.name) {
                                    continue;
                                }
                                let track_size = Self::track_box_size(track);
                                let total_ins = track.primary_audio_ins()
                                    + track.midi.ins
                                    + track.return_count();
                                for j in 0..total_ins {
                                    let port_pos = Self::track_port_position(
                                        track,
                                        j,
                                        track.position,
                                        track_size,
                                    );
                                    if cursor_position.distance(port_pos) < 10.0 {
                                        target_port = Some((track.name.clone(), j));
                                        break;
                                    }
                                }
                                if target_port.is_some() {
                                    break;
                                }
                            }

                            if target_port.is_none() && from_t != HW_OUT_ID {
                                if let Some(folder) = Self::folder_track(&data) {
                                    let count = Self::folder_output_count(folder);
                                    for j in 0..count {
                                        let pos = Self::folder_output_port_position(
                                            folder, j, bounds, hw_width,
                                        );
                                        if cursor_position.distance(pos) < 10.0 {
                                            target_port = Some((HW_OUT_ID.to_string(), j));
                                            break;
                                        }
                                    }
                                } else if let Some(hw_out) = &data.hw_out {
                                    for j in 0..hw_out.channels {
                                        let py = 50.0
                                            + ((bounds.height - 60.0)
                                                / (hw_out.channels + 1) as f32)
                                                * (j + 1) as f32;
                                        if cursor_position
                                            .distance(Point::new(bounds.width - hw_width, py))
                                            < 10.0
                                        {
                                            target_port = Some((HW_OUT_ID.to_string(), j));
                                            break;
                                        }
                                    }
                                }
                            }

                            if kind == Kind::MIDI
                                && target_port.is_none()
                                && data.connections_folder.is_none()
                            {
                                for (idx, device) in data.opened_midi_out_hw.iter().enumerate() {
                                    let port_pos = Self::midi_hw_out_port_pos(
                                        &data,
                                        device,
                                        idx,
                                        bounds,
                                        hw_width,
                                        midi_hw_box_h,
                                        midi_hw_box_gap,
                                    );
                                    if cursor_position.distance(port_pos) < 10.0 {
                                        target_port =
                                            Some((format!("{MIDI_HW_OUT_ID}:{device}"), 0));
                                        break;
                                    }
                                }
                            }
                        }

                        if target_port.is_none()
                            && data.connections_folder.is_some()
                            && (from_t == HW_IN_ID
                                || from_t == HW_OUT_ID
                                || visible_names.contains(&from_t))
                        {
                            let plugin_hits = Self::plugin_only_port_hits(&data, bounds, hw_width);
                            let radius = if kind == Kind::Audio {
                                PORT_HIT_RADIUS
                            } else {
                                MIDI_HIT_RADIUS
                            };
                            let target = Self::closest_plugin_port_hit(
                                &plugin_hits
                                    .iter()
                                    .filter(|p| {
                                        p.is_input != is_input && can_connect_kinds(p.kind, kind)
                                    })
                                    .cloned()
                                    .collect::<Vec<_>>(),
                                cursor_position,
                                radius,
                            );
                            if let Some(target) = target {
                                let target_ref =
                                    Self::plugin_node_to_connectable_ref(&target.node)?;
                                if from_t == HW_IN_ID || from_t == HW_OUT_ID {
                                    let (from_node, from_port, to_node, to_port) = if is_input {
                                        (
                                            target.node,
                                            target.port,
                                            PluginGraphNode::TrackOutput,
                                            from_p,
                                        )
                                    } else {
                                        (
                                            PluginGraphNode::TrackInput,
                                            from_p,
                                            target.node,
                                            target.port,
                                        )
                                    };
                                    if let Some(action) = Self::plugin_graph_connection_actions(
                                        &data, from_node, from_port, to_node, to_port, kind,
                                    ) {
                                        return Some(action);
                                    }
                                } else {
                                    let source_track =
                                        data.tracks.iter().find(|t| t.name == from_t)?;
                                    let source_ref =
                                        ConnectableRef::ChildTrack(source_track.name.clone());
                                    if is_input {
                                        // Drag started on a child-track input; target is a plugin output.
                                        // Signal flows plugin output -> child input.
                                        let engine_port = Self::track_port_to_engine_index(
                                            source_track,
                                            from_p,
                                            true,
                                        )
                                        .1;
                                        return Self::connectable_connection_actions(
                                            &data,
                                            target_ref,
                                            target.port,
                                            source_ref,
                                            engine_port,
                                            kind,
                                        );
                                    } else {
                                        // Drag started on a child-track output; target is a plugin input.
                                        // Signal flows child output -> plugin input.
                                        let engine_port = Self::track_port_to_engine_index(
                                            source_track,
                                            from_p,
                                            false,
                                        )
                                        .1;
                                        return Self::connectable_connection_actions(
                                            &data,
                                            source_ref,
                                            engine_port,
                                            target_ref,
                                            target.port,
                                            kind,
                                        );
                                    }
                                }
                            }
                        }

                        if let Some((to_t_name, to_p)) = target_port {
                            let folder_name = data.connections_folder.clone();
                            let from_t = if data.connections_folder.is_some() && from_t == HW_IN_ID
                            {
                                folder_name.clone().unwrap()
                            } else {
                                from_t
                            };
                            let to_t_name =
                                if data.connections_folder.is_some() && to_t_name == HW_OUT_ID {
                                    folder_name.unwrap()
                                } else {
                                    to_t_name
                                };
                            let target_track_option =
                                data.tracks.iter().find(|t| t.name == to_t_name);

                            let is_target_midi_hw = to_t_name.starts_with(MIDI_HW_IN_ID)
                                || to_t_name.starts_with(MIDI_HW_OUT_ID);
                            let target_kind = if to_t_name == HW_IN_ID || to_t_name == HW_OUT_ID {
                                if let Some(folder) = Self::folder_track(&data) {
                                    Self::track_port_kind(folder, to_p, to_t_name == HW_IN_ID)
                                } else {
                                    Kind::Audio
                                }
                            } else if is_target_midi_hw {
                                Kind::MIDI
                            } else {
                                target_track_option
                                    .map(|t| Self::track_port_kind(t, to_p, !is_input))
                                    .unwrap_or(Kind::Audio)
                            };

                            if can_connect_kinds(kind, target_kind) {
                                let is_source_hw_audio = from_t == HW_IN_ID || from_t == HW_OUT_ID;
                                let is_source_midi_hw = from_t.starts_with(MIDI_HW_IN_ID)
                                    || from_t.starts_with(MIDI_HW_OUT_ID);

                                let parallel_count = if data.shift {
                                    let src_count = if is_source_hw_audio {
                                        if let Some(folder) = Self::folder_track(&data) {
                                            if from_t == HW_IN_ID {
                                                Self::folder_input_count(folder)
                                                    .saturating_sub(from_p)
                                            } else {
                                                Self::folder_output_count(folder)
                                                    .saturating_sub(from_p)
                                            }
                                        } else {
                                            data.hw_in
                                                .as_ref()
                                                .map(|h| h.channels.saturating_sub(from_p))
                                                .unwrap_or(0)
                                        }
                                    } else if is_source_midi_hw {
                                        1usize.saturating_sub(from_p)
                                    } else if let Some(t) =
                                        data.tracks.iter().find(|t| t.name == from_t)
                                    {
                                        let total = if is_input {
                                            t.primary_audio_ins() + t.midi.ins + t.return_count()
                                        } else {
                                            t.primary_audio_outs() + t.midi.outs + t.send_count()
                                        };
                                        (from_p..total)
                                            .take_while(|&p| {
                                                Self::track_port_kind(t, p, is_input) == kind
                                            })
                                            .count()
                                    } else {
                                        0
                                    };
                                    let tgt_count = if to_t_name == HW_IN_ID
                                        || to_t_name == HW_OUT_ID
                                    {
                                        if let Some(folder) = Self::folder_track(&data) {
                                            if to_t_name == HW_IN_ID {
                                                Self::folder_input_count(folder)
                                                    .saturating_sub(to_p)
                                            } else {
                                                Self::folder_output_count(folder)
                                                    .saturating_sub(to_p)
                                            }
                                        } else {
                                            let hw =
                                                if !is_input { &data.hw_in } else { &data.hw_out };
                                            hw.as_ref()
                                                .map(|h| h.channels.saturating_sub(to_p))
                                                .unwrap_or(0)
                                        }
                                    } else if is_target_midi_hw {
                                        1usize.saturating_sub(to_p)
                                    } else if let Some(t) = target_track_option {
                                        let total = if !is_input {
                                            t.primary_audio_ins() + t.midi.ins + t.return_count()
                                        } else {
                                            t.primary_audio_outs() + t.midi.outs + t.send_count()
                                        };
                                        (to_p..total)
                                            .take_while(|&p| {
                                                Self::track_port_kind(t, p, !is_input) == kind
                                            })
                                            .count()
                                    } else {
                                        0
                                    };
                                    src_count.min(tgt_count).max(1)
                                } else {
                                    1
                                };

                                let mut actions = Vec::with_capacity(parallel_count);
                                for offset in 0..parallel_count {
                                    let f_p_idx = if is_source_hw_audio || is_source_midi_hw {
                                        from_p + offset
                                    } else {
                                        let t =
                                            data.tracks.iter().find(|t| t.name == from_t).unwrap();
                                        Self::track_port_to_engine_index(
                                            t,
                                            from_p + offset,
                                            is_input,
                                        )
                                        .1
                                    };

                                    let t_p_idx = if to_t_name == HW_IN_ID
                                        || to_t_name == HW_OUT_ID
                                        || is_target_midi_hw
                                    {
                                        to_p + offset
                                    } else {
                                        let t = target_track_option.unwrap();
                                        Self::track_port_to_engine_index(
                                            t,
                                            to_p + offset,
                                            !is_input,
                                        )
                                        .1
                                    };

                                    let (final_from, final_f_p, final_to, final_t_p) = if is_input {
                                        (to_t_name.clone(), t_p_idx, from_t.clone(), f_p_idx)
                                    } else {
                                        (from_t.clone(), f_p_idx, to_t_name.clone(), t_p_idx)
                                    };

                                    actions.push(EngineAction::Connect {
                                        from_track: final_from,
                                        from_port: final_f_p,
                                        to_track: final_to,
                                        to_port: final_t_p,
                                        kind,
                                    });
                                }

                                if actions.len() == 1 {
                                    return Some(Action::publish(Message::Request(
                                        actions.into_iter().next().unwrap(),
                                    )));
                                } else {
                                    return Some(Action::publish(Message::RequestBatch(actions)));
                                }
                            }
                        }
                    }
                    if let Some(connecting) = data.plugin_graph_connecting.take() {
                        let hits = Self::plugin_port_hits(&data, bounds, hw_width);
                        let radius = if connecting.kind == Kind::Audio {
                            PORT_HIT_RADIUS
                        } else {
                            MIDI_HIT_RADIUS
                        };
                        let target = Self::closest_plugin_port_hit(
                            &hits
                                .iter()
                                .filter(|p| {
                                    p.is_input != connecting.is_input
                                        && can_connect_kinds(p.kind, connecting.kind)
                                })
                                .cloned()
                                .collect::<Vec<_>>(),
                            cursor_position,
                            radius,
                        );
                        if let Some(target) = target {
                            let from_node = connecting.from_node.clone();
                            let (from_node, from_port, to_node, to_port) = if connecting.is_input {
                                (target.node, target.port, from_node, connecting.from_port)
                            } else {
                                (from_node, connecting.from_port, target.node, target.port)
                            };
                            if let Some(action) = Self::plugin_graph_connection_actions(
                                &data,
                                from_node,
                                from_port,
                                to_node,
                                to_port,
                                connecting.kind,
                            ) {
                                return Some(action);
                            }
                        }

                        // Allow dropping a plugin port onto a child-track port.
                        if data.connections_folder.is_some() {
                            let visible_names = Self::visible_track_names(&data);
                            for track in data
                                .tracks
                                .iter()
                                .filter(|t| visible_names.contains(&t.name))
                            {
                                let size = Self::track_box_size(track);
                                if connecting.is_input {
                                    let total_outs = track.primary_audio_outs()
                                        + track.midi.outs
                                        + track.send_count();
                                    for j in 0..total_outs {
                                        if Self::track_port_kind(track, j, false) != connecting.kind
                                        {
                                            continue;
                                        }
                                        let pos = Self::track_output_port_position(
                                            track,
                                            j,
                                            track.position,
                                            size,
                                        );
                                        if cursor_position.distance(pos) < radius {
                                            let engine_port =
                                                Self::track_port_to_engine_index(track, j, false).1;
                                            let from =
                                                ConnectableRef::ChildTrack(track.name.clone());
                                            let to = Self::plugin_node_to_connectable_ref(
                                                &connecting.from_node,
                                            )?;
                                            if let Some(action) =
                                                Self::connectable_connection_actions(
                                                    &data,
                                                    from,
                                                    engine_port,
                                                    to,
                                                    connecting.from_port,
                                                    connecting.kind,
                                                )
                                            {
                                                return Some(action);
                                            }
                                        }
                                    }
                                } else {
                                    let total_ins = track.primary_audio_ins()
                                        + track.midi.ins
                                        + track.return_count();
                                    for j in 0..total_ins {
                                        if Self::track_port_kind(track, j, true) != connecting.kind
                                        {
                                            continue;
                                        }
                                        let pos = Self::track_port_position(
                                            track,
                                            j,
                                            track.position,
                                            size,
                                        );
                                        if cursor_position.distance(pos) < radius {
                                            let engine_port =
                                                Self::track_port_to_engine_index(track, j, true).1;
                                            let from = Self::plugin_node_to_connectable_ref(
                                                &connecting.from_node,
                                            )?;
                                            let to = ConnectableRef::ChildTrack(track.name.clone());
                                            if let Some(action) =
                                                Self::connectable_connection_actions(
                                                    &data,
                                                    from,
                                                    connecting.from_port,
                                                    to,
                                                    engine_port,
                                                    connecting.kind,
                                                )
                                            {
                                                return Some(action);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        return Some(Action::request_redraw());
                    }
                    data.moving_track = None;
                    return Some(Action::request_redraw());
                }

                Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                    let mut new_h = None;

                    let folder = Self::folder_track(&data);
                    let folder_view = folder.is_some();
                    if new_h.is_none()
                        && let Some(folder) = folder
                    {
                        let in_count = Self::folder_input_count(folder);
                        for j in 0..in_count {
                            let pos = Self::folder_input_port_position(folder, j, bounds, hw_width);
                            if cursor_position.distance(pos) < 10.0 {
                                new_h = Some(Hovering::Port {
                                    track_idx: HW_IN_ID.to_string(),
                                    port_idx: j,
                                    is_input: false,
                                });
                                break;
                            }
                        }
                        if new_h.is_none() {
                            let out_count = Self::folder_output_count(folder);
                            for j in 0..out_count {
                                let pos =
                                    Self::folder_output_port_position(folder, j, bounds, hw_width);
                                if cursor_position.distance(pos) < 10.0 {
                                    new_h = Some(Hovering::Port {
                                        track_idx: HW_OUT_ID.to_string(),
                                        port_idx: j,
                                        is_input: true,
                                    });
                                    break;
                                }
                            }
                        }
                    }

                    if new_h.is_none()
                        && !folder_view
                        && let Some(hw_in) = &data.hw_in
                    {
                        let pos = Point::new(0.0, 0.0);
                        for j in 0..hw_in.channels {
                            let py = pos.y
                                + 50.0
                                + ((bounds.height - 60.0) / (hw_in.channels + 1) as f32)
                                    * (j + 1) as f32;
                            if cursor_position.distance(Point::new(pos.x + hw_width, py)) < 10.0 {
                                new_h = Some(Hovering::Port {
                                    track_idx: HW_IN_ID.to_string(),
                                    port_idx: j,
                                    is_input: false,
                                });
                                break;
                            }
                        }
                    }

                    if new_h.is_none()
                        && !folder_view
                        && let Some(hw_out) = &data.hw_out
                    {
                        let pos = Point::new(bounds.width - hw_width, 0.0);
                        for j in 0..hw_out.channels {
                            let py = pos.y
                                + 50.0
                                + ((bounds.height - 60.0) / (hw_out.channels + 1) as f32)
                                    * (j + 1) as f32;
                            if cursor_position.distance(Point::new(pos.x, py)) < 10.0 {
                                new_h = Some(Hovering::Port {
                                    track_idx: HW_OUT_ID.to_string(),
                                    port_idx: j,
                                    is_input: true,
                                });
                                break;
                            }
                        }
                    }

                    if new_h.is_none() && !folder_view {
                        for (idx, device) in data.opened_midi_in_hw.iter().enumerate() {
                            let port_pos = Self::midi_hw_in_port_pos(
                                &data,
                                device,
                                idx,
                                midi_hw_box_h,
                                midi_hw_box_gap,
                            );
                            if cursor_position.distance(port_pos) < 10.0 {
                                new_h = Some(Hovering::Port {
                                    track_idx: format!("{MIDI_HW_IN_ID}:{device}"),
                                    port_idx: 0,
                                    is_input: false,
                                });
                                break;
                            }
                        }
                    }

                    if new_h.is_none() && !folder_view {
                        for (idx, device) in data.opened_midi_out_hw.iter().enumerate() {
                            let port_pos = Self::midi_hw_out_port_pos(
                                &data,
                                device,
                                idx,
                                bounds,
                                hw_width,
                                midi_hw_box_h,
                                midi_hw_box_gap,
                            );
                            if cursor_position.distance(port_pos) < 10.0 {
                                new_h = Some(Hovering::Port {
                                    track_idx: format!("{MIDI_HW_OUT_ID}:{device}"),
                                    port_idx: 0,
                                    is_input: true,
                                });
                                break;
                            }
                        }
                    }

                    let visible_names = Self::visible_track_names(&data);
                    if new_h.is_none() {
                        for track in data.tracks.iter().rev() {
                            if !visible_names.contains(&track.name) {
                                continue;
                            }
                            let track_size = Self::track_box_size(track);
                            let t_ins =
                                track.primary_audio_ins() + track.midi.ins + track.return_count();
                            for j in 0..t_ins {
                                let port_pos =
                                    Self::track_port_position(track, j, track.position, track_size);
                                if cursor_position.distance(port_pos) < 10.0 {
                                    new_h = Some(Hovering::Port {
                                        track_idx: track.name.clone(),
                                        port_idx: j,
                                        is_input: true,
                                    });
                                    break;
                                }
                            }
                            if new_h.is_some() {
                                break;
                            }

                            let t_outs =
                                track.primary_audio_outs() + track.midi.outs + track.send_count();
                            for j in 0..t_outs {
                                let port_pos = Self::track_output_port_position(
                                    track,
                                    j,
                                    track.position,
                                    track_size,
                                );
                                if cursor_position.distance(port_pos) < 10.0 {
                                    new_h = Some(Hovering::Port {
                                        track_idx: track.name.clone(),
                                        port_idx: j,
                                        is_input: false,
                                    });
                                    break;
                                }
                            }
                            if new_h.is_some() {
                                break;
                            }

                            if Rectangle::new(track.position, track_size).contains(cursor_position)
                            {
                                new_h = Some(Hovering::Track(track.name.clone()));
                                break;
                            }
                        }
                    }

                    if new_h.is_none() && data.connections_folder.is_some() {
                        for (idx, plugin) in data.plugin_graph_plugins.iter().enumerate().rev() {
                            let pos = Self::plugin_node_position(&data, plugin, idx, bounds);
                            let total_ins = plugin.audio_inputs + plugin.midi_inputs;
                            for j in 0..total_ins {
                                let Some(point) = Self::plugin_port_position(plugin, pos, j, true)
                                else {
                                    continue;
                                };
                                let radius =
                                    if Self::plugin_port_kind(plugin, j, true) == Kind::Audio {
                                        PORT_HIT_RADIUS
                                    } else {
                                        MIDI_HIT_RADIUS
                                    };
                                if cursor_position.distance(point) <= radius {
                                    new_h = Some(Hovering::PluginPort {
                                        instance_id: plugin.instance_id,
                                        port_idx: j,
                                        is_input: true,
                                    });
                                    break;
                                }
                            }
                            if new_h.is_some() {
                                break;
                            }

                            let total_outs = plugin.audio_outputs + plugin.midi_outputs;
                            for j in 0..total_outs {
                                let Some(point) = Self::plugin_port_position(plugin, pos, j, false)
                                else {
                                    continue;
                                };
                                let radius =
                                    if Self::plugin_port_kind(plugin, j, false) == Kind::Audio {
                                        PORT_HIT_RADIUS
                                    } else {
                                        MIDI_HIT_RADIUS
                                    };
                                if cursor_position.distance(point) <= radius {
                                    new_h = Some(Hovering::PluginPort {
                                        instance_id: plugin.instance_id,
                                        port_idx: j,
                                        is_input: false,
                                    });
                                    break;
                                }
                            }
                            if new_h.is_some() {
                                break;
                            }

                            let size = Self::plugin_box_size(plugin);
                            if Rectangle::new(pos, size).contains(cursor_position) {
                                new_h = Some(Hovering::Plugin {
                                    instance_id: plugin.instance_id,
                                });
                                break;
                            }
                        }
                    }

                    let mut redraw_needed = false;

                    if let Some(ref mut conn) = data.connecting {
                        conn.point = cursor_position;
                        redraw_needed = true;
                    }
                    if let Some(mt) = data.moving_track.clone() {
                        if let Some(t) = data.tracks.iter_mut().find(|tr| tr.name == mt.track_idx) {
                            if visible_names.contains(&mt.track_idx) {
                                t.position.x = cursor_position.x - mt.offset_x;
                                t.position.y = cursor_position.y - mt.offset_y;
                                redraw_needed = true;
                            }
                        } else if let Some(device) =
                            mt.track_idx.strip_prefix(&format!("{MIDI_HW_IN_ID}:"))
                        {
                            data.midi_hw_in_positions.insert(
                                device.to_string(),
                                Point::new(
                                    cursor_position.x - mt.offset_x,
                                    cursor_position.y - mt.offset_y,
                                ),
                            );
                            redraw_needed = true;
                        } else if let Some(device) =
                            mt.track_idx.strip_prefix(&format!("{MIDI_HW_OUT_ID}:"))
                        {
                            data.midi_hw_out_positions.insert(
                                device.to_string(),
                                Point::new(
                                    cursor_position.x - mt.offset_x,
                                    cursor_position.y - mt.offset_y,
                                ),
                            );
                            redraw_needed = true;
                        }
                    }

                    if let Some(ref mut conn) = data.plugin_graph_connecting {
                        conn.point = cursor_position;
                        redraw_needed = true;
                    }
                    if let Some(mp) = data.plugin_graph_moving_plugin.clone() {
                        data.plugin_graph_plugin_positions.insert(
                            mp.instance_id,
                            Point::new(
                                cursor_position.x - mp.offset_x,
                                cursor_position.y - mp.offset_y,
                            ),
                        );
                        redraw_needed = true;
                    }

                    if data.hovering != new_h {
                        data.hovering = new_h;
                        redraw_needed = true;
                    }

                    if redraw_needed {
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
        let hw_width = 70.0;
        let midi_hw_box_h = 24.0;
        let midi_hw_box_gap = 6.0;
        let cursor_position = cursor.position_in(bounds);
        let rgb8 = |r: u8, g: u8, b: u8| Color::from_rgb8(r, g, b);
        let draw_gradient_box = |frame: &mut Frame, pos: Point, size: iced::Size, base: Color| {
            frame.fill(&Path::rectangle(pos, size), base);

            let top_h = (size.height * 0.45).max(4.0).min(size.height);
            let bottom_h = (size.height * 0.28).max(3.0).min(size.height);
            frame.fill(
                &Path::rectangle(pos, iced::Size::new(size.width, top_h)),
                Color::from_rgba(1.0, 1.0, 1.0, 0.05),
            );
            frame.fill(
                &Path::rectangle(
                    Point::new(pos.x, pos.y + size.height - bottom_h),
                    iced::Size::new(size.width, bottom_h),
                ),
                Color::from_rgba(0.0, 0.0, 0.0, 0.08),
            );
        };
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
        let track_node_fill = rgb8(36, 45, 68);
        let node_border = rgb8(78, 93, 130);
        let node_hover = rgb8(106, 122, 158);
        let node_selected = rgb8(123, 173, 240);
        let midi_box_fill = rgb8(55, 90, 50);
        let midi_box_selected_fill = rgb8(84, 133, 72);
        let midi_box_border = rgb8(148, 215, 118);
        let conn_audio = Color::from_rgb(0.36, 0.66, 0.98);
        let conn_midi = Color::from_rgb(0.30, 0.82, 0.38);
        let conn_selected = Color::from_rgb(0.72, 0.90, 1.0);
        frame.fill(&Path::rectangle(Point::new(0.0, 0.0), bounds.size()), bg);
        draw_grid(&mut frame, bounds.width, bounds.height);

        if let Ok(data) = self.state.try_read() {
            use crate::state::ConnectionViewSelection;

            let visible_names = Self::visible_track_names(&data);
            if let Some(folder) = &data.connections_folder {
                frame.fill_text(Text {
                    content: format!("Folder: {folder}"),
                    position: Point::new(bounds.width / 2.0, 18.0),
                    color: Color::from_rgb(0.78, 0.86, 1.0),
                    size: 14.0.into(),
                    align_x: Horizontal::Center.into(),
                    align_y: Vertical::Center,
                    ..Default::default()
                });
            }

            let folder_name_opt = data.connections_folder.as_ref();
            for (idx, conn) in data.connections.iter().enumerate() {
                // In folder view, hide track-level connections that involve the folder
                // track itself; those are managed in the root connections view.
                let from_visible = if data.connections_folder.is_some() {
                    Self::is_track_view_hw_node(&conn.from_track)
                        || visible_names.contains(&conn.from_track)
                } else {
                    Self::is_hw_node(&conn.from_track) || visible_names.contains(&conn.from_track)
                };
                let to_visible = if data.connections_folder.is_some() {
                    Self::is_track_view_hw_node(&conn.to_track)
                        || visible_names.contains(&conn.to_track)
                } else {
                    Self::is_hw_node(&conn.to_track) || visible_names.contains(&conn.to_track)
                };
                if !from_visible || !to_visible {
                    continue;
                }
                let start_track_option = data.tracks.iter().find(|t| t.name == conn.from_track);
                let end_track_option = data.tracks.iter().find(|t| t.name == conn.to_track);

                let start_is_folder = folder_name_opt == Some(&conn.from_track);
                let end_is_folder = folder_name_opt == Some(&conn.to_track);

                let start_point = if conn.from_track == HW_IN_ID
                    || (data.connections_folder.is_some() && start_is_folder)
                {
                    if let Some(folder) = Self::folder_track(&data) {
                        Some(Self::folder_input_port_position(
                            folder,
                            conn.from_port,
                            bounds,
                            hw_width,
                        ))
                    } else {
                        data.hw_in.as_ref().map(move |hw_in| {
                            let py = 50.0
                                + ((bounds.height - 60.0) / (hw_in.channels + 1) as f32)
                                    * (conn.from_port + 1) as f32;
                            Point::new(hw_width, py)
                        })
                    }
                } else if let Some(device) =
                    conn.from_track.strip_prefix(&format!("{MIDI_HW_IN_ID}:"))
                {
                    data.opened_midi_in_hw
                        .iter()
                        .position(|d| d == device)
                        .map(|idx| {
                            Self::midi_hw_in_port_pos(
                                &data,
                                device,
                                idx,
                                midi_hw_box_h,
                                midi_hw_box_gap,
                            )
                        })
                } else {
                    start_track_option.map(|t| {
                        let track_size = Self::track_box_size(t);
                        let port_idx =
                            Self::connection_port_index(t, conn.kind, conn.from_port, false);
                        Self::track_output_port_position(t, port_idx, t.position, track_size)
                    })
                };

                let end_point = if conn.to_track == HW_OUT_ID
                    || (data.connections_folder.is_some() && end_is_folder)
                {
                    if let Some(folder) = Self::folder_track(&data) {
                        Some(Self::folder_output_port_position(
                            folder,
                            conn.to_port,
                            bounds,
                            hw_width,
                        ))
                    } else {
                        data.hw_out.as_ref().map(move |hw_out| {
                            let py = 50.0
                                + ((bounds.height - 60.0) / (hw_out.channels + 1) as f32)
                                    * (conn.to_port + 1) as f32;
                            Point::new(bounds.width - hw_width, py)
                        })
                    }
                } else if let Some(device) =
                    conn.to_track.strip_prefix(&format!("{MIDI_HW_OUT_ID}:"))
                {
                    data.opened_midi_out_hw
                        .iter()
                        .position(|d| d == device)
                        .map(|idx| {
                            Self::midi_hw_out_port_pos(
                                &data,
                                device,
                                idx,
                                bounds,
                                hw_width,
                                midi_hw_box_h,
                                midi_hw_box_gap,
                            )
                        })
                } else {
                    end_track_option.map(|t| {
                        let track_size = Self::track_box_size(t);
                        let port_idx =
                            Self::connection_port_index(t, conn.kind, conn.to_port, true);
                        Self::track_port_position(t, port_idx, t.position, track_size)
                    })
                };

                if let (Some(start), Some(end)) = (start_point, end_point) {
                    let start_edge = if let Some(track) = start_track_option {
                        Self::track_port_edge(
                            track,
                            Self::connection_port_index(track, conn.kind, conn.from_port, false),
                            false,
                        )
                    } else {
                        TrackPortEdge::Right
                    };
                    let end_edge = if let Some(track) = end_track_option {
                        Self::track_port_edge(
                            track,
                            Self::connection_port_index(track, conn.kind, conn.to_port, true),
                            true,
                        )
                    } else {
                        TrackPortEdge::Left
                    };
                    let (c1, c2) = Self::bezier_controls(start, start_edge, end, end_edge);
                    let path = Path::new(|p| {
                        p.move_to(start);
                        p.bezier_curve_to(c1, c2, end);
                    });

                    let is_selected = matches!(&data.connection_view_selection, ConnectionViewSelection::Connections(set) if set.contains(&idx));
                    let is_hovered = cursor_position
                        .is_some_and(|cursor| is_bezier_hit(start, end, cursor, 20, 10.0));
                    let (color, width) = if is_selected {
                        (conn_selected, 4.0)
                    } else if is_hovered {
                        let c = match conn.kind {
                            Kind::Audio => conn_audio,
                            Kind::MIDI => conn_midi,
                        };
                        (c, 3.0)
                    } else {
                        let c = match conn.kind {
                            Kind::Audio => conn_audio,
                            Kind::MIDI => conn_midi,
                        };
                        (c, 2.0)
                    };
                    frame.stroke(
                        &path,
                        canvas::Stroke::default()
                            .with_color(color)
                            .with_width(width),
                    );
                }
            }

            // Draw folder plugin graph connections in the same canvas.
            if data.connections_folder.is_some() {
                for (idx, conn) in data.plugin_graph_connections.iter().enumerate() {
                    let start = Self::plugin_graph_node_port_position(
                        &data,
                        &conn.from_node,
                        conn.from_port,
                        false,
                        bounds,
                        hw_width,
                    );
                    let end = Self::plugin_graph_node_port_position(
                        &data,
                        &conn.to_node,
                        conn.to_port,
                        true,
                        bounds,
                        hw_width,
                    );
                    let Some(start) = start else { continue };
                    let Some(end) = end else { continue };

                    let start_edge = Self::plugin_graph_node_edge(&conn.from_node, false);
                    let end_edge = Self::plugin_graph_node_edge(&conn.to_node, true);
                    let (c1, c2) = Self::bezier_controls(start, start_edge, end, end_edge);
                    let path = Path::new(|p| {
                        p.move_to(start);
                        p.bezier_curve_to(c1, c2, end);
                    });

                    let is_selected = data.plugin_graph_selected_connections.contains(&idx);
                    let color = if is_selected {
                        conn_selected
                    } else {
                        match conn.kind {
                            Kind::Audio => conn_audio,
                            Kind::MIDI => conn_midi,
                        }
                    };
                    let width = if is_selected { 4.0 } else { 2.0 };
                    frame.stroke(
                        &path,
                        canvas::Stroke::default()
                            .with_color(color)
                            .with_width(width),
                    );
                }

                // Draw all connectable connections that involve a child track. This covers
                // child↔folder I/O wiring as well as child↔plugin routing.
                for (idx, conn) in data.connectable_connections.iter().enumerate() {
                    let involves_child = matches!(conn.from, ConnectableRef::ChildTrack(_))
                        || matches!(conn.to, ConnectableRef::ChildTrack(_));
                    if !involves_child {
                        continue;
                    }
                    let Some(start) = Self::connectable_port_position(
                        &data,
                        &conn.from,
                        conn.from_port,
                        false,
                        conn.kind,
                        bounds,
                        hw_width,
                    ) else {
                        continue;
                    };
                    let Some(end) = Self::connectable_port_position(
                        &data,
                        &conn.to,
                        conn.to_port,
                        true,
                        conn.kind,
                        bounds,
                        hw_width,
                    ) else {
                        continue;
                    };
                    let start_edge = Self::connectable_port_edge(&conn.from, false);
                    let end_edge = Self::connectable_port_edge(&conn.to, true);
                    let (c1, c2) = Self::bezier_controls(start, start_edge, end, end_edge);
                    let path = Path::new(|p| {
                        p.move_to(start);
                        p.bezier_curve_to(c1, c2, end);
                    });
                    let is_selected = data
                        .plugin_graph_selected_connectable_connections
                        .contains(&idx);
                    let color = if is_selected {
                        conn_selected
                    } else {
                        match conn.kind {
                            Kind::Audio => conn_audio,
                            Kind::MIDI => conn_midi,
                        }
                    };
                    let width = if is_selected { 4.0 } else { 2.0 };
                    frame.stroke(
                        &path,
                        canvas::Stroke::default()
                            .with_color(color)
                            .with_width(width),
                    );
                }
            }

            if let Some(conn) = &data.connecting {
                let start_track_option = data.tracks.iter().find(|t| t.name == conn.from_track);

                let preview_count = if data.shift {
                    if conn.from_track == HW_IN_ID {
                        data.hw_in
                            .as_ref()
                            .map(|h| h.channels.saturating_sub(conn.from_port))
                            .unwrap_or(1)
                            .max(1)
                    } else if conn.from_track == HW_OUT_ID {
                        data.hw_out
                            .as_ref()
                            .map(|h| h.channels.saturating_sub(conn.from_port))
                            .unwrap_or(1)
                            .max(1)
                    } else if conn.from_track.starts_with(MIDI_HW_IN_ID)
                        || conn.from_track.starts_with(MIDI_HW_OUT_ID)
                    {
                        1usize.saturating_sub(conn.from_port).max(1)
                    } else if let Some(t) = start_track_option {
                        let total = if conn.is_input {
                            t.primary_audio_ins() + t.midi.ins + t.return_count()
                        } else {
                            t.primary_audio_outs() + t.midi.outs + t.send_count()
                        };
                        (conn.from_port..total)
                            .take_while(|&p| {
                                Self::track_port_kind(t, p, conn.is_input) == conn.kind
                            })
                            .count()
                            .max(1)
                    } else {
                        1
                    }
                } else {
                    1
                };

                for offset in 0..preview_count {
                    let from_port = conn.from_port + offset;
                    let start_point = if conn.from_track == HW_IN_ID {
                        data.hw_in.as_ref().map(move |hw_in| {
                            let py = 50.0
                                + ((bounds.height - 60.0) / (hw_in.channels + 1) as f32)
                                    * (from_port + 1) as f32;
                            Point::new(hw_width, py)
                        })
                    } else if conn.from_track == HW_OUT_ID {
                        data.hw_out.as_ref().map(move |hw_out| {
                            let py = 50.0
                                + ((bounds.height - 60.0) / (hw_out.channels + 1) as f32)
                                    * (from_port + 1) as f32;
                            Point::new(bounds.width - hw_width, py)
                        })
                    } else if let Some(device) =
                        conn.from_track.strip_prefix(&format!("{MIDI_HW_IN_ID}:"))
                    {
                        data.opened_midi_in_hw
                            .iter()
                            .position(|d| d == device)
                            .map(|idx| {
                                Self::midi_hw_in_port_pos(
                                    &data,
                                    device,
                                    idx,
                                    midi_hw_box_h,
                                    midi_hw_box_gap,
                                )
                            })
                    } else if let Some(device) =
                        conn.from_track.strip_prefix(&format!("{MIDI_HW_OUT_ID}:"))
                    {
                        data.opened_midi_out_hw
                            .iter()
                            .position(|d| d == device)
                            .map(|idx| {
                                Self::midi_hw_out_port_pos(
                                    &data,
                                    device,
                                    idx,
                                    bounds,
                                    hw_width,
                                    midi_hw_box_h,
                                    midi_hw_box_gap,
                                )
                            })
                    } else {
                        start_track_option.map(|t| {
                            let track_size = Self::track_box_size(t);
                            if conn.is_input {
                                Self::track_port_position(t, from_port, t.position, track_size)
                            } else {
                                Self::track_output_port_position(
                                    t, from_port, t.position, track_size,
                                )
                            }
                        })
                    };

                    if let Some(start) = start_point {
                        let end = conn.point;
                        let start_edge = if let Some(track) = start_track_option {
                            Self::track_port_edge(track, from_port, conn.is_input)
                        } else {
                            match conn.from_track.as_str() {
                                HW_IN_ID => TrackPortEdge::Right,
                                HW_OUT_ID => TrackPortEdge::Left,
                                _ if conn.from_track.starts_with(MIDI_HW_IN_ID) => {
                                    TrackPortEdge::Right
                                }
                                _ if conn.from_track.starts_with(MIDI_HW_OUT_ID) => {
                                    TrackPortEdge::Left
                                }
                                _ => {
                                    if conn.is_input {
                                        TrackPortEdge::Left
                                    } else {
                                        TrackPortEdge::Right
                                    }
                                }
                            }
                        };
                        let end_edge = if conn.is_input {
                            TrackPortEdge::Right
                        } else {
                            TrackPortEdge::Left
                        };
                        let (c1, c2) = Self::bezier_controls(start, start_edge, end, end_edge);
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
            }

            if let Some(connecting) = &data.plugin_graph_connecting {
                let start = Self::plugin_graph_node_port_position(
                    &data,
                    &connecting.from_node,
                    connecting.from_port,
                    connecting.is_input,
                    bounds,
                    hw_width,
                );
                if let Some(start) = start {
                    let end = connecting.point;
                    let start_edge =
                        Self::plugin_graph_node_edge(&connecting.from_node, connecting.is_input);
                    let end_edge = if connecting.is_input {
                        TrackPortEdge::Right
                    } else {
                        TrackPortEdge::Left
                    };
                    let (c1, c2) = Self::bezier_controls(start, start_edge, end, end_edge);
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

            if Self::folder_track(&data).is_some() {
                Self::draw_folder_side_panel(
                    &mut frame,
                    &data,
                    bounds,
                    hw_width,
                    true,
                    edge_panel,
                    edge_panel_border,
                );
                Self::draw_folder_side_panel(
                    &mut frame,
                    &data,
                    bounds,
                    hw_width,
                    false,
                    edge_panel,
                    edge_panel_border,
                );
            }

            if data.connections_folder.is_none()
                && let Some(hw_in) = &data.hw_in
            {
                let pos = Point::new(0.0, 0.0);
                let rect = Path::rectangle(pos, iced::Size::new(hw_width, bounds.height));
                frame.fill(&rect, edge_panel);
                frame.stroke(
                    &rect,
                    canvas::Stroke::default()
                        .with_color(edge_panel_border)
                        .with_width(2.0),
                );
                frame.fill_text(Text {
                    content: "hw:in".into(),
                    position: Point::new(pos.x + hw_width / 2.0, pos.y + 20.0),
                    color: Color::WHITE,
                    align_x: Horizontal::Center.into(),
                    ..Default::default()
                });
                for j in 0..hw_in.channels {
                    let py = pos.y
                        + 50.0
                        + ((bounds.height - 60.0) / (hw_in.channels + 1) as f32) * (j + 1) as f32;
                    frame.fill_text(Text {
                        content: format!("{}", j + 1),
                        position: Point::new(pos.x + hw_width - 10.0, py),
                        color: Color::from_rgb(0.65, 0.72, 0.84),
                        size: 10.0.into(),
                        align_x: Horizontal::Right.into(),
                        align_y: Vertical::Center,
                        ..Default::default()
                    });
                    let h_port = Hovering::Port {
                        track_idx: HW_IN_ID.to_string(),
                        port_idx: j,
                        is_input: false,
                    };
                    let h = data.hovering == Some(h_port.clone());

                    let can_highlight_port = should_highlight_port(
                        h,
                        data.connecting.as_ref().map(|c| c.kind),
                        Self::get_port_kind(&data, &h_port).unwrap_or(Kind::Audio),
                    );

                    frame.fill(
                        &Path::circle(
                            Point::new(pos.x + hw_width, py),
                            hover_radius(5.0, can_highlight_port),
                        ),
                        audio_port_color(),
                    );
                }
            }

            if data.connections_folder.is_none()
                && let Some(hw_out) = &data.hw_out
            {
                let pos = Point::new(bounds.width - hw_width, 0.0);
                let rect = Path::rectangle(pos, iced::Size::new(hw_width, bounds.height));
                frame.fill(&rect, edge_panel);
                frame.stroke(
                    &rect,
                    canvas::Stroke::default()
                        .with_color(edge_panel_border)
                        .with_width(2.0),
                );
                frame.fill_text(Text {
                    content: "hw:out".into(),
                    position: Point::new(pos.x + hw_width / 2.0, pos.y + 20.0),
                    color: Color::WHITE,
                    align_x: Horizontal::Center.into(),
                    ..Default::default()
                });
                for j in 0..hw_out.channels {
                    let py = pos.y
                        + 50.0
                        + ((bounds.height - 60.0) / (hw_out.channels + 1) as f32) * (j + 1) as f32;
                    frame.fill_text(Text {
                        content: format!("{}", j + 1),
                        position: Point::new(pos.x + 10.0, py),
                        color: Color::from_rgb(0.65, 0.72, 0.84),
                        size: 10.0.into(),
                        align_x: Horizontal::Left.into(),
                        align_y: Vertical::Center,
                        ..Default::default()
                    });
                    let h_port = Hovering::Port {
                        track_idx: HW_OUT_ID.to_string(),
                        port_idx: j,
                        is_input: true,
                    };
                    let h = data.hovering == Some(h_port.clone());

                    let can_highlight_port = should_highlight_port(
                        h,
                        data.connecting.as_ref().map(|c| c.kind),
                        Self::get_port_kind(&data, &h_port).unwrap_or(Kind::Audio),
                    );

                    frame.fill(
                        &Path::circle(Point::new(pos.x, py), hover_radius(5.0, can_highlight_port)),
                        audio_port_color(),
                    );
                }
            }

            if data.connections_folder.is_none() {
                for (j, device) in data.opened_midi_in_hw.iter().enumerate() {
                    let label = Self::midi_device_label(&data, device);
                    let default_rect =
                        Self::default_midi_in_rect(j, &label, midi_hw_box_h, midi_hw_box_gap);
                    let pos = data
                        .midi_hw_in_positions
                        .get(device)
                        .copied()
                        .unwrap_or(Point::new(default_rect.x, default_rect.y));
                    let selected_id = format!("{MIDI_HW_IN_ID}:{device}");
                    let is_selected = data
                        .moving_track
                        .as_ref()
                        .is_some_and(|mt| mt.track_idx == selected_id);
                    let rect = Path::rectangle(
                        pos,
                        iced::Size::new(default_rect.width, default_rect.height),
                    );
                    let fill_color = if is_selected {
                        midi_box_selected_fill
                    } else {
                        midi_box_fill
                    };
                    let stroke_color = if is_selected {
                        midi_port_color()
                    } else {
                        midi_box_border
                    };
                    draw_gradient_box(
                        &mut frame,
                        pos,
                        iced::Size::new(default_rect.width, default_rect.height),
                        fill_color,
                    );
                    frame.stroke(
                        &rect,
                        canvas::Stroke::default()
                            .with_color(stroke_color)
                            .with_width(2.0),
                    );
                    frame.fill_text(Text {
                        content: label,
                        position: Point::new(
                            pos.x + default_rect.width / 2.0,
                            pos.y + default_rect.height / 2.0,
                        ),
                        color: Color::WHITE,
                        size: 11.0.into(),
                        align_x: Horizontal::Center.into(),
                        align_y: Vertical::Center,
                        ..Default::default()
                    });
                    frame.fill(
                        &Path::circle(
                            Point::new(
                                pos.x + default_rect.width,
                                pos.y + default_rect.height / 2.0,
                            ),
                            hover_radius(
                                5.0,
                                should_highlight_port(
                                    data.hovering
                                        == Some(Hovering::Port {
                                            track_idx: selected_id.clone(),
                                            port_idx: 0,
                                            is_input: false,
                                        }),
                                    data.connecting.as_ref().map(|c| c.kind),
                                    Kind::MIDI,
                                ),
                            ),
                        ),
                        midi_port_color(),
                    );
                }

                for (j, device) in data.opened_midi_out_hw.iter().enumerate() {
                    let label = Self::midi_device_label(&data, device);
                    let default_rect = Self::default_midi_out_rect(
                        j,
                        &label,
                        bounds,
                        hw_width,
                        midi_hw_box_h,
                        midi_hw_box_gap,
                    );
                    let pos = data
                        .midi_hw_out_positions
                        .get(device)
                        .copied()
                        .unwrap_or(Point::new(default_rect.x, default_rect.y));
                    let selected_id = format!("{MIDI_HW_OUT_ID}:{device}");
                    let is_selected = data
                        .moving_track
                        .as_ref()
                        .is_some_and(|mt| mt.track_idx == selected_id);
                    let rect = Path::rectangle(
                        pos,
                        iced::Size::new(default_rect.width, default_rect.height),
                    );
                    let fill_color = if is_selected {
                        midi_box_selected_fill
                    } else {
                        midi_box_fill
                    };
                    let stroke_color = if is_selected {
                        midi_port_color()
                    } else {
                        midi_box_border
                    };
                    draw_gradient_box(
                        &mut frame,
                        pos,
                        iced::Size::new(default_rect.width, default_rect.height),
                        fill_color,
                    );
                    frame.stroke(
                        &rect,
                        canvas::Stroke::default()
                            .with_color(stroke_color)
                            .with_width(2.0),
                    );
                    frame.fill_text(Text {
                        content: label,
                        position: Point::new(
                            pos.x + default_rect.width / 2.0,
                            pos.y + default_rect.height / 2.0,
                        ),
                        color: Color::WHITE,
                        size: 11.0.into(),
                        align_x: Horizontal::Center.into(),
                        align_y: Vertical::Center,
                        ..Default::default()
                    });
                    frame.fill(
                        &Path::circle(
                            Point::new(pos.x, pos.y + default_rect.height / 2.0),
                            hover_radius(
                                5.0,
                                should_highlight_port(
                                    data.hovering
                                        == Some(Hovering::Port {
                                            track_idx: selected_id.clone(),
                                            port_idx: 0,
                                            is_input: true,
                                        }),
                                    data.connecting.as_ref().map(|c| c.kind),
                                    Kind::MIDI,
                                ),
                            ),
                        ),
                        midi_port_color(),
                    );
                }
            }

            for track in data.tracks.iter() {
                if !visible_names.contains(&track.name) {
                    continue;
                }
                let pos = track.position;
                let size = Self::track_box_size(track);
                let path = Path::rectangle(pos, size);
                draw_true_gradient_box(&mut frame, pos, size, track_node_fill);

                let is_h = data.hovering == Some(Hovering::Track(track.name.clone()));
                let is_s = matches!(&data.connection_view_selection, ConnectionViewSelection::Tracks(set) if set.contains(&track.name));
                let (sc, sw) = if is_s {
                    (node_selected, 2.5)
                } else if is_h {
                    (node_hover, 1.4)
                } else {
                    (node_border, 1.0)
                };
                frame.stroke(
                    &path,
                    canvas::Stroke::default().with_color(sc).with_width(sw),
                );

                let total_ins = track.primary_audio_ins() + track.midi.ins + track.return_count();
                for j in 0..total_ins {
                    let point = Self::track_port_position(track, j, pos, size);
                    let c = Self::track_port_color(track, j, true);
                    let h_port = Hovering::Port {
                        track_idx: track.name.clone(),
                        port_idx: j,
                        is_input: true,
                    };
                    let h = data.hovering == Some(h_port.clone());

                    let can_highlight_port = should_highlight_port(
                        h,
                        data.connecting.as_ref().map(|c| c.kind),
                        Self::get_port_kind(&data, &h_port).unwrap_or(Kind::Audio),
                    );

                    frame.fill(
                        &Path::circle(point, hover_radius(4.0, can_highlight_port)),
                        c,
                    );
                }

                let total_outs = track.primary_audio_outs() + track.midi.outs + track.send_count();
                for j in 0..total_outs {
                    let point = Self::track_output_port_position(track, j, pos, size);
                    let c = Self::track_port_color(track, j, false);
                    let h_port = Hovering::Port {
                        track_idx: track.name.clone(),
                        port_idx: j,
                        is_input: false,
                    };
                    let h = data.hovering == Some(h_port.clone());

                    let can_highlight_port = should_highlight_port(
                        h,
                        data.connecting.as_ref().map(|c| c.kind),
                        Self::get_port_kind(&data, &h_port).unwrap_or(Kind::Audio),
                    );

                    frame.fill(
                        &Path::circle(point, hover_radius(4.0, can_highlight_port)),
                        c,
                    );
                }

                frame.fill_text(Text {
                    content: Self::trim_label_to_width(&track.name, size.width),
                    position: Point::new(pos.x + size.width / 2.0, pos.y + size.height / 2.0 - 8.0),
                    color: Color::WHITE,
                    size: 14.0.into(),
                    align_x: Horizontal::Center.into(),
                    align_y: Vertical::Center,
                    ..Default::default()
                });
            }

            // Draw folder plugin nodes as equals in the same graph.
            if data.connections_folder.is_some() {
                for (idx, plugin) in data.plugin_graph_plugins.iter().enumerate() {
                    let pos = Self::plugin_node_position(&data, plugin, idx, bounds);
                    let size = Self::plugin_box_size(plugin);
                    let path = Path::rectangle(pos, size);
                    draw_true_gradient_box(&mut frame, pos, size, track_node_fill);

                    let is_h = data.hovering
                        == Some(Hovering::Plugin {
                            instance_id: plugin.instance_id,
                        });
                    let is_s = data
                        .plugin_graph_selected_plugins
                        .contains(&plugin.instance_id);
                    let (sc, sw) = if is_s {
                        (node_selected, 2.5)
                    } else if is_h {
                        (node_hover, 1.4)
                    } else {
                        (node_border, 1.0)
                    };
                    frame.stroke(
                        &path,
                        canvas::Stroke::default().with_color(sc).with_width(sw),
                    );

                    let total_ins = plugin.audio_inputs + plugin.midi_inputs;
                    for j in 0..total_ins {
                        let Some(point) = Self::plugin_port_position(plugin, pos, j, true) else {
                            continue;
                        };
                        let c = Self::plugin_port_color(plugin, j, true);
                        let h_port = Hovering::PluginPort {
                            instance_id: plugin.instance_id,
                            port_idx: j,
                            is_input: true,
                        };
                        let h = data.hovering == Some(h_port.clone());

                        let can_highlight_port = should_highlight_port(
                            h,
                            data.plugin_graph_connecting
                                .as_ref()
                                .map(|c| c.kind)
                                .or(data.connecting.as_ref().map(|c| c.kind)),
                            Self::plugin_port_kind(plugin, j, true),
                        );

                        frame.fill(
                            &Path::circle(point, hover_radius(4.0, can_highlight_port)),
                            c,
                        );
                    }

                    let total_outs = plugin.audio_outputs + plugin.midi_outputs;
                    for j in 0..total_outs {
                        let Some(point) = Self::plugin_port_position(plugin, pos, j, false) else {
                            continue;
                        };
                        let c = Self::plugin_port_color(plugin, j, false);
                        let h_port = Hovering::PluginPort {
                            instance_id: plugin.instance_id,
                            port_idx: j,
                            is_input: false,
                        };
                        let h = data.hovering == Some(h_port.clone());

                        let can_highlight_port = should_highlight_port(
                            h,
                            data.plugin_graph_connecting
                                .as_ref()
                                .map(|c| c.kind)
                                .or(data.connecting.as_ref().map(|c| c.kind)),
                            Self::plugin_port_kind(plugin, j, false),
                        );

                        frame.fill(
                            &Path::circle(point, hover_radius(4.0, can_highlight_port)),
                            c,
                        );
                    }

                    frame.fill_text(Text {
                        content: Self::trim_label_to_width(
                            &format!("{} ({})", plugin.name, plugin.format),
                            size.width,
                        ),
                        position: Point::new(
                            pos.x + size.width / 2.0,
                            pos.y + size.height / 2.0 - 8.0,
                        ),
                        color: Color::WHITE,
                        size: 12.0.into(),
                        align_x: Horizontal::Center.into(),
                        align_y: Vertical::Center,
                        ..Default::default()
                    });
                }
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
    fn midi_box_width_clamps_to_range() {
        assert_eq!(Graph::midi_box_width("A"), 90.0_f32.min(360.0));

        let long_label = "a".repeat(100);
        assert_eq!(Graph::midi_box_width(&long_label), 360.0);
    }

    #[test]
    fn midi_box_width_calculates_correctly() {
        let w1 = Graph::midi_box_width("Hello");
        assert!((90.0..=360.0).contains(&w1));

        let w2 = Graph::midi_box_width("012345678901234567890123456789");
        assert!(w2 >= w1);
    }

    #[test]
    fn trim_label_to_width_truncates_correctly() {
        let trimmed = Graph::trim_label_to_width("Hello World Test", 100.0);
        assert!(trimmed.len() <= 13);

        assert_eq!(Graph::trim_label_to_width("Short", 500.0), "Short");

        assert_eq!(Graph::trim_label_to_width("Test", 5.0), "");
    }

    #[test]
    fn trim_label_to_width_handles_edge_cases() {
        assert_eq!(Graph::trim_label_to_width("", 100.0), "");
        assert_eq!(Graph::trim_label_to_width("X", 0.0), "");
    }

    #[test]
    fn track_port_kind_for_inputs() {
        let track = crate::state::Track::new("test".to_string(), 0.0, 2, 1, 2, 1);
        let primary = track.primary_audio_ins();

        if primary > 0 {
            assert_eq!(Graph::track_port_kind(&track, 0, true), Kind::Audio);
        }

        if track.midi.ins > 0 && primary < usize::MAX {
            let midi_port = primary;
            assert_eq!(Graph::track_port_kind(&track, midi_port, true), Kind::MIDI);
        }
    }

    #[test]
    fn track_port_kind_for_outputs() {
        let track = crate::state::Track::new("test".to_string(), 0.0, 2, 1, 2, 1);
        let primary = track.primary_audio_outs();

        if primary > 0 {
            assert_eq!(Graph::track_port_kind(&track, 0, false), Kind::Audio);
        }
    }

    #[test]
    fn connection_port_index_for_midi() {
        let track = crate::state::Track::new("test".to_string(), 0.0, 2, 1, 2, 1);

        let midi_in_flat = Graph::connection_port_index(&track, Kind::MIDI, 0, true);
        assert!(midi_in_flat >= track.primary_audio_ins());

        let midi_out_flat = Graph::connection_port_index(&track, Kind::MIDI, 0, false);
        assert!(midi_out_flat >= track.primary_audio_outs());
    }

    #[test]
    fn connection_port_index_for_audio() {
        let track = crate::state::Track::new("test".to_string(), 0.0, 2, 0, 2, 0);

        assert_eq!(
            Graph::connection_port_index(&track, Kind::Audio, 0, true),
            0
        );
        assert_eq!(
            Graph::connection_port_index(&track, Kind::Audio, 1, true),
            1
        );
    }

    #[test]
    fn track_box_size_is_square() {
        let track_few = crate::state::Track::new("few".to_string(), 0.0, 1, 0, 1, 0);
        let size_few = Graph::track_box_size(&track_few);
        assert_eq!(size_few.width, Graph::TRACK_NODE_SIZE);
        assert_eq!(size_few.height, Graph::TRACK_NODE_SIZE);

        let track_many = crate::state::Track::new("many".to_string(), 0.0, 8, 2, 8, 2);
        let size_many = Graph::track_box_size(&track_many);
        assert_eq!(size_many.width, Graph::TRACK_NODE_SIZE);
        assert_eq!(size_many.height, Graph::TRACK_NODE_SIZE);
    }

    #[test]
    fn track_port_to_engine_index_audio() {
        let track = crate::state::Track::new("test".to_string(), 0.0, 2, 0, 2, 0);
        let (kind, idx) = Graph::track_port_to_engine_index(&track, 0, true);
        assert_eq!(kind, Kind::Audio);
        assert_eq!(idx, 0);
    }

    #[test]
    fn track_port_to_engine_index_midi() {
        let track = crate::state::Track::new("test".to_string(), 0.0, 2, 1, 2, 1);
        let (kind, idx) = Graph::track_port_to_engine_index(&track, 2, true);
        assert_eq!(kind, Kind::MIDI);
        assert_eq!(idx, 0);
    }

    #[test]
    fn midi_device_label_uses_cached_label() {
        let mut data = crate::state::StateData::default();
        data.midi_hw_labels
            .insert("/dev/midi0".to_string(), "MIDI Keyboard".to_string());

        assert_eq!(
            Graph::midi_device_label(&data, "/dev/midi0"),
            "MIDI Keyboard"
        );
    }

    #[test]
    fn midi_device_label_fallback_to_basename() {
        let data = crate::state::StateData::default();
        assert_eq!(Graph::midi_device_label(&data, "/dev/midi0"), "midi0");
    }

    #[test]
    fn update_clicking_track_body_selects_and_moves_track() {
        let state = Arc::new(RwLock::new(crate::state::StateData::default()));
        let track = crate::state::Track::new("Track".to_string(), 0.0, 1, 1, 0, 0);
        let click = Point::new(track.position.x + 5.0, track.position.y + 5.0);
        state.blocking_write().tracks.push(track);
        let graph = Graph::new(state.clone());
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(800.0, 600.0));
        let cursor = mouse::Cursor::Available(click);

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
        assert_eq!(
            data.moving_track
                .as_ref()
                .map(|moving| moving.track_idx.as_str()),
            Some("Track")
        );
        match &data.connection_view_selection {
            ConnectionViewSelection::Tracks(selected) => assert!(selected.contains("Track")),
            other => panic!("unexpected selection: {other:?}"),
        }
        assert!(data.selected.contains("Track"));
    }

    #[test]
    fn update_double_clicking_hw_in_opens_hw_input_ports_view() {
        let state = Arc::new(RwLock::new(crate::state::StateData::default()));
        state.blocking_write().hw_in = Some(crate::state::HW { channels: 1 });
        let graph = Graph::new(state.clone());
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(800.0, 600.0));
        let cursor = mouse::Cursor::Available(Point::new(20.0, 20.0));

        let first = graph
            .update(
                &mut (),
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                cursor,
            )
            .expect("first action");
        let (first_message, first_status) = action_message(first);
        assert!(first_message.is_none());
        assert_eq!(first_status, event::Status::Captured);

        let second = graph
            .update(
                &mut (),
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                cursor,
            )
            .expect("second action");
        let (second_message, second_status) = action_message(second);
        assert!(matches!(
            second_message,
            Some(Message::OpenHwPorts { input: true })
        ));
        assert_eq!(second_status, event::Status::Ignored);
    }

    #[test]
    fn update_double_clicking_hw_out_opens_hw_output_ports_view() {
        let state = Arc::new(RwLock::new(crate::state::StateData::default()));
        state.blocking_write().hw_out = Some(crate::state::HW { channels: 1 });
        let graph = Graph::new(state.clone());
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(800.0, 600.0));
        let cursor = mouse::Cursor::Available(Point::new(780.0, 20.0));

        let first = graph
            .update(
                &mut (),
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                cursor,
            )
            .expect("first action");
        let (first_message, first_status) = action_message(first);
        assert!(first_message.is_none());
        assert_eq!(first_status, event::Status::Captured);

        let second = graph
            .update(
                &mut (),
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                cursor,
            )
            .expect("second action");
        let (second_message, second_status) = action_message(second);
        assert!(matches!(
            second_message,
            Some(Message::OpenHwPorts { input: false })
        ));
        assert_eq!(second_status, event::Status::Ignored);
    }

    fn test_plugin_graph_plugin(instance_id: usize) -> PluginGraphPlugin {
        PluginGraphPlugin {
            node: PluginGraphNode::Vst3PluginInstance(instance_id),
            instance_id,
            format: "vst3".into(),
            uri: "/test/plugin.vst3".into(),
            plugin_id: "test".into(),
            name: "Test Plugin".into(),
            main_audio_inputs: 2,
            main_audio_outputs: 2,
            audio_inputs: 2,
            audio_outputs: 2,
            midi_inputs: 0,
            midi_outputs: 0,
            state: None,
            bypassed: false,
        }
    }

    #[test]
    fn update_clicking_plugin_port_starts_plugin_connecting() {
        let state = Arc::new(RwLock::new(crate::state::StateData::default()));
        {
            let mut data = state.blocking_write();
            let folder = crate::state::Track::new("Folder".to_string(), 0.0, 2, 0, 2, 0);
            data.tracks.push(folder);
            data.connections_folder = Some("Folder".to_string());
            data.plugin_graph_track = Some("Folder".to_string());
            data.plugin_graph_plugins.push(test_plugin_graph_plugin(7));
        }
        let graph = Graph::new(state.clone());
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(800.0, 600.0));
        let port_pos = {
            let data = state.blocking_read();
            let plugin = &data.plugin_graph_plugins[0];
            let pos = Graph::plugin_node_position(&data, plugin, 0, bounds);
            Graph::plugin_port_position(plugin, pos, 0, true).unwrap()
        };
        let cursor = mouse::Cursor::Available(port_pos);

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
        let conn = data.plugin_graph_connecting.as_ref().expect("connecting");
        assert_eq!(conn.from_node, PluginGraphNode::Vst3PluginInstance(7));
        assert_eq!(conn.from_port, 0);
        assert!(conn.is_input);
        assert_eq!(conn.kind, Kind::Audio);
    }

    #[test]
    fn update_clicking_plugin_body_selects_and_moves_plugin() {
        let state = Arc::new(RwLock::new(crate::state::StateData::default()));
        {
            let mut data = state.blocking_write();
            let folder = crate::state::Track::new("Folder".to_string(), 0.0, 2, 0, 2, 0);
            data.tracks.push(folder);
            data.connections_folder = Some("Folder".to_string());
            data.plugin_graph_track = Some("Folder".to_string());
            data.plugin_graph_plugins.push(test_plugin_graph_plugin(7));
        }
        let graph = Graph::new(state.clone());
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(800.0, 600.0));
        let click = {
            let data = state.blocking_read();
            let plugin = &data.plugin_graph_plugins[0];
            let pos = Graph::plugin_node_position(&data, plugin, 0, bounds);
            Point::new(pos.x + 10.0, pos.y + 20.0)
        };
        let cursor = mouse::Cursor::Available(click);

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
        assert!(data.plugin_graph_selected_plugins.contains(&7));
        assert_eq!(
            data.plugin_graph_moving_plugin
                .as_ref()
                .map(|moving| moving.instance_id),
            Some(7)
        );
    }

    #[test]
    fn plugin_graph_connection_actions_batches_shift_plugin_to_track_output() {
        let mut data = crate::state::StateData::default();
        let folder = crate::state::Track::new("Folder".to_string(), 0.0, 2, 2, 0, 0);
        data.tracks.push(folder);
        data.plugin_graph_track = Some("Folder".to_string());
        data.connections_folder = Some("Folder".to_string());
        data.shift = true;
        data.plugin_graph_plugins.push(test_plugin_graph_plugin(7));

        let action = Graph::plugin_graph_connection_actions(
            &data,
            PluginGraphNode::Vst3PluginInstance(7),
            0,
            PluginGraphNode::TrackOutput,
            0,
            Kind::Audio,
        )
        .expect("action");

        let message = action_message(action).0.expect("message");
        match message {
            Message::RequestBatch(actions) => {
                assert_eq!(actions.len(), 2);
                assert!(
                    actions
                        .iter()
                        .all(|a| matches!(a, EngineAction::TrackConnectPluginAudio { .. }))
                );
            }
            other => panic!("expected RequestBatch, got {other:?}"),
        }
    }

    #[test]
    fn connectable_connection_actions_batches_shift_plugin_to_track_output() {
        let mut data = crate::state::StateData::default();
        let folder = crate::state::Track::new("Folder".to_string(), 0.0, 2, 2, 0, 0);
        data.tracks.push(folder);
        data.plugin_graph_track = Some("Folder".to_string());
        data.connections_folder = Some("Folder".to_string());
        data.shift = true;
        data.plugin_graph_plugins.push(test_plugin_graph_plugin(7));

        let action = Graph::connectable_connection_actions(
            &data,
            ConnectableRef::Vst3Plugin(7),
            0,
            ConnectableRef::TrackOutput,
            0,
            Kind::Audio,
        )
        .expect("action");

        let message = action_message(action).0.expect("message");
        match message {
            Message::RequestBatch(actions) => {
                assert_eq!(actions.len(), 2);
                assert!(
                    actions
                        .iter()
                        .all(|a| matches!(a, EngineAction::TrackConnectAudio { .. }))
                );
            }
            other => panic!("expected RequestBatch, got {other:?}"),
        }
    }

    #[test]
    fn shift_plugin_connection_ignores_sidechain_outputs() {
        let mut data = crate::state::StateData::default();
        let folder = crate::state::Track::new("Folder".to_string(), 0.0, 4, 4, 0, 0);
        data.tracks.push(folder);
        data.plugin_graph_track = Some("Folder".to_string());
        data.connections_folder = Some("Folder".to_string());
        data.shift = true;
        let mut plugin = test_plugin_graph_plugin(8);
        plugin.main_audio_outputs = 2;
        plugin.audio_outputs = 4;
        data.plugin_graph_plugins.push(plugin);

        let action = Graph::plugin_graph_connection_actions(
            &data,
            PluginGraphNode::Vst3PluginInstance(8),
            0,
            PluginGraphNode::TrackOutput,
            0,
            Kind::Audio,
        )
        .expect("action");

        let message = action_message(action).0.expect("message");
        match message {
            Message::RequestBatch(actions) => assert_eq!(actions.len(), 2),
            other => panic!("expected RequestBatch, got {other:?}"),
        }
    }
}
