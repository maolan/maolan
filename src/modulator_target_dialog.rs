use crate::{message::Message, state::State};
use maolan_widgets::iced::{
    Alignment, Element, Length,
    widget::{button, column, container, row, text, text_input},
};

pub struct ModulatorTargetDialogView {
    pub dialog: Option<crate::state::ModulatorTargetDialog>,
}

impl ModulatorTargetDialogView {
    pub fn new(_state: State) -> Self {
        Self { dialog: None }
    }

    pub fn open(&mut self, dialog: crate::state::ModulatorTargetDialog) {
        self.dialog = Some(dialog);
    }

    pub fn close(&mut self) {
        self.dialog = None;
    }

    pub fn is_open(&self) -> bool {
        self.dialog.is_some()
    }

    pub fn update(&mut self, message: &Message) {
        let Some(dialog) = &mut self.dialog else {
            return;
        };
        match message {
            Message::ModulatorTargetMinInput(v) => dialog.min_input = v.clone(),
            Message::ModulatorTargetMaxInput(v) => dialog.max_input = v.clone(),
            _ => {}
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let Some(dialog) = &self.dialog else {
            return container("").into();
        };

        let target_label = dialog.target.to_string();

        let min_ok = dialog.min_input.trim().parse::<f32>().is_ok();
        let max_ok = dialog.max_input.trim().parse::<f32>().is_ok();
        let can_confirm = min_ok && max_ok;

        let confirm_button = if can_confirm {
            button("Assign").on_press(Message::ModulatorTargetConfirm)
        } else {
            button("Assign")
        };

        let mut buttons = row![
            confirm_button,
            button("Cancel")
                .on_press(Message::ModulatorTargetCancel)
                .style(button::secondary)
        ]
        .spacing(10);
        if dialog.existing {
            buttons = buttons.push(
                button("Remove")
                    .on_press(Message::ModulatorTargetRemove {
                        modulator_id: dialog.modulator_id,
                        track_name: dialog.track_name.clone(),
                        target: dialog.target.clone(),
                    })
                    .style(button::secondary),
            );
        }

        container(
            column![
                text(format!(
                    "Assign modulator to {} - {}",
                    dialog.track_name, target_label
                ))
                .size(16),
                row![
                    text("Min:").size(13),
                    text_input("Enter min", &dialog.min_input)
                        .on_input(Message::ModulatorTargetMinInput)
                        .width(Length::Fixed(120.0)),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                row![
                    text("Max:").size(13),
                    text_input("Enter max", &dialog.max_input)
                        .on_input(Message::ModulatorTargetMaxInput)
                        .width(Length::Fixed(120.0)),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                buttons,
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
