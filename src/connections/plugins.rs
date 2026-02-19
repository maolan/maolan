use crate::{
    connections::colors::{audio_port_color, midi_port_color},
    connections::selection::is_bezier_hit,
    message::Message,
    state::{Lv2Connecting, MovingPlugin, State},
};
use iced::{
    Color, Element, Length, Point, Rectangle, Renderer, Theme,
    alignment::{Horizontal, Vertical},
    event::Event,
    mouse,
    widget::{
        canvas,
        canvas::{Action, Frame, Geometry, Path, Text},
        container,
    },
};
use maolan_engine::message::{Action as EngineAction, Lv2GraphNode, Lv2GraphPlugin};
use std::time::{Duration, Instant};

const PLUGIN_W: f32 = 170.0;
const PLUGIN_H: f32 = 96.0;
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
                let start_x = TRACK_IO_MARGIN_X + TRACK_IO_W + 60.0;
                let max_x = (bounds.width - TRACK_IO_MARGIN_X - TRACK_IO_W - PLUGIN_W).max(start_x);
                let x = (start_x + idx as f32 * (PLUGIN_W + 24.0)).min(max_x);
                Point::new(x, bounds.height / 2.0 - PLUGIN_H / 2.0)
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
}

pub struct TrackPlugins {
    graph: Graph,
}

impl TrackPlugins {
    pub fn new(state: State) -> Self {
        Self {
            graph: Graph::new(state),
        }
    }

    pub fn update(&mut self, _message: Message) {}

