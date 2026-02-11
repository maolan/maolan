use crate::{
    message::Message,
    state::{Connecting, Hovering, MovingTrack, State},
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
use maolan_engine::{kind::Kind, message::Action as EngineAction};

pub struct Graph {
    state: State,
}

impl Graph {
    pub fn new(state: State) -> Self {
        Self { state }
    }

    pub fn update(&mut self, _message: Message) {}

    pub fn view(&self) -> Element<'_, Message> {
        canvas(self).width(Length::Fill).height(Length::Fill).into()
    }
}

pub struct Connections {
    state: State,
    graph: Graph,
}

impl Connections {
    pub fn new(state: State) -> Self {
        Self {
            state: state.clone(),
            graph: Graph::new(state.clone()),
        }
    }
    pub fn update(&mut self, message: Message) {
        self.graph.update(message.clone());
    }

    pub fn view(&self) -> iced::Element<'_, Message> {
        container(self.graph.view())
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
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
        let size = iced::Size::new(140.0, 80.0);

        if let Ok(mut data) = self.state.try_write() {
            match event {
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                    let ctrl = data.ctrl;

                    // Check for connection clicks (no modifier needed)
                    let mut clicked_connection = None;
                    for (idx, conn) in data.connections.iter().enumerate() {
                        if let (Some(from_t), Some(to_t)) = (
                            data.tracks.get(conn.from_track),
                            data.tracks.get(conn.to_track),
                        ) {
                            let total_outs = from_t.audio.outs + from_t.midi.outs;
                            let out_py = from_t.position.y
                                + (size.height / (total_outs + 1) as f32)
                                    * (conn.from_port + 1) as f32;
                            let start = Point::new(from_t.position.x + size.width, out_py);

                            let total_ins = to_t.audio.ins + to_t.midi.ins;
                            let in_py = to_t.position.y
                                + (size.height / (total_ins + 1) as f32) * (conn.to_port + 1) as f32;
                            let end = Point::new(to_t.position.x, in_py);

                            // Check distance to Bezier curve (sample multiple points)
                            let dist_x = (end.x - start.x).abs() / 2.0;
                            let mut min_dist = f32::MAX;

                            // Sample 20 points along the Bezier curve
                            for i in 0..=20 {
                                let t = i as f32 / 20.0;
                                // Cubic Bezier formula: B(t) = (1-t)^3*P0 + 3(1-t)^2*t*P1 + 3(1-t)*t^2*P2 + t^3*P3
                                let t2 = t * t;
                                let t3 = t2 * t;
                                let mt = 1.0 - t;
                                let mt2 = mt * mt;
                                let mt3 = mt2 * mt;

                                let p1 = Point::new(start.x + dist_x, start.y);
                                let p2 = Point::new(end.x - dist_x, end.y);

                                let x = mt3 * start.x + 3.0 * mt2 * t * p1.x + 3.0 * mt * t2 * p2.x + t3 * end.x;
                                let y = mt3 * start.y + 3.0 * mt2 * t * p1.y + 3.0 * mt * t2 * p2.y + t3 * end.y;

                                let curve_point = Point::new(x, y);
                                let dist = cursor_position.distance(curve_point);
                                min_dist = min_dist.min(dist);
                            }

                            if min_dist < 10.0 {
                                clicked_connection = Some(idx);
                                break;
                            }
                        }
                    }

                    if let Some(idx) = clicked_connection {
                        return Some(Action::publish(Message::ConnectionViewSelectConnection(idx)));
                    }

                    for (i, track) in data.tracks.iter().enumerate().rev() {
                        // Check input ports (left side)
                        let total_ins = track.audio.ins + track.midi.ins;
                        for j in 0..total_ins {
                            let py = track.position.y
                                + (size.height / (total_ins + 1) as f32) * (j + 1) as f32;
                            let port_pos = Point::new(track.position.x, py);

                            if cursor_position.distance(port_pos) < 10.0 {
                                // Clicking on input port starts connection from input
                                let kind = if j < track.audio.ins {
                                    Kind::Audio
                                } else {
                                    Kind::MIDI
                                };
                                data.connecting = Some(Connecting {
                                    from_track: i,
                                    from_port: j,
                                    kind,
                                    point: cursor_position,
                                    is_input: true,
                                });
                                return Some(Action::capture());
                            }
                        }

                        // Check output ports (right side)
                        let total_outs = track.audio.outs + track.midi.outs;
                        for j in 0..total_outs {
                            let py = track.position.y
                                + (size.height / (total_outs + 1) as f32) * (j + 1) as f32;
                            let port_pos = Point::new(track.position.x + size.width, py);

                            if cursor_position.distance(port_pos) < 10.0 {
                                // Clicking on output port starts connection from output
                                let kind = if j < track.audio.outs {
                                    Kind::Audio
                                } else {
                                    Kind::MIDI
                                };
                                data.connecting = Some(Connecting {
                                    from_track: i,
                                    from_port: j,
                                    kind,
                                    point: cursor_position,
                                    is_input: false,
                                });
                                return Some(Action::capture());
                            }
                        }

                        // Check for track body click
                        let rect = Rectangle::new(track.position, size);
                        if rect.contains(cursor_position) {
                            if ctrl {
                                // Ctrl+click: toggle track in selection (no move)
                                return Some(Action::publish(Message::ConnectionViewSelectTrack(i)));
                            } else {
                                // Regular click: select track and start moving
                                let offset_x = cursor_position.x - track.position.x;
                                let offset_y = cursor_position.y - track.position.y;
                                data.moving_track = Some(MovingTrack {
                                    track_idx: i,
                                    offset_x,
                                    offset_y,
                                });
                                // Also select the track
                                return Some(Action::publish(Message::ConnectionViewSelectTrack(i)));
                            }
                        }
                    }

                    // If we get here, nothing was clicked - deselect all
                    return Some(Action::publish(Message::DeselectAll));
                }
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                    if let Some(conn) = &data.connecting {
                        let from_t = conn.from_track;
                        let from_p = conn.from_port;
                        let kind = conn.kind;
                        let is_input = conn.is_input;
                        let mut target_port = None;

                        if is_input {
                            // Started from input, look for output ports
                            for (i, track) in data.tracks.iter().enumerate() {
                                let total_outs = track.audio.outs + track.midi.outs;
                                for j in 0..total_outs {
                                    let py = track.position.y
                                        + (size.height / (total_outs + 1) as f32) * (j + 1) as f32;
                                    let port_pos = Point::new(track.position.x + size.width, py);

                                    if cursor_position.distance(port_pos) < 10.0 {
                                        target_port = Some((i, j));
                                        break;
                                    }
                                }
                                if target_port.is_some() {
                                    break;
                                }
                            }
                        } else {
                            // Started from output, look for input ports
                            for (i, track) in data.tracks.iter().enumerate() {
                                let total_ins = track.audio.ins + track.midi.ins;
                                for j in 0..total_ins {
                                    let py = track.position.y
                                        + (size.height / (total_ins + 1) as f32) * (j + 1) as f32;
                                    let port_pos = Point::new(track.position.x, py);

                                    if cursor_position.distance(port_pos) < 10.0 {
                                        target_port = Some((i, j));
                                        break;
                                    }
                                }
                                if target_port.is_some() {
                                    break;
                                }
                            }
                        }

                        if let Some((to_t, to_p)) = target_port
                            && let Some(to_track) = data.tracks.get(to_t)
                            && let Some(from_track) = data.tracks.get(from_t)
                        {
                            let target_kind = if is_input {
                                // Target is output port
                                if to_p < to_track.audio.outs {
                                    Kind::Audio
                                } else {
                                    Kind::MIDI
                                }
                            } else {
                                // Target is input port
                                if to_p < to_track.audio.ins {
                                    Kind::Audio
                                } else {
                                    Kind::MIDI
                                }
                            };

                            if kind == target_kind {
                                let (from_track_name, from_port_idx, to_track_name, to_port_idx) = if is_input {
                                    // Swap: we started from input, so target is output
                                    // Connection should be: output -> input
                                    let to_track_name = to_track.name.clone();
                                    let from_track_name = from_track.name.clone();

                                    // Convert combined indices to type-specific indices
                                    let from_port_idx = match kind {
                                        Kind::Audio => from_p,
                                        Kind::MIDI => from_p - from_track.audio.ins,
                                    };
                                    let to_port_idx = match kind {
                                        Kind::Audio => to_p,
                                        Kind::MIDI => to_p - to_track.audio.outs,
                                    };

                                    (to_track_name, to_port_idx, from_track_name, from_port_idx)
                                } else {
                                    // Normal: we started from output, target is input
                                    let from_track_name = from_track.name.clone();
                                    let to_track_name = to_track.name.clone();

                                    // Convert combined indices to type-specific indices
                                    let from_port_idx = match kind {
                                        Kind::Audio => from_p,
                                        Kind::MIDI => from_p - from_track.audio.outs,
                                    };
                                    let to_port_idx = match kind {
                                        Kind::Audio => to_p,
                                        Kind::MIDI => to_p - to_track.audio.ins,
                                    };

                                    (from_track_name, from_port_idx, to_track_name, to_port_idx)
                                };

                                data.connecting = None;
                                return Some(Action::publish(Message::Request(
                                    EngineAction::Connect {
                                        from_track: from_track_name,
                                        from_port: from_port_idx,
                                        to_track: to_track_name,
                                        to_port: to_port_idx,
                                        kind,
                                    },
                                )));
                            }
                        }
                    }

                    if data.connecting.is_some() {
                        data.connecting = None;
                        return Some(Action::request_redraw());
                    }
                    if data.moving_track.is_some() {
                        data.moving_track = None;
                        return Some(Action::request_redraw());
                    }
                }

                Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                    if let Some(ref mut conn) = data.connecting {
                        conn.point = cursor_position;
                        return Some(Action::request_redraw());
                    }
                    if let Some(mt) = data.moving_track.clone()
                        && let Some(track) = data.tracks.get_mut(mt.track_idx)
                    {
                        track.position.x = cursor_position.x - mt.offset_x;
                        track.position.y = cursor_position.y - mt.offset_y;
                        return Some(Action::request_redraw());
                    }

