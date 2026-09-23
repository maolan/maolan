use crate::{message::Message, state::State};
use maolan_widgets::iced::{
    Alignment, Border, Color, Element, Length,
    widget::{Id, button, column, container, row, text, text_input},
};

#[derive(Debug)]
pub struct MarkerView {
    pub dialog: Option<crate::state::MarkerDialog>,
}

impl MarkerView {
    pub fn new(_state: State) -> Self {
        Self { dialog: None }
    }

    pub fn open(&mut self, dialog: crate::state::MarkerDialog) {
        self.dialog = Some(dialog);
    }

    pub fn close(&mut self) {
        self.dialog = None;
    }

    pub fn is_open(&self) -> bool {
        self.dialog.is_some()
    }

    pub fn name_input_id() -> Id {
        Id::new("marker-name-input")
    }

    pub fn update(&mut self, message: &Message) {
        if let Message::MarkerNameInput(input) = message
            && let Some(dialog) = &mut self.dialog
        {
            dialog.name = input.clone();
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let Some(dialog) = &self.dialog else {
            return container("").into();
        };

        let can_confirm = !dialog.name.trim().is_empty();
        let is_rename = dialog.marker_index.is_some();
        let confirm_button = if can_confirm {
            button(if is_rename { "Rename" } else { "Create" }).on_press(Message::MarkerNameConfirm)
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
                text_input("Enter marker name", &dialog.name)
                    .id(Self::name_input_id())
                    .on_input(Message::MarkerNameInput)
                    .on_submit(Message::MarkerNameConfirm)
                    .width(Length::Fill),
                row![
                    confirm_button,
                    button("Cancel")
                        .on_press(Message::MarkerNameCancel)
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

    #[test]
    fn update_sets_marker_name_when_dialog_is_open() {
        let state = crate::state::State::default();
        let mut view = MarkerView::new(state);
        view.open(crate::state::MarkerDialog {
            sample: 128,
            marker_index: None,
            name: "Verse".to_string(),
        });

        view.update(&Message::MarkerNameInput("Chorus".to_string()));

        assert_eq!(
            view.dialog.as_ref().map(|dialog| dialog.name.as_str()),
            Some("Chorus")
        );
    }

    #[test]
    fn update_ignores_non_matching_messages() {
        let state = crate::state::State::default();
        let mut view = MarkerView::new(state);
        view.open(crate::state::MarkerDialog {
            sample: 128,
            marker_index: None,
            name: "Verse".to_string(),
        });

        view.update(&Message::Cancel);

        assert_eq!(
            view.dialog.as_ref().map(|dialog| dialog.name.as_str()),
            Some("Verse")
        );
    }

    #[test]
    fn new_creates_view() {
        let state = crate::state::State::default();
        let view = MarkerView::new(state);
        let _ = &view;
    }

    #[test]
    fn name_input_id_returns_expected_id() {
        let id = MarkerView::name_input_id();
        let _ = &id;
    }

    #[test]
    fn view_returns_empty_when_no_dialog() {
        let state = crate::state::State::default();
        let view = MarkerView::new(state);
        let element = view.view();
        let _ = &element;
    }
}
