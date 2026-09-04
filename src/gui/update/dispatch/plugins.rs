use super::*;

impl Maolan {
    fn plugin_display_name(
        state: &crate::state::StateData,
        instance_id: usize,
        format: &str,
        plugin_id: &str,
    ) -> String {
        state
            .plugin_graph_plugins
            .iter()
            .find(|plugin| plugin.instance_id == instance_id)
            .map(|plugin| plugin.name.clone())
            .or_else(|| {
                if format.eq_ignore_ascii_case("CLAP") {
                    state
                        .clap_plugins
                        .iter()
                        .find(|plugin| plugin.id == plugin_id)
                        .map(|plugin| plugin.name.clone())
                } else if format.eq_ignore_ascii_case("VST3") {
                    state
                        .vst3_plugins
                        .iter()
                        .find(|plugin| plugin.id == plugin_id)
                        .map(|plugin| plugin.name.clone())
                } else if format.eq_ignore_ascii_case("LV2") {
                    state
                        .lv2_plugins
                        .iter()
                        .find(|plugin| plugin.uri == plugin_id)
                        .map(|plugin| plugin.name.clone())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| plugin_id.to_string())
    }

    pub(super) fn open_generic_plugin_ui(
        &mut self,
        track_name: String,
        clip_idx: Option<usize>,
        instance_id: usize,
        format: String,
        plugin_id: String,
    ) -> Task<Message> {
        let (name, cached) = {
            let state = self.state.blocking_read();
            let name = Self::plugin_display_name(&state, instance_id, &format, &plugin_id);
            let cached = if let Some(clip_idx) = clip_idx {
                state
                    .plugin_parameters_by_clip
                    .get(&(track_name.clone(), clip_idx))
                    .and_then(|cache| cache.get(&instance_id))
                    .is_some()
            } else {
                state
                    .plugin_parameters_by_track
                    .get(&track_name)
                    .and_then(|cache| cache.get(&instance_id))
                    .is_some()
            };
            (name, cached)
        };
        self.modal = Some(Show::GenericPluginView {
            track_name: track_name.clone(),
            clip_idx,
            instance_id,
            format: format.clone(),
            plugin_id,
            name,
        });
        if cached {
            return Task::none();
        }
        match (format.as_str(), clip_idx) {
            (format, Some(clip_idx)) if format.eq_ignore_ascii_case("CLAP") => {
                self.send(Action::ClipGetClapParameters {
                    track_name,
                    clip_idx,
                    instance_id,
                })
            }
            (format, Some(clip_idx)) if format.eq_ignore_ascii_case("VST3") => {
                self.send(Action::ClipGetVst3Parameters {
                    track_name,
                    clip_idx,
                    instance_id,
                })
            }
            #[cfg(unix)]
            (format, Some(clip_idx)) if format.eq_ignore_ascii_case("LV2") => {
                self.send(Action::ClipGetLv2PluginControls {
                    track_name,
                    clip_idx,
                    instance_id,
                })
            }
            (format, None) if format.eq_ignore_ascii_case("CLAP") => {
                self.send(Action::TrackGetClapParameters {
                    track_name,
                    instance_id,
                })
            }
            (format, None) if format.eq_ignore_ascii_case("VST3") => {
                self.send(Action::TrackGetVst3Parameters {
                    track_name,
                    instance_id,
                })
            }
            #[cfg(unix)]
            (format, None) if format.eq_ignore_ascii_case("LV2") => {
                self.send(Action::TrackGetLv2PluginControls {
                    track_name,
                    instance_id,
                })
            }
            _ => Task::none(),
        }
    }

    pub(super) fn handle_plugin_message(&mut self, message: Message) -> Option<Task<Message>> {
        match message {
            #[cfg(unix)]
            Message::RefreshLv2Plugins => Some(self.send(Action::ListLv2Plugins)),
            Message::RefreshVst3Plugins => Some(self.send(Action::ListVst3Plugins)),
            Message::RefreshClapPlugins => Some(self.send(Action::ListClapPlugins)),
            Message::FilterPluginList(ref query) => {
                self.plugin_list_filter = query.clone();
                None
            }
            #[cfg(unix)]
            Message::SelectLv2Plugin(ref plugin_uri) => {
                if self.selected_lv2_plugins.contains(plugin_uri) {
                    self.selected_lv2_plugins.remove(plugin_uri);
                } else {
                    self.selected_lv2_plugins.insert(plugin_uri.clone());
                }
                None
            }
            Message::SelectVst3Plugin(ref plugin_id) => {
                if self.selected_vst3_plugins.contains(plugin_id) {
                    self.selected_vst3_plugins.remove(plugin_id);
                } else {
                    self.selected_vst3_plugins.insert(plugin_id.clone());
                }
                None
            }
            Message::SelectClapPlugin(ref plugin_id) => {
                if self.selected_clap_plugins.contains(plugin_id) {
                    self.selected_clap_plugins.remove(plugin_id);
                } else {
                    self.selected_clap_plugins.insert(plugin_id.clone());
                }
                None
            }
            #[cfg(unix)]
            Message::LoadSelectedPlugins => {
                let (clip_target, track_name) = {
                    let state = self.state.blocking_read();
                    (
                        state.plugin_graph_clip.clone(),
                        state
                            .plugin_graph_track
                            .clone()
                            .or_else(|| state.selected.iter().next().cloned()),
                    )
                };

                if clip_target.is_some() {
                    #[cfg(unix)]
                    let lv2_selected = self
                        .selected_lv2_plugins
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>();
                    let clap_selected = self
                        .selected_clap_plugins
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>();
                    let vst3_selected = self
                        .selected_vst3_plugins
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>();
                    #[cfg(unix)]
                    self.selected_lv2_plugins.clear();
                    self.selected_clap_plugins.clear();
                    self.selected_vst3_plugins.clear();
                    self.modal = None;

                    let mut state = self.state.blocking_write();
                    let mut next_id = state
                        .plugin_graph_plugins
                        .iter()
                        .map(|plugin| plugin.instance_id)
                        .max()
                        .map(|id| id.saturating_add(1))
                        .unwrap_or(0);
                    #[cfg(unix)]
                    {
                        let plugin_infos = state.lv2_plugins.clone();
                        for plugin_uri in lv2_selected {
                            if let Some(info) =
                                plugin_infos.iter().find(|info| info.uri == plugin_uri)
                            {
                                state
                                    .plugin_graph_plugins
                                    .push(maolan_engine::message::PluginGraphPlugin {
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
                                });
                                next_id = next_id.saturating_add(1);
                            }
                        }
                    }
                    let plugin_infos = state.clap_plugins.clone();
                    for plugin_id in clap_selected {
                        if let Some(info) = plugin_infos.iter().find(|info| info.id == plugin_id) {
                            let caps = info.capabilities.as_ref();
                            state.plugin_graph_plugins.push(
                                maolan_engine::message::PluginGraphPlugin {
                                    node:
                                        maolan_engine::message::PluginGraphNode::ClapPluginInstance(
                                            next_id,
                                        ),
                                    instance_id: next_id,
                                    format: "CLAP".to_string(),
                                    uri: info.path.clone(),
                                    plugin_id: info.id.clone(),
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
                                },
                            );
                            next_id = next_id.saturating_add(1);
                        }
                    }
                    let plugin_infos = state.vst3_plugins.clone();
                    for plugin_id in vst3_selected {
                        if let Some(info) = plugin_infos.iter().find(|info| info.id == plugin_id) {
                            state.plugin_graph_plugins.push(
                                maolan_engine::message::PluginGraphPlugin {
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
                                },
                            );
                            next_id = next_id.saturating_add(1);
                        }
                    }
                    let sync = Self::save_open_clip_plugin_graph(&mut state);
                    return Some(sync.map_or_else(Task::none, |action| self.send(action)));
                }

                if let Some(track_name) = track_name {
                    let mut tasks: Vec<Task<Message>> = Vec::new();
                    #[cfg(unix)]
                    {
                        tasks.extend(self.selected_lv2_plugins.iter().cloned().map(|plugin_uri| {
                            self.send(Action::TrackLoadLv2Plugin {
                                track_name: track_name.clone(),
                                plugin_uri,
                                instance_id: None,
                            })
                        }));
                        self.selected_lv2_plugins.clear();
                    }
                    tasks.extend(self.selected_clap_plugins.iter().cloned().map(|plugin_id| {
                        self.send(Action::TrackLoadClapPlugin {
                            track_name: track_name.clone(),
                            plugin_id,
                            instance_id: None,
                        })
                    }));
                    tasks.extend(self.selected_vst3_plugins.iter().cloned().map(|plugin_id| {
                        self.send(Action::TrackLoadVst3Plugin {
                            track_name: track_name.clone(),
                            plugin_id,
                            instance_id: None,
                        })
                    }));
                    self.selected_clap_plugins.clear();
                    self.selected_vst3_plugins.clear();
                    self.modal = None;
                    return Some(Task::batch(tasks));
                }

                self.state.blocking_write().message =
                    "Select a track before loading plugins".to_string();
                None
            }

            Message::ShowClapPluginUi {
                ref track_name,
                clip_idx,
                instance_id,
                ref plugin_id,
            } => {
                if self.session_restore_in_progress {
                    self.state.blocking_write().message =
                        "Plugin UI will be available after session restore finishes".to_string();
                    return Some(self.open_track_plugins_followup(track_name.clone()));
                }
                let has_native_ui = {
                    let state = self.state.blocking_read();
                    state
                        .clap_plugins
                        .iter()
                        .find(|plugin| plugin.id == *plugin_id)
                        .and_then(|plugin| plugin.capabilities.as_ref())
                        .is_none_or(|caps| caps.has_gui)
                };
                if !has_native_ui {
                    self.info(format!(
                        "Opening generic CLAP editor for track '{}' instance {}",
                        track_name, instance_id
                    ));
                    return Some(self.open_generic_plugin_ui(
                        track_name.clone(),
                        clip_idx,
                        instance_id,
                        "CLAP".to_string(),
                        plugin_id.clone(),
                    ));
                }
                self.info(format!(
                    "Requesting CLAP UI for track '{}' instance {}",
                    track_name, instance_id
                ));
                if let Some(clip_idx) = clip_idx {
                    Some(self.send(Action::ClipShowClapGui {
                        track_name: track_name.clone(),
                        clip_idx,
                        instance_id,
                    }))
                } else {
                    Some(self.send(Action::TrackShowClapGui {
                        track_name: track_name.clone(),
                        instance_id,
                    }))
                }
            }
            #[cfg(unix)]
            Message::OpenLv2PluginUi {
                ref track_name,
                clip_idx,
                instance_id,
            } => {
                if self.session_restore_in_progress {
                    self.state.blocking_write().message =
                        "Plugin UI will be available after session restore finishes".to_string();
                    return Some(self.open_track_plugins_followup(track_name.clone()));
                }
                let plugin_id = {
                    let state = self.state.blocking_read();
                    state
                        .plugin_graph_plugins
                        .iter()
                        .find(|plugin| {
                            plugin.instance_id == instance_id
                                && plugin.format.eq_ignore_ascii_case("LV2")
                        })
                        .map(|plugin| plugin.plugin_id.clone())
                        .unwrap_or_default()
                };
                self.pending_native_ui_fallback = Some(crate::gui::PendingNativeUiFallback {
                    track_name: track_name.clone(),
                    clip_idx,
                    instance_id,
                    format: "LV2".to_string(),
                    plugin_id,
                });
                self.info(format!(
                    "Requesting LV2 UI for track '{}' instance {}",
                    track_name, instance_id
                ));
                if let Some(clip_idx) = clip_idx {
                    Some(self.send(Action::ClipShowLv2Gui {
                        track_name: track_name.clone(),
                        clip_idx,
                        instance_id,
                    }))
                } else {
                    Some(self.send(Action::TrackShowLv2Gui {
                        track_name: track_name.clone(),
                        instance_id,
                    }))
                }
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
                clip_idx,
                instance_id,
                ref plugin_id,
            } => {
                if self.session_restore_in_progress {
                    self.state.blocking_write().message =
                        "Plugin UI will be available after session restore finishes".to_string();
                    return Some(self.open_track_plugins_followup(track_name.clone()));
                }
                self.pending_native_ui_fallback = Some(crate::gui::PendingNativeUiFallback {
                    track_name: track_name.clone(),
                    clip_idx,
                    instance_id,
                    format: "VST3".to_string(),
                    plugin_id: plugin_id.clone(),
                });
                self.info(format!(
                    "Requesting VST3 UI for track '{}' instance {}",
                    track_name, instance_id
                ));
                if let Some(clip_idx) = clip_idx {
                    Some(self.send(Action::ClipShowVst3Gui {
                        track_name: track_name.clone(),
                        clip_idx,
                        instance_id,
                    }))
                } else {
                    Some(self.send(Action::TrackShowVst3Gui {
                        track_name: track_name.clone(),
                        instance_id,
                    }))
                }
            }
            Message::OpenGenericPluginUi {
                ref track_name,
                clip_idx,
                instance_id,
                ref format,
                ref plugin_id,
            } => Some(self.open_generic_plugin_ui(
                track_name.clone(),
                clip_idx,
                instance_id,
                format.clone(),
                plugin_id.clone(),
            )),
            Message::GenericPluginParameterChanged {
                ref track_name,
                clip_idx,
                instance_id,
                ref format,
                param_id,
                value,
            } => {
                self.generic_plugin_param_values
                    .insert((track_name.clone(), clip_idx, instance_id, param_id), value);
                match (format.as_str(), clip_idx) {
                    (format, Some(clip_idx)) if format.eq_ignore_ascii_case("CLAP") => {
                        Some(self.send(Action::ClipSetClapParameter {
                            track_name: track_name.clone(),
                            clip_idx,
                            instance_id,
                            param_id,
                            value,
                        }))
                    }
                    (format, Some(clip_idx)) if format.eq_ignore_ascii_case("VST3") => {
                        Some(self.send(Action::ClipSetVst3Parameter {
                            track_name: track_name.clone(),
                            clip_idx,
                            instance_id,
                            param_id,
                            value: value as f32,
                        }))
                    }
                    (format, None) if format.eq_ignore_ascii_case("CLAP") => {
                        Some(self.send(Action::TrackSetClapParameter {
                            track_name: track_name.clone(),
                            instance_id,
                            param_id,
                            value,
                        }))
                    }
                    (format, None) if format.eq_ignore_ascii_case("VST3") => {
                        Some(self.send(Action::TrackSetVst3Parameter {
                            track_name: track_name.clone(),
                            instance_id,
                            param_id,
                            value: value as f32,
                        }))
                    }
                    #[cfg(unix)]
                    (format, Some(clip_idx)) if format.eq_ignore_ascii_case("LV2") => {
                        Some(self.send(Action::ClipSetLv2ControlValue {
                            track_name: track_name.clone(),
                            clip_idx,
                            instance_id,
                            index: param_id,
                            value: value as f32,
                        }))
                    }
                    #[cfg(unix)]
                    (format, None) if format.eq_ignore_ascii_case("LV2") => {
                        Some(self.send(Action::TrackSetLv2ControlValue {
                            track_name: track_name.clone(),
                            instance_id,
                            index: param_id,
                            value: value as f32,
                        }))
                    }
                    _ => None,
                }
            }
            Message::SendMessageFinished(Err(_e)) => None,
            Message::SendMessageFinished(Ok(())) => None,
            _ => None,
        }
    }
}
