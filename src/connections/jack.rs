use crate::{
    connections::{
        colors::{audio_port_color, midi_port_color},
        ports::hover_radius,
        selection::is_bezier_hit,
    },
    consts::connections_plugins::{MIN_PLUGIN_H, MIN_PORT_GAP},
    message::Message,
    state::State,
};
use iced::{
    Color, Point, Rectangle, Renderer, Theme,
    alignment::{Horizontal, Vertical},
    event::Event,
    mouse,
    widget::canvas::{
        self, Action, Frame, Geometry, Path, Text,
        stroke::{LineCap, LineJoin},
    },
};
use maolan_engine::kind::Kind;
use maolan_engine::message::Action as EngineAction;
use std::collections::HashMap;

const TOOLBAR_H: f32 = 44.0;
const NODE_W: f32 = 70.0;
const NODE_TOP: f32 = 70.0;
const PORT_DOT_RADIUS: f32 = 5.0;
const PORT_HIT_RADIUS: f32 = 10.0;
const BUTTON_H: f32 = 28.0;
const MAOLAN_SIZE: f32 = 170.0;
const CLIENT_W: f32 = 150.0;

#[derive(Debug, Default)]
pub struct GraphState {
    hovering_port: Option<String>,
    connecting: Option<JackConnecting>,
    moving_node: Option<MovingJackNode>,
}

#[derive(Debug, Clone)]
struct JackConnecting {
    port: String,
    is_output: bool,
    kind: Kind,
    point: Point,
}

#[derive(Debug, Clone)]
struct MovingJackNode {
    id: String,
    offset_x: f32,
    offset_y: f32,
}

pub struct Graph {
    state: State,
}

#[derive(Clone)]
struct PortLayout {
    name: String,
    point: Point,
    is_output: bool,
    kind: Kind,
    is_maolan: bool,
    side_index: usize,
    side_ports: Vec<String>,
}

#[derive(Clone)]
struct NodeLayout {
    id: String,
    title: String,
    rect: Rectangle,
    draggable: bool,
}

impl Graph {
    pub fn new(state: State) -> Self {
        Self { state }
    }

    fn button_rects(bounds: Rectangle) -> [(&'static str, &'static str, Rectangle); 2] {
        [
            (
                "add_input",
                "+",
                Rectangle::new(Point::new(0.0, 8.0), iced::Size::new(NODE_W, BUTTON_H)),
            ),
            (
                "add_output",
                "+",
                Rectangle::new(
                    Point::new(bounds.width - NODE_W, 8.0),
                    iced::Size::new(NODE_W, BUTTON_H),
                ),
            ),
        ]
    }

    fn graph_rect(bounds: Rectangle) -> Rectangle {
        let graph_top = NODE_TOP;
        let graph_h = (bounds.height - graph_top - 24.0).max(260.0);
        Rectangle::new(
            Point::new(0.0, graph_top),
            iced::Size::new(bounds.width, graph_h),
        )
    }

    fn default_maolan_rect(bounds: Rectangle, height: f32) -> Rectangle {
        let graph = Self::graph_rect(bounds);
        let x = (bounds.width - MAOLAN_SIZE) / 2.0;
        let y = graph.y + (graph.height - height) / 2.0;
        Rectangle::new(Point::new(x, y), iced::Size::new(MAOLAN_SIZE, height))
    }

    fn client_name(port_name: &str) -> &str {
        port_name
            .split_once(':')
            .map(|(client, _)| client)
            .unwrap_or(port_name)
    }

    fn node_id(port: &maolan_engine::message::JackPortInfo) -> String {
        if port.is_maolan {
            "maolan".to_string()
        } else if port.is_physical && port.is_output {
            "hw:in".to_string()
        } else if port.is_physical && port.is_input {
            "hw:out".to_string()
        } else {
            format!("client:{}", Self::client_name(&port.name))
        }
    }

    fn node_title(id: &str) -> String {
        match id {
            "maolan" => "Maolan".to_string(),
            "hw:in" => "hw:in".to_string(),
            "hw:out" => "hw:out".to_string(),
            _ => id.strip_prefix("client:").unwrap_or(id).to_string(),
        }
    }

    fn port_y(index: usize, count: usize, rect: Rectangle) -> f32 {
        rect.y + (rect.height / (count + 1).max(1) as f32) * (index + 1) as f32
    }

