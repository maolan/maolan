use super::*;

impl Maolan {
    pub(super) fn handle_session_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::AddTrack(crate::message::AddTrack::Submit) => {
                let existing_names: std::collections::HashSet<String> = self
                    .state
                    .read()
                    .expect("state lock poisoned")
                    .tracks
                    .iter()
                    .map(|t| t.name.clone())
                    .collect();
                let messages = self.add_track.create_messages();
                let mut new_names = Vec::new();
                for message in &messages {
                    let name = match message {
                        Message::Request(maolan_engine::message::Action::AddTrack {
                            name, ..
                        }) => Some(name.as_str()),
                        Message::AddTrackFromTemplate { name, .. } => Some(name.as_str()),
                        _ => None,
                    };
                    if let Some(name) = name {
                        if existing_names.contains(name) {
                            self.error(format!("Track '{}' already exists", name));
                            return Task::none();
                        }
                        new_names.push(name.to_string());
                    }
                }
                {
                    let mut state = self.state.write().expect("state lock poisoned");
                    for name in &new_names {
                        state.session.ensure_track_slots(name);
                    }
                }
                let tasks: Vec<_> = messages
                    .into_iter()
                    .map(|message| self.handle_session_message(message))
                    .collect();
                if !tasks.is_empty() {
                    self.modal = None;
                }
                Task::batch(tasks)
            }
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
                    folder: false,
                    mixosc_addr: None,
                });

                self.state
                    .write()
                    .expect("state lock poisoned")
                    .pending_track_template_loads
                    .push((name.clone(), template.clone()));
                task
            }
            Message::AddFolderFromTemplate {
                ref name,
                ref template,
            } => {
                self.modal = None;
                self.apply_folder_template(name.clone(), template.clone())
            }
            Message::ApplyTemplate(crate::message::ApplyTemplate::Submit) => {
                let (track_name, template, is_folder) = {
                    let state = self.state.read().expect("state lock poisoned");
                    let Some(dialog) = self.apply_template.dialog.as_ref() else {
                        return Task::none();
                    };
                    let is_folder = state
                        .tracks
                        .iter()
                        .find(|t| t.name == dialog.track_name)
                        .is_some_and(|t| t.is_folder);
                    (
                        dialog.track_name.clone(),
                        dialog.selected_template.clone(),
                        is_folder,
                    )
                };
                let Some(template) = template else {
                    return Task::done(Message::Response(Err("No template selected".to_string())));
                };
                self.modal = None;
                self.apply_template.close();
                if is_folder {
                    self.apply_folder_template(track_name, template)
                } else {
                    self.apply_track_template(track_name, template)
                }
            }
            Message::ApplyTrackTemplate {
                ref track_name,
                ref template,
            } => self.apply_track_template(track_name.clone(), template.clone()),
            Message::ApplyFolderTemplate {
                ref track_name,
                ref template,
            } => self.apply_folder_template(track_name.clone(), template.clone()),
            Message::NewFromTemplate(ref template_name) => {
                let template_path = crate::config::daw_config_dir()
                    .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"))
                    .join("session_templates")
                    .join(template_name.as_str());
                self.state.write().expect("state lock poisoned").message =
                    format!("Loading template '{}'...", template_name);
                Task::perform(async move { template_path }, Message::LoadSessionPath)
            }
            Message::NewSession => {
                if !self.state.read().expect("state lock poisoned").hw_loaded {
                    return Task::none();
                }
                self.transport.playing = false;
                self.transport.paused = false;
                self.transport.transport_samples = 0.0;
                self.automation.track_automation_runtime.clear();
                self.automation.touch_automation_overrides.clear();
                self.automation.touch_active_keys.clear();
                self.automation.latch_automation_overrides.clear();
                self.transport.loop_enabled = false;
                self.transport.loop_range_samples = None;
                self.transport.punch_enabled = false;
                self.transport.punch_range_samples = None;
                self.transport.last_playback_tick = None;
                self.transport.record_armed = false;
                self.transport.pending_record_after_save = false;
                self.pending.pending_save_path = None;
                self.pending.pending_save_tracks.clear();
                self.pending.pending_save_clap_tracks.clear();
                self.pending.pending_save_clap_clips.clear();
                self.pending.pending_peak_file_loads.clear();
                self.pending.pending_peak_rebuilds.clear();
                self.transport.midi_clip_previews.clear();
                self.pending.pending_midi_clip_previews.clear();
                self.drag.session_slot_record_target = None;
                self.session_dir = None;
                self.session_branch = "main".to_string();
                self.rec.stop_recording_preview();

                let existing_tracks: Vec<String> = {
                    let state = self.state.read().expect("state lock poisoned");
                    state.tracks.iter().map(|t| t.name.clone()).collect()
                };
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
                tasks.push(self.send_modulators_to_engine());
                tasks.push(self.send(Action::EndSessionRestore));
                {
                    let mut state = self.state.write().expect("state lock poisoned");
                    state.connections.clear();
                    state.selected.clear();
                    state.selected_clips.clear();
                    state.connection_view_selection = ConnectionViewSelection::None;
                    state.plugin_graph_track = None;
                    state.plugin_graph_clip = None;
                    #[cfg(unix)]
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
                    state.selected_modulator_id = None;
                }
                self.pending.pending_track_freeze_restore.clear();
                self.pending.pending_track_midi_editor_view_mode.clear();
                self.pending.pending_track_freeze_bounce.clear();
                self.modulators.clear();
                self.selected_modulator_id = None;
                self.pending.freeze_in_progress = false;
                self.pending.freeze_progress = 0.0;
                self.pending.freeze_track_name = None;
                self.pending.freeze_cancel_requested = false;
                self.session_ops.has_unsaved_changes = false;
                self.session_ops.engine_dirty = false;
                self.session_ops.pending_exit_after_save = false;
                self.session_ops.session_restore_in_progress = false;
                self.session_ops.last_autosave_snapshot = None;
                self.session_ops.pending_recovery_session_dir = None;
                self.session_ops.pending_autosave_recovery = None;
                self.session_ops.pending_open_session_dir = None;
                Task::batch(tasks)
            }
            Message::Request(ref a) => {
                if let Action::TransportPosition(sample) = a {
                    self.transport.transport_samples = *sample as f64;
                }
                if let Some(expanded) = self.expand_request_to_folder_children(a) {
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
            Message::RequestBatch(ref actions) => {
                let mut tasks = Vec::with_capacity(actions.len() + 2);
                tasks.push(self.send(Action::BeginHistoryGroup));
                for action in actions {
                    if let Action::TransportPosition(sample) = action {
                        self.transport.transport_samples = *sample as f64;
                    }
                    if let Some(expanded) = self.expand_request_to_folder_children(action) {
                        for expanded_action in expanded {
                            self.maybe_record_automation_from_request(&expanded_action);
                            tasks.push(self.send(expanded_action));
                        }
                    } else {
                        self.maybe_record_automation_from_request(action);
                        tasks.push(self.send(action.clone()));
                    }
                }
                tasks.push(self.send(Action::EndHistoryGroup));
                Task::batch(tasks)
            }
            Message::MeterPollTick => {
                if !self.transport.playing
                    && !self.transport.paused
                    && !self.transport.live_session_playing
                    && let Some(action) = self.transport.meter_stop_decay_action()
                {
                    return self
                        .handle_response_freeze_meter_action(&action)
                        .unwrap_or_else(Task::none);
                }
                if let Some(snapshot) = CLIENT.meter_snapshot() {
                    let action = Action::MeterSnapshot {
                        hw_out_db: std::sync::Arc::new(snapshot.hw_out_db),
                        track_meters: std::sync::Arc::new(snapshot.track_meters),
                    };
                    self.handle_response_freeze_meter_action(&action)
                        .unwrap_or_else(Task::none)
                } else {
                    Task::none()
                }
            }
            _ => Task::none(),
        }
    }
}

