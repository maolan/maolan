use crate::{message::Message, state::State};
use maolan_widgets::iced::{
    Alignment, Element, Length,
    widget::{button, column, container, row, text, text_input},
};

#[derive(Debug)]
pub struct SceneRenameView {
    pub dialog: Option<crate::state::SceneRenameDialog>,
}

impl SceneRenameView {
    pub fn new(_state: State) -> Self {
        Self { dialog: None }
    }

    pub fn open(&mut self, dialog: crate::state::SceneRenameDialog) {
        self.dialog = Some(dialog);
    }

    pub fn close(&mut self) {
        self.dialog = None;
    }

    pub fn is_open(&self) -> bool {
        self.dialog.is_some()
    }

    pub fn update(&mut self, message: &Message) {
        if let Message::SessionSceneRenameInput(input) = message
            && let Some(dialog) = &mut self.dialog
        {
            dialog.name = input.clone();
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let Some(dialog) = &self.dialog else {
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

    #[test]
    fn update_sets_scene_rename_input_when_dialog_is_open() {
        let state = crate::state::State::default();
        let mut view = SceneRenameView::new(state);
        view.open(crate::state::SceneRenameDialog {
            scene_index: 0,
            name: "Old".to_string(),
        });

        view.update(&Message::SessionSceneRenameInput("New".to_string()));

        assert_eq!(
            view.dialog.as_ref().map(|dialog| dialog.name.as_str()),
            Some("New")
        );
    }

    #[test]
    fn update_ignores_non_matching_messages() {
        let state = crate::state::State::default();
        let mut view = SceneRenameView::new(state);
        view.open(crate::state::SceneRenameDialog {
            scene_index: 0,
            name: "Old".to_string(),
        });

        view.update(&Message::Cancel);

        assert_eq!(
            view.dialog.as_ref().map(|dialog| dialog.name.as_str()),
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
        let state = crate::state::State::default();
        let view = SceneRenameView::new(state);
        let _element = view.view();
    }
}
