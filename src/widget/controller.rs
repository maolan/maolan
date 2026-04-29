use crate::{
    consts::message_lists::{PIANO_NRPN_KIND_ALL, PIANO_RPN_KIND_ALL},
    message::{Message, PianoControllerLane},
    state::{PianoControllerPoint, PianoNote, PianoSysExPoint, State},
    widget::piano::PianoRollInteraction,
};
use iced::{
    Color, Event, Point, Rectangle, Renderer, Size, Theme, mouse,
    widget::canvas::{self, Action as CanvasAction, Frame, Geometry, Path, Program},
};
use maolan_widgets::controller::{
    controller_lane_line_count, lane_controller_events, nrpn_param, rpn_param,
};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub struct ControllerHitTest<'a> {
    pub lane: PianoControllerLane,
    pub pane_h: f32,
    pub pps: f32,
    pub selected_row: Option<usize>,
    pub controllers: &'a [PianoControllerPoint],
}

#[derive(Debug, Clone, Copy)]
pub enum ControllerAdjustTarget {
    Controller(usize),
    Velocity(usize),
}

#[derive(Debug, Clone, Copy)]
pub enum ControllerEraseTarget {
    Controller(usize),
    Velocity(usize),
    ControllerRange,
}

#[derive(Default, Debug)]
pub struct ControllerRollInteractionState {
    pub mode: ControllerDragMode,
    pub last_sysex_click: Option<(usize, Instant)>,
}

#[derive(Default, Debug, Clone, Copy)]
pub enum ControllerDragMode {
    #[default]
    None,
    Adjusting {
        target: ControllerAdjustTarget,
        start_y: f32,
        start_value: u8,
        current_y: f32,
    },
    Drawing {
        start: Point,
        current: Point,
    },
    DraggingSysEx {
        index: usize,
        original_sample: usize,
        start_x: f32,
        current_x: f32,
    },
    Erasing {
        start: Point,
        current: Point,
        target: ControllerEraseTarget,
    },
}

#[derive(Debug)]
pub struct ControllerRollInteraction {
    pub state: State,
    pub pixels_per_sample: f32,
    pub sample_rate_hz: f64,
    pub samples_per_bar: f32,
}

impl ControllerRollInteraction {
    pub fn new(
        state: State,
        pixels_per_sample: f32,
        sample_rate_hz: f64,
        samples_per_bar: f32,
    ) -> Self {
        Self {
            state,
            pixels_per_sample,
            sample_rate_hz,
            samples_per_bar,
        }
    }

    fn controller_at_position(&self, position: Point, hit: ControllerHitTest<'_>) -> Option<usize> {
        let mut best: Option<(usize, f32)> = None;
        for (idx, row) in lane_controller_events(hit.lane, hit.controllers) {
            if let Some(selected_row) = hit.selected_row
                && row != selected_row
            {
                continue;
            }
            let ctrl = &hit.controllers[idx];
            let stem_h = (hit.pane_h * (ctrl.value as f32 / 127.0)).max(1.0);
            let stem_y = hit.pane_h - stem_h;
            if position.y < stem_y || position.y > hit.pane_h {
                continue;
            }
            let x = ctrl.sample as f32 * hit.pps;
            let dx = (position.x - x).abs();
            if dx > 4.0 {
                continue;
            }
            match best {
                Some((_, best_dx)) if dx >= best_dx => {}
                _ => best = Some((idx, dx)),
            }
        }
        best.map(|(idx, _)| idx)
    }

    fn velocity_note_at_position(
        &self,
        position: Point,
        row_h: f32,
        pps: f32,
        notes: &[PianoNote],
    ) -> Option<usize> {
        let mut best: Option<(usize, f32)> = None;
        for (idx, note) in notes.iter().enumerate() {
            let row = usize::from(127_u8.saturating_sub(note.velocity));
            let y0 = row as f32 * row_h;
            let y1 = y0 + row_h;
            if position.y < y0 || position.y > y1 {
                continue;
            }
            let x = note.start_sample as f32 * pps;
            let dx = (position.x - x).abs();
            if dx > 5.0 {
                continue;
            }
            match best {
                Some((_, best_dx)) if dx >= best_dx => {}
                _ => best = Some((idx, dx)),
            }
        }
        best.map(|(idx, _)| idx)
    }

