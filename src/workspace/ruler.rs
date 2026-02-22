use crate::message::Message;
use iced::{
    Color, Element, Length, Point, Rectangle, Renderer, Theme,
    event::Event,
    mouse,
    widget::canvas,
    widget::canvas::{Action as CanvasAction, Frame, Geometry, Path, Stroke, Text},
};
use maolan_engine::message::Action as EngineAction;

const RULER_HEIGHT: f32 = 28.0;
const BEATS_PER_BAR: usize = 4;
const BARS_TO_DRAW: usize = 256;
const MIN_TICK_SPACING_PX: f32 = 8.0;
const MIN_LABEL_SPACING_PX: f32 = 64.0;

#[derive(Debug, Default)]
pub struct Ruler;

#[derive(Debug, Default)]
struct RulerState {
    scrubbing: bool,
    last_x: f32,
}

#[derive(Debug, Clone, Copy)]
struct RulerCanvas {
    playhead_x: Option<f32>,
    beat_pixels: f32,
    pixels_per_sample: f32,
}

impl Ruler {
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

    pub fn view(
        &self,
        playhead_x: Option<f32>,
        beat_pixels: f32,
        pixels_per_sample: f32,
    ) -> Element<'_, Message> {
        canvas(RulerCanvas {
            playhead_x,
            beat_pixels,
            pixels_per_sample,
        })
        .width(Length::Fill)
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

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(pos) = cursor_position {
                    state.scrubbing = true;
                    state.last_x = pos.x.clamp(0.0, bounds.width.max(0.0));
                    return Some(CanvasAction::capture());
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if state.scrubbing
                    && let Some(pos) = cursor_position
                {
                    state.last_x = pos.x.clamp(0.0, bounds.width.max(0.0));
                    return Some(CanvasAction::capture());
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.scrubbing {
                    state.scrubbing = false;
                    let sample = if self.pixels_per_sample > 1.0e-9 {
                        (state.last_x / self.pixels_per_sample).max(0.0) as usize
                    } else {
                        0
                    };
                    return Some(CanvasAction::publish(Message::Request(
                        EngineAction::TransportPosition(sample),
                    )));
                }
            }
            _ => {}
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

        frame.fill(
            &Path::rectangle(Point::new(0.0, 0.0), bounds.size()),
            Color::from_rgba(0.12, 0.12, 0.12, 1.0),
        );

        let tick_step_beats = Ruler::step_for_spacing(self.beat_pixels, MIN_TICK_SPACING_PX);
        let bar_pixels = self.beat_pixels * BEATS_PER_BAR as f32;
        let label_step_bars = Ruler::step_for_spacing(bar_pixels, MIN_LABEL_SPACING_PX);
        let total_beats = BARS_TO_DRAW * BEATS_PER_BAR;

        for beat_idx in (0..=total_beats).step_by(tick_step_beats) {
            let x = beat_idx as f32 * self.beat_pixels;
            let is_bar = beat_idx % BEATS_PER_BAR == 0;
            let is_numbered_bar = is_bar && ((beat_idx / BEATS_PER_BAR) % label_step_bars == 0);
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

        for bar in (0..BARS_TO_DRAW).step_by(label_step_bars) {
            let x = bar as f32 * BEATS_PER_BAR as f32 * self.beat_pixels;
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