    fn label(name: &str) -> String {
        let short = name
            .rsplit_once(':')
            .map(|(_, short)| short)
            .unwrap_or(name);
        let digits = short
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>();
        if digits.is_empty() {
            short.to_string()
        } else {
            digits
        }
    }

    fn trailing_number(name: &str) -> Option<usize> {
        Self::label(name).parse().ok()
    }

    fn sort_ports(ports: &mut Vec<&maolan_engine::message::JackPortInfo>) {
        let kind_order = |kind| match kind {
            Kind::Audio => 0,
            Kind::MIDI => 1,
        };
        ports.sort_by(|a, b| {
            kind_order(a.kind)
                .cmp(&kind_order(b.kind))
                .then_with(|| Self::trailing_number(&a.name).cmp(&Self::trailing_number(&b.name)))
                .then_with(|| a.name.cmp(&b.name))
        });
    }

    fn node_height(inputs: usize, outputs: usize) -> f32 {
        let count = inputs.max(outputs);
        if count == 0 {
            MIN_PLUGIN_H
        } else {
            ((count + 1) as f32 * (PORT_DOT_RADIUS * 2.0 + MIN_PORT_GAP)).max(MIN_PLUGIN_H)
        }
    }

    fn default_client_position(
        index: usize,
        count: usize,
        bounds: Rectangle,
        rect: Rectangle,
    ) -> Point {
        let graph = Self::graph_rect(bounds);
        let usable_h = (graph.height - rect.height).max(0.0);
        let y = graph.y + (usable_h / (count + 1).max(1) as f32) * (index + 1) as f32;
        let center_x = (bounds.width - rect.width) / 2.0;
        let lane = if index.is_multiple_of(2) { -1.0 } else { 1.0 };
        let offset = (MAOLAN_SIZE / 2.0 + CLIENT_W * 0.7).min(bounds.width * 0.22);
        Point::new(
            (center_x + lane * offset)
                .clamp(NODE_W + 16.0, bounds.width - NODE_W - rect.width - 16.0),
            y,
        )
    }

    fn clamp_node_position(position: Point, rect: Rectangle, bounds: Rectangle) -> Point {
        let min_x = NODE_W + 8.0;
        let max_x = (bounds.width - NODE_W - rect.width - 8.0).max(min_x);
        let min_y = TOOLBAR_H + 8.0;
        let max_y = (bounds.height - rect.height - 8.0).max(min_y);
        Point::new(
            position.x.clamp(min_x, max_x),
            position.y.clamp(min_y, max_y),
        )
    }