impl Maolan {
    pub(super) fn handle_branch_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::BranchInput(ref value) => {
                self.session_ops.pending_branch_input = value.clone();
            }
            Message::BranchCreate(ref name) => {
                if let Some(session_dir) = self.session_dir.clone() {
                    let src = session_dir.join(format!("{}.json", self.session_branch));
                    let dst = session_dir.join(format!("{}.json", name));
                    if src.exists() {
                        if let Err(e) = std::fs::copy(&src, &dst) {
                            self.state.write().expect("state lock poisoned").message =
                                format!("Failed to create branch '{}': {}", name, e);
                        } else {
                            self.session_branch = name.clone();
                            self.session_ops.pending_branch_input.clear();
                            self.state.write().expect("state lock poisoned").message =
                                format!("Created and switched to branch '{}'", name);
                        }
                    } else {
                        self.state.write().expect("state lock poisoned").message =
                            format!("Source branch file '{}' not found", src.display());
                    }
                } else {
                    self.state.write().expect("state lock poisoned").message =
                        "No session directory set. Save the session first.".to_string();
                }
            }
            Message::BranchSwitch(ref name) => {
                if let Some(session_dir) = self.session_dir.clone() {
                    let branch_file = session_dir.join(format!("{}.json", name));
                    if branch_file.exists() {
                        self.session_branch = name.clone();
                        self.modal = None;
                        self.rec.stop_recording_preview();
                        self.state.write().expect("state lock poisoned").message =
                            format!("Switching to branch '{}'...", name);
                        return Task::perform(async move { session_dir }, Message::LoadSessionPath);
                    } else {
                        self.state.write().expect("state lock poisoned").message =
                            format!("Branch file '{}' not found", branch_file.display());
                    }
                } else {
                    self.state.write().expect("state lock poisoned").message =
                        "No session directory set. Save the session first.".to_string();
                }
            }
            Message::BranchMerge(ref name) => {
                if let Some(session_dir) = self.session_dir.clone() {
                    if *name == self.session_branch {
                        self.state.write().expect("state lock poisoned").message =
                            "Cannot merge a branch into itself".to_string();
                    } else {
                        let src = session_dir.join(format!("{}.json", name));
                        let dst = session_dir.join(format!("{}.json", self.session_branch));
                        if !src.exists() {
                            self.state.write().expect("state lock poisoned").message =
                                format!("Branch file '{}' not found", src.display());
                        } else if let Err(e) = std::fs::copy(&src, &dst) {
                            self.state.write().expect("state lock poisoned").message =
                                format!("Failed to merge branch '{}': {}", name, e);
                        } else {
                            let commit_dir = session_dir
                                .join(".maolan_commits")
                                .join(&self.session_branch);
                            if let Err(e) = std::fs::create_dir_all(&commit_dir) {
                                self.state.write().expect("state lock poisoned").message =
                                    format!("Failed to create commit dir: {}", e);
                            } else {
                                let commit_filename =
                                    chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
                                let commit_path =
                                    commit_dir.join(format!("{}.json", commit_filename));
                                if let Err(e) = std::fs::copy(&dst, &commit_path) {
                                    self.state.write().expect("state lock poisoned").message =
                                        format!("Failed to create merge commit: {}", e);
                                } else {
                                    self.modal = None;
                                    self.rec.stop_recording_preview();
                                    self.state.write().expect("state lock poisoned").message = format!(
                                        "Merged branch '{}' into '{}'",
                                        name, self.session_branch
                                    );
                                    return Task::perform(
                                        async move { session_dir },
                                        Message::LoadSessionPath,
                                    );
                                }
                            }
                        }
                    }
                } else {
                    self.state.write().expect("state lock poisoned").message =
                        "No session directory set. Save the session first.".to_string();
                }
            }
            Message::BranchResetHard(ref name) => {
                if let Some(session_dir) = self.session_dir.clone() {
                    if *name == self.session_branch {
                        self.state.write().expect("state lock poisoned").message =
                            "Cannot reset to the current branch".to_string();
                    } else {
                        let src = session_dir.join(format!("{}.json", name));
                        let dst = session_dir.join(format!("{}.json", self.session_branch));
                        if !src.exists() {
                            self.state.write().expect("state lock poisoned").message =
                                format!("Branch file '{}' not found", src.display());
                        } else if let Err(e) = std::fs::copy(&src, &dst) {
                            self.state.write().expect("state lock poisoned").message =
                                format!("Failed to reset to branch '{}': {}", name, e);
                        } else {
                            self.modal = None;
                            self.rec.stop_recording_preview();
                            self.state.write().expect("state lock poisoned").message = format!(
                                "Reset '{}' to state of branch '{}'",
                                self.session_branch, name
                            );
                            return Task::perform(
                                async move { session_dir },
                                Message::LoadSessionPath,
                            );
                        }
                    }
                } else {
                    self.state.write().expect("state lock poisoned").message =
                        "No session directory set. Save the session first.".to_string();
                }
            }
            Message::BranchCopyTrack {
                ref branch,
                ref track_name,
            } => {
                if let Some(session_dir) = self.session_dir.clone() {
                    match self.copy_track_from_branch(&session_dir, branch, track_name) {
                        Ok(task) => {
                            self.modal = None;
                            self.rec.stop_recording_preview();
                            self.state.write().expect("state lock poisoned").message =
                                format!("Copied track '{}' from branch '{}'", track_name, branch);
                            return task;
                        }
                        Err(e) => {
                            self.state.write().expect("state lock poisoned").message = e;
                        }
                    }
                } else {
                    self.state.write().expect("state lock poisoned").message =
                        "No session directory set. Save the session first.".to_string();
                }
            }
            _ => {}
        }
        self.update_children(&message);
        Task::none()
    }
}
