use super::*;

impl Maolan {
    pub(super) fn handle_response_timing_state_action(&mut self, action: &Action) -> bool {
        match action {
            Action::BeginSessionRestore => {
                self.session_restore_in_progress = true;
                self.has_unsaved_changes = false;
                self.last_autosave_snapshot = None;
                self.pending_autosave_recovery = None;
                self.pending_open_session_dir = None;
                true
            }
            Action::EndSessionRestore => {
                self.session_restore_in_progress = false;
                self.has_unsaved_changes = false;
                self.last_autosave_snapshot = None;
                self.pending_autosave_recovery = None;
                self.pending_open_session_dir = None;
                true
            }
            Action::TransportPosition(sample) => {
                // While paused/stopped we treat UI transport position as user-driven.
                // Ignore late engine transport echoes to prevent pause->stop jump-forward.
                if self.playing && !self.paused {
                    self.transport_samples = *sample as f64;
                    self.last_playback_tick = Some(Instant::now());
                }
                true
            }
            Action::SetLoopEnabled(enabled) => {
                self.loop_enabled = *enabled && self.loop_range_samples.is_some();
                true
            }
            Action::SetLoopRange(range) => {
                self.loop_range_samples = *range;
                self.loop_enabled = range.is_some();
                true
            }
            Action::SetPunchEnabled(enabled) => {
                self.punch_enabled = *enabled && self.punch_range_samples.is_some();
                true
            }
            Action::SetPunchRange(range) => {
                self.punch_range_samples = *range;
                self.punch_enabled = range.is_some();
                true
            }
            Action::SetMetronomeEnabled(enabled) => {
                self.metronome_enabled = *enabled;
                self.state.blocking_write().metronome_enabled = *enabled;
                true
            }
            Action::SetTempo(bpm) => {
                let bpm = (*bpm as f32).clamp(20.0, 300.0);
                let mut state = self.state.blocking_write();
                let (base_bpm, _, _) = Self::timing_at_sample(&state, 0);
                state.tempo = base_bpm;
                self.tempo_input = format!("{:.2}", bpm);
                self.last_sent_tempo_bpm = Some(bpm as f64);
                true
            }
            Action::SetTimeSignature {
                numerator,
                denominator,
            } => {
                let mut state = self.state.blocking_write();
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
                self.time_signature_num_input = incoming_num.to_string();
                self.time_signature_denom_input = incoming_den.to_string();
                self.last_sent_time_signature = Some((
                    incoming_num as u16,
                    incoming_den as u16,
                ));
                true
            }
            _ => false,
        }
    }
}
