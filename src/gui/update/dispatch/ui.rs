use super::*;

impl Maolan {
    pub(super) fn handle_simple_ui_message(&mut self, message: &Message) -> bool {
        if self.handle_export_settings_ui_message(message) {
            return true;
        }
        self.handle_hw_settings_ui_message(message)
    }

    fn adjust_export_bit_depth_if_needed(&mut self) {
        let selected = self.selected_export_formats();
        let valid_bit_depths = Self::export_bit_depth_options(&selected);
        if !valid_bit_depths.contains(&self.export_bit_depth)
            && let Some(first) = valid_bit_depths.first().copied()
        {
            self.export_bit_depth = first;
        }
    }

    fn clamp_export_mp3_if_needed(&mut self) {
        if !self.export_mp3_supported_for_current_settings() {
            self.export_format_mp3 = false;
        }
    }

    fn handle_export_settings_ui_message(&mut self, message: &Message) -> bool {
        match message {
            Message::ExportSampleRateSelected(rate) => {
                self.export_sample_rate_hz = *rate;
                true
            }
            Message::ExportFormatWavToggled(enabled) => {
                self.export_format_wav = *enabled;
                self.adjust_export_bit_depth_if_needed();
                true
            }
            Message::ExportFormatMp3Toggled(enabled) => {
                self.export_format_mp3 = *enabled && self.export_mp3_supported_for_current_settings();
                self.adjust_export_bit_depth_if_needed();
                true
            }
            Message::ExportFormatOggToggled(enabled) => {
                self.export_format_ogg = *enabled;
                self.adjust_export_bit_depth_if_needed();
                true
            }
            Message::ExportFormatFlacToggled(enabled) => {
                self.export_format_flac = *enabled;
                self.adjust_export_bit_depth_if_needed();
                true
            }
            Message::ExportMp3ModeSelected(mode) => {
                self.export_mp3_mode = *mode;
                true
            }
            Message::ExportMp3BitrateSelected(kbps) => {
                self.export_mp3_bitrate_kbps = *kbps;
                true
            }
            Message::ExportOggQualityInput(input) => {
                self.export_ogg_quality_input = input
                    .chars()
                    .filter(|c| c.is_ascii_digit() || *c == '-' || *c == '.')
                    .collect();
                true
            }
            Message::ExportBitDepthSelected(bit_depth) => {
                self.export_bit_depth = *bit_depth;
                true
            }
            Message::ExportRenderModeSelected(mode) => {
                self.export_render_mode = *mode;
                if !matches!(mode, ExportRenderMode::Mixdown) {
                    self.export_normalize = false;
                }
                self.clamp_export_mp3_if_needed();
                self.adjust_export_bit_depth_if_needed();
                true
            }
            Message::ExportHwOutPortToggled(port, enabled) => {
                if *enabled {
                    self.export_hw_out_ports.insert(*port);
                } else {
                    self.export_hw_out_ports.remove(port);
                }
                self.clamp_export_mp3_if_needed();
                self.adjust_export_bit_depth_if_needed();
                true
            }
            Message::ExportRealtimeFallbackToggled(enabled) => {
                self.export_realtime_fallback = *enabled;
                true
            }
            Message::ExportNormalizeToggled(enabled) => {
                self.export_normalize = *enabled;
                true
            }
            Message::ExportNormalizeModeSelected(mode) => {
                self.export_normalize_mode = *mode;
                true
            }
            Message::ExportNormalizeDbfsInput(input) => {
                self.export_normalize_dbfs_input = input
                    .chars()
                    .filter(|c| c.is_ascii_digit() || *c == '-' || *c == '.')
                    .collect();
                true
            }
            Message::ExportNormalizeLufsInput(input) => {
                self.export_normalize_lufs_input = input
                    .chars()
                    .filter(|c| c.is_ascii_digit() || *c == '-' || *c == '.')
                    .collect();
                true
            }
            Message::ExportNormalizeDbtpInput(input) => {
                self.export_normalize_dbtp_input = input
                    .chars()
                    .filter(|c| c.is_ascii_digit() || *c == '-' || *c == '.')
                    .collect();
                true
            }
            Message::ExportNormalizeLimiterToggled(enabled) => {
                self.export_normalize_tp_limiter = *enabled;
                true
            }
            Message::ExportMasterLimiterToggled(enabled) => {
                self.export_master_limiter = *enabled;
                true
            }
            Message::ExportMasterLimiterCeilingInput(input) => {
                self.export_master_limiter_ceiling_input = input
                    .chars()
                    .filter(|c| c.is_ascii_digit() || *c == '-' || *c == '.')
                    .collect();
                true
            }
            _ => false,
        }
    }

    fn handle_hw_settings_ui_message(&mut self, message: &Message) -> bool {
        match message {
            Message::HWSelected(hw) => {
                self.apply_hw_selected(hw);
                true
            }
            #[cfg(any(target_os = "windows", target_os = "freebsd", target_os = "linux"))]
            Message::HWInputSelected(hw) => {
                self.apply_hw_input_selected(hw);
                true
            }
            Message::HWBackendSelected(backend) => {
                self.apply_hw_backend_selected(backend);
                true
            }
            Message::HWExclusiveToggled(exclusive) => {
                self.state.blocking_write().oss_exclusive = *exclusive;
                true
            }
            #[cfg(any(unix, target_os = "windows"))]
            Message::HWBitsChanged(bits) => {
                self.state.blocking_write().oss_bits = *bits;
                true
            }
            Message::HWSampleRateChanged(rate_hz) => {
                self.state.blocking_write().hw_sample_rate_hz = (*rate_hz).max(1);
                true
            }
            Message::HWPeriodFramesChanged(period_frames) => {
                self.state.blocking_write().oss_period_frames =
                    Self::normalize_period_frames(*period_frames);
                true
            }
            Message::HWNPeriodsChanged(nperiods) => {
                self.state.blocking_write().oss_nperiods = (*nperiods).max(1);
                true
            }
            Message::HWSyncModeToggled(sync_mode) => {
                self.state.blocking_write().oss_sync_mode = *sync_mode;
                true
            }
            _ => false,
        }
    }
}
