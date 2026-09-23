use super::*;

impl Maolan {
    pub(super) fn handle_simple_ui_message(&mut self, message: &Message) -> bool {
        if self.handle_export_settings_ui_message(message) {
            return true;
        }
        self.handle_hw_settings_ui_message(message)
    }

    fn handle_export_settings_ui_message(&mut self, message: &Message) -> bool {
        match message {
            Message::ExportSampleRateSelected(rate) => {
                self.transfer.export_sample_rate_hz = *rate;
                true
            }
            Message::ExportFormatWavToggled(enabled) => {
                self.transfer.export_format_wav = *enabled;
                self.transfer.adjust_bit_depth_if_needed();
                true
            }
            Message::ExportFormatFlacToggled(enabled) => {
                self.transfer.export_format_flac = *enabled;
                self.transfer.adjust_bit_depth_if_needed();
                true
            }
            Message::ExportFormatMp3Toggled(enabled) => {
                self.transfer.export_format_mp3 = *enabled;
                self.transfer.adjust_bit_depth_if_needed();
                true
            }
            Message::ExportFormatOggToggled(enabled) => {
                self.transfer.export_format_ogg = *enabled;
                self.transfer.adjust_bit_depth_if_needed();
                true
            }
            Message::ExportBitDepthSelected(bit_depth) => {
                self.transfer.export_bit_depth = *bit_depth;
                true
            }
            Message::ExportDitherSelected(dither) => {
                self.transfer.export_dither = *dither;
                true
            }
            Message::ExportRenderModeSelected(mode) => {
                self.transfer.export_render_mode = *mode;
                if !matches!(mode, ExportRenderMode::Mixdown) {
                    self.transfer.export_normalize = false;
                }
                self.transfer.adjust_bit_depth_if_needed();
                true
            }
            Message::ExportHwOutPortToggled(port, enabled) => {
                if *enabled {
                    self.transfer.export_hw_out_ports.insert(*port);
                } else {
                    self.transfer.export_hw_out_ports.remove(port);
                }
                self.transfer.adjust_bit_depth_if_needed();
                true
            }
            Message::ExportRealtimeFallbackToggled(enabled) => {
                self.transfer.export_realtime_fallback = *enabled;
                true
            }
            Message::ExportNormalizeToggled(enabled) => {
                self.transfer.export_normalize = *enabled;
                true
            }
            Message::ExportNormalizeModeSelected(mode) => {
                self.transfer.export_normalize_mode = *mode;
                true
            }
            Message::ExportNormalizeDbfsInput(input) => {
                self.transfer.export_normalize_dbfs_input = input
                    .chars()
                    .filter(|c| c.is_ascii_digit() || *c == '-' || *c == '.')
                    .collect();
                true
            }
            Message::ExportNormalizeLufsInput(input) => {
                self.transfer.export_normalize_lufs_input = input
                    .chars()
                    .filter(|c| c.is_ascii_digit() || *c == '-' || *c == '.')
                    .collect();
                true
            }
            Message::ExportNormalizeDbtpInput(input) => {
                self.transfer.export_normalize_dbtp_input = input
                    .chars()
                    .filter(|c| c.is_ascii_digit() || *c == '-' || *c == '.')
                    .collect();
                true
            }
            Message::ExportNormalizeLimiterToggled(enabled) => {
                self.transfer.export_normalize_tp_limiter = *enabled;
                true
            }
            Message::ExportMasterLimiterToggled(enabled) => {
                self.transfer.export_master_limiter = *enabled;
                true
            }
            Message::ExportMasterLimiterCeilingInput(input) => {
                self.transfer.export_master_limiter_ceiling_input = input
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
            #[cfg(any(
                target_os = "freebsd",
                target_os = "linux",
                target_os = "openbsd",
                target_os = "windows",
                target_os = "macos"
            ))]
            Message::HWInputSelected(hw) => {
                self.apply_hw_input_selected(hw);
                true
            }
            Message::HWBackendSelected(backend) => {
                self.apply_hw_backend_selected(backend);
                true
            }
            Message::HWExclusiveToggled(exclusive) => {
                self.state
                    .write()
                    .expect("state lock poisoned")
                    .oss_exclusive = *exclusive;
                true
            }
            #[cfg(any(unix, target_os = "windows"))]
            Message::HWBitsChanged(bits) => {
                self.state.write().expect("state lock poisoned").oss_bits = *bits;
                true
            }
            Message::HWSampleRateChanged(rate_hz) => {
                self.state
                    .write()
                    .expect("state lock poisoned")
                    .hw_sample_rate_hz = (*rate_hz).max(1);
                true
            }
            Message::HWPeriodFramesChanged(period_frames) => {
                self.state
                    .write()
                    .expect("state lock poisoned")
                    .oss_period_frames = Self::normalize_period_frames(*period_frames);
                true
            }
            Message::HWNPeriodsChanged(nperiods) => {
                self.state
                    .write()
                    .expect("state lock poisoned")
                    .oss_nperiods = (*nperiods).max(1);
                true
            }
            Message::HWSyncModeToggled(sync_mode) => {
                self.state
                    .write()
                    .expect("state lock poisoned")
                    .oss_sync_mode = *sync_mode;
                true
            }
            _ => false,
        }
    }
}

