use crate::{message::Message, state::Track, style};
use iced::{
    Background, Border, Color, Element, Length,
    widget::{button, column, container, mouse_area, row, text},
};
use maolan_engine::message::Action;
use serde_json::{Value, json};

#[derive(Debug, Default)]
pub struct Tracks {
    selected: Vec<String>,
    tracks: Vec<Track>,
}

impl Tracks {
    pub fn json(&self) -> Value {
        json!(self.tracks)
    }

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
                } => {
                    self.tracks.push(Track::new(
                        name.clone(),
                        *kind,
                        0.0,
                        ins.clone(),
                        audio_outs.clone(),
                        midi_outs.clone(),
                    ));
                }
                Action::DeleteTrack(name) => {
                    self.selected.clear();
                    self.tracks.retain(|track| track.name != *name);
                }
                _ => {}
            },
            Message::SelectTrack(ref name) => {
                self.selected.push(name.clone());
            }
            _ => {}
        }
        self.update_children(message);
    }

    pub fn view(&self) -> Element<'_, Message> {
        let mut result = column![];
        for track in &self.tracks {
            result = result.push(
                mouse_area(
                    container(column![
                        text(track.name.clone()),
                        row![
                            button("R")
                                .padding(3)
                                .style(|theme, _state| { style::arm::style(theme, track.armed) })
                                .on_press(Message::Request(Action::TrackToggleArm(
                                    track.name.clone()
                                ))),
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
                    .width(Length::Fill)
                    .height(Length::Fixed(60.0))
                    .padding(5)
                    .style(|_theme| {
                        use container::Style;

                        Style {
                            background: if self.selected.contains(&track.name) {
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
                .on_press(Message::SelectTrack(track.name.clone())),
            );
        }
        result.into()
    }
}
