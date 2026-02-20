use crate::{
    message::Message,
    state::State,
    style,
    widget::{horizontal_slider::HorizontalSlider, slider::Slider},
};

use iced::{
    Alignment, Background, Border, Color, Element, Length, Point,
    widget::{Space, Stack, button, column, container, mouse_area, pin, row, text},
};
use maolan_engine::message::Action;

#[derive(Debug, Default)]
pub struct Mixer {
    state: State,
}

impl Mixer {
    const FADER_MIN_DB: f32 = -90.0;
    const FADER_MAX_DB: f32 = 20.0;
    const FADER_WITH_TICKS_WIDTH: f32 = 58.0;

    pub fn new(state: State) -> Self {
        Self { state }
    }

    fn level_to_meter_fill(level_db: f32) -> f32 {
        ((level_db - Self::FADER_MIN_DB) / (Self::FADER_MAX_DB - Self::FADER_MIN_DB))
            .clamp(0.0, 1.0)
    }

    fn fader_height_from_panel(height: Length) -> f32 {
        match height {
            Length::Fixed(panel_h) => (panel_h - 84.0).max(80.0),
            _ => 300.0,
        }
    }

    fn db_to_y(db: f32, fader_height: f32) -> f32 {
        let normalized =
            ((db - Self::FADER_MIN_DB) / (Self::FADER_MAX_DB - Self::FADER_MIN_DB)).clamp(0.0, 1.0);
        fader_height * (1.0 - normalized)
    }

    fn slider_with_ticks<F>(
        value: f32,
        fader_height: f32,
        on_change: F,
    ) -> Element<'static, Message>
    where
        F: Fn(f32) -> Message + 'static,
    {
        let tick_values = [
            20.0, 12.0, 6.0, 0.0, -6.0, -12.0, -18.0, -24.0, -36.0, -48.0, -60.0, -72.0, -90.0,
        ];
        let mut marks: Vec<Element<'static, Message>> = vec![];
        for db in tick_values {
            let y = Self::db_to_y(db, fader_height).clamp(0.0, fader_height - 1.0);
            let label_y = (y - 5.0).clamp(0.0, (fader_height - 11.0).max(0.0));
            let label = if db > 0.0 {
                format!("+{}", db as i32)
            } else {
                format!("{}", db as i32)
            };
            marks.push(
                pin(row![
                    container("")
                        .width(Length::Fixed(6.0))
                        .height(Length::Fixed(1.0))
                        .style(|_theme| container::Style {
                            background: Some(Background::Color(Color {
                                r: 0.7,
                                g: 0.7,
                                b: 0.7,
                                a: 0.8,
                            })),
                            ..container::Style::default()
                        }),
                    text(label).size(10),
                ]
                .spacing(3)
                .align_y(Alignment::Center))
                .position(Point::new(0.0, label_y))
                .into(),
            );
        }
        let scale = Stack::from_vec(marks)
            .width(Length::Fixed(34.0))
            .height(Length::Fixed(fader_height));
        row![
            Slider::new(Self::FADER_MIN_DB..=Self::FADER_MAX_DB, value, on_change)
                .width(Length::Fixed(20.0))
                .height(Length::Fixed(fader_height)),
            scale,
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .into()
    }

    fn balance_slider<F>(value: f32, on_change: F) -> Element<'static, Message>
    where
        F: Fn(f32) -> Message + 'static,
    {
        HorizontalSlider::new(-1.0..=1.0, value.clamp(-1.0, 1.0), on_change)
            .width(Length::Fixed(Self::FADER_WITH_TICKS_WIDTH))
            .height(Length::Fixed(14.0))
            .into()
    }

    fn vu_meter(channels: usize, levels_db: &[f32], meter_h: f32) -> Element<'static, Message> {
        let channels = channels.max(1);
        let strip_w = 1.0;

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
        let (
            tracks,
            selected,
            height,
            hw_out_channels,
            hw_out_level,
            hw_out_balance,
            hw_out_muted,
            hw_out_meter_db,
        ) = {
            let state = self.state.blocking_read();
            (
                state.tracks.clone(),
                state.selected.clone(),
                state.mixer_height,
                state.hw_out.as_ref().map(|hw| hw.channels).unwrap_or(0),
                state.hw_out_level,
                state.hw_out_balance,
                state.hw_out_muted,
                state.hw_out_meter_db.clone(),
            )
        };
        let fader_height = Self::fader_height_from_panel(height);

        for track in tracks {
            let selected = selected.contains(&track.name);
            let t_name = track.name.clone();

            strips = strips.push(
                mouse_area(
                    container(
                        column![
                            row![
                                column![
                                    if track.audio.outs == 2 {
                                        Self::balance_slider(track.balance, {
                                            let name = track.name.clone();
                                            move |new_val| {
                                                Message::Request(Action::TrackBalance(
                                                    name.clone(),
                                                    new_val
                                                ))
                                            }
                                        })
                                    } else {
                                        Space::new()
                                            .width(Length::Fixed(Self::FADER_WITH_TICKS_WIDTH))
                                            .height(Length::Fixed(14.0))
                                            .into()
                                    },
                                    Self::slider_with_ticks(track.level, fader_height, {
                                        let name = track.name.clone();
                                        move |new_val| {
                                            Message::Request(Action::TrackLevel(name.clone(), new_val))
                                        }
                                    }),
                                ]
                                .spacing(4),
                                Self::vu_meter(track.audio.outs, &track.meter_out_db, fader_height),
                            ]
                            .height(Length::Fill)
                            .spacing(6)
                            .align_y(Alignment::Center),
                            // .shift_step(0.1),
                            row![
                                button("R")
                                    .padding(3)
                                    .style(move |theme, _state| {
                                        style::arm::style(theme, track.armed)
                                    })
                                    .on_press(Message::Request(Action::TrackToggleArm(
                                        t_name.clone()
                                    ))),
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
                        ]
                        .height(Length::Fill),
                    )
                    .padding(5)
                    .height(Length::Fill)
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
            container(
                column![
                    row![
                        column![
                            if hw_out_channels == 2 {
                                Self::balance_slider(hw_out_balance, move |new_val| {
                                    Message::Request(Action::TrackBalance("hw:out".to_string(), new_val))
                                })
                            } else {
                                Space::new()
                                    .width(Length::Fixed(Self::FADER_WITH_TICKS_WIDTH))
                                    .height(Length::Fixed(14.0))
                                    .into()
                            },
                            Self::slider_with_ticks(hw_out_level, fader_height, {
                                move |new_val| {
                                    Message::Request(Action::TrackLevel("hw:out".to_string(), new_val))
                                }
                            }),
                        ]
                        .spacing(4),
                        Self::vu_meter(hw_out_channels.max(1), &hw_out_meter_db, fader_height),
                    ]
                    .height(Length::Fill)
                    .spacing(6)
                    .align_y(Alignment::Center),
                    // .shift_step(0.1),
                    row![
                        button("M")
                            .padding(3)
                            .style(move |theme, _state| { style::mute::style(theme, hw_out_muted) })
                            .on_press(Message::Request(Action::TrackToggleMute(
                                "hw:out".to_string()
                            ))),
                    ]
                ]
                .height(Length::Fill)
            )
            .padding(5)
            .height(Length::Fill)
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

        mouse_area(
            row![strips, master]
                .height(height)
                .align_y(Alignment::Start),
        )
        .on_press(Message::DeselectAll)
        .into()
    }
}
