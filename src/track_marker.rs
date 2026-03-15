use crate::{message::Message, state::State};
use iced::{
    Alignment, Element, Length,
    widget::{button, column, container, row, text, text_input},
};

#[derive(Debug)]
pub struct TrackMarkerView {
    state: State,
}

impl TrackMarkerView {
    pub fn new(state: State) -> Self {
        Self { state }
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
                text(format!(
                    "{} marker on track: {}",
                    if is_rename { "Rename" } else { "Create" },
                    dialog.track_name
                ))
                .size(16),
                row![
                    text("Marker name:"),
                    text_input("Enter marker name", &dialog.name)
                        .on_input(Message::TrackMarkerNameInput)
                        .on_submit(Message::TrackMarkerNameConfirm)
                        .width(Length::Fixed(300.0)),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                row![
                    confirm_button,
                    button("Cancel")
                        .on_press(Message::TrackMarkerNameCancel)
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
