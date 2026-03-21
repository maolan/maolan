use crate::{message::Message, state::State};
use iced::{
    Alignment, Element, Length,
    widget::{button, column, container, row, text, text_input},
};

#[derive(Debug)]
pub struct TrackRenameView {
    state: State,
}

impl TrackRenameView {
    pub fn new(state: State) -> Self {
        Self { state }
    }

    pub fn update(&mut self, message: &Message) {
        if let Message::TrackRenameInput(input) = message
            && let Some(dialog) = &mut self.state.blocking_write().track_rename_dialog
        {
            dialog.new_name = input.clone();
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let state = self.state.blocking_read();
        let Some(dialog) = &state.track_rename_dialog else {
            return container("").into();
        };

        let old_name = &dialog.old_name;
        let new_name = &dialog.new_name;
        let can_confirm = !new_name.trim().is_empty() && new_name != old_name;

        let rename_button = if can_confirm {
            button("Rename").on_press(Message::TrackRenameConfirm)
        } else {
            button("Rename")
        };

        container(
            column![
                text(format!("Rename track: {}", old_name)).size(16),
                row![
                    text("New name:"),
                    text_input("Enter new name", new_name)
                        .on_input(Message::TrackRenameInput)
                        .width(Length::Fixed(300.0)),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                row![
                    rename_button,
                    button("Cancel")
                        .on_press(Message::TrackRenameCancel)
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
    fn update_sets_track_rename_input_when_dialog_is_open() {
        let state = Arc::new(RwLock::new(crate::state::StateData::default()));
        state.blocking_write().track_rename_dialog = Some(crate::state::TrackRenameDialog {
            old_name: "Drums".to_string(),
            new_name: "Old".to_string(),
        });
        let mut view = TrackRenameView::new(state.clone());

        view.update(&Message::TrackRenameInput("New".to_string()));

        assert_eq!(
            state
                .blocking_read()
                .track_rename_dialog
                .as_ref()
                .map(|dialog| dialog.new_name.as_str()),
            Some("New")
        );
    }

    #[test]
    fn update_ignores_non_matching_messages() {
        let state = Arc::new(RwLock::new(crate::state::StateData::default()));
        state.blocking_write().track_rename_dialog = Some(crate::state::TrackRenameDialog {
            old_name: "Drums".to_string(),
            new_name: "Old".to_string(),
        });
        let mut view = TrackRenameView::new(state.clone());

        view.update(&Message::Cancel);

        assert_eq!(
            state
                .blocking_read()
                .track_rename_dialog
                .as_ref()
                .map(|dialog| dialog.new_name.as_str()),
            Some("Old")
        );
    }
}
