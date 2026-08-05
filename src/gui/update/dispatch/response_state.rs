use super::*;
use crate::state::SlotPlayState;
use maolan_engine::message::{
    ConnectableConnection, ConnectableRef, PluginGraphConnection, PluginGraphNode,
};

impl Maolan {
    pub(super) fn handle_response_engine_state_action(&mut self, action: &Action) -> bool {
        match action {
            Action::Connect {
                from_track,
                from_port,
                to_track,
                to_port,
                kind,
            } => {
                let mut state = self.state.blocking_write();
                if from_track == to_track && from_track != "hw:in" && to_track != "hw:out" {
                    return true;
                }
                state.connections.push(crate::state::Connection {
                    from_track: from_track.clone(),
                    from_port: *from_port,
                    to_track: to_track.clone(),
                    to_port: *to_port,
                    kind: *kind,
                });
                true
            }
            Action::Disconnect {
                from_track,
                from_port,
                to_track,
                to_port,
                kind,
            } => {
                let mut state = self.state.blocking_write();
                let original_len = state.connections.len();
                if from_track == to_track && from_track != "hw:in" && to_track != "hw:out" {
                    return true;
                }

                state.connections.retain(|conn| {
                    !(conn.from_track == from_track.as_str()
                        && conn.from_port == *from_port
                        && conn.to_track == to_track.as_str()
                        && conn.to_port == *to_port
                        && conn.kind == *kind)
                });
                if state.connections.len() < original_len {
                    state.message = format!("Disconnected {} from {}", from_track, to_track);
                }
                true
            }
            Action::TrackConnectPluginAudio {
                track_name,
                from_node,
                from_port,
                to_node,
                to_port,
            }
            | Action::TrackConnectPluginMidi {
                track_name,
                from_node,
                from_port,
                to_node,
                to_port,
            } => {
                let mut state = self.state.blocking_write();
                let connection = PluginGraphConnection {
                    from_node: from_node.clone(),
                    from_port: *from_port,
                    to_node: to_node.clone(),
                    to_port: *to_port,
                    kind: action_to_kind(action),
                };
                let cached_plugins = state
                    .plugin_graphs_by_track
                    .entry(track_name.clone())
                    .or_default();
                if !cached_plugins
                    .1
                    .iter()
                    .any(|existing| existing == &connection)
                {
                    cached_plugins.1.push(connection.clone());
                }
                if state.plugin_graph_clip.is_none()
                    && state.plugin_graph_track.as_deref() == Some(track_name.as_str())
                    && !state
                        .plugin_graph_connections
                        .iter()
                        .any(|existing| existing == &connection)
                {
                    state.plugin_graph_connections.push(connection);
                }
                state.message = format!(
                    "Connected {} {}:{} -> {}:{}",
                    track_name,
                    plugin_node_label(from_node),
                    from_port,
                    plugin_node_label(to_node),
                    to_port
                );
                true
            }
            Action::TrackDisconnectPluginAudio {
                track_name,
                from_node,
                from_port,
                to_node,
                to_port,
            }
            | Action::TrackDisconnectPluginMidi {
                track_name,
                from_node,
                from_port,
                to_node,
                to_port,
            } => {
                let mut state = self.state.blocking_write();
                let kind = action_to_kind(action);
                if let Some((_, cached_connections)) =
                    state.plugin_graphs_by_track.get_mut(track_name)
                {
                    cached_connections.retain(|conn| {
                        !(conn.from_node == *from_node
                            && conn.from_port == *from_port
                            && conn.to_node == *to_node
                            && conn.to_port == *to_port
                            && conn.kind == kind)
                    });
                }
                if state.plugin_graph_clip.is_none()
                    && state.plugin_graph_track.as_deref() == Some(track_name.as_str())
                {
                    state.plugin_graph_connections.retain(|conn| {
                        !(conn.from_node == *from_node
                            && conn.from_port == *from_port
                            && conn.to_node == *to_node
                            && conn.to_port == *to_port
                            && conn.kind == kind)
                    });
                }
                state.message = format!(
                    "Disconnected {} {}:{} -> {}:{}",
                    track_name,
                    plugin_node_label(from_node),
                    from_port,
                    plugin_node_label(to_node),
                    to_port
                );
                true
            }
            Action::TrackConnectAudio {
                track_name,
                from,
                from_port,
                to,
                to_port,
            }
            | Action::TrackConnectMidi {
                track_name,
                from,
                from_port,
                to,
                to_port,
            } => {
                let mut state = self.state.blocking_write();
                let connection = ConnectableConnection {
                    from: from.clone(),
                    from_port: *from_port,
                    to: to.clone(),
                    to_port: *to_port,
                    kind: action_to_kind(action),
                };
                let track_connections = state
                    .connectable_connections_by_track
                    .entry(track_name.clone())
                    .or_default();
                if !track_connections
                    .iter()
                    .any(|existing| existing == &connection)
                {
                    track_connections.push(connection.clone());
                }
                if state.plugin_graph_clip.is_none()
                    && state.plugin_graph_track.as_deref() == Some(track_name.as_str())
                    && !state
                        .connectable_connections
                        .iter()
                        .any(|existing| existing == &connection)
                {
                    state.connectable_connections.push(connection);
                }
                state.message = format!(
                    "Connected {} {}:{} -> {}:{}",
                    track_name,
                    connectable_label(from),
                    from_port,
                    connectable_label(to),
                    to_port
                );
                true
            }
            Action::TrackDisconnectAudio {
                track_name,
                from,
                from_port,
                to,
                to_port,
            }
            | Action::TrackDisconnectMidi {
                track_name,
                from,
                from_port,
                to,
                to_port,
            } => {
                let mut state = self.state.blocking_write();
                let kind = action_to_kind(action);
                if let Some(cached) = state.connectable_connections_by_track.get_mut(track_name) {
                    cached.retain(|conn| {
                        !(conn.from == *from
                            && conn.from_port == *from_port
                            && conn.to == *to
                            && conn.to_port == *to_port
                            && conn.kind == kind)
                    });
                }
                if state.plugin_graph_clip.is_none()
                    && state.plugin_graph_track.as_deref() == Some(track_name.as_str())
                {
                    let original_len = state.connectable_connections.len();
                    state.connectable_connections.retain(|conn| {
                        !(conn.from == *from
                            && conn.from_port == *from_port
                            && conn.to == *to
                            && conn.to_port == *to_port
                            && conn.kind == kind)
                    });
                    if state.connectable_connections.len() < original_len {
                        state.message = format!(
                            "Disconnected {} {}:{} -> {}:{}",
                            track_name,
                            connectable_label(from),
                            from_port,
                            connectable_label(to),
                            to_port
                        );
                    }
                } else {
                    state.message = format!(
                        "Disconnected {} {}:{} -> {}:{}",
                        track_name,
                        connectable_label(from),
                        from_port,
                        connectable_label(to),
                        to_port
                    );
                }
                true
            }
            Action::OpenAudioDevice {
                device,
                input_device: _,
                sample_rate_hz,
                bits,
                exclusive,
                period_frames,
                nperiods,
                sync_mode,
                actual_period_frames,
                input_channels,
                output_channels,
                bytes_per_frame,
            } => {
                let mut state = self.state.blocking_write();
                let configured_period_frames = if *actual_period_frames > 0 {
                    *actual_period_frames
                } else {
                    *period_frames
                };
                state.message = format!(
                    "Opened device {} (rate={} Hz, bits={}, channels={}/{}, period_frames={}, periods={}, bytes_per_frame={}, exclusive={}, sync_mode={})",
                    device,
                    sample_rate_hz,
                    bits,
                    input_channels,
                    output_channels,
                    configured_period_frames,
                    nperiods,
                    bytes_per_frame,
                    exclusive,
                    sync_mode,
                );
                state.hw_loaded = true;
                state.hw_sample_rate_hz = *sample_rate_hz;
                state.oss_period_frames = configured_period_frames.max(1);
                state.oss_nperiods = (*nperiods).max(1);
                true
            }
            Action::OpenMidiInputDevice(s) => {
                let mut state = self.state.blocking_write();
                if !state.opened_midi_in_hw.iter().any(|name| name == s) {
                    state.opened_midi_in_hw.push(s.clone());
                }
                state
                    .midi_hw_labels
                    .entry(s.clone())
                    .or_insert_with(|| platform::kernel_midi_label(s));
                state.message = format!("Opened MIDI input {s}");
                true
            }
            Action::OpenMidiOutputDevice(s) => {
                let mut state = self.state.blocking_write();
                if !state.opened_midi_out_hw.iter().any(|name| name == s) {
                    state.opened_midi_out_hw.push(s.clone());
                }
                state
                    .midi_hw_labels
                    .entry(s.clone())
                    .or_insert_with(|| platform::kernel_midi_label(s));
                state.message = format!("Opened MIDI output {s}");
                true
            }
            Action::HWInfo {
                channels,
                rate,
                input,
            } => {
                if *rate > 0 {
                    self.playback_rate_hz = *rate as f64;
                }
                let mut state = self.state.blocking_write();
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
                true
            }
            Action::JackGraph(graph) => {
                let mut state = self.state.blocking_write();
                state.jack_graph = graph.clone();
                state.jack_session_routing = Some(graph.clone());
                state.message = format!(
                    "JACK graph: {} ports, {} connections",
                    graph.ports.len(),
                    graph.connections.len()
                );
                true
            }
            Action::JackConnect {
                source,
                destination,
            } => {
                let mut state = self.state.blocking_write();
                state.jack_connecting = None;
                state.message = format!("Connected JACK {source} -> {destination}");
                true
            }
            Action::JackDisconnect {
                source,
                destination,
            } => {
                let mut state = self.state.blocking_write();
                state.message = format!("Disconnected JACK {source} -> {destination}");
                true
            }
            Action::MidiLearnMappingsReport { lines } => {
                let report = lines.join(" | ");
                self.midi_mappings_report_lines = lines.clone();
                let mut state = self.state.blocking_write();
                state.message = format!("MIDI mappings: {}", report);
                true
            }
            Action::ClearAllMidiLearnBindings => {
                self.midi_mappings_report_lines.clear();
                let mut state = self.state.blocking_write();
                state.global_midi_learn_play_pause = None;
                state.global_midi_learn_stop = None;
                state.global_midi_learn_record_toggle = None;
                state.session_midi_learn_slots.clear();
                state.session_midi_learn_scenes.clear();
                state.session_midi_learn_stop_track.clear();
                state.session_midi_learn_stop_all = None;
                for track in &mut state.tracks {
                    track.midi_learn_volume = None;
                    track.midi_learn_balance = None;
                    track.midi_learn_mute = None;
                    track.midi_learn_solo = None;
                    track.midi_learn_arm = None;
                    track.midi_learn_input_monitor = None;
                    track.midi_learn_disk_monitor = None;
                }
                state.message = "Cleared all MIDI mappings".to_string();
                true
            }
            Action::SetModulators(modulators) => {
                self.modulators = modulators.clone().into_iter().map(Into::into).collect();
                true
            }
            Action::TrackSetFolder {
                track_name,
                is_folder,
            } => {
                let mut state = self.state.blocking_write();
                if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name) {
                    // The master track can never be turned into a folder.
                    if *is_folder && track.is_master {
                        return true;
                    }
                    track.is_folder = *is_folder;
                }
                true
            }
            Action::TrackSetParent {
                track_name,
                parent_name,
            } => {
                let mut state = self.state.blocking_write();
                if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name) {
                    track.parent_track = parent_name.clone();
                }
                true
            }
            Action::TrackToggleFolder { track_name } => {
                let mut state = self.state.blocking_write();
                if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name) {
                    track.folder_open = !track.folder_open;
                }
                true
            }
            Action::SessionRuntimeReport {
                track_name,
                scene_index,
                state: engine_state,
                play_position_samples,
                elapsed_samples,
            } => {
                let mut state = self.state.blocking_write();
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
                true
            }
            _ => false,
        }
    }
}

