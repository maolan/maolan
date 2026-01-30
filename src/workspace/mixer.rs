use crate::{
    message::Message,
    state::{Track},
    style,
};

use iced::{
    Alignment, Background, Border, Color, Element,
    widget::{button, column, container, row, vertical_slider},
};
use maolan_engine::message::{Action, TrackKind};

#[derive(Debug, Default)]
pub struct Mixer {
    tracks: Vec<Track>,
}

impl Mixer {
    fn update_children(&mut self, message: Message) {
        match message {
            _ => {
                for track in &mut self.tracks {
                    track.update(message.clone());
                }
            }
        }
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::Response(Ok(ref a)) => match a {
                Action::AddTrack {
                    name,
                    kind,
                    ins,
                    audio_outs,
                    midi_outs,
                } => match kind {
                    TrackKind::Audio => {
                        self.tracks.push(Track::new(
                            name.clone(),
                            TrackKind::Audio,
                            0.0,
                            ins.clone(),
                            audio_outs.clone(),
                            midi_outs.clone(),
                        ));
                    }
                    TrackKind::MIDI => {
                        self.tracks.push(Track::new(
                            name.clone(),
                            TrackKind::MIDI,
                            0.0,
                            ins.clone(),
                            audio_outs.clone(),
                            midi_outs.clone(),
                        ));
                    }
                },
                _ => {}
            },
            _ => {}
        }
        self.update_children(message);
    }

    pub fn view(&self) -> Element<'_, Message> {
        let mut result = row![];
        for track in &self.tracks {
            result = result.push(
                container(column![
                    vertical_slider(-90.0..=20.0, track.level, |new_val| {
                        Message::Request(Action::TrackLevel(track.name.clone(), new_val))
                    })
                    .shift_step(0.1),
                    row![
                        button("R")
                            .padding(3)
                            .style(|theme, _state| { style::arm::style(theme, track.armed) })
                            .on_press(Message::Request(Action::TrackToggleArm(track.name.clone()))),
                        button("M")
                            .padding(3)
                            .style(|theme, _state| { style::mute::style(theme, track.muted) })
                            .on_press(Message::Request(Action::TrackToggleMute(
                                track.name.clone()
                            ))),
                        button("S")
                            .padding(3)
                            .style(|theme, _state| { style::solo::style(theme, track.soloed) })
                            .on_press(Message::Request(Action::TrackToggleSolo(
                                track.name.clone()
                            ))),
                    ]
                ])
                .padding(5)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .style(|_theme| {
                    use container::Style;

                    Style {
                        background: Some(Background::Color(Color {
                            r: 0.8,
                            g: 0.8,
                            b: 0.8,
                            a: 0.8,
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
                        ..Style::default()
                    }
                }),
            )
        }
        result.into()
    }
}
