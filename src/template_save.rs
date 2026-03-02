use crate::{message::Message, state::State};
use iced::{
    Alignment, Element, Length,
    widget::{button, column, container, row, text, text_input},
};

#[derive(Debug)]
pub struct TemplateSaveView {
    state: State,
}

impl TemplateSaveView {
    pub fn new(state: State) -> Self {
        Self { state }
    }

    pub fn update(&mut self, message: Message) {
        if let Message::TemplateSaveInput(input) = message
            && let Some(dialog) = &mut self.state.blocking_write().template_save_dialog
        {
            dialog.name = input;
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let state = self.state.blocking_read();
        let Some(dialog) = &state.template_save_dialog else {
            return container("").into();
        };

        let template_name = &dialog.name;
        let can_confirm = !template_name.trim().is_empty();

        let save_button = if can_confirm {
            button("Save").on_press(Message::TemplateSaveConfirm)
        } else {
            button("Save")
        };

        container(
            column![
                text("Save as template").size(16),
                row![
                    text("Template name:"),
                    text_input("Enter template name", template_name)
                        .on_input(Message::TemplateSaveInput)
                        .width(Length::Fixed(300.0)),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                row![
                    save_button,
                    button("Cancel")
                        .on_press(Message::TemplateSaveCancel)
                        .style(button::secondary)
                ]
                .spacing(10),
            ]
            .align_x(Alignment::End)
            .spacing(15),
        )
        .padding(20)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
    }
}
