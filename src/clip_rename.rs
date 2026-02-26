use crate::message::Message;
use crate::state::State;
use iced::{
    Alignment, Element, Length,
    widget::{button, column, container, row, text, text_input},
};

#[derive(Debug)]
pub struct ClipRenameView {
    state: State,
}

impl ClipRenameView {
    pub fn new(state: State) -> Self {
        Self { state }
    }

    pub fn update(&mut self, message: Message) {
        if let Message::ClipRenameInput(input) = message {
            if let Some(dialog) = &mut self.state.blocking_write().clip_rename_dialog {
                dialog.new_name = input;
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let state = self.state.blocking_read();
        let Some(dialog) = &state.clip_rename_dialog else {
            return container("").into();
        };

        // Get current clip name and clean it for display
        let current_name_raw = state
            .tracks
            .iter()
            .find(|t| t.name == dialog.track_idx)
            .and_then(|t| match dialog.kind {
                maolan_engine::kind::Kind::Audio => {
                    t.audio.clips.get(dialog.clip_idx).map(|c| c.name.as_str())
                }
                maolan_engine::kind::Kind::MIDI => {
                    t.midi.clips.get(dialog.clip_idx).map(|c| c.name.as_str())
                }
            })
            .unwrap_or("");

        // Clean the current name for display
        let current_name = clean_clip_name(current_name_raw);

        let new_name = &dialog.new_name;
        let can_confirm = !new_name.trim().is_empty() && new_name != &current_name;

        fn clean_clip_name(name: &str) -> String {
            let mut cleaned = name.to_string();
            if let Some(stripped) = cleaned.strip_prefix("audio/") {
                cleaned = stripped.to_string();
            }
            if let Some(stripped) = cleaned.strip_suffix(".wav") {
                cleaned = stripped.to_string();
            }
            cleaned
        }

        let rename_button = if can_confirm {
            button("Rename").on_press(Message::ClipRenameConfirm)
        } else {
            button("Rename")
        };

        container(
            column![
                text(format!("Rename clip: {}", current_name)).size(16),
                row![
                    text("New name:"),
                    text_input("Enter new name", new_name)
                        .on_input(Message::ClipRenameInput)
                        .width(Length::Fixed(300.0)),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                row![
                    rename_button,
                    button("Cancel")
                        .on_press(Message::ClipRenameCancel)
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
