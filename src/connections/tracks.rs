use crate::{
    connections::colors::{audio_port_color, aux_port_color, midi_port_color},
    connections::port_kind::{can_connect_kinds, should_highlight_port},
    connections::ports::hover_radius,
    connections::selection::is_bezier_hit,
    message::Message,
    state::{
        Connecting, ConnectionViewSelection, HW_IN_ID, HW_OUT_ID, Hovering, MIDI_HW_IN_ID,
        MIDI_HW_OUT_ID, MovingTrack, State, StateData,
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
use maolan_engine::{kind::Kind, message::Action as EngineAction};
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

    fn track_box_size(track: &crate::state::Track) -> iced::Size {
        let side_ins = track.primary_audio_ins() + track.midi.ins;
        let side_outs = track.primary_audio_outs() + track.midi.outs;
        let max_ports = side_ins.max(side_outs).max(1);
        let port_pitch = 8.0_f32;
        // +1 keeps a small top/bottom margin with the existing (n+1) spacing formula.
        let adaptive_h = (max_ports as f32 + 1.0) * port_pitch;
        iced::Size::new(140.0, adaptive_h.max(80.0))
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

    fn track_port_edge(track: &crate::state::Track, flat_port: usize, is_input: bool) -> TrackPortEdge {
        let (kind, engine_port) = Self::track_port_to_engine_index(track, flat_port, is_input);
        match (is_input, kind) {
            (true, Kind::Audio) if engine_port >= track.primary_audio_ins() => TrackPortEdge::Bottom,
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
            TrackPortEdge::Left | TrackPortEdge::Right => match Self::track_port_kind(track, flat_port, is_input) {
                Kind::Audio => audio_port_color(),
                Kind::MIDI => midi_port_color(),
            },
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
                        let track_size = Self::track_box_size(track);
                        let t_ins = track.primary_audio_ins() + track.midi.ins + track.return_count();
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
                                let track_size = Self::track_box_size(t);
                                let port_idx = Self::connection_port_index(
                                    t,
                                    conn.kind,
                                    conn.from_port,
                                    false,
                                );
                                Self::track_output_port_position(
                                    t,
                                    port_idx,
                                    t.position,
                                    track_size,
                                )
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
                                let track_size = Self::track_box_size(track);
                                let total_outs =
                                    track.primary_audio_outs() + track.midi.outs + track.send_count();
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
                                let track_size = Self::track_box_size(track);
                                let total_ins =
                                    track.primary_audio_ins() + track.midi.ins + track.return_count();
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
                                    .map(|t| Self::track_port_kind(t, to_p, !is_input))
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
                                    Self::track_port_to_engine_index(t, from_p, is_input).1
                                };

                                let t_p_idx = if to_t_name == HW_IN_ID
                                    || to_t_name == HW_OUT_ID
                                    || is_target_midi_hw
                                {
                                    to_p
                                } else {
                                    let t = target_track_option.unwrap();
                                    Self::track_port_to_engine_index(t, to_p, !is_input).1
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
                            let track_size = Self::track_box_size(track);
                            let t_ins =
                                track.primary_audio_ins() + track.midi.ins + track.return_count();
                            for j in 0..t_ins {
                                let port_pos = Self::track_port_position(
                                    track,
                                    j,
                                    track.position,
                                    track_size,
                                );
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
        let hw_width = 70.0;
        let midi_hw_box_h = 24.0;
        let midi_hw_box_gap = 6.0;
        let cursor_position = cursor.position_in(bounds);
        let rgb8 = |r: u8, g: u8, b: u8| Color::from_rgb8(r, g, b);
        let draw_gradient_box = |frame: &mut Frame, pos: Point, size: iced::Size, base: Color| {
            frame.fill(&Path::rectangle(pos, size), base);

            // Subtle faux gradient: soft top highlight + bottom shadow.
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
        let node_fill = rgb8(36, 45, 68);
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
                        let track_size = Self::track_box_size(t);
                        let port_idx =
                            Self::connection_port_index(t, conn.kind, conn.from_port, false);
                        Self::track_output_port_position(t, port_idx, t.position, track_size)
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
                        let track_size = Self::track_box_size(t);
                        if conn.is_input {
                            Self::track_port_position(t, conn.from_port, t.position, track_size)
                        } else {
                            Self::track_output_port_position(
                                t,
                                conn.from_port,
                                t.position,
                                track_size,
                            )
                        }
                    })
                };

                if let Some(start) = start_point {
                    let end = conn.point;
                    let start_edge = if let Some(track) = start_track_option {
                        Self::track_port_edge(track, conn.from_port, conn.is_input)
                    } else {
                        match conn.from_track.as_str() {
                            HW_IN_ID => TrackPortEdge::Right,
                            HW_OUT_ID => TrackPortEdge::Left,
                            _ if conn.from_track.starts_with(MIDI_HW_IN_ID) => TrackPortEdge::Right,
                            _ if conn.from_track.starts_with(MIDI_HW_OUT_ID) => TrackPortEdge::Left,
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

            if let Some(hw_in) = &data.hw_in {
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

            if let Some(hw_out) = &data.hw_out {
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

            for track in data.tracks.iter() {
                let pos = track.position;
                let size = Self::track_box_size(track);
                let path = Path::rectangle(pos, size);
                draw_true_gradient_box(&mut frame, pos, size, node_fill);

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

                let total_outs =
                    track.primary_audio_outs() + track.midi.outs + track.send_count();
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
        }
        vec![frame.into_geometry()]
    }
}
