use super::*;

impl Maolan {
    pub(super) fn handle_plugin_message(&mut self, message: Message) -> Option<Task<Message>> {
        match message {
            #[cfg(all(unix, not(target_os = "macos")))]
            Message::RefreshLv2Plugins => Some(self.send(Action::ListLv2Plugins)),
            Message::RefreshVst3Plugins => Some(self.send(Action::ListVst3Plugins)),
            Message::RefreshClapPlugins => {
                if self.scan_clap_capabilities {
                    Some(self.send(Action::ListClapPluginsWithCapabilities))
                } else {
                    Some(self.send(Action::ListClapPlugins))
                }
            }
            Message::ToggleClapCapabilityScanning(enabled) => {
                self.scan_clap_capabilities = enabled;
                if self.scan_clap_capabilities {
                    Some(self.send(Action::ListClapPluginsWithCapabilities))
                } else {
                    Some(self.send(Action::ListClapPlugins))
                }
            }
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
                #[cfg(any(target_os = "windows", target_os = "macos"))]
                let format = if format == PluginFormat::Lv2 {
                    PluginFormat::Vst3
                } else {
                    format
                };
                self.plugin_format = format;
                None
            }
            Message::UnloadClapPlugin(ref plugin_path) => {
                let track_name = {
                    let state = self.state.blocking_read();
                    state
                        .plugin_graph_track
                        .clone()
                        .or_else(|| state.selected.iter().next().cloned())
                };
                if let Some(track_name) = track_name {
                    return Some(self.send(Action::TrackUnloadClapPlugin {
                        track_name,
                        plugin_path: plugin_path.clone(),
                    }));
                }
                self.state.blocking_write().message =
                    "Select a track before unloading CLAP plugin".to_string();
                None
            }
            Message::ShowClapPluginUi(ref plugin_path) => {
                if let Err(e) = self.clap_ui_host.open_editor(plugin_path) {
                    self.state.blocking_write().message = e;
                }
                None
            }
            Message::OpenLv2PluginUi {
                ref track_name,
                instance_id,
            } => Some(self.open_lv2_plugin_ui_task(track_name, instance_id)),
            Message::PumpLv2Ui => {
                self.pump_lv2_ui();
                None
            }
            Message::OpenVst3PluginUi {
                ref track_name,
                instance_id,
                ref plugin_path,
                ref plugin_name,
                ref plugin_id,
                audio_inputs,
                audio_outputs,
            } => {
                #[cfg(target_os = "windows")]
                {
                    let _ = (
                        plugin_path,
                        plugin_name,
                        plugin_id,
                        audio_inputs,
                        audio_outputs,
                    );
                    return Some(self.send(Action::TrackGetVst3EditorHandle {
                        track_name: track_name.clone(),
                        instance_id,
                    }));
                }

                #[cfg(not(target_os = "windows"))]
                {
                    let _ = (track_name, instance_id);
                    let (sample_rate_hz, block_size) = {
                        let st = self.state.blocking_read();
                        (self.playback_rate_hz.max(1.0), st.oss_period_frames.max(1))
                    };
                    if let Err(e) = self.vst3_ui_host.open_editor(
                        plugin_path,
                        plugin_name,
                        plugin_id,
                        sample_rate_hz,
                        block_size,
                        audio_inputs,
                        audio_outputs,
                        None,
                    ) {
                        self.state.blocking_write().message = e;
                    }
                }
                None
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
