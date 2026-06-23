use super::*;

impl Maolan {
    pub(super) fn handle_plugin_message(&mut self, message: Message) -> Option<Task<Message>> {
        match message {
            #[cfg(all(unix, not(target_os = "macos")))]
            Message::RefreshLv2Plugins => Some(self.send(Action::ListLv2Plugins)),
            Message::RefreshVst3Plugins => Some(self.send(Action::ListVst3Plugins)),
            Message::RefreshClapPlugins => Some(self.send(Action::ListClapPlugins)),
            #[cfg(all(unix, not(target_os = "macos")))]
            Message::FilterLv2Plugins(ref query) => {
                self.plugin_filter = query.clone();
                None
            }
            Message::FilterVst3Plugins(ref query) => {
                self.vst3_plugin_filter = query.clone();
                None
            }
            Message::FilterClapPlugin(ref query) => {
                self.clap_plugin_filter = query.clone();
                None
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            Message::SelectLv2Plugin(ref plugin_uri) => {
                if self.selected_lv2_plugins.contains(plugin_uri) {
                    self.selected_lv2_plugins.remove(plugin_uri);
                } else {
                    self.selected_lv2_plugins.insert(plugin_uri.clone());
                }
                None
            }
            Message::SelectVst3Plugin(ref plugin_path) => {
                if self.selected_vst3_plugins.contains(plugin_path) {
                    self.selected_vst3_plugins.remove(plugin_path);
                } else {
                    self.selected_vst3_plugins.insert(plugin_path.clone());
                }
                None
            }
            Message::SelectClapPlugin(ref plugin_path) => {
                if self.selected_clap_plugins.contains(plugin_path) {
                    self.selected_clap_plugins.remove(plugin_path);
                } else {
                    self.selected_clap_plugins.insert(plugin_path.clone());
                }
                None
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            Message::LoadSelectedLv2Plugins => {
                let clip_target = {
                    let state = self.state.blocking_read();
                    state.plugin_graph_clip.clone()
                };
                if clip_target.is_some() {
                    let selected = self
                        .selected_lv2_plugins
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>();
                    self.selected_lv2_plugins.clear();
                    self.modal = None;
                    let mut state = self.state.blocking_write();
                    let mut next_id = state
                        .plugin_graph_plugins
                        .iter()
                        .map(|plugin| plugin.instance_id)
                        .max()
                        .map(|id| id.saturating_add(1))
                        .unwrap_or(0);
                    let plugin_infos = state.lv2_plugins.clone();
                    for plugin_uri in selected {
                        if let Some(info) = plugin_infos.iter().find(|info| info.uri == plugin_uri)
                        {
                            state.plugin_graph_plugins.push(
                                maolan_engine::message::PluginGraphPlugin {
                                    node:
                                        maolan_engine::message::PluginGraphNode::Lv2PluginInstance(
                                            next_id,
                                        ),
                                    instance_id: next_id,
                                    format: "LV2".to_string(),
                                    uri: info.uri.clone(),
                                    plugin_id: info.uri.clone(),
                                    name: info.name.clone(),
                                    main_audio_inputs: info.audio_inputs,
                                    main_audio_outputs: info.audio_outputs,
                                    audio_inputs: info.audio_inputs,
                                    audio_outputs: info.audio_outputs,
                                    midi_inputs: info.midi_inputs,
                                    midi_outputs: info.midi_outputs,
                                    state: None,
                                    bypassed: false,
                                },
                            );
                            next_id = next_id.saturating_add(1);
                        }
                    }
                    let sync = Self::save_open_clip_plugin_graph(&mut state);
                    return Some(sync.map_or_else(Task::none, |action| self.send(action)));
                }
                let track_name = {
                    let state = self.state.blocking_read();
                    state
                        .plugin_graph_track
                        .clone()
                        .or_else(|| state.selected.iter().next().cloned())
                };
                if let Some(track_name) = track_name {
                    let tasks: Vec<Task<Message>> = self
                        .selected_lv2_plugins
                        .iter()
                        .cloned()
                        .map(|plugin_uri| {
                            self.send(Action::TrackLoadLv2Plugin {
                                track_name: track_name.clone(),
                                plugin_uri,
                                instance_id: None,
                            })
                        })
                        .collect();
                    self.selected_lv2_plugins.clear();
                    self.modal = None;
                    return Some(Task::batch(tasks));
                }
                self.state.blocking_write().message =
                    "Select a track before loading LV2 plugin".to_string();
                None
            }
            Message::LoadSelectedVst3Plugins => {
                let clip_target = {
                    let state = self.state.blocking_read();
                    state.plugin_graph_clip.clone()
                };
                if clip_target.is_some() {
                    let selected = self
                        .selected_vst3_plugins
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>();
                    self.selected_vst3_plugins.clear();
                    self.modal = None;
                    #[cfg(all(unix, not(target_os = "macos")))]
                    {
                        let mut state = self.state.blocking_write();
                        let mut next_id = state
                            .plugin_graph_plugins
                            .iter()
                            .map(|plugin| plugin.instance_id)
                            .max()
                            .map(|id| id.saturating_add(1))
                            .unwrap_or(0);
                        let plugin_infos = state.vst3_plugins.clone();
                        for plugin_path in selected {
                            if let Some(info) =
                                plugin_infos.iter().find(|info| info.path == plugin_path)
                            {
                                state
                                    .plugin_graph_plugins
                                    .push(maolan_engine::message::PluginGraphPlugin {
                                    node:
                                        maolan_engine::message::PluginGraphNode::Vst3PluginInstance(
                                            next_id,
                                        ),
                                    instance_id: next_id,
                                    format: "VST3".to_string(),
                                    uri: info.path.clone(),
                                    plugin_id: info.id.clone(),
                                    name: info.name.clone(),
                                    main_audio_inputs: info.audio_inputs,
                                    main_audio_outputs: info.audio_outputs,
                                    audio_inputs: info.audio_inputs,
                                    audio_outputs: info.audio_outputs,
                                    midi_inputs: usize::from(info.has_midi_input),
                                    midi_outputs: usize::from(info.has_midi_output),
                                    state: None,
                                    bypassed: false,
                                });
                                next_id = next_id.saturating_add(1);
                            }
                        }
                        let sync = Self::save_open_clip_plugin_graph(&mut state);
                        return Some(sync.map_or_else(Task::none, |action| self.send(action)));
                    }
                    #[cfg(not(all(unix, not(target_os = "macos"))))]
                    {
                        let _ = selected;
                    }
                }
                let track_name = {
                    let state = self.state.blocking_read();
                    state
                        .plugin_graph_track
                        .clone()
                        .or_else(|| state.selected.iter().next().cloned())
                };
                if let Some(track_name) = track_name {
                    let tasks: Vec<Task<Message>> = self
                        .selected_vst3_plugins
                        .iter()
                        .cloned()
                        .map(|plugin_path| {
                            self.send(Action::TrackLoadVst3Plugin {
                                track_name: track_name.clone(),
                                plugin_path,
                                instance_id: None,
                            })
                        })
                        .collect();
                    self.selected_vst3_plugins.clear();
                    self.modal = None;
                    return Some(Task::batch(tasks));
                }
                self.state.blocking_write().message =
                    "Select a track before loading VST3 plugin".to_string();
                None
            }
            Message::LoadSelectedClapPlugins => {
                let clip_target = {
                    let state = self.state.blocking_read();
                    state.plugin_graph_clip.clone()
                };
                if clip_target.is_some() {
                    let selected = self
                        .selected_clap_plugins
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>();
                    self.selected_clap_plugins.clear();
                    self.modal = None;
                    #[cfg(all(unix, not(target_os = "macos")))]
                    {
                        let mut state = self.state.blocking_write();
                        let mut next_id = state
                            .plugin_graph_plugins
                            .iter()
                            .map(|plugin| plugin.instance_id)
                            .max()
                            .map(|id| id.saturating_add(1))
                            .unwrap_or(0);
                        let plugin_infos = state.clap_plugins.clone();
                        for plugin_path in selected {
                            if let Some(info) =
                                plugin_infos.iter().find(|info| info.path == plugin_path)
                            {
                                let caps = info.capabilities.as_ref();
                                state
                                    .plugin_graph_plugins
                                    .push(maolan_engine::message::PluginGraphPlugin {
                                    node:
                                        maolan_engine::message::PluginGraphNode::ClapPluginInstance(
                                            next_id,
                                        ),
                                    instance_id: next_id,
                                    format: "CLAP".to_string(),
                                    uri: info.path.clone(),
                                    plugin_id: info
                                        .path
                                        .split_once("::")
                                        .map(|(_, id)| id.to_string())
                                        .unwrap_or_default(),
                                    name: info.name.clone(),
                                    main_audio_inputs: caps
                                        .map(|caps| caps.audio_inputs)
                                        .unwrap_or(0),
                                    main_audio_outputs: caps
                                        .map(|caps| caps.audio_outputs)
                                        .unwrap_or(0),
                                    audio_inputs: caps.map(|caps| caps.audio_inputs).unwrap_or(0),
                                    audio_outputs: caps.map(|caps| caps.audio_outputs).unwrap_or(0),
                                    midi_inputs: caps.map(|caps| caps.midi_inputs).unwrap_or(0),
                                    midi_outputs: caps.map(|caps| caps.midi_outputs).unwrap_or(0),
                                    state: None,
                                    bypassed: false,
                                });
                                next_id = next_id.saturating_add(1);
                            }
                        }
                        let sync = Self::save_open_clip_plugin_graph(&mut state);
                        return Some(sync.map_or_else(Task::none, |action| self.send(action)));
                    }
                    #[cfg(not(all(unix, not(target_os = "macos"))))]
                    {
                        let _ = selected;
                    }
                }
                let track_name = {
                    let state = self.state.blocking_read();
                    state
                        .plugin_graph_track
                        .clone()
                        .or_else(|| state.selected.iter().next().cloned())
                };
                if let Some(track_name) = track_name {
                    let tasks: Vec<Task<Message>> = self
                        .selected_clap_plugins
                        .iter()
                        .cloned()
                        .map(|plugin_path| {
                            self.send(Action::TrackLoadClapPlugin {
                                track_name: track_name.clone(),
                                plugin_path,
                                instance_id: None,
                            })
                        })
                        .collect();
                    self.selected_clap_plugins.clear();
                    self.modal = None;
                    return Some(Task::batch(tasks));
                }
                self.state.blocking_write().message =
                    "Select a track before loading CLAP plugin".to_string();
                None
            }
            Message::PluginFormatSelected(format) => {
                #[cfg(target_os = "macos")]
                let format = if format == PluginFormat::Lv2 {
                    PluginFormat::Vst3
                } else {
                    format
                };
                self.plugin_format = format;
                None
            }
            Message::ShowClapPluginUi {
                ref track_name,
                clip_idx: _,
                instance_id,
                plugin_path: _,
            } => {
                if self.session_restore_in_progress {
                    self.state.blocking_write().message =
                        "Plugin UI will be available after session restore finishes".to_string();
                    return Some(self.open_track_plugins_followup(track_name.clone()));
                }
                tracing::info!(%track_name, instance_id, "DAW requesting CLAP UI");
                self.info(format!(
                    "Requesting CLAP UI for track '{}' instance {}",
                    track_name, instance_id
                ));
                Some(self.send(Action::TrackShowClapGui {
                    track_name: track_name.clone(),
                    instance_id,
                }))
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            Message::OpenLv2PluginUi {
                ref track_name,
                clip_idx: _,
                instance_id,
            } => {
                if self.session_restore_in_progress {
                    self.state.blocking_write().message =
                        "Plugin UI will be available after session restore finishes".to_string();
                    return Some(self.open_track_plugins_followup(track_name.clone()));
                }
                Some(self.send(Action::TrackShowLv2Gui {
                    track_name: track_name.clone(),
                    instance_id,
                }))
            }
            Message::ClipConnectPlugin {
                ref from_node,
                from_port,
                ref to_node,
                to_port,
                kind,
            } => {
                let mut state = self.state.blocking_write();
                state.plugin_graph_clip.as_ref()?;
                if from_node == to_node && from_port == to_port {
                    state.message = "Cannot connect a plugin port to itself".to_string();
                    return None;
                }
                let connection = maolan_engine::message::PluginGraphConnection {
                    from_node: from_node.clone(),
                    from_port,
                    to_node: to_node.clone(),
                    to_port,
                    kind,
                };
                if !state
                    .plugin_graph_connections
                    .iter()
                    .any(|existing| existing == &connection)
                {
                    state.plugin_graph_connections.push(connection);
                    let sync = Self::save_open_clip_plugin_graph(&mut state);
                    return sync.map(|action| self.send(action));
                }
                None
            }
            Message::ClipConnectPlugins(connections) => {
                let mut state = self.state.blocking_write();
                state.plugin_graph_clip.as_ref()?;
                let mut added = false;
                for connection in connections {
                    if connection.from_node == connection.to_node
                        && connection.from_port == connection.to_port
                    {
                        continue;
                    }
                    if !state
                        .plugin_graph_connections
                        .iter()
                        .any(|existing| existing == &connection)
                    {
                        state.plugin_graph_connections.push(connection);
                        added = true;
                    }
                }
                if added {
                    let sync = Self::save_open_clip_plugin_graph(&mut state);
                    return sync.map(|action| self.send(action));
                }
                None
            }
            Message::OpenVst3PluginUi {
                ref track_name,
                clip_idx: _,
                instance_id,
                plugin_path: _,
            } => {
                if self.session_restore_in_progress {
                    self.state.blocking_write().message =
                        "Plugin UI will be available after session restore finishes".to_string();
                    return Some(self.open_track_plugins_followup(track_name.clone()));
                }
                self.info(format!(
                    "Requesting VST3 UI for track '{}' instance {}",
                    track_name, instance_id
                ));
                Some(self.send(Action::TrackShowVst3Gui {
                    track_name: track_name.clone(),
                    instance_id,
                }))
            }
            Message::SendMessageFinished(Err(_e)) => None,
            Message::SendMessageFinished(Ok(())) => None,
            _ => None,
        }
    }
}
