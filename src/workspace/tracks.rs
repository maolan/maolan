use super::meta::{Track, TrackType};
use crate::message::Message;
use iced::{
    Background, Border, Color, Element, Length,
    widget::{button, column, container, row, text},
};
use maolan_engine::message::Action;

#[derive(Debug, Default)]
pub struct Tracks {
    tracks: Vec<Track>,
}

impl Tracks {
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
                Action::TrackToggleMute(_name) => {
                    self.update_children(message);
                }
                _ => {}
            },
            _ => {
                self.update_children(message);
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let mut result = column![];
        for track in &self.tracks {
            result = result.push(
                container(column![
                    text(track.name.clone()),
                    row![
                        button("R").padding(3),
                        button("M").padding(3),
                        button("S").padding(3),
                    ]
                ])
                .width(Length::Fill)
                .height(Length::Fixed(60.0))
                .padding(5)
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
                            color: Color{ r: 0.0, g: 0.0, b: 0.0, a: 1.0},
                            width: 1.0,
                            radius: 5.0.into(),
                        },
                        ..Style::default()
                    }
                }),
            );
        }
        result.into()
    }
}
