use crate::{message::Message, state::State};
use iced::{
    Alignment, Length,
    widget::{button, checkbox, column, container, pick_list, row, text},
};
use maolan_engine::message::Action;

pub struct HW {
    state: State,
}

impl HW {
    pub fn new(state: State) -> Self {
        Self { state }
    }

    pub fn audio_view(&self) -> iced::Element<'_, Message> {
        let period_options = vec![64, 128, 256, 512, 1024, 2048, 4096, 8192];
        let nperiod_options: Vec<usize> = (1..=16).collect();
        let (available_hw, selected_hw, exclusive, period_frames, nperiods, sync_mode) = {
            let state = self.state.blocking_read();
            (
                state.available_hw.clone(),
                state.selected_hw.clone(),
                state.oss_exclusive,
                state.oss_period_frames,
                state.oss_nperiods,
                state.oss_sync_mode,
            )
        };
        let selected_is_jack = selected_hw
            .as_deref()
            .map(|s| s.eq_ignore_ascii_case("jack"))
            .unwrap_or(false);
        let mut submit = button("Open Audio");
        if let Some(ref hw) = selected_hw {
            submit = submit.on_press(Message::Request(Action::OpenAudioDevice {
                device: hw.to_string(),
                exclusive,
                period_frames,
                nperiods,
                sync_mode,
            }));
        }
        let mut content = column![
            pick_list(available_hw, selected_hw, Message::HWSelected)
                .placeholder("Choose audio device")
        ]
        .spacing(10);

        if !selected_is_jack {
            content = content
                .push(
                    row![
                        text("Period frames:"),
                        pick_list(
                            period_options.clone(),
                            Some(period_frames),
                            Message::HWPeriodFramesChanged
                        )
                        .placeholder("Period")
                    ]
                    .spacing(10),
                )
                .push(
                    row![
                        text("N periods:"),
                        pick_list(
                            nperiod_options.clone(),
                            Some(nperiods),
                            Message::HWNPeriodsChanged
                        )
                        .placeholder("N periods")
                    ]
                    .spacing(10),
                )
                .push(
                    checkbox(exclusive)
                        .label("Exclusive mode")
                        .on_toggle(Message::HWExclusiveToggled),
                )
                .push(
                    checkbox(sync_mode)
                        .label("Sync mode")
                        .on_toggle(Message::HWSyncModeToggled),
                );
        }

        content = content.push(submit);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .into()
    }
}
