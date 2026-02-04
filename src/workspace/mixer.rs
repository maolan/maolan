use crate::{message::Message, state::State, style, widget::slider::Slider};

use iced::{
    Alignment, Background, Border, Color, Element, Length,
    widget::{button, column, container, mouse_area, row},
};
use maolan_engine::message::Action;

#[derive(Debug, Default)]
pub struct Mixer {
    state: State,
    master: f32,
}

impl Mixer {
    pub fn new(state: State) -> Self {
        Self { state, master: 0.0 }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let mut strips = row![].width(Length::Fill);
        let (tracks, selected, height) = {
            let state = self.state.blocking_read();
            (
                state.tracks.clone(),
                state.selected.clone(),
                state.mixer_height,
            )
        };

        for track in tracks {
            let selected = selected.contains(&track.name);
            let t_name = track.name.clone();

            strips = strips.push(
                mouse_area(
                    container(column![
                        Slider::new(-90.0..=20.0, track.level, {
                            let name = track.name.clone();
                            move |new_val| {
                                Message::Request(Action::TrackLevel(name.clone(), new_val))
                            }
                        })
                        .width(Length::Fixed(20.0)),
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
                    .style(move |_theme| container::Style {
                        background: if selected {
                            Some(Background::Color(Color {
                                r: 1.0,
                                g: 1.0,
                                b: 1.0,
                                a: 0.1,
                            }))
                        } else {
                            Some(Background::Color(Color {
                                r: 0.0,
                                g: 0.0,
                                b: 0.0,
                                a: 0.0,
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
                        ..container::Style::default()
                    }),
                )
                .on_press(Message::SelectTrack(t_name.clone())),
            )
        }

        let master = row![mouse_area(
            container(column![
                Slider::new(-90.0..=20.0, self.master, {
                    move |new_val| {
                        Message::Request(Action::TrackLevel("master".to_string(), new_val))
                    }
                }),
                // .shift_step(0.1),
                row![
                    button("M")
                        .padding(3)
                        .style(move |theme, _state| { style::mute::style(theme, false) })
                        .on_press(Message::Request(Action::TrackToggleMute(
                            "master".to_string()
                        ))),
                    button("S")
                        .padding(3)
                        .style(move |theme, _state| { style::solo::style(theme, false) })
                        .on_press(Message::Request(Action::TrackToggleSolo(
                            "master".to_string()
                        ))),
                ]
            ])
            .padding(5)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .style(move |_theme| container::Style {
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
                    radius: 5.0.into(),
                },
                ..container::Style::default()
            }),
        )];

        row![strips, master].height(height).into()
    }
}
