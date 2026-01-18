use crate::message::Message;
use iced::widget::{button, container};
use maolan_engine::message::Action;

#[derive(Default)]
pub struct MaolanWorkspace {}

impl MaolanWorkspace {
    pub fn update(&mut self, message: Message) {
        match message {
            _ => {}
        }
    }

    pub fn view(&self) -> iced::Element<'_, Message> {
        container(
            button("something").on_press(Message::Request(Action::Echo("something".to_string()))),
        )
        .into()
    }
}