    fn sysex_at_position(
        &self,
        position: Point,
        pps: f32,
        sysexes: &[PianoSysExPoint],
    ) -> Option<usize> {
        let mut best: Option<(usize, f32)> = None;
        for (idx, ev) in sysexes.iter().enumerate() {
            let x = ev.sample as f32 * pps;
            let dx = (position.x - x).abs();
            if dx > 5.0 {
                continue;
            }
            match best {
                Some((_, best_dx)) if dx >= best_dx => {}
                _ => best = Some((idx, dx)),
            }
        }
        best.map(|(idx, _)| idx)
    }

    fn controller_indices_in_sample_range(
        &self,
        start_x: f32,
        end_x: f32,
        pps: f32,
        hit: ControllerHitTest<'_>,
    ) -> Vec<usize> {
        let sample_start = ((start_x.min(end_x) / pps).floor().max(0.0)) as usize;
        let sample_end = ((start_x.max(end_x) / pps).ceil().max(0.0)) as usize;
        let mut out = Vec::new();
        for (idx, row) in lane_controller_events(hit.lane, hit.controllers) {
            if let Some(selected_row) = hit.selected_row
                && row != selected_row
            {
                continue;
            }
            let ctrl = &hit.controllers[idx];
            if ctrl.sample >= sample_start && ctrl.sample <= sample_end {
                out.push(idx);
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }
}

impl Program<Message> for ControllerRollInteraction {
    type State = ControllerRollInteractionState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<CanvasAction<Message>> {
        let app_state = self.state.blocking_read();
        let roll = app_state.piano.as_ref()?;
        let lane = app_state.piano_controller_lane;
        let selected_row = match lane {
            PianoControllerLane::Controller => Some(usize::from(
                127_u8.saturating_sub(app_state.piano_controller_kind),
            )),
            PianoControllerLane::Rpn => PIANO_RPN_KIND_ALL
                .iter()
                .position(|kind| *kind == app_state.piano_rpn_kind),
            PianoControllerLane::Nrpn => PIANO_NRPN_KIND_ALL
                .iter()
                .position(|kind| *kind == app_state.piano_nrpn_kind),
            PianoControllerLane::Velocity => None,
            PianoControllerLane::SysEx => None,
        };
        let controllers = &roll.controllers;
        let sysexes = &roll.sysexes;
        let notes = &roll.notes;
        let clip_len = roll.clip_length_samples;
        let zoom_x = app_state.piano_zoom_x;
        let pps = (self.pixels_per_sample * zoom_x).max(0.0001);
        let row_h = (bounds.height / controller_lane_line_count(lane).max(1) as f32).max(1.0);

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let position = cursor.position_in(bounds)?;
                if matches!(lane, PianoControllerLane::SysEx) {
                    if let Some(index) = self.sysex_at_position(position, pps, sysexes) {
                        let now = Instant::now();
                        let double_click = state
                            .last_sysex_click
                            .map(|(last_idx, last_time)| {
                                last_idx == index
                                    && now.duration_since(last_time) <= Duration::from_millis(350)
                            })
                            .unwrap_or(false);
                        state.last_sysex_click = Some((index, now));
                        if double_click {
                            state.mode = ControllerDragMode::None;
                            return Some(
                                CanvasAction::publish(Message::PianoSysExOpenEditor(Some(index)))
                                    .and_capture(),
                            );
                        }
                        let original_sample = sysexes.get(index).map(|s| s.sample).unwrap_or(0);
                        state.mode = ControllerDragMode::DraggingSysEx {
                            index,
                            original_sample,
                            start_x: position.x,
                            current_x: position.x,
                        };
                        return Some(
                            CanvasAction::publish(Message::PianoSysExSelect(Some(index)))
                                .and_capture(),
                        );
                    }
                    state.last_sysex_click = None;
                }
                let target = if matches!(lane, PianoControllerLane::Velocity) {
                    self.velocity_note_at_position(position, row_h, pps, notes)
                        .and_then(|idx| notes.get(idx).map(|n| (idx, n.velocity)))
                        .map(|(idx, velocity)| (ControllerAdjustTarget::Velocity(idx), velocity))
                } else {
                    self.controller_at_position(
                        position,
                        ControllerHitTest {
                            lane,
                            pane_h: bounds.height,
                            pps,
                            selected_row,
                            controllers,
                        },
                    )
                    .and_then(|idx| controllers.get(idx).map(|c| (idx, c.value)))
                    .map(|(idx, value)| (ControllerAdjustTarget::Controller(idx), value))
                };
                if let Some((target, start_value)) = target {
                    state.mode = ControllerDragMode::Adjusting {
                        target,
                        start_y: position.y,
                        start_value,
                        current_y: position.y,
                    };
                    return Some(CanvasAction::request_redraw().and_capture());
                }
                None
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle)) => {
                let position = cursor.position_in(bounds)?;
                if matches!(lane, PianoControllerLane::SysEx) {
                    return None;
                }
                let target = if matches!(lane, PianoControllerLane::Velocity) {
                    self.velocity_note_at_position(position, row_h, pps, notes)
                        .map(ControllerEraseTarget::Velocity)
                } else {
                    self.controller_at_position(
                        position,
                        ControllerHitTest {
                            lane,
                            pane_h: bounds.height,
                            pps,
                            selected_row,
                            controllers,
                        },
                    )
                    .map(ControllerEraseTarget::Controller)
                    .or(Some(ControllerEraseTarget::ControllerRange))
                };
                if let Some(target) = target {
                    state.mode = ControllerDragMode::Erasing {
                        start: position,
                        current: position,
                        target,
                    };
                    return Some(CanvasAction::request_redraw().and_capture());
                }
                None
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if matches!(lane, PianoControllerLane::Velocity) {
                    return None;
                }
                let position = cursor.position_in(bounds)?;
                let controller_index = self.controller_at_position(
                    position,
                    ControllerHitTest {
                        lane,
                        pane_h: bounds.height,
                        pps,
                        selected_row,
                        controllers,
                    },
                )?;
                let value_delta = PianoRollInteraction::velocity_delta_from_scroll(delta);
                if value_delta == 0 {
                    return None;
                }
                Some(
                    CanvasAction::publish(Message::PianoAdjustController {
                        controller_index,
                        delta: value_delta,
                    })
                    .and_capture(),
                )
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some(position) = cursor.position_in(bounds)
                    && let ControllerDragMode::Erasing { start, target, .. } = state.mode
                {
                    state.mode = ControllerDragMode::Erasing {
                        start,
                        current: position,
                        target,
                    };
                    return Some(CanvasAction::request_redraw().and_capture());
                }
                if let Some(position) = cursor.position_in(bounds)
                    && let ControllerDragMode::Adjusting {
                        target,
                        start_y,
                        start_value,
                        ..
                    } = state.mode
                {
                    state.mode = ControllerDragMode::Adjusting {
                        target,
                        start_y,
                        start_value,
                        current_y: position.y,
                    };
                    return Some(CanvasAction::request_redraw().and_capture());
                }
                if let Some(position) = cursor.position_in(bounds)
                    && let ControllerDragMode::Drawing { start, .. } = state.mode
                {
                    state.mode = ControllerDragMode::Drawing {
                        start,
                        current: position,
                    };
                    return Some(CanvasAction::request_redraw().and_capture());
                }
                if let Some(position) = cursor.position_in(bounds)
                    && let ControllerDragMode::DraggingSysEx {
                        index,
                        original_sample,
                        start_x,
                        ..
                    } = state.mode
                {
                    state.mode = ControllerDragMode::DraggingSysEx {
                        index,
                        original_sample,
                        start_x,
                        current_x: position.x,
                    };
                    return Some(CanvasAction::request_redraw().and_capture());
                }
                None
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if let ControllerDragMode::Adjusting {
                    target,
                    start_y,
                    start_value,
                    mut current_y,
                } = state.mode
                {
                    state.mode = ControllerDragMode::None;
                    if let Some(position) = cursor.position_in(bounds) {
                        current_y = position.y;
                    }
                    let delta = ((start_y - current_y) / 4.0).round() as i16;
                    let value = (i16::from(start_value) + delta).clamp(0, 127) as u8;
                    let msg = match target {
                        ControllerAdjustTarget::Controller(controller_index) => {
                            Message::PianoSetControllerValue {
                                controller_index,
                                value,
                            }
                        }
                        ControllerAdjustTarget::Velocity(note_index) => Message::PianoSetVelocity {
                            note_index,
                            velocity: value,
                        },
                    };
                    return Some(CanvasAction::publish(msg).and_capture());
                }
                if let ControllerDragMode::DraggingSysEx {
                    index,
                    original_sample,
                    start_x,
                    mut current_x,
                } = state.mode
                {
                    state.mode = ControllerDragMode::None;
                    if let Some(position) = cursor.position_in(bounds) {
                        current_x = position.x;
                    }
                    let delta_samples = ((current_x - start_x) / pps)
                        .round()
                        .max(-(original_sample as f32))
                        as isize;
                    let new_sample = (original_sample as isize + delta_samples).max(0) as usize;
                    return Some(
                        CanvasAction::publish(Message::PianoSysExMove {
                            index,
                            sample: new_sample.min(clip_len.saturating_sub(1)),
                        })
                        .and_capture(),
                    );
                }
                None
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Middle)) => {
                if let ControllerDragMode::Erasing {
                    start,
                    mut current,
                    target,
                } = state.mode
                {
                    state.mode = ControllerDragMode::None;
                    if let Some(position) = cursor.position_in(bounds) {
                        current = position;
                    }
                    let drag_delta = (current.x - start.x).abs().max((current.y - start.y).abs());
                    let msg = match target {
                        ControllerEraseTarget::Velocity(note_index) => Message::PianoDeleteNotes {
                            note_indices: vec![note_index],
                        },
                        ControllerEraseTarget::Controller(controller_index) if drag_delta < 3.0 => {
                            Message::PianoDeleteControllers {
                                controller_indices: vec![controller_index],
                            }
                        }
                        ControllerEraseTarget::Controller(_) => {
                            let controller_indices = self.controller_indices_in_sample_range(
                                start.x,
                                current.x,
                                pps,
                                ControllerHitTest {
                                    lane,
                                    pane_h: bounds.height,
                                    pps,
                                    selected_row: None,
                                    controllers,
                                },
                            );
                            if controller_indices.is_empty() {
                                return Some(CanvasAction::capture());
                            }
                            Message::PianoDeleteControllers { controller_indices }
                        }
                        ControllerEraseTarget::ControllerRange if drag_delta < 3.0 => {
                            return Some(CanvasAction::capture());
                        }
                        ControllerEraseTarget::ControllerRange => {
                            let controller_indices = self.controller_indices_in_sample_range(
                                start.x,
                                current.x,
                                pps,
                                ControllerHitTest {
                                    lane,
                                    pane_h: bounds.height,
                                    pps,
                                    selected_row: None,
                                    controllers,
                                },
                            );
                            if controller_indices.is_empty() {
                                return Some(CanvasAction::capture());
                            }
                            Message::PianoDeleteControllers { controller_indices }
                        }
                    };
                    return Some(CanvasAction::publish(msg).and_capture());
                }
                None
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                if matches!(
                    lane,
                    PianoControllerLane::Velocity | PianoControllerLane::SysEx
                ) {
                    return None;
                }
                if let Some(position) = cursor.position_in(bounds) {
                    state.mode = ControllerDragMode::Drawing {
                        start: position,
                        current: position,
                    };
                    return Some(CanvasAction::request_redraw().and_capture());
                }
                None
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)) => {
                if let ControllerDragMode::Drawing { start, current } = state.mode {
                    state.mode = ControllerDragMode::None;
                    let lane_cfg = match lane {
                        PianoControllerLane::Controller => {
                            controllers_lane::LaneConfig::Controller(
                                127_u8.saturating_sub(selected_row.unwrap_or(126) as u8),
                            )
                        }
                        PianoControllerLane::Rpn => {
                            controllers_lane::LaneConfig::Rpn(selected_row.unwrap_or(0))
                        }
                        PianoControllerLane::Nrpn => {
                            controllers_lane::LaneConfig::Nrpn(selected_row.unwrap_or(0))
                        }
                        PianoControllerLane::Velocity | PianoControllerLane::SysEx => {
                            return Some(CanvasAction::capture());
                        }
                    };
                    let new_controllers = controllers_lane::build_drawn_controllers(
                        start,
                        current,
                        controllers_lane::DrawContext {
                            bounds,
                            pps,
                            sample_rate_hz: self.sample_rate_hz,
                            samples_per_bar: self.samples_per_bar,
                            clip_len,
                        },
                        lane_cfg,
                    );
                    if new_controllers.is_empty() {
                        return Some(CanvasAction::request_redraw().and_capture());
                    }
                    return Some(
                        CanvasAction::publish(Message::PianoInsertControllers {
                            controllers: new_controllers,
                        })
                        .and_capture(),
                    );
                }
                None
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        match state.mode {
            ControllerDragMode::None => return vec![],
            ControllerDragMode::Erasing { start, current, .. } => {
                let line = Path::line(start, current);
                frame.stroke(
                    &line,
                    canvas::Stroke::default()
                        .with_width(2.0)
                        .with_color(Color::from_rgba(1.0, 0.35, 0.28, 0.95)),
                );
            }
            ControllerDragMode::Drawing { start, current } => {
                let line = Path::line(start, current);
                frame.stroke(
                    &line,
                    canvas::Stroke::default()
                        .with_width(2.0)
                        .with_color(Color::from_rgba(0.98, 0.94, 0.2, 0.95)),
                );

                let lane = self.state.blocking_read().piano_controller_lane;
                let value_from_y = |y: f32| -> u16 {
                    if bounds.height <= f32::EPSILON {
                        return if matches!(
                            lane,
                            PianoControllerLane::Rpn | PianoControllerLane::Nrpn
                        ) {
                            8192
                        } else {
                            64
                        };
                    }
                    let t = (1.0 - (y / bounds.height)).clamp(0.0, 1.0);
                    if matches!(lane, PianoControllerLane::Rpn | PianoControllerLane::Nrpn) {
                        (t * 16383.0).round().clamp(0.0, 16383.0) as u16
                    } else {
                        (t * 127.0).round().clamp(0.0, 127.0) as u16
                    }
                };
                let start_value = value_from_y(start.y);
                let current_value = value_from_y(current.y);
                let drag_right = current.x >= start.x;
                let x_offset = if drag_right { -24.0 } else { 8.0 };

                use iced::widget::canvas::Text;
                frame.fill_text(Text {
                    content: start_value.to_string(),
                    position: Point::new(start.x + x_offset, (start.y - 6.0).max(0.0)),
                    color: Color::from_rgba(1.0, 0.96, 0.45, 0.95),
                    size: 11.0.into(),
                    ..Text::default()
                });
                frame.fill_text(Text {
                    content: current_value.to_string(),
                    position: Point::new(current.x + x_offset, (current.y - 6.0).max(0.0)),
                    color: Color::from_rgba(1.0, 0.96, 0.45, 0.95),
                    size: 11.0.into(),
                    ..Text::default()
                });
            }
            ControllerDragMode::Adjusting {
                target,
                start_y,
                start_value,
                current_y,
            } => {
                let app_state = self.state.blocking_read();
                let Some(roll) = app_state.piano.as_ref() else {
                    return vec![];
                };
                let pps = (self.pixels_per_sample * app_state.piano_zoom_x).max(0.0001);
                let delta = ((start_y - current_y) / 4.0).round() as i16;
                let preview_value = (i16::from(start_value) + delta).clamp(0, 127) as u8;
                let x = match target {
                    ControllerAdjustTarget::Controller(idx) => {
                        roll.controllers.get(idx).map(|c| c.sample as f32 * pps)
                    }
                    ControllerAdjustTarget::Velocity(idx) => {
                        roll.notes.get(idx).map(|n| n.start_sample as f32 * pps)
                    }
                };
                let Some(x) = x else {
                    return vec![];
                };
                let old_stem_h = (bounds.height * (start_value as f32 / 127.0)).max(1.0);
                let old_stem_y = bounds.height - old_stem_h;
                let erase_rect = Path::rectangle(
                    Point::new((x - 1.0).max(0.0), old_stem_y),
                    Size::new(5.0, old_stem_h),
                );
                frame.fill(&erase_rect, Color::from_rgba(0.16, 0.16, 0.18, 1.0));

                let stem_h = (bounds.height * (preview_value as f32 / 127.0)).max(1.0);
                let stem_y = bounds.height - stem_h;
                let rect = Path::rectangle(Point::new(x, stem_y), Size::new(3.0, stem_h));
                frame.fill(&rect, Color::from_rgba(1.0, 0.85, 0.2, 0.95));

                use iced::widget::canvas::Text;
                frame.fill_text(Text {
                    content: preview_value.to_string(),
                    position: Point::new(x + 6.0, (stem_y - 6.0).max(0.0)),
                    color: Color::from_rgba(1.0, 0.96, 0.45, 1.0),
                    size: 11.0.into(),
                    ..Text::default()
                });
            }
            ControllerDragMode::DraggingSysEx { current_x, .. } => {
                let line = Path::line(
                    Point::new(current_x.max(0.0), 0.0),
                    Point::new(current_x.max(0.0), bounds.height),
                );
                frame.stroke(
                    &line,
                    canvas::Stroke::default()
                        .with_width(2.0)
                        .with_color(Color::from_rgba(1.0, 0.5, 0.2, 0.95)),
                );
            }
        }
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::{Point, Rectangle, Size, event, mouse};
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    fn action_message(action: CanvasAction<Message>) -> (Option<Message>, event::Status) {
        let (message, _redraw, status) = action.into_inner();
        (message, status)
    }

    #[test]
    fn update_double_clicking_sysex_opens_editor() {
        let state = Arc::new(RwLock::new(crate::state::StateData::default()));
        {
            let mut data = state.blocking_write();
            data.piano_controller_lane = PianoControllerLane::SysEx;
            data.piano_zoom_x = 1.0;
            data.piano = Some(crate::state::PianoData {
                track_idx: "Track".to_string(),
                clip_index: 0,
                clip_start_samples: 0,
                clip_length_samples: 256,
                notes: Vec::new(),
                controllers: Vec::new(),
                sysexes: vec![PianoSysExPoint {
                    sample: 10,
                    data: vec![0xF0, 0x7E, 0xF7],
                }],
                midnam_note_names: HashMap::new(),
            });
        }
        let interaction = ControllerRollInteraction::new(state, 1.0, 48_000.0, 192.0);
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(400.0, 120.0));
        let cursor = mouse::Cursor::Available(Point::new(10.0, 20.0));
        let mut interaction_state = ControllerRollInteractionState {
            last_sysex_click: Some((0, Instant::now())),
            ..ControllerRollInteractionState::default()
        };

        let action = interaction
            .update(
                &mut interaction_state,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                cursor,
            )
            .expect("action");

        let (message, status) = action_message(action);
        match message {
            Some(Message::PianoSysExOpenEditor(Some(index))) => assert_eq!(index, 0),
            other => panic!("unexpected message: {other:?}"),
        }
        assert_eq!(status, event::Status::Captured);
        assert!(matches!(interaction_state.mode, ControllerDragMode::None));
    }

    #[test]
    fn controller_roll_interaction_new_creates_instance() {
        let state = crate::state::State::default();
        let interaction = ControllerRollInteraction::new(state, 0.1, 48000.0, 1920.0);
        assert!(interaction.pixels_per_sample > 0.0);
        assert!(interaction.sample_rate_hz > 0.0);
    }

    #[test]
    fn controller_drag_mode_default_is_none() {
        let mode = ControllerDragMode::default();
        assert!(matches!(mode, ControllerDragMode::None));
    }

    #[test]
    fn controller_roll_interaction_state_default() {
        let state = ControllerRollInteractionState::default();
        assert!(matches!(state.mode, ControllerDragMode::None));
        assert!(state.last_sysex_click.is_none());
    }
}

