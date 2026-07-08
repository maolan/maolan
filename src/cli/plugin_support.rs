use maolan_engine::{
    clap::{ClapPluginInfo, ClapPluginState},
    kind::Kind,
    message::Action,
    vst3::{Vst3PluginInfo, Vst3PluginState},
};
use serde_json::Value;
use std::collections::BTreeSet;
use tracing::warn;

pub fn load_session_graph_restore_actions(
    session: &Value,
    valid_track_names: &BTreeSet<String>,
    clap_plugins: &[ClapPluginInfo],
    vst3_plugins: &[Vst3PluginInfo],
) -> Result<Vec<Action>, String> {
    let mut actions = Vec::new();
    push_track_plugin_graph_restore_actions(
        &mut actions,
        session.get("graphs"),
        valid_track_names,
        clap_plugins,
        vst3_plugins,
    )?;
    Ok(actions)
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
                    Some("CLAP") => {
                        let instance_id = next_instance_id;
                        next_instance_id += 1;
                        runtime_nodes.push(PluginGraphNode::ClapPluginInstance(instance_id));
                        if let Some(plugin_path) = resolve_clap_plugin_path(uri, clap_plugins) {
                            actions.push(Action::TrackLoadClapPlugin {
                                track_name: track_name.clone(),
                                plugin_path,
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
                        if let Some(plugin_path) = resolve_vst3_plugin_path(uri, vst3_plugins) {
                            actions.push(Action::TrackLoadVst3Plugin {
                                track_name: track_name.clone(),
                                plugin_path,
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

fn resolve_clap_plugin_path(stored: &str, clap_plugins: &[ClapPluginInfo]) -> Option<String> {
    if stored.contains("::") || stored.contains('/') {
        return Some(stored.to_string());
    }
    for info in clap_plugins {
        if let Some((_, id)) = info.path.split_once("::")
            && id == stored
        {
            return Some(info.path.clone());
        }
    }
    None
}

fn resolve_vst3_plugin_path(stored: &str, vst3_plugins: &[Vst3PluginInfo]) -> Option<String> {
    if stored.contains('/') {
        return Some(stored.to_string());
    }
    vst3_plugins
        .iter()
        .find(|info| info.path == stored || info.id == stored)
        .map(|info| info.path.clone())
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
