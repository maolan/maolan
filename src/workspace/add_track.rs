use crate::message::{AddTrack, Message, Show, TrackKind};
use iced::{
    Alignment, Element, Length,
    widget::{button, column, container, pick_list, row, text, text_input},
};
use iced_aw::number_input;
use maolan_engine::message::Action;

#[derive(Debug)]
pub struct AddTrackView {
    name: String,
    kind: TrackKind,
    ins: usize,
    audio_outs: usize,
    midi_outs: usize,
}

impl AddTrackView {
    pub fn update(&mut self, message: Message) {
        match message {
            Message::AddTrack(a) => match a {
                AddTrack::Name(name) => {
                    self.name = name;
                }
                AddTrack::Kind(kind) => {
                    self.kind = kind;
                }
                AddTrack::Ins(ins) => {
                    self.ins = ins;
                }
                AddTrack::AudioOuts(outs) => {
                    self.audio_outs = outs;
                }
                AddTrack::MIDIOuts(outs) => {
                    self.midi_outs = outs;
                }
            },
            _ => {}
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let kinds = [TrackKind::Audio, TrackKind::MIDI];
        container(
            column![
                row![
                    text("Track type"),
                    pick_list(kinds, Some(self.kind), |kind: TrackKind| Message::AddTrack(
                        AddTrack::Kind(kind)
                    )),
                ],
                row![
                    text("Name:"),
                    text_input("Track name", &self.name)
                        .on_input(|name: String| Message::AddTrack(AddTrack::Name(name)))
                        .width(Length::Fixed(200.0)),
                ]
                .spacing(10),
                match self.kind {
                    TrackKind::Audio => {
                        row![
                            text("Number of inputs:"),
                            number_input(&self.ins, 1..=32, |ins: usize| {
                                Message::AddTrack(AddTrack::Ins(ins))
                            })
                        ]
                        .spacing(10)
                    }
                    _ => row![],
                },
                row![
                    text("Audio outputs:"),
                    number_input(&self.audio_outs, 0..=32, |outs: usize| {
                        Message::AddTrack(AddTrack::AudioOuts(outs))
                    }),
                ]
                .spacing(10),
                row![
                    text("Midi outputs:"),
                    number_input(&self.midi_outs, 0..=32, |outs: usize| {
                        Message::AddTrack(AddTrack::MIDIOuts(outs))
                    }),
                ]
                .spacing(10),
                row![
                    match self.kind {
                        TrackKind::Audio => {
                            button("Create").on_press(Message::Request(Action::AddAudioTrack {
                                name: self.name.clone(),
                                ins: self.ins,
                                audio_outs: self.audio_outs,
                                midi_outs: self.midi_outs,
                            }))
                        }
                        TrackKind::MIDI => {
                            button("Create").on_press(Message::Request(Action::AddMIDITrack {
                                name: self.name.clone(),
                                audio_outs: self.audio_outs,
                                midi_outs: self.midi_outs,
                            }))
                        }
                    },
                    button("Cancel")
                        .on_press(Message::Cancel(Show::AddTrack))
                        .style(button::secondary)
                ]
                .spacing(10),
            ]
            .align_x(Alignment::End)
            .spacing(10),
        )
        .padding(20)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
    }
}

impl Default for AddTrackView {
    fn default() -> Self {
        Self {
            name: "".to_string(),
            kind: TrackKind::Audio,
            ins: 1,
            audio_outs: 1,
            midi_outs: 0,
        }
    }
}
