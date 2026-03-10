use crate::message::{Message, SnapMode};
use iced::{
    Color, Element, Length, Point, Rectangle, Renderer, Theme,
    event::Event,
    mouse,
    widget::canvas,
    widget::canvas::{Action as CanvasAction, Frame, Geometry, Path, Stroke, Text},
};
use maolan_engine::message::Action as EngineAction;
use std::cell::Cell;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

const TEMPO_HEIGHT: f32 = 28.0;
const TEMPO_HIT_HEIGHT: f32 = 14.0;
const TIME_SIG_HIT_X_SPLIT: f32 = 36.0;
const LEFT_HIT_WIDTH: f32 = 84.0;
const CONTEXT_MENU_WIDTH: f32 = 132.0;
const CONTEXT_MENU_ITEM_HEIGHT: f32 = 16.0;

#[derive(Debug, Default)]
pub struct Tempo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MarkerLane {
    Tempo,
    TimeSignature,
}

#[derive(Debug, Clone)]
enum DragMode {
    None,
    Punch {
        drag_start_x: f32,
        last_x: f32,
    },
    Marker {
        lane: MarkerLane,
        original_samples: Vec<usize>,
        start_sample: usize,
        current_sample: usize,
    },
}

#[derive(Debug, Clone, Copy)]
struct ContextMenuState {
    lane: MarkerLane,
    x: f32,
    y: f32,
}

#[derive(Debug)]
struct TempoState {
    drag_mode: DragMode,
    context_menu: Option<ContextMenuState>,
    cache: canvas::Cache,
    last_hash: Cell<u64>,
}

impl Default for TempoState {
    fn default() -> Self {
        Self {
            drag_mode: DragMode::None,
            context_menu: None,
            cache: canvas::Cache::default(),
            last_hash: Cell::new(0),
        }
    }
}

#[derive(Debug, Clone)]
struct TempoCanvas {
    bpm: f32,
    time_signature: (u8, u8),
    pixels_per_sample: f32,
    playhead_x: Option<f32>,
    punch_range_samples: Option<(usize, usize)>,
    snap_mode: SnapMode,
    samples_per_beat: f64,
    samples_per_bar: f64,
    shift_pressed: bool,
    tempo_points: Vec<(usize, f32)>,
    time_signature_points: Vec<(usize, u8, u8)>,
    selected_tempo_points: Vec<usize>,
    selected_time_signature_points: Vec<usize>,
    timeline_left_inset_px: f32,
}

#[derive(Debug, Clone)]
pub struct TempoViewArgs {
    pub bpm: f32,
    pub time_signature: (u8, u8),
    pub pixels_per_sample: f32,
    pub playhead_x: Option<f32>,
    pub punch_range_samples: Option<(usize, usize)>,
    pub snap_mode: SnapMode,
    pub samples_per_beat: f64,
    pub samples_per_bar: f64,
    pub content_width: f32,
    pub shift_pressed: bool,
    pub tempo_points: Vec<(usize, f32)>,
    pub time_signature_points: Vec<(usize, u8, u8)>,
    pub selected_tempo_points: Vec<usize>,
    pub selected_time_signature_points: Vec<usize>,
    pub timeline_left_inset_px: f32,
}

impl Tempo {
    pub fn new() -> Self {
        Self
    }

    pub fn height(&self) -> f32 {
        TEMPO_HEIGHT
    }

    fn snap_mode_key(mode: SnapMode) -> u8 {
        match mode {
            SnapMode::NoSnap => 0,
            SnapMode::Bar => 1,
            SnapMode::Beat => 2,
            SnapMode::Eighth => 3,
            SnapMode::Sixteenth => 4,
            SnapMode::ThirtySecond => 5,
            SnapMode::SixtyFourth => 6,
        }
    }

    fn lane_key(lane: MarkerLane) -> u8 {
        match lane {
            MarkerLane::Tempo => 0,
            MarkerLane::TimeSignature => 1,
        }
    }

    pub fn view(&self, args: TempoViewArgs) -> Element<'_, Message> {
        canvas(TempoCanvas {
            bpm: args.bpm,
            time_signature: args.time_signature,
            pixels_per_sample: args.pixels_per_sample,
            playhead_x: args.playhead_x,
            punch_range_samples: args.punch_range_samples,
            snap_mode: args.snap_mode,
            samples_per_beat: args.samples_per_beat,
            samples_per_bar: args.samples_per_bar,
            shift_pressed: args.shift_pressed,
            tempo_points: args.tempo_points,
            time_signature_points: args.time_signature_points,
            selected_tempo_points: args.selected_tempo_points,
            selected_time_signature_points: args.selected_time_signature_points,
            timeline_left_inset_px: args.timeline_left_inset_px,
        })
        .width(Length::Fixed(args.content_width.max(1.0)))
        .height(Length::Fill)
        .into()
    }
}

