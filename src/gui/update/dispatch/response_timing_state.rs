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
                self.transport_samples = *sample as f64;
                if self.playing && !self.paused {
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
            Action::SetTempo(bpm) => {
                let bpm = (*bpm as f32).clamp(20.0, 300.0);
                self.state.blocking_write().tempo = bpm;
                self.tempo_input = format!("{:.2}", bpm);
                self.last_sent_tempo_bpm = Some(bpm as f64);
                true
            }
            Action::SetTimeSignature {
                numerator,
                denominator,
            } => {
                let mut state = self.state.blocking_write();
                state.time_signature_num = (*numerator).clamp(1, 16) as u8;
                state.time_signature_denom = match *denominator {
                    2 => 2,
                    4 => 4,
                    8 => 8,
                    16 => 16,
                    _ => 4,
                };
                self.time_signature_num_input = state.time_signature_num.to_string();
                self.time_signature_denom_input = state.time_signature_denom.to_string();
                self.last_sent_time_signature = Some((
                    state.time_signature_num as u16,
                    state.time_signature_denom as u16,
                ));
                true
            }
            _ => false,
        }
    }
}
