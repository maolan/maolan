use super::*;
use crate::state::StateData;

fn select_track_range(state: &mut StateData, from: &str, to: &str) {
    let from_index = state.tracks.iter().position(|t| t.name == from);
    let to_index = state.tracks.iter().position(|t| t.name == to);
    if let (Some(from_idx), Some(to_idx)) = (from_index, to_index) {
        let start = from_idx.min(to_idx);
        let end = from_idx.max(to_idx);
        let names: Vec<String> = state.tracks[start..=end]
            .iter()
            .map(|t| t.name.clone())
            .collect();
        let mut set = match &state.connection_view_selection {
            ConnectionViewSelection::Tracks(existing) => existing.clone(),
            _ => std::collections::HashSet::new(),
        };
        for name in names {
            state.selected.insert(name.clone());
            set.insert(name);
        }
        state.connection_view_selection = ConnectionViewSelection::Tracks(set);
    }
}

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
                let shift = self.state.blocking_read().shift;
                let ctrl = self.state.blocking_read().ctrl;
                let mut state = self.state.blocking_write();
                state.track_context_menu = None;
                if shift || ctrl {
                    state.connections_last_track_click = None;
                } else if let Some((last_track, last_time)) = &state.connections_last_track_click
                    && *last_track == track_name
                    && now.duration_since(*last_time) <= DOUBLE_CLICK.saturating_mul(2)
                {
                    state.connections_last_track_click = None;
                    let is_folder = state
                        .tracks
                        .iter()
                        .find(|t| t.name == track_name)
                        .map(|t| t.is_folder)
                        .unwrap_or(false);
                    return Some(Task::perform(async {}, move |_| {
                        if is_folder {
                            Message::OpenFolderConnections(track_name)
                        } else {
                            Message::OpenTrackPlugins(track_name)
                        }
                    }));
                } else {
                    state.connections_last_track_click = Some((track_name.clone(), now));
                }

                if shift {
                    let anchor = state
                        .last_selected_track
                        .as_ref()
                        .filter(|anchor| state.tracks.iter().any(|t| t.name == **anchor))
                        .cloned();
                    if let Some(anchor) = anchor {
                        if !ctrl {
                            state.selected.clear();
                            state.connection_view_selection = ConnectionViewSelection::None;
                        }
                        select_track_range(&mut state, &anchor, name);
                    } else if ctrl {
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
                        state.last_selected_track = Some(name.clone());
                        state.selected.clear();
                        state.selected.insert(name.clone());
                        let mut set = std::collections::HashSet::new();
                        set.insert(name.clone());
                        state.connection_view_selection = ConnectionViewSelection::Tracks(set);
                    }
                } else if ctrl {
                    state.last_selected_track = Some(name.clone());
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
                    state.last_selected_track = Some(name.clone());
                    state.selected.clear();
                    state.selected.insert(name.clone());
                    let mut set = std::collections::HashSet::new();
                    set.insert(name.clone());
                    state.connection_view_selection = ConnectionViewSelection::Tracks(set);
                }
                None
            }
            Message::SelectTrackFromMixer(ref name) => {
                let shift = self.state.blocking_read().shift;
                let ctrl = self.state.blocking_read().ctrl;
                let mut state = self.state.blocking_write();
                state.track_context_menu = None;
                state.connections_last_track_click = None;

                if shift {
                    let anchor = state
                        .last_selected_track
                        .as_ref()
                        .filter(|anchor| state.tracks.iter().any(|t| t.name == **anchor))
                        .cloned();
                    if let Some(anchor) = anchor {
                        if !ctrl {
                            state.selected.clear();
                            state.connection_view_selection = ConnectionViewSelection::None;
                        }
                        select_track_range(&mut state, &anchor, name);
                    } else if ctrl {
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
                        state.last_selected_track = Some(name.clone());
                        state.selected.clear();
                        state.selected.insert(name.clone());
                        let mut set = std::collections::HashSet::new();
                        set.insert(name.clone());
                        state.connection_view_selection = ConnectionViewSelection::Tracks(set);
                    }
                } else if ctrl {
                    state.last_selected_track = Some(name.clone());
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
                    state.last_selected_track = Some(name.clone());
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
