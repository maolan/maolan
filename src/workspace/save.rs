use crate::message::Message;
use iced::{
    Alignment, Element, Length,
    widget::{button, column, container, row},
};

#[derive(Debug)]
pub struct SaveView {}

impl SaveView {
    pub fn update(&mut self, _message: Message) {}

    pub fn view(&self) -> Element<'_, Message> {
        container(
            column![
                row![
                    button("Save").on_press(Message::Save),
                    button("Cancel")
                        .on_press(Message::Cancel)
                        .style(button::secondary),
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

impl Default for SaveView {
    fn default() -> Self {
        Self {}
    }
}
