use super::*;

impl Maolan {
    pub(super) fn handle_session_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::AddTrackFromTemplate {
                ref name,
                ref template,
                audio_ins,
                midi_ins,
                audio_outs,
                midi_outs,
            } => {
                let task = self.send(Action::AddTrack {
                    name: name.clone(),
                    audio_ins,
                    midi_ins,
                    audio_outs,
                    midi_outs,
                });

                self.state.blocking_write().pending_track_template_load =
                    Some((name.clone(), template.clone()));

                self.modal = None;
                task
            }
            Message::NewFromTemplate(ref template_name) => {
                let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
                let template_path = format!(
                    "{}/.config/maolan/session_templates/{}",
                    home, template_name
                );
                self.state.blocking_write().message =
                    format!("Loading template '{}'...", template_name);
                Task::perform(
                    async move { std::path::PathBuf::from(template_path) },
                    Message::LoadSessionPath,
                )
            }
            Message::NewSession => {
                if !self.state.blocking_read().hw_loaded {
                    return Task::none();
                }
                self.playing = false;
                self.paused = false;
                self.transport_samples = 0.0;
                self.track_automation_runtime.clear();
                self.touch_automation_overrides.clear();
                self.touch_active_keys.clear();
                self.latch_automation_overrides.clear();
                self.loop_enabled = false;
                self.loop_range_samples = None;
                self.punch_enabled = false;
                self.punch_range_samples = None;
                self.last_playback_tick = None;
                self.record_armed = false;
                self.pending_record_after_save = false;
                self.pending_save_path = None;
                self.pending_save_tracks.clear();
                #[cfg(any(target_os = "windows", target_os = "macos"))]
                self.pending_save_vst3_states.clear();
                self.pending_audio_peaks.clear();
                self.pending_peak_file_loads.clear();
                self.pending_peak_rebuilds.clear();
                self.midi_clip_previews.clear();
                self.pending_midi_clip_previews.clear();
                self.session_dir = None;
                self.stop_recording_preview();

                let existing_tracks: Vec<String> = self
                    .state
                    .blocking_read()
                    .tracks
                    .iter()
                    .map(|t| t.name.clone())
                    .collect();
                let mut tasks = vec![
                    self.send(Action::BeginSessionRestore),
                    self.send(Action::Stop),
                    self.send(Action::SetRecordEnabled(false)),
                    self.send(Action::SetLoopRange(None)),
                    self.send(Action::SetPunchRange(None)),
                    self.send(Action::SetGlobalMidiLearnBinding {
                        target: maolan_engine::message::GlobalMidiLearnTarget::PlayPause,
                        binding: None,
                    }),
                    self.send(Action::SetGlobalMidiLearnBinding {
                        target: maolan_engine::message::GlobalMidiLearnTarget::Stop,
                        binding: None,
                    }),
                    self.send(Action::SetGlobalMidiLearnBinding {
                        target: maolan_engine::message::GlobalMidiLearnTarget::RecordToggle,
                        binding: None,
                    }),
                ];
                for name in existing_tracks {
                    tasks.push(self.send(Action::RemoveTrack(name)));
                }
                tasks.push(self.send(Action::EndSessionRestore));
                {
                    let mut state = self.state.blocking_write();
                    state.connections.clear();
                    state.selected.clear();
                    state.selected_clips.clear();
                    state.connection_view_selection = ConnectionViewSelection::None;
                    state.plugin_graph_track = None;
                    #[cfg(all(unix, not(target_os = "macos")))]
                    {
                        state.plugin_graph_plugins.clear();
                        state.plugin_graph_connections.clear();
                        state.plugin_graphs_by_track.clear();
                    }
                    state.clap_plugins_by_track.clear();
                    state.clap_states_by_track.clear();
                    state.global_midi_learn_play_pause = None;
                    state.global_midi_learn_stop = None;
                    state.global_midi_learn_record_toggle = None;
                    state.session_author.clear();
                    state.session_album.clear();
                    state.session_year.clear();
                    state.session_track_number.clear();
                    state.session_genre.clear();
                    state.message = "New session".to_string();
                    state.piano = None;
                }
                self.pending_track_freeze_restore.clear();
                self.pending_track_freeze_bounce.clear();
                self.freeze_in_progress = false;
                self.freeze_progress = 0.0;
                self.freeze_track_name = None;
                self.freeze_cancel_requested = false;
                self.has_unsaved_changes = false;
                self.session_restore_in_progress = false;
                self.last_autosave_snapshot = None;
                self.pending_recovery_session_dir = None;
                self.pending_autosave_recovery = None;
                self.pending_open_session_dir = None;
                self.pending_diagnostics_bundle_export = false;
                self.diagnostics_bundle_wait_session_report = false;
                self.diagnostics_bundle_wait_midi_report = false;
                Task::batch(tasks)
            }
            Message::Request(ref a) => {
                if let Some(expanded) = self.expand_request_to_vca_group(a) {
                    let mut tasks = Vec::with_capacity(expanded.len());
                    for action in expanded {
                        self.maybe_record_automation_from_request(&action);
                        tasks.push(self.send(action));
                    }
                    return Task::batch(tasks);
                }
                self.maybe_record_automation_from_request(a);
                self.send(a.clone())
            }
            Message::MeterPollTick => {
                let _ = CLIENT
                    .sender
                    .try_send(EngineMessage::Request(Action::RequestMeterSnapshot));
                Task::none()
            }
            _ => Task::none(),
        }
    }
}
