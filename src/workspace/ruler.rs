use crate::message::Message;
use iced::{
    Background, Color, Element, Length, Point,
    widget::{Stack, container, pin, text},
};

const RULER_HEIGHT: f32 = 28.0;
const BEATS_PER_BAR: usize = 4;
const BARS_TO_DRAW: usize = 256;
const MIN_TICK_SPACING_PX: f32 = 8.0;
const MIN_LABEL_SPACING_PX: f32 = 64.0;

#[derive(Debug, Default)]
pub struct Ruler;

impl Ruler {
    pub fn new() -> Self {
        Self
    }

    pub fn height(&self) -> f32 {
        RULER_HEIGHT
    }

    fn playhead_line() -> Element<'static, Message> {
        container("")
            .width(Length::Fixed(2.0))
            .height(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(Background::Color(Color {
                    r: 0.95,
                    g: 0.18,
                    b: 0.14,
                    a: 0.95,
                })),
                ..container::Style::default()
            })
            .into()
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

    pub fn view(&self, playhead_x: Option<f32>, beat_pixels: f32) -> Element<'_, Message> {
        let mut children: Vec<Element<'static, Message>> = vec![
            container("")
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_theme| container::Style {
                    background: Some(Background::Color(Color {
                        r: 0.12,
                        g: 0.12,
                        b: 0.12,
                        a: 1.0,
                    })),
                    ..container::Style::default()
                })
                .into(),
        ];

        let tick_step_beats = Self::step_for_spacing(beat_pixels, MIN_TICK_SPACING_PX);
        let bar_pixels = beat_pixels * BEATS_PER_BAR as f32;
        let label_step_bars = Self::step_for_spacing(bar_pixels, MIN_LABEL_SPACING_PX);
        let total_beats = BARS_TO_DRAW * BEATS_PER_BAR;

        for beat_idx in (0..=total_beats).step_by(tick_step_beats) {
            let x = beat_idx as f32 * beat_pixels;
            let is_bar = beat_idx % BEATS_PER_BAR == 0;
            let is_numbered_bar =
                is_bar && ((beat_idx / BEATS_PER_BAR) % label_step_bars == 0);
            let tick_h = if is_numbered_bar { 8.0 } else { 3.0 };

            children.push(
                pin(
                    container("")
                        .width(Length::Fixed(1.0))
                        .height(Length::Fixed(tick_h))
                        .style(move |_theme| container::Style {
                            background: Some(Background::Color(if is_bar {
                                Color {
                                    r: 0.83,
                                    g: 0.83,
                                    b: 0.83,
                                    a: 0.9,
                                }
                            } else {
                                Color {
                                    r: 0.54,
                                    g: 0.54,
                                    b: 0.54,
                                    a: 0.7,
                                }
                            })),
                            ..container::Style::default()
                        }),
                )
                .position(Point::new(x, RULER_HEIGHT - tick_h - 2.0))
                .into(),
            );
        }

        for bar in (0..BARS_TO_DRAW).step_by(label_step_bars) {
            let x = bar as f32 * BEATS_PER_BAR as f32 * beat_pixels;
            children.push(
                pin(
                    text(bar.to_string()).size(12).color(Color {
                        r: 0.86,
                        g: 0.86,
                        b: 0.86,
                        a: 1.0,
                    }),
                )
                .position(Point::new(x + 4.0, 2.0))
                .into(),
            );
        }

        if let Some(x) = playhead_x {
            children.push(
                pin(Self::playhead_line())
                    .position(Point::new(x.max(0.0), 0.0))
                    .into(),
            );
        }

        Stack::from_vec(children)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}