                    let mut new_hovering = None;

                    for (i, track) in data.tracks.iter().enumerate() {
                        let total_ins = track.audio.ins + track.midi.ins;
                        for j in 0..total_ins {
                            let py = track.position.y
                                + (size.height / (total_ins + 1) as f32) * (j + 1) as f32;
                            let port_pos = Point::new(track.position.x, py);

                            if cursor_position.distance(port_pos) < 10.0 {
                                new_hovering = Some(Hovering::Port {
                                    track_idx: i,
                                    port_idx: j,
                                    is_input: true,
                                });
                                break;
                            }
                        }

                        if new_hovering.is_some() {
                            break;
                        }

                        let total_outs = track.audio.outs + track.midi.outs;
                        for j in 0..total_outs {
                            let py = track.position.y
                                + (size.height / (total_outs + 1) as f32) * (j + 1) as f32;
                            let port_pos = Point::new(track.position.x + size.width, py);

                            if cursor_position.distance(port_pos) < 10.0 {
                                new_hovering = Some(Hovering::Port {
                                    track_idx: i,
                                    port_idx: j,
                                    is_input: false,
                                });
                                break;
                            }
                        }

                        if new_hovering.is_some() {
                            break;
                        }
                    }

                    if new_hovering.is_none() {
                        for (i, track) in data.tracks.iter().enumerate() {
                            let rect = Rectangle::new(track.position, size);
                            if rect.contains(cursor_position) {
                                new_hovering = Some(Hovering::Track(i));
                                break;
                            }
                        }
                    }

