use crate::{message::Message, state::State};
use iced::{
    Alignment, Element, Length,
    widget::{button, column, container, row, text, text_input},
};

#[derive(Debug)]
pub struct TrackTemplateSaveView {
    state: State,
}

impl TrackTemplateSaveView {
    pub fn new(state: State) -> Self {
        Self { state }
    }

    pub fn update(&mut self, message: &Message) {
        if let Message::TrackTemplateSaveInput(input) = message
            && let Some(dialog) = &mut self.state.blocking_write().track_template_save_dialog
        {
            dialog.name = input.clone();
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let state = self.state.blocking_read();
        let Some(dialog) = &state.track_template_save_dialog else {
            return container("").into();
        };

        let template_name = &dialog.name;
        let can_confirm = !template_name.trim().is_empty();

        let save_button = if can_confirm {
            button("Save").on_press(Message::TrackTemplateSaveConfirm)
        } else {
            button("Save")
        };

        container(
            column![
                text("Save track as template").size(16),
                row![
                    text("Template name:"),
                    text_input("Enter template name", template_name)
                        .on_input(Message::TrackTemplateSaveInput)
                        .width(Length::Fixed(300.0)),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                row![
                    save_button,
                    button("Cancel")
                        .on_press(Message::TrackTemplateSaveCancel)
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
    use std::sync::Arc;
    use tokio::sync::RwLock;

    #[test]
    fn update_sets_track_template_name_when_dialog_is_open() {
        let state = Arc::new(RwLock::new(crate::state::StateData::default()));
        state.blocking_write().track_template_save_dialog =
            Some(crate::state::TrackTemplateSaveDialog {
                track_name: "Bass".to_string(),
                name: "Old".to_string(),
            });
        let mut view = TrackTemplateSaveView::new(state.clone());

        view.update(&Message::TrackTemplateSaveInput("New".to_string()));

        assert_eq!(
            state
                .blocking_read()
                .track_template_save_dialog
                .as_ref()
                .map(|dialog| dialog.name.as_str()),
            Some("New")
        );
    }

    #[test]
    fn update_ignores_non_matching_messages() {
        let state = Arc::new(RwLock::new(crate::state::StateData::default()));
        state.blocking_write().track_template_save_dialog =
            Some(crate::state::TrackTemplateSaveDialog {
                track_name: "Bass".to_string(),
                name: "Old".to_string(),
            });
        let mut view = TrackTemplateSaveView::new(state.clone());

        view.update(&Message::Cancel);

        assert_eq!(
            state
                .blocking_read()
                .track_template_save_dialog
                .as_ref()
                .map(|dialog| dialog.name.as_str()),
            Some("Old")
        );
    }

    #[test]
    fn new_creates_view() {
        let state = crate::state::State::default();
        let view = TrackTemplateSaveView::new(state);
        let _ = &view;
    }

    #[test]
    fn view_returns_empty_when_no_dialog() {
        let state = Arc::new(RwLock::new(crate::state::StateData::default()));
        let view = TrackTemplateSaveView::new(state);
        let element = view.view();
        let _ = &element;
    }
}
