use maolan_engine::{
    clap::{ClapPluginInfo, ClapPluginState},
    kind::Kind,
    message::Action,
    vst3::{Vst3PluginInfo, Vst3PluginState},
};
use serde_json::{Map, Value};
use std::collections::BTreeSet;
use tracing::{info, warn};

pub fn load_session_graph_restore_actions(
    session: &Value,
    valid_track_names: &BTreeSet<String>,
    clap_plugins: &[ClapPluginInfo],
    vst3_plugins: &[Vst3PluginInfo],
) -> Result<Vec<Action>, String> {
    let mut actions = Vec::new();
    let graphs = merged_session_graphs(session);
    push_track_plugin_graph_restore_actions(
        &mut actions,
        Some(&graphs),
        valid_track_names,
        clap_plugins,
        vst3_plugins,
    )?;
    Ok(actions)
}

pub(crate) fn merged_session_graphs(session: &Value) -> Value {
    let mut graphs = session
        .get("graphs")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    if let Some(tracks) = session.get("tracks").and_then(Value::as_array) {
        for track in tracks {
            let Some(name) = track.get("name").and_then(Value::as_str) else {
                continue;
            };
            let Some(legacy_graph) = legacy_track_graph(track) else {
                continue;
            };
            if graphs
                .get(name)
                .is_none_or(|graph| !graph_has_restore_payload(graph))
            {
                info!(track_name = name, "Using legacy track plugin graph");
                graphs.insert(name.to_string(), legacy_graph);
            }
        }
    }

    Value::Object(graphs)
}

fn legacy_track_graph(track: &Value) -> Option<Value> {
    let plugins = track
        .get("plugins")
        .and_then(Value::as_array)
        .filter(|plugins| !plugins.is_empty())
        .map(|_| track["plugins"].clone());
    let connections = track
        .get("connections")
        .and_then(Value::as_array)
        .filter(|connections| !connections.is_empty())
        .map(|_| track["connections"].clone());

    if plugins.is_none() && connections.is_none() {
        return None;
    }

    let mut graph = Map::new();
    graph.insert(
        "plugins".to_string(),
        plugins.unwrap_or_else(|| Value::Array(Vec::new())),
    );
    graph.insert(
        "connections".to_string(),
        connections.unwrap_or_else(|| Value::Array(Vec::new())),
    );
    Some(Value::Object(graph))
}

pub(crate) fn graph_has_restore_payload(graph: &Value) -> bool {
    graph
        .get("plugins")
        .and_then(Value::as_array)
        .is_some_and(|plugins| !plugins.is_empty())
        || graph
            .get("connections")
            .and_then(Value::as_array)
            .is_some_and(|connections| !connections.is_empty())
}