                    if data.hovering != new_hovering {
                        data.hovering = new_hovering;
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
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let size = iced::Size::new(140.0, 80.0);

        if let Ok(data) = self.state.try_read() {
            use crate::state::ConnectionViewSelection;

            for (idx, conn) in data.connections.iter().enumerate() {
                if let (Some(from_t), Some(to_t)) = (
                    data.tracks.get(conn.from_track),
                    data.tracks.get(conn.to_track),
                ) {
                    let total_outs = from_t.audio.outs + from_t.midi.outs;
                    let out_py = from_t.position.y
                        + (size.height / (total_outs + 1) as f32) * (conn.from_port + 1) as f32;
                    let start = Point::new(from_t.position.x + size.width, out_py);

                    let total_ins = to_t.audio.ins + to_t.midi.ins;
                    let in_py = to_t.position.y
                        + (size.height / (total_ins + 1) as f32) * (conn.to_port + 1) as f32;
                    let end = Point::new(to_t.position.x, in_py);

                    let dist = (end.x - start.x).abs() / 2.0;
                    let path = Path::new(|p| {
                        p.move_to(start);
                        p.bezier_curve_to(
                            Point::new(start.x + dist, start.y),
                            Point::new(end.x - dist, end.y),
                            end,
                        );
                    });

                    let is_selected = matches!(
                        &data.connection_view_selection,
                        ConnectionViewSelection::Connections(set) if set.contains(&idx)
                    );

                    let (color, width) = if is_selected {
                        (Color::from_rgb(1.0, 1.0, 0.0), 4.0)
                    } else {
                        let c = match conn.kind {
                            Kind::Audio => Color::from_rgb(0.2, 0.5, 1.0),
                            Kind::MIDI => Color::from_rgb(1.0, 0.6, 0.0),
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

            if let Some(conn) = &data.connecting
                && let Some(from_t) = data.tracks.get(conn.from_track)
            {
                let (start, end) = if conn.is_input {
                    // Started from input port (left side), draw to cursor
                    let total_ins = from_t.audio.ins + from_t.midi.ins;
                    let py = from_t.position.y
                        + (size.height / (total_ins + 1) as f32) * (conn.from_port + 1) as f32;
                    let start = Point::new(from_t.position.x, py);
                    (start, conn.point)
                } else {
                    // Started from output port (right side), draw to cursor
                    let total_outs = from_t.audio.outs + from_t.midi.outs;
                    let py = from_t.position.y
                        + (size.height / (total_outs + 1) as f32) * (conn.from_port + 1) as f32;
                    let start = Point::new(from_t.position.x + size.width, py);
                    (start, conn.point)
                };

                let dist = (end.x - start.x).abs() / 2.0;
                let (control1, control2) = if conn.is_input {
                    // Input port: curve goes left from start, right to end
                    (
                        Point::new(start.x - dist, start.y),
                        Point::new(end.x + dist, end.y),
                    )
                } else {
                    // Output port: curve goes right from start, left to end
                    (
                        Point::new(start.x + dist, start.y),
                        Point::new(end.x - dist, end.y),
                    )
                };

                let path = Path::new(|p| {
                    p.move_to(start);
                    p.bezier_curve_to(control1, control2, end);
                });
                frame.stroke(
                    &path,
                    canvas::Stroke::default()
                        .with_color(Color::from_rgba(1.0, 1.0, 1.0, 0.5))
                        .with_width(2.0),
                );
            }

            for (i, track) in data.tracks.iter().enumerate() {
                let pos = track.position;
                let path = Path::rectangle(pos, size);
                frame.fill(&path, Color::from_rgb8(45, 45, 45));

                let is_track_hovered = data.hovering == Some(Hovering::Track(i));
                let is_track_selected = matches!(
                    &data.connection_view_selection,
                    ConnectionViewSelection::Tracks(set) if set.contains(&i)
                );

                let (stroke_color, stroke_width) = if is_track_selected {
                    (Color::from_rgb(1.0, 1.0, 0.0), 3.0)
                } else if is_track_hovered {
                    (Color::from_rgb8(120, 120, 120), 1.0)
                } else {
                    (Color::from_rgb8(80, 80, 80), 1.0)
                };

                frame.stroke(
                    &path,
                    canvas::Stroke::default()
                        .with_color(stroke_color)
                        .with_width(stroke_width),
                );

                let total_ins = track.audio.ins + track.midi.ins;
                for j in 0..total_ins {
                    let py = pos.y + (size.height / (total_ins + 1) as f32) * (j + 1) as f32;
                    let color = if j < track.audio.ins {
                        Color::from_rgb(0.2, 0.5, 1.0)
                    } else {
                        Color::from_rgb(1.0, 0.6, 0.0)
                    };

                    let is_hovered = data.hovering
                        == Some(Hovering::Port {
                            track_idx: i,
                            port_idx: j,
                            is_input: true,
                        });
                    let radius = if is_hovered { 6.0 } else { 4.0 };

                    frame.fill(&Path::circle(Point::new(pos.x, py), radius), color);
                }

                let total_outs = track.audio.outs + track.midi.outs;
                for j in 0..total_outs {
                    let py = pos.y + (size.height / (total_outs + 1) as f32) * (j + 1) as f32;
                    let color = if j < track.audio.outs {
                        Color::from_rgb(0.2, 0.5, 1.0)
                    } else {
                        Color::from_rgb(1.0, 0.6, 0.0)
                    };

                    let is_hovered = data.hovering
                        == Some(Hovering::Port {
                            track_idx: i,
                            port_idx: j,
                            is_input: false,
                        });
                    let radius = if is_hovered { 6.0 } else { 4.0 };

                    frame.fill(
                        &Path::circle(Point::new(pos.x + size.width, py), radius),
                        color,
                    );
                }

                frame.fill_text(Text {
                    content: track.name.clone(),
                    position: Point::new(pos.x + size.width / 2.0, pos.y + size.height / 2.0),
                    color: Color::WHITE,
                    size: 14.0.into(),
                    align_x: Horizontal::Center.into(),
                    align_y: Vertical::Center,
                    ..Default::default()
                });
            }
        }
        vec![frame.into_geometry()]
    }
}
