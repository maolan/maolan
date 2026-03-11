use super::*;

impl Maolan {
    pub(super) fn handle_core_message(&mut self, message: &Message) -> Option<Task<Message>> {
        match message {
            Message::None => Some(Task::none()),
            Message::Undo => Some(self.send(Action::Undo)),
            Message::Redo => Some(self.send(Action::Redo)),
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
            Message::WindowCloseRequested => {
                if self.has_unsaved_changes && !self.close_confirm_pending {
                    self.close_confirm_pending = true;
                    self.state.blocking_write().message =
                        "Unsaved changes detected. Close again to discard, or save the session."
                            .to_string();
                    return Some(Task::none());
                }
                exit(0);
            }
            _ => None,
        }
    }
}
