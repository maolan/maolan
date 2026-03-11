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
                if let Some(track) = self
                    .state
                    .blocking_write()
                    .tracks
                    .iter_mut()
                    .find(|t| t.name == *name)
                {
                    track.level = *level;
                }
                true
            }
            Action::TrackAutomationBalance(name, balance) => {
                if let Some(track) = self
                    .state
                    .blocking_write()
                    .tracks
                    .iter_mut()
                    .find(|t| t.name == *name)
                {
                    track.balance = *balance;
                }
                true
            }
            Action::TrackAutomationMute(name, muted) => {
                if let Some(track) = self
                    .state
                    .blocking_write()
                    .tracks
                    .iter_mut()
                    .find(|t| t.name == *name)
                {
                    track.muted = *muted;
                }
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
            Action::TrackToggleArm(name) => {
                if let Some(track) = self
                    .state
                    .blocking_write()
                    .tracks
                    .iter_mut()
                    .find(|t| t.name == *name)
                {
                    track.armed = !track.armed;
                }
                true
            }
            Action::TrackToggleInputMonitor(name) => {
                if let Some(track) = self
                    .state
                    .blocking_write()
                    .tracks
                    .iter_mut()
                    .find(|t| t.name == *name)
                {
                    track.input_monitor = !track.input_monitor;
                }
                true
            }
            Action::TrackToggleDiskMonitor(name) => {
                if let Some(track) = self
                    .state
                    .blocking_write()
                    .tracks
                    .iter_mut()
                    .find(|t| t.name == *name)
                {
                    track.disk_monitor = !track.disk_monitor;
                }
                true
            }
            Action::TrackSetVcaMaster {
                track_name,
                master_track,
            } => {
                if let Some(track) = self
                    .state
                    .blocking_write()
                    .tracks
                    .iter_mut()
                    .find(|t| t.name == *track_name)
                {
                    track.vca_master = master_track.clone();
                }
                true
            }
            Action::TrackAddAudioInput(name) => {
                let mut state = self.state.blocking_write();
                if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *name) {
                    track.audio.ins = track.audio.ins.saturating_add(1);
                    track.height = track.height.max(track.min_height_for_layout().max(60.0));
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
                let removed_port = state
                    .tracks
                    .iter()
                    .find(|t| t.name == *name)
                    .and_then(|track| {
                        (track.audio.ins > track.primary_audio_ins()).then_some(track.audio.ins - 1)
                    });
                if let Some(removed_port) = removed_port {
                    state.connections.retain(|conn| {
                        !(conn.kind == maolan_engine::kind::Kind::Audio
                            && conn.to_track == *name
                            && conn.to_port == removed_port)
                    });
                    if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *name) {
                        track.audio.ins -= 1;
                        track.height = track.height.max(track.min_height_for_layout().max(60.0));
                    }
                }
                true
            }
            Action::TrackRemoveAudioOutput(name) => {
                let mut state = self.state.blocking_write();
                let removed_port = state
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
            Action::TrackSetFrozen { track_name, frozen } => {
                self.state.blocking_write().message = if *frozen {
                    format!("Track '{track_name}' frozen")
                } else {
                    format!("Track '{track_name}' unfrozen")
                };
                true
            }
            _ => false,
        }
    }
}
