use super::*;

impl Maolan {
    pub(super) fn handle_response_track_action(&mut self, action: &Action) -> bool {
        match action {
            Action::TrackLevel(name, level) => {
                let mut state = self.state.blocking_write();
                if name == "hw:out" {
                    state.hw_out_level = *level;
                } else if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *name) {
                    track.level = *level;
                }
                true
            }
            Action::TrackBalance(name, balance) => {
                let mut state = self.state.blocking_write();
                if name == "hw:out" {
                    state.hw_out_balance = *balance;
                } else if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *name) {
                    track.balance = *balance;
                }
                true
            }
            Action::TrackAutomationLevel(name, level) => {
                tracing::debug!(%name, level, "DAW received TrackAutomationLevel");
                let mut state = self.state.blocking_write();
                if name == "hw:out" {
                    state.hw_out_level = *level;
                } else if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *name) {
                    track.level = *level;
                }
                true
            }
            Action::TrackAutomationBalance(name, balance) => {
                tracing::debug!(%name, balance, "DAW received TrackAutomationBalance");
                let mut state = self.state.blocking_write();
                if name == "hw:out" {
                    state.hw_out_balance = *balance;
                } else if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *name) {
                    track.balance = *balance;
                }
                true
            }
            Action::TrackSetVst3Parameter {
                track_name,
                instance_id,
                param_id,
                value,
            } => {
                let normalized = (*value).clamp(0.0, 1.0);
                let (min, max) = {
                    let state = self.state.blocking_read();
                    state
                        .plugin_parameters_by_track
                        .get(track_name)
                        .and_then(|p| p.get(instance_id))
                        .and_then(|params| params.iter().find(|p| p.param_id == *param_id))
                        .map(|p| (p.min as f32, p.max as f32))
                        .unwrap_or((0.0, 1.0))
                };
                let actual = min + normalized * (max - min);
                self.update_visible_controller_value(track_name, *instance_id, *param_id, actual);
                true
            }
            Action::TrackSetClapParameter {
                track_name,
                instance_id,
                param_id,
                value,
            }
            | Action::TrackSetClapParameterAt {
                track_name,
                instance_id,
                param_id,
                value,
                ..
            } => {
                self.update_visible_controller_value(
                    track_name,
                    *instance_id,
                    *param_id,
                    *value as f32,
                );
                true
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            Action::TrackSetLv2ControlValue {
                track_name,
                instance_id,
                index,
                value,
            } => {
                self.update_visible_controller_value(track_name, *instance_id, *index, *value);
                true
            }
            Action::TrackToggleMute(name) => {
                let mut state = self.state.blocking_write();
                if name == "hw:out" {
                    state.hw_out_muted = !state.hw_out_muted;
                } else if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *name) {
                    track.muted = !track.muted;
                }
                true
            }
            Action::TrackTogglePhase(name) => {
                if let Some(track) = self
                    .state
                    .blocking_write()
                    .tracks
                    .iter_mut()
                    .find(|t| t.name == *name && !t.is_folder)
                {
                    track.phase_inverted = !track.phase_inverted;
                }
                true
            }
            Action::TrackToggleSolo(name) => {
                if let Some(track) = self
                    .state
                    .blocking_write()
                    .tracks
                    .iter_mut()
                    .find(|t| t.name == *name)
                {
                    track.soloed = !track.soloed;
                }
                true
            }
            Action::TrackToggleMaster(name) => {
                if let Some(track) = self
                    .state
                    .blocking_write()
                    .tracks
                    .iter_mut()
                    .find(|t| t.name == *name)
                {
                    // Folder tracks can never be master; ignore any engine echo
                    // that would set the flag on a folder.
                    if !track.is_folder {
                        track.is_master = !track.is_master;
                    }
                }
                true
            }
            Action::TrackToggleArm(name) => {
                if let Some(track) = self
                    .state
                    .blocking_write()
                    .tracks
                    .iter_mut()
                    .find(|t| t.name == *name)
                {
                    let _old = track.armed;
                    track.armed = !track.armed;
                }
                true
            }
            Action::TrackToggleInputMonitor { track_name, lane } => {
                if let Some(track) = self
                    .state
                    .blocking_write()
                    .tracks
                    .iter_mut()
                    .find(|t| t.name == *track_name)
                    && let Some(monitor) = track.input_monitor.get_mut(*lane)
                {
                    *monitor = !*monitor;
                }
                true
            }
            Action::TrackToggleDiskMonitor { track_name, lane } => {
                if let Some(track) = self
                    .state
                    .blocking_write()
                    .tracks
                    .iter_mut()
                    .find(|t| t.name == *track_name)
                    && let Some(monitor) = track.disk_monitor.get_mut(*lane)
                {
                    *monitor = !*monitor;
                }
                true
            }
            Action::TrackToggleMidiInputMonitor { track_name, lane } => {
                if let Some(track) = self
                    .state
                    .blocking_write()
                    .tracks
                    .iter_mut()
                    .find(|t| t.name == *track_name)
                    && let Some(monitor) = track.midi_input_monitor.get_mut(*lane)
                {
                    *monitor = !*monitor;
                }
                true
            }
            Action::TrackToggleMidiDiskMonitor { track_name, lane } => {
                if let Some(track) = self
                    .state
                    .blocking_write()
                    .tracks
                    .iter_mut()
                    .find(|t| t.name == *track_name)
                    && let Some(monitor) = track.midi_disk_monitor.get_mut(*lane)
                {
                    *monitor = !*monitor;
                }
                true
            }
            Action::TrackSetColor { track_name, color } => {
                if let Some(track) = self
                    .state
                    .blocking_write()
                    .tracks
                    .iter_mut()
                    .find(|t| t.name == *track_name)
                {
                    track.color = color.map(|c| iced::Color::from_rgba(c.r, c.g, c.b, c.a));
                }
                true
            }
            Action::TrackSetMidiLaneChannel {
                track_name,
                lane,
                channel,
            } => {
                if let Some(track) = self
                    .state
                    .blocking_write()
                    .tracks
                    .iter_mut()
                    .find(|t| t.name == *track_name)
                {
                    if track.midi_lane_channels.len() < track.midi.ins {
                        track.midi_lane_channels.resize(track.midi.ins, None);
                    }
                    if let Some(slot) = track.midi_lane_channels.get_mut(*lane) {
                        *slot = *channel;
                    }
                }
                true
            }
            Action::TrackAddAudioInput(name) => {
                let mut state = self.state.blocking_write();
                if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *name) {
                    track.audio.ins = track.audio.ins.saturating_add(1);
                    track.height = track.height.max(track.min_height_for_layout());
                }
                true
            }
            Action::TrackAddAudioOutput(name) => {
                let mut state = self.state.blocking_write();
                if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *name) {
                    track.audio.outs = track.audio.outs.saturating_add(1);
                    if track.meter_out_db.len() < track.audio.outs {
                        track.meter_out_db.resize(track.audio.outs, -90.0);
                    }
                }
                true
            }
            Action::TrackRemoveAudioInput(name) => {
                let mut state = self.state.blocking_write();
                let removed_port =
                    state
                        .tracks
                        .iter()
                        .find(|t| t.name == *name)
                        .and_then(|track| {
                            (track.audio.ins > track.primary_audio_ins())
                                .then_some(track.audio.ins - 1)
                        });
                if let Some(removed_port) = removed_port {
                    state.connections.retain(|conn| {
                        !(conn.kind == maolan_engine::kind::Kind::Audio
                            && conn.to_track == *name
                            && conn.to_port == removed_port)
                    });
                    if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *name) {
                        track.audio.ins -= 1;
                        track.height = track.height.max(track.min_height_for_layout());
                    }
                }
                true
            }
            Action::TrackRemoveAudioOutput(name) => {
                let mut state = self.state.blocking_write();
                let removed_port =
                    state
                        .tracks
                        .iter()
                        .find(|t| t.name == *name)
                        .and_then(|track| {
                            (track.audio.outs > track.primary_audio_outs())
                                .then_some(track.audio.outs - 1)
                        });
                if let Some(removed_port) = removed_port {
                    state.connections.retain(|conn| {
                        !(conn.kind == maolan_engine::kind::Kind::Audio
                            && conn.from_track == *name
                            && conn.from_port == removed_port)
                    });
                    if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *name) {
                        track.audio.outs -= 1;
                        track.meter_out_db.truncate(track.audio.outs);
                    }
                }
                true
            }
            Action::TrackArmMidiLearn { track_name, target } => {
                self.state.blocking_write().message = format!(
                    "MIDI learn armed for '{}' ({:?}). Move a hardware MIDI CC control.",
                    track_name, target
                );
                true
            }
            Action::TrackSetMidiLearnBinding {
                track_name,
                target,
                binding,
            } => {
                if let Some(track) = self
                    .state
                    .blocking_write()
                    .tracks
                    .iter_mut()
                    .find(|t| t.name == *track_name)
                {
                    match target {
                        maolan_engine::message::TrackMidiLearnTarget::Volume => {
                            track.midi_learn_volume = binding.clone();
                        }
                        maolan_engine::message::TrackMidiLearnTarget::Balance => {
                            track.midi_learn_balance = binding.clone();
                        }
                        maolan_engine::message::TrackMidiLearnTarget::Mute => {
                            track.midi_learn_mute = binding.clone();
                        }
                        maolan_engine::message::TrackMidiLearnTarget::Solo => {
                            track.midi_learn_solo = binding.clone();
                        }
                        maolan_engine::message::TrackMidiLearnTarget::Arm => {
                            track.midi_learn_arm = binding.clone();
                        }
                        maolan_engine::message::TrackMidiLearnTarget::InputMonitor => {
                            track.midi_learn_input_monitor = binding.clone();
                        }
                        maolan_engine::message::TrackMidiLearnTarget::DiskMonitor => {
                            track.midi_learn_disk_monitor = binding.clone();
                        }
                    }
                }
                let message = if let Some(binding) = binding {
                    format!(
                        "MIDI learn mapped '{}' {:?} to CH{} CC{}",
                        track_name,
                        target,
                        binding.channel + 1,
                        binding.cc
                    )
                } else {
                    format!("MIDI learn cleared for '{}' {:?}", track_name, target)
                };
                self.state.blocking_write().message = message;
                if self.midi_mappings_panel_open {
                    self.rebuild_midi_mappings_report_lines_from_state();
                }
                true
            }
            Action::SetGlobalMidiLearnBinding { target, binding } => {
                {
                    let mut state = self.state.blocking_write();
                    match target {
                        maolan_engine::message::GlobalMidiLearnTarget::PlayPause => {
                            state.global_midi_learn_play_pause = binding.clone();
                        }
                        maolan_engine::message::GlobalMidiLearnTarget::Stop => {
                            state.global_midi_learn_stop = binding.clone();
                        }
                        maolan_engine::message::GlobalMidiLearnTarget::RecordToggle => {
                            state.global_midi_learn_record_toggle = binding.clone();
                        }
                    }
                }
                self.state.blocking_write().message = if let Some(binding) = binding {
                    format!(
                        "Global MIDI learn mapped {:?} to CH{} CC{}",
                        target,
                        binding.channel + 1,
                        binding.cc
                    )
                } else {
                    format!("Global MIDI learn cleared for {:?}", target)
                };
                if self.midi_mappings_panel_open {
                    self.rebuild_midi_mappings_report_lines_from_state();
                }
                true
            }
            Action::SetSessionMidiLearnBinding { target, binding } => {
                {
                    let mut state = self.state.blocking_write();
                    match target {
                        maolan_engine::message::SessionMidiLearnTarget::Slot {
                            track_name,
                            scene_index,
                        } => {
                            if let Some(binding) = binding.clone() {
                                state
                                    .session_midi_learn_slots
                                    .insert((track_name.clone(), *scene_index), binding);
                            } else {
                                state
                                    .session_midi_learn_slots
                                    .remove(&(track_name.clone(), *scene_index));
                            }
                        }
                        maolan_engine::message::SessionMidiLearnTarget::Scene(scene_index) => {
                            if let Some(binding) = binding.clone() {
                                state
                                    .session_midi_learn_scenes
                                    .insert(*scene_index, binding);
                            } else {
                                state.session_midi_learn_scenes.remove(scene_index);
                            }
                        }
                        maolan_engine::message::SessionMidiLearnTarget::StopTrack(track_name) => {
                            if let Some(binding) = binding.clone() {
                                state
                                    .session_midi_learn_stop_track
                                    .insert(track_name.clone(), binding);
                            } else {
                                state.session_midi_learn_stop_track.remove(track_name);
                            }
                        }
                        maolan_engine::message::SessionMidiLearnTarget::StopAll => {
                            state.session_midi_learn_stop_all = binding.clone();
                        }
                    }
                }
                self.state.blocking_write().message = if let Some(binding) = binding {
                    format!(
                        "Session MIDI learn mapped {:?} to CH{} CC{}",
                        target,
                        binding.channel + 1,
                        binding.cc
                    )
                } else {
                    format!("Session MIDI learn cleared for {:?}", target)
                };
                if self.midi_mappings_panel_open {
                    self.rebuild_midi_mappings_report_lines_from_state();
                }
                true
            }
            Action::TrackSetFrozen { track_name, frozen } => {
                self.state.blocking_write().message = if *frozen {
                    format!("Track '{track_name}' frozen")
                } else {
                    format!("Track '{track_name}' unfrozen")
                };
                true
            }
            Action::SetClipPluginGraphJson {
                track_name,
                clip_index,
                plugin_graph_json,
            } => {
                let mut state = self.state.blocking_write();
                if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name)
                    && let Some(clip) = track.audio.clips.get_mut(*clip_index)
                {
                    clip.plugin_graph_json = plugin_graph_json.clone();
                }
                true
            }
            Action::SetTrackAutomationLanes {
                track_name,
                lanes,
                mode,
            } => {
                let mut state = self.state.blocking_write();
                if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name) {
                    let previous_lane_height = track
                        .lane_layout()
                        .representative_height()
                        .max(crate::consts::state_track::TRACK_SUBTRACK_MIN_HEIGHT);
                    let previously_visible = track.automation_lane_count();
                    track.automation_lanes =
                        serde_json::from_value(lanes.clone()).unwrap_or_default();
                    let lanes_delta =
                        track.automation_lane_count() as isize - previously_visible as isize;
                    track.adjust_height_for_automation_lanes(previous_lane_height, lanes_delta);
                    track.automation_mode = (*mode).into();
                }
                true
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            Action::TrackSetPluginBypassed {
                track_name,
                instance_id,
                bypassed,
                ..
            } => {
                let mut state = self.state.blocking_write();
                if state.plugin_graph_track.as_deref() == Some(track_name)
                    && let Some(plugin) = state
                        .plugin_graph_plugins
                        .iter_mut()
                        .find(|p| p.instance_id == *instance_id)
                {
                    plugin.bypassed = *bypassed;
                }
                if let Some((plugins, _)) = state.plugin_graphs_by_track.get_mut(track_name)
                    && let Some(plugin) = plugins.iter_mut().find(|p| p.instance_id == *instance_id)
                {
                    plugin.bypassed = *bypassed;
                }
                true
            }
            #[cfg(not(all(unix, not(target_os = "macos"))))]
            Action::TrackSetPluginBypassed { .. } => true,
            _ => false,
        }
    }
}
