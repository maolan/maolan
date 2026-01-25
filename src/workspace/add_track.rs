use crate::message::Message;
use iced::{
    Element,
    widget::{button, column},
};
use maolan_engine::message::Action;

#[derive(Debug, Default)]
pub struct AddTrack {}

impl AddTrack {
    pub fn update(&mut self, _message: Message) {}

    pub fn view(&self) -> Element<'_, Message> {
        column![
            button("Create").on_press(Message::Request(Action::AddAudioTrack {
                name: "Traka".to_string(),
                ins: 1,
                audio_outs: 1,
                midi_outs: 0
            }))
        ]
        .into()
    }
}
