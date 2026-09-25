use super::*;
use crate::state::{HW, SlotPlayState};
use maolan_engine::message::{Event, QueryReply};

/// Handling for engine `Event` reports (unsolicited state reports that were
/// formerly `Action` echoes wrapped in `Ok(...)`). Arm bodies are ported
/// verbatim from the former `Message::Response(Ok(Action::...))` flow.
impl Maolan {
    pub(super) fn handle_engine_event(&mut self, e: &Event) -> Task<Message> {
        match e {
            Event::HistoryState { dirty } => {
                self.session_ops.engine_dirty = *dirty;
            }
            Event::Log { source, message } => {
                self.info(format!("[{source}] {message}"));
            }
            Event::TrackAutomationLevel { track_name, level } => {
                tracing::debug!(%track_name, level, "DAW received TrackAutomationLevel");
                let mut state = self.state.write().expect("state lock poisoned");
                if track_name == "hw:out" {
                    state.hw_out_level = *level;
                } else if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name)
                {
                    track.level = *level;
                }
            }
            Event::TrackAutomationBalance {
                track_name,
                balance,
            } => {
                tracing::debug!(%track_name, balance, "DAW received TrackAutomationBalance");
                let mut state = self.state.write().expect("state lock poisoned");
                if track_name == "hw:out" {
                    state.hw_out_balance = *balance;
                } else if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name)
                {
                    track.balance = *balance;
                }
            }
            Event::TrackMeters {
                track_name,
                output_db,
            } => {
                if track_name == "hw:out" {
                    let mut state = self.state.write().expect("state lock poisoned");
                    Self::smooth_meter_db_levels(&mut state.hw_out_meter_db, output_db);
                } else {
                    let mut state = self.state.write().expect("state lock poisoned");
                    if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name) {
                        Self::smooth_meter_db_levels(&mut track.meter_out_db, output_db);
                    }
                }
            }
            _ => {}
        }
        if let Event::StepRecordMidiNote {
            channel,
            pitch,
            velocity,
            ..
        } = e
        {
            return self.handle_step_record_note(*channel, *pitch, *velocity);
        }
        if let Event::SessionMidiLearnTriggered { target } = e {
            match target {
                maolan_engine::message::SessionMidiLearnTarget::Slot {
                    track_name,
                    scene_index,
                } => {
                    self.toggle_session_slot(track_name, *scene_index);
                }
                maolan_engine::message::SessionMidiLearnTarget::Scene(scene_index) => {
                    let _task = self.launch_session_scene(*scene_index, false);
                }
                maolan_engine::message::SessionMidiLearnTarget::StopTrack(track_name) => {
                    self.stop_session_track(track_name);
                }
                maolan_engine::message::SessionMidiLearnTarget::StopAll => {
                    self.stop_all_session_clips();
                }
            }
            return Task::none();
        }
        match e {
            Event::TrackClapResourceFiles {
                track_name,
                instance_id,
                files,
            } => {
                let plugin_ref = crate::gui::PluginInstanceRef::Track {
                    track_name: track_name.clone(),
                    instance_id: *instance_id,
                };
                self.handle_clap_resource_files_response(&plugin_ref, files);
            }
            Event::ClipClapResourceFiles {
                track_name,
                clip_idx,
                instance_id,
                files,
            } => {
                let plugin_ref = crate::gui::PluginInstanceRef::Clip {
                    track_name: track_name.clone(),
                    clip_idx: *clip_idx,
                    instance_id: *instance_id,
                };
                self.handle_clap_resource_files_response(&plugin_ref, files);
            }
            Event::TrackClapStateDirty {
                track_name,
                instance_id,
            } => {
                tracing::info!(%track_name, instance_id, "DAW received CLAP state dirty");
                self.session_ops.engine_dirty = true;
                let mut tasks = vec![self.send(Action::TrackClapSnapshotState {
                    track_name: track_name.clone(),
                    instance_id: *instance_id,
                })];
                if let Some(task) = self.maybe_refresh_plugin_graph_for_track(track_name) {
                    tasks.push(task);
                }
                return Task::batch(tasks);
            }
            Event::ClipClapStateDirty {
                track_name,
                clip_idx,
                instance_id,
            } => {
                tracing::info!(%track_name, instance_id, "DAW received CLAP state dirty");
                self.session_ops.engine_dirty = true;
                let mut tasks = vec![self.send(Action::ClipClapSnapshotState {
                    track_name: track_name.clone(),
                    clip_idx: *clip_idx,
                    instance_id: *instance_id,
                })];
                if let Some(task) = self.maybe_refresh_plugin_graph_for_track(track_name) {
                    tasks.push(task);
                }
                return Task::batch(tasks);
            }
            _ => {}
        }
        // Session runtime reports and hw info (ported from
        // `handle_response_engine_state_action`).
        match e {
            Event::SessionRuntimeReport {
                track_name,
                scene_index,
                state: engine_state,
                play_position_samples,
                elapsed_samples,
            } => {
                let mut state = self.state.write().expect("state lock poisoned");
                let runtime = state
                    .slot_runtimes
                    .entry((track_name.clone(), *scene_index))
                    .or_default();
                runtime.state = match engine_state {
                    EngineSessionSlotState::Stopped => SlotPlayState::Stopped,
                    EngineSessionSlotState::Queued => SlotPlayState::Queued,
                    EngineSessionSlotState::Playing => SlotPlayState::Playing,
                    EngineSessionSlotState::Stopping => SlotPlayState::Stopping,
                };
                runtime.play_position_samples = *play_position_samples;
                runtime.elapsed_samples = *elapsed_samples;
            }
            Event::HWInfo {
                channels,
                rate,
                input,
            } => {
                if *rate > 0 {
                    self.transport.playback_rate_hz = *rate as f64;
                }
                let mut state = self.state.write().expect("state lock poisoned");
                if *rate > 0 {
                    state.hw_sample_rate_hz = *rate as i32;
                }
                if !state.hw_loaded {
                    state.hw_loaded = true;
                }
                let direction = if *input { "input" } else { "output" };
                state.message = format!("HW {direction} channels: {channels} @ {rate} Hz");
                if *input {
                    state.hw_in = Some(HW {
                        channels: *channels,
                    });
                } else {
                    state.hw_out = Some(HW {
                        channels: *channels,
                    });
                    if state.hw_out_meter_db.len() != *channels {
                        state.hw_out_meter_db = vec![-90.0; *channels];
                    }
                }
            }
            _ => {}
        }
        match e {
            Event::TrackClapStateSnapshot {
                track_name,
                instance_id: _instance_id,
                plugin_id,
                state: clap_state,
                ..
            } => {
                let state_len = clap_state.bytes.len();
                tracing::info!(%track_name, instance_id = *_instance_id, %plugin_id, state_len, "DAW received TrackClapStateSnapshot");
                let mut state = self.state.write().expect("state lock poisoned");
                state
                    .clap_states_by_track
                    .entry(track_name.clone())
                    .or_default()
                    .insert(plugin_id.clone(), (**clap_state).clone());
                #[cfg(unix)]
                {
                    let state_json =
                        serde_json::to_value(clap_state).unwrap_or(serde_json::Value::Null);
                    if let Some((plugins, _)) = state.plugin_graphs_by_track.get_mut(track_name)
                        && let Some(plugin) = plugins
                            .iter_mut()
                            .find(|plugin| plugin.instance_id == *_instance_id)
                    {
                        tracing::info!(%track_name, instance_id = *_instance_id, "DAW updated plugin_graphs_by_track state");
                        plugin.state = Some(state_json.clone());
                    } else {
                        tracing::warn!(%track_name, instance_id = *_instance_id, "DAW could not find plugin in plugin_graphs_by_track");
                    }
                    if state.plugin_graph_clip.is_none()
                        && state.plugin_graph_track.as_deref() == Some(track_name.as_str())
                        && let Some(plugin) = state
                            .plugin_graph_plugins
                            .iter_mut()
                            .find(|plugin| plugin.instance_id == *_instance_id)
                    {
                        plugin.state = Some(state_json);
                    }
                }
            }
            Event::ClipClapStateSnapshot {
                track_name,
                clip_idx,
                instance_id,
                plugin_id: _,
                state: clap_state,
                ..
            } => {
                let state_json =
                    serde_json::to_value(clap_state).unwrap_or(serde_json::Value::Null);
                {
                    let mut state = self.state.write().expect("state lock poisoned");
                    if let Some(track) = state
                        .tracks
                        .iter_mut()
                        .find(|track| track.name == *track_name)
                        && let Some(clip) = track.audio.clips.get_mut(*clip_idx)
                        && let Some(graph_json) = Self::plugin_graph_json_with_saved_plugin_state(
                            clip.plugin_graph_json.as_ref(),
                            *instance_id,
                            state_json,
                        )
                    {
                        clip.plugin_graph_json = Some(graph_json);
                    }
                }
                if self.pending.pending_save_path.is_some() {
                    self.pending.pending_save_clap_clips.remove(&(
                        track_name.clone(),
                        *clip_idx,
                        *instance_id,
                    ));
                    if let Some(task) = self.complete_pending_save(track_name) {
                        return task;
                    }
                }
            }
            Event::TrackVst3StateSnapshot {
                track_name,
                instance_id,
                state,
            } => {
                let mut gui_state = self.state.write().expect("state lock poisoned");
                gui_state
                    .vst3_states_by_track
                    .entry(track_name.clone())
                    .or_default()
                    .insert(*instance_id, (**state).clone());
            }
            Event::ClipVst3StateSnapshot {
                track_name,
                clip_idx,
                instance_id,
                state,
            } => {
                let state_json = serde_json::to_value(state).unwrap_or(serde_json::Value::Null);
                let mut gui_state = self.state.write().expect("state lock poisoned");
                if let Some(track) = gui_state
                    .tracks
                    .iter_mut()
                    .find(|track| track.name == *track_name)
                    && let Some(clip) = track.audio.clips.get_mut(*clip_idx)
                    && let Some(graph_json) = Self::plugin_graph_json_with_saved_plugin_state(
                        clip.plugin_graph_json.as_ref(),
                        *instance_id,
                        state_json,
                    )
                {
                    clip.plugin_graph_json = Some(graph_json);
                }
            }
            Event::ClipLv2StateSnapshot {
                track_name,
                clip_idx,
                instance_id,
                state: lv2_state,
            } => {
                let mut state = self.state.write().expect("state lock poisoned");
                if let Some(track) = state
                    .tracks
                    .iter_mut()
                    .find(|track| track.name == *track_name)
                    && let Some(clip) = track.audio.clips.get_mut(*clip_idx)
                    && let Some(graph_json) = Self::plugin_graph_json_with_saved_plugin_state(
                        clip.plugin_graph_json.as_ref(),
                        *instance_id,
                        Self::lv2_state_to_json(lv2_state),
                    )
                {
                    clip.plugin_graph_json = Some(graph_json);
                }
                if state.plugin_graph_clip.as_ref().is_some_and(|target| {
                    target.track_name == *track_name && target.clip_idx == *clip_idx
                }) && let Some(plugin) = state
                    .plugin_graph_plugins
                    .iter_mut()
                    .find(|plugin| plugin.instance_id == *instance_id)
                {
                    plugin.state = Some(Self::lv2_state_to_json(lv2_state));
                }
            }
            Event::TrackSnapshotAllClapStatesDone { track_name }
                if self.pending.pending_save_path.is_some() =>
            {
                self.pending.pending_save_clap_tracks.remove(track_name);
                if let Some(task) = self.complete_pending_save(track_name) {
                    return task;
                }
            }
            _ => {}
        }
        Task::none()
    }

    pub(super) fn handle_engine_query_reply(&mut self, q: &QueryReply) -> Task<Message> {
        if let Some(task) = self.handle_query_reply_session_state(q) {
            return task;
        }
        match q {
            QueryReply::JackGraph(graph) => {
                let mut state = self.state.write().expect("state lock poisoned");
                state.jack_graph = graph.clone();
                state.jack_session_routing = Some(graph.clone());
                state.message = format!(
                    "JACK graph: {} ports, {} connections",
                    graph.ports.len(),
                    graph.connections.len()
                );
            }
            QueryReply::MidiLearnMappingsReport { lines } => {
                let report = lines.join(" | ");
                self.ui.midi_mappings_report_lines = lines.clone();
                let mut state = self.state.write().expect("state lock poisoned");
                state.message = format!("MIDI mappings: {}", report);
            }
            QueryReply::MeterSnapshot {
                hw_out_db,
                track_meters,
                ..
            } => {
                return self.apply_meter_snapshot(hw_out_db, track_meters);
            }
            QueryReply::TrackClapParameters {
                track_name,
                instance_id,
                parameters,
            } => {
                let pending = self
                    .pending
                    .pending_add_clap_automation_instances
                    .remove(&(track_name.clone(), *instance_id));
                {
                    let mut state = self.state.write().expect("state lock poisoned");
                    let cached = state
                        .plugin_parameters_by_track
                        .entry(track_name.clone())
                        .or_default();
                    cached.insert(
                        *instance_id,
                        parameters
                            .iter()
                            .map(|p| crate::state::PluginParameterInfo {
                                param_id: p.id,
                                name: p.name.clone(),
                                min: p.min_value,
                                max: p.max_value,
                                default_value: p.default_value,
                            })
                            .collect(),
                    );
                    if pending
                        && let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name)
                    {
                        for param in parameters {
                            let target = TrackAutomationTarget::ClapParameter {
                                instance_id: *instance_id,
                                param_id: param.id,
                                min: param.min_value,
                                max: param.max_value,
                            };
                            if let Some(existing) = track
                                .automation_lanes
                                .iter_mut()
                                .find(|lane| lane.target == target)
                            {
                                existing.visible = true;
                            } else {
                                track
                                    .automation_lanes
                                    .push(crate::state::TrackAutomationLane {
                                        target,
                                        visible: true,
                                        points: vec![],
                                    });
                            }
                        }
                        track.height = track.min_height_for_layout().max(TRACK_MIN_HEIGHT);
                        state.message = format!(
                            "Added {} CLAP automation lanes on '{}'",
                            parameters.len(),
                            track_name
                        );
                    }
                }
                if pending && let Some(action) = self.track_automation_lanes_action(track_name) {
                    self.try_send_engine(EngineMessage::Request(action));
                }
            }
            QueryReply::ClipClapParameters {
                track_name,
                clip_idx,
                instance_id,
                parameters,
            } => {
                let mut state = self.state.write().expect("state lock poisoned");
                let cached = state
                    .plugin_parameters_by_clip
                    .entry((track_name.clone(), *clip_idx))
                    .or_default();
                cached.insert(
                    *instance_id,
                    parameters
                        .iter()
                        .map(|p| crate::state::PluginParameterInfo {
                            param_id: p.id,
                            name: p.name.clone(),
                            min: p.min_value,
                            max: p.max_value,
                            default_value: p.default_value,
                        })
                        .collect(),
                );
            }
            QueryReply::TrackVst3Parameters {
                track_name,
                instance_id,
                parameters,
            } => {
                let pending = self
                    .pending
                    .pending_add_vst3_automation_instances
                    .remove(&(track_name.clone(), *instance_id));
                {
                    let mut state = self.state.write().expect("state lock poisoned");
                    let cached = state
                        .plugin_parameters_by_track
                        .entry(track_name.clone())
                        .or_default();
                    cached.insert(
                        *instance_id,
                        parameters
                            .iter()
                            .map(|p| crate::state::PluginParameterInfo {
                                param_id: p.id,
                                name: p.title.clone(),
                                min: 0.0,
                                max: 1.0,
                                default_value: p.default_value,
                            })
                            .collect(),
                    );
                    if pending
                        && let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name)
                    {
                        for param in parameters {
                            let target = TrackAutomationTarget::Vst3Parameter {
                                instance_id: *instance_id,
                                param_id: param.id,
                            };
                            if let Some(existing) = track
                                .automation_lanes
                                .iter_mut()
                                .find(|lane| lane.target == target)
                            {
                                existing.visible = true;
                            } else {
                                track
                                    .automation_lanes
                                    .push(crate::state::TrackAutomationLane {
                                        target,
                                        visible: true,
                                        points: vec![],
                                    });
                            }
                        }
                        track.height = track.min_height_for_layout().max(TRACK_MIN_HEIGHT);
                        state.message = format!(
                            "Added {} VST3 automation lanes on '{}'",
                            parameters.len(),
                            track_name
                        );
                    }
                }
                if pending && let Some(action) = self.track_automation_lanes_action(track_name) {
                    self.try_send_engine(EngineMessage::Request(action));
                }
            }
            QueryReply::ClipVst3Parameters {
                track_name,
                clip_idx,
                instance_id,
                parameters,
            } => {
                let mut state = self.state.write().expect("state lock poisoned");
                let cached = state
                    .plugin_parameters_by_clip
                    .entry((track_name.clone(), *clip_idx))
                    .or_default();
                cached.insert(
                    *instance_id,
                    parameters
                        .iter()
                        .map(|p| crate::state::PluginParameterInfo {
                            param_id: p.id,
                            name: p.title.clone(),
                            min: 0.0,
                            max: 1.0,
                            default_value: p.default_value,
                        })
                        .collect(),
                );
            }
            QueryReply::ClipLv2PluginControls {
                track_name,
                clip_idx,
                instance_id,
                controls,
                instance_access_handle: _,
            } => {
                let mut state = self.state.write().expect("state lock poisoned");
                let cached = state
                    .plugin_parameters_by_clip
                    .entry((track_name.clone(), *clip_idx))
                    .or_default();
                cached.insert(
                    *instance_id,
                    controls
                        .iter()
                        .map(|p| crate::state::PluginParameterInfo {
                            param_id: p.index,
                            name: p.name.clone(),
                            min: p.min as f64,
                            max: p.max as f64,
                            default_value: p.value as f64,
                        })
                        .collect(),
                );
            }
            QueryReply::TrackLv2PluginControls {
                track_name,
                instance_id,
                controls,
                instance_access_handle: _,
            } => {
                let pending = self
                    .pending
                    .pending_add_lv2_automation_instances
                    .remove(&(track_name.clone(), *instance_id));
                {
                    let mut state = self.state.write().expect("state lock poisoned");
                    let cached = state
                        .plugin_parameters_by_track
                        .entry(track_name.clone())
                        .or_default();
                    cached.insert(
                        *instance_id,
                        controls
                            .iter()
                            .map(|p| crate::state::PluginParameterInfo {
                                param_id: p.index,
                                name: p.name.clone(),
                                min: p.min as f64,
                                max: p.max as f64,
                                default_value: p.value as f64,
                            })
                            .collect(),
                    );
                    if pending
                        && let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name)
                    {
                        for param in controls {
                            let target = TrackAutomationTarget::Lv2Parameter {
                                instance_id: *instance_id,
                                index: param.index,
                                min: param.min,
                                max: param.max,
                            };
                            if let Some(existing) = track
                                .automation_lanes
                                .iter_mut()
                                .find(|lane| lane.target == target)
                            {
                                existing.visible = true;
                            } else {
                                track
                                    .automation_lanes
                                    .push(crate::state::TrackAutomationLane {
                                        target,
                                        visible: true,
                                        points: vec![],
                                    });
                            }
                        }
                        track.height = track.min_height_for_layout().max(TRACK_MIN_HEIGHT);
                        state.message = format!(
                            "Added {} LV2 automation lanes on '{}'",
                            controls.len(),
                            track_name
                        );
                    }
                }
                if pending && let Some(action) = self.track_automation_lanes_action(track_name) {
                    self.try_send_engine(EngineMessage::Request(action));
                }
            }
            QueryReply::TrackLv2Midnam {
                track_name,
                note_names,
            } => {
                let mut state = self.state.write().expect("state lock poisoned");
                if let Some(piano) = &mut state.piano
                    && piano.track_idx == *track_name
                {
                    piano.midnam_note_names = note_names.clone();
                }
            }
            QueryReply::TrackClapNoteNames {
                track_name,
                note_names,
            } => {
                let mut state = self.state.write().expect("state lock poisoned");
                if let Some(piano) = &mut state.piano
                    && piano.track_idx == *track_name
                {
                    for (note, name) in note_names.iter() {
                        piano.midnam_note_names.insert(*note, name.clone());
                    }
                }
                self.workspace.set_midi_edit_midnam_note_names(note_names);
            }
            QueryReply::TrackPluginGraph {
                track_name,
                plugins,
                connections,
                connectable_connections,
            } => {
                tracing::info!(
                    %track_name,
                    plugins = plugins.len(),
                    connections = connections.len(),
                    connectable = connectable_connections.len(),
                    restore_in_progress = self.session_ops.session_restore_in_progress,
                    "received TrackPluginGraph"
                );
                let state = self.state.write().expect("state lock poisoned");
                let keep_restore_cache = self.session_ops.session_restore_in_progress
                    && plugins.is_empty()
                    && state
                        .plugin_graphs_by_track
                        .get(track_name)
                        .is_some_and(|(cached_plugins, _)| !cached_plugins.is_empty());
                if keep_restore_cache {
                    return Task::none();
                }
                let mut plugins = plugins.clone();
                drop(state);

                let pending_queries =
                    self.queue_pending_graph_automation_queries(track_name, &plugins);

                let mut state = self.state.write().expect("state lock poisoned");
                Self::preserve_plugin_graph_states_from_cache(track_name, &state, &mut plugins);
                state
                    .connectable_connections_by_track
                    .insert(track_name.clone(), connectable_connections.clone());
                if state.plugin_graph_clip.is_none()
                    && state.plugin_graph_track.as_deref() == Some(track_name.as_str())
                {
                    state.plugin_graph_track = Some(track_name.clone());
                    state.plugin_graph_plugins = plugins.clone();
                    state.plugin_graph_connections = connections.clone();
                    state.connectable_connections = connectable_connections.clone();
                    state.plugin_graph_selected_connections.clear();
                    state.plugin_graph_selected_connectable_connections.clear();
                    state
                        .plugin_graph_selected_plugins
                        .retain(|id| plugins.iter().any(|p| p.instance_id == *id));
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
                state
                    .plugin_graphs_by_track
                    .insert(track_name.clone(), (plugins, connections.clone()));
                drop(state);

                if !pending_queries.is_empty() {
                    return Task::batch(pending_queries);
                }

                if self.pending.pending_save_path.is_some() {
                    self.pending.pending_save_tracks.remove(track_name);
                    if let Some(task) = self.complete_pending_save(track_name) {
                        return task;
                    }
                }
            }
            _ => {}
        }
        Task::none()
    }
}
