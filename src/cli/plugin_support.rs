use maolan_engine::{kind::Kind, message::Action};
use serde_json::Value;
use std::path::Path;

#[cfg(unix)]
pub fn load_session_plugin_restore_actions(
    session_dir: &Path,
    branch: &str,
    clap_plugins: &[maolan_engine::clap::ClapPluginInfo],
) -> Result<Vec<Action>, String> {
    let session = load_session_json(session_dir, branch)?;
    let mut actions = Vec::new();
    push_track_plugin_graph_restore_actions(&mut actions, session.get("graphs"), clap_plugins)?;
    Ok(actions)
}

#[cfg(not(unix))]
pub fn load_session_plugin_restore_actions(
    _session_dir: &Path,
    _clap_plugins: &[maolan_engine::clap::ClapPluginInfo],
) -> Result<Vec<Action>, String> {
    Ok(Vec::new())
}

#[cfg(unix)]
fn push_track_plugin_graph_restore_actions(
    actions: &mut Vec<Action>,
    graphs: Option<&Value>,
    clap_plugins: &[maolan_engine::clap::ClapPluginInfo],
) -> Result<(), String> {
    use maolan_engine::message::PluginGraphNode;

    let Some(graphs) = graphs.and_then(Value::as_object) else {
        return Ok(());
    };

    for (track_name, graph) in graphs {
        actions.push(Action::TrackClearDefaultPassthrough {
            track_name: track_name.clone(),
        });

        let mut runtime_nodes: Vec<PluginGraphNode> = Vec::new();
        let mut next_lv2_instance_id = 0usize;
        let mut next_clap_instance_id = 0usize;
        let mut next_vst3_instance_id = 0usize;

        if let Some(plugins) = graph.get("plugins").and_then(Value::as_array) {
            for plugin in plugins {
                let Some(uri) = plugin.get("uri").and_then(Value::as_str) else {
                    continue;
                };
                match plugin.get("format").and_then(Value::as_str) {
                    Some("LV2") => {
                        let instance_id = next_lv2_instance_id;
                        next_lv2_instance_id += 1;
                        runtime_nodes.push(PluginGraphNode::Lv2PluginInstance(instance_id));
                        actions.push(Action::TrackLoadLv2Plugin {
                            track_name: track_name.clone(),
                            plugin_uri: uri.to_string(),
                            instance_id: Some(instance_id),
                        });
                    }
                    Some("CLAP") => {
                        let instance_id = next_clap_instance_id;
                        next_clap_instance_id += 1;
                        runtime_nodes.push(PluginGraphNode::ClapPluginInstance(instance_id));
                        let plugin_path = resolve_clap_plugin_path(uri, clap_plugins);
                        if let Some(plugin_path) = plugin_path {
                            actions.push(Action::TrackLoadClapPlugin {
                                track_name: track_name.clone(),
                                plugin_path,
                                instance_id: Some(instance_id),
                            });
                        }
                    }
                    Some("VST3") => {
                        let instance_id = next_vst3_instance_id;
                        next_vst3_instance_id += 1;
                        runtime_nodes.push(PluginGraphNode::Vst3PluginInstance(instance_id));
                        actions.push(Action::TrackLoadVst3Plugin {
                            track_name: track_name.clone(),
                            plugin_path: uri.to_string(),
                            instance_id: Some(instance_id),
                        });
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

#[cfg(unix)]
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
        "plugin" => runtime_nodes
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

#[cfg(unix)]
fn resolve_clap_plugin_path(
    stored: &str,
    clap_plugins: &[maolan_engine::clap::ClapPluginInfo],
) -> Option<String> {
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

fn load_session_json(session_dir: &Path, branch: &str) -> Result<Value, String> {
    let session_path = session_dir.join(format!("{}.json", branch));
    let file = std::fs::File::open(&session_path)
        .map_err(|err| format!("Failed to open {}: {err}", session_path.display()))?;
    let reader = std::io::BufReader::new(file);
    serde_json::from_reader(reader)
        .map_err(|err| format!("Failed to parse {}: {err}", session_path.display()))
}

fn parse_kind(value: Option<&Value>) -> Option<Kind> {
    match value.and_then(Value::as_str) {
        Some("audio") | Some("Audio") => Some(Kind::Audio),
        Some("midi") | Some("MIDI") => Some(Kind::MIDI),
        _ => None,
    }
}
