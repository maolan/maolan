use crate::{
    connections::colors::{audio_port_color, midi_port_color},
    connections::port_kind::{can_connect_kinds, should_highlight_port},
    connections::ports::hover_radius,
    connections::selection::is_bezier_hit,
    message::Message,
    state::{
        Connecting, HW_IN_ID, HW_OUT_ID, Hovering, MIDI_HW_IN_ID, MIDI_HW_OUT_ID, MovingTrack,
        State, StateData,
    },
    ui_timing::DOUBLE_CLICK,
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
use maolan_engine::{kind::Kind, message::Action as EngineAction};
use std::time::Instant;

pub struct Graph {
    state: State,
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
                    Some(Kind::Audio)
                } else if track_idx.starts_with(MIDI_HW_IN_ID)
                    || track_idx.starts_with(MIDI_HW_OUT_ID)
                {
                    Some(Kind::MIDI)
                } else {
                    data.tracks.iter().find(|t| t.name == *track_idx).map(|t| {
                        if *is_input {
                            if *port_idx < t.audio.ins {
                                Kind::Audio
                            } else {
                                Kind::MIDI
                            }
                        } else if *port_idx < t.audio.outs {
                            Kind::Audio
                        } else {
                            Kind::MIDI
                        }
                    })
                }
            }
            _ => None,
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
                track.audio.ins
            } else {
                track.audio.outs
            }
        } else {
            port
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
        let size = iced::Size::new(140.0, 80.0);
        let hw_width = 70.0;
        let midi_hw_box_h = 24.0;
        let midi_hw_box_gap = 6.0;

        if let Ok(mut data) = self.state.try_write() {
            match event {
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                    let ctrl = data.ctrl;
                    let mut pending_action: Option<Action<Message>> = None;

                    if let Some(hw_in) = &data.hw_in {
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
                    }

                    if let Some(hw_out) = &data.hw_out {
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
                    }

                    for (idx, device) in data.opened_midi_in_hw.iter().enumerate() {
                        let label = Self::midi_device_label(&data, device);
                        let default_rect =
                            Self::default_midi_in_rect(idx, &label, midi_hw_box_h, midi_hw_box_gap);
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

                    for track in data.tracks.iter().rev() {
                        let track_name = track.name.clone();
                        let track_pos = track.position;
                        let track_audio_ins = track.audio.ins;
                        let track_audio_outs = track.audio.outs;
                        let track_midi_ins = track.midi.ins;
                        let track_midi_outs = track.midi.outs;
                        let t_ins = track_audio_ins + track_midi_ins;
                        for j in 0..t_ins {
                            let py =
                                track_pos.y + (size.height / (t_ins + 1) as f32) * (j + 1) as f32;
                            if cursor_position.distance(Point::new(track_pos.x, py)) < 10.0 {
                                data.connecting = Some(Connecting {
                                    from_track: track_name.clone(),
                                    from_port: j,
                                    kind: if j < track_audio_ins {
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

                        let t_outs = track_audio_outs + track_midi_outs;
                        for j in 0..t_outs {
                            let py =
                                track_pos.y + (size.height / (t_outs + 1) as f32) * (j + 1) as f32;
                            if cursor_position.distance(Point::new(track_pos.x + size.width, py))
                                < 10.0
                            {
                                data.connecting = Some(Connecting {
                                    from_track: track_name.clone(),
                                    from_port: j,
                                    kind: if j < track_audio_outs {
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

                        if Rectangle::new(track_pos, size).contains(cursor_position) {
                            let now = Instant::now();
                            if let Some((last_track, last_time)) =
                                &data.connections_last_track_click
                                && *last_track == track_name
                                && now.duration_since(*last_time) <= DOUBLE_CLICK
                            {
                                data.connections_last_track_click = None;
                                return Some(Action::publish(Message::OpenTrackPlugins(
                                    track_name.clone(),
                                )));
                            }
                            data.connections_last_track_click = Some((track_name.clone(), now));

                            if ctrl {
                                pending_action = Some(Action::publish(
                                    Message::ConnectionViewSelectTrack(track_name.clone()),
                                ));
                            } else {
                                let moving_track_data = MovingTrack {
                                    track_idx: track_name.clone(),
                                    offset_x: cursor_position.x - track_pos.x,
                                    offset_y: cursor_position.y - track_pos.y,
                                };
                                pending_action =
                                    Some(Action::publish(Message::StartMovingTrackAndSelect(
                                        moving_track_data,
                                        track_name.clone(),
                                    )));
                            }
                            break;
                        }
                    }

                    let mut clicked_connection = None;
                    for (idx, conn) in data.connections.iter().enumerate() {
                        let start_track_option =
                            data.tracks.iter().find(|t| t.name == conn.from_track);
                        let end_track_option = data.tracks.iter().find(|t| t.name == conn.to_track);

                        let start_point = if conn.from_track == HW_IN_ID {
                            data.hw_in.as_ref().map(move |hw_in| {
                                let py = 50.0
                                    + ((bounds.height - 60.0) / (hw_in.channels + 1) as f32)
                                        * (conn.from_port + 1) as f32;
                                Point::new(hw_width, py)
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
                        } else {
                            start_track_option.map(|t| {
                                let total_outs = t.audio.outs + t.midi.outs;
                                let port_idx = Self::connection_port_index(
                                    t,
                                    conn.kind,
                                    conn.from_port,
                                    false,
                                );
                                let py = t.position.y
                                    + (size.height / (total_outs + 1) as f32)
                                        * (port_idx + 1) as f32;
                                Point::new(t.position.x + size.width, py)
                            })
                        };

                        let end_point = if conn.to_track == HW_OUT_ID {
                            data.hw_out.as_ref().map(move |hw_out| {
                                let py = 50.0
                                    + ((bounds.height - 60.0) / (hw_out.channels + 1) as f32)
                                        * (conn.to_port + 1) as f32;
                                Point::new(bounds.width - hw_width, py)
                            })
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
                                let total_ins = t.audio.ins + t.midi.ins;
                                let port_idx =
                                    Self::connection_port_index(t, conn.kind, conn.to_port, true);
                                let py = t.position.y
                                    + (size.height / (total_ins + 1) as f32)
                                        * (port_idx + 1) as f32;
                                Point::new(t.position.x, py)
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
                    if let Some(conn) = data.connecting.take() {
                        let from_t = conn.from_track;
                        let from_p = conn.from_port;
                        let kind = conn.kind;
                        let is_input = conn.is_input;
                        let mut target_port = None;

                        if is_input {
                            for track in data.tracks.iter() {
                                let total_outs = track.audio.outs + track.midi.outs;
                                for j in 0..total_outs {
                                    let py = track.position.y
                                        + (size.height / (total_outs + 1) as f32) * (j + 1) as f32;
                                    let port_pos = Point::new(track.position.x + size.width, py);
                                    if cursor_position.distance(port_pos) < 10.0 {
                                        target_port = Some((track.name.clone(), j));
                                        break;
                                    }
                                }
                                if target_port.is_some() {
                                    break;
                                }
                            }

                            if target_port.is_none()
                                && from_t != HW_IN_ID
                                && let Some(hw_in) = &data.hw_in
                            {
                                for j in 0..hw_in.channels {
                                    let py = 50.0
                                        + ((bounds.height - 60.0) / (hw_in.channels + 1) as f32)
                                            * (j + 1) as f32;
                                    if cursor_position.distance(Point::new(hw_width, py)) < 10.0 {
                                        target_port = Some((HW_IN_ID.to_string(), j));
                                        break;
                                    }
                                }
                            }

                            if kind == Kind::MIDI && target_port.is_none() {
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
                                let total_ins = track.audio.ins + track.midi.ins;
                                for j in 0..total_ins {
                                    let py = track.position.y
                                        + (size.height / (total_ins + 1) as f32) * (j + 1) as f32;
                                    let port_pos = Point::new(track.position.x, py);
                                    if cursor_position.distance(port_pos) < 10.0 {
                                        target_port = Some((track.name.clone(), j));
                                        break;
                                    }
                                }
                                if target_port.is_some() {
                                    break;
                                }
                            }

                            if target_port.is_none()
                                && from_t != HW_OUT_ID
                                && let Some(hw_out) = &data.hw_out
                            {
                                for j in 0..hw_out.channels {
                                    let py = 50.0
                                        + ((bounds.height - 60.0) / (hw_out.channels + 1) as f32)
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

                            if kind == Kind::MIDI && target_port.is_none() {
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

                        if let Some((to_t_name, to_p)) = target_port {
                            let target_track_option =
                                data.tracks.iter().find(|t| t.name == to_t_name);

                            let is_target_midi_hw = to_t_name.starts_with(MIDI_HW_IN_ID)
                                || to_t_name.starts_with(MIDI_HW_OUT_ID);
                            let target_kind = if to_t_name == HW_IN_ID || to_t_name == HW_OUT_ID {
                                Kind::Audio
                            } else if is_target_midi_hw {
                                Kind::MIDI
                            } else {
                                target_track_option
                                    .map(|t| {
                                        if is_input {
                                            if to_p < t.audio.outs {
                                                Kind::Audio
                                            } else {
                                                Kind::MIDI
                                            }
                                        } else if to_p < t.audio.ins {
                                            Kind::Audio
                                        } else {
                                            Kind::MIDI
                                        }
                                    })
                                    .unwrap_or(Kind::Audio)
                            };

                            if can_connect_kinds(kind, target_kind) {
                                let is_source_hw_audio = from_t == HW_IN_ID || from_t == HW_OUT_ID;
                                let is_source_midi_hw = from_t.starts_with(MIDI_HW_IN_ID)
                                    || from_t.starts_with(MIDI_HW_OUT_ID);
                                let f_p_idx = if is_source_hw_audio || is_source_midi_hw {
                                    from_p
                                } else {
                                    let t = data.tracks.iter().find(|t| t.name == from_t).unwrap();
                                    if kind == Kind::MIDI {
                                        from_p - (if is_input { t.audio.ins } else { t.audio.outs })
                                    } else {
                                        from_p
                                    }
                                };

                                let t_p_idx = if to_t_name == HW_IN_ID
                                    || to_t_name == HW_OUT_ID
                                    || is_target_midi_hw
                                {
                                    to_p
                                } else {
                                    let t = target_track_option.unwrap();
                                    if kind == Kind::MIDI {
                                        to_p - (if is_input { t.audio.outs } else { t.audio.ins })
                                    } else {
                                        to_p
                                    }
                                };

                                let (final_from, final_f_p, final_to, final_t_p) = if is_input {
                                    (to_t_name, t_p_idx, from_t, f_p_idx)
                                } else {
                                    (from_t, f_p_idx, to_t_name, t_p_idx)
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

                    if let Some(hw_in) = &data.hw_in {
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

                    if new_h.is_none() {
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

                    if new_h.is_none() {
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

                    if new_h.is_none() {
                        for track in data.tracks.iter().rev() {
                            let t_ins = track.audio.ins + track.midi.ins;
                            for j in 0..t_ins {
                                let py = track.position.y
                                    + (size.height / (t_ins + 1) as f32) * (j + 1) as f32;
                                if cursor_position.distance(Point::new(track.position.x, py)) < 10.0
                                {
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

                            let t_outs = track.audio.outs + track.midi.outs;
                            for j in 0..t_outs {
                                let py = track.position.y
                                    + (size.height / (t_outs + 1) as f32) * (j + 1) as f32;
                                if cursor_position
                                    .distance(Point::new(track.position.x + size.width, py))
                                    < 10.0
                                {
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

                            if Rectangle::new(track.position, size).contains(cursor_position) {
                                new_h = Some(Hovering::Track(track.name.clone()));
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
                            t.position.x = cursor_position.x - mt.offset_x;
                            t.position.y = cursor_position.y - mt.offset_y;
                            redraw_needed = true;
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
        let size = iced::Size::new(140.0, 80.0);
        let hw_width = 70.0;
        let midi_hw_box_h = 24.0;
        let midi_hw_box_gap = 6.0;
        let cursor_position = cursor.position_in(bounds);

        if let Ok(data) = self.state.try_read() {
            use crate::state::ConnectionViewSelection;

            for (idx, conn) in data.connections.iter().enumerate() {
                let start_track_option = data.tracks.iter().find(|t| t.name == conn.from_track);
                let end_track_option = data.tracks.iter().find(|t| t.name == conn.to_track);

                let start_point = if conn.from_track == HW_IN_ID {
                    data.hw_in.as_ref().map(move |hw_in| {
                        let py = 50.0
                            + ((bounds.height - 60.0) / (hw_in.channels + 1) as f32)
                                * (conn.from_port + 1) as f32;
                        Point::new(hw_width, py)
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
                } else {
                    start_track_option.map(|t| {
                        let port_idx =
                            Self::connection_port_index(t, conn.kind, conn.from_port, false);
                        let total_outs = t.audio.outs + t.midi.outs;
                        let py = t.position.y
                            + (size.height / (total_outs + 1) as f32) * (port_idx + 1) as f32;
                        Point::new(t.position.x + size.width, py)
                    })
                };

                let end_point = if conn.to_track == HW_OUT_ID {
                    data.hw_out.as_ref().map(move |hw_out| {
                        let py = 50.0
                            + ((bounds.height - 60.0) / (hw_out.channels + 1) as f32)
                                * (conn.to_port + 1) as f32;
                        Point::new(bounds.width - hw_width, py)
                    })
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
                        let port_idx =
                            Self::connection_port_index(t, conn.kind, conn.to_port, true);
                        let total_ins = t.audio.ins + t.midi.ins;
                        let py = t.position.y
                            + (size.height / (total_ins + 1) as f32) * (port_idx + 1) as f32;
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
                    let is_hovered = cursor_position
                        .is_some_and(|cursor| is_bezier_hit(start, end, cursor, 20, 10.0));
                    let (color, width) = if is_selected {
                        (Color::from_rgb(1.0, 1.0, 0.0), 4.0)
                    } else if is_hovered {
                        let c = match conn.kind {
                            Kind::Audio => Color::from_rgb(0.2, 0.5, 1.0),
                            Kind::MIDI => Color::from_rgb(1.0, 0.6, 0.0),
                        };
                        (c, 3.0)
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

            if let Some(conn) = &data.connecting {
                let start_track_option = data.tracks.iter().find(|t| t.name == conn.from_track);

                let start_point = if conn.from_track == HW_IN_ID {
                    data.hw_in.as_ref().map(move |hw_in| {
                        let py = 50.0
                            + ((bounds.height - 60.0) / (hw_in.channels + 1) as f32)
                                * (conn.from_port + 1) as f32;
                        Point::new(hw_width, py)
                    })
                } else if conn.from_track == HW_OUT_ID {
                    data.hw_out.as_ref().map(move |hw_out| {
                        let py = 50.0
                            + ((bounds.height - 60.0) / (hw_out.channels + 1) as f32)
                                * (conn.from_port + 1) as f32;
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

            if let Some(hw_in) = &data.hw_in {
                let pos = Point::new(0.0, 0.0);
                let rect = Path::rectangle(pos, iced::Size::new(hw_width, bounds.height));
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
                        + ((bounds.height - 60.0) / (hw_in.channels + 1) as f32) * (j + 1) as f32;
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

            if let Some(hw_out) = &data.hw_out {
                let pos = Point::new(bounds.width - hw_width, 0.0);
                let rect = Path::rectangle(pos, iced::Size::new(hw_width, bounds.height));
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
                        + ((bounds.height - 60.0) / (hw_out.channels + 1) as f32) * (j + 1) as f32;
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
                    Color::from_rgb8(45, 40, 20)
                } else {
                    Color::from_rgb8(28, 24, 14)
                };
                let stroke_color = if is_selected {
                    midi_port_color()
                } else {
                    Color::from_rgb(0.45, 0.28, 0.12)
                };
                frame.fill(&rect, fill_color);
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
                    Color::from_rgb8(45, 40, 20)
                } else {
                    Color::from_rgb8(28, 24, 14)
                };
                let stroke_color = if is_selected {
                    midi_port_color()
                } else {
                    Color::from_rgb(0.45, 0.28, 0.12)
                };
                frame.fill(&rect, fill_color);
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

            for track in data.tracks.iter() {
                let pos = track.position;
                let path = Path::rectangle(pos, size);
                frame.fill(&path, Color::from_rgb8(45, 45, 45));

                let is_h = data.hovering == Some(Hovering::Track(track.name.clone()));
                let is_s = matches!(&data.connection_view_selection, ConnectionViewSelection::Tracks(set) if set.contains(&track.name));
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
                        audio_port_color()
                    } else {
                        midi_port_color()
                    };
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
                        &Path::circle(Point::new(pos.x, py), hover_radius(4.0, can_highlight_port)),
                        c,
                    );
                }

                let total_outs = track.audio.outs + track.midi.outs;
                for j in 0..total_outs {
                    let py = pos.y + (size.height / (total_outs + 1) as f32) * (j + 1) as f32;
                    let c = if j < track.audio.outs {
                        audio_port_color()
                    } else {
                        midi_port_color()
                    };
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
                        &Path::circle(
                            Point::new(pos.x + size.width, py),
                            hover_radius(4.0, can_highlight_port),
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
