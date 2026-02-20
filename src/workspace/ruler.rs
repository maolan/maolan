use crate::message::Message;
use iced::{
    Background, Color, Element, Length, Point,
    widget::{Stack, container, pin, text},
};

const RULER_HEIGHT: f32 = 28.0;
const BEAT_PIXELS: f32 = 120.0;
const BEATS_PER_BAR: usize = 4;
const BARS_TO_DRAW: usize = 256;

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

    pub fn view(&self, playhead_x: Option<f32>) -> Element<'_, Message> {
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

        for bar in 0..BARS_TO_DRAW {
            for beat in 0..BEATS_PER_BAR {
                let x = (bar * BEATS_PER_BAR + beat) as f32 * BEAT_PIXELS;
                let is_bar = beat == 0;
                let tick_h = if is_bar { 16.0 } else { 9.0 };

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

                if is_bar {
                    children.push(
                        pin(
                            text((bar + 1).to_string())
                                .size(12)
                                .color(Color {
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
            }
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