    fn layout(
        graph: &maolan_engine::message::JackGraphInfo,
        bounds: Rectangle,
        node_positions: &HashMap<String, Point>,
    ) -> (HashMap<String, PortLayout>, Vec<NodeLayout>) {
        let graph_rect = Self::graph_rect(bounds);
        let mut layouts = HashMap::new();
        let mut node_ports: HashMap<
            String,
            (
                Vec<&maolan_engine::message::JackPortInfo>,
                Vec<&maolan_engine::message::JackPortInfo>,
            ),
        > = HashMap::new();

        for port in &graph.ports {
            let entry = node_ports.entry(Self::node_id(port)).or_default();
            if port.is_input {
                entry.0.push(port);
            }
            if port.is_output {
                entry.1.push(port);
            }
        }

        for (inputs, outputs) in node_ports.values_mut() {
            Self::sort_ports(inputs);
            Self::sort_ports(outputs);
        }

        let mut ids = node_ports.keys().cloned().collect::<Vec<_>>();
        ids.sort_by(|a, b| {
            let order = |id: &str| match id {
                "hw:in" => 0,
                "maolan" => 1,
                "hw:out" => 2,
                _ => 3,
            };
            order(a).cmp(&order(b)).then_with(|| a.cmp(b))
        });

        let client_count = ids
            .iter()
            .filter(|id| {
                id.as_str() != "hw:in" && id.as_str() != "hw:out" && id.as_str() != "maolan"
            })
            .count();
        let mut client_idx = 0;
        let mut nodes = Vec::new();

        for id in ids {
            let (inputs, outputs) = node_ports.remove(&id).unwrap_or_default();
            let dynamic_height = Self::node_height(inputs.len(), outputs.len());
            let mut rect = match id.as_str() {
                "hw:in" => Rectangle::new(
                    Point::new(0.0, graph_rect.y),
                    iced::Size::new(NODE_W, graph_rect.height),
                ),
                "hw:out" => Rectangle::new(
                    Point::new(bounds.width - NODE_W, graph_rect.y),
                    iced::Size::new(NODE_W, graph_rect.height),
                ),
                "maolan" => Self::default_maolan_rect(bounds, dynamic_height),
                _ => Rectangle::new(Point::ORIGIN, iced::Size::new(CLIENT_W, dynamic_height)),
            };
            let draggable = id != "hw:in" && id != "hw:out";
            if draggable {
                let default_position = if id == "maolan" {
                    rect.position()
                } else {
                    let position =
                        Self::default_client_position(client_idx, client_count, bounds, rect);
                    client_idx += 1;
                    position
                };
                rect = Rectangle::new(
                    Self::clamp_node_position(
                        *node_positions.get(&id).unwrap_or(&default_position),
                        rect,
                        bounds,
                    ),
                    rect.size(),
                );
            }

            let input_names_by_kind = |kind| {
                inputs
                    .iter()
                    .filter(|port| port.kind == kind)
                    .map(|port| port.name.clone())
                    .collect::<Vec<_>>()
            };
            let output_names_by_kind = |kind| {
                outputs
                    .iter()
                    .filter(|port| port.kind == kind)
                    .map(|port| port.name.clone())
                    .collect::<Vec<_>>()
            };

            for (idx, port) in inputs.iter().enumerate() {
                let side_ports = input_names_by_kind(port.kind);
                let side_index = side_ports
                    .iter()
                    .position(|name| name == &port.name)
                    .unwrap_or(0);
                layouts.insert(
                    port.name.clone(),
                    PortLayout {
                        name: port.name.clone(),
                        point: Point::new(rect.x, Self::port_y(idx, inputs.len(), rect)),
                        is_output: false,
                        kind: port.kind,
                        is_maolan: id == "maolan",
                        side_index,
                        side_ports,
                    },
                );
            }
            for (idx, port) in outputs.iter().enumerate() {
                let side_ports = output_names_by_kind(port.kind);
                let side_index = side_ports
                    .iter()
                    .position(|name| name == &port.name)
                    .unwrap_or(0);
                layouts.insert(
                    port.name.clone(),
                    PortLayout {
                        name: port.name.clone(),
                        point: Point::new(
                            rect.x + rect.width,
                            Self::port_y(idx, outputs.len(), rect),
                        ),
                        is_output: true,
                        kind: port.kind,
                        is_maolan: id == "maolan",
                        side_index,
                        side_ports,
                    },
                );
            }

            nodes.push(NodeLayout {
                id: id.clone(),
                title: Self::node_title(&id),
                rect,
                draggable,
            });
        }

        (layouts, nodes)
    }

    fn controls(start: Point, end: Point) -> (Point, Point) {
        let dx = ((end.x - start.x).abs() * 0.45).clamp(80.0, 220.0);
        (
            Point::new(start.x + dx, start.y),
            Point::new(end.x - dx, end.y),
        )
    }

    fn preview_controls(start: Point, end: Point, starts_from_output: bool) -> (Point, Point) {
        let dx = ((end.x - start.x).abs() * 0.45).clamp(80.0, 220.0);
        let dir = if starts_from_output { 1.0 } else { -1.0 };
        (
            Point::new(start.x + dx * dir, start.y),
            Point::new(end.x - dx * dir, end.y),
        )
    }

    fn draw_node(frame: &mut Frame, node: &NodeLayout) {
        let rect = node.rect;
        let path = Path::rectangle(rect.position(), rect.size());
        frame.fill(&path, Color::from_rgba(0.12, 0.15, 0.21, 0.96));
        frame.stroke(
            &path,
            canvas::Stroke::default()
                .with_color(Color::from_rgba(0.78, 0.87, 0.99, 0.22))
                .with_width(1.0),
        );
        frame.fill_text(Text {
            content: node.title.clone(),
            position: Point::new(rect.x + rect.width / 2.0, rect.y + 24.0),
            color: Color::WHITE,
            size: 18.0.into(),
            align_x: Horizontal::Center.into(),
            ..Default::default()
        });
    }

