use crate::{message::Message, state::State, style, widget::slider::Slider};

use iced::{
    Alignment, Background, Border, Color, Element, Length,
    widget::{Space, button, column, container, mouse_area, row},
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

    fn level_to_meter_fill(level_db: f32) -> f32 {
        ((level_db + 90.0) / 110.0).clamp(0.0, 1.0)
    }

    fn vu_meter(channels: usize, levels_db: &[f32]) -> Element<'static, Message> {
        let channels = channels.max(1);
        let meter_h = 300.0;
        let strip_w = 3.5;

        let mut strips = row![].spacing(3).align_y(Alignment::End);
        for channel_idx in 0..channels {
            let db = levels_db.get(channel_idx).copied().unwrap_or(-90.0);
            let fill = Self::level_to_meter_fill(db);
            let filled_h = (meter_h * fill).max(1.0);
            let empty_h = (meter_h - filled_h).max(0.0);
            let strip = container(column![
                Space::new().height(Length::Fixed(empty_h)),
                container("")
                    .width(Length::Fill)
                    .height(Length::Fixed(filled_h))
                    .style(|_theme| container::Style {
                        background: Some(Background::Color(Color {
                            r: 0.2,
                            g: 0.8,
                            b: 0.35,
                            a: 0.95,
                        })),
                        ..container::Style::default()
                    }),
            ])
            .width(Length::Fixed(strip_w))
            .height(Length::Fixed(meter_h))
            .style(|_theme| container::Style {
                background: Some(Background::Color(Color {
                    r: 0.08,
                    g: 0.08,
                    b: 0.08,
                    a: 1.0,
                })),
                border: Border {
                    color: Color {
                        r: 0.2,
                        g: 0.2,
                        b: 0.2,
                        a: 1.0,
                    },
                    width: 1.0,
                    radius: 2.0.into(),
                },
                ..container::Style::default()
            });
            strips = strips.push(strip);
        }

        container(strips).into()
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
                        row![
                            Slider::new(-90.0..=20.0, track.level, {
                                let name = track.name.clone();
                                move |new_val| {
                                    Message::Request(Action::TrackLevel(name.clone(), new_val))
                                }
                            })
                            .width(Length::Fixed(20.0)),
                            Self::vu_meter(track.audio.outs, &track.meter_out_db),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
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
                row![
                    Slider::new(-90.0..=20.0, self.master, {
                        move |new_val| {
                            Message::Request(Action::TrackLevel("master".to_string(), new_val))
                        }
                    }),
                    Self::vu_meter(2, &[self.master, self.master]),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
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

        mouse_area(row![strips, master].height(height))
            .on_press(Message::DeselectAll)
            .into()
    }
}
