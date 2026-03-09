use crate::{
    message::Message,
    platform_caps::{HAS_SEPARATE_AUDIO_INPUT_DEVICE, REQUIRE_SAMPLE_RATES_FOR_HW_READY, SUPPORTS_LV2},
    state::State,
};
use iced::{
    Alignment, Length,
    widget::{button, checkbox, column, container, pick_list, row, text},
};
use maolan_engine::message::Action;

pub struct HW {
    state: State,
}

struct OpenAudioSelection {
    input_device: Option<String>,
    chosen_bits: usize,
    chosen_sample_rate_hz: i32,
    exclusive: bool,
    period_frames: usize,
    nperiods: usize,
    sync_mode: bool,
}

trait DeviceId {
    fn device_id(&self) -> String;
}

impl DeviceId for String {
    fn device_id(&self) -> String {
        self.clone()
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
impl DeviceId for crate::state::AudioDeviceOption {
    fn device_id(&self) -> String {
        self.id.clone()
    }
}

impl HW {
    pub fn new(state: State) -> Self {
        Self { state }
    }

    fn device_pick_row<T>(
        label: &'static str,
        options: Vec<T>,
        selected: Option<T>,
        on_select: fn(T) -> Message,
        placeholder: &'static str,
    ) -> iced::Element<'static, Message>
    where
        T: Clone + PartialEq + std::fmt::Display + 'static,
    {
        row![
            text(label),
            pick_list(options, selected, on_select).placeholder(placeholder)
        ]
        .spacing(10)
        .into()
    }

    fn plugins_loaded(&self) -> bool {
        let state = self.state.blocking_read();
        let core_plugins_loaded = state.vst3_plugins_loaded && state.clap_plugins_loaded;
        if SUPPORTS_LV2 {
            core_plugins_loaded && state.lv2_plugins_loaded
        } else {
            core_plugins_loaded
        }
    }

    #[cfg(target_os = "freebsd")]
    fn append_device_rows(
        content: iced::widget::Column<'static, Message>,
        available_hw: Vec<crate::state::AudioDeviceOption>,
        selected_hw: Option<crate::state::AudioDeviceOption>,
        selected_input_hw: Option<crate::state::AudioDeviceOption>,
    ) -> iced::widget::Column<'static, Message> {
        Self::device_rows(available_hw, selected_hw, selected_input_hw)
            .into_iter()
            .fold(content, |content, row| content.push(row))
    }

    #[cfg(target_os = "linux")]
    fn append_device_rows(
        content: iced::widget::Column<'static, Message>,
        available_hw: Vec<crate::state::AudioDeviceOption>,
        selected_hw: Option<crate::state::AudioDeviceOption>,
        available_input_hw: Vec<crate::state::AudioDeviceOption>,
        selected_input_hw: Option<crate::state::AudioDeviceOption>,
    ) -> iced::widget::Column<'static, Message> {
        Self::device_rows(available_hw, selected_hw, available_input_hw, selected_input_hw)
            .into_iter()
            .fold(content, |content, row| content.push(row))
    }

