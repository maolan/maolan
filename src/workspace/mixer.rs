use super::meta::{Track, TrackType};

use crate::message::Message;
use iced::{
    Alignment, Background, Border, Color, Element,
    widget::{button, column, container, row, vertical_slider},
};
use maolan_engine::message::Action;

#[derive(Debug, Default)]
pub struct Mixer {
    tracks: Vec<Track>,
}

impl Mixer {
    pub fn update(&mut self, message: Message) {
        match message {
            Message::Response(Ok(ref a)) => match a {
                Action::AddAudioTrack {
                    name,
                    ins,
                    audio_outs,
                    midi_outs,
                } => {
                    self.tracks.push(Track::new(
                        name.clone(),
                        0.0,
                        ins.clone(),
                        TrackType::Audio,
                        audio_outs.clone(),
                        midi_outs.clone(),
                    ));
                }
                Action::AddMIDITrack {
                    name,
                    midi_outs,
                    audio_outs,
                } => {
                    self.tracks.push(Track::new(
                        name.clone(),
                        0.0,
                        0,
                        TrackType::MIDI,
                        audio_outs.clone(),
                        midi_outs.clone(),
                    ));
                }
                Action::TrackLevel(name, value) => {
                    for track in &mut self.tracks {
                        if track.name == *name {
                            track.level = *value;
                        }
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let mut result = row![];
        for track in &self.tracks {
            result =
                result.push(
                    container(column![
                        vertical_slider(-90.0..=20.0, track.level, |new_val| {
                            Message::Request(Action::TrackLevel(track.name.clone(), new_val))
                        })
                        .shift_step(0.1),
                        row![
                            button("R").padding(3).on_press(Message::Request(
                                Action::TrackToggleArm(track.name.clone())
                            )),
                            button("M").padding(3).on_press(Message::Request(
                                Action::TrackToggleMute(track.name.clone())
                            )),
                            button("S").padding(3).on_press(Message::Request(
                                Action::TrackToggleSolo(track.name.clone())
                            )),
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