pub mod controllers_lane {
    use iced::{Point, Rectangle};

    use super::{nrpn_param, rpn_param};
    use crate::consts::message_lists::{PIANO_NRPN_KIND_ALL, PIANO_RPN_KIND_ALL};
    use crate::message::{PianoControllerEditData, PianoNrpnKind, PianoRpnKind};

    #[derive(Debug, Clone, Copy)]
    pub struct DrawContext {
        pub bounds: Rectangle,
        pub pps: f32,
        pub sample_rate_hz: f64,
        pub samples_per_bar: f32,
        pub clip_len: usize,
    }

    fn max_points_for_rate(
        start_sample: usize,
        end_sample: usize,
        sample_rate_hz: f64,
        bytes_per_point: f64,
        fixed_bytes: f64,
    ) -> usize {
        let span_samples = end_sample.saturating_sub(start_sample).max(1) as f64;
        let duration_sec = span_samples / sample_rate_hz.max(1.0);
        let bytes_budget = (duration_sec * crate::consts::widget_piano::MIDI_DIN_BYTES_PER_SEC
            - fixed_bytes)
            .max(0.0);
        let point_budget = (bytes_budget / bytes_per_point).floor() as usize;
        point_budget.saturating_add(1).max(2)
    }

    fn max_points_for_1_128(start_sample: usize, end_sample: usize, samples_per_bar: f32) -> usize {
        let span_samples = end_sample.saturating_sub(start_sample).max(1);
        let min_step = (samples_per_bar.max(1.0) / 128.0).round().max(1.0) as usize;
        span_samples / min_step + 1
    }