    pub fn view(&self) -> Element<'_, Message> {
        container(canvas(&self.graph).width(Length::Fill).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

#[derive(Clone)]
struct PortHit {
    node: Lv2GraphNode,
    port: usize,
    is_input: bool,
    pos: Point,
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

            let mut all_ports: Vec<PortHit> = vec![];
            let in_rect = Self::track_input_rect(bounds);
            let out_rect = Self::track_output_rect(bounds);

            for port in 0..data
                .tracks
                .iter()
                .find(|t| Some(&t.name) == data.lv2_graph_track.as_ref())
                .map(|t| t.audio.ins)
                .unwrap_or(0)
            {
                let py = in_rect.y + (in_rect.height / (data.tracks.iter().find(|t| Some(&t.name) == data.lv2_graph_track.as_ref()).map(|t| t.audio.ins).unwrap_or(0) + 1) as f32) * (port + 1) as f32;
                all_ports.push(PortHit {
                    node: Lv2GraphNode::TrackInput,
                    port,
                    is_input: false,
                    pos: Point::new(in_rect.x + in_rect.width, py),
                });
            }
            for port in 0..data
                .tracks
                .iter()
                .find(|t| Some(&t.name) == data.lv2_graph_track.as_ref())
                .map(|t| t.audio.outs)
                .unwrap_or(0)
            {
                let py = out_rect.y + (out_rect.height / (data.tracks.iter().find(|t| Some(&t.name) == data.lv2_graph_track.as_ref()).map(|t| t.audio.outs).unwrap_or(0) + 1) as f32) * (port + 1) as f32;
                all_ports.push(PortHit {
                    node: Lv2GraphNode::TrackOutput,
                    port,
                    is_input: true,
                    pos: Point::new(out_rect.x, py),
                });
            }
            for (idx, plugin) in data.lv2_graph_plugins.iter().enumerate() {
                let pos = Self::plugin_pos(&data, plugin, idx, bounds);
                for port in 0..plugin.audio_inputs {
                    let py = pos.y + (PLUGIN_H / (plugin.audio_inputs + 1) as f32) * (port + 1) as f32;
                    all_ports.push(PortHit {
                        node: Lv2GraphNode::PluginInstance(plugin.instance_id),
                        port,
                        is_input: true,
                        pos: Point::new(pos.x, py),
                    });
                }
                for port in 0..plugin.audio_outputs {
                    let py =
                        pos.y + (PLUGIN_H / (plugin.audio_outputs + 1) as f32) * (port + 1) as f32;
                    all_ports.push(PortHit {
                        node: Lv2GraphNode::PluginInstance(plugin.instance_id),
                        port,
                        is_input: false,
                        pos: Point::new(pos.x + PLUGIN_W, py),
                    });
                }
            }

            match event {
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                    let mut clicked_connection = None;
                    for (idx, conn) in data.lv2_graph_connections.iter().enumerate() {
                        let start = match &conn.from_node {
                            Lv2GraphNode::TrackInput => {
                                let track_ins = data
                                    .tracks
                                    .iter()
                                    .find(|t| Some(&t.name) == data.lv2_graph_track.as_ref())
                                    .map(|t| t.audio.ins)
                                    .unwrap_or(0);
                                if track_ins == 0 {
                                    continue;
                                }
                                let py = in_rect.y
                                    + (in_rect.height / (track_ins + 1) as f32)
                                        * (conn.from_port + 1) as f32;
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
                                if plugin.audio_outputs == 0 {
                                    continue;
                                }
                                let pos = Self::plugin_pos(&data, plugin, pidx, bounds);
                                let py = pos.y
                                    + (PLUGIN_H / (plugin.audio_outputs + 1) as f32)
                                        * (conn.from_port + 1) as f32;
                                Point::new(pos.x + PLUGIN_W, py)
                            }
                            Lv2GraphNode::TrackOutput => continue,
                        };
                        let end = match &conn.to_node {
                            Lv2GraphNode::TrackOutput => {
                                let track_outs = data
                                    .tracks
                                    .iter()
                                    .find(|t| Some(&t.name) == data.lv2_graph_track.as_ref())
                                    .map(|t| t.audio.outs)
                                    .unwrap_or(0);
                                if track_outs == 0 {
                                    continue;
                                }
                                let py = out_rect.y
                                    + (out_rect.height / (track_outs + 1) as f32)
                                        * (conn.to_port + 1) as f32;
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
                                if plugin.audio_inputs == 0 {
                                    continue;
                                }
                                let pos = Self::plugin_pos(&data, plugin, pidx, bounds);
                                let py = pos.y
                                    + (PLUGIN_H / (plugin.audio_inputs + 1) as f32)
                                        * (conn.to_port + 1) as f32;
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

                    if let Some(hit) = all_ports
                        .iter()
                        .find(|p| cursor_position.distance(p.pos) <= 10.0)
                        .cloned()
                    {
                        data.lv2_graph_connecting = Some(Lv2Connecting {
                            from_node: hit.node,
                            from_port: hit.port,
                            point: cursor_position,
                            is_input: hit.is_input,
                        });
                        return Some(Action::capture());
                    }

                    for (idx, plugin) in data.lv2_graph_plugins.iter().enumerate().rev() {
                        let instance_id = plugin.instance_id;
                        let pos = Self::plugin_pos(&data, plugin, idx, bounds);
                        let rect = Rectangle::new(pos, iced::Size::new(PLUGIN_W, PLUGIN_H));
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
                    let mut redraw = false;
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
                        let target = all_ports
                            .iter()
                            .find(|p| {
                                p.is_input != connecting.is_input
                                    && cursor_position.distance(p.pos) <= 10.0
                            })
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
                                return Some(Action::publish(Message::Request(
                                    EngineAction::TrackConnectLv2Audio {
                                        track_name,
                                        from_node,
                                        from_port,
                                        to_node,
                                        to_port,
                                    },
                                )));
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
                let py = in_rect.y + (in_rect.height / (track.audio.ins + 1) as f32) * (port + 1) as f32;
                frame.fill(
                    &Path::circle(Point::new(in_rect.x + in_rect.width, py), 5.0),
                    audio_port_color(),
                );
            }
            for port in 0..track.midi.ins {
                let py = in_rect.y + (in_rect.height / (track.midi.ins + 1) as f32) * (port + 1) as f32;
                frame.fill(
                    &Path::circle(Point::new(in_rect.x + 10.0, py), 4.0),
                    midi_port_color(),
                );
            }
            for port in 0..track.audio.outs {
                let py = out_rect.y + (out_rect.height / (track.audio.outs + 1) as f32) * (port + 1) as f32;
                frame.fill(
                    &Path::circle(Point::new(out_rect.x, py), 5.0),
                    audio_port_color(),
                );
            }
            for port in 0..track.midi.outs {
                let py =
                    out_rect.y + (out_rect.height / (track.midi.outs + 1) as f32) * (port + 1) as f32;
                frame.fill(
                    &Path::circle(Point::new(out_rect.x + out_rect.width - 10.0, py), 4.0),
                    midi_port_color(),
                );
            }

            for (idx, plugin) in data.lv2_graph_plugins.iter().enumerate() {
                let pos = Self::plugin_pos(&data, plugin, idx, bounds);
                let rect = Path::rectangle(pos, iced::Size::new(PLUGIN_W, PLUGIN_H));
                frame.fill(&rect, Color::from_rgb8(28, 28, 42));
                frame.stroke(
                    &rect,
                    canvas::Stroke::default()
                        .with_color(Color::from_rgb(0.55, 0.55, 0.85))
                        .with_width(2.0),
                );
                frame.fill_text(Text {
                    content: format!("{} #{}", plugin.name, plugin.instance_id),
                    position: Point::new(pos.x + PLUGIN_W / 2.0, pos.y + 16.0),
                    color: Color::WHITE,
                    size: 14.0.into(),
                    align_x: Horizontal::Center.into(),
                    align_y: Vertical::Center,
                    ..Default::default()
                });
                for port in 0..plugin.audio_inputs {
                    let py =
                        pos.y + (PLUGIN_H / (plugin.audio_inputs + 1) as f32) * (port + 1) as f32;
                    frame.fill(
                        &Path::circle(Point::new(pos.x, py), 4.5),
                        audio_port_color(),
                    );
                }
                for port in 0..plugin.audio_outputs {
                    let py =
                        pos.y + (PLUGIN_H / (plugin.audio_outputs + 1) as f32) * (port + 1) as f32;
                    frame.fill(
                        &Path::circle(Point::new(pos.x + PLUGIN_W, py), 4.5),
                        audio_port_color(),
                    );
                }
                for port in 0..plugin.midi_inputs {
                    let py = pos.y + (PLUGIN_H / (plugin.midi_inputs + 1) as f32) * (port + 1) as f32;
                    frame.fill(
                        &Path::circle(Point::new(pos.x + 12.0, py), 3.5),
                        midi_port_color(),
                    );
                }
                for port in 0..plugin.midi_outputs {
                    let py =
                        pos.y + (PLUGIN_H / (plugin.midi_outputs + 1) as f32) * (port + 1) as f32;
                    frame.fill(
                        &Path::circle(Point::new(pos.x + PLUGIN_W - 12.0, py), 3.5),
                        midi_port_color(),
                    );
                }
            }

            for (conn_idx, conn) in data.lv2_graph_connections.iter().enumerate() {
                let start = match &conn.from_node {
                    Lv2GraphNode::TrackInput => {
                        let py =
                            in_rect.y + (in_rect.height / (track.audio.ins + 1) as f32) * (conn.from_port + 1) as f32;
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
                        let py = pos.y
                            + (PLUGIN_H / (plugin.audio_outputs + 1) as f32)
                                * (conn.from_port + 1) as f32;
                        Point::new(pos.x + PLUGIN_W, py)
                    }
                    Lv2GraphNode::TrackOutput => continue,
                };
                let end = match &conn.to_node {
                    Lv2GraphNode::TrackOutput => {
                        let py = out_rect.y
                            + (out_rect.height / (track.audio.outs + 1) as f32)
                                * (conn.to_port + 1) as f32;
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
                        let py = pos.y
                            + (PLUGIN_H / (plugin.audio_inputs + 1) as f32)
                                * (conn.to_port + 1) as f32;
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
                        let py =
                            in_rect.y + (in_rect.height / (track.audio.ins + 1) as f32) * (connecting.from_port + 1) as f32;
                        Point::new(in_rect.x + in_rect.width, py)
                    }
                    Lv2GraphNode::TrackOutput => {
                        let py =
                            out_rect.y + (out_rect.height / (track.audio.outs + 1) as f32) * (connecting.from_port + 1) as f32;
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
                        if connecting.is_input {
                            let py = pos.y
                                + (PLUGIN_H / (plugin.audio_inputs + 1) as f32)
                                    * (connecting.from_port + 1) as f32;
                            Point::new(pos.x, py)
                        } else {
                            let py = pos.y
                                + (PLUGIN_H / (plugin.audio_outputs + 1) as f32)
                                    * (connecting.from_port + 1) as f32;
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
                        .with_color(Color::from_rgba(1.0, 1.0, 1.0, 0.6))
                        .with_width(2.0),
                );
            }
        }
        vec![frame.into_geometry()]
    }
}
