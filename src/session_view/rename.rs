use crate::{message::Message, state::State};
use iced::{
    Alignment, Element, Length,
    widget::{button, column, container, row, text, text_input},
};

#[derive(Debug)]
pub struct SceneRenameView {
    state: State,
}

impl SceneRenameView {
    pub fn new(state: State) -> Self {
        Self { state }
    }

    pub fn update(&mut self, message: &Message) {
        if let Message::SessionSceneRenameInput(input) = message
            && let Some(dialog) = &mut self.state.blocking_write().scene_rename_dialog
        {
            dialog.name = input.clone();
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let state = self.state.blocking_read();
        let Some(dialog) = &state.scene_rename_dialog else {
            return container("").into();
        };

        let name = dialog.name.trim();
        let can_confirm = !name.is_empty();

        let rename_button = if can_confirm {
            button("Rename").on_press(Message::SessionSceneRenameConfirm)
        } else {
            button("Rename")
        };

        container(
            column![
                text(format!("Rename scene {}", dialog.scene_index + 1)).size(16),
                row![
                    text("New name:"),
                    text_input("Enter new name", &dialog.name)
                        .on_input(Message::SessionSceneRenameInput)
                        .width(Length::Fixed(300.0)),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                row![
                    rename_button,
                    button("Cancel")
                        .on_press(Message::SessionSceneRenameCancel)
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
    fn update_sets_scene_rename_input_when_dialog_is_open() {
        let state = Arc::new(RwLock::new(crate::state::StateData::default()));
        state.blocking_write().scene_rename_dialog = Some(crate::state::SceneRenameDialog {
            scene_index: 0,
            name: "Old".to_string(),
        });
        let mut view = SceneRenameView::new(state.clone());

        view.update(&Message::SessionSceneRenameInput("New".to_string()));

        assert_eq!(
            state
                .blocking_read()
                .scene_rename_dialog
                .as_ref()
                .map(|dialog| dialog.name.as_str()),
            Some("New")
        );
    }

    #[test]
    fn update_ignores_non_matching_messages() {
        let state = Arc::new(RwLock::new(crate::state::StateData::default()));
        state.blocking_write().scene_rename_dialog = Some(crate::state::SceneRenameDialog {
            scene_index: 0,
            name: "Old".to_string(),
        });
        let mut view = SceneRenameView::new(state.clone());

        view.update(&Message::Cancel);

        assert_eq!(
            state
                .blocking_read()
                .scene_rename_dialog
                .as_ref()
                .map(|dialog| dialog.name.as_str()),
            Some("Old")
        );
    }

    #[test]
    fn new_creates_view() {
        let state = crate::state::State::default();
        let view = SceneRenameView::new(state);
        let _ = &view;
    }

    #[test]
    fn view_returns_empty_when_no_dialog() {
        let state = Arc::new(RwLock::new(crate::state::StateData::default()));
        let view = SceneRenameView::new(state);
        let _element = view.view();
    }
}
