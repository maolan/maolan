use crate::{message::Message, state::State, widget::piano::PianoKeyboard};
use iced::{
    Background, Border, Color, Element, Length, Point, Renderer, Theme,
    widget::{Stack, canvas, column, container, mouse_area, pin, row, text},
};
use std::collections::HashSet;

#[derive(Debug)]
pub struct Editor {
    state: State,
    active_notes: HashSet<u8>,
}

impl Editor {
    pub fn new(state: State) -> Self {
        Self {
            state,
            active_notes: HashSet::new(),
        }
    }

    pub fn update(&mut self, _: Message) {}

    pub fn view(&self) -> Element<'_, Message> {
        let mut result = column![];
        let state = self.state.blocking_read();

        for track in &state.tracks {
            let mut clips: Vec<Element<'_, Message, Theme, Renderer>> = vec![];
            let height = track.height;
            let track_name = track.name.clone();

            for clip in &track.clips {
                let clip_name = clip.name.clone();
                let track_name_left = track_name.clone();
                let track_name_right = track_name.clone();
                let clip_name_left = clip_name.clone();
                let clip_name_right = clip_name.clone();

                // Left resize handle
                let left_handle = mouse_area(
                    container("")
                        .width(Length::Fixed(5.0))
                        .height(Length::Fill)
                        .style(|_theme| {
                            use container::Style;
                            Style {
                                background: Some(Background::Color(Color {
                                    r: 0.2,
                                    g: 0.4,
                                    b: 0.6,
                                    a: 0.9,
                                })),
                                ..Style::default()
                            }
                        }),
                )
                .on_press(Message::ClipResizeStart(
                    track_name_left,
                    clip_name_left,
                    false,
                ));

                // Right resize handle
                let right_handle = mouse_area(
                    container("")
                        .width(Length::Fixed(5.0))
                        .height(Length::Fill)
                        .style(|_theme| {
                            use container::Style;
                            Style {
                                background: Some(Background::Color(Color {
                                    r: 0.2,
                                    g: 0.4,
                                    b: 0.6,
                                    a: 0.9,
                                })),
                                ..Style::default()
                            }
                        }),
                )
                .on_press(Message::ClipResizeStart(
                    track_name_right,
                    clip_name_right,
                    true,
                ));

                // Clip content (middle part)
                let clip_content = mouse_area(
                    container(text(clip_name.clone()).size(12))
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .padding(5)
                        .style(|_theme| {
                            use container::Style;
                            Style {
                                background: Some(Background::Color(Color {
                                    r: 0.3,
                                    g: 0.5,
                                    b: 0.7,
                                    a: 0.8,
                                })),
                                ..Style::default()
                            }
                        }),
                );

                // Combine handles and content in a row
                let clip_widget = container(row![left_handle, clip_content, right_handle])
                    .width(Length::Fixed(clip.length))
                    .height(Length::Fill)
                    .style(|_theme| {
                        use container::Style;
                        Style {
                            background: None,
                            border: Border {
                                color: Color {
                                    r: 0.2,
                                    g: 0.4,
                                    b: 0.6,
                                    a: 1.0,
                                },
                                width: 1.0,
                                radius: 3.0.into(),
                            },
                            ..Style::default()
                        }
                    });

                clips.push(
                    pin(clip_widget)
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
                .height(Length::Fixed(height))
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
        result
            .push(canvas(PianoKeyboard {
                pressed_notes: self.active_notes.clone(),
            }))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}
