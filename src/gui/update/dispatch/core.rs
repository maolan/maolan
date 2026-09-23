use super::*;

impl Maolan {
    pub(super) fn handle_core_message(&mut self, message: &Message) -> Option<Task<Message>> {
        match message {
            Message::None => Some(Task::none()),
            Message::Undo => Some(self.send(Action::Undo)),
            Message::Redo => Some(self.send(Action::Redo)),
            Message::ToggleTransport => {
                if !self.state.read().expect("state lock poisoned").hw_loaded {
                    return Some(Task::none());
                }
                if matches!(
                    self.state.read().expect("state lock poisoned").view,
                    crate::state::View::Session
                ) {
                    if self.transport.live_session_playing {
                        return Some(self.stop_live_session_play());
                    }
                    return Some(self.start_live_session_play());
                }
                if self.transport.playing && !self.transport.paused {
                    let stop_live = if self.transport.live_session_playing {
                        self.stop_live_session_play()
                    } else {
                        Task::none()
                    };
                    self.toolbar.update(message);
                    self.transport.playing = false;
                    self.transport.paused = false;
                    self.transport.last_playback_tick = None;
                    self.automation.track_automation_runtime.clear();
                    self.automation.touch_automation_overrides.clear();
                    self.automation.touch_active_keys.clear();
                    self.automation.latch_automation_overrides.clear();
                    self.rec.stop_recording_preview();
                    return Some(stop_live.chain(Task::batch(vec![
                        self.send(Action::SetClipPlaybackEnabled(true)),
                        self.send(Action::Stop),
                    ])));
                }
                // Toggling editor playback on while the live session is
                // playing hands over: live playback stops first.
                let stop_live = if self.transport.live_session_playing {
                    self.stop_live_session_play()
                } else {
                    Task::none()
                };
                let was_playing = self.transport.playing;
                self.toolbar.update(message);
                self.transport.playing = true;
                self.transport.paused = false;
                self.transport.last_playback_tick = Some(Instant::now());
                if self.transport.record_armed {
                    self.start_recording_preview();
                }
                let mut tasks = vec![self.send(Action::SetClipPlaybackEnabled(true))];
                if !was_playing {
                    tasks.push(self.send(Action::Play));
                }
                Some(stop_live.chain(Task::batch(tasks)))
            }
            Message::ToggleLoop => {
                if self.transport.loop_range_samples.is_none() {
                    return Some(Task::none());
                }
                let enabled = !self.transport.loop_enabled;
                self.transport.loop_enabled = enabled;
                Some(self.send(Action::SetLoopEnabled(enabled)))
            }
            Message::TogglePunch => {
                if self.transport.punch_range_samples.is_none() {
                    return Some(Task::none());
                }
                let enabled = !self.transport.punch_enabled;
                self.transport.punch_enabled = enabled;
                Some(self.send(Action::SetPunchEnabled(enabled)))
            }
            Message::ToggleMetronome => {
                self.transport.metronome_enabled = !self.transport.metronome_enabled;
                Some(self.send(Action::SetMetronomeEnabled(
                    self.transport.metronome_enabled,
                )))
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
        let initial = app.transport.metronome_enabled;
        let _result = app.handle_core_message(&Message::ToggleMetronome);
        assert_eq!(app.transport.metronome_enabled, !initial);
    }

    #[test]
    fn handle_window_resized_updates_size() {
        let mut app = Maolan::default();
        let new_size = maolan_widgets::iced::Size::new(1024.0, 768.0);
        let _result = app.handle_core_message(&Message::WindowResized(new_size));
        assert_eq!(app.size, new_size);
    }

    #[test]
    fn handle_toggle_loop_without_range_returns_task() {
        let mut app = Maolan::default();

        let result = app.handle_core_message(&Message::ToggleLoop);
        assert!(result.is_some());
    }

    #[test]
    fn handle_toggle_punch_without_range_returns_task() {
        let mut app = Maolan::default();

        let result = app.handle_core_message(&Message::TogglePunch);
        assert!(result.is_some());
    }
}
