use crate::message::{AddTrack, Message};
use iced::{
    Alignment, Element, Length,
    widget::{button, column, container, pick_list, row, text, text_input},
};
use iced_aw::number_input;
use maolan_engine::message::{Action, TrackKind};

#[derive(Debug)]
pub struct AddTrackView {
    audio_outs: usize,
    ins: usize,
    kind: TrackKind,
    midi_outs: usize,
    name: String,
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
                row![
                    text("Number of inputs:"),
                    number_input(&self.ins, 1..=32, |ins: usize| {
                        Message::AddTrack(AddTrack::Ins(ins))
                    })
                ]
                .spacing(10),
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
                    button("Create").on_press(Message::Request(Action::AddTrack {
                        name: self.name.clone(),
                        kind: self.kind,
                        ins: self.ins,
                        audio_outs: self.audio_outs,
                        midi_outs: self.midi_outs,
                    })),
                    button("Cancel")
                        .on_press(Message::Cancel)
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
            audio_outs: 1,
            ins: 1,
            kind: TrackKind::Audio,
            midi_outs: 0,
            name: "".to_string(),
        }
    }
}
