use crate::{
    connections::colors::{audio_port_color, midi_port_color},
    connections::port_kind::{can_connect_kinds, should_highlight_port},
    connections::ports::hover_radius,
    connections::selection::is_bezier_hit,
    message::Message,
    state::{Lv2Connecting, MovingPlugin, State},
};
use iced::{
    Color, Point, Rectangle, Renderer, Theme,
    alignment::{Horizontal, Vertical},
    event::Event,
    mouse,
    widget::{
        canvas,
        canvas::{Action, Frame, Geometry, Path, Text},
    },
};
use maolan_engine::message::{Action as EngineAction, Lv2GraphNode, Lv2GraphPlugin};
use maolan_engine::kind::Kind;
use std::time::{Duration, Instant};

const PLUGIN_W: f32 = 170.0;
const MIN_PLUGIN_H: f32 = 96.0;
const AUDIO_PORT_RADIUS: f32 = 4.5;
const MIDI_PORT_RADIUS: f32 = 3.5;
const MIN_PORT_GAP: f32 = 2.0;
const PORT_HIT_RADIUS: f32 = AUDIO_PORT_RADIUS + 2.0;
const MIDI_HIT_RADIUS: f32 = MIDI_PORT_RADIUS + 2.0;
const TRACK_IO_W: f32 = 86.0;
const TRACK_IO_H: f32 = 200.0;
const TRACK_IO_MARGIN_X: f32 = 24.0;

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

    fn plugin_height(plugin: &Lv2GraphPlugin) -> f32 {
        MIN_PLUGIN_H
            .max(Self::required_height_for_ports(
                plugin.audio_inputs,
                AUDIO_PORT_RADIUS,
            ))
            .max(Self::required_height_for_ports(
                plugin.audio_outputs,
                AUDIO_PORT_RADIUS,
            ))
            .max(Self::required_height_for_ports(plugin.midi_inputs, MIDI_PORT_RADIUS))
            .max(Self::required_height_for_ports(plugin.midi_outputs, MIDI_PORT_RADIUS))
    }

    fn plugin_pos(
        data: &crate::state::StateData,
        plugin: &Lv2GraphPlugin,
        idx: usize,
        bounds: Rectangle,
    ) -> Point {
        data.lv2_graph_plugin_positions
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

    fn track_input_port_y(track: &crate::state::Track, rect: Rectangle, kind: Kind, port: usize) -> Option<f32> {
        Self::edge_port_y(rect.y, rect.height, track.audio.ins, track.midi.ins, kind, port)
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

    fn plugin_input_port_y(plugin: &Lv2GraphPlugin, plugin_h: f32, y: f32, kind: Kind, port: usize) -> Option<f32> {
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
        plugin: &Lv2GraphPlugin,
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
}

#[derive(Clone)]
struct PortHit {
    node: Lv2GraphNode,
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
            if data.lv2_graph_track.is_none() {
                return None;
            }

            let mut audio_ports: Vec<PortHit> = vec![];
            let mut midi_ports: Vec<PortHit> = vec![];
            let in_rect = Self::track_input_rect(bounds);
            let out_rect = Self::track_output_rect(bounds);
            if let Some(track) = data
                .tracks
                .iter()
                .find(|t| Some(&t.name) == data.lv2_graph_track.as_ref())
            {
                for port in 0..track.audio.ins {
                    if let Some(py) = Self::track_input_port_y(track, in_rect, Kind::Audio, port) {
                        audio_ports.push(PortHit {
                            node: Lv2GraphNode::TrackInput,
                            port,
                            is_input: false,
                            kind: Kind::Audio,
                            pos: Point::new(in_rect.x + in_rect.width, py),
                        });
                    }
                }
                for port in 0..track.audio.outs {
                    if let Some(py) = Self::track_output_port_y(track, out_rect, Kind::Audio, port) {
                        audio_ports.push(PortHit {
                            node: Lv2GraphNode::TrackOutput,
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
                            node: Lv2GraphNode::TrackInput,
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
                            node: Lv2GraphNode::TrackOutput,
                            port,
                            is_input: true,
                            kind: Kind::MIDI,
                            pos: Point::new(out_rect.x, py),
                        });
                    }
                }
            }
            for (idx, plugin) in data.lv2_graph_plugins.iter().enumerate() {
                let pos = Self::plugin_pos(&data, plugin, idx, bounds);
                let plugin_h = Self::plugin_height(plugin);
                for port in 0..plugin.audio_inputs {
                    if let Some(py) =
                        Self::plugin_input_port_y(plugin, plugin_h, pos.y, Kind::Audio, port)
                    {
                        audio_ports.push(PortHit {
                            node: Lv2GraphNode::PluginInstance(plugin.instance_id),
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
                            node: Lv2GraphNode::PluginInstance(plugin.instance_id),
                            port,
                            is_input: false,
                            kind: Kind::Audio,
                            pos: Point::new(pos.x + PLUGIN_W, py),
                        });
                    }
                }
            }
            for (idx, plugin) in data.lv2_graph_plugins.iter().enumerate() {
                let pos = Self::plugin_pos(&data, plugin, idx, bounds);
                let plugin_h = Self::plugin_height(plugin);
                for port in 0..plugin.midi_inputs {
                    if let Some(py) =
                        Self::plugin_input_port_y(plugin, plugin_h, pos.y, Kind::MIDI, port)
                    {
                        midi_ports.push(PortHit {
                            node: Lv2GraphNode::PluginInstance(plugin.instance_id),
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
                            node: Lv2GraphNode::PluginInstance(plugin.instance_id),
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
                    let port_hit = Self::closest_port(&audio_ports, cursor_position, PORT_HIT_RADIUS)
                        .cloned()
                        .or_else(|| {
                            Self::closest_port(&midi_ports, cursor_position, MIDI_HIT_RADIUS).cloned()
                        });
                    if let Some(hit) = port_hit {
                        data.lv2_graph_connecting = Some(Lv2Connecting {
                            from_node: hit.node,
                            from_port: hit.port,
                            kind: hit.kind,
                            point: cursor_position,
                            is_input: hit.is_input,
                        });
                        return Some(Action::capture());
                    }

                    let mut clicked_connection = None;
                    for (idx, conn) in data.lv2_graph_connections.iter().enumerate() {
                        let start = match &conn.from_node {
                            Lv2GraphNode::TrackInput => {
                                let Some(track) = data
                                    .tracks
                                    .iter()
                                    .find(|t| Some(&t.name) == data.lv2_graph_track.as_ref())
                                else {
                                    continue;
                                };
                                let Some(py) = Self::track_input_port_y(track, in_rect, conn.kind, conn.from_port)
                                else {
                                    continue;
                                };
                                Point::new(in_rect.x + in_rect.width, py)
                            }
                            Lv2GraphNode::PluginInstance(id) => {
                                let Some((pidx, plugin)) = data
                                    .lv2_graph_plugins
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
                            Lv2GraphNode::TrackOutput => continue,
                        };
                        let end = match &conn.to_node {
                            Lv2GraphNode::TrackOutput => {
                                let Some(track) = data
                                    .tracks
                                    .iter()
                                    .find(|t| Some(&t.name) == data.lv2_graph_track.as_ref())
                                else {
                                    continue;
                                };
                                let Some(py) = Self::track_output_port_y(track, out_rect, conn.kind, conn.to_port)
                                else {
                                    continue;
                                };
                                Point::new(out_rect.x, py)
                            }
                            Lv2GraphNode::PluginInstance(id) => {
                                let Some((pidx, plugin)) = data
                                    .lv2_graph_plugins
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
                            Lv2GraphNode::TrackInput => continue,
                        };

                        if is_bezier_hit(start, end, cursor_position, 100, 12.0) {
                            clicked_connection = Some(idx);
                            break;
                        }
                    }
                    if let Some(idx) = clicked_connection {
                        let ctrl = data.ctrl;
                        crate::connections::selection::select_connection_indices(
                            &mut data.lv2_graph_selected_connections,
                            idx,
                            ctrl,
                        );
                        return Some(Action::request_redraw());
                    }

                    for (idx, plugin) in data.lv2_graph_plugins.iter().enumerate().rev() {
                        let instance_id = plugin.instance_id;
                        let pos = Self::plugin_pos(&data, plugin, idx, bounds);
                        let rect = Rectangle::new(
                            pos,
                            iced::Size::new(PLUGIN_W, Self::plugin_height(plugin)),
                        );
                        if rect.contains(cursor_position) {
                            let now = Instant::now();
                            if let Some((last_instance, last_time)) = data.lv2_graph_last_plugin_click
                                && last_instance == instance_id
                                && now.duration_since(last_time) <= Duration::from_millis(350)
                            {
                                data.lv2_graph_last_plugin_click = None;
                                if let Some(track_name) = data.lv2_graph_track.clone() {
                                    return Some(Action::publish(Message::Request(
                                        EngineAction::TrackShowLv2PluginUiInstance {
                                            track_name,
                                            instance_id,
                                        },
                                    )));
                                }
                                return Some(Action::capture());
                            }
                            data.lv2_graph_last_plugin_click = Some((instance_id, now));

                            data.lv2_graph_moving_plugin = Some(MovingPlugin {
                                instance_id,
                                offset_x: cursor_position.x - pos.x,
                                offset_y: cursor_position.y - pos.y,
                            });
                            return Some(Action::capture());
                        }
                    }

                    data.lv2_graph_selected_connections.clear();
                    return Some(Action::request_redraw());
                }
                Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                    let mut redraw = true;
                    if let Some(connecting) = data.lv2_graph_connecting.as_mut() {
                        connecting.point = cursor_position;
                        redraw = true;
                    }
                    if let Some(moving) = data.lv2_graph_moving_plugin.clone() {
                        data.lv2_graph_plugin_positions.insert(
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
                    data.lv2_graph_moving_plugin = None;
                    if let Some(connecting) = data.lv2_graph_connecting.take() {
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
                            let Some(track_name) = data.lv2_graph_track.clone() else {
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
                                let action = if connecting.kind == Kind::Audio {
                                    EngineAction::TrackConnectLv2Audio {
                                        track_name,
                                        from_node,
                                        from_port,
                                        to_node,
                                        to_port,
                                    }
                                } else {
                                    EngineAction::TrackConnectLv2Midi {
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
        if let Ok(data) = self.state.try_read() {
            let Some(track_name) = data.lv2_graph_track.as_ref() else {
                return vec![frame.into_geometry()];
            };
            let Some(track) = data.tracks.iter().find(|t| &t.name == track_name) else {
                return vec![frame.into_geometry()];
            };
            let active_connecting = data.lv2_graph_connecting.as_ref();

            let in_rect = Self::track_input_rect(bounds);
            let out_rect = Self::track_output_rect(bounds);
            let left_box = Path::rectangle(in_rect.position(), in_rect.size());
            frame.fill(&left_box, Color::from_rgb8(30, 45, 30));
            frame.stroke(
                &left_box,
                canvas::Stroke::default()
                    .with_color(Color::from_rgb(0.2, 0.85, 0.45))
                    .with_width(2.0),
            );
            frame.fill_text(Text {
                content: "Track In".into(),
                position: Point::new(in_rect.center_x(), in_rect.y + 18.0),
                color: Color::WHITE,
                align_x: Horizontal::Center.into(),
                align_y: Vertical::Center,
                ..Default::default()
            });

            let right_box = Path::rectangle(out_rect.position(), out_rect.size());
            frame.fill(&right_box, Color::from_rgb8(45, 30, 30));
            frame.stroke(
                &right_box,
                canvas::Stroke::default()
                    .with_color(Color::from_rgb(0.85, 0.25, 0.25))
                    .with_width(2.0),
            );
            frame.fill_text(Text {
                content: "Track Out".into(),
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
                let kind_ok =
                    should_highlight_port(is_hovered, active_connecting.map(|c| c.kind), Kind::Audio);
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
                    audio_port_color(),
                );
            }
            for port in 0..track.midi.ins {
                let Some(py) = Self::track_input_port_y(track, in_rect, Kind::MIDI, port) else {
                    continue;
                };
                let is_hovered = cursor_position
                    .is_some_and(|cursor| cursor.distance(Point::new(in_rect.x + in_rect.width, py)) <= MIDI_HIT_RADIUS);
                let kind_ok =
                    should_highlight_port(is_hovered, active_connecting.map(|c| c.kind), Kind::MIDI);
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
                let is_hovered = cursor_position
                    .is_some_and(|cursor| cursor.distance(Point::new(out_rect.x, py)) <= PORT_HIT_RADIUS);
                let kind_ok =
                    should_highlight_port(is_hovered, active_connecting.map(|c| c.kind), Kind::Audio);
                let can_highlight = if let Some(connecting) = active_connecting {
                    kind_ok && !connecting.is_input
                } else {
                    kind_ok
                };
                frame.fill(
                    &Path::circle(Point::new(out_rect.x, py), hover_radius(5.0, can_highlight)),
                    audio_port_color(),
                );
            }
            for port in 0..track.midi.outs {
                let Some(py) = Self::track_output_port_y(track, out_rect, Kind::MIDI, port) else {
                    continue;
                };
                let is_hovered = cursor_position.is_some_and(|cursor| {
                    cursor.distance(Point::new(out_rect.x, py)) <= MIDI_HIT_RADIUS
                });
                let kind_ok =
                    should_highlight_port(is_hovered, active_connecting.map(|c| c.kind), Kind::MIDI);
                let can_highlight = if let Some(connecting) = active_connecting {
                    kind_ok && !connecting.is_input
                } else {
                    kind_ok
                };
                frame.fill(
                    &Path::circle(
                        Point::new(out_rect.x, py),
                        hover_radius(4.0, can_highlight),
                    ),
                    midi_port_color(),
                );
            }

            for (idx, plugin) in data.lv2_graph_plugins.iter().enumerate() {
                let pos = Self::plugin_pos(&data, plugin, idx, bounds);
                let plugin_h = Self::plugin_height(plugin);
                let rect = Path::rectangle(pos, iced::Size::new(PLUGIN_W, plugin_h));
                frame.fill(&rect, Color::from_rgb8(28, 28, 42));
                frame.stroke(
                    &rect,
                    canvas::Stroke::default()
                        .with_color(Color::from_rgb(0.55, 0.55, 0.85))
                        .with_width(2.0),
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
                    let is_hovered = cursor_position
                        .is_some_and(|cursor| cursor.distance(Point::new(pos.x, py)) <= PORT_HIT_RADIUS);
                    let kind_ok =
                        should_highlight_port(is_hovered, active_connecting.map(|c| c.kind), Kind::Audio);
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
                        audio_port_color(),
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
                    let kind_ok =
                        should_highlight_port(is_hovered, active_connecting.map(|c| c.kind), Kind::Audio);
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
                        audio_port_color(),
                    );
                }
                for port in 0..plugin.midi_inputs {
                    let Some(py) =
                        Self::plugin_input_port_y(plugin, plugin_h, pos.y, Kind::MIDI, port)
                    else {
                        continue;
                    };
                    let is_hovered = cursor_position
                        .is_some_and(|cursor| cursor.distance(Point::new(pos.x, py)) <= MIDI_HIT_RADIUS);
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

            for (conn_idx, conn) in data.lv2_graph_connections.iter().enumerate() {
                let start = match &conn.from_node {
                    Lv2GraphNode::TrackInput => {
                        let Some(py) =
                            Self::track_input_port_y(track, in_rect, conn.kind, conn.from_port)
                        else {
                            continue;
                        };
                        Point::new(in_rect.x + in_rect.width, py)
                    }
                    Lv2GraphNode::PluginInstance(id) => {
                        let Some((idx, plugin)) = data
                            .lv2_graph_plugins
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
                    Lv2GraphNode::TrackOutput => continue,
                };
                let end = match &conn.to_node {
                    Lv2GraphNode::TrackOutput => {
                        let Some(py) =
                            Self::track_output_port_y(track, out_rect, conn.kind, conn.to_port)
                        else {
                            continue;
                        };
                        Point::new(out_rect.x, py)
                    }
                    Lv2GraphNode::PluginInstance(id) => {
                        let Some((idx, plugin)) = data
                            .lv2_graph_plugins
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
                    Lv2GraphNode::TrackInput => continue,
                };
                let dist = (end.x - start.x).abs() / 2.0;
                let is_hovered = cursor_position
                    .is_some_and(|cursor| is_bezier_hit(start, end, cursor, 100, 12.0));
                let is_selected = data.lv2_graph_selected_connections.contains(&conn_idx);
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
                            Color::from_rgb(1.0, 1.0, 0.0)
                        } else if conn.kind == Kind::MIDI {
                            midi_port_color()
                        } else {
                            Color::from_rgb(0.2, 0.5, 1.0)
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

            if let Some(connecting) = &data.lv2_graph_connecting {
                let start = match &connecting.from_node {
                    Lv2GraphNode::TrackInput => {
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
                    Lv2GraphNode::TrackOutput => {
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
                    Lv2GraphNode::PluginInstance(id) => {
                        let Some((idx, plugin)) = data
                            .lv2_graph_plugins
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
                        .with_color(if connecting.kind == Kind::Audio {
                            Color::from_rgba(1.0, 1.0, 1.0, 0.6)
                        } else {
                            Color::from_rgba(
                                midi_port_color().r,
                                midi_port_color().g,
                                midi_port_color().b,
                                0.7,
                            )
                        })
                        .with_width(2.0),
                );
            }
        }
        vec![frame.into_geometry()]
    }
}
