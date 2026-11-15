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
                state.view = View::Workspace;
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
