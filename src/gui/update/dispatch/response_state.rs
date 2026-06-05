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
                realtime_frames,
                low_watermark_frames,
                nperiods,
                sync_mode,
                hybrid_enabled,
            } => {
                let mut state = self.state.blocking_write();
                state.message = format!(
                    "Opened device {} (rate={} Hz, bits={}, exclusive={}, period={}, realtime={}, low_watermark={}, nperiods={}, sync_mode={}, hybrid={})",
                    device,
                    state.hw_sample_rate_hz.max(1),
                    bits,
                    exclusive,
                    period_frames,
                    realtime_frames,
                    low_watermark_frames,
                    nperiods,
                    sync_mode,
                    hybrid_enabled,
                );
                state.hw_loaded = true;
                state.oss_period_frames = (*period_frames).max(1);
                state.oss_realtime_frames = (*realtime_frames).max(1).min(state.oss_period_frames);
                let step = state.oss_realtime_frames.max(1);
                state.oss_low_watermark_frames =
                    ((*low_watermark_frames).max(1).min(state.oss_period_frames) / step)
                        .max(1)
                        .saturating_mul(step)
                        .min(state.oss_period_frames);
                state.oss_nperiods = (*nperiods).max(1);
                state.oss_hybrid_buffer_enabled = *hybrid_enabled;
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
            Action::MidiLearnMappingsReport { lines } => {
                let report = lines.join(" | ");
                self.midi_mappings_report_lines = lines.clone();
                let mut state = self.state.blocking_write();
                state.message = format!("MIDI mappings: {}", report);
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
            Action::TrackSetFolder {
                track_name,
                is_folder,
            } => {
                let mut state = self.state.blocking_write();
                if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name) {
                    track.is_folder = *is_folder;
                }
                true
            }
            Action::TrackSetParent {
                track_name,
                parent_name,
            } => {
                let mut state = self.state.blocking_write();
                if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name) {
                    track.parent_track = parent_name.clone();
                }
                true
            }
            Action::TrackToggleFolder { track_name } => {
                let mut state = self.state.blocking_write();
                if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name) {
                    track.folder_open = !track.folder_open;
                }
                true
            }
            _ => false,
        }
    }
}
