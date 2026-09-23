use super::*;
#[cfg(test)]
use crate::gui::field_groups::{RecordingPreviewState, TransportUiState};

impl Maolan {
    pub(super) fn handle_response_timing_state_action(&mut self, action: &Action) -> bool {
        match action {
            Action::Play => {
                self.transport.stop_meter_stop_decay();
                self.transport.playing = true;
                self.transport.paused = false;
                self.transport.last_playback_tick = Some(Instant::now());
                true
            }
            Action::Pause => {
                self.transport.stop_meter_stop_decay();
                self.transport.playing = true;
                self.transport.paused = true;
                self.transport.last_playback_tick = None;
                true
            }
            Action::Stop => {
                self.transport.start_meter_stop_decay(&self.state);
                self.transport.playing = false;
                self.transport.paused = false;
                self.transport.last_playback_tick = None;
                self.automation.track_automation_runtime.clear();
                self.automation.touch_automation_overrides.clear();
                self.automation.touch_active_keys.clear();
                self.automation.latch_automation_overrides.clear();
                true
            }
            Action::BeginSessionRestore => {
                self.session_ops.session_restore_in_progress = true;
                self.session_ops.last_autosave_snapshot = None;
                self.session_ops.pending_autosave_recovery = None;
                self.session_ops.pending_open_session_dir = None;
                self.state
                    .write()
                    .expect("state lock poisoned")
                    .undo_track_indices
                    .clear();
                true
            }
            Action::EndSessionRestore => {
                self.session_ops.session_restore_in_progress = false;
                self.session_ops.last_autosave_snapshot = None;
                self.session_ops.pending_autosave_recovery = None;
                self.session_ops.pending_open_session_dir = None;
                true
            }
            Action::TransportPosition(sample) => {
                self.transport.pending_transport_position = None;
                self.transport.transport_samples = *sample as f64;
                if self.transport.playing && !self.transport.paused {
                    self.transport.last_playback_tick = Some(Instant::now());
                }

                if !self.transport.playing && self.rec.recording_preview_start_sample.is_some() {
                    self.rec.stop_recording_preview();
                }
                true
            }
            Action::TransportPositionAt {
                sample,
                after_frames,
            } => {
                if self.transport.playing && !self.transport.paused {
                    let delay_s = *after_frames as f64 / self.transport.playback_rate_hz.max(1.0);
                    self.transport.pending_transport_position =
                        Some((Instant::now() + Duration::from_secs_f64(delay_s), *sample));
                } else {
                    self.transport.transport_samples = *sample as f64;
                    self.transport.pending_transport_position = None;
                }
                true
            }
            Action::SetLoopEnabled(enabled) => {
                self.transport.loop_enabled =
                    *enabled && self.transport.loop_range_samples.is_some();
                self.transport.pending_transport_position = None;
                true
            }
            Action::SetLoopRange(range) => {
                self.transport.loop_range_samples = *range;
                self.transport.loop_enabled = range.is_some();
                self.transport.pending_transport_position = None;
                true
            }
            Action::SetPunchEnabled(enabled) => {
                self.transport.punch_enabled =
                    *enabled && self.transport.punch_range_samples.is_some();
                true
            }
            Action::SetPunchRange(range) => {
                self.transport.punch_range_samples = *range;
                self.transport.punch_enabled = range.is_some();
                true
            }
            Action::SetMetronomeEnabled(enabled) => {
                self.transport.metronome_enabled = *enabled;
                self.state
                    .write()
                    .expect("state lock poisoned")
                    .metronome_enabled = *enabled;
                true
            }
            Action::SetTempo(bpm) => {
                let bpm = (*bpm as f32).clamp(20.0, 300.0);
                let mut state = self.state.write().expect("state lock poisoned");
                let (base_bpm, _, _) = Self::timing_at_sample(&state, 0);
                state.tempo = base_bpm;
                self.timing.tempo_input = format!("{:.2}", bpm);
                self.timing.last_sent_tempo_bpm = Some(bpm as f64);
                true
            }
            Action::SetTimeSignature {
                numerator,
                denominator,
            } => {
                let mut state = self.state.write().expect("state lock poisoned");
                let incoming_num = (*numerator).clamp(1, 16) as u8;
                let incoming_den = match *denominator {
                    2 => 2,
                    4 => 4,
                    8 => 8,
                    16 => 16,
                    _ => 4,
                };
                let (_, base_num, base_den) = Self::timing_at_sample(&state, 0);
                state.time_signature_num = base_num;
                state.time_signature_denom = base_den;
                self.timing.time_signature_num_input = incoming_num.to_string();
                self.timing.time_signature_denom_input = incoming_den.to_string();
                self.timing.last_sent_time_signature =
                    Some((incoming_num as u16, incoming_den as u16));
                true
            }
            Action::SetTempoMap {
                tempo_points,
                time_signature_points,
            } => {
                let mut state = self.state.write().expect("state lock poisoned");
                state.tempo_points = tempo_points
                    .iter()
                    .map(|p| crate::state::TempoPoint {
                        sample: p.sample,
                        bpm: p.bpm as f32,
                    })
                    .collect();
                state.time_signature_points = time_signature_points
                    .iter()
                    .map(|p| crate::state::TimeSignaturePoint {
                        sample: p.sample,
                        numerator: p.numerator as u8,
                        denominator: p.denominator as u8,
                    })
                    .collect();
                let (base_bpm, base_num, base_den) = Self::timing_at_sample(&state, 0);
                state.tempo = base_bpm;
                state.time_signature_num = base_num;
                state.time_signature_denom = base_den;
                self.timing.tempo_input = format!("{:.2}", base_bpm);
                self.timing.time_signature_num_input = base_num.to_string();
                self.timing.time_signature_denom_input = base_den.to_string();
                self.timing.last_sent_tempo_bpm = Some(base_bpm as f64);
                self.timing.last_sent_time_signature = Some((base_num as u16, base_den as u16));
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::TrackAutomationRuntime;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn play_response_sets_transport_running_state() {
        let mut app = Maolan {
            transport: TransportUiState {
                paused: true,
                ..Default::default()
            },
            ..Maolan::default()
        };

        assert!(app.handle_response_timing_state_action(&Action::Play));

        assert!(app.transport.playing);
        assert!(!app.transport.paused);
        assert!(app.transport.last_playback_tick.is_some());
    }

    #[test]
    fn stop_response_clears_transport_and_automation_runtime_state() {
        let mut app = Maolan {
            transport: TransportUiState {
                playing: true,
                paused: true,
                ..Default::default()
            },
            rec: RecordingPreviewState {
                recording_preview_start_sample: Some(32),
                recording_preview_sample: Some(64),
                ..Default::default()
            },
            ..Maolan::default()
        };
        app.transport.last_playback_tick = Some(Instant::now());
        app.automation
            .track_automation_runtime
            .insert("Track".to_string(), TrackAutomationRuntime::default());
        app.automation
            .touch_automation_overrides
            .insert("Track".to_string(), HashMap::new());
        app.automation
            .touch_active_keys
            .insert("Track".to_string(), HashSet::new());
        app.automation
            .latch_automation_overrides
            .insert("Track".to_string(), HashMap::new());

        assert!(app.handle_response_timing_state_action(&Action::Stop));

        assert!(!app.transport.playing);
        assert!(!app.transport.paused);
        assert!(app.transport.last_playback_tick.is_none());
        assert!(app.automation.track_automation_runtime.is_empty());
        assert!(app.automation.touch_automation_overrides.is_empty());
        assert!(app.automation.touch_active_keys.is_empty());
        assert!(app.automation.latch_automation_overrides.is_empty());

        assert!(app.rec.recording_preview_start_sample.is_some());
        assert!(app.rec.recording_preview_sample.is_some());
    }

    #[test]
    fn transport_position_when_stopped_clears_recording_preview() {
        let mut app = Maolan {
            transport: TransportUiState {
                playing: false,
                ..Default::default()
            },
            rec: RecordingPreviewState {
                recording_preview_start_sample: Some(32),
                recording_preview_sample: Some(64),
                ..Default::default()
            },
            ..Maolan::default()
        };

        assert!(app.handle_response_timing_state_action(&Action::TransportPosition(0)));

        assert!(app.rec.recording_preview_start_sample.is_none());
        assert!(app.rec.recording_preview_sample.is_none());
    }

    #[test]
    fn stopped_transport_position_response_updates_visible_position() {
        let mut app = Maolan {
            transport: TransportUiState {
                transport_samples: 128.0,
                ..Default::default()
            },
            ..Maolan::default()
        };

        assert!(app.handle_response_timing_state_action(&Action::TransportPosition(0)));

        assert_eq!(app.transport.transport_samples, 0.0);
        assert!(app.transport.last_playback_tick.is_none());
    }
}
