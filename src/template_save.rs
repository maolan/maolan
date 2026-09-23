use crate::{message::Message, state::State};
use maolan_widgets::iced::{
    Alignment, Element, Length,
    widget::{button, column, container, row, text, text_input},
};

#[derive(Debug)]
pub struct TemplateSaveView {
    pub dialog: Option<crate::state::TemplateSaveDialog>,
}

impl TemplateSaveView {
    pub fn new(_state: State) -> Self {
        Self { dialog: None }
    }

    pub fn open(&mut self, dialog: crate::state::TemplateSaveDialog) {
        self.dialog = Some(dialog);
    }

    pub fn close(&mut self) {
        self.dialog = None;
    }

    pub fn is_open(&self) -> bool {
        self.dialog.is_some()
    }

    pub fn update(&mut self, message: &Message) {
        if let Message::TemplateSaveInput(input) = message
            && let Some(dialog) = &mut self.dialog
        {
            dialog.name = input.clone();
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let Some(dialog) = &self.dialog else {
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
        .style(|_theme| crate::style::app_background())
        .padding(20)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_sets_template_name_when_dialog_is_open() {
        let state = crate::state::State::default();
        let mut view = TemplateSaveView::new(state);
        view.open(crate::state::TemplateSaveDialog {
            name: "Old".to_string(),
        });

        view.update(&Message::TemplateSaveInput("New".to_string()));

        assert_eq!(
            view.dialog.as_ref().map(|dialog| dialog.name.as_str()),
            Some("New")
        );
    }

    #[test]
    fn update_ignores_template_input_when_dialog_is_closed() {
        let state = crate::state::State::default();
        let mut view = TemplateSaveView::new(state);

        view.update(&Message::TemplateSaveInput("New".to_string()));

        assert!(view.dialog.is_none());
    }

    #[test]
    fn new_creates_view() {
        let state = crate::state::State::default();
        let view = TemplateSaveView::new(state);
        let _ = &view;
    }

    #[test]
    fn view_returns_empty_when_no_dialog() {
        let state = crate::state::State::default();
        let view = TemplateSaveView::new(state);
        let element = view.view();
        let _ = &element;
    }
}