    fn draw_button(frame: &mut Frame, rect: Rectangle, label: &str) {
        let path = Path::rectangle(rect.position(), rect.size());
        frame.fill(&path, Color::from_rgba(0.16, 0.19, 0.25, 0.98));
        frame.stroke(
            &path,
            canvas::Stroke::default()
                .with_color(Color::from_rgba(0.78, 0.87, 0.99, 0.2))
                .with_width(1.0),
        );
        frame.fill_text(Text {
            content: label.into(),
            position: Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0),
            color: Color::from_rgb(0.92, 0.95, 1.0),
            size: 12.0.into(),
            align_x: Horizontal::Center.into(),
            align_y: Vertical::Center,
            ..Default::default()
        });
    }
}

impl canvas::Program<Message> for Graph {
    type State = GraphState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Message>> {
        let cursor_position = cursor.position_in(bounds)?;
        let data = self.state.blocking_read();
        let (ports, nodes) = Self::layout(&data.jack_graph, bounds, &data.jack_node_positions);

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                state.moving_node = None;
                for (id, _label, rect) in Self::button_rects(bounds) {
                    if rect.contains(cursor_position) {
                        return Some(match id {
                            "add_input" => Action::publish(Message::Request(
                                EngineAction::JackAddAudioInputPort,
                            )),
                            "add_output" => Action::publish(Message::Request(
                                EngineAction::JackAddAudioOutputPort,
                            )),
                            _ => Action::capture(),
                        });
                    }
                }
                for port in ports.values() {
                    if cursor_position.distance(port.point) <= PORT_HIT_RADIUS {
                        state.connecting = Some(JackConnecting {
                            port: port.name.clone(),
                            is_output: port.is_output,
                            kind: port.kind,
                            point: cursor_position,
                        });
                        return Some(Action::capture());
                    }
                }
                for node in nodes.iter().rev().filter(|node| node.draggable) {
                    if node.rect.contains(cursor_position) {
                        state.moving_node = Some(MovingJackNode {
                            id: node.id.clone(),
                            offset_x: cursor_position.x - node.rect.x,
                            offset_y: cursor_position.y - node.rect.y,
                        });
                        return Some(Action::capture());
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.moving_node.take().is_some() {
                    return Some(Action::request_redraw());
                }
                let connecting = state.connecting.take()?;
                for port in ports.values().filter(|port| {
                    port.is_output != connecting.is_output && port.kind == connecting.kind
                }) {
                    if cursor_position.distance(port.point) <= PORT_HIT_RADIUS {
                        let Some(start_port) = ports.get(&connecting.port) else {
                            return Some(Action::request_redraw());
                        };
                        let (source, destination) = if start_port.is_output {
                            (start_port, port)
                        } else {
                            (port, start_port)
                        };
                        let parallel_count = if data.shift {
                            source
                                .side_ports
                                .len()
                                .saturating_sub(source.side_index)
                                .min(
                                    destination
                                        .side_ports
                                        .len()
                                        .saturating_sub(destination.side_index),
                                )
                                .max(1)
                        } else {
                            1
                        };
                        let actions = (0..parallel_count)
                            .map(|offset| EngineAction::JackConnect {
                                source: source.side_ports[source.side_index + offset].clone(),
                                destination: destination.side_ports
                                    [destination.side_index + offset]
                                    .clone(),
                            })
                            .collect::<Vec<_>>();
                        if actions.len() == 1 {
                            return Some(Action::publish(Message::Request(
                                actions.into_iter().next().unwrap(),
                            )));
                        }
                        return Some(Action::publish(Message::RequestBatch(actions)));
                    }
                }
                return Some(Action::request_redraw());
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle)) => {
                for port in ports.values() {
                    if port.is_maolan
                        && port.kind == Kind::Audio
                        && cursor_position.distance(port.point) <= PORT_HIT_RADIUS
                    {
                        let action = if port.is_output {
                            EngineAction::JackRemoveAudioOutputPort(port.side_index)
                        } else {
                            EngineAction::JackRemoveAudioInputPort(port.side_index)
                        };
                        return Some(Action::publish(Message::Request(action)));
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                for connection in &data.jack_graph.connections {
                    let (Some(source), Some(destination)) = (
                        ports.get(&connection.source),
                        ports.get(&connection.destination),
                    ) else {
                        continue;
                    };
                    if is_bezier_hit(source.point, destination.point, cursor_position, 32, 6.0) {
                        return Some(Action::publish(Message::JackDisconnect {
                            source: connection.source.clone(),
                            destination: connection.destination.clone(),
                        }));
                    }
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some(moving) = &state.moving_node {
                    drop(data);
                    let mut data = self.state.blocking_write();
                    let graph_rect = nodes
                        .iter()
                        .find(|node| node.id == moving.id)
                        .map(|node| node.rect)
                        .unwrap_or_else(|| {
                            Rectangle::new(Point::ORIGIN, iced::Size::new(CLIENT_W, MIN_PLUGIN_H))
                        });
                    let position = Point::new(
                        cursor_position.x - moving.offset_x,
                        cursor_position.y - moving.offset_y,
                    );
                    data.jack_node_positions.insert(
                        moving.id.clone(),
                        Self::clamp_node_position(position, graph_rect, bounds),
                    );
                    return Some(Action::request_redraw());
                }
                let hovering = ports
                    .values()
                    .find(|port| cursor_position.distance(port.point) <= PORT_HIT_RADIUS)
                    .map(|port| port.name.clone());
                if state.hovering_port != hovering {
                    state.hovering_port = hovering;
                    return Some(Action::request_redraw());
                }
                if let Some(connecting) = &mut state.connecting {
                    connecting.point = cursor_position;
                    return Some(Action::request_redraw());
                }
            }
            _ => {}
        }
        None
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let data = self.state.blocking_read();
        let mut frame = Frame::new(renderer, bounds.size());
        let (ports, nodes) = Self::layout(&data.jack_graph, bounds, &data.jack_node_positions);

        frame.fill(
            &Path::rectangle(Point::ORIGIN, bounds.size()),
            Color::from_rgba(0.07, 0.08, 0.11, 1.0),
        );
        for (_id, label, rect) in Self::button_rects(bounds) {
            Self::draw_button(&mut frame, rect, label);
        }
        frame.fill_text(Text {
            content: "JACK Connections".into(),
            position: Point::new(460.0, TOOLBAR_H / 2.0),
            color: Color::WHITE,
            size: 20.0.into(),
            align_y: Vertical::Center,
            ..Default::default()
        });

        for node in &nodes {
            Self::draw_node(&mut frame, node);
        }

        for connection in &data.jack_graph.connections {
            let (Some(source), Some(destination)) = (
                ports.get(&connection.source),
                ports.get(&connection.destination),
            ) else {
                continue;
            };
            let (c1, c2) = Self::controls(source.point, destination.point);
            frame.stroke(
                &Path::new(|p| {
                    p.move_to(source.point);
                    p.bezier_curve_to(c1, c2, destination.point);
                }),
                canvas::Stroke {
                    style: canvas::Style::Solid(match source.kind {
                        Kind::Audio => Color::from_rgba(0.73, 0.84, 1.0, 0.62),
                        Kind::MIDI => Color::from_rgba(0.58, 0.92, 0.62, 0.62),
                    }),
                    width: 2.0,
                    line_cap: LineCap::Round,
                    line_join: LineJoin::Round,
                    ..Default::default()
                },
            );
        }

        for port in ports.values() {
            let hovering = state.hovering_port.as_ref() == Some(&port.name);
            let dot = Path::circle(port.point, hover_radius(PORT_DOT_RADIUS, hovering));
            frame.fill(
                &dot,
                match port.kind {
                    Kind::Audio => audio_port_color(),
                    Kind::MIDI => midi_port_color(),
                },
            );
            frame.stroke(
                &dot,
                canvas::Stroke::default()
                    .with_color(Color::from_rgba(0.03, 0.04, 0.06, 0.9))
                    .with_width(1.0),
            );
            let label = Self::label(&port.name);
            let align_left = port.point.x < bounds.width / 2.0;
            let x = if align_left {
                port.point.x - 10.0
            } else {
                port.point.x + 10.0
            };
            frame.fill_text(Text {
                content: label,
                position: Point::new(x, port.point.y),
                color: Color::from_rgb(0.78, 0.84, 0.92),
                size: 10.0.into(),
                align_x: if align_left {
                    Horizontal::Right.into()
                } else {
                    Horizontal::Left.into()
                },
                align_y: Vertical::Center,
                ..Default::default()
            });
        }

        if let Some(connecting) = &state.connecting
            && let Some(port) = ports.get(&connecting.port)
        {
            let (c1, c2) =
                Self::preview_controls(port.point, connecting.point, connecting.is_output);
            frame.stroke(
                &Path::new(|p| {
                    p.move_to(port.point);
                    p.bezier_curve_to(c1, c2, connecting.point);
                }),
                canvas::Stroke::default()
                    .with_color(match connecting.kind {
                        Kind::Audio => Color::from_rgba(0.73, 0.84, 1.0, 0.62),
                        Kind::MIDI => Color::from_rgba(0.58, 0.92, 0.62, 0.62),
                    })
                    .with_width(2.0),
            );
            frame.stroke(
                &Path::circle(port.point, PORT_DOT_RADIUS + 6.0),
                canvas::Stroke::default()
                    .with_color(Color::from_rgb(0.45, 0.78, 1.0))
                    .with_width(2.0),
            );
        }

        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::widget::canvas::Program;
    use iced::{Size, event};
    use maolan_engine::message::{JackGraphInfo, JackPortInfo};
    use std::sync::Arc;
    use tokio::sync::RwLock;

    fn action_message(action: Action<Message>) -> (Option<Message>, event::Status) {
        let (message, _redraw, status) = action.into_inner();
        (message, status)
    }

    fn port_point(
        state: &Arc<RwLock<crate::state::StateData>>,
        bounds: Rectangle,
        name: &str,
    ) -> Point {
        let data = state.blocking_read();
        Graph::layout(&data.jack_graph, bounds, &data.jack_node_positions)
            .0
            .get(name)
            .map(|port| port.point)
            .expect("port point")
    }

    fn node_rect(
        state: &Arc<RwLock<crate::state::StateData>>,
        bounds: Rectangle,
        id: &str,
    ) -> Rectangle {
        let data = state.blocking_read();
        Graph::layout(&data.jack_graph, bounds, &data.jack_node_positions)
            .1
            .iter()
            .find(|node| node.id == id)
            .map(|node| node.rect)
            .expect("node rect")
    }

    #[test]
    fn dragging_output_to_matching_input_requests_jack_connect() {
        let state = Arc::new(RwLock::new(crate::state::StateData::default()));
        state.blocking_write().jack_graph = JackGraphInfo {
            ports: vec![
                JackPortInfo {
                    name: "system:capture_1".to_string(),
                    kind: Kind::Audio,
                    is_input: false,
                    is_output: true,
                    is_physical: true,
                    is_maolan: false,
                },
                JackPortInfo {
                    name: "maolan:in_1".to_string(),
                    kind: Kind::Audio,
                    is_input: true,
                    is_output: false,
                    is_physical: false,
                    is_maolan: true,
                },
            ],
            connections: vec![],
        };
        let graph = Graph::new(state.clone());
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(800.0, 600.0));
        let source = port_point(&state, bounds, "system:capture_1");
        let destination = port_point(&state, bounds, "maolan:in_1");
        let mut graph_state = GraphState::default();

        let press = graph
            .update(
                &mut graph_state,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(source),
            )
            .expect("press action");
        let (_, press_status) = action_message(press);
        assert_eq!(press_status, event::Status::Captured);

        graph.update(
            &mut graph_state,
            &Event::Mouse(mouse::Event::CursorMoved {
                position: destination,
            }),
            bounds,
            mouse::Cursor::Available(destination),
        );

        let release = graph
            .update(
                &mut graph_state,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(destination),
            )
            .expect("release action");
        let (message, _) = action_message(release);
        assert!(matches!(
            message,
            Some(Message::Request(EngineAction::JackConnect {
                source,
                destination,
            })) if source == "system:capture_1" && destination == "maolan:in_1"
        ));
    }

    #[test]
    fn dragging_input_to_matching_output_requests_normalized_jack_connect() {
        let state = Arc::new(RwLock::new(crate::state::StateData::default()));
        state.blocking_write().jack_graph = JackGraphInfo {
            ports: vec![
                JackPortInfo {
                    name: "system:capture_1".to_string(),
                    kind: Kind::Audio,
                    is_input: false,
                    is_output: true,
                    is_physical: true,
                    is_maolan: false,
                },
                JackPortInfo {
                    name: "maolan:in_1".to_string(),
                    kind: Kind::Audio,
                    is_input: true,
                    is_output: false,
                    is_physical: false,
                    is_maolan: true,
                },
            ],
            connections: vec![],
        };
        let graph = Graph::new(state.clone());
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(800.0, 600.0));
        let input = port_point(&state, bounds, "maolan:in_1");
        let output = port_point(&state, bounds, "system:capture_1");
        let mut graph_state = GraphState::default();

        graph
            .update(
                &mut graph_state,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(input),
            )
            .expect("press action");

        let release = graph
            .update(
                &mut graph_state,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(output),
            )
            .expect("release action");
        let (message, _) = action_message(release);
        assert!(matches!(
            message,
            Some(Message::Request(EngineAction::JackConnect {
                source,
                destination,
            })) if source == "system:capture_1" && destination == "maolan:in_1"
        ));
    }

    #[test]
    fn shift_dragging_connects_parallel_jack_ports() {
        let state = Arc::new(RwLock::new(crate::state::StateData::default()));
        {
            let mut data = state.blocking_write();
            data.shift = true;
            data.jack_graph = JackGraphInfo {
                ports: vec![
                    JackPortInfo {
                        name: "system:capture_1".to_string(),
                        kind: Kind::Audio,
                        is_input: false,
                        is_output: true,
                        is_physical: true,
                        is_maolan: false,
                    },
                    JackPortInfo {
                        name: "system:capture_2".to_string(),
                        kind: Kind::Audio,
                        is_input: false,
                        is_output: true,
                        is_physical: true,
                        is_maolan: false,
                    },
                    JackPortInfo {
                        name: "maolan:in_1".to_string(),
                        kind: Kind::Audio,
                        is_input: true,
                        is_output: false,
                        is_physical: false,
                        is_maolan: true,
                    },
                    JackPortInfo {
                        name: "maolan:in_2".to_string(),
                        kind: Kind::Audio,
                        is_input: true,
                        is_output: false,
                        is_physical: false,
                        is_maolan: true,
                    },
                ],
                connections: vec![],
            };
        }
        let graph = Graph::new(state.clone());
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(800.0, 600.0));
        let source = port_point(&state, bounds, "system:capture_1");
        let destination = port_point(&state, bounds, "maolan:in_1");
        let mut graph_state = GraphState::default();

        graph
            .update(
                &mut graph_state,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(source),
            )
            .expect("press action");

        let release = graph
            .update(
                &mut graph_state,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(destination),
            )
            .expect("release action");
        let (message, _) = action_message(release);
        let Some(Message::RequestBatch(actions)) = message else {
            panic!("expected RequestBatch");
        };
        assert!(matches!(
            actions.as_slice(),
            [
                EngineAction::JackConnect { source: source_1, destination: destination_1 },
                EngineAction::JackConnect { source: source_2, destination: destination_2 },
            ] if source_1 == "system:capture_1"
                && destination_1 == "maolan:in_1"
                && source_2 == "system:capture_2"
                && destination_2 == "maolan:in_2"
        ));
    }

