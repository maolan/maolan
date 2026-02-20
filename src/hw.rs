use crate::{message::Message, state::State};
use iced::{
    Alignment, Length,
    widget::{button, column, container, pick_list},
};
use maolan_engine::message::Action;

pub struct HW {
    state: State,
}

impl HW {
    pub fn new(state: State) -> Self {
        Self { state }
    }

    pub fn audio_view(&self) -> iced::Element<'_, Message> {
        let (available_hw, selected_hw) = {
            let state = self.state.blocking_read();
            (state.available_hw.clone(), state.selected_hw.clone())
        };
        let mut submit = button("Open Audio");
        if let Some(ref hw) = selected_hw {
            submit = submit.on_press(Message::Request(Action::OpenAudioDevice(hw.to_string())));
        }
        container(
            column![
                pick_list(available_hw, selected_hw, Message::HWSelected)
                    .placeholder("Choose audio device"),
                submit,
            ]
            .spacing(10),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
    }

    pub fn midi_view(&self) -> iced::Element<'_, Message> {
        let (
            available_midi_hw,
            selected_midi_in_hw,
            selected_midi_out_hw,
            opened_midi_in_hw,
            opened_midi_out_hw,
        ) = {
            let state = self.state.blocking_read();
            (
                state.available_midi_hw.clone(),
                state.selected_midi_in_hw.clone(),
                state.selected_midi_out_hw.clone(),
                state.opened_midi_in_hw.clone(),
                state.opened_midi_out_hw.clone(),
            )
        };
        let mut open_midi = button("Add MIDI Input");
        if let Some(ref hw) = selected_midi_in_hw {
            open_midi = open_midi.on_press(Message::Request(Action::OpenMidiInputDevice(
                hw.to_string(),
            )));
        }
        let mut open_midi_out = button("Add MIDI Output");
        if let Some(ref hw) = selected_midi_out_hw {
            open_midi_out = open_midi_out.on_press(Message::Request(Action::OpenMidiOutputDevice(
                hw.to_string(),
            )));
        }
        container(
            column![
                pick_list(
                    available_midi_hw.clone(),
                    selected_midi_in_hw,
                    Message::MIDIHWSelected
                )
                .placeholder("Choose MIDI input device"),
                open_midi,
                opened_midi_in_hw
                    .iter()
                    .fold(column![], |acc, name| acc.push(iced::widget::text(name.clone()))),
                pick_list(
                    available_midi_hw,
                    selected_midi_out_hw,
                    Message::MIDIHWOutSelected
                )
                .placeholder("Choose MIDI output device"),
                open_midi_out,
                opened_midi_out_hw
                    .iter()
                    .fold(column![], |acc, name| acc.push(iced::widget::text(name.clone()))),
                button("Close").on_press(Message::Cancel),
            ]
            .spacing(10),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
    }
}
