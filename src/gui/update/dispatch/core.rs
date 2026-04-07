use super::*;

impl Maolan {
    pub(super) fn handle_core_message(&mut self, message: &Message) -> Option<Task<Message>> {
        match message {
            Message::None => Some(Task::none()),
            Message::Undo => {
                if matches!(
                    self.state.blocking_read().view,
                    crate::state::View::PitchCorrection
                ) {
                    return Some(self.undo_pitch_correction_edit());
                }
                Some(self.send(Action::Undo))
            }
            Message::Redo => {
                if matches!(
                    self.state.blocking_read().view,
                    crate::state::View::PitchCorrection
                ) {
                    return Some(self.redo_pitch_correction_edit());
                }
                Some(self.send(Action::Redo))
            }
            Message::ToggleTransport => {
                if !self.state.blocking_read().hw_loaded {
                    return Some(Task::none());
                }
                if self.playing && !self.paused {
                    self.toolbar.update(message);
                    self.playing = false;
                    self.paused = false;
                    self.last_playback_tick = None;
                    self.track_automation_runtime.clear();
                    self.touch_automation_overrides.clear();
                    self.touch_active_keys.clear();
                    self.latch_automation_overrides.clear();
                    self.stop_recording_preview();
                    return Some(Task::batch(vec![
                        self.send(Action::SetClipPlaybackEnabled(true)),
                        self.send(Action::Stop),
                    ]));
                }
                let was_playing = self.playing;
                self.toolbar.update(message);
                self.playing = true;
                self.paused = false;
                self.last_playback_tick = Some(Instant::now());
                if self.record_armed {
                    self.start_recording_preview();
                }
                let mut tasks = vec![self.send(Action::SetClipPlaybackEnabled(true))];
                if !was_playing {
                    tasks.push(self.send(Action::Play));
                }
                Some(Task::batch(tasks))
            }
            Message::ToggleLoop => {
                if self.loop_range_samples.is_none() {
                    return Some(Task::none());
                }
                let enabled = !self.loop_enabled;
                self.loop_enabled = enabled;
                Some(self.send(Action::SetLoopEnabled(enabled)))
            }
            Message::TogglePunch => {
                if self.punch_range_samples.is_none() {
                    return Some(Task::none());
                }
                let enabled = !self.punch_enabled;
                self.punch_enabled = enabled;
                Some(self.send(Action::SetPunchEnabled(enabled)))
            }
            Message::ToggleMetronome => {
                self.metronome_enabled = !self.metronome_enabled;
                Some(self.send(Action::SetMetronomeEnabled(self.metronome_enabled)))
            }
            Message::WindowResized(size) => {
                self.size = *size;
                Some(self.sync_editor_scrollbars())
            }
            Message::WindowCloseRequested => Some(self.request_window_close()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_none_message_returns_none_task() {
        let mut app = Maolan::default();
        let result = app.handle_core_message(&Message::None);
        assert!(result.is_some());
    }

    #[test]
    fn handle_undo_message_sends_undo_action() {
        let mut app = Maolan::default();
        let result = app.handle_core_message(&Message::Undo);
        assert!(result.is_some());
    }

    #[test]
    fn handle_redo_message_sends_redo_action() {
        let mut app = Maolan::default();
        let result = app.handle_core_message(&Message::Redo);
        assert!(result.is_some());
    }

    #[test]
    fn handle_toggle_metronome_toggles_state() {
        let mut app = Maolan::default();
        let initial = app.metronome_enabled;
        let _result = app.handle_core_message(&Message::ToggleMetronome);
        assert_eq!(app.metronome_enabled, !initial);
    }

    #[test]
    fn handle_window_resized_updates_size() {
        let mut app = Maolan::default();
        let new_size = iced::Size::new(1024.0, 768.0);
        let _result = app.handle_core_message(&Message::WindowResized(new_size));
        assert_eq!(app.size, new_size);
    }

    #[test]
    fn handle_toggle_loop_without_range_returns_task() {
        let mut app = Maolan::default();
        // loop_range_samples is None by default
        let result = app.handle_core_message(&Message::ToggleLoop);
        assert!(result.is_some());
    }

    #[test]
    fn handle_toggle_punch_without_range_returns_task() {
        let mut app = Maolan::default();
        // punch_range_samples is None by default
        let result = app.handle_core_message(&Message::TogglePunch);
        assert!(result.is_some());
    }
}
