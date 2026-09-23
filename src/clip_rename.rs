use crate::{message::Message, state::State};
use maolan_widgets::iced::{
    Alignment, Element, Length,
    widget::{button, column, container, row, text, text_input},
};

pub(crate) fn clean_clip_name(name: &str) -> String {
    let mut cleaned = name.to_string();
    if let Some(stripped) = cleaned.strip_prefix("audio/") {
        cleaned = stripped.to_string();
    }
    if let Some(stripped) = cleaned.strip_prefix("midi/") {
        cleaned = stripped.to_string();
    }
    if let Some(stripped) = cleaned.strip_suffix(".wav") {
        cleaned = stripped.to_string();
    }
    if let Some(stripped) = cleaned.strip_suffix(".mid") {
        cleaned = stripped.to_string();
    }
    cleaned
}

#[derive(Debug)]
pub struct ClipRenameView {
    state: State,
    pub dialog: Option<crate::state::ClipRenameDialog>,
}

impl ClipRenameView {
    pub fn new(state: State) -> Self {
        Self {
            state,
            dialog: None,
        }
    }

    pub fn open(&mut self, dialog: crate::state::ClipRenameDialog) {
        self.dialog = Some(dialog);
    }

    pub fn close(&mut self) {
        self.dialog = None;
    }

    pub fn is_open(&self) -> bool {
        self.dialog.is_some()
    }

    pub fn update(&mut self, message: &Message) {
        if let Message::ClipRenameInput(input) = message
            && let Some(dialog) = &mut self.dialog
        {
            dialog.new_name = input.clone();
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let state = self.state.read().expect("state lock poisoned");
        let Some(dialog) = &self.dialog else {
            return container("").into();
        };

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

        let current_name = clean_clip_name(current_name_raw);

        let new_name = &dialog.new_name;
        let can_confirm = !new_name.trim().is_empty() && new_name != &current_name;

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
    use maolan_engine::kind::Kind;

    #[test]
    fn update_sets_clip_rename_input_when_dialog_is_open() {
        let state = crate::state::State::default();
        let mut view = ClipRenameView::new(state);
        view.open(crate::state::ClipRenameDialog {
            track_idx: "Track".to_string(),
            clip_idx: 0,
            kind: Kind::Audio,
            new_name: "Old".to_string(),
        });

        view.update(&Message::ClipRenameInput("New".to_string()));

        assert_eq!(
            view.dialog.as_ref().map(|dialog| dialog.new_name.as_str()),
            Some("New")
        );
    }

    #[test]
    fn update_ignores_clip_rename_input_when_dialog_is_closed() {
        let state = crate::state::State::default();
        let mut view = ClipRenameView::new(state);

        view.update(&Message::ClipRenameInput("New".to_string()));

        assert!(view.dialog.is_none());
    }

    #[test]
    fn new_creates_view() {
        let state = crate::state::State::default();
        let view = ClipRenameView::new(state);
        let _ = &view;
    }

    #[test]
    fn view_returns_empty_when_no_dialog() {
        let state = crate::state::State::default();
        let view = ClipRenameView::new(state);
        let element = view.view();
        let _ = &element;
    }

    #[test]
    fn clean_clip_name_strips_audio_prefix_and_wav_suffix() {
        assert_eq!(clean_clip_name("audio/my_clip.wav"), "my_clip");
    }

    #[test]
    fn clean_clip_name_strips_midi_prefix() {
        assert_eq!(clean_clip_name("midi/my_clip"), "my_clip");
    }

    #[test]
    fn clean_clip_name_strips_midi_prefix_and_mid_suffix() {
        assert_eq!(clean_clip_name("midi/my_clip.mid"), "my_clip");
    }

    #[test]
    fn clean_clip_name_returns_plain_name() {
        assert_eq!(clean_clip_name("my_clip"), "my_clip");
    }
}