fn push_track_plugin_graph_restore_actions(
    actions: &mut Vec<Action>,
    graphs: Option<&Value>,
    valid_track_names: &BTreeSet<String>,
    clap_plugins: &[ClapPluginInfo],
    vst3_plugins: &[Vst3PluginInfo],
) -> Result<(), String> {
    use maolan_engine::message::PluginGraphNode;

    let Some(graphs) = graphs.and_then(Value::as_object) else {
        return Ok(());
    };

    for (track_name, graph) in graphs {
        if !valid_track_names.contains(track_name) {
            warn!(
                "Skipping plugin graph for unknown track '{}' (valid tracks: {:?})",
                track_name, valid_track_names
            );
            continue;
        }
        if !graph_has_restore_payload(graph) {
            continue;
        }
        actions.push(Action::TrackClearDefaultPassthrough {
            track_name: track_name.clone(),
        });

        let mut runtime_nodes: Vec<PluginGraphNode> = Vec::new();
        let mut next_instance_id = 0usize;

        if let Some(plugins) = graph.get("plugins").and_then(Value::as_array) {
            for plugin in plugins {
                let Some(uri) = plugin.get("uri").and_then(Value::as_str) else {
                    continue;
                };
                match plugin.get("format").and_then(Value::as_str) {
                    #[cfg(all(unix, not(target_os = "macos")))]
                    Some("LV2") => {
                        let instance_id = next_instance_id;
                        next_instance_id += 1;
                        runtime_nodes.push(PluginGraphNode::Lv2PluginInstance(instance_id));
                        actions.push(Action::TrackLoadLv2Plugin {
                            track_name: track_name.clone(),
                            plugin_uri: uri.to_string(),
                            instance_id: Some(instance_id),
                        });
                        if let Some(state) = lv2_state_from_json(&plugin["state"]) {
                            actions.push(Action::TrackSetLv2PluginState {
                                track_name: track_name.clone(),
                                instance_id,
                                state,
                            });
                        }
                    }
                    Some("CLAP") => {
                        let instance_id = next_instance_id;
                        next_instance_id += 1;
                        runtime_nodes.push(PluginGraphNode::ClapPluginInstance(instance_id));
                        if let Some(plugin_id) = resolve_clap_plugin_id(uri, clap_plugins) {
                            actions.push(Action::TrackLoadClapPlugin {
                                track_name: track_name.clone(),
                                plugin_id,
                                instance_id: Some(instance_id),
                            });
                            if let Some(state) = clap_state_from_json(&plugin["state"]) {
                                actions.push(Action::TrackClapRestoreState {
                                    track_name: track_name.clone(),
                                    instance_id,
                                    state,
                                });
                            }
                        }
                    }
                    Some("VST3") => {
                        let instance_id = next_instance_id;
                        next_instance_id += 1;
                        runtime_nodes.push(PluginGraphNode::Vst3PluginInstance(instance_id));
                        if let Some(plugin_id) = resolve_vst3_plugin_id(uri, vst3_plugins) {
                            actions.push(Action::TrackLoadVst3Plugin {
                                track_name: track_name.clone(),
                                plugin_id,
                                instance_id: Some(instance_id),
                            });
                            if let Some(state) = vst3_state_from_json(&plugin["state"]) {
                                actions.push(Action::TrackVst3RestoreState {
                                    track_name: track_name.clone(),
                                    instance_id,
                                    state,
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        if let Some(connections) = graph.get("connections").and_then(Value::as_array) {
            for connection in connections {
                let Some(kind) = parse_kind(connection.get("kind")) else {
                    continue;
                };
                let Some(from_node) = parse_plugin_node_with_runtime_nodes(
                    connection.get("from_node"),
                    &runtime_nodes,
                ) else {
                    continue;
                };
                let Some(to_node) =
                    parse_plugin_node_with_runtime_nodes(connection.get("to_node"), &runtime_nodes)
                else {
                    continue;
                };
                let from_port = connection
                    .get("from_port")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as usize;
                let to_port = connection
                    .get("to_port")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as usize;
                match kind {
                    Kind::Audio => actions.push(Action::TrackConnectPluginAudio {
                        track_name: track_name.clone(),
                        from_node,
                        from_port,
                        to_node,
                        to_port,
                    }),
                    Kind::MIDI => actions.push(Action::TrackConnectPluginMidi {
                        track_name: track_name.clone(),
                        from_node,
                        from_port,
                        to_node,
                        to_port,
                    }),
                }
            }
        }
    }
    Ok(())
}

fn parse_plugin_node_with_runtime_nodes(
    value: Option<&Value>,
    runtime_nodes: &[maolan_engine::message::PluginGraphNode],
) -> Option<maolan_engine::message::PluginGraphNode> {
    use maolan_engine::message::PluginGraphNode;
    let value = value?;
    if let Some(text) = value.as_str() {
        return match text {
            "TrackInput" => Some(PluginGraphNode::TrackInput),
            "TrackOutput" => Some(PluginGraphNode::TrackOutput),
            _ => None,
        };
    }
    let t = value.get("type").and_then(Value::as_str)?;
    match t {
        "track_input" => Some(PluginGraphNode::TrackInput),
        "track_output" => Some(PluginGraphNode::TrackOutput),
        #[cfg(all(unix, not(target_os = "macos")))]
        "lv2_plugin" => runtime_nodes
            .get(value.get("plugin_index").and_then(Value::as_u64)? as usize)
            .filter(|node| matches!(node, PluginGraphNode::Lv2PluginInstance(_)))
            .cloned(),
        "clap_plugin" => runtime_nodes
            .get(value.get("plugin_index").and_then(Value::as_u64)? as usize)
            .filter(|node| matches!(node, PluginGraphNode::ClapPluginInstance(_)))
            .cloned(),
        "vst3_plugin" => runtime_nodes
            .get(value.get("plugin_index").and_then(Value::as_u64)? as usize)
            .filter(|node| matches!(node, PluginGraphNode::Vst3PluginInstance(_)))
            .cloned(),
        _ => None,
    }
}

fn resolve_clap_plugin_id(stored: &str, clap_plugins: &[ClapPluginInfo]) -> Option<String> {
    if stored.contains("::") {
        // Old combined path::id URI; keep the ID half so sessions remain portable.
        return stored.split_once("::").map(|(_, id)| id.to_string());
    }
    if stored.contains('/') {
        // Legacy bare path; try to locate the matching scanned plugin by path.
        return clap_plugins
            .iter()
            .find(|info| info.path == stored)
            .map(|info| info.id.clone());
    }
    for info in clap_plugins {
        if info.id == stored {
            return Some(info.id.clone());
        }
    }
    None
}

fn resolve_vst3_plugin_id(stored: &str, vst3_plugins: &[Vst3PluginInfo]) -> Option<String> {
    if stored.contains('/') {
        // Legacy path; locate the matching scanned plugin and return its class ID.
        return vst3_plugins
            .iter()
            .find(|info| info.path == stored)
            .map(|info| info.id.clone());
    }
    vst3_plugins
        .iter()
        .find(|info| info.id == stored || info.path == stored)
        .map(|info| info.id.clone())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn lv2_state_from_json(value: &Value) -> Option<Vec<u8>> {
    if value.is_null() {
        return None;
    }
    if let Some(arr) = value.as_array() {
        let bytes = arr
            .iter()
            .filter_map(|item| item.as_u64().map(|value| value as u8))
            .collect::<Vec<_>>();
        if bytes.is_empty() {
            return None;
        }
        return Some(bytes);
    }
    serde_json::to_vec(value)
        .ok()
        .filter(|bytes| !bytes.is_empty())
}

fn clap_state_from_json(value: &Value) -> Option<ClapPluginState> {
    if value.is_null() {
        return None;
    }
    if let Some(arr) = value.as_array() {
        let bytes: Vec<u8> = arr
            .iter()
            .filter_map(|item| item.as_u64().map(|n| n as u8))
            .collect();
        if bytes.is_empty() {
            return None;
        }
        return Some(ClapPluginState { bytes });
    }
    serde_json::from_value(value.clone()).ok()
}

fn vst3_state_from_json(value: &Value) -> Option<Vst3PluginState> {
    if value.is_null() {
        return None;
    }
    serde_json::from_value(value.clone()).ok()
}

fn parse_kind(value: Option<&Value>) -> Option<Kind> {
    match value.and_then(Value::as_str) {
        Some("audio") | Some("Audio") => Some(Kind::Audio),
        Some("midi") | Some("MIDI") => Some(Kind::MIDI),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use maolan_engine::message::PluginGraphNode;
    use serde_json::json;

    fn valid_tracks(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn empty_top_level_graph_uses_legacy_track_connections() {
        let session = json!({
            "graphs": {
                "Synth": {
                    "plugins": [],
                    "connections": []
                }
            },
            "tracks": [{
                "name": "Synth",
                "connections": [{
                    "from_node": {"type": "track_input"},
                    "from_port": 0,
                    "to_node": {"type": "track_output"},
                    "to_port": 0,
                    "kind": "audio"
                }]
            }]
        });

        let actions =
            load_session_graph_restore_actions(&session, &valid_tracks(&["Synth"]), &[], &[])
                .unwrap();

        assert!(matches!(
            actions.as_slice(),
            [
                Action::TrackClearDefaultPassthrough { track_name },
                Action::TrackConnectPluginAudio {
                    track_name: route_track,
                    from_node: PluginGraphNode::TrackInput,
                    from_port: 0,
                    to_node: PluginGraphNode::TrackOutput,
                    to_port: 0
                }
            ] if track_name == "Synth" && route_track == "Synth"
        ));
    }

    #[test]
    fn truly_empty_graph_does_not_clear_default_passthrough() {
        let session = json!({
            "graphs": {
                "Synth": {
                    "plugins": [],
                    "connections": []
                }
            },
            "tracks": [{
                "name": "Synth"
            }]
        });

        let actions =
            load_session_graph_restore_actions(&session, &valid_tracks(&["Synth"]), &[], &[])
                .unwrap();

        assert!(actions.is_empty());
    }
}
