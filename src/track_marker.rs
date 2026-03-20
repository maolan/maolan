use crate::{message::Message, state::State};
use iced::{
    Alignment, Border, Color, Element, Length,
    widget::{Id, button, column, container, row, text, text_input},
};

#[derive(Debug)]
pub struct TrackMarkerView {
    state: State,
}

impl TrackMarkerView {
    pub fn new(state: State) -> Self {
        Self { state }
    }

    pub fn name_input_id() -> Id {
        Id::new("track-marker-name-input")
    }

    pub fn update(&mut self, message: &Message) {
        if let Message::TrackMarkerNameInput(input) = message
            && let Some(dialog) = &mut self.state.blocking_write().track_marker_dialog
        {
            dialog.name = input.clone();
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let state = self.state.blocking_read();
        let Some(dialog) = &state.track_marker_dialog else {
            return container("").into();
        };

        let can_confirm = !dialog.name.trim().is_empty();
        let is_rename = dialog.marker_index.is_some();
        let confirm_button = if can_confirm {
            button(if is_rename { "Rename" } else { "Create" })
                .on_press(Message::TrackMarkerNameConfirm)
        } else {
            button(if is_rename { "Rename" } else { "Create" })
        };

        container(
            column![
                text(if is_rename {
                    "Edit Marker"
                } else {
                    "Add Marker"
                }),
                text(format!("Track: {}", dialog.track_name)).size(11),
                text_input("Enter marker name", &dialog.name)
                    .id(Self::name_input_id())
                    .on_input(Message::TrackMarkerNameInput)
                    .on_submit(Message::TrackMarkerNameConfirm)
                    .width(Length::Fill),
                row![
                    confirm_button,
                    button("Cancel")
                        .on_press(Message::TrackMarkerNameCancel)
                        .style(button::secondary)
                ]
                .spacing(10),
            ]
            .align_x(Alignment::Start)
            .spacing(10),
        )
        .style(|_theme| container::Style {
            border: Border {
                color: Color::from_rgba(0.34, 0.42, 0.56, 0.72),
                width: 1.0,
                ..Border::default()
            },
            ..crate::style::app_background()
        })
        .padding(12)
        .width(Length::Fixed(320.0))
        .height(Length::Fill)
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    #[test]
    fn update_sets_track_marker_name_when_dialog_is_open() {
        let state = Arc::new(RwLock::new(crate::state::StateData::default()));
        state.blocking_write().track_marker_dialog = Some(crate::state::TrackMarkerDialog {
            track_name: "Lead".to_string(),
            sample: 128,
            marker_index: None,
            name: "Verse".to_string(),
        });
        let mut view = TrackMarkerView::new(state.clone());

        view.update(&Message::TrackMarkerNameInput("Chorus".to_string()));

        assert_eq!(
            state
                .blocking_read()
                .track_marker_dialog
                .as_ref()
                .map(|dialog| dialog.name.as_str()),
            Some("Chorus")
        );
    }

    #[test]
    fn update_ignores_non_matching_messages() {
        let state = Arc::new(RwLock::new(crate::state::StateData::default()));
        state.blocking_write().track_marker_dialog = Some(crate::state::TrackMarkerDialog {
            track_name: "Lead".to_string(),
            sample: 128,
            marker_index: None,
            name: "Verse".to_string(),
        });
        let mut view = TrackMarkerView::new(state.clone());

        view.update(&Message::Cancel);

        assert_eq!(
            state
                .blocking_read()
                .track_marker_dialog
                .as_ref()
                .map(|dialog| dialog.name.as_str()),
            Some("Verse")
        );
    }
}
