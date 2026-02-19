use crate::message::{AddTrack, Message};
use iced::{
    Alignment, Element, Length,
    widget::{button, column, container, row, text, text_input},
};
use iced_aw::number_input;
use maolan_engine::message::Action;

#[derive(Debug)]
pub struct AddTrackView {
    name: String,
    audio_ins: usize,
    audio_outs: usize,
    midi_ins: usize,
    midi_outs: usize,
}

impl AddTrackView {
    pub fn update(&mut self, message: Message) {
        if let Message::AddTrack(a) = message {
            match a {
                AddTrack::Name(name) => {
                    self.name = name;
                }
                AddTrack::AudioIns(ins) => {
                    self.audio_ins = ins;
                }
                AddTrack::MIDIIns(ins) => {
                    self.midi_ins = ins;
                }
                AddTrack::AudioOuts(outs) => {
                    self.audio_outs = outs;
                }
                AddTrack::MIDIOuts(outs) => {
                    self.midi_outs = outs;
                }
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let create = if self.name.trim().is_empty() {
            button("Create")
        } else {
            button("Create").on_press(Message::Request(Action::AddTrack {
                name: self.name.clone(),
                audio_ins: self.audio_ins,
                midi_ins: self.midi_ins,
                audio_outs: self.audio_outs,
                midi_outs: self.midi_outs,
            }))
        };

        container(
            column![
                row![
                    text("Name:"),
                    text_input("Track name", &self.name)
                        .on_input(|name: String| Message::AddTrack(AddTrack::Name(name)))
                        .width(Length::Fixed(200.0)),
                ]
                .spacing(10),
                row![
                    text("Number of audio inputs:"),
                    number_input(&self.audio_ins, 1..=32, |ins: usize| {
                        Message::AddTrack(AddTrack::AudioIns(ins))
                    })
                ]
                .spacing(10),
                row![
                    text("Number of midi inputs:"),
                    number_input(&self.midi_ins, 1..=32, |ins: usize| {
                        Message::AddTrack(AddTrack::MIDIIns(ins))
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
                    create,
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
            audio_ins: 1,
            audio_outs: 1,
            midi_ins: 0,
            midi_outs: 0,
            name: "".to_string(),
        }
    }
}