impl Maolan {
    pub(super) fn handle_ui_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ZoomSliderChanged(value) => {
                self.ui.zoom_visible_bars = crate::gui::zoom_slider_to_visible_bars(value);
                let max_scroll = self.editor_max_scroll_samples();
                self.ui.editor_scroll_origin_samples =
                    self.ui.editor_scroll_origin_samples.clamp(0.0, max_scroll);
                self.ui.editor_scroll_x = self.editor_scroll_relative_x();
                return self.sync_editor_scrollbars();
            }
            Message::TimelineZoomByScroll(delta) if delta.abs() > f32::EPSILON => {
                let factor = 1.12_f32.powf(delta.abs());
                self.ui.zoom_visible_bars = if delta > 0.0 {
                    self.ui.zoom_visible_bars / factor
                } else {
                    self.ui.zoom_visible_bars * factor
                }
                .max(crate::gui::MIN_ZOOM_VISIBLE_BARS);
                let max_scroll = self.editor_max_scroll_samples();
                self.ui.editor_scroll_origin_samples =
                    self.ui.editor_scroll_origin_samples.clamp(0.0, max_scroll);
                self.ui.editor_scroll_x = self.editor_scroll_relative_x();
                return self.sync_editor_scrollbars();
            }
            Message::EditorScrollXChanged(value) => {
                let x = value.clamp(0.0, 1.0);
                if (self.ui.editor_scroll_x - x).abs() > 0.0005 {
                    self.ui.editor_scroll_x = x;
                    self.ui.editor_scroll_origin_samples =
                        self.editor_max_scroll_samples() * x as f64;
                    return self.sync_editor_scrollbars();
                }
            }
            Message::EditorScrollYChanged(value) => {
                let y = value.clamp(0.0, 1.0);
                if (self.ui.editor_scroll_y - y).abs() > 0.0005 {
                    if matches!(
                        self.state.read().expect("state lock poisoned").resizing,
                        Some(crate::state::Resizing::Track(..))
                    ) {
                        return Task::none();
                    }
                    self.ui.editor_scroll_y = y;
                    return self.sync_editor_scrollbars();
                }
            }
            Message::MixerScrollXChanged(value) => {
                let x = value.clamp(0.0, 1.0);
                if (self.ui.mixer_scroll_x - x).abs() > 0.0005 {
                    self.ui.mixer_scroll_x = x;
                }
            }
            Message::TracksResizeHover(hovered) => {
                self.ui.tracks_resize_hovered = hovered;
            }
            Message::TracksFilterInput(ref value) => {
                self.ui.tracks_filter = value.clone();
            }
            Message::MixerResizeHover(hovered) => {
                self.ui.mixer_resize_hovered = hovered;
            }
            Message::ShortcutsHint(ref hint) => {
                let mut state = self.state.write().expect("state lock poisoned");
                state.shortcuts_hint = hint.clone();
            }
            Message::ShortcutEditStart(action) => {
                self.ui.shortcut_capture_action = Some(action);
                self.state.write().expect("state lock poisoned").message =
                    "Press the new shortcut key combination".to_string();
            }
            Message::ShortcutCaptured(ref binding) => {
                let Some(action) = self.ui.shortcut_capture_action.take() else {
                    return Task::none();
                };

                if *binding == crate::keyboard_shortcuts::default_binding(action) {
                    self.shortcut_overrides.remove(&action);
                } else {
                    self.shortcut_overrides.insert(action, binding.clone());
                }

                let mut cfg = crate::config::Config::load().unwrap_or_default();
                cfg.shortcut_overrides = self.shortcut_overrides.clone();
                match cfg.save() {
                    Ok(()) => {
                        self.state.write().expect("state lock poisoned").message =
                            format!("Shortcut updated: {}", binding.label());
                    }
                    Err(err) => {
                        self.state.write().expect("state lock poisoned").message =
                            format!("Failed to save shortcut: {err}");
                    }
                }
            }
            Message::ClipResizeHandleHover {
                kind,
                ref track_idx,
                clip_idx,
                is_right_side,
                hovered: true,
            } => {
                self.state
                    .write()
                    .expect("state lock poisoned")
                    .hovered_clip_resize_handle =
                    Some((track_idx.clone(), clip_idx, kind, is_right_side));
            }
            Message::ClipResizeHandleHover { hovered: false, .. } => {}
            Message::MixerLevelEditStart(ref track_name) => {
                let level = {
                    let state = self.state.read().expect("state lock poisoned");
                    if track_name == "hw:out" {
                        state.hw_out_level
                    } else {
                        state
                            .tracks
                            .iter()
                            .find(|t| t.name == *track_name)
                            .map(|t| t.level)
                            .unwrap_or(0.0)
                    }
                };
                self.mixer_level_edit_track = Some(track_name.clone());
                self.mixer_level_edit_input = if level <= -90.0 {
                    "-inf".to_string()
                } else {
                    format!("{level:+.1}")
                };
            }
            Message::MixerLevelEditInput(ref value) => {
                self.mixer_level_edit_input = value.clone();
            }
            Message::MixerLevelEditCommit => {
                let Some(track_name) = self.mixer_level_edit_track.clone() else {
                    return Task::none();
                };
                let mut value = self.mixer_level_edit_input.trim().to_ascii_lowercase();
                if value.ends_with("db") {
                    value.truncate(value.len().saturating_sub(2));
                }
                let value = value.trim();
                let parsed = if value == "-inf" || value == "-infinity" {
                    Some(-90.0)
                } else {
                    value.parse::<f32>().ok()
                };
                if let Some(level_db) = parsed {
                    self.mixer_level_edit_track = None;
                    self.mixer_level_edit_input.clear();
                    return self.send(Action::TrackLevel(track_name, level_db.clamp(-90.0, 20.0)));
                }
                self.state.write().expect("state lock poisoned").message =
                    "Invalid mixer level value".to_string();
            }
            Message::Workspace => {
                // Switching views never stops playback; only transport
                // controls do that.
                let mut state = self.state.write().expect("state lock poisoned");
                state.view = View::Workspace;
                state.pitch_correction = None;
                state.pitch_correction_selected_points.clear();
                state.pitch_correction_dragging_points = None;
                state.pitch_correction_selecting_rect = None;
                drop(state);
                let view_task = self.queue_midi_clip_preview_loads();
                return view_task;
            }
            Message::ToggleMixerVisibility => {
                self.ui.mixer_visible = !self.ui.mixer_visible;
                if !self.ui.mixer_visible {
                    self.ui.mixer_resize_hovered = false;
                }
            }
            Message::ToggleTracksVisibility => {
                self.ui.tracks_visible = !self.ui.tracks_visible;
                if !self.ui.tracks_visible {
                    self.ui.tracks_resize_hovered = false;
                }
            }
            Message::ToggleEditorVisibility => {
                self.ui.editor_visible = !self.ui.editor_visible;
            }
            Message::ToggleToolbarVisibility => {
                self.ui.toolbar_visible = !self.ui.toolbar_visible;
            }
            Message::X32 => {
                let mut state = self.state.write().expect("state lock poisoned");
                state.view = crate::state::View::X32;
            }
            Message::Session => {
                let mut state = self.state.write().expect("state lock poisoned");
                state.view = crate::state::View::Session;
                let track_names: Vec<String> =
                    state.tracks.iter().map(|t| t.name.clone()).collect();
                for track_name in track_names {
                    state.session.ensure_track_slots(&track_name);
                }
            }
            Message::ToggleLogVisibility => {
                self.ui.show_log_window = !self.ui.show_log_window;
            }
            Message::ToggleShortcutsPane => {
                self.ui.shortcuts_pane_visible = !self.ui.shortcuts_pane_visible;
            }
            Message::ToggleClipsPane => {
                self.ui.clips_pane_visible = !self.ui.clips_pane_visible;
            }
            Message::ToggleModulatorsPane => {
                self.ui.modulators_pane_visible = !self.ui.modulators_pane_visible;
                if !self.ui.modulators_pane_visible {
                    self.selected_modulator_id = None;
                    self.state
                        .write()
                        .expect("state lock poisoned")
                        .selected_modulator_id = None;
                }
            }
            Message::ToggleCutIndicator => {
                let cursor = self.active_workspace_cursor();
                let mut state = self.state.write().expect("state lock poisoned");
                state.cut_preview_active = !state.cut_preview_active;
                let now_active = state.cut_preview_active;
                drop(state);

                if now_active
                    && matches!(
                        self.state.read().expect("state lock poisoned").view,
                        View::Workspace
                    )
                {
                    self.update_cut_indicator(cursor);
                } else {
                    let mut state = self.state.write().expect("state lock poisoned");
                    state.cut_indicator = None;
                }
            }
            Message::LogViewAction(ref action) if !action.is_edit() => {
                self.ui.log_viewer_content.perform(action.clone());
            }
            _ => {}
        }
        self.update_children(&message);
        Task::none()
    }
}