fn action_to_kind(action: &maolan_engine::message::Action) -> maolan_engine::kind::Kind {
    match action {
        maolan_engine::message::Action::TrackConnectAudio { .. }
        | maolan_engine::message::Action::TrackDisconnectAudio { .. }
        | maolan_engine::message::Action::TrackConnectPluginAudio { .. }
        | maolan_engine::message::Action::TrackDisconnectPluginAudio { .. } => {
            maolan_engine::kind::Kind::Audio
        }
        maolan_engine::message::Action::TrackConnectMidi { .. }
        | maolan_engine::message::Action::TrackDisconnectMidi { .. }
        | maolan_engine::message::Action::TrackConnectPluginMidi { .. }
        | maolan_engine::message::Action::TrackDisconnectPluginMidi { .. } => {
            maolan_engine::kind::Kind::MIDI
        }
        _ => maolan_engine::kind::Kind::Audio,
    }
}

fn connectable_label(connectable: &ConnectableRef) -> String {
    match connectable {
        ConnectableRef::TrackInput => "track input".to_string(),
        ConnectableRef::TrackOutput => "track output".to_string(),
        ConnectableRef::ChildTrack(name) => format!("child '{name}'"),
        ConnectableRef::ClapPlugin(id) => format!("CLAP plugin {id}"),
        ConnectableRef::Vst3Plugin(id) => format!("VST3 plugin {id}"),
        #[cfg(unix)]
        ConnectableRef::Lv2Plugin(id) => format!("LV2 plugin {id}"),
    }
}

fn plugin_node_label(node: &PluginGraphNode) -> String {
    match node {
        PluginGraphNode::TrackInput => "track input".to_string(),
        PluginGraphNode::TrackOutput => "track output".to_string(),
        PluginGraphNode::ClapPluginInstance(id) => format!("CLAP plugin {id}"),
        PluginGraphNode::Vst3PluginInstance(id) => format!("VST3 plugin {id}"),
        #[cfg(unix)]
        PluginGraphNode::Lv2PluginInstance(id) => format!("LV2 plugin {id}"),
    }
}
