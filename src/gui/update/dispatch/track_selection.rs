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
                if !self.state.read().expect("state lock poisoned").hw_loaded {
                    return Some(Task::none());
                }
                self.state.write().expect("state lock poisoned").shift = true;
                None
            }
            Message::ShiftReleased => {
                if !self.state.read().expect("state lock poisoned").hw_loaded {
                    return Some(Task::none());
                }
                self.state.write().expect("state lock poisoned").shift = false;
                None
            }
            Message::CtrlPressed => {
                if !self.state.read().expect("state lock poisoned").hw_loaded {
                    return Some(Task::none());
                }
                self.state.write().expect("state lock poisoned").ctrl = true;
                None
            }
            Message::CtrlReleased => {
                if !self.state.read().expect("state lock poisoned").hw_loaded {
                    return Some(Task::none());
                }
                self.state.write().expect("state lock poisoned").ctrl = false;
                None
            }
            Message::SelectTrack(ref name) => {
                let now = Instant::now();
                let track_name = name.clone();
                let shift = self.state.read().expect("state lock poisoned").shift;
                let ctrl = self.state.read().expect("state lock poisoned").ctrl;
                let mut state = self.state.write().expect("state lock poisoned");
                state.track_context_menu = None;
                if shift || ctrl {
                    state.connections_last_track_click = None;
                } else if let Some((last_track, last_time)) = &state.connections_last_track_click
                    && *last_track == track_name
                    && now.duration_since(*last_time) <= DOUBLE_CLICK.saturating_mul(2)
                {
                    state.connections_last_track_click = None;
                    if state.view == View::Workspace {
                        return Some(Task::perform(async {}, move |_| {
                            Message::EditorConnectionsOpen(track_name)
                        }));
                    }
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

                if state.view == View::Workspace && state.editor_connections.is_some() {
                    state.editor_connections = Some(name.clone());
                    return Some(self.load_track_connection_view(&mut state, name));
                }
                None
            }
            Message::SelectTrackFromMixer(ref name) => {
                let shift = self.state.read().expect("state lock poisoned").shift;
                let ctrl = self.state.read().expect("state lock poisoned").ctrl;
                let mut state = self.state.write().expect("state lock poisoned");
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

                if state.view == View::Workspace && state.editor_connections.is_some() {
                    state.editor_connections = Some(name.clone());
                    return Some(self.load_track_connection_view(&mut state, name));
                }
                if state.view == View::Session && state.session_view_connections.is_some() {
                    state.session_view_connections = Some(name.clone());
                    return Some(self.load_track_connection_view(&mut state, name));
                }
                None
            }
            _ => None,
        }
    }
}

impl Maolan {
    pub(super) fn handle_selection_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::DeselectAll => {
                let mut state = self.state.write().expect("state lock poisoned");
                state.selected.clear();
                state.selected_clips.clear();
                state.track_context_menu = None;
                state.connection_view_selection = ConnectionViewSelection::None;
                state.plugin_graph_selected_connectable_connections.clear();
            }
            Message::DeselectClips => {
                let mut state = self.state.write().expect("state lock poisoned");
                if state.clip_click_consumed {
                    state.clip_click_consumed = false;
                    return Task::none();
                }
                self.drag.clip = None;
                if self.modal.is_none() && matches!(state.view, View::Workspace) {
                    state.mouse_left_down = true;
                }
                state.mouse_right_down = false;
                state.clip_context_menu = None;
                state.track_context_menu = None;
                state.clip_marquee_start = None;
                state.clip_marquee_end = None;
                state.midi_clip_create_start = None;
                state.midi_clip_create_end = None;
                state.selected_clips.clear();
            }
            _ => {}
        }
        self.update_children(&message);
        Task::none()
    }
}
