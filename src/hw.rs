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
}
