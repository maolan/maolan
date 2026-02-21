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
        let (
            available_backends,
            selected_backend,
            available_hw,
            mut selected_hw,
            exclusive,
            period_frames,
            nperiods,
            sync_mode,
        ) = {
            let state = self.state.blocking_read();
            (
                state.available_backends.clone(),
                state.selected_backend.clone(),
                state.available_hw.clone(),
                state.selected_hw.clone(),
                state.oss_exclusive,
                state.oss_period_frames,
                state.oss_nperiods,
                state.oss_sync_mode,
            )
        };
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        let available_hw: Vec<crate::state::AudioDeviceOption> = available_hw
            .into_iter()
            .filter(|hw| match selected_backend {
                crate::state::AudioBackendOption::Jack => false,
                #[cfg(target_os = "freebsd")]
                crate::state::AudioBackendOption::Oss => hw.id.starts_with("/dev/dsp"),
                #[cfg(target_os = "linux")]
                crate::state::AudioBackendOption::Alsa => hw.id.starts_with("hw:"),
            })
            .collect();
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        {
            selected_hw = selected_hw.filter(|s| available_hw.iter().any(|hw| hw.id == s.id));
        }
        let selected_is_jack = matches!(selected_backend, crate::state::AudioBackendOption::Jack);
        let mut submit = button("Open Audio");
        if selected_is_jack || selected_hw.is_some() {
            #[cfg(any(target_os = "linux", target_os = "freebsd"))]
            let device = if selected_is_jack {
                "jack".to_string()
            } else {
                selected_hw
                    .as_ref()
                    .map(|hw| hw.id.to_string())
                    .unwrap_or_default()
            };
            #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
            let device = if selected_is_jack {
                "jack".to_string()
            } else {
                selected_hw.clone().unwrap_or_default()
            };
            submit = submit.on_press(Message::Request(Action::OpenAudioDevice {
                device,
                exclusive,
                period_frames,
                nperiods,
                sync_mode,
            }));
        }
        let mut content = column![
            row![
                text("Backend:"),
                pick_list(
                    available_backends,
                    Some(selected_backend),
                    Message::HWBackendSelected
                )
                .placeholder("Choose backend")
            ]
            .spacing(10)
        ]
        .spacing(10);
        if !selected_is_jack {
            content = content.push(
                pick_list(available_hw, selected_hw, Message::HWSelected)
                    .placeholder("Choose audio device"),
            );
        }

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
