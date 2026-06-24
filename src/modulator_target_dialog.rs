use crate::{
    message::Message,
    state::{ModulatorController, State},
};
use iced::{
    Alignment, Element, Length,
    widget::{button, column, container, row, text, text_input},
};

pub struct ModulatorTargetDialogView {
    state: State,
}

impl ModulatorTargetDialogView {
    pub fn new(state: State) -> Self {
        Self { state }
    }

    pub fn update(&mut self, message: &Message) {
        let mut state = self.state.blocking_write();
        let Some(dialog) = &mut state.modulator_target_dialog else {
            return;
        };
        match message {
            Message::ModulatorTargetMinInput(v) => dialog.min_input = v.clone(),
            Message::ModulatorTargetMaxInput(v) => dialog.max_input = v.clone(),
            _ => {}
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let state = self.state.blocking_read();
        let Some(dialog) = &state.modulator_target_dialog else {
            return container("").into();
        };

        let controller_label = match dialog.controller {
            ModulatorController::Volume => "Volume",
            ModulatorController::Balance => "Pan",
        };

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
                        controller: dialog.controller,
                    })
                    .style(button::secondary),
            );
        }

        container(
            column![
                text(format!(
                    "Assign modulator to {} - {}",
                    dialog.track_name, controller_label
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
