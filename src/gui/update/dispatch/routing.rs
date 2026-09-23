use super::*;

impl Maolan {
    pub(super) fn handle_connections_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ConnectionViewSelectTrack(ref idx) => {
                let ctrl = self.state.read().expect("state lock poisoned").ctrl;
                let mut state = self.state.write().expect("state lock poisoned");

                match &mut state.connection_view_selection {
                    ConnectionViewSelection::Tracks(set) if ctrl => {
                        if set.contains(idx.as_str()) {
                            set.remove(idx.as_str());
                        } else {
                            set.insert(idx.clone());
                        }
                    }
                    _ => {
                        let mut set = std::collections::HashSet::new();
                        set.insert(idx.clone());
                        state.connection_view_selection = ConnectionViewSelection::Tracks(set);
                    }
                }
            }
            Message::ConnectionViewDeselectAll => {
                let mut state = self.state.write().expect("state lock poisoned");
                state.connection_view_selection = ConnectionViewSelection::None;
                state.plugin_graph_selected_connections.clear();
                state.plugin_graph_selected_connectable_connections.clear();
                state.plugin_graph_selected_plugins.clear();
            }
            Message::ConnectionPositionsChanged => {
                self.session_ops.has_unsaved_changes = true;
            }
            Message::ConnectionViewSelectConnection(idx) => {
                let ctrl = self.state.read().expect("state lock poisoned").ctrl;
                let mut state = self.state.write().expect("state lock poisoned");
                connections::selection::apply_track_connection_selection(&mut state, idx, ctrl);
            }
            Message::Connections => {
                let mut state = self.state.write().expect("state lock poisoned");
                state.view = View::Connections;
                state.connections_folder = None;
                state.connection_view_selection = ConnectionViewSelection::None;
            }
            Message::OpenTrackPlugins(ref track_name) => {
                {
                    let mut state = self.state.write().expect("state lock poisoned");
                    state.view = View::TrackPlugins;
                    state.plugin_graph_track = Some(track_name.clone());
                    state.plugin_graph_clip = None;
                    Self::reset_track_plugin_view_state(&mut state);
                    if let Some((plugins, connections)) =
                        state.plugin_graphs_by_track.get(track_name).cloned()
                    {
                        state.plugin_graph_plugins = plugins.clone();
                        state.plugin_graph_connections = connections;
                        let track_positions = state
                            .plugin_graph_plugin_positions
                            .entry(track_name.clone())
                            .or_default();
                        for (idx, plugin) in plugins.iter().enumerate() {
                            let fallback = Point::new(200.0 + idx as f32 * 180.0, 220.0);
                            track_positions
                                .entry(plugin.instance_id)
                                .or_insert(fallback);
                        }
                    }
                }
                return self.open_track_plugins_followup(track_name.clone());
            }
            Message::OpenFolderConnections(ref folder_name) => {
                let mut state = self.state.write().expect("state lock poisoned");
                state.view = View::Connections;
                state.connections_folder = Some(folder_name.clone());
                state.connection_view_selection = ConnectionViewSelection::None;
                state.message = format!("Opened folder connections: {}", folder_name);
                state.plugin_graph_track = Some(folder_name.clone());
                state.plugin_graph_clip = None;
                Self::reset_track_plugin_view_state(&mut state);
                if let Some((plugins, connections)) =
                    state.plugin_graphs_by_track.get(folder_name).cloned()
                {
                    state.plugin_graph_plugins = plugins.clone();
                    state.plugin_graph_connections = connections;
                    let track_positions = state
                        .plugin_graph_plugin_positions
                        .entry(folder_name.clone())
                        .or_default();
                    for (idx, plugin) in plugins.iter().enumerate() {
                        let fallback = Point::new(200.0 + idx as f32 * 180.0, 220.0);
                        track_positions
                            .entry(plugin.instance_id)
                            .or_insert(fallback);
                    }
                }

                let node_size = 140.0_f32;
                let spacing = 40.0_f32;
                let start_x = 80.0_f32;
                let folder_y = 80.0_f32;
                let children_y = folder_y + node_size + spacing;
                let canvas_w = self.size.width.max(600.0);
                let default_position = Point::new(100.0, 100.0);

                if let Some(folder_idx) = state.tracks.iter().position(|t| t.name == *folder_name)
                    && state.tracks[folder_idx].position == default_position
                {
                    state.tracks[folder_idx].position = Point::new(start_x, folder_y);

                    let child_names: Vec<String> = state
                        .tracks
                        .iter()
                        .filter(|t| t.parent_track.as_deref() == Some(folder_name.as_str()))
                        .map(|t| t.name.clone())
                        .collect();

                    if !child_names.is_empty() {
                        let cols = ((canvas_w - start_x * 2.0) / (node_size + spacing))
                            .floor()
                            .max(1.0) as usize;
                        let cols = cols.min(child_names.len());
                        for (i, name) in child_names.iter().enumerate() {
                            let col = i % cols;
                            let row = i / cols;
                            let x = start_x + col as f32 * (node_size + spacing);
                            let y = children_y + row as f32 * (node_size + spacing);
                            if let Some(child) = state.tracks.iter_mut().find(|t| t.name == *name)
                                && child.position == default_position
                            {
                                child.position = Point::new(x, y);
                            }
                        }
                    }
                }
                return self.send(Action::TrackGetPluginGraph {
                    track_name: folder_name.clone(),
                    include_state: false,
                });
            }
            Message::SessionViewConnectionsOpen(ref track_name) => {
                let mut state = self.state.write().expect("state lock poisoned");
                state.session_view_connections = Some(track_name.clone());
                state.connection_view_selection = ConnectionViewSelection::None;
                state.message = format!("Opened live view connections: {}", track_name);
                return self.load_track_connection_view(&mut state, track_name);
            }
            Message::SessionViewConnectionsClose => {
                let mut state = self.state.write().expect("state lock poisoned");
                state.session_view_connections = None;
                state.plugin_graph_track = None;
            }
            Message::EditorConnectionsOpen(ref track_name) => {
                let mut state = self.state.write().expect("state lock poisoned");
                state.editor_connections = Some(track_name.clone());
                state.connection_view_selection = ConnectionViewSelection::None;
                state.message = format!("Opened editor connections: {}", track_name);
                return self.load_track_connection_view(&mut state, track_name);
            }
            Message::EditorConnectionsClose => {
                let mut state = self.state.write().expect("state lock poisoned");
                state.editor_connections = None;
                state.plugin_graph_track = None;
            }
            Message::OpenJackConnections => {
                let mut state = self.state.write().expect("state lock poisoned");
                state.view = View::JackConnections;
                state.jack_connecting = None;
                drop(state);
                return self.send(Action::JackGetGraph);
            }
            Message::CloseJackConnections => {
                let mut state = self.state.write().expect("state lock poisoned");
                state.view = View::Connections;
                state.jack_connecting = None;
            }
            Message::JackPortClick { port, is_output } => {
                let mut state = self.state.write().expect("state lock poisoned");
                if is_output {
                    state.jack_connecting = Some(port);
                    state.message = "Select a JACK input port to connect".to_string();
                    return Task::none();
                }
                let Some(source) = state.jack_connecting.take() else {
                    state.message = "Select a JACK output port first".to_string();
                    return Task::none();
                };
                drop(state);
                return self.send(Action::JackConnect {
                    source,
                    destination: port,
                });
            }
            Message::JackDisconnect {
                source,
                destination,
            } => {
                return self.send(Action::JackDisconnect {
                    source,
                    destination,
                });
            }
            Message::OpenHwPorts { input } => {
                let mut state = self.state.write().expect("state lock poisoned");
                state.view = if input {
                    View::HwInputPorts
                } else {
                    View::HwOutputPorts
                };
            }
            _ => {}
        }
        self.update_children(&message);
        Task::none()
    }
}