impl canvas::Program<Message> for TempoCanvas {
    type State = TempoState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<CanvasAction<Message>> {
        let signed_step = |v: f32| if v >= 0.0 { 1_i8 } else { -1_i8 };
        let sample_at_x = |x: f32| {
            if self.pixels_per_sample <= 1.0e-9 {
                0_usize
            } else {
                ((x - self.timeline_left_inset_px).max(0.0) / self.pixels_per_sample)
                    .round()
                    .max(0.0) as usize
            }
        };
        let snap_sample = |sample: usize| {
            let interval = match self.snap_mode {
                SnapMode::NoSnap => 1.0,
                SnapMode::Bar => self.samples_per_bar.max(1.0),
                SnapMode::Beat => self.samples_per_beat.max(1.0),
                SnapMode::Eighth => (self.samples_per_beat / 2.0).max(1.0),
                SnapMode::Sixteenth => (self.samples_per_beat / 4.0).max(1.0),
                SnapMode::ThirtySecond => (self.samples_per_beat / 8.0).max(1.0),
                SnapMode::SixtyFourth => (self.samples_per_beat / 16.0).max(1.0),
            };
            if matches!(self.snap_mode, SnapMode::NoSnap) {
                sample
            } else {
                ((sample as f64 / interval).round() * interval).max(0.0) as usize
            }
        };
        let nearest_tempo_point_at_x = |x: f32| -> Option<usize> {
            self.tempo_points
                .iter()
                .filter(|(sample, _)| *sample > 0)
                .map(|(sample, _)| *sample)
                .min_by(|a, b| {
                    let ax = self.timeline_left_inset_px + *a as f32 * self.pixels_per_sample;
                    let bx = self.timeline_left_inset_px + *b as f32 * self.pixels_per_sample;
                    (ax - x)
                        .abs()
                        .partial_cmp(&(bx - x).abs())
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .filter(|sample| {
                    let sx = self.timeline_left_inset_px + *sample as f32 * self.pixels_per_sample;
                    (sx - x).abs() <= 6.0
                })
        };
        let nearest_tsig_point_at_x = |x: f32| -> Option<usize> {
            self.time_signature_points
                .iter()
                .filter(|(sample, _, _)| *sample > 0)
                .map(|(sample, _, _)| *sample)
                .min_by(|a, b| {
                    let ax = self.timeline_left_inset_px + *a as f32 * self.pixels_per_sample;
                    let bx = self.timeline_left_inset_px + *b as f32 * self.pixels_per_sample;
                    (ax - x)
                        .abs()
                        .partial_cmp(&(bx - x).abs())
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .filter(|sample| {
                    let sx = self.timeline_left_inset_px + *sample as f32 * self.pixels_per_sample;
                    (sx - x).abs() <= 6.0
                })
        };
        let cursor_position = cursor.position_in(bounds);
        let cursor_x = cursor
            .position()
            .map(|pos| (pos.x - bounds.x).clamp(0.0, bounds.width.max(0.0)));
        let context_menu_hit = |pos: Point, menu: ContextMenuState| -> Option<usize> {
            if pos.x < menu.x
                || pos.x > menu.x + CONTEXT_MENU_WIDTH
                || pos.y < menu.y
                || pos.y > menu.y + CONTEXT_MENU_ITEM_HEIGHT * 3.0
            {
                return None;
            }
            Some(
                ((pos.y - menu.y) / CONTEXT_MENU_ITEM_HEIGHT)
                    .floor()
                    .max(0.0) as usize,
            )
        };

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(pos) = cursor_position {
                    if let Some(menu) = state.context_menu
                        && let Some(item_idx) = context_menu_hit(pos, menu)
                    {
                        state.context_menu = None;
                        return Some(match (menu.lane, item_idx) {
                            (MarkerLane::Tempo, 0) => {
                                CanvasAction::publish(Message::TempoSelectionDuplicate)
                                    .and_capture()
                            }
                            (MarkerLane::Tempo, 1) => {
                                CanvasAction::publish(Message::TempoSelectionResetToPrevious)
                                    .and_capture()
                            }
                            (MarkerLane::Tempo, _) => {
                                CanvasAction::publish(Message::TempoSelectionDelete).and_capture()
                            }
                            (MarkerLane::TimeSignature, 0) => {
                                CanvasAction::publish(Message::TimeSignatureSelectionDuplicate)
                                    .and_capture()
                            }
                            (MarkerLane::TimeSignature, 1) => CanvasAction::publish(
                                Message::TimeSignatureSelectionResetToPrevious,
                            )
                            .and_capture(),
                            (MarkerLane::TimeSignature, _) => {
                                CanvasAction::publish(Message::TimeSignatureSelectionDelete)
                                    .and_capture()
                            }
                        });
                    }
                    state.context_menu = None;
                    if pos.x <= LEFT_HIT_WIDTH {
                        if pos.y <= TEMPO_HIT_HEIGHT {
                            return Some(
                                CanvasAction::publish(Message::TempoAdjust(1.0)).and_capture(),
                            );
                        }
                        if pos.x <= TIME_SIG_HIT_X_SPLIT {
                            return Some(
                                CanvasAction::publish(Message::TimeSignatureNumeratorAdjust(1))
                                    .and_capture(),
                            );
                        }
                        return Some(
                            CanvasAction::publish(Message::TimeSignatureDenominatorAdjust(1))
                                .and_capture(),
                        );
                    }
                    if pos.y <= TEMPO_HIT_HEIGHT
                        && let Some(sample) = nearest_tempo_point_at_x(pos.x)
                    {
                        let sample = snap_sample(sample);
                        let sample_selected = self.selected_tempo_points.contains(&sample);
                        let mut originals = if sample_selected && !self.shift_pressed {
                            self.selected_tempo_points.to_vec()
                        } else {
                            vec![sample]
                        };
                        originals.sort_unstable();
                        originals.dedup();
                        state.drag_mode = DragMode::Marker {
                            lane: MarkerLane::Tempo,
                            original_samples: originals,
                            start_sample: sample,
                            current_sample: sample,
                        };
                        if !self.shift_pressed && sample_selected {
                            return Some(CanvasAction::capture());
                        }
                        return Some(
                            CanvasAction::publish(Message::TempoPointSelect {
                                sample,
                                additive: self.shift_pressed,
                            })
                            .and_capture(),
                        );
                    }
                    if pos.y > TEMPO_HIT_HEIGHT
                        && let Some(sample) = nearest_tsig_point_at_x(pos.x)
                    {
                        let sample = snap_sample(sample);
                        let sample_selected = self.selected_time_signature_points.contains(&sample);
                        let mut originals = if sample_selected && !self.shift_pressed {
                            self.selected_time_signature_points.to_vec()
                        } else {
                            vec![sample]
                        };
                        originals.sort_unstable();
                        originals.dedup();
                        state.drag_mode = DragMode::Marker {
                            lane: MarkerLane::TimeSignature,
                            original_samples: originals,
                            start_sample: sample,
                            current_sample: sample,
                        };
                        if !self.shift_pressed && sample_selected {
                            return Some(CanvasAction::capture());
                        }
                        return Some(
                            CanvasAction::publish(Message::TimeSignaturePointSelect {
                                sample,
                                additive: self.shift_pressed,
                            })
                            .and_capture(),
                        );
                    }
                    let x = cursor_x.unwrap_or(pos.x.clamp(0.0, bounds.width.max(0.0)));
                    state.drag_mode = DragMode::Punch {
                        drag_start_x: x,
                        last_x: x,
                    };
                    return Some(
                        CanvasAction::publish(Message::ClearTimingPointSelection).and_capture(),
                    );
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some(x) = cursor_x {
                    match &mut state.drag_mode {
                        DragMode::None => {}
                        DragMode::Punch { last_x, .. } => {
                            *last_x = x;
                            return Some(CanvasAction::request_redraw().and_capture());
                        }
                        DragMode::Marker { current_sample, .. } => {
                            *current_sample = snap_sample(sample_at_x(x));
                            return Some(CanvasAction::request_redraw().and_capture());
                        }
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let drag_mode = std::mem::replace(&mut state.drag_mode, DragMode::None);
                match drag_mode {
                    DragMode::None => {}
                    DragMode::Punch {
                        drag_start_x,
                        last_x,
                    } => {
                        if self.pixels_per_sample <= 1.0e-9 {
                            return None;
                        }

                        let drag_delta = (last_x - drag_start_x).abs();
                        if drag_delta < 3.0 {
                            let sample = (last_x / self.pixels_per_sample).max(0.0) as usize;
                            return Some(CanvasAction::publish(Message::Request(
                                EngineAction::TransportPosition(sample),
                            )));
                        }

                        let snap_interval = match self.snap_mode {
                            SnapMode::NoSnap => 1.0,
                            SnapMode::Bar => (self.samples_per_beat * 4.0).max(1.0),
                            SnapMode::Beat => self.samples_per_beat.max(1.0),
                            SnapMode::Eighth => (self.samples_per_beat / 2.0).max(1.0),
                            SnapMode::Sixteenth => (self.samples_per_beat / 4.0).max(1.0),
                            SnapMode::ThirtySecond => (self.samples_per_beat / 8.0).max(1.0),
                            SnapMode::SixtyFourth => (self.samples_per_beat / 16.0).max(1.0),
                        };

                        let start_x = drag_start_x.min(last_x).max(0.0);
                        let end_x = drag_start_x.max(last_x).max(0.0);

                        let snap_interval_f32 = snap_interval as f32;

                        let start_sample = if matches!(self.snap_mode, SnapMode::NoSnap) {
                            (start_x / self.pixels_per_sample).max(0.0)
                        } else {
                            ((start_x / self.pixels_per_sample) / snap_interval_f32).floor()
                                * snap_interval_f32
                        };

                        let mut end_sample = if matches!(self.snap_mode, SnapMode::NoSnap) {
                            (end_x / self.pixels_per_sample).max(0.0)
                        } else {
                            ((end_x / self.pixels_per_sample) / snap_interval_f32).ceil()
                                * snap_interval_f32
                        };

                        if end_sample <= start_sample {
                            end_sample = start_sample + snap_interval_f32;
                        }
                        return Some(CanvasAction::publish(Message::SetPunchRange(Some((
                            start_sample.max(0.0) as usize,
                            end_sample.max(0.0) as usize,
                        )))));
                    }
                    DragMode::Marker {
                        lane,
                        original_samples,
                        start_sample,
                        current_sample,
                    } => {
                        if current_sample == start_sample || original_samples.is_empty() {
                            return Some(CanvasAction::capture());
                        }
                        let delta = current_sample as i64 - start_sample as i64;
                        let mut to_samples = original_samples
                            .iter()
                            .map(|sample| (*sample as i64 + delta).max(1) as usize)
                            .collect::<Vec<_>>();
                        to_samples.sort_unstable();
                        to_samples.dedup();
                        return Some(match lane {
                            MarkerLane::Tempo => CanvasAction::publish(Message::TempoPointsMove {
                                from_samples: original_samples,
                                to_samples,
                            })
                            .and_capture(),
                            MarkerLane::TimeSignature => {
                                CanvasAction::publish(Message::TimeSignaturePointsMove {
                                    from_samples: original_samples,
                                    to_samples,
                                })
                                .and_capture()
                            }
                        });
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                if let Some(pos) = cursor_position {
                    if pos.x <= LEFT_HIT_WIDTH {
                        if pos.y <= TEMPO_HIT_HEIGHT {
                            return Some(
                                CanvasAction::publish(Message::TempoAdjust(-1.0)).and_capture(),
                            );
                        }
                        if pos.x <= TIME_SIG_HIT_X_SPLIT {
                            return Some(
                                CanvasAction::publish(Message::TimeSignatureNumeratorAdjust(-1))
                                    .and_capture(),
                            );
                        }
                        return Some(
                            CanvasAction::publish(Message::TimeSignatureDenominatorAdjust(-1))
                                .and_capture(),
                        );
                    }
                    if pos.y <= TEMPO_HIT_HEIGHT
                        && let Some(sample) = nearest_tempo_point_at_x(pos.x)
                    {
                        let sample = snap_sample(sample);
                        state.context_menu = Some(ContextMenuState {
                            lane: MarkerLane::Tempo,
                            x: pos.x.min((bounds.width - CONTEXT_MENU_WIDTH).max(0.0)),
                            y: pos
                                .y
                                .min((bounds.height - CONTEXT_MENU_ITEM_HEIGHT * 3.0).max(0.0)),
                        });
                        return Some(
                            CanvasAction::publish(Message::TempoPointSelect {
                                sample,
                                additive: self.shift_pressed,
                            })
                            .and_capture(),
                        );
                    }
                    if pos.y > TEMPO_HIT_HEIGHT
                        && let Some(sample) = nearest_tsig_point_at_x(pos.x)
                    {
                        let sample = snap_sample(sample);
                        state.context_menu = Some(ContextMenuState {
                            lane: MarkerLane::TimeSignature,
                            x: pos.x.min((bounds.width - CONTEXT_MENU_WIDTH).max(0.0)),
                            y: pos
                                .y
                                .min((bounds.height - CONTEXT_MENU_ITEM_HEIGHT * 3.0).max(0.0)),
                        });
                        return Some(
                            CanvasAction::publish(Message::TimeSignaturePointSelect {
                                sample,
                                additive: self.shift_pressed,
                            })
                            .and_capture(),
                        );
                    }
                    state.context_menu = None;
                    let x = cursor_x.unwrap_or(pos.x.clamp(0.0, bounds.width.max(0.0)));
                    state.drag_mode = DragMode::Punch {
                        drag_start_x: x,
                        last_x: x,
                    };
                    return Some(
                        CanvasAction::publish(Message::ClearTimingPointSelection).and_capture(),
                    );
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)) => {
                let drag_mode = std::mem::replace(&mut state.drag_mode, DragMode::None);
                match drag_mode {
                    DragMode::None => {}
                    DragMode::Punch {
                        drag_start_x,
                        last_x,
                    } => {
                        if self.pixels_per_sample <= 1.0e-9 {
                            return None;
                        }

                        let drag_delta = (last_x - drag_start_x).abs();
                        if drag_delta < 3.0 {
                            return Some(
                                CanvasAction::publish(Message::SetPunchRange(None)).and_capture(),
                            );
                        }

                        let snap_interval = match self.snap_mode {
                            SnapMode::NoSnap => 1.0,
                            SnapMode::Bar => (self.samples_per_beat * 4.0).max(1.0),
                            SnapMode::Beat => self.samples_per_beat.max(1.0),
                            SnapMode::Eighth => (self.samples_per_beat / 2.0).max(1.0),
                            SnapMode::Sixteenth => (self.samples_per_beat / 4.0).max(1.0),
                            SnapMode::ThirtySecond => (self.samples_per_beat / 8.0).max(1.0),
                            SnapMode::SixtyFourth => (self.samples_per_beat / 16.0).max(1.0),
                        };

                        let start_x = drag_start_x.min(last_x).max(0.0);
                        let end_x = drag_start_x.max(last_x).max(0.0);

                        let snap_interval_f32 = snap_interval as f32;

                        let start_sample = if matches!(self.snap_mode, SnapMode::NoSnap) {
                            (start_x / self.pixels_per_sample).max(0.0)
                        } else {
                            ((start_x / self.pixels_per_sample) / snap_interval_f32).floor()
                                * snap_interval_f32
                        };

                        let mut end_sample = if matches!(self.snap_mode, SnapMode::NoSnap) {
                            (end_x / self.pixels_per_sample).max(0.0)
                        } else {
                            ((end_x / self.pixels_per_sample) / snap_interval_f32).ceil()
                                * snap_interval_f32
                        };

                        if end_sample <= start_sample {
                            end_sample = start_sample + snap_interval_f32;
                        }
                        return Some(CanvasAction::publish(Message::SetPunchRange(Some((
                            start_sample.max(0.0) as usize,
                            end_sample.max(0.0) as usize,
                        )))));
                    }
                    DragMode::Marker { .. } => return Some(CanvasAction::capture()),
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle)) => {
                if let Some(pos) = cursor_position
                    && pos.x > LEFT_HIT_WIDTH
                {
                    let sample = snap_sample(sample_at_x(pos.x));
                    if pos.y <= TEMPO_HIT_HEIGHT {
                        return Some(
                            CanvasAction::publish(Message::TempoPointAdd(sample)).and_capture(),
                        );
                    }
                    return Some(
                        CanvasAction::publish(Message::TimeSignaturePointAdd(sample)).and_capture(),
                    );
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if let Some(pos) = cursor_position {
                    let scroll_y = match delta {
                        mouse::ScrollDelta::Lines { y, .. } => *y,
                        mouse::ScrollDelta::Pixels { y, .. } => *y / 40.0,
                    };
                    if scroll_y.abs() < f32::EPSILON {
                        return Some(CanvasAction::capture());
                    }
                    if pos.x <= LEFT_HIT_WIDTH {
                        if pos.y <= TEMPO_HIT_HEIGHT {
                            return Some(
                                CanvasAction::publish(Message::TempoAdjust(scroll_y.signum()))
                                    .and_capture(),
                            );
                        }
                        if pos.x <= TIME_SIG_HIT_X_SPLIT {
                            return Some(
                                CanvasAction::publish(Message::TimeSignatureNumeratorAdjust(
                                    signed_step(scroll_y),
                                ))
                                .and_capture(),
                            );
                        }
                        return Some(
                            CanvasAction::publish(Message::TimeSignatureDenominatorAdjust(
                                signed_step(scroll_y),
                            ))
                            .and_capture(),
                        );
                    }
                    return Some(CanvasAction::capture());
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
        cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let cursor_hash = cursor
            .position_in(bounds)
            .map(|p| (p.x.to_bits(), p.y.to_bits()));
        let mut hasher = DefaultHasher::new();
        bounds.width.to_bits().hash(&mut hasher);
        bounds.height.to_bits().hash(&mut hasher);
        self.bpm.to_bits().hash(&mut hasher);
        self.time_signature.hash(&mut hasher);
        self.pixels_per_sample.to_bits().hash(&mut hasher);
        self.playhead_x.map(f32::to_bits).hash(&mut hasher);
        self.punch_range_samples.hash(&mut hasher);
        Tempo::snap_mode_key(self.snap_mode).hash(&mut hasher);
        self.samples_per_beat.to_bits().hash(&mut hasher);
        self.samples_per_bar.to_bits().hash(&mut hasher);
        self.timeline_left_inset_px.to_bits().hash(&mut hasher);
        self.shift_pressed.hash(&mut hasher);
        self.tempo_points.len().hash(&mut hasher);
        for (sample, bpm) in &self.tempo_points {
            sample.hash(&mut hasher);
            bpm.to_bits().hash(&mut hasher);
        }
        self.time_signature_points.hash(&mut hasher);
        self.selected_tempo_points.hash(&mut hasher);
        self.selected_time_signature_points.hash(&mut hasher);
        cursor_hash.hash(&mut hasher);
        match &state.drag_mode {
            DragMode::None => {
                0_u8.hash(&mut hasher);
            }
            DragMode::Punch {
                drag_start_x,
                last_x,
            } => {
                1_u8.hash(&mut hasher);
                drag_start_x.to_bits().hash(&mut hasher);
                last_x.to_bits().hash(&mut hasher);
            }
            DragMode::Marker {
                lane,
                original_samples,
                start_sample,
                current_sample,
            } => {
                2_u8.hash(&mut hasher);
                Tempo::lane_key(*lane).hash(&mut hasher);
                original_samples.hash(&mut hasher);
                start_sample.hash(&mut hasher);
                current_sample.hash(&mut hasher);
            }
        }
        if let Some(menu) = state.context_menu {
            1_u8.hash(&mut hasher);
            Tempo::lane_key(menu.lane).hash(&mut hasher);
            menu.x.to_bits().hash(&mut hasher);
            menu.y.to_bits().hash(&mut hasher);
        } else {
            0_u8.hash(&mut hasher);
        }
        let hash = hasher.finish();

        if state.last_hash.get() != hash {
            state.cache.clear();
            state.last_hash.set(hash);
        }

        let geom = state
            .cache
            .draw(renderer, bounds.size(), |frame: &mut Frame| {
                let sample_to_x = |sample: usize| {
                    self.timeline_left_inset_px + sample as f32 * self.pixels_per_sample
                };
                frame.fill(
                    &Path::rectangle(Point::new(0.0, 0.0), bounds.size()),
                    Color::from_rgba(0.12, 0.12, 0.12, 1.0),
                );
                for (sample, bpm) in self.tempo_points.iter().filter(|(sample, _)| *sample > 0) {
                    let x = sample_to_x(*sample);
                    let selected = self.selected_tempo_points.contains(sample);
                    frame.stroke(
                        &Path::line(Point::new(x, 0.0), Point::new(x, TEMPO_HIT_HEIGHT)),
                        Stroke::default()
                            .with_width(if selected { 2.0 } else { 1.0 })
                            .with_color(if selected {
                                Color::from_rgba(1.0, 0.95, 0.45, 1.0)
                            } else {
                                Color::from_rgba(0.9, 0.9, 0.35, 0.85)
                            }),
                    );
                    frame.fill_text(Text {
                        content: format!("{:.0}", bpm),
                        position: Point::new(x + 3.0, 2.0),
                        color: Color::from_rgba(0.95, 0.95, 0.78, 0.92),
                        size: 9.0.into(),
                        ..Default::default()
                    });
                }
                for (sample, n, d) in self
                    .time_signature_points
                    .iter()
                    .filter(|(sample, _, _)| *sample > 0)
                {
                    let x = sample_to_x(*sample);
                    let selected = self.selected_time_signature_points.contains(sample);
                    frame.stroke(
                        &Path::line(
                            Point::new(x, TEMPO_HIT_HEIGHT),
                            Point::new(x, bounds.height),
                        ),
                        Stroke::default()
                            .with_width(if selected { 2.0 } else { 1.0 })
                            .with_color(if selected {
                                Color::from_rgba(0.52, 1.0, 1.0, 1.0)
                            } else {
                                Color::from_rgba(0.45, 0.9, 0.9, 0.85)
                            }),
                    );
                    frame.fill_text(Text {
                        content: format!("{n}/{d}"),
                        position: Point::new(x + 3.0, TEMPO_HIT_HEIGHT + 1.0),
                        color: Color::from_rgba(0.8, 0.98, 0.98, 0.9),
                        size: 9.0.into(),
                        ..Default::default()
                    });
                }

                if !matches!(state.drag_mode, DragMode::Punch { .. })
                    && let Some((punch_start, punch_end)) = self.punch_range_samples
                    && self.pixels_per_sample > 1.0e-9
                    && punch_end > punch_start
                {
                    let start_x = sample_to_x(punch_start);
                    let end_x = sample_to_x(punch_end);
                    frame.fill(
                        &Path::rectangle(
                            Point::new(start_x.max(0.0), 0.0),
                            iced::Size::new((end_x - start_x).max(1.0), bounds.height),
                        ),
                        Color::from_rgba(0.55, 0.18, 0.18, 0.30),
                    );
                    frame.stroke(
                        &Path::line(
                            Point::new(start_x.max(0.0), 0.0),
                            Point::new(start_x.max(0.0), bounds.height),
                        ),
                        Stroke::default()
                            .with_width(1.5)
                            .with_color(Color::from_rgba(0.92, 0.45, 0.45, 0.9)),
                    );
                    frame.stroke(
                        &Path::line(
                            Point::new(end_x.max(0.0), 0.0),
                            Point::new(end_x.max(0.0), bounds.height),
                        ),
                        Stroke::default()
                            .with_width(1.5)
                            .with_color(Color::from_rgba(0.92, 0.45, 0.45, 0.9)),
                    );
                }

                match &state.drag_mode {
                    DragMode::None => {}
                    DragMode::Punch {
                        drag_start_x,
                        last_x,
                    } => {
                        let start_x = drag_start_x.min(*last_x).max(0.0);
                        let end_x = drag_start_x.max(*last_x).max(0.0);
                        frame.fill(
                            &Path::rectangle(
                                Point::new(start_x, 0.0),
                                iced::Size::new((end_x - start_x).max(1.0), bounds.height),
                            ),
                            Color::from_rgba(0.92, 0.36, 0.36, 0.22),
                        );
                        frame.stroke(
                            &Path::line(
                                Point::new(start_x, 0.0),
                                Point::new(start_x, bounds.height),
                            ),
                            Stroke::default()
                                .with_width(1.5)
                                .with_color(Color::from_rgba(0.97, 0.58, 0.58, 0.95)),
                        );
                        frame.stroke(
                            &Path::line(Point::new(end_x, 0.0), Point::new(end_x, bounds.height)),
                            Stroke::default()
                                .with_width(1.5)
                                .with_color(Color::from_rgba(0.97, 0.58, 0.58, 0.95)),
                        );
                    }
                    DragMode::Marker {
                        lane,
                        original_samples,
                        start_sample,
                        current_sample,
                    } => {
                        let delta = *current_sample as i64 - *start_sample as i64;
                        for sample in original_samples {
                            let moved_sample = (*sample as i64 + delta).max(1) as usize;
                            let x = sample_to_x(moved_sample);
                            let (y0, y1, color) = match lane {
                                MarkerLane::Tempo => (
                                    0.0,
                                    TEMPO_HIT_HEIGHT,
                                    Color::from_rgba(1.0, 0.95, 0.45, 0.85),
                                ),
                                MarkerLane::TimeSignature => (
                                    TEMPO_HIT_HEIGHT,
                                    bounds.height,
                                    Color::from_rgba(0.52, 1.0, 1.0, 0.85),
                                ),
                            };
                            frame.stroke(
                                &Path::line(Point::new(x, y0), Point::new(x, y1)),
                                Stroke::default().with_width(2.0).with_color(color),
                            );
                        }
                    }
                }

                frame.fill_text(Text {
                    content: format!("{:.0} BPM", self.bpm),
                    position: Point::new(10.0, 2.0),
                    color: Color::WHITE,
                    size: 14.0.into(),
                    ..Default::default()
                });

                frame.fill_text(Text {
                    content: format!("{}/{}", self.time_signature.0, self.time_signature.1),
                    position: Point::new(10.0, 15.0),
                    color: Color::WHITE,
                    size: 10.0.into(),
                    ..Default::default()
                });
                frame.fill_text(Text {
                    content: "L+/R-".to_string(),
                    position: Point::new(56.0, 2.0),
                    color: Color::from_rgba(0.82, 0.82, 0.82, 0.8),
                    size: 9.0.into(),
                    ..Default::default()
                });
                frame.fill_text(Text {
                    content: "Scroll +/-".to_string(),
                    position: Point::new(47.0, 15.0),
                    color: Color::from_rgba(0.82, 0.82, 0.82, 0.8),
                    size: 9.0.into(),
                    ..Default::default()
                });

                if let Some(menu) = state.context_menu {
                    let menu_height = CONTEXT_MENU_ITEM_HEIGHT * 3.0;
                    frame.fill(
                        &Path::rectangle(
                            Point::new(menu.x, menu.y),
                            iced::Size::new(CONTEXT_MENU_WIDTH, menu_height),
                        ),
                        Color::from_rgba(0.16, 0.16, 0.16, 0.98),
                    );
                    for i in 1..3 {
                        let y = menu.y + CONTEXT_MENU_ITEM_HEIGHT * i as f32;
                        frame.stroke(
                            &Path::line(
                                Point::new(menu.x, y),
                                Point::new(menu.x + CONTEXT_MENU_WIDTH, y),
                            ),
                            Stroke::default()
                                .with_width(1.0)
                                .with_color(Color::from_rgba(0.32, 0.32, 0.32, 0.9)),
                        );
                    }
                    let labels = ["Duplicate", "Reset to Previous", "Delete"];
                    for (idx, label) in labels.iter().enumerate() {
                        frame.fill_text(Text {
                            content: (*label).to_string(),
                            position: Point::new(
                                menu.x + 6.0,
                                menu.y + 2.0 + CONTEXT_MENU_ITEM_HEIGHT * idx as f32,
                            ),
                            color: Color::from_rgba(0.95, 0.95, 0.95, 0.95),
                            size: 10.0.into(),
                            ..Default::default()
                        });
                    }
                }

                if let Some(pos) = cursor.position_in(bounds)
                    && pos.x > LEFT_HIT_WIDTH
                {
                    let tooltip = if pos.y <= TEMPO_HIT_HEIGHT {
                        self.tempo_points
                            .iter()
                            .filter(|(sample, _)| *sample > 0)
                            .min_by(|(sa, _), (sb, _)| {
                                let ax = sample_to_x(*sa);
                                let bx = sample_to_x(*sb);
                                (ax - pos.x)
                                    .abs()
                                    .partial_cmp(&(bx - pos.x).abs())
                                    .unwrap_or(std::cmp::Ordering::Equal)
                            })
                            .and_then(|(sample, bpm)| {
                                let x = sample_to_x(*sample);
                                if (x - pos.x).abs() <= 6.0 {
                                    Some(format!("s:{}  {:.2} BPM", sample, bpm))
                                } else {
                                    None
                                }
                            })
                    } else {
                        self.time_signature_points
                            .iter()
                            .filter(|(sample, _, _)| *sample > 0)
                            .min_by(|(sa, _, _), (sb, _, _)| {
                                let ax = sample_to_x(*sa);
                                let bx = sample_to_x(*sb);
                                (ax - pos.x)
                                    .abs()
                                    .partial_cmp(&(bx - pos.x).abs())
                                    .unwrap_or(std::cmp::Ordering::Equal)
                            })
                            .and_then(|(sample, n, d)| {
                                let x = sample_to_x(*sample);
                                if (x - pos.x).abs() <= 6.0 {
                                    Some(format!("s:{}  {}/{}", sample, n, d))
                                } else {
                                    None
                                }
                            })
                    };
                    if let Some(text) = tooltip {
                        let tip_x = (pos.x + 8.0).min((bounds.width - 150.0).max(0.0));
                        let tip_y = (pos.y + 2.0).min((bounds.height - 14.0).max(0.0));
                        frame.fill(
                            &Path::rectangle(
                                Point::new(tip_x, tip_y),
                                iced::Size::new(148.0, 12.0),
                            ),
                            Color::from_rgba(0.08, 0.08, 0.08, 0.9),
                        );
                        frame.fill_text(Text {
                            content: text,
                            position: Point::new(tip_x + 4.0, tip_y + 1.0),
                            color: Color::from_rgba(0.96, 0.96, 0.96, 0.95),
                            size: 9.0.into(),
                            ..Default::default()
                        });
                    }
                }

                if let Some(x) = self.playhead_x {
                    let path = Path::line(
                        Point::new(x.max(0.0), 0.0),
                        Point::new(x.max(0.0), bounds.height),
                    );
                    frame.stroke(
                        &path,
                        Stroke::default().with_width(2.0).with_color(Color {
                            r: 0.95,
                            g: 0.18,
                            b: 0.14,
                            a: 0.95,
                        }),
                    );
                }
            });

        vec![geom]
    }
}
