use crate::{
    message::Message,
    state::{Connecting, HW_IN_ID, HW_OUT_ID, Hovering, MovingTrack, State, StateData},
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

    fn get_port_kind(
        data: &StateData,
        hovering_port: &Hovering,
    ) -> Option<Kind> {
        match hovering_port {
            Hovering::Port {
                track_idx,
                port_idx,
                is_input,
            } => {
                if *track_idx == HW_IN_ID || *track_idx == HW_OUT_ID {
                    Some(Kind::Audio) // HW ports are always Audio
                } else {
                    data.tracks.get(*track_idx).map(|t| {
                        if *is_input {
                            // If hovered port is an INPUT on a track
                            if *port_idx < t.audio.ins {
                                Kind::Audio
                            } else {
                                Kind::MIDI
                            }
                        } else {
                            // If hovered port is an OUTPUT on a track
                            if *port_idx < t.audio.outs {
                                Kind::Audio
                            } else {
                                Kind::MIDI
                            }
                        }
                    })
                }
            }
            _ => None, // Not a port hover
        }
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
        let hw_width = 70.0;
        let hw_margin = 20.0;
        let hw_height = bounds.height - (hw_margin * 2.0);

        if let Ok(mut data) = self.state.try_write() {
            match event {
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                    let ctrl = data.ctrl;

                    // 1. PROVERA KLIKA NA KONEKCIJE (Bezier krive)
                    let mut clicked_connection = None;
                    for (idx, conn) in data.connections.iter().enumerate() {
                        // Određivanje startne tačke (izlaz)
                        let start_point = if conn.from_track == HW_IN_ID {
                            data.hw_in.as_ref().map(|hw| {
                                let py = hw_margin
                                    + 50.0
                                    + ((hw_height - 60.0) / (hw.channels + 1) as f32)
                                        * (conn.from_port + 1) as f32;
                                Point::new(hw_margin + hw_width, py)
                            })
                        } else {
                            data.tracks.get(conn.from_track).map(|t| {
                                let total_outs = t.audio.outs + t.midi.outs;
                                let py = t.position.y
                                    + (size.height / (total_outs + 1) as f32)
                                        * (conn.from_port + 1) as f32;
                                Point::new(t.position.x + size.width, py)
                            })
                        };

                        // Određivanje krajnje tačke (ulaz)
                        let end_point = if conn.to_track == HW_OUT_ID {
                            data.hw_out.as_ref().map(|hw| {
                                let py = hw_margin
                                    + 50.0
                                    + ((hw_height - 60.0) / (hw.channels + 1) as f32)
                                        * (conn.to_port + 1) as f32;
                                Point::new(bounds.width - hw_width - hw_margin, py)
                            })
                        } else {
                            data.tracks.get(conn.to_track).map(|t| {
                                let total_ins = t.audio.ins + t.midi.ins;
                                let py = t.position.y
                                    + (size.height / (total_ins + 1) as f32)
                                        * (conn.to_port + 1) as f32;
                                Point::new(t.position.x, py)
                            })
                        };

                        if let (Some(start), Some(end)) = (start_point, end_point) {
                            let dist_x = (end.x - start.x).abs() / 2.0;
                            let mut min_dist = f32::MAX;

                            for i in 0..=20 {
                                let t = i as f32 / 20.0;
                                let mt = 1.0 - t;
                                let p1 = Point::new(start.x + dist_x, start.y);
                                let p2 = Point::new(end.x - dist_x, end.y);
                                let x = mt.powi(3) * start.x
                                    + 3.0 * mt.powi(2) * t * p1.x
                                    + 3.0 * mt * t.powi(2) * p2.x
                                    + t.powi(3) * end.x;
                                let y = mt.powi(3) * start.y
                                    + 3.0 * mt.powi(2) * t * p1.y
                                    + 3.0 * mt * t.powi(2) * p2.y
                                    + t.powi(3) * end.y;

                                min_dist = min_dist.min(cursor_position.distance(Point::new(x, y)));
                            }

                            if min_dist < 10.0 {
                                clicked_connection = Some(idx);
                                break;
                            }
                        }
                    }

                    if let Some(idx) = clicked_connection {
                        return Some(Action::publish(Message::ConnectionViewSelectConnection(
                            idx,
                        )));
                    }

                    // 2. PROVERA KLIKA NA HW:IN PORTOVE (Izlazi)
                    if let Some(hw_in) = &data.hw_in {
                        let pos = Point::new(hw_margin, hw_margin);
                        for j in 0..hw_in.channels {
                            let py = pos.y
                                + 50.0
                                + ((hw_height - 60.0) / (hw_in.channels + 1) as f32)
                                    * (j + 1) as f32;
                            if cursor_position.distance(Point::new(pos.x + hw_width, py)) < 10.0 {
                                data.connecting = Some(Connecting {
                                    from_track: HW_IN_ID,
                                    from_port: j,
                                    kind: Kind::Audio,
                                    point: cursor_position,
                                    is_input: false,
                                });
                                return Some(Action::capture());
                            }
                        }
                    }

                    // 3. PROVERA KLIKA NA HW:OUT PORTOVE (Ulazi)
                    if let Some(hw_out) = &data.hw_out {
                        let pos = Point::new(bounds.width - hw_width - hw_margin, hw_margin);
                        for j in 0..hw_out.channels {
                            let py = pos.y
                                + 50.0
                                + ((hw_height - 60.0) / (hw_out.channels + 1) as f32)
                                    * (j + 1) as f32;
                            if cursor_position.distance(Point::new(pos.x, py)) < 10.0 {
                                data.connecting = Some(Connecting {
                                    from_track: HW_OUT_ID,
                                    from_port: j,
                                    kind: Kind::Audio,
                                    point: cursor_position,
                                    is_input: true,
                                });
                                return Some(Action::capture());
                            }
                        }
                    }

                    // 4. PROVERA KLIKA NA TRAKE (Portovi i Telo)
                    for (i, track) in data.tracks.iter().enumerate().rev() {
                        // Ulazni portovi
                        let t_ins = track.audio.ins + track.midi.ins;
                        for j in 0..t_ins {
                            let py = track.position.y
                                + (size.height / (t_ins + 1) as f32) * (j + 1) as f32;
                            if cursor_position.distance(Point::new(track.position.x, py)) < 10.0 {
                                data.connecting = Some(Connecting {
                                    from_track: i,
                                    from_port: j,
                                    kind: if j < track.audio.ins {
                                        Kind::Audio
                                    } else {
                                        Kind::MIDI
                                    },
                                    point: cursor_position,
                                    is_input: true,
                                });
                                return Some(Action::capture());
                            }
                        }

                        // Izlazni portovi
                        let t_outs = track.audio.outs + track.midi.outs;
                        for j in 0..t_outs {
                            let py = track.position.y
                                + (size.height / (t_outs + 1) as f32) * (j + 1) as f32;
                            if cursor_position
                                .distance(Point::new(track.position.x + size.width, py))
                                < 10.0
                            {
                                data.connecting = Some(Connecting {
                                    from_track: i,
                                    from_port: j,
                                    kind: if j < track.audio.outs {
                                        Kind::Audio
                                    } else {
                                        Kind::MIDI
                                    },
                                    point: cursor_position,
                                    is_input: false,
                                });
                                return Some(Action::capture());
                            }
                        }

                        // Telo trake
                        if Rectangle::new(track.position, size).contains(cursor_position) {
                            if ctrl {
                                return Some(Action::publish(Message::ConnectionViewSelectTrack(
                                    i,
                                )));
                            } else {
                                data.moving_track = Some(MovingTrack {
                                    track_idx: i,
                                    offset_x: cursor_position.x - track.position.x,
                                    offset_y: cursor_position.y - track.position.y,
                                });
                                return Some(Action::publish(Message::ConnectionViewSelectTrack(
                                    i,
                                )));
                            }
                        }
                    }

                    // Klik u prazno
                    return Some(Action::publish(Message::DeselectAll));
                }

                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                    if let Some(conn) = data.connecting.take() {
                        let from_t = conn.from_track;
                        let from_p = conn.from_port;
                        let kind = conn.kind;
                        let is_input = conn.is_input;
                        let mut target_port = None;

                        if is_input {
                            // --- Tražimo OUTPUT port kao metu ---
                            // 1. Provera na običnim trakama
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
                            // 2. Provera na hw:in (on je izvor/output)
                            if target_port.is_none()
                                && let Some(hw_in) = &data.hw_in
                            {
                                for j in 0..hw_in.channels {
                                    let py = hw_margin
                                        + 50.0
                                        + ((hw_height - 60.0) / (hw_in.channels + 1) as f32)
                                            * (j + 1) as f32;
                                    if cursor_position
                                        .distance(Point::new(hw_margin + hw_width, py))
                                        < 10.0
                                    {
                                        target_port = Some((HW_IN_ID, j));
                                        break;
                                    }
                                }
                            }
                        } else {
                            // --- Tražimo INPUT port kao metu ---
                            // 1. Provera na običnim trakama
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
                            // 2. Provera na hw:out (on je ponor/input)
                            if target_port.is_none()
                                && let Some(hw_out) = &data.hw_out
                            {
                                for j in 0..hw_out.channels {
                                    let py = hw_margin
                                        + 50.0
                                        + ((hw_height - 60.0) / (hw_out.channels + 1) as f32)
                                            * (j + 1) as f32;
                                    if cursor_position.distance(Point::new(
                                        bounds.width - hw_width - hw_margin,
                                        py,
                                    )) < 10.0
                                    {
                                        target_port = Some((HW_OUT_ID, j));
                                        break;
                                    }
                                }
                            }
                        }

                        // --- Obrada rezultata povezivanja ---
                        if let Some((to_t, to_p)) = target_port {
                            // Bezbedno dobavljanje imena "izvora"
                            let from_name = match from_t {
                                HW_IN_ID => "hw:in".to_string(),
                                HW_OUT_ID => "hw:out".to_string(),
                                idx => data
                                    .tracks
                                    .get(idx)
                                    .map(|t| t.name.clone())
                                    .unwrap_or_default(),
                            };

                            // Bezbedno dobavljanje imena "ponora"
                            let to_name = match to_t {
                                HW_IN_ID => "hw:in".to_string(),
                                HW_OUT_ID => "hw:out".to_string(),
                                idx => data
                                    .tracks
                                    .get(idx)
                                    .map(|t| t.name.clone())
                                    .unwrap_or_default(),
                            };

                            // Određivanje Kind-a mete (Hardver je uvek Audio)
                            let target_kind = if to_t == HW_IN_ID || to_t == HW_OUT_ID {
                                Kind::Audio
                            } else {
                                data.tracks
                                    .get(to_t)
                                    .map(|t| {
                                        if is_input {
                                            // Target is Output side
                                            if to_p < t.audio.outs {
                                                Kind::Audio
                                            } else {
                                                Kind::MIDI
                                            }
                                        } else {
                                            // Target is Input side
                                            if to_p < t.audio.ins {
                                                Kind::Audio
                                            } else {
                                                Kind::MIDI
                                            }
                                        }
                                    })
                                    .unwrap_or(Kind::Audio)
                            };

                            if kind == target_kind {
                                // Konverzija indeksa porta za Engine (oduzimanje Audio broja za MIDI)
                                let f_p_idx = if from_t == HW_IN_ID || from_t == HW_OUT_ID {
                                    from_p
                                } else {
                                    let t = &data.tracks[from_t];
                                    if kind == Kind::MIDI {
                                        from_p - (if is_input { t.audio.ins } else { t.audio.outs })
                                    } else {
                                        from_p
                                    }
                                };

                                let t_p_idx = if to_t == HW_IN_ID || to_t == HW_OUT_ID {
                                    to_p
                                } else {
                                    let t = &data.tracks[to_t];
                                    if kind == Kind::MIDI {
                                        to_p - (if is_input { t.audio.outs } else { t.audio.ins })
                                    } else {
                                        to_p
                                    }
                                };

                                // Finalna poruka: uvek formiramo (Output -> Input) za Engine
                                let (final_from, final_f_p, final_to, final_t_p) = if is_input {
                                    (to_name, t_p_idx, from_name, f_p_idx)
                                } else {
                                    (from_name, f_p_idx, to_name, t_p_idx)
                                };

                                return Some(Action::publish(Message::Request(
                                    EngineAction::Connect {
                                        from_track: final_from,
                                        from_port: final_f_p,
                                        to_track: final_to,
                                        to_port: final_t_p,
                                        kind,
                                    },
                                )));
                            }
                        }
                        return Some(Action::request_redraw());
                    }
                    data.moving_track = None;
                }

                Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                    let mut new_h = None;

                    // 1. Provera hover-a nad HW:IN portovima
                    if let Some(hw_in) = &data.hw_in {
                        let pos = Point::new(hw_margin, hw_margin);
                        for j in 0..hw_in.channels {
                            let py = pos.y
                                + 50.0
                                + ((hw_height - 60.0) / (hw_in.channels + 1) as f32)
                                    * (j + 1) as f32;
                            if cursor_position.distance(Point::new(pos.x + hw_width, py)) < 10.0 {
                                new_h = Some(Hovering::Port {
                                    track_idx: HW_IN_ID,
                                    port_idx: j,
                                    is_input: false,
                                });
                                break;
                            }
                        }
                    }

                    // 2. Provera hover-a nad HW:OUT portovima
                    if new_h.is_none() {
                        if let Some(hw_out) = &data.hw_out {
                            let pos = Point::new(bounds.width - hw_width - hw_margin, hw_margin);
                            for j in 0..hw_out.channels {
                                let py = pos.y
                                    + 50.0
                                    + ((hw_height - 60.0) / (hw_out.channels + 1) as f32)
                                        * (j + 1) as f32;
                                if cursor_position.distance(Point::new(pos.x, py)) < 10.0 {
                                    new_h = Some(Hovering::Port {
                                        track_idx: HW_OUT_ID,
                                        port_idx: j,
                                        is_input: true,
                                    });
                                    break;
                                }
                            }
                        }
                    }

                    // 3. Provera hover-a nad portovima i telom traka
                    if new_h.is_none() {
                        for (i, track) in data.tracks.iter().enumerate().rev() {
                            // Ulazni portovi
                            let t_ins = track.audio.ins + track.midi.ins;
                            for j in 0..t_ins {
                                let py = track.position.y
                                    + (size.height / (t_ins + 1) as f32) * (j + 1) as f32;
                                if cursor_position.distance(Point::new(track.position.x, py)) < 10.0
                                {
                                    new_h = Some(Hovering::Port {
                                        track_idx: i,
                                        port_idx: j,
                                        is_input: true,
                                    });
                                    break;
                                }
                            }
                            if new_h.is_some() {
                                break;
                            }

                            // Izlazni portovi
                            let t_outs = track.audio.outs + track.midi.outs;
                            for j in 0..t_outs {
                                let py = track.position.y
                                    + (size.height / (t_outs + 1) as f32) * (j + 1) as f32;
                                if cursor_position
                                    .distance(Point::new(track.position.x + size.width, py))
                                    < 10.0
                                {
                                    new_h = Some(Hovering::Port {
                                        track_idx: i,
                                        port_idx: j,
                                        is_input: false,
                                    });
                                    break;
                                }
                            }
                            if new_h.is_some() {
                                break;
                            }

                            // Telo trake
                            if Rectangle::new(track.position, size).contains(cursor_position) {
                                new_h = Some(Hovering::Track(i));
                                break;
                            }
                        }
                    }

                    let mut redraw_needed = false;

                    if let Some(ref mut conn) = data.connecting {
                        conn.point = cursor_position;
                        redraw_needed = true;
                    }
                    if let Some(mt) = data.moving_track.clone()
                        && let Some(t) = data.tracks.get_mut(mt.track_idx)
                    {
                        t.position.x = cursor_position.x - mt.offset_x;
                        t.position.y = cursor_position.y - mt.offset_y;
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
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let size = iced::Size::new(140.0, 80.0);
        let hw_width = 70.0;
        let hw_margin = 20.0;
        let hw_height = bounds.height - (hw_margin * 2.0);

        if let Ok(data) = self.state.try_read() {
            use crate::state::ConnectionViewSelection;

            // 1. CRTANJE SVIH POTVRĐENIH KONEKCIJA
            for (idx, conn) in data.connections.iter().enumerate() {
                // Pronalaženje START tačke
                let start_point = if conn.from_track == HW_IN_ID {
                    data.hw_in.as_ref().map(|hw| {
                        let py = hw_margin
                            + 50.0
                            + ((hw_height - 60.0) / (hw.channels + 1) as f32)
                                * (conn.from_port + 1) as f32;
                        Point::new(hw_margin + hw_width, py)
                    })
                } else {
                    data.tracks.get(conn.from_track).map(|t| {
                        let total_outs = t.audio.outs + t.midi.outs;
                        let py = t.position.y
                            + (size.height / (total_outs + 1) as f32) * (conn.from_port + 1) as f32;
                        Point::new(t.position.x + size.width, py)
                    })
                };

                // Pronalaženje END tačke
                let end_point = if conn.to_track == HW_OUT_ID {
                    data.hw_out.as_ref().map(|hw| {
                        let py = hw_margin
                            + 50.0
                            + ((hw_height - 60.0) / (hw.channels + 1) as f32)
                                * (conn.to_port + 1) as f32;
                        Point::new(bounds.width - hw_width - hw_margin, py)
                    })
                } else {
                    data.tracks.get(conn.to_track).map(|t| {
                        let total_ins = t.audio.ins + t.midi.ins;
                        let py = t.position.y
                            + (size.height / (total_ins + 1) as f32) * (conn.to_port + 1) as f32;
                        Point::new(t.position.x, py)
                    })
                };

                if let (Some(start), Some(end)) = (start_point, end_point) {
                    let dist = (end.x - start.x).abs() / 2.0;
                    let path = Path::new(|p| {
                        p.move_to(start);
                        p.bezier_curve_to(
                            Point::new(start.x + dist, start.y),
                            Point::new(end.x - dist, end.y),
                            end,
                        );
                    });

                    let is_selected = matches!(&data.connection_view_selection, ConnectionViewSelection::Connections(set) if set.contains(&idx));
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

            // 2. CRTANJE "GUMENE TRAKE" DOK SE VUČE MIŠEM
            if let Some(conn) = &data.connecting {
                let start_point = if conn.from_track == HW_IN_ID {
                    data.hw_in.as_ref().map(|hw| {
                        let py = hw_margin
                            + 50.0
                            + ((hw_height - 60.0) / (hw.channels + 1) as f32)
                                * (conn.from_port + 1) as f32;
                        Point::new(hw_margin + hw_width, py)
                    })
                } else if conn.from_track == HW_OUT_ID {
                    data.hw_out.as_ref().map(|hw| {
                        let py = hw_margin
                            + 50.0
                            + ((hw_height - 60.0) / (hw.channels + 1) as f32)
                                * (conn.from_port + 1) as f32;
                        Point::new(bounds.width - hw_width - hw_margin, py)
                    })
                } else {
                    data.tracks.get(conn.from_track).map(|t| {
                        if conn.is_input {
                            let py = t.position.y
                                + (size.height / (t.audio.ins + t.midi.ins + 1) as f32)
                                    * (conn.from_port + 1) as f32;
                            Point::new(t.position.x, py)
                        } else {
                            let py = t.position.y
                                + (size.height / (t.audio.outs + t.midi.outs + 1) as f32)
                                    * (conn.from_port + 1) as f32;
                            Point::new(t.position.x + size.width, py)
                        }
                    })
                };

                if let Some(start) = start_point {
                    let end = conn.point;
                    let dist = (end.x - start.x).abs() / 2.0;
                    let (c1, c2) = if conn.is_input {
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
                            .with_color(Color::from_rgba(1.0, 1.0, 1.0, 0.5))
                            .with_width(2.0),
                    );
                }
            }

            // 3. CRTANJE HW:IN (Sistem ulazi)
            if let Some(hw_in) = &data.hw_in {
                let pos = Point::new(hw_margin, hw_margin);
                let rect = Path::rectangle(pos, iced::Size::new(hw_width, hw_height));
                frame.fill(&rect, Color::from_rgb8(30, 45, 30));
                frame.stroke(
                    &rect,
                    canvas::Stroke::default()
                        .with_color(Color::from_rgb(0.0, 0.8, 0.4))
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
                        + ((hw_height - 60.0) / (hw_in.channels + 1) as f32) * (j + 1) as f32;
                    frame.fill_text(Text {
                        content: format!("{}", j + 1),
                        position: Point::new(pos.x + hw_width - 10.0, py),
                        color: Color::from_rgb(0.6, 0.6, 0.6),
                        size: 10.0.into(),
                        align_x: Horizontal::Right.into(),
                        align_y: Vertical::Center,
                        ..Default::default()
                    });
                    let h_port = Hovering::Port {
                        track_idx: HW_IN_ID,
                        port_idx: j,
                        is_input: false,
                    };
                    let h = data.hovering == Some(h_port.clone());

                    let can_highlight_port = if let Some(ref connecting) = data.connecting {
                        if let Some(hovered_kind) = Self::get_port_kind(&data, &h_port) {
                            h && connecting.kind == hovered_kind
                        } else {
                            false
                        }
                    } else {
                        h
                    };

                    frame.fill(
                        &Path::circle(Point::new(pos.x + hw_width, py), if can_highlight_port { 8.0 } else { 5.0 }),
                        Color::from_rgb(0.0, 1.0, 0.5),
                    );
                }
            }

            // 4. CRTANJE HW:OUT (Sistem izlazi)
            if let Some(hw_out) = &data.hw_out {
                let pos = Point::new(bounds.width - hw_width - hw_margin, hw_margin);
                let rect = Path::rectangle(pos, iced::Size::new(hw_width, hw_height));
                frame.fill(&rect, Color::from_rgb8(45, 30, 30));
                frame.stroke(
                    &rect,
                    canvas::Stroke::default()
                        .with_color(Color::from_rgb(0.8, 0.2, 0.2))
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
                        + ((hw_height - 60.0) / (hw_out.channels + 1) as f32) * (j + 1) as f32;
                    frame.fill_text(Text {
                        content: format!("{}", j + 1),
                        position: Point::new(pos.x + 10.0, py),
                        color: Color::from_rgb(0.6, 0.6, 0.6),
                        size: 10.0.into(),
                        align_x: Horizontal::Left.into(),
                        align_y: Vertical::Center,
                        ..Default::default()
                    });
                    let h_port = Hovering::Port {
                        track_idx: HW_OUT_ID,
                        port_idx: j,
                        is_input: true,
                    };
                    let h = data.hovering == Some(h_port.clone());

                    let can_highlight_port = if let Some(ref connecting) = data.connecting {
                        if let Some(hovered_kind) = Self::get_port_kind(&data, &h_port) {
                            h && connecting.kind == hovered_kind
                        } else {
                            false
                        }
                    } else {
                        h
                    };

                    frame.fill(
                        &Path::circle(Point::new(pos.x, py), if can_highlight_port { 8.0 } else { 5.0 }),
                        Color::from_rgb(1.0, 0.3, 0.3),
                    );
                }
            }

            // 5. CRTANJE TRAKA
            for (i, track) in data.tracks.iter().enumerate() {
                let pos = track.position;
                let path = Path::rectangle(pos, size);
                frame.fill(&path, Color::from_rgb8(45, 45, 45));

                let is_h = data.hovering == Some(Hovering::Track(i));
                let is_s = matches!(&data.connection_view_selection, ConnectionViewSelection::Tracks(set) if set.contains(&i));
                let (sc, sw) = if is_s {
                    (Color::from_rgb(1.0, 1.0, 0.0), 3.0)
                } else if is_h {
                    (Color::from_rgb8(120, 120, 120), 1.0)
                } else {
                    (Color::from_rgb8(80, 80, 80), 1.0)
                };
                frame.stroke(
                    &path,
                    canvas::Stroke::default().with_color(sc).with_width(sw),
                );

                let total_ins = track.audio.ins + track.midi.ins;
                for j in 0..total_ins {
                    let py = pos.y + (size.height / (total_ins + 1) as f32) * (j + 1) as f32;
                    let c = if j < track.audio.ins {
                        Color::from_rgb(0.2, 0.5, 1.0)
                    } else {
                        Color::from_rgb(1.0, 0.6, 0.0)
                    };
                    let h_port = Hovering::Port {
                        track_idx: i,
                        port_idx: j,
                        is_input: true,
                    };
                    let h = data.hovering == Some(h_port.clone());

                    let can_highlight_port = if let Some(ref connecting) = data.connecting {
                        if let Some(hovered_kind) = Self::get_port_kind(&data, &h_port) {
                            h && connecting.kind == hovered_kind
                        } else {
                            false
                        }
                    } else {
                        h
                    };

                    frame.fill(
                        &Path::circle(Point::new(pos.x, py), if can_highlight_port { 7.0 } else { 4.0 }),
                        c,
                    );
                }

                let total_outs = track.audio.outs + track.midi.outs;
                for j in 0..total_outs {
                    let py = pos.y + (size.height / (total_outs + 1) as f32) * (j + 1) as f32;
                    let c = if j < track.audio.outs {
                        Color::from_rgb(0.2, 0.5, 1.0)
                    } else {
                        Color::from_rgb(1.0, 0.6, 0.0)
                    };
                    let h_port = Hovering::Port {
                        track_idx: i,
                        port_idx: j,
                        is_input: false,
                    };
                    let h = data.hovering == Some(h_port.clone());

                    let can_highlight_port = if let Some(ref connecting) = data.connecting {
                        if let Some(hovered_kind) = Self::get_port_kind(&data, &h_port) {
                            h && connecting.kind == hovered_kind
                        } else {
                            false
                        }
                    } else {
                        h
                    };

                    frame.fill(
                        &Path::circle(
                            Point::new(pos.x + size.width, py),
                            if can_highlight_port { 7.0 } else { 4.0 },
                        ),
                        c,
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
