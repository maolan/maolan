use crate::message::Message;
use iced::{
    Alignment, Element, Length,
    widget::{button, column, container, row, text_input},
};

#[derive(Debug)]
pub struct SaveView {
    path: String,
}

impl SaveView {
    pub fn update(&mut self, message: Message) {
        match message {
            Message::SavePath(path) => {
                self.path = path;
            }
            _ => {}
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        container(
            column![
                text_input("Path", &self.path).on_input(Message::SavePath),
                row![
                    button("Save").on_press_with(|| { Message::Save(self.path.clone()) }),
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
        Self {
            path: "/tmp/".to_string(),
        }
    }
}
