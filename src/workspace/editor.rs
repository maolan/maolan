use crate::{message::Message, state::State};
use iced::{
    Background, Border, Color, Element, Length, Point, Renderer, Theme,
    widget::{Stack, column, container, pin, text},
};

#[derive(Debug, Default)]
pub struct Editor {
    state: State,
}

impl Editor {
    pub fn new(state: State) -> Self {
        Self { state }
    }

    pub fn update(&mut self, _: Message) {}

    pub fn view(&self) -> Element<'_, Message> {
        let mut result = column![];
        let state = self.state.blocking_read();

        for track in &state.tracks {
            let mut clips: Vec<Element<'_, Message, Theme, Renderer>> = vec![];
            for clip in &track.clips {
                clips.push(
                    pin(text(clip.name.clone()))
                        .position(Point::new(clip.start, 0.0))
                        .into(),
                );
            }
            result = result.push(
                container(
                    Stack::from_vec(clips)
                        .height(Length::Fill)
                        .width(Length::Fill),
                )
                .width(Length::Fill)
                .height(Length::Fixed(60.0))
                .padding(5)
                .style(|_theme| {
                    use container::Style;

                    Style {
                        background: Some(Background::Color(Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 0.0,
                        })),
                        border: Border {
                            color: Color {
                                r: 0.0,
                                g: 0.0,
                                b: 0.0,
                                a: 1.0,
                            },
                            width: 1.0,
                            radius: 0.0.into(),
                        },
                        ..Style::default()
                    }
                }),
            );
        }
        result.into()
    }
}
