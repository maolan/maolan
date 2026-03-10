use super::*;

impl Maolan {
    pub(super) fn handle_response_engine_state_action(&mut self, action: &Action) -> bool {
        match action {
            Action::Connect {
                from_track,
                from_port,
                to_track,
                to_port,
                kind,
            } => {
                let mut state = self.state.blocking_write();
                state.connections.push(crate::state::Connection {
                    from_track: from_track.clone(),
                    from_port: *from_port,
                    to_track: to_track.clone(),
                    to_port: *to_port,
                    kind: *kind,
                });
                true
            }
            Action::Disconnect {
                from_track,
                from_port,
                to_track,
                to_port,
                kind,
            } => {
                let mut state = self.state.blocking_write();
                let original_len = state.connections.len();

                state.connections.retain(|conn| {
                    !(conn.from_track == from_track.as_str()
                        && conn.from_port == *from_port
                        && conn.to_track == to_track.as_str()
                        && conn.to_port == *to_port
                        && conn.kind == *kind)
                });
                if state.connections.len() < original_len {
                    state.message = format!("Disconnected {} from {}", from_track, to_track);
                }
                true
            }
            Action::OpenAudioDevice {
                device,
                input_device: _,
                sample_rate_hz: _,
                bits,
                exclusive,
                period_frames,
                nperiods,
                sync_mode,
            } => {
                let mut state = self.state.blocking_write();
                state.message = format!(
                    "Opened device {} (rate={} Hz, bits={}, exclusive={}, period={}, nperiods={}, sync_mode={})",
                    device,
                    state.hw_sample_rate_hz.max(1),
                    bits,
                    exclusive,
                    period_frames,
                    nperiods,
                    sync_mode
                );
                state.hw_loaded = true;
                state.oss_period_frames = (*period_frames).max(1);
                state.oss_nperiods = (*nperiods).max(1);
                true
            }
            Action::OpenMidiInputDevice(s) => {
                let mut state = self.state.blocking_write();
                if !state.opened_midi_in_hw.iter().any(|name| name == s) {
                    state.opened_midi_in_hw.push(s.clone());
                }
                state
                    .midi_hw_labels
                    .entry(s.clone())
                    .or_insert_with(|| platform::kernel_midi_label(s));
                state.message = format!("Opened MIDI input {s}");
                true
            }
            Action::OpenMidiOutputDevice(s) => {
                let mut state = self.state.blocking_write();
                if !state.opened_midi_out_hw.iter().any(|name| name == s) {
                    state.opened_midi_out_hw.push(s.clone());
                }
                state
                    .midi_hw_labels
                    .entry(s.clone())
                    .or_insert_with(|| platform::kernel_midi_label(s));
                state.message = format!("Opened MIDI output {s}");
                true
            }
            Action::HWInfo {
                channels,
                rate,
                input,
            } => {
                if *rate > 0 {
                    self.playback_rate_hz = *rate as f64;
                }
                let mut state = self.state.blocking_write();
                if *rate > 0 {
                    state.hw_sample_rate_hz = *rate as i32;
                }
                if !state.hw_loaded {
                    state.hw_loaded = true;
                }
                let direction = if *input { "input" } else { "output" };
                state.message = format!("HW {direction} channels: {channels} @ {rate} Hz");
                if *input {
                    state.hw_in = Some(HW {
                        channels: *channels,
                    });
                } else {
                    state.hw_out = Some(HW {
                        channels: *channels,
                    });
                    if state.hw_out_meter_db.len() != *channels {
                        state.hw_out_meter_db = vec![-90.0; *channels];
                    }
                }
                true
            }
            Action::SessionDiagnosticsReport {
                track_count,
                frozen_track_count,
                audio_clip_count,
                midi_clip_count,
                #[cfg(all(unix, not(target_os = "macos")))]
                lv2_instance_count,
                vst3_instance_count,
                clap_instance_count,
                pending_requests,
                workers_total,
                workers_ready,
                pending_hw_midi_events,
                playing,
                transport_sample,
                tempo_bpm,
                sample_rate_hz,
                cycle_samples,
            } => {
                let plugin_summary = format!(
                    "VST3={} CLAP={}{}",
                    vst3_instance_count,
                    clap_instance_count,
                    {
                        #[cfg(all(unix, not(target_os = "macos")))]
                        {
                            format!(" LV2={}", lv2_instance_count)
                        }
                        #[cfg(not(all(unix, not(target_os = "macos"))))]
                        {
                            String::new()
                        }
                    }
                );
                let report = format!(
                    "Session Diagnostics: tracks={} frozen={} audio_clips={} midi_clips={} | plugins: {} | engine: playing={} transport={} tempo={:.2} BPM | audio: rate={}Hz cycle={} | workers: ready={}/{} pending_req={} pending_midi_ev={}",
                    track_count,
                    frozen_track_count,
                    audio_clip_count,
                    midi_clip_count,
                    plugin_summary,
                    playing,
                    transport_sample,
                    tempo_bpm,
                    sample_rate_hz,
                    cycle_samples,
                    workers_ready,
                    workers_total,
                    pending_requests,
                    pending_hw_midi_events
                );
                let mut state = self.state.blocking_write();
                state.message = report.clone();
                state.diagnostics_report = Some(report);
                if self.pending_diagnostics_bundle_export {
                    self.diagnostics_bundle_wait_session_report = false;
                    if !self.diagnostics_bundle_wait_session_report
                        && !self.diagnostics_bundle_wait_midi_report
                    {
                        self.pending_diagnostics_bundle_export = false;
                        match self.export_diagnostics_bundle() {
                            Ok(path) => {
                                state.message =
                                    format!("Diagnostics bundle exported: {}", path.display());
                            }
                            Err(e) => {
                                state.message = format!("Diagnostics bundle export failed: {e}");
                            }
                        }
                    }
                }
                true
            }
            Action::MidiLearnMappingsReport { lines } => {
                let report = lines.join(" | ");
                self.midi_mappings_report_lines = lines.clone();
                let mut state = self.state.blocking_write();
                state.message = format!("MIDI mappings: {}", report);
                state.diagnostics_report = Some(format!("MIDI mappings: {}", report));
                if self.pending_diagnostics_bundle_export {
                    self.diagnostics_bundle_wait_midi_report = false;
                    if !self.diagnostics_bundle_wait_session_report
                        && !self.diagnostics_bundle_wait_midi_report
                    {
                        self.pending_diagnostics_bundle_export = false;
                        match self.export_diagnostics_bundle() {
                            Ok(path) => {
                                state.message =
                                    format!("Diagnostics bundle exported: {}", path.display());
                            }
                            Err(e) => {
                                state.message = format!("Diagnostics bundle export failed: {e}");
                            }
                        }
                    }
                }
                true
            }
            Action::ClearAllMidiLearnBindings => {
                self.midi_mappings_report_lines.clear();
                let mut state = self.state.blocking_write();
                state.global_midi_learn_play_pause = None;
                state.global_midi_learn_stop = None;
                state.global_midi_learn_record_toggle = None;
                for track in &mut state.tracks {
                    track.midi_learn_volume = None;
                    track.midi_learn_balance = None;
                    track.midi_learn_mute = None;
                    track.midi_learn_solo = None;
                    track.midi_learn_arm = None;
                    track.midi_learn_input_monitor = None;
                    track.midi_learn_disk_monitor = None;
                }
                state.message = "Cleared all MIDI mappings".to_string();
                true
            }
            _ => false,
        }
    }
}
