use super::ClipSnapEdge;
use super::timeline_x_to_sample_f32;
use crate::consts::workspace::{
    BEATS_PER_BAR, MIN_LABEL_SPACING_PX, MIN_TICK_SPACING_PX, RULER_HEIGHT,
};
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

fn clip_kind_key(kind: maolan_engine::kind::Kind) -> u8 {
    match kind {
        maolan_engine::kind::Kind::Audio => 0,
        maolan_engine::kind::Kind::MIDI => 1,
    }
}

#[derive(Debug, Default)]
pub struct Ruler;

#[derive(Debug)]
struct RulerState {
    dragging: bool,
    drag_with_right: bool,
    drag_adjust_loop_edge: bool,
    adjust_loop_start: bool,
    drag_start_x: f32,
    last_x: f32,
    cache: canvas::Cache,
    last_hash: Cell<u64>,
}

impl Default for RulerState {
    fn default() -> Self {
        Self {
            dragging: false,
            drag_with_right: false,
            drag_adjust_loop_edge: false,
            adjust_loop_start: false,
            drag_start_x: 0.0,
            last_x: 0.0,
            cache: canvas::Cache::default(),
            last_hash: Cell::new(0),
        }
    }
}

#[derive(Debug, Clone)]
struct RulerCanvas {
    playhead_x: Option<f32>,
    beat_pixels: f32,
    pixels_per_sample: f32,
    loop_range_samples: Option<(usize, usize)>,
    clip_snap_edges: Vec<ClipSnapEdge>,
    snap_mode: SnapMode,
    samples_per_beat: f64,
    timeline_left_inset_px: f32,
}

pub struct RulerViewArgs {
    pub playhead_x: Option<f32>,
    pub beat_pixels: f32,
    pub pixels_per_sample: f32,
    pub loop_range_samples: Option<(usize, usize)>,
    pub clip_snap_edges: Vec<ClipSnapEdge>,
    pub snap_mode: SnapMode,
    pub samples_per_beat: f64,
    pub content_width: f32,
    pub timeline_left_inset_px: f32,
}

impl Ruler {
    const RANGE_EDGE_HIT_PX: f32 = 8.0;

    pub fn new() -> Self {
        Self
    }

    pub fn height(&self) -> f32 {
        RULER_HEIGHT
    }

    fn step_for_spacing(base_px: f32, min_spacing_px: f32) -> usize {
        if base_px <= 0.0 {
            return 1;
        }
        let mut step = 1usize;
        while base_px * (step as f32) < min_spacing_px {
            step *= 2;
        }
        step
    }

    fn snap_mode_key(mode: SnapMode) -> u8 {
        match mode {
            SnapMode::NoSnap => 0,
            SnapMode::Clips => 1,
            SnapMode::Bar => 2,
            SnapMode::Beat => 3,
            SnapMode::Eighth => 4,
            SnapMode::Sixteenth => 5,
            SnapMode::ThirtySecond => 6,
            SnapMode::SixtyFourth => 7,
        }
    }

    pub fn view(&self, args: RulerViewArgs) -> Element<'_, Message> {
        let RulerViewArgs {
            playhead_x,
            beat_pixels,
            pixels_per_sample,
            loop_range_samples,
            clip_snap_edges,
            snap_mode,
            samples_per_beat,
            content_width,
            timeline_left_inset_px,
        } = args;
        canvas(RulerCanvas {
            playhead_x,
            beat_pixels,
            pixels_per_sample,
            loop_range_samples,
            clip_snap_edges,
            snap_mode,
            samples_per_beat,
            timeline_left_inset_px,
        })
        .width(Length::Fixed(content_width.max(1.0)))
        .height(Length::Fill)
        .into()
    }
}