    pub enum LaneConfig {
        Controller(u8),
        Rpn(usize),
        Nrpn(usize),
    }

    fn y_to_value7(y: f32, bounds: Rectangle) -> u8 {
        if bounds.height <= f32::EPSILON {
            return 64;
        }
        let t = (1.0 - (y / bounds.height)).clamp(0.0, 1.0);
        (t * 127.0).round().clamp(0.0, 127.0) as u8
    }

    fn y_to_value14(y: f32, bounds: Rectangle) -> u16 {
        if bounds.height <= f32::EPSILON {
            return 8192;
        }
        let t = (1.0 - (y / bounds.height)).clamp(0.0, 1.0);
        (t * 16383.0).round().clamp(0.0, 16383.0) as u16
    }

    pub fn build_drawn_controllers(
        start: Point,
        end: Point,
        ctx: DrawContext,
        lane: LaneConfig,
    ) -> Vec<PianoControllerEditData> {
        let DrawContext {
            bounds,
            pps,
            sample_rate_hz,
            samples_per_bar,
            clip_len,
        } = ctx;
        if pps <= f32::EPSILON || clip_len == 0 {
            return vec![];
        }
        let x0 = start.x.min(end.x).max(0.0);
        let x1 = start.x.max(end.x).max(0.0);
        let s0 = (x0 / pps).round().max(0.0) as usize;
        let s1 = (x1 / pps).round().max(0.0) as usize;
        let start_sample = s0.min(clip_len.saturating_sub(1));
        let end_sample = s1.min(clip_len.saturating_sub(1)).max(start_sample);

        let mut out = Vec::new();
        match lane {
            LaneConfig::Controller(cc) => {
                let start_value = i16::from(y_to_value7(start.y, bounds));
                let end_value = i16::from(y_to_value7(end.y, bounds));
                let delta = end_value - start_value;
                let value_steps = delta.unsigned_abs() as usize;
                let points_by_rate =
                    max_points_for_rate(start_sample, end_sample, sample_rate_hz, 3.0, 0.0);
                let points_by_snap =
                    max_points_for_1_128(start_sample, end_sample, samples_per_bar);
                let points = (value_steps + 1)
                    .min(points_by_rate)
                    .min(points_by_snap)
                    .max(2);
                for i in 0..points {
                    let t = if points <= 1 {
                        0.0
                    } else {
                        i as f32 / (points - 1) as f32
                    };
                    let sample = start_sample + ((end_sample - start_sample) as f32 * t) as usize;
                    let value = if delta >= 0 {
                        (start_value + i as i16).clamp(0, 127) as u8
                    } else {
                        (start_value - i as i16).clamp(0, 127) as u8
                    };
                    out.push(PianoControllerEditData {
                        sample,
                        controller: cc,
                        value,
                        channel: 0,
                    });
                }
            }
            LaneConfig::Rpn(row) => {
                let start_value = i32::from(y_to_value14(start.y, bounds));
                let end_value = i32::from(y_to_value14(end.y, bounds));
                let delta = end_value - start_value;
                let value_steps = delta.unsigned_abs() as usize;
                let mut points = value_steps + 1;
                let max_by_span = end_sample
                    .saturating_sub(start_sample)
                    .saturating_add(1)
                    .max(2);
                let max_by_rate =
                    max_points_for_rate(start_sample, end_sample, sample_rate_hz, 6.0, 6.0);
                let max_by_snap = max_points_for_1_128(start_sample, end_sample, samples_per_bar);
                points = points
                    .min(max_by_span)
                    .min(max_by_rate)
                    .min(max_by_snap)
                    .min(crate::consts::widget_piano::MAX_RPN_NRPN_POINTS);

                let kind = PIANO_RPN_KIND_ALL
                    .get(row)
                    .copied()
                    .unwrap_or(PianoRpnKind::PitchBendSensitivity);
                let (msb, lsb) = rpn_param(kind);

                out.push(PianoControllerEditData {
                    sample: start_sample,
                    controller: 101,
                    value: msb,
                    channel: 0,
                });
                out.push(PianoControllerEditData {
                    sample: start_sample,
                    controller: 100,
                    value: lsb,
                    channel: 0,
                });
                for i in 0..points {
                    let t = if points <= 1 {
                        0.0
                    } else {
                        i as f32 / (points - 1) as f32
                    };
                    let sample = start_sample + ((end_sample - start_sample) as f32 * t) as usize;
                    let value14: u16 = (start_value as f32 + (delta as f32) * t)
                        .round()
                        .clamp(0.0, 16383.0) as u16;
                    let value_msb = ((value14 >> 7) & 0x7f) as u8;
                    let value_lsb = (value14 & 0x7f) as u8;
                    out.push(PianoControllerEditData {
                        sample,
                        controller: 6,
                        value: value_msb,
                        channel: 0,
                    });
                    out.push(PianoControllerEditData {
                        sample,
                        controller: 38,
                        value: value_lsb,
                        channel: 0,
                    });
                }
            }
            LaneConfig::Nrpn(row) => {
                let start_value = i32::from(y_to_value14(start.y, bounds));
                let end_value = i32::from(y_to_value14(end.y, bounds));
                let delta = end_value - start_value;
                let value_steps = delta.unsigned_abs() as usize;
                let mut points = value_steps + 1;
                let max_by_span = end_sample
                    .saturating_sub(start_sample)
                    .saturating_add(1)
                    .max(2);
                let max_by_rate =
                    max_points_for_rate(start_sample, end_sample, sample_rate_hz, 6.0, 6.0);
                let max_by_snap = max_points_for_1_128(start_sample, end_sample, samples_per_bar);
                points = points
                    .min(max_by_span)
                    .min(max_by_rate)
                    .min(max_by_snap)
                    .min(crate::consts::widget_piano::MAX_RPN_NRPN_POINTS);

                let kind = PIANO_NRPN_KIND_ALL
                    .get(row)
                    .copied()
                    .unwrap_or(PianoNrpnKind::Brightness);
                let (msb, lsb) = nrpn_param(kind);

                out.push(PianoControllerEditData {
                    sample: start_sample,
                    controller: 99,
                    value: msb,
                    channel: 0,
                });
                out.push(PianoControllerEditData {
                    sample: start_sample,
                    controller: 98,
                    value: lsb,
                    channel: 0,
                });
                for i in 0..points {
                    let t = if points <= 1 {
                        0.0
                    } else {
                        i as f32 / (points - 1) as f32
                    };
                    let sample = start_sample + ((end_sample - start_sample) as f32 * t) as usize;
                    let value14: u16 = (start_value as f32 + (delta as f32) * t)
                        .round()
                        .clamp(0.0, 16383.0) as u16;
                    let value_msb = ((value14 >> 7) & 0x7f) as u8;
                    let value_lsb = (value14 & 0x7f) as u8;
                    out.push(PianoControllerEditData {
                        sample,
                        controller: 6,
                        value: value_msb,
                        channel: 0,
                    });
                    out.push(PianoControllerEditData {
                        sample,
                        controller: 38,
                        value: value_lsb,
                        channel: 0,
                    });
                }
            }
        }
        out
    }
}
