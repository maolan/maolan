use crate::message::{Message, SnapMode};
use iced::{
    Color, Element, Length, Point, Rectangle, Renderer, Theme,
    event::Event,
    mouse,
    widget::canvas,
    widget::canvas::{Action as CanvasAction, Frame, Geometry, Path, Stroke, Text},
};
use maolan_engine::message::Action as EngineAction;

const TEMPO_HEIGHT: f32 = 28.0;

#[derive(Debug, Default)]
pub struct Tempo;

#[derive(Debug, Default)]
struct TempoState {
    dragging: bool,
    drag_start_x: f32,
    last_x: f32,
}

#[derive(Debug, Clone, Copy)]
struct TempoCanvas {
    bpm: f32,
    time_signature: (u8, u8),
    pixels_per_sample: f32,
    playhead_x: Option<f32>,
    punch_range_samples: Option<(usize, usize)>,
    snap_mode: SnapMode,
    samples_per_beat: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct TempoViewArgs {
    pub bpm: f32,
    pub time_signature: (u8, u8),
    pub pixels_per_sample: f32,
    pub playhead_x: Option<f32>,
    pub punch_range_samples: Option<(usize, usize)>,
    pub snap_mode: SnapMode,
    pub samples_per_beat: f64,
    pub content_width: f32,
}

impl Tempo {
    pub fn new() -> Self {
        Self
    }

    pub fn height(&self) -> f32 {
        TEMPO_HEIGHT
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
        let cursor_position = cursor.position_in(bounds);
        let cursor_x = cursor
            .position()
            .map(|pos| (pos.x - bounds.x).clamp(0.0, bounds.width.max(0.0)));

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(pos) = cursor_position {
                    state.dragging = true;
                    let x = cursor_x.unwrap_or(pos.x.clamp(0.0, bounds.width.max(0.0)));
                    state.drag_start_x = x;
                    state.last_x = x;
                    return Some(CanvasAction::capture());
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if state.dragging
                    && let Some(x) = cursor_x
                {
                    state.last_x = x;
                    return Some(CanvasAction::request_redraw().and_capture());
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.dragging {
                    state.dragging = false;
                    if self.pixels_per_sample <= 1.0e-9 {
                        return None;
                    }

                    let drag_delta = (state.last_x - state.drag_start_x).abs();
                    if drag_delta < 3.0 {
                        let sample = (state.last_x / self.pixels_per_sample).max(0.0) as usize;
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

                    let start_x = state.drag_start_x.min(state.last_x).max(0.0);
                    let end_x = state.drag_start_x.max(state.last_x).max(0.0);

                    let snap_interval_f32 = snap_interval as f32;

                    let start_sample = if matches!(self.snap_mode, SnapMode::NoSnap) {
                        (start_x / self.pixels_per_sample).max(0.0)
                    } else {
                        ((start_x / self.pixels_per_sample) / snap_interval_f32).floor() * snap_interval_f32
                    };

                    let mut end_sample = if matches!(self.snap_mode, SnapMode::NoSnap) {
                        (end_x / self.pixels_per_sample).max(0.0)
                    } else {
                        ((end_x / self.pixels_per_sample) / snap_interval_f32).ceil() * snap_interval_f32
                    };

                    if end_sample <= start_sample {
                        end_sample = start_sample + snap_interval_f32;
                    }
                    return Some(CanvasAction::publish(Message::SetPunchRange(Some((
                        start_sample.max(0.0) as usize,
                        end_sample.max(0.0) as usize,
                    )))));
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                if cursor_position.is_some() {
                    return Some(CanvasAction::publish(Message::SetPunchRange(None)).and_capture());
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
        let mut frame = Frame::new(renderer, bounds.size());

        frame.fill(
            &Path::rectangle(Point::new(0.0, 0.0), bounds.size()),
            Color::from_rgba(0.12, 0.12, 0.12, 1.0),
        );

        if !state.dragging
            && let Some((punch_start, punch_end)) = self.punch_range_samples
            && self.pixels_per_sample > 1.0e-9
            && punch_end > punch_start
        {
            let start_x = punch_start as f32 * self.pixels_per_sample;
            let end_x = punch_end as f32 * self.pixels_per_sample;
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

        if state.dragging {
            let start_x = state.drag_start_x.min(state.last_x).max(0.0);
            let end_x = state.drag_start_x.max(state.last_x).max(0.0);
            frame.fill(
                &Path::rectangle(
                    Point::new(start_x, 0.0),
                    iced::Size::new((end_x - start_x).max(1.0), bounds.height),
                ),
                Color::from_rgba(0.92, 0.36, 0.36, 0.22),
            );
            frame.stroke(
                &Path::line(Point::new(start_x, 0.0), Point::new(start_x, bounds.height)),
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

        vec![frame.into_geometry()]
    }
}
