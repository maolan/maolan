use crate::{message::Message, state::State};
use iced::{
    Alignment, Element, Length,
    widget::{button, column, container, row, text, text_input},
};

#[derive(Debug)]
pub struct TrackGroupView {
    state: State,
}

impl TrackGroupView {
    pub fn new(state: State) -> Self {
        Self { state }
    }

    pub fn update(&mut self, message: &Message) {
        if let Message::TrackGroupInput(input) = message
            && let Some(dialog) = &mut self.state.blocking_write().track_group_dialog
        {
            dialog.name = input.clone();
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let state = self.state.blocking_read();
        let Some(dialog) = &state.track_group_dialog else {
            return container("").into();
        };

        let can_confirm = !dialog.name.trim().is_empty();
        let create_button = if can_confirm {
            button("Create Group").on_press(Message::TrackGroupConfirm)
        } else {
            button("Create Group")
        };

        container(
            column![
                text(format!("Group {} tracks", dialog.selected_tracks.len())).size(16),
                row![
                    text("Group name:"),
                    text_input("Enter group name", &dialog.name)
                        .on_input(Message::TrackGroupInput)
                        .width(Length::Fixed(300.0)),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                row![
                    create_button,
                    button("Cancel")
                        .on_press(Message::TrackGroupCancel)
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
    fn update_sets_track_group_name_when_dialog_is_open() {
        let state = Arc::new(RwLock::new(crate::state::StateData::default()));
        state.blocking_write().track_group_dialog = Some(crate::state::TrackGroupDialog {
            selected_tracks: vec!["Kick".to_string(), "Snare".to_string()],
            name: "Rhythm".to_string(),
        });
        let mut view = TrackGroupView::new(state.clone());

        view.update(&Message::TrackGroupInput("Drums".to_string()));

        assert_eq!(
            state
                .blocking_read()
                .track_group_dialog
                .as_ref()
                .map(|dialog| dialog.name.as_str()),
            Some("Drums")
        );
    }

    #[test]
    fn update_ignores_track_group_input_when_dialog_is_closed() {
        let state = Arc::new(RwLock::new(crate::state::StateData::default()));
        let mut view = TrackGroupView::new(state.clone());

        view.update(&Message::TrackGroupInput("Drums".to_string()));

        assert!(state.blocking_read().track_group_dialog.is_none());
    }

    #[test]
    fn new_creates_view() {
        let state = crate::state::State::default();
        let view = TrackGroupView::new(state);
        let _ = &view;
    }

    #[test]
    fn view_returns_empty_when_no_dialog() {
        let state = Arc::new(RwLock::new(crate::state::StateData::default()));
        let view = TrackGroupView::new(state);
        let element = view.view();
        let _ = &element;
    }
}
