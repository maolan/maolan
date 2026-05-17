use super::*;
use crate::gui::{PendingClapUiOpen, PendingVst3UiOpen};

impl Maolan {
    pub(super) fn build_sidechain_options_json(&self) -> String {
        let state = self.state.blocking_read();
        let mut tracks = Vec::new();
        for track in &state.tracks {
            let mut plugins = Vec::new();
            if let Some((graph_plugins, _)) = state.plugin_graphs_by_track.get(&track.name) {
                for plugin in graph_plugins {
                    plugins.push(serde_json::json!({ "name": plugin.name }));
                }
            }
            tracks.push(serde_json::json!({
                "name": track.name,
                "outputs": track.audio.outs,
                "plugins": plugins,
            }));
        }
        serde_json::json!({ "tracks": tracks }).to_string()
    }

    fn pump_clap_ui(&mut self) -> Option<Task<Message>> {
        if let Some(update) = self.clap_ui_host.pop_param_update() {
            self.has_unsaved_changes = true;
            let key = (
                update.track_name.clone(),
                update.clip_idx,
                update.instance_id,
                update.param_id,
            );
            self.clap_param_values.insert(key, update.value);

            // Intercept EQ sidechain routing params (196-199):
            // 196 = SidechainEnable, 197 = SidechainSourceTrackIdx,
            // 198 = SidechainSourcePort, 199 = SidechainSourcePluginIdx
            let is_sidechain_param =
                update.param_id >= 196 && update.param_id <= 199 && update.clip_idx.is_none();
            let sidechain_task = if is_sidechain_param {
                self.reconfigure_sidechain(&update.track_name, update.instance_id)
            } else {
                None
            };

            let param_action = if let Some(clip_idx) = update.clip_idx {
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

            if let Some(task) = sidechain_task {
                return Some(Task::batch([task, self.send(param_action)]));
            }
            return Some(self.send(param_action));
        }

        // Check for pending sidechain plugin connections that can now be completed
        // after the plugin has dynamically added its sidechain port.
        if let Some(task) = self.process_pending_sidechain_connections() {
            return Some(task);
        }
        if let Some(update) = self.clap_ui_host.pop_state_update() {
            self.has_unsaved_changes = true;
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
            self.has_unsaved_changes = true;
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

    fn get_clap_param_value(
        &self,
        track_name: &str,
        clip_idx: Option<usize>,
        instance_id: usize,
        param_id: u32,
    ) -> Option<f64> {
        let key = (track_name.to_string(), clip_idx, instance_id, param_id);
        self.clap_param_values.get(&key).copied()
    }

    /// Check if adding a sidechain connection from `source_track` to `target_track`
    /// would create a feedback cycle in the track routing graph.
    fn would_create_sidechain_cycle(
        &self,
        source_track: &str,
        target_track: &str,
    ) -> bool {
        let state = self.state.blocking_read();

        // Build adjacency list from existing audio connections.
        let mut adjacency: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for conn in &state.connections {
            if matches!(conn.kind, maolan_engine::kind::Kind::Audio) {
                adjacency
                    .entry(conn.from_track.clone())
                    .or_default()
                    .push(conn.to_track.clone());
            }
        }
        drop(state);

        // DFS from target_track to see if we can already reach source_track.
        let mut visited = std::collections::HashSet::new();
        let mut stack = vec![target_track.to_string()];

        while let Some(current) = stack.pop() {
            if current == source_track {
                return true;
            }
            if !visited.insert(current.clone()) {
                continue;
            }
            if let Some(neighbors) = adjacency.get(&current) {
                for neighbor in neighbors {
                    if !visited.contains(neighbor) {
                        stack.push(neighbor.clone());
                    }
                }
            }
        }

        false
    }

    pub(super) fn reconfigure_sidechain(
        &mut self,
        track_name: &str,
        instance_id: usize,
    ) -> Option<Task<Message>> {
        let enabled = self
            .get_clap_param_value(track_name, None, instance_id, 196)
            .unwrap_or(0.0)
            >= 0.5;
        let source_track_idx = self
            .get_clap_param_value(track_name, None, instance_id, 197)
            .unwrap_or(0.0) as usize;
        let source_port = self
            .get_clap_param_value(track_name, None, instance_id, 198)
            .unwrap_or(0.0) as usize;
        let source_plugin_idx = self
            .get_clap_param_value(track_name, None, instance_id, 199)
            .unwrap_or(0.0) as usize;

        let mut state = self.state.blocking_write();
        let sidechain_key = (track_name.to_string(), instance_id);

        if !enabled {
            if let Some(existing) = state.plugin_sidechains.remove(&sidechain_key) {
                drop(state);
                return Some(self.cleanup_sidechain_routing(track_name, instance_id, existing));
            }
            return None;
        }

        // Determine source track name
        let tracks_snapshot: Vec<String> = state.tracks.iter().map(|t| t.name.clone()).collect();
        let source_track = if source_track_idx > 0 && source_track_idx <= tracks_snapshot.len() {
            Some(tracks_snapshot[source_track_idx - 1].clone())
        } else {
            None
        };

        let Some(source_track_name) = source_track else {
            return None;
        };

        // Detect feedback cycles: reject if target_track already routes to source_track.
        if source_track_name != track_name
            && self.would_create_sidechain_cycle(&source_track_name, track_name)
        {
            let mut state = self.state.blocking_write();
            state.message = format!(
                "Sidechain cycle detected: {} already routes to {}",
                track_name, source_track_name
            );
            return None;
        }

        // If a plugin source is requested, try to resolve it from the plugin graph.
        let source_plugin_instance_id = if source_plugin_idx > 0 {
            state
                .plugin_graphs_by_track
                .get(&source_track_name)
                .and_then(|(plugins, _)| plugins.get(source_plugin_idx - 1))
                .map(|p| p.instance_id)
        } else {
            None
        };

        // Check if we already have a sidechain configured for this plugin
        let existing_needs_cleanup = state.plugin_sidechains.get(&sidechain_key).cloned();
        let needs_new_routing = if let Some(existing) = &existing_needs_cleanup {
            if existing.source_track.as_deref() == Some(&source_track_name)
                && existing.source_port == source_port
                && existing.source_plugin_instance_id == source_plugin_instance_id
                && !existing.plugin_connection_pending
            {
                // No change needed
                return None;
            }
            true
        } else {
            true
        };

        if !needs_new_routing {
            return None;
        }

        // Collect needed data before dropping the lock
        let target_track_ins = state
            .tracks
            .iter()
            .find(|t| t.name == track_name)
            .map(|t| t.audio.ins)
            .unwrap_or(0);

        // Determine how many sidechain channels the plugin needs (from its graph).
        let sidechain_channel_count = state
            .plugin_graphs_by_track
            .get(track_name)
            .and_then(|(plugins, _)| plugins.iter().find(|p| p.instance_id == instance_id))
            .map(|p| p.main_audio_inputs.max(1))
            .unwrap_or(2);

        let source_track_send_port = if source_plugin_instance_id.is_some() {
            state
                .tracks
                .iter()
                .find(|t| t.name == source_track_name)
                .map(|t| t.audio.outs)
        } else {
            None
        };

        let plugin_graph_missing = source_plugin_idx > 0 && source_plugin_instance_id.is_none();

        state.plugin_sidechains.insert(
            sidechain_key,
            crate::state::PluginSidechainState {
                enabled: true,
                source_track: Some(source_track_name.clone()),
                source_plugin_idx,
                source_plugin_instance_id,
                source_port,
                source_is_hw: false,
                target_receive_port: Some(target_track_ins),
                target_plugin_sidechain_port: 0, // computed dynamically after plugin reconfigures
                sidechain_channel_count,
                plugin_connection_pending: true,
                source_track_send_port,
            },
        );

        drop(state);

        // Build routing actions: add target receive and connect source track output to it.
        // The plugin connection is deferred until the plugin dynamically adds its sidechain port.
        let mut tasks: Vec<Task<Message>> = vec![];

        // Clean up old routing if source changed
        if let Some(existing) = existing_needs_cleanup {
            tasks.push(self.cleanup_sidechain_routing(track_name, instance_id, existing));
        }

        // If plugin graph is missing, proactively fetch it so the plugin source
        // can be resolved on the next graph update.
        if plugin_graph_missing {
            tasks.push(self.send(Action::TrackGetPluginGraph {
                track_name: source_track_name.clone(),
            }));
        }

        // If using a plugin source, add a send port on the source track and
        // connect the plugin output to it.
        if let Some(src_plugin_id) = source_plugin_instance_id {
            if let Some(send_port) = source_track_send_port {
                tasks.push(self.send(Action::TrackAddAudioOutput(source_track_name.clone())));
                tasks.push(self.send(Action::TrackConnectPluginAudio {
                    track_name: source_track_name.clone(),
                    from_node: maolan_engine::message::PluginGraphNode::ClapPluginInstance(src_plugin_id),
                    from_port: 0,
                    to_node: maolan_engine::message::PluginGraphNode::TrackOutput,
                    to_port: send_port,
                }));
            }
        }

        // Add one track input per sidechain channel and connect each source channel.
        let connect_from_port = source_track_send_port.unwrap_or(source_port);
        for ch in 0..sidechain_channel_count {
            tasks.push(self.send(Action::TrackAddAudioInput(track_name.to_string())));
            tasks.push(self.send(Action::Connect {
                from_track: source_track_name.clone(),
                from_port: connect_from_port + ch,
                to_track: track_name.to_string(),
                to_port: target_track_ins + ch,
                kind: maolan_engine::kind::Kind::Audio,
            }));
        }

        Some(Task::batch(tasks))
    }

    fn cleanup_sidechain_routing(
        &mut self,
        track_name: &str,
        instance_id: usize,
        state: crate::state::PluginSidechainState,
    ) -> Task<Message> {
        let mut actions = vec![];

        if let Some(source_track) = &state.source_track {
            let ch_count = state.sidechain_channel_count.max(1);
            let receive_port_start = state.target_receive_port.unwrap_or(0);
            let disconnect_from_port = state.source_track_send_port.unwrap_or(state.source_port);

            // Disconnect plugin inputs (only if already connected)
            if !state.plugin_connection_pending {
                for ch in 0..ch_count {
                    actions.push(Action::TrackDisconnectPluginAudio {
                        track_name: track_name.to_string(),
                        from_node: maolan_engine::message::PluginGraphNode::TrackInput,
                        from_port: receive_port_start + ch,
                        to_node: maolan_engine::message::PluginGraphNode::ClapPluginInstance(instance_id),
                        to_port: state.target_plugin_sidechain_port + ch,
                    });
                }
            }
            // Disconnect track-to-track for each sidechain channel
            for ch in 0..ch_count {
                actions.push(Action::Disconnect {
                    from_track: source_track.clone(),
                    from_port: disconnect_from_port + ch,
                    to_track: track_name.to_string(),
                    to_port: receive_port_start + ch,
                    kind: maolan_engine::kind::Kind::Audio,
                });
            }
            // Remove receive ports
            for _ in 0..ch_count {
                actions.push(Action::TrackRemoveAudioInput(track_name.to_string()));
            }

            // Clean up plugin-to-track output routing if a plugin source was used.
            if let (Some(src_plugin_id), Some(src_send_port)) =
                (state.source_plugin_instance_id, state.source_track_send_port)
            {
                actions.push(Action::TrackDisconnectPluginAudio {
                    track_name: source_track.clone(),
                    from_node: maolan_engine::message::PluginGraphNode::ClapPluginInstance(src_plugin_id),
                    from_port: 0,
                    to_node: maolan_engine::message::PluginGraphNode::TrackOutput,
                    to_port: src_send_port,
                });
                actions.push(Action::TrackRemoveAudioOutput(source_track.to_string()));
            }
        }

        Task::batch(actions.into_iter().map(|a| self.send(a)))
    }

    pub(super) fn process_pending_sidechain_connections(&mut self) -> Option<Task<Message>> {
        let pending: Vec<((String, usize), crate::state::PluginSidechainState)> = {
            let state = self.state.blocking_read();
            state
                .plugin_sidechains
                .iter()
                .filter(|(_, sc)| sc.enabled && sc.plugin_connection_pending)
                .map(|(key, sc)| (key.clone(), sc.clone()))
                .collect()
        };

        for ((track_name, instance_id), sc) in pending {
            // Check if the plugin now has sidechain ports by looking at the
            // published plugin graph. When audio_inputs > main_audio_inputs,
            // the extra port(s) are sidechain ports.
            let plugin_info = {
                let state = self.state.blocking_read();
                state
                    .plugin_graphs_by_track
                    .get(&track_name)
                    .and_then(|(plugins, _)| {
                        plugins.iter().find(|p| p.instance_id == instance_id)
                    })
                    .cloned()
            };

            if let Some(plugin) = plugin_info {
                let expected_sc_ports = sc.sidechain_channel_count.max(1);
                let actual_sc_ports = plugin.audio_inputs.saturating_sub(plugin.main_audio_inputs);
                if actual_sc_ports >= expected_sc_ports {
                    let sidechain_port_start = plugin.main_audio_inputs;
                    if let Some(receive_port_start) = sc.target_receive_port {
                        let mut actions = vec![];
                        for ch in 0..expected_sc_ports {
                            actions.push(Action::TrackConnectPluginAudio {
                                track_name: track_name.clone(),
                                from_node: maolan_engine::message::PluginGraphNode::TrackInput,
                                from_port: receive_port_start + ch,
                                to_node: maolan_engine::message::PluginGraphNode::ClapPluginInstance(
                                    instance_id,
                                ),
                                to_port: sidechain_port_start + ch,
                            });
                        }
                        // Update state to mark connection as no longer pending
                        let mut state = self.state.blocking_write();
                        if let Some(sc_state) = state
                            .plugin_sidechains
                            .get_mut(&(track_name.clone(), instance_id))
                        {
                            sc_state.plugin_connection_pending = false;
                            sc_state.target_plugin_sidechain_port = sidechain_port_start;
                        }
                        drop(state);

                        return Some(Task::batch(actions.into_iter().map(|a| self.send(a))));
                    }
                }
            }
        }

        None
    }

    /// After a plugin graph arrives, retry any sidechains on tracks that reference
    /// this graph's track as a plugin source but failed to resolve the plugin
    /// because the graph was not yet loaded.
    pub(super) fn retry_sidechain_plugin_sources(
        &mut self,
        graph_track_name: &str,
    ) -> Option<Task<Message>> {
        let pending: Vec<(String, usize)> = {
            let state = self.state.blocking_read();
            state
                .plugin_sidechains
                .iter()
                .filter(|(_, sc)| {
                    sc.enabled
                        && sc.source_track.as_deref() == Some(graph_track_name)
                        && sc.source_plugin_idx > 0
                        && sc.source_plugin_instance_id.is_none()
                })
                .map(|((track_name, instance_id), _)| (track_name.clone(), *instance_id))
                .collect()
        };

        let mut tasks = Vec::new();
        for (target_track, instance_id) in pending {
            if let Some(task) = self.reconfigure_sidechain(&target_track, instance_id) {
                tasks.push(task);
            }
        }

        if tasks.is_empty() {
            None
        } else {
            Some(Task::batch(tasks))
        }
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
