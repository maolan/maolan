use super::*;
use crate::gui::{PendingClapUiOpen, PendingVst3UiOpen};

impl Maolan {
    fn pump_clap_ui(&mut self) -> Option<Task<Message>> {
        if let Some(update) = self.clap_ui_host.pop_param_update() {
            let action = if let Some(clip_idx) = update.clip_idx {
                Action::ClipSetClapParameter {
                    track_name: update.track_name,
                    clip_idx,
                    instance_id: update.instance_id,
                    param_id: update.param_id,
                    value: update.value,
                }
            } else {
                Action::TrackSetClapParameter {
                    track_name: update.track_name,
                    instance_id: update.instance_id,
                    param_id: update.param_id,
                    value: update.value,
                }
            };
            return Some(self.send(action));
        }
        if let Some(update) = self.clap_ui_host.pop_state_update() {
            let restore_action = if let Some(clip_idx) = update.clip_idx {
                Action::ClipClapRestoreState {
                    track_name: update.track_name.clone(),
                    clip_idx,
                    instance_id: update.instance_id,
                    state: update.state.clone(),
                }
            } else {
                Action::TrackClapRestoreState {
                    track_name: update.track_name.clone(),
                    instance_id: update.instance_id,
                    state: update.state.clone(),
                }
            };
            let mut state = self.state.blocking_write();
            if let Some(clip_idx) = update.clip_idx {
                if let Some(track) = state
                    .tracks
                    .iter_mut()
                    .find(|track| track.name == update.track_name)
                    && let Some(clip) = track.audio.clips.get_mut(clip_idx)
                    && let Some(graph_json) = Self::plugin_graph_json_with_saved_plugin_state(
                        clip.plugin_graph_json.as_ref(),
                        update.instance_id,
                        serde_json::to_value(&update.state).unwrap_or(serde_json::Value::Null),
                    )
                {
                    clip.plugin_graph_json = Some(graph_json);
                }
                drop(state);
                return Some(self.send(restore_action));
            } else {
                state
                    .clap_states_by_track
                    .entry(update.track_name)
                    .or_default()
                    .insert(update.plugin_path, update.state);
                drop(state);
                return Some(self.send(restore_action));
            }
        }
        if let Some(closed) = self.clap_ui_host.drain_closed_states().into_iter().next() {
            let mut state = self.state.blocking_write();
            if let Some(clip_idx) = closed.clip_idx {
                if let Some(track) = state
                    .tracks
                    .iter_mut()
                    .find(|track| track.name == closed.track_name)
                    && let Some(clip) = track.audio.clips.get_mut(clip_idx)
                    && let Some(graph_json) = Self::plugin_graph_json_with_saved_plugin_state(
                        clip.plugin_graph_json.as_ref(),
                        closed.instance_id,
                        serde_json::to_value(&closed.state).unwrap_or(serde_json::Value::Null),
                    )
                {
                    clip.plugin_graph_json = Some(graph_json);
                }
            } else {
                state
                    .clap_states_by_track
                    .entry(closed.track_name)
                    .or_default()
                    .insert(closed.plugin_path, closed.state);
            }
        }
        None
    }

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
                clip_idx,
                instance_id,
                ref plugin_path,
            } => {
                self.pending_clap_ui_open = Some(PendingClapUiOpen {
                    track_name: track_name.clone(),
                    clip_idx,
                    instance_id,
                    plugin_path: plugin_path.clone(),
                });
                Some(if let Some(clip_idx) = clip_idx {
                    self.send(Action::ClipGetClapProcessor {
                        track_name: track_name.clone(),
                        clip_idx,
                        instance_id,
                    })
                } else {
                    self.send(Action::TrackGetClapProcessor {
                        track_name: track_name.clone(),
                        instance_id,
                    })
                })
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            Message::OpenLv2PluginUi {
                ref track_name,
                clip_idx,
                instance_id,
            } => Some(if let Some(clip_idx) = clip_idx {
                self.send(Action::ClipGetLv2PluginControls {
                    track_name: track_name.clone(),
                    clip_idx,
                    instance_id,
                })
            } else {
                self.open_lv2_plugin_ui_task(track_name, instance_id)
            }),
            #[cfg(all(unix, not(target_os = "macos")))]
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
            Message::PumpClapUi => self.pump_clap_ui(),
            #[cfg(all(unix, not(target_os = "macos")))]
            Message::PumpLv2Ui => {
                self.pump_lv2_ui();
                for closed in self.vst3_ui_host.drain_closed_states() {
                    let mut state = self.state.blocking_write();
                    if let Some(clip_idx) = closed.clip_idx {
                        if let Some(track) = state
                            .tracks
                            .iter_mut()
                            .find(|track| track.name == closed.track_name)
                            && let Some(clip) = track.audio.clips.get_mut(clip_idx)
                            && let Some(graph_json) =
                                Self::plugin_graph_json_with_saved_plugin_state(
                                    clip.plugin_graph_json.as_ref(),
                                    closed.instance_id,
                                    serde_json::to_value(&closed.state)
                                        .unwrap_or(serde_json::Value::Null),
                                )
                        {
                            clip.plugin_graph_json = Some(graph_json);
                        }
                    } else {
                        state
                            .vst3_states_by_track
                            .entry(closed.track_name)
                            .or_default()
                            .insert(closed.instance_id, closed.state);
                    }
                }
                None
            }
            Message::OpenVst3PluginUi {
                ref track_name,
                clip_idx,
                instance_id,
                ref plugin_path,
            } => {
                self.pending_vst3_ui_open = Some(PendingVst3UiOpen {
                    track_name: track_name.clone(),
                    clip_idx,
                    instance_id,
                    plugin_path: plugin_path.clone(),
                });
                Some(if let Some(clip_idx) = clip_idx {
                    self.send(Action::ClipGetVst3Processor {
                        track_name: track_name.clone(),
                        clip_idx,
                        instance_id,
                    })
                } else {
                    self.send(Action::TrackGetVst3Processor {
                        track_name: track_name.clone(),
                        instance_id,
                    })
                })
            }
            Message::SendMessageFinished(Err(ref e)) => {
                error!("Error: {}", e);
                None
            }
            Message::SendMessageFinished(Ok(())) => None,
            _ => None,
        }
    }
}