impl canvas::Program<Message> for RulerCanvas {
    type State = RulerState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<CanvasAction<Message>> {
        let cursor_position = cursor.position_in(bounds);
        let cursor_x = cursor
            .position()
            .map(|pos| (pos.x - bounds.x).clamp(0.0, bounds.width.max(0.0)));
        let sample_at_x = |x: f32| {
            timeline_x_to_sample_f32(x, self.pixels_per_sample, self.timeline_left_inset_px)
                as usize
        };
        let snap_to_clips = |sample: usize| {
            let threshold_samples = (12.0 / self.pixels_per_sample.max(1.0e-6)).max(1.0);
            let mut snap_targets = self
                .clip_snap_edges
                .iter()
                .filter_map(|edge| {
                    let distance = (sample as f32 - edge.sample as f32).abs();
                    (distance <= threshold_samples).then_some((distance, edge))
                })
                .map(|(_, edge)| edge.clip_id.clone())
                .collect::<Vec<_>>();
            snap_targets.sort_unstable_by(|a, b| {
                a.track_idx
                    .cmp(&b.track_idx)
                    .then_with(|| a.clip_idx.cmp(&b.clip_idx))
                    .then_with(|| clip_kind_key(a.kind).cmp(&clip_kind_key(b.kind)))
            });
            snap_targets.dedup();
            let snapped_sample = self
                .clip_snap_edges
                .iter()
                .filter_map(|edge| {
                    let distance = (sample as f32 - edge.sample as f32).abs();
                    (distance <= threshold_samples).then_some((distance, edge.sample))
                })
                .min_by(|(a, a_edge), (b, b_edge)| {
                    a.partial_cmp(b)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| a_edge.cmp(b_edge))
                })
                .map(|(_, edge)| edge)
                .unwrap_or(sample);
            (snapped_sample, snap_targets)
        };
        let snap_sample = |sample: usize| {
            let interval = match self.snap_mode {
                SnapMode::NoSnap => 1.0,
                SnapMode::Clips => 1.0,
                SnapMode::Bar => (self.samples_per_beat * 4.0).max(1.0),
                SnapMode::Beat => self.samples_per_beat.max(1.0),
                SnapMode::Eighth => (self.samples_per_beat / 2.0).max(1.0),
                SnapMode::Sixteenth => (self.samples_per_beat / 4.0).max(1.0),
                SnapMode::ThirtySecond => (self.samples_per_beat / 8.0).max(1.0),
                SnapMode::SixtyFourth => (self.samples_per_beat / 16.0).max(1.0),
            };
            if matches!(self.snap_mode, SnapMode::Clips) {
                snap_to_clips(sample)
            } else if matches!(self.snap_mode, SnapMode::NoSnap) {
                (sample, Vec::new())
            } else {
                (
                    ((sample as f64 / interval).round() * interval).max(0.0) as usize,
                    Vec::new(),
                )
            }
        };

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(pos) = cursor_position {
                    state.dragging = true;
                    state.drag_with_right = false;
                    let x = cursor_x.unwrap_or(pos.x.clamp(0.0, bounds.width.max(0.0)));
                    state.drag_start_x = x;
                    state.last_x = x;
                    return Some(CanvasAction::capture());
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                if let Some(pos) = cursor_position {
                    state.dragging = true;
                    state.drag_adjust_loop_edge = false;
                    state.drag_with_right = true;
                    let x = cursor_x.unwrap_or(pos.x.clamp(0.0, bounds.width.max(0.0)));
                    state.drag_start_x = x;
                    state.last_x = x;
                    return Some(CanvasAction::capture());
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle)) => {
                if let Some(pos) = cursor_position
                    && let Some((loop_start, loop_end)) = self.loop_range_samples
                    && loop_end > loop_start
                {
                    let x = cursor_x.unwrap_or(pos.x.clamp(0.0, bounds.width.max(0.0)));
                    let start_x = loop_start as f32 * self.pixels_per_sample;
                    let end_x = loop_end as f32 * self.pixels_per_sample;
                    let start_hit = (x - start_x).abs() <= Ruler::RANGE_EDGE_HIT_PX;
                    let end_hit = (x - end_x).abs() <= Ruler::RANGE_EDGE_HIT_PX;
                    if start_hit || end_hit {
                        state.dragging = true;
                        state.drag_with_right = false;
                        state.drag_adjust_loop_edge = true;
                        state.adjust_loop_start = start_hit;
                        state.drag_start_x = x;
                        state.last_x = x;
                        return Some(CanvasAction::capture());
                    }
                    if x >= start_x.min(end_x) && x <= start_x.max(end_x) {
                        return Some(CanvasAction::capture());
                    }
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if state.dragging
                    && let Some(x) = cursor_x
                {
                    state.last_x = x;
                    if matches!(self.snap_mode, SnapMode::Clips) {
                        let snap_targets = if state.drag_adjust_loop_edge {
                            snap_sample(sample_at_x(x)).1
                        } else {
                            let start_x = state.drag_start_x.min(x).max(0.0);
                            let end_x = state.drag_start_x.max(x).max(0.0);
                            let mut targets = snap_sample(sample_at_x(start_x)).1;
                            targets.extend(snap_sample(sample_at_x(end_x)).1);
                            targets.sort_unstable_by(|a, b| {
                                a.track_idx
                                    .cmp(&b.track_idx)
                                    .then_with(|| a.clip_idx.cmp(&b.clip_idx))
                                    .then_with(|| clip_kind_key(a.kind).cmp(&clip_kind_key(b.kind)))
                            });
                            targets.dedup();
                            targets
                        };
                        return Some(
                            CanvasAction::publish(Message::SetClipSnapTargets(snap_targets))
                                .and_capture(),
                        );
                    }
                    return Some(CanvasAction::request_redraw().and_capture());
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.dragging && !state.drag_with_right {
                    state.dragging = false;
                    if self.pixels_per_sample <= 1.0e-9 {
                        return None;
                    }

                    let drag_delta = (state.last_x - state.drag_start_x).abs();
                    if drag_delta < 3.0 {
                        let sample = snap_sample(sample_at_x(state.last_x)).0;
                        return Some(CanvasAction::publish(Message::Request(
                            EngineAction::TransportPosition(sample),
                        )));
                    }

                    let snap_interval = match self.snap_mode {
                        SnapMode::NoSnap => 1.0,
                        SnapMode::Clips => 1.0,
                        SnapMode::Bar => (self.samples_per_beat * 4.0).max(1.0),
                        SnapMode::Beat => self.samples_per_beat.max(1.0),
                        SnapMode::Eighth => (self.samples_per_beat / 2.0).max(1.0),
                        SnapMode::Sixteenth => (self.samples_per_beat / 4.0).max(1.0),
                        SnapMode::ThirtySecond => (self.samples_per_beat / 8.0).max(1.0),
                        SnapMode::SixtyFourth => (self.samples_per_beat / 16.0).max(1.0),
                    };

                    let start_x = state.drag_start_x.min(state.last_x).max(0.0);
                    let end_x = state.drag_start_x.max(state.last_x).max(0.0);

                    let snap_interval_f32 = snap_interval as f32;

                    let start_sample = if matches!(self.snap_mode, SnapMode::Clips) {
                        snap_to_clips((start_x / self.pixels_per_sample).max(0.0) as usize).0 as f32
                    } else if matches!(self.snap_mode, SnapMode::NoSnap) {
                        (start_x / self.pixels_per_sample).max(0.0)
                    } else {
                        ((start_x / self.pixels_per_sample) / snap_interval_f32).floor()
                            * snap_interval_f32
                    };

                    let mut end_sample = if matches!(self.snap_mode, SnapMode::Clips) {
                        snap_to_clips((end_x / self.pixels_per_sample).max(0.0) as usize).0 as f32
                    } else if matches!(self.snap_mode, SnapMode::NoSnap) {
                        (end_x / self.pixels_per_sample).max(0.0)
                    } else {
                        ((end_x / self.pixels_per_sample) / snap_interval_f32).ceil()
                            * snap_interval_f32
                    };

                    if end_sample <= start_sample {
                        end_sample = start_sample + snap_interval_f32;
                    }
                    return Some(CanvasAction::publish(Message::SetLoopRange(Some((
                        start_sample.max(0.0) as usize,
                        end_sample.max(0.0) as usize,
                    )))));
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)) => {
                if state.dragging && state.drag_with_right {
                    state.dragging = false;
                    state.drag_adjust_loop_edge = false;
                    if self.pixels_per_sample <= 1.0e-9 {
                        return None;
                    }

                    let drag_delta = (state.last_x - state.drag_start_x).abs();
                    if drag_delta < 3.0 {
                        return Some(
                            CanvasAction::publish(Message::SetLoopRange(None)).and_capture(),
                        );
                    }

                    let snap_interval = match self.snap_mode {
                        SnapMode::NoSnap => 1.0,
                        SnapMode::Clips => 1.0,
                        SnapMode::Bar => (self.samples_per_beat * 4.0).max(1.0),
                        SnapMode::Beat => self.samples_per_beat.max(1.0),
                        SnapMode::Eighth => (self.samples_per_beat / 2.0).max(1.0),
                        SnapMode::Sixteenth => (self.samples_per_beat / 4.0).max(1.0),
                        SnapMode::ThirtySecond => (self.samples_per_beat / 8.0).max(1.0),
                        SnapMode::SixtyFourth => (self.samples_per_beat / 16.0).max(1.0),
                    };

                    let start_x = state.drag_start_x.min(state.last_x).max(0.0);
                    let end_x = state.drag_start_x.max(state.last_x).max(0.0);

                    let snap_interval_f32 = snap_interval as f32;

                    let start_sample = if matches!(self.snap_mode, SnapMode::Clips) {
                        snap_to_clips((start_x / self.pixels_per_sample).max(0.0) as usize).0 as f32
                    } else if matches!(self.snap_mode, SnapMode::NoSnap) {
                        (start_x / self.pixels_per_sample).max(0.0)
                    } else {
                        ((start_x / self.pixels_per_sample) / snap_interval_f32).floor()
                            * snap_interval_f32
                    };

                    let mut end_sample = if matches!(self.snap_mode, SnapMode::Clips) {
                        snap_to_clips((end_x / self.pixels_per_sample).max(0.0) as usize).0 as f32
                    } else if matches!(self.snap_mode, SnapMode::NoSnap) {
                        (end_x / self.pixels_per_sample).max(0.0)
                    } else {
                        ((end_x / self.pixels_per_sample) / snap_interval_f32).ceil()
                            * snap_interval_f32
                    };

                    if end_sample <= start_sample {
                        end_sample = start_sample + snap_interval_f32;
                    }

                    return Some(CanvasAction::publish(Message::SetLoopRange(Some((
                        start_sample.max(0.0) as usize,
                        end_sample.max(0.0) as usize,
                    )))));
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Middle)) => {
                if state.dragging && state.drag_adjust_loop_edge {
                    state.dragging = false;
                    state.drag_adjust_loop_edge = false;
                    if self.pixels_per_sample <= 1.0e-9 {
                        return None;
                    }
                    let Some((loop_start, loop_end)) = self.loop_range_samples else {
                        return Some(CanvasAction::capture());
                    };
                    let moved_sample = snap_sample(sample_at_x(state.last_x)).0;
                    let (new_start, new_end) = if state.adjust_loop_start {
                        (moved_sample.min(loop_end.saturating_sub(1)), loop_end)
                    } else {
                        (loop_start, moved_sample.max(loop_start.saturating_add(1)))
                    };
                    return Some(
                        CanvasAction::publish(Message::SetLoopRange(Some((new_start, new_end))))
                            .and_capture(),
                    );
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
        let mut hasher = DefaultHasher::new();
        bounds.width.to_bits().hash(&mut hasher);
        bounds.height.to_bits().hash(&mut hasher);
        self.beat_pixels.to_bits().hash(&mut hasher);
        self.pixels_per_sample.to_bits().hash(&mut hasher);
        self.samples_per_beat.to_bits().hash(&mut hasher);
        self.timeline_left_inset_px.to_bits().hash(&mut hasher);
        self.loop_range_samples.hash(&mut hasher);
        self.clip_snap_edges.hash(&mut hasher);
        Ruler::snap_mode_key(self.snap_mode).hash(&mut hasher);
        self.playhead_x.map(f32::to_bits).hash(&mut hasher);
        state.dragging.hash(&mut hasher);
        state.drag_with_right.hash(&mut hasher);
        state.drag_adjust_loop_edge.hash(&mut hasher);
        state.adjust_loop_start.hash(&mut hasher);
        state.drag_start_x.to_bits().hash(&mut hasher);
        state.last_x.to_bits().hash(&mut hasher);
        let hash = hasher.finish();

        if state.last_hash.get() != hash {
            state.cache.clear();
            state.last_hash.set(hash);
        }

        let geom = state
            .cache
            .draw(renderer, bounds.size(), |frame: &mut Frame| {
                frame.fill(
                    &Path::rectangle(Point::new(0.0, 0.0), bounds.size()),
                    Color::from_rgba(0.12, 0.12, 0.12, 1.0),
                );

                if !state.dragging
                    && let Some((loop_start, loop_end)) = self.loop_range_samples
                    && self.pixels_per_sample > 1.0e-9
                    && loop_end > loop_start
                {
                    let start_x = loop_start as f32 * self.pixels_per_sample;
                    let end_x = loop_end as f32 * self.pixels_per_sample;
                    frame.fill(
                        &Path::rectangle(
                            Point::new(start_x.max(0.0), 0.0),
                            iced::Size::new((end_x - start_x).max(1.0), bounds.height),
                        ),
                        Color::from_rgba(0.18, 0.42, 0.20, 0.35),
                    );
                    frame.stroke(
                        &Path::line(
                            Point::new(start_x.max(0.0), 0.0),
                            Point::new(start_x.max(0.0), bounds.height),
                        ),
                        Stroke::default()
                            .with_width(1.5)
                            .with_color(Color::from_rgba(0.45, 0.82, 0.46, 0.9)),
                    );
                    frame.stroke(
                        &Path::line(
                            Point::new(end_x.max(0.0), 0.0),
                            Point::new(end_x.max(0.0), bounds.height),
                        ),
                        Stroke::default()
                            .with_width(1.5)
                            .with_color(Color::from_rgba(0.45, 0.82, 0.46, 0.9)),
                    );
                }

                if state.dragging {
                    let (start_x, end_x) = if state.drag_adjust_loop_edge {
                        if let Some((loop_start, loop_end)) = self.loop_range_samples {
                            let moved_x = state.last_x.max(0.0);
                            if state.adjust_loop_start {
                                (
                                    moved_x.min(loop_end as f32 * self.pixels_per_sample),
                                    loop_end as f32 * self.pixels_per_sample,
                                )
                            } else {
                                (
                                    loop_start as f32 * self.pixels_per_sample,
                                    moved_x.max(loop_start as f32 * self.pixels_per_sample),
                                )
                            }
                        } else {
                            (
                                state.drag_start_x.min(state.last_x).max(0.0),
                                state.drag_start_x.max(state.last_x).max(0.0),
                            )
                        }
                    } else {
                        (
                            state.drag_start_x.min(state.last_x).max(0.0),
                            state.drag_start_x.max(state.last_x).max(0.0),
                        )
                    };
                    frame.fill(
                        &Path::rectangle(
                            Point::new(start_x, 0.0),
                            iced::Size::new((end_x - start_x).max(1.0), bounds.height),
                        ),
                        Color::from_rgba(0.45, 0.82, 0.46, 0.22),
                    );
                    frame.stroke(
                        &Path::line(Point::new(start_x, 0.0), Point::new(start_x, bounds.height)),
                        Stroke::default()
                            .with_width(1.5)
                            .with_color(Color::from_rgba(0.60, 0.92, 0.62, 0.95)),
                    );
                    frame.stroke(
                        &Path::line(Point::new(end_x, 0.0), Point::new(end_x, bounds.height)),
                        Stroke::default()
                            .with_width(1.5)
                            .with_color(Color::from_rgba(0.60, 0.92, 0.62, 0.95)),
                    );
                }

                let tick_step_beats =
                    Ruler::step_for_spacing(self.beat_pixels, MIN_TICK_SPACING_PX);
                let bar_pixels = self.beat_pixels * BEATS_PER_BAR as f32;
                let label_step_bars = Ruler::step_for_spacing(bar_pixels, MIN_LABEL_SPACING_PX);
                let drawable_width = (bounds.width - self.timeline_left_inset_px).max(0.0);
                let total_beats = (drawable_width / self.beat_pixels.max(0.0001))
                    .ceil()
                    .max(0.0) as usize;
                let total_bars =
                    (total_beats as f32 / BEATS_PER_BAR as f32).ceil().max(0.0) as usize;

                for beat_idx in (0..=total_beats).step_by(tick_step_beats) {
                    let x = self.timeline_left_inset_px + beat_idx as f32 * self.beat_pixels;
                    let is_bar = beat_idx % BEATS_PER_BAR == 0;
                    let is_numbered_bar =
                        is_bar && ((beat_idx / BEATS_PER_BAR).is_multiple_of(label_step_bars));
                    let tick_h = if is_numbered_bar { 8.0 } else { 3.0 };
                    frame.stroke(
                        &Path::line(
                            Point::new(x, RULER_HEIGHT - tick_h - 2.0),
                            Point::new(x, RULER_HEIGHT - 2.0),
                        ),
                        Stroke::default().with_color(if is_bar {
                            Color::from_rgba(0.83, 0.83, 0.83, 0.9)
                        } else {
                            Color::from_rgba(0.54, 0.54, 0.54, 0.7)
                        }),
                    );
                }

                for bar in (0..=total_bars).step_by(label_step_bars) {
                    let x = self.timeline_left_inset_px
                        + bar as f32 * BEATS_PER_BAR as f32 * self.beat_pixels;
                    frame.fill_text(Text {
                        content: bar.to_string(),
                        position: Point::new(x + 4.0, 2.0),
                        color: Color::from_rgba(0.86, 0.86, 0.86, 1.0),
                        size: 12.0.into(),
                        ..Default::default()
                    });
                }

                if let Some(x) = self.playhead_x {
                    let path = Path::line(
                        Point::new(x.max(0.0), 0.0),
                        Point::new(x.max(0.0), bounds.height),
                    );
                    frame.stroke(
                        &path,
                        Stroke::default().with_width(1.5).with_color(Color {
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

#[cfg(test)]
mod tests {
    use super::*;
    use iced::widget::canvas::Program;
    use iced::{Point, Rectangle, Size, event, mouse};

    fn action_message(action: CanvasAction<Message>) -> (Option<Message>, event::Status) {
        let (message, _redraw, status) = action.into_inner();
        (message, status)
    }

    #[test]
    fn update_click_release_moves_transport() {
        let canvas = RulerCanvas {
            playhead_x: None,
            beat_pixels: 16.0,
            pixels_per_sample: 2.0,
            loop_range_samples: None,
            clip_snap_edges: Vec::new(),
            snap_mode: SnapMode::NoSnap,
            samples_per_beat: 4.0,
            timeline_left_inset_px: 0.0,
        };
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(400.0, 40.0));
        let mut state = RulerState::default();
        let cursor = mouse::Cursor::Available(Point::new(100.0, 10.0));

        let press = canvas
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                cursor,
            )
            .expect("press action");
        let (_, status) = action_message(press);
        assert_eq!(status, event::Status::Captured);

        let release = canvas
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                cursor,
            )
            .expect("release action");
        let (message, status) = action_message(release);
        match message {
            Some(Message::Request(EngineAction::TransportPosition(sample))) => {
                assert_eq!(sample, 50);
            }
            other => panic!("unexpected message: {other:?}"),
        }
        assert_eq!(status, event::Status::Ignored);
        assert!(!state.dragging);
    }

    #[test]
    fn clip_kind_key_returns_expected_values() {
        use maolan_engine::kind::Kind;
        assert_eq!(clip_kind_key(Kind::Audio), 0);
        assert_eq!(clip_kind_key(Kind::MIDI), 1);
    }

    #[test]
    fn ruler_canvas_can_be_constructed() {
        let canvas = RulerCanvas {
            playhead_x: None,
            beat_pixels: 16.0,
            pixels_per_sample: 2.0,
            loop_range_samples: None,
            clip_snap_edges: Vec::new(),
            snap_mode: SnapMode::NoSnap,
            samples_per_beat: 4.0,
            timeline_left_inset_px: 0.0,
        };
        assert!(canvas.beat_pixels > 0.0);
    }

    #[test]
    fn ruler_state_default() {
        let state = RulerState::default();
        assert!(!state.dragging);
        assert!(!state.drag_with_right);
        assert!(!state.drag_adjust_loop_edge);
    }

    #[test]
    fn ruler_new_creates_instance() {
        let ruler = Ruler::new();
        let _ = &ruler; // Just verify it creates without panicking
    }

    #[test]
    fn ruler_default_creates_instance() {
        let ruler: Ruler = Default::default();
        let _ = &ruler;
    }
}