    #[test]
    fn toolbar_edits_maolan_jack_port_counts_in_place() {
        let state = Arc::new(RwLock::new(crate::state::StateData::default()));
        state.blocking_write().jack_graph = JackGraphInfo {
            ports: vec![
                JackPortInfo {
                    name: "maolan:in_1".to_string(),
                    kind: Kind::Audio,
                    is_input: true,
                    is_output: false,
                    is_physical: false,
                    is_maolan: true,
                },
                JackPortInfo {
                    name: "maolan:out_1".to_string(),
                    kind: Kind::Audio,
                    is_input: false,
                    is_output: true,
                    is_physical: false,
                    is_maolan: true,
                },
            ],
            connections: vec![],
        };
        let graph = Graph::new(state);
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(800.0, 600.0));
        let mut graph_state = GraphState::default();

        let in_plus = graph
            .update(
                &mut graph_state,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(Point::new(35.0, 20.0)),
            )
            .expect("in plus action");
        assert!(matches!(
            action_message(in_plus).0,
            Some(Message::Request(EngineAction::JackAddAudioInputPort))
        ));

        let out_plus = graph
            .update(
                &mut graph_state,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(Point::new(765.0, 20.0)),
            )
            .expect("out plus action");
        assert!(matches!(
            action_message(out_plus).0,
            Some(Message::Request(EngineAction::JackAddAudioOutputPort))
        ));
    }

    #[test]
    fn middle_clicking_specific_maolan_jack_port_removes_that_port() {
        let state = Arc::new(RwLock::new(crate::state::StateData::default()));
        state.blocking_write().jack_graph = JackGraphInfo {
            ports: vec![
                JackPortInfo {
                    name: "maolan:in_1".to_string(),
                    kind: Kind::Audio,
                    is_input: true,
                    is_output: false,
                    is_physical: false,
                    is_maolan: true,
                },
                JackPortInfo {
                    name: "maolan:in_2".to_string(),
                    kind: Kind::Audio,
                    is_input: true,
                    is_output: false,
                    is_physical: false,
                    is_maolan: true,
                },
                JackPortInfo {
                    name: "maolan:in_3".to_string(),
                    kind: Kind::Audio,
                    is_input: true,
                    is_output: false,
                    is_physical: false,
                    is_maolan: true,
                },
            ],
            connections: vec![],
        };
        let graph = Graph::new(state.clone());
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(800.0, 600.0));
        let mut graph_state = GraphState::default();
        let target = port_point(&state, bounds, "maolan:in_2");

        let action = graph
            .update(
                &mut graph_state,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle)),
                bounds,
                mouse::Cursor::Available(target),
            )
            .expect("middle-click action");

        assert!(matches!(
            action_message(action).0,
            Some(Message::Request(EngineAction::JackRemoveAudioInputPort(1)))
        ));
    }

    #[test]
    fn dragging_maolan_updates_node_position() {
        let state = Arc::new(RwLock::new(crate::state::StateData::default()));
        state.blocking_write().jack_graph = JackGraphInfo {
            ports: vec![JackPortInfo {
                name: "maolan:in_1".to_string(),
                kind: Kind::Audio,
                is_input: true,
                is_output: false,
                is_physical: false,
                is_maolan: true,
            }],
            connections: vec![],
        };
        let graph = Graph::new(state.clone());
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(800.0, 600.0));
        let mut graph_state = GraphState::default();
        let maolan = node_rect(&state, bounds, "maolan");
        let press = Point::new(maolan.x + 45.0, maolan.y + 32.0);
        let moved = Point::new(press.x + 30.0, press.y + 30.0);

        let press_action = graph
            .update(
                &mut graph_state,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(press),
            )
            .expect("press action");
        let (_, press_status) = action_message(press_action);
        assert_eq!(press_status, event::Status::Captured);

        graph
            .update(
                &mut graph_state,
                &Event::Mouse(mouse::Event::CursorMoved { position: moved }),
                bounds,
                mouse::Cursor::Available(moved),
            )
            .expect("move action");

        assert_eq!(
            state
                .blocking_read()
                .jack_node_positions
                .get("maolan")
                .copied(),
            Some(Point::new(maolan.x + 30.0, maolan.y + 30.0))
        );
    }

    #[test]
    fn node_height_keeps_port_spacing_constant() {
        assert_eq!(Graph::node_height(1, 1), MIN_PLUGIN_H);
        assert_eq!(Graph::node_height(12, 1), 156.0);
        let rect = Rectangle::new(Point::new(0.0, 70.0), Size::new(150.0, 156.0));

        assert_eq!(Graph::port_y(0, 12, rect), 82.0);
        assert_eq!(Graph::port_y(1, 12, rect), 94.0);
        assert_eq!(Graph::port_y(11, 12, rect), 214.0);
    }

    #[test]
    fn hardware_nodes_keep_graph_height_regardless_of_port_count() {
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(800.0, 600.0));
        let graph = JackGraphInfo {
            ports: (1..=12)
                .map(|idx| JackPortInfo {
                    name: format!("system:capture_{idx}"),
                    kind: Kind::Audio,
                    is_input: false,
                    is_output: true,
                    is_physical: true,
                    is_maolan: false,
                })
                .collect(),
            connections: vec![],
        };

        let (_, nodes) = Graph::layout(&graph, bounds, &HashMap::new());
        let hw_in = nodes.iter().find(|node| node.id == "hw:in").unwrap();

        assert_eq!(hw_in.rect.y, NODE_TOP);
        assert_eq!(hw_in.rect.height, bounds.height - NODE_TOP - 24.0);
    }
}
