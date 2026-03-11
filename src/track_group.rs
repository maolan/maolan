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
