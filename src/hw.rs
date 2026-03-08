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
        let fallback_sample_rates = vec![
            8_000, 11_025, 16_000, 22_050, 32_000, 44_100, 48_000, 88_200, 96_000, 176_400,
            192_000, 384_000,
        ];
        #[cfg(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd"
        ))]
        let fallback_bits = vec![32, 24, 16, 8];
        let (
            available_backends,
            selected_backend,
            available_hw,
            mut selected_hw,
            sample_rate_hz,
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
                state.hw_sample_rate_hz,
                state.oss_exclusive,
                state.oss_period_frames,
                state.oss_nperiods,
                state.oss_sync_mode,
            )
        };
        #[cfg(target_os = "windows")]
        let (available_input_hw, mut selected_input_hw) = {
            let state = self.state.blocking_read();
            (
                state.available_input_hw.clone(),
                state.selected_input_hw.clone(),
            )
        };
        #[cfg(target_os = "linux")]
        let (available_input_hw, mut selected_input_hw) = {
            let state = self.state.blocking_read();
            (
                state.available_input_hw.clone(),
                state.selected_input_hw.clone(),
            )
        };
        #[cfg(target_os = "freebsd")]
        let mut selected_input_hw = self.state.blocking_read().selected_input_hw.clone();
        #[cfg(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd"
        ))]
        let selected_bits = self.state.blocking_read().oss_bits;
        #[cfg(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd"
        ))]
        let available_hw: Vec<crate::state::AudioDeviceOption> = available_hw
            .into_iter()
            .filter(|hw| match selected_backend {
                #[cfg(unix)]
                crate::state::AudioBackendOption::Jack => false,
                #[cfg(target_os = "freebsd")]
                crate::state::AudioBackendOption::Oss => hw.id.starts_with("/dev/dsp"),
                #[cfg(target_os = "netbsd")]
                crate::state::AudioBackendOption::Audio4 => hw.id.starts_with("/dev/audio"),
                #[cfg(target_os = "openbsd")]
                crate::state::AudioBackendOption::Sndio => !hw.id.is_empty(),
                #[cfg(target_os = "linux")]
                crate::state::AudioBackendOption::Alsa => hw.id.starts_with("hw:"),
            })
            .collect();
        #[cfg(target_os = "linux")]
        let available_input_hw: Vec<crate::state::AudioDeviceOption> = available_input_hw
            .into_iter()
            .filter(|hw| match selected_backend {
                crate::state::AudioBackendOption::Alsa => hw.id.starts_with("hw:"),
                #[cfg(unix)]
                crate::state::AudioBackendOption::Jack => false,
            })
            .collect();
        #[cfg(target_os = "windows")]
        let available_hw: Vec<String> = available_hw
            .into_iter()
            .filter(|hw| match selected_backend {
                crate::state::AudioBackendOption::Wasapi => hw.starts_with("wasapi:"),
                crate::state::AudioBackendOption::Asio => hw.starts_with("asio:"),
            })
            .collect();
        #[cfg(target_os = "windows")]
        let available_input_hw: Vec<String> = available_input_hw
            .into_iter()
            .filter(|hw| match selected_backend {
                crate::state::AudioBackendOption::Wasapi => hw.starts_with("wasapi:"),
                crate::state::AudioBackendOption::Asio => hw.starts_with("asio:"),
            })
            .collect();
        #[cfg(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd"
        ))]
        {
            selected_hw = selected_hw.filter(|s| available_hw.iter().any(|hw| hw.id == s.id));
        }
        #[cfg(target_os = "freebsd")]
        {
            selected_input_hw =
                selected_input_hw.filter(|s| available_hw.iter().any(|hw| hw.id == s.id));
        }
        #[cfg(target_os = "linux")]
        {
            selected_input_hw =
                selected_input_hw.filter(|s| available_input_hw.iter().any(|hw| hw.id == s.id));
        }
        #[cfg(target_os = "windows")]
        {
            selected_hw = selected_hw.filter(|s| available_hw.iter().any(|hw| hw == s));
            selected_input_hw =
                selected_input_hw.filter(|s| available_input_hw.iter().any(|hw| hw == s));
        }
        #[cfg(unix)]
        let selected_is_jack = matches!(selected_backend, crate::state::AudioBackendOption::Jack);
        #[cfg(not(unix))]
        let selected_is_jack = false;
        #[cfg(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd"
        ))]
        let sample_rate_options = if selected_is_jack {
            fallback_sample_rates.clone()
        } else {
            selected_hw
                .as_ref()
                .map(|hw| hw.supported_sample_rates.clone())
                .unwrap_or_default()
        };
        #[cfg(not(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd"
        )))]
        let sample_rate_options = fallback_sample_rates.clone();
        let chosen_sample_rate_hz = if sample_rate_options.contains(&sample_rate_hz) {
            sample_rate_hz
        } else {
            sample_rate_options
                .iter()
                .min_by_key(|candidate| ((*candidate).saturating_sub(sample_rate_hz)).abs())
                .copied()
                .unwrap_or(48_000)
        };
        let selected_sample_rate_hz = if sample_rate_options.is_empty() {
            None
        } else {
            Some(chosen_sample_rate_hz)
        };
        #[cfg(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd"
        ))]
        let bit_options = if selected_is_jack {
            fallback_bits.clone()
        } else {
            selected_hw
                .as_ref()
                .map(|hw| {
                    if hw.supported_bits.is_empty() {
                        fallback_bits.clone()
                    } else {
                        hw.supported_bits.clone()
                    }
                })
                .unwrap_or_else(|| fallback_bits.clone())
        };
        #[cfg(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd"
        ))]
        let chosen_bits = if bit_options.contains(&selected_bits) {
            selected_bits
        } else {
            bit_options.first().copied().unwrap_or(32)
        };
        #[cfg(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd"
        ))]
        let plugins_loaded = {
            let state = self.state.blocking_read();
            state.lv2_plugins_loaded && state.vst3_plugins_loaded && state.clap_plugins_loaded
        };
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        let plugins_loaded = {
            let state = self.state.blocking_read();
            state.vst3_plugins_loaded && state.clap_plugins_loaded
        };
        #[cfg(not(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "windows",
            target_os = "macos"
        )))]
        let plugins_loaded = true;
        let mut submit = button("Open Audio");
        #[cfg(target_os = "windows")]
        let hw_ready = selected_is_jack || (selected_hw.is_some() && selected_input_hw.is_some());
        #[cfg(target_os = "freebsd")]
        let hw_ready = selected_is_jack
            || (selected_hw.is_some()
                && selected_input_hw.is_some()
                && !sample_rate_options.is_empty());
        #[cfg(target_os = "linux")]
        let hw_ready = selected_is_jack
            || (selected_hw.is_some()
                && selected_input_hw.is_some()
                && !sample_rate_options.is_empty());
        #[cfg(not(target_os = "windows"))]
        #[cfg(not(target_os = "freebsd"))]
        #[cfg(not(target_os = "linux"))]
        let hw_ready =
            selected_is_jack || (selected_hw.is_some() && !sample_rate_options.is_empty());
        if plugins_loaded && hw_ready {
            #[cfg(any(
                target_os = "linux",
                target_os = "freebsd",
                target_os = "netbsd",
                target_os = "openbsd"
            ))]
            let device = if selected_is_jack {
                "jack".to_string()
            } else {
                selected_hw
                    .as_ref()
                    .map(|hw| hw.id.to_string())
                    .unwrap_or_default()
            };
            #[cfg(not(any(
                target_os = "linux",
                target_os = "freebsd",
                target_os = "netbsd",
                target_os = "openbsd"
            )))]
            let device = if selected_is_jack {
                "jack".to_string()
            } else {
                selected_hw.clone().unwrap_or_default()
            };
            #[cfg(any(
                target_os = "linux",
                target_os = "freebsd",
                target_os = "netbsd",
                target_os = "openbsd"
            ))]
            let bits = if selected_is_jack {
                32
            } else {
                chosen_bits as i32
            };
            #[cfg(not(any(
                target_os = "linux",
                target_os = "freebsd",
                target_os = "netbsd",
                target_os = "openbsd"
            )))]
            let bits = 32;
            #[cfg(any(target_os = "freebsd", target_os = "linux"))]
            let input_device = if selected_is_jack {
                None
            } else {
                selected_input_hw.as_ref().map(|hw| hw.id.clone())
            };
            submit = submit.on_press(Message::Request(Action::OpenAudioDevice {
                device,
                #[cfg(target_os = "windows")]
                input_device: selected_input_hw.clone(),
                #[cfg(any(target_os = "freebsd", target_os = "linux"))]
                input_device,
                sample_rate_hz: chosen_sample_rate_hz,
                bits,
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
            #[cfg(target_os = "freebsd")]
            {
                content = content.push(
                    row![
                        text("Output device:"),
                        pick_list(available_hw.clone(), selected_hw, Message::HWSelected)
                            .placeholder("Choose output device")
                    ]
                    .spacing(10),
                );
                content = content.push(
                    row![
                        text("Input device:"),
                        pick_list(available_hw, selected_input_hw, Message::HWInputSelected)
                            .placeholder("Choose input device")
                    ]
                    .spacing(10),
                );
            }
            #[cfg(target_os = "linux")]
            {
                content = content.push(
                    row![
                        text("Output device:"),
                        pick_list(available_hw, selected_hw, Message::HWSelected)
                            .placeholder("Choose output device")
                    ]
                    .spacing(10),
                );
                content = content.push(
                    row![
                        text("Input device:"),
                        pick_list(
                            available_input_hw,
                            selected_input_hw,
                            Message::HWInputSelected
                        )
                        .placeholder("Choose input device")
                    ]
                    .spacing(10),
                );
            }
            #[cfg(not(target_os = "freebsd"))]
            #[cfg(not(target_os = "linux"))]
            {
                content = content.push(
                    row![
                        text("Output device:"),
                        pick_list(available_hw, selected_hw, Message::HWSelected)
                            .placeholder("Choose output device")
                    ]
                    .spacing(10),
                );
            }
            #[cfg(target_os = "windows")]
            {
                content = content.push(
                    row![
                        text("Input device:"),
                        pick_list(
                            available_input_hw,
                            selected_input_hw,
                            Message::HWInputSelected,
                        )
                        .placeholder("Choose input device")
                    ]
                    .spacing(10),
                );
            }
        }

        if !selected_is_jack {
            content = content.push(
                row![
                    text("Sample rate (Hz):"),
                    pick_list(
                        sample_rate_options,
                        selected_sample_rate_hz,
                        Message::HWSampleRateChanged
                    )
                    .placeholder("Choose sample rate")
                ]
                .spacing(10),
            );
            #[cfg(any(
                target_os = "linux",
                target_os = "freebsd",
                target_os = "netbsd",
                target_os = "openbsd"
            ))]
            {
                content = content.push(
                    row![
                        text("Bit depth:"),
                        pick_list(
                            bit_options.clone(),
                            Some(chosen_bits),
                            Message::HWBitsChanged
                        )
                        .placeholder("Bit depth")
                    ]
                    .spacing(10),
                );
            }
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