    #[cfg(target_os = "windows")]
    fn append_device_rows(
        content: iced::widget::Column<'static, Message>,
        available_hw: Vec<String>,
        selected_hw: Option<String>,
        available_input_hw: Vec<String>,
        selected_input_hw: Option<String>,
    ) -> iced::widget::Column<'static, Message> {
        Self::device_rows(available_hw, selected_hw, available_input_hw, selected_input_hw)
            .into_iter()
            .fold(content, |content, row| content.push(row))
    }

    #[cfg(not(any(target_os = "freebsd", target_os = "linux", target_os = "windows")))]
    fn append_device_rows(
        content: iced::widget::Column<'static, Message>,
        available_hw: Vec<String>,
        selected_hw: Option<String>,
    ) -> iced::widget::Column<'static, Message> {
        Self::device_rows(available_hw, selected_hw)
            .into_iter()
            .fold(content, |content, row| content.push(row))
    }

    #[cfg(target_os = "freebsd")]
    fn device_rows(
        available_hw: Vec<crate::state::AudioDeviceOption>,
        selected_hw: Option<crate::state::AudioDeviceOption>,
        selected_input_hw: Option<crate::state::AudioDeviceOption>,
    ) -> Vec<iced::Element<'static, Message>> {
        vec![
            Self::device_pick_row(
                "Output device:",
                available_hw.clone(),
                selected_hw,
                Message::HWSelected,
                "Choose output device",
            ),
            Self::device_pick_row(
                "Input device:",
                available_hw,
                selected_input_hw,
                Message::HWInputSelected,
                "Choose input device",
            ),
        ]
    }

    #[cfg(target_os = "linux")]
    fn device_rows(
        available_hw: Vec<crate::state::AudioDeviceOption>,
        selected_hw: Option<crate::state::AudioDeviceOption>,
        available_input_hw: Vec<crate::state::AudioDeviceOption>,
        selected_input_hw: Option<crate::state::AudioDeviceOption>,
    ) -> Vec<iced::Element<'static, Message>> {
        vec![
            Self::device_pick_row(
                "Output device:",
                available_hw,
                selected_hw,
                Message::HWSelected,
                "Choose output device",
            ),
            Self::device_pick_row(
                "Input device:",
                available_input_hw,
                selected_input_hw,
                Message::HWInputSelected,
                "Choose input device",
            ),
        ]
    }

    #[cfg(target_os = "windows")]
    fn device_rows(
        available_hw: Vec<String>,
        selected_hw: Option<String>,
        available_input_hw: Vec<String>,
        selected_input_hw: Option<String>,
    ) -> Vec<iced::Element<'static, Message>> {
        vec![
            Self::device_pick_row(
                "Output device:",
                available_hw,
                selected_hw,
                Message::HWSelected,
                "Choose output device",
            ),
            Self::device_pick_row(
                "Input device:",
                available_input_hw,
                selected_input_hw,
                Message::HWInputSelected,
                "Choose input device",
            ),
        ]
    }

    #[cfg(not(any(target_os = "freebsd", target_os = "linux", target_os = "windows")))]
    fn device_rows(
        available_hw: Vec<String>,
        selected_hw: Option<String>,
    ) -> Vec<iced::Element<'static, Message>> {
        vec![Self::device_pick_row(
            "Output device:",
            available_hw,
            selected_hw,
            Message::HWSelected,
            "Choose output device",
        )]
    }

    fn selected_device_id<T: DeviceId>(
        selected_is_jack: bool,
        selected_hw: &Option<T>,
    ) -> String {
        if selected_is_jack {
            "jack".to_string()
        } else {
            selected_hw.as_ref().map(DeviceId::device_id).unwrap_or_default()
        }
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    fn selected_bits(selected_is_jack: bool, chosen_bits: usize) -> i32 {
        if selected_is_jack {
            32
        } else {
            chosen_bits as i32
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
    fn selected_bits(_selected_is_jack: bool, _chosen_bits: usize) -> i32 {
        32
    }

    fn selected_input_device_id<T: DeviceId>(
        selected_is_jack: bool,
        selected_input_hw: &Option<T>,
    ) -> Option<String> {
        if selected_is_jack {
            None
        } else {
            selected_input_hw.as_ref().map(DeviceId::device_id)
        }
    }

    fn open_audio_action<T: DeviceId>(
        selected_is_jack: bool,
        selected_hw: &Option<T>,
        selection: OpenAudioSelection,
    ) -> Action {
        let device = Self::selected_device_id(selected_is_jack, selected_hw);
        let bits = Self::selected_bits(selected_is_jack, selection.chosen_bits);
        Action::OpenAudioDevice {
            device,
            input_device: selection.input_device,
            sample_rate_hz: selection.chosen_sample_rate_hz,
            bits,
            exclusive: selection.exclusive,
            period_frames: selection.period_frames,
            nperiods: selection.nperiods,
            sync_mode: selection.sync_mode,
        }
    }

    pub fn audio_view(&self) -> iced::Element<'_, Message> {
        let period_options = vec![
            16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384, 32768, 65536,
        ];
        let nperiod_options: Vec<usize> = (1..=16).collect();
        let fallback_sample_rates = vec![
            8_000, 11_025, 16_000, 22_050, 32_000, 44_100, 48_000, 88_200, 96_000, 176_400,
            192_000, 384_000,
        ];
        #[cfg(any(
            target_os = "linux",
            target_os = "freebsd"
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
            target_os = "freebsd"
        ))]
        let selected_bits = self.state.blocking_read().oss_bits;
        #[cfg(any(
            target_os = "linux",
            target_os = "freebsd"
        ))]
        let available_hw: Vec<crate::state::AudioDeviceOption> = available_hw
            .into_iter()
            .filter(|hw| match selected_backend {
                #[cfg(unix)]
                crate::state::AudioBackendOption::Jack => false,
                #[cfg(target_os = "freebsd")]
                crate::state::AudioBackendOption::Oss => hw.id.starts_with("/dev/dsp"),
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
            target_os = "freebsd"
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
            target_os = "freebsd"
        ))]
        let sample_rate_options = if selected_is_jack {
            fallback_sample_rates.clone()
        } else {
            selected_hw
                .as_ref()
                .map(|hw| hw.supported_sample_rates.clone())
                .unwrap_or_default()
        };
        #[cfg(target_os = "windows")]
        let sample_rate_options = selected_hw
            .as_ref()
            .map(|hw| crate::state::discover_windows_output_sample_rates(hw))
            .unwrap_or_else(|| fallback_sample_rates.clone());
        #[cfg(not(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "windows"
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
            target_os = "freebsd"
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
            target_os = "freebsd"
        ))]
        let chosen_bits = if bit_options.contains(&selected_bits) {
            selected_bits
        } else {
            bit_options.first().copied().unwrap_or(32)
        };
        #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
        let chosen_bits = 32usize;
        let plugins_loaded = self.plugins_loaded();
        let mut submit = button("Open Audio");
        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        let selected_input_present = selected_input_hw.is_some();
        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "freebsd")))]
        let selected_input_present = true;
        let hw_ready = selected_is_jack
            || (selected_hw.is_some()
                && (!HAS_SEPARATE_AUDIO_INPUT_DEVICE || selected_input_present)
                && (!REQUIRE_SAMPLE_RATES_FOR_HW_READY || !sample_rate_options.is_empty()));
        if plugins_loaded && hw_ready {
            #[cfg(any(target_os = "freebsd", target_os = "linux", target_os = "windows"))]
            let input_device = Self::selected_input_device_id(selected_is_jack, &selected_input_hw);
            #[cfg(not(any(target_os = "freebsd", target_os = "linux", target_os = "windows")))]
            let input_device = None;
            let selection = OpenAudioSelection {
                input_device,
                chosen_bits,
                chosen_sample_rate_hz,
                exclusive,
                period_frames,
                nperiods,
                sync_mode,
            };
            submit = submit.on_press(Message::Request(Self::open_audio_action(
                selected_is_jack,
                &selected_hw,
                selection,
            )));
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
                content = Self::append_device_rows(content, available_hw, selected_hw, selected_input_hw);
            }
            #[cfg(target_os = "linux")]
            {
                content = Self::append_device_rows(
                    content,
                    available_hw,
                    selected_hw,
                    available_input_hw,
                    selected_input_hw,
                );
            }
            #[cfg(target_os = "windows")]
            {
                content = Self::append_device_rows(
                    content,
                    available_hw,
                    selected_hw,
                    available_input_hw,
                    selected_input_hw,
                );
            }
            #[cfg(not(any(target_os = "freebsd", target_os = "linux", target_os = "windows")))]
            {
                content = Self::append_device_rows(content, available_hw, selected_hw);
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
                target_os = "freebsd"
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
