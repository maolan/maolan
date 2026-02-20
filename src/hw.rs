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

    pub fn view(&self) -> iced::Element<'_, Message> {
        let (available_hw, selected_hw, available_midi_hw, selected_midi_hw, opened_midi_hw) = {
            let state = self.state.blocking_read();
            (
                state.available_hw.clone(),
                state.selected_hw.clone(),
                state.available_midi_hw.clone(),
                state.selected_midi_hw.clone(),
                state.opened_midi_hw.clone(),
            )
        };
        let mut submit = button("Open Audio");
        if let Some(ref hw) = selected_hw {
            submit = submit.on_press(Message::Request(Action::OpenAudioDevice(hw.to_string())));
        }
        let mut open_midi = button("Add MIDI Input");
        if let Some(ref hw) = selected_midi_hw {
            open_midi =
                open_midi.on_press(Message::Request(Action::OpenMidiDevice(hw.to_string())));
        }
        container(
            column![
                pick_list(available_hw, selected_hw, Message::HWSelected)
                    .placeholder("Choose audio device"),
                submit,
                pick_list(available_midi_hw, selected_midi_hw, Message::MIDIHWSelected)
                    .placeholder("Choose MIDI input device"),
                open_midi,
                opened_midi_hw.iter().fold(column![], |acc, name| acc
                    .push(iced::widget::text(name.clone()))),
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
