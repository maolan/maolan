use super::*;

impl Maolan {
    pub(super) fn handle_dialog_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::EscapePressed => {
                if matches!(
                    self.modal,
                    Some(Show::AddTrack | Show::AddFolder | Show::ApplyTemplate { .. })
                ) {
                    self.modal = None;
                    self.apply_template.close();
                } else if self.track_marker.is_open() {
                    self.track_marker.close();
                } else if self
                    .state
                    .read()
                    .expect("state lock poisoned")
                    .session_slot_context_menu
                    .is_some()
                {
                    self.state
                        .write()
                        .expect("state lock poisoned")
                        .session_slot_context_menu = None;
                }
            }
            Message::Cancel => {
                if self.transfer.export_in_progress {
                    self.transfer
                        .export_cancel
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                    self.state.write().expect("state lock poisoned").message =
                        "Cancelling export...".to_string();
                    return self.send(Action::TrackOfflineBounceCancelAll);
                }
                self.modal = None;
                self.apply_template.close();
            }
            Message::OpenUrl(ref url) => {
                #[cfg(target_os = "windows")]
                let _ = std::process::Command::new("cmd")
                    .args(["/c", "start", "", url])
                    .spawn();
                #[cfg(unix)]
                let _ = std::process::Command::new("xdg-open").arg(url).spawn();
            }
            Message::ConfirmCloseSave => {
                self.session_ops.pending_exit_after_save = true;
                self.modal = None;
                return self.handle_show_message(&Show::Save);
            }
            Message::ConfirmCloseDiscard => {
                return self.request_quit();
            }
            Message::ConfirmCloseCancel => {
                self.session_ops.pending_exit_after_save = false;
                self.modal = None;
                self.state.write().expect("state lock poisoned").message =
                    "Close cancelled".to_string();
                return Task::none();
            }
            Message::PreferencesSampleRateSelected(rate) => {
                self.prefs_export_sample_rate_hz = rate;
            }
            Message::PreferencesSnapModeSelected(mode) => {
                self.prefs_snap_mode = mode;
            }
            Message::PreferencesMidiSnapModeSelected(mode) => {
                self.prefs_midi_snap_mode = mode;
            }
            Message::PreferencesBitDepthSelected(bits) => {
                self.prefs_audio_bit_depth = bits;
            }
            Message::PreferencesOscEnabledToggled(enabled) => {
                self.prefs_osc_enabled = enabled;
            }
            Message::PreferencesOutputDeviceSelected(ref device) => {
                self.prefs_default_output_device_id = (device.id
                    != super::super::super::PREF_DEVICE_AUTO_ID)
                    .then(|| device.id.clone());
            }
            Message::PreferencesInputDeviceSelected(ref device) => {
                self.prefs_default_input_device_id = (device.id
                    != super::super::super::PREF_DEVICE_AUTO_ID)
                    .then(|| device.id.clone());
            }
            Message::PreferencesSave => {
                let mut cfg = crate::config::Config::load().unwrap_or_default();
                cfg.osc_enabled = self.prefs_osc_enabled;
                cfg.default_export_sample_rate_hz = self.prefs_export_sample_rate_hz;
                cfg.default_snap_mode = self.prefs_snap_mode;
                cfg.default_midi_snap_mode = self.prefs_midi_snap_mode;
                cfg.default_audio_bit_depth = self.prefs_audio_bit_depth;
                cfg.default_output_device_id = self.prefs_default_output_device_id.clone();
                cfg.default_input_device_id = self.prefs_default_input_device_id.clone();
                let prefs = super::super::super::AppPreferences {
                    osc_enabled: cfg.osc_enabled,
                    default_export_sample_rate_hz: cfg.default_export_sample_rate_hz,
                    default_snap_mode: cfg.default_snap_mode,
                    default_midi_snap_mode: cfg.default_midi_snap_mode,
                    default_audio_bit_depth: cfg.default_audio_bit_depth,
                    default_output_device_id: cfg.default_output_device_id.clone(),
                    default_input_device_id: cfg.default_input_device_id.clone(),
                    recent_session_paths: cfg.recent_session_paths.clone(),
                    shortcut_overrides: cfg.shortcut_overrides.clone(),
                };
                match cfg.save().map_err(|e| e.to_string()) {
                    Ok(()) => {
                        self.transfer.export_sample_rate_hz = self.prefs_export_sample_rate_hz;
                        self.timing.snap_mode = self.prefs_snap_mode;
                        self.timing.midi_snap_mode = self.prefs_midi_snap_mode;
                        let task = self.send(Action::SetOscEnabled(self.prefs_osc_enabled));
                        {
                            let mut state = self.state.write().expect("state lock poisoned");
                            Self::apply_preferred_devices_to_state(&mut state, &prefs);
                        }
                        self.modal = None;
                        self.info("Preferences saved: ~/.config/maolan/daw/config.toml");
                        return task;
                    }
                    Err(e) => {
                        self.error(format!("Failed to save preferences: {e}"));
                    }
                }
            }
            Message::SessionMetadataAuthorInput(ref value) => {
                self.state
                    .write()
                    .expect("state lock poisoned")
                    .session_author = value.clone();
            }
            Message::SessionMetadataAlbumInput(ref value) => {
                self.state
                    .write()
                    .expect("state lock poisoned")
                    .session_album = value.clone();
            }
            Message::SessionMetadataYearInput(ref value) => {
                self.state
                    .write()
                    .expect("state lock poisoned")
                    .session_year = value
                    .chars()
                    .filter(|c| c.is_ascii_digit())
                    .collect::<String>();
            }
            Message::SessionMetadataTrackNumberInput(ref value) => {
                self.state
                    .write()
                    .expect("state lock poisoned")
                    .session_track_number = value
                    .chars()
                    .filter(|c| c.is_ascii_digit())
                    .collect::<String>();
            }
            Message::SessionMetadataGenreInput(ref value) => {
                self.state
                    .write()
                    .expect("state lock poisoned")
                    .session_genre = value.clone();
            }
            Message::SessionMetadataSave => {
                {
                    let mut state = self.state.write().expect("state lock poisoned");
                    state.session_author = state.session_author.trim().to_string();
                    state.session_album = state.session_album.trim().to_string();
                    state.session_year = state.session_year.trim().to_string();
                    state.session_track_number = state.session_track_number.trim().to_string();
                    state.session_genre = state.session_genre.trim().to_string();
                    state.message = "Session metadata updated".to_string();
                }
                self.session_ops.has_unsaved_changes = true;
                self.modal = None;
            }
            _ => {}
        }
        self.update_children(&message);
        Task::none()
    }
}
