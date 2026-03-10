use super::*;

impl Maolan {
    pub(super) fn handle_track_selection_message(
        &mut self,
        message: Message,
    ) -> Option<Task<Message>> {
        match message {
            Message::ShiftPressed => {
                if !self.state.blocking_read().hw_loaded {
                    return Some(Task::none());
                }
                self.state.blocking_write().shift = true;
                None
            }
            Message::ShiftReleased => {
                if !self.state.blocking_read().hw_loaded {
                    return Some(Task::none());
                }
                self.state.blocking_write().shift = false;
                None
            }
            Message::CtrlPressed => {
                if !self.state.blocking_read().hw_loaded {
                    return Some(Task::none());
                }
                self.state.blocking_write().ctrl = true;
                None
            }
            Message::CtrlReleased => {
                if !self.state.blocking_read().hw_loaded {
                    return Some(Task::none());
                }
                self.state.blocking_write().ctrl = false;
                None
            }
            Message::SelectTrack(ref name) => {
                let now = Instant::now();
                let track_name = name.clone();
                let ctrl = self.state.blocking_read().ctrl;
                let mut state = self.state.blocking_write();
                state.track_context_menu = None;
                if ctrl {
                    state.connections_last_track_click = None;
                } else if let Some((last_track, last_time)) = &state.connections_last_track_click
                    && *last_track == track_name
                    && now.duration_since(*last_time) <= DOUBLE_CLICK.saturating_mul(2)
                {
                    state.connections_last_track_click = None;
                    return Some(Task::perform(async {}, move |_| {
                        Message::OpenTrackPlugins(track_name)
                    }));
                } else {
                    state.connections_last_track_click = Some((track_name.clone(), now));
                }

                if ctrl {
                    state.selected.insert(name.clone());
                    if let ConnectionViewSelection::Tracks(set) =
                        &mut state.connection_view_selection
                    {
                        set.insert(name.clone());
                    } else {
                        let mut set = std::collections::HashSet::new();
                        set.insert(name.clone());
                        state.connection_view_selection = ConnectionViewSelection::Tracks(set);
                    }
                } else {
                    state.selected.clear();
                    state.selected.insert(name.clone());
                    let mut set = std::collections::HashSet::new();
                    set.insert(name.clone());
                    state.connection_view_selection = ConnectionViewSelection::Tracks(set);
                }
                None
            }
            Message::SelectTrackFromMixer(ref name) => {
                let ctrl = self.state.blocking_read().ctrl;
                let mut state = self.state.blocking_write();
                state.track_context_menu = None;
                state.connections_last_track_click = None;

                if ctrl {
                    state.selected.insert(name.clone());
                    if let ConnectionViewSelection::Tracks(set) =
                        &mut state.connection_view_selection
                    {
                        set.insert(name.clone());
                    } else {
                        let mut set = std::collections::HashSet::new();
                        set.insert(name.clone());
                        state.connection_view_selection = ConnectionViewSelection::Tracks(set);
                    }
                } else {
                    state.selected.clear();
                    state.selected.insert(name.clone());
                    let mut set = std::collections::HashSet::new();
                    set.insert(name.clone());
                    state.connection_view_selection = ConnectionViewSelection::Tracks(set);
                }
                None
            }
            _ => None,
        }
    }
}
