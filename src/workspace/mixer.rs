use crate::{message::Message, state::State, style, widget::slider::Slider};

use iced::{
    Alignment, Background, Border, Color, Element, Length,
    widget::{button, column, container, mouse_area, row},
};
use maolan_engine::message::Action;

#[derive(Debug, Default)]
pub struct Mixer {
    state: State,
}

impl Mixer {
    pub fn new(state: State) -> Self {
        Self { state }
    }

    pub fn update(&mut self, _: Message) {}

    pub fn view(&self) -> Element<'_, Message> {
        let mut result = row![];
        let (tracks, selected) = {
            let state = self.state.blocking_read();
            (state.tracks.clone(), state.selected.clone())
        };

        for track in tracks {
            let selected = selected.contains(&track.name);
            let t_name = track.name.clone();

            result = result.push(
                mouse_area(
                    container(column![
                        Slider::new(-90.0..=20.0, track.level, {
                            let name = track.name.clone();
                            move |new_val| {
                                Message::Request(Action::TrackLevel(name.clone(), new_val))
                            }
                        })
                        .width(Length::Fixed(40.0))
                        .dark_rect_style(),
                        // .shift_step(0.1),
                        row![
                            button("R")
                                .padding(3)
                                .style(move |theme, _state| {
                                    style::arm::style(theme, track.armed)
                                })
                                .on_press(Message::Request(Action::TrackToggleArm(t_name.clone()))),
                            button("M")
                                .padding(3)
                                .style(move |theme, _state| {
                                    style::mute::style(theme, track.muted)
                                })
                                .on_press(Message::Request(Action::TrackToggleMute(
                                    t_name.clone()
                                ))),
                            button("S")
                                .padding(3)
                                .style(move |theme, _state| {
                                    style::solo::style(theme, track.soloed)
                                })
                                .on_press(Message::Request(Action::TrackToggleSolo(
                                    t_name.clone()
                                ))),
                        ]
                    ])
                    .padding(5)
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center)
                    .style(move |_theme| {
                        use container::Style;

                        Style {
                            background: if selected {
                                Some(Background::Color(Color {
                                    r: 1.0,
                                    g: 1.0,
                                    b: 1.0,
                                    a: 1.0,
                                }))
                            } else {
                                Some(Background::Color(Color {
                                    r: 0.8,
                                    g: 0.8,
                                    b: 0.8,
                                    a: 0.8,
                                }))
                            },
                            border: Border {
                                color: Color {
                                    r: 0.0,
                                    g: 0.0,
                                    b: 0.0,
                                    a: 1.0,
                                },
                                width: 1.0,
                                radius: 5.0.into(),
                            },
                            ..Style::default()
                        }
                    }),
                )
                .on_press(Message::SelectTrack(t_name.clone())),
            )
        }
        result.into()
    }
}
