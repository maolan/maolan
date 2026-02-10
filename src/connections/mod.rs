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
use maolan_engine::kind::Kind;

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

impl<Message> canvas::Program<Message> for Graph {
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
                    for (i, track) in data.tracks.iter().enumerate().rev() {
                        // 1. Provera klika na OUT portove (desna strana) za novu konekciju
                        let total_outs = track.audio.outs + track.midi.outs;
                        for j in 0..total_outs {
                            let py = track.position.y
                                + (size.height / (total_outs + 1) as f32) * (j + 1) as f32;
                            let port_pos = Point::new(track.position.x + size.width, py);

                            if cursor_position.distance(port_pos) < 10.0 {
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
                                });
                                return Some(Action::capture());
                            }
                        }

                        // 2. Provera klika na traku za pomeranje
                        let rect = Rectangle::new(track.position, size);
                        if rect.contains(cursor_position) {
                            let offset_x = cursor_position.x - track.position.x;
                            let offset_y = cursor_position.y - track.position.y;
                            data.moving_track = Some(MovingTrack {
                                track_idx: i,
                                offset_x,
                                offset_y,
                            });
                            return Some(Action::capture());
                        }
                    }
                }
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                    if let Some(conn) = &data.connecting {
                        let from_t = conn.from_track;
                        let from_p = conn.from_port;
                        let kind = conn.kind;
                        let mut target_port = None;

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

                        if let Some((to_t, to_p)) = target_port
                            && let Some(to_track) = data.tracks.get(to_t)
                        {
                            let target_kind = if to_p < to_track.audio.ins {
                                Kind::Audio
                            } else {
                                Kind::MIDI
                            };

                            if kind == target_kind {
                                data.connections.push(crate::state::Connection {
                                    from_track: from_t,
                                    from_port: from_p,
                                    to_track: to_t,
                                    to_port: to_p,
                                    kind,
                                });
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
                    if let Some(mt) = data.moving_track.clone() {
                        if let Some(track) = data.tracks.get_mut(mt.track_idx) {
                            track.position.x = cursor_position.x - mt.offset_x;
                            track.position.y = cursor_position.y - mt.offset_y;
                            return Some(Action::request_redraw());
                        }
                    }

                    // Detect hover state
                    let mut new_hovering = None;

                    // First check all ports
                    for (i, track) in data.tracks.iter().enumerate() {
                        // Check input ports (left side)
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

                        // Check output ports (right side)
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

                    // If no port is hovered, check tracks
                    if new_hovering.is_none() {
                        for (i, track) in data.tracks.iter().enumerate() {
                            let rect = Rectangle::new(track.position, size);
                            if rect.contains(cursor_position) {
                                new_hovering = Some(Hovering::Track(i));
                                break;
                            }
                        }
                    }

                    // Update hover state if changed
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
            // Iscrtavanje postojećih konekcija
            for conn in &data.connections {
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

                    let color = match conn.kind {
                        Kind::Audio => Color::from_rgb(0.2, 0.5, 1.0),
                        Kind::MIDI => Color::from_rgb(1.0, 0.6, 0.0),
                    };
                    frame.stroke(
                        &path,
                        canvas::Stroke::default().with_color(color).with_width(2.0),
                    );
                }
            }

            // Iscrtavanje "žive" konekcije dok se vuče
            if let Some(conn) = &data.connecting {
                if let Some(from_t) = data.tracks.get(conn.from_track) {
                    let total_outs = from_t.audio.outs + from_t.midi.outs;
                    let py = from_t.position.y
                        + (size.height / (total_outs + 1) as f32) * (conn.from_port + 1) as f32;
                    let start = Point::new(from_t.position.x + size.width, py);

                    let dist = (conn.point.x - start.x).abs() / 2.0;
                    let path = Path::new(|p| {
                        p.move_to(start);
                        p.bezier_curve_to(
                            Point::new(start.x + dist, start.y),
                            Point::new(conn.point.x - dist, conn.point.y),
                            conn.point,
                        );
                    });
                    frame.stroke(
                        &path,
                        canvas::Stroke::default()
                            .with_color(Color::from_rgba(1.0, 1.0, 1.0, 0.5))
                            .with_width(2.0),
                    );
                }
            }

            // Iscrtavanje traka (ovde ide tvoj postojeći for track in data.tracks.iter() kod...)
            for (i, track) in data.tracks.iter().enumerate() {
                // ... (tvoj originalni draw kod za trake, krugove i tekst ostaje isti)
                let pos = track.position;
                let path = Path::rectangle(pos, size);
                frame.fill(&path, Color::from_rgb8(45, 45, 45));

                // Highlight track if hovered (but not if a port on it is hovered)
                let is_track_hovered = data.hovering == Some(Hovering::Track(i));
                let stroke_color = if is_track_hovered {
                    Color::from_rgb8(120, 120, 120)
                } else {
                    Color::from_rgb8(80, 80, 80)
                };
                frame.stroke(
                    &path,
                    canvas::Stroke::default()
                        .with_color(stroke_color)
                        .with_width(1.0),
                );

                // Krugovi za portove...
                let total_ins = track.audio.ins + track.midi.ins;
                for j in 0..total_ins {
                    let py = pos.y + (size.height / (total_ins + 1) as f32) * (j + 1) as f32;
                    let color = if j < track.audio.ins {
                        Color::from_rgb(0.2, 0.5, 1.0)
                    } else {
                        Color::from_rgb(1.0, 0.6, 0.0)
                    };

                    // Highlight if hovered
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

                    // Highlight if hovered
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
