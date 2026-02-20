use super::{CLIENT, Maolan};
use crate::{
    message::Message,
    state::ConnectionViewSelection,
};
use iced::{Length, Point, Task};
use maolan_engine::{
    kind::Kind,
    message::{Action, Message as EngineMessage},
};
use serde_json::{Value, json};
use std::{
    fs::{self, File},
    io::{self, BufReader, BufWriter},
    path::{Path, PathBuf},
};
use tracing::error;

impl Maolan {
    pub(super) fn restore_actions_task(actions: Vec<Action>) -> Task<Message> {
        Task::perform(
            async move {
                for action in actions {
                    CLIENT
                        .send(EngineMessage::Request(action))
                        .await
                        .map_err(|e| e.to_string())?;
                }
                Ok(())
            },
            Message::SendMessageFinished,
        )
    }

    pub(super) fn save(&self, path: String) -> std::io::Result<()> {
        let filename = "session.json";
        let session_root = PathBuf::from(path.clone());
        let mut p = session_root.clone();
        p.push(filename);
        fs::create_dir_all(&path)?;
        fs::create_dir_all(session_root.join("plugins"))?;
        fs::create_dir_all(session_root.join("audio"))?;
        fs::create_dir_all(session_root.join("midi"))?;
        fs::create_dir_all(session_root.join("peaks"))?;
        let file = File::create(&p)?;
        let state = self.state.blocking_read();
        let tracks_width = match state.tracks_width {
            Length::Fixed(v) => v,
            _ => 200.0,
        };
        let mixer_height = match state.mixer_height {
            Length::Fixed(v) => v,
            _ => 300.0,
        };
        let mut tracks_json = serde_json::to_value(&state.tracks).map_err(io::Error::other)?;
        if let Some(tracks) = tracks_json.as_array_mut() {
            for (track_idx, track) in tracks.iter_mut().enumerate() {
                let track_name = track["name"].as_str().unwrap_or("").to_string();
                let state_track = state.tracks.get(track_idx);
                if let Some(audio_clips) = track
                    .get_mut("audio")
                    .and_then(|m| m.get_mut("clips"))
                    .and_then(Value::as_array_mut)
                {
                    for (clip_idx, clip) in audio_clips.iter_mut().enumerate() {
                        let clip_name = clip
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        let mut peaks = state_track
                            .and_then(|t| t.audio.clips.get(clip_idx))
                            .map(|c| c.peaks.clone())
                            .unwrap_or_default();
                        if peaks.is_empty() && clip_name.to_ascii_lowercase().ends_with(".wav") {
                            let wav_path = session_root.join(&clip_name);
                            if wav_path.exists()
                                && let Ok(computed) =
                                    Self::compute_audio_clip_peaks(&wav_path, 512)
                            {
                                peaks = computed;
                            }
                        }
                        if !peaks.is_empty() {
                            let rel = Self::build_peak_file_rel(&track_name, clip_idx, &clip_name);
                            let abs = session_root.join(&rel);
                            let peak_json = json!({
                                "version": 1,
                                "track": track_name,
                                "clip": clip_name,
                                "peaks": peaks,
                            });
                            let peak_file = File::create(&abs)?;
                            serde_json::to_writer_pretty(BufWriter::new(peak_file), &peak_json)?;
                            clip["peaks_file"] = Value::String(rel);
                        }
                    }
                }
                let Some(midi_clips) = track
                    .get_mut("midi")
                    .and_then(|m| m.get_mut("clips"))
                    .and_then(Value::as_array_mut)
                else {
                    continue;
                };
                for clip in midi_clips {
                    let Some(name) = clip.get("name").and_then(Value::as_str) else {
                        continue;
                    };
                    let lower = name.to_ascii_lowercase();
                    if !(lower.ends_with(".mid") || lower.ends_with(".midi")) {
                        continue;
                    }
                    let src_path = {
                        let p = PathBuf::from(name);
                        if p.is_absolute() {
                            p
                        } else {
                            let in_session = session_root.join(&p);
                            if in_session.exists() {
                                in_session
                            } else {
                                p
                            }
                        }
                    };
                    let basename = Path::new(name)
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("clip.mid");
                    let rel = format!("midi/{basename}");
                    let dst_path = session_root.join(&rel);
                    if src_path.exists() && src_path.is_file() && src_path != dst_path {
                        let _ = fs::copy(&src_path, &dst_path);
                    }
                    clip["name"] = Value::String(rel);
                }
            }
        }
        let mut graphs = serde_json::Map::new();
        for (track_name, (plugins, connections)) in &state.lv2_graphs_by_track {
            let id_to_index: std::collections::HashMap<usize, usize> = plugins
                .iter()
                .enumerate()
                .map(|(idx, p)| (p.instance_id, idx))
                .collect();
            let plugins_json: Vec<Value> = plugins
                .iter()
                .map(|p| json!({"uri":p.uri, "state": Self::lv2_state_to_json(&p.state)}))
                .collect();
            let conns_json: Vec<Value> = connections
                .iter()
                .filter_map(|c| {
                    let from_node = Self::lv2_node_to_json(&c.from_node, &id_to_index)?;
                    let to_node = Self::lv2_node_to_json(&c.to_node, &id_to_index)?;
                    Some(json!({
                        "from_node": from_node,
                        "from_port": c.from_port,
                        "to_node": to_node,
                        "to_port": c.to_port,
                        "kind": Self::kind_to_json(c.kind),
                    }))
                })
                .collect();
            graphs.insert(
                track_name.clone(),
                json!({
                    "plugins": plugins_json,
                    "connections": conns_json,
                }),
            );
        }
        let result = json!({
            "tracks": tracks_json,
            "connections": &state.connections,
            "graphs": Value::Object(graphs),
            "ui": {
                "tracks_width": tracks_width,
                "mixer_height": mixer_height,
            }
        });
        serde_json::to_writer_pretty(file, &result)?;
        Ok(())
    }

    pub(super) fn load(&mut self, path: String) -> std::io::Result<Task<Message>> {
        let mut tasks = vec![];
        let mut restore_actions: Vec<Action> = vec![Action::SetSessionPath(path.clone())];
        let mut warnings: Vec<String> = Vec::new();
        let session_root = PathBuf::from(path.clone());
        self.pending_audio_peaks.clear();
        let existing_tracks: Vec<String> = self
            .state
            .blocking_read()
            .tracks
            .iter()
            .map(|t| t.name.clone())
            .collect();
        for name in existing_tracks {
            tasks.push(self.send(Action::RemoveTrack(name)));
        }
        {
            let mut state = self.state.blocking_write();
            state.connections.clear();
            state.selected.clear();
            state.selected_clips.clear();
            state.connection_view_selection = ConnectionViewSelection::None;
            state.lv2_graphs_by_track.clear();
        }
        let filename = "session.json";
        let mut p = PathBuf::from(path.clone());
        p.push(filename);
        let file = File::open(&p)?;
        let reader = BufReader::new(file);
        let session: Value = serde_json::from_reader(reader)?;

        {
            let mut state = self.state.blocking_write();
            state.pending_track_positions.clear();
            state.pending_track_heights.clear();

            let tracks_width = session["ui"]["tracks_width"].as_f64().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "No 'ui.tracks_width' in session")
            })?;
            let mixer_height = session["ui"]["mixer_height"].as_f64().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "No 'ui.mixer_height' in session")
            })?;
            state.tracks_width = Length::Fixed(tracks_width as f32);
            state.mixer_height = Length::Fixed(mixer_height as f32);
        }

        if let Some(arr) = session["tracks"].as_array() {
            for track in arr {
                let name = {
                    if let Some(value) = track["name"].as_str() {
                        value.to_string()
                    } else {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "No 'name' in track",
                        ));
                    }
                };

                if let (Some(x), Some(y)) = (
                    track["position"]["x"].as_f64(),
                    track["position"]["y"].as_f64(),
                ) {
                    self.state
                        .blocking_write()
                        .pending_track_positions
                        .insert(name.clone(), Point::new(x as f32, y as f32));
                }

                if let Some(height) = track["height"].as_f64() {
                    self.state
                        .blocking_write()
                        .pending_track_heights
                        .insert(name.clone(), height as f32);
                }

                let audio_ins = {
                    if let Some(value) = track["audio"]["ins"].as_u64() {
                        value as usize
                    } else {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "No 'audio_ins' in track",
                        ));
                    }
                };
                let midi_ins = {
                    if let Some(value) = track["midi"]["ins"].as_u64() {
                        value as usize
                    } else {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "No 'midi_ins' in track",
                        ));
                    }
                };
                let audio_outs = {
                    if let Some(value) = track["audio"]["outs"].as_u64() {
                        value as usize
                    } else {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "No 'audio_outs' in track",
                        ));
                    }
                };
                let midi_outs = {
                    if let Some(value) = track["midi"]["outs"].as_u64() {
                        value as usize
                    } else {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "No 'midi_outs' in track",
                        ));
                    }
                };
                tasks.push(self.send(Action::AddTrack {
                    name: name.clone(),
                    audio_ins,
                    audio_outs,
                    midi_ins,
                    midi_outs,
                }));
                if let Some(value) = track["armed"].as_bool() {
                    if value {
                        tasks.push(self.send(Action::TrackToggleArm(name.clone())));
                    }
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "'armed' is not boolean",
                    ));
                }
                if let Some(value) = track["muted"].as_bool() {
                    if value {
                        tasks.push(self.send(Action::TrackToggleMute(name.clone())));
                    }
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "'muted' is not boolean",
                    ));
                }
                if let Some(value) = track["soloed"].as_bool() {
                    if value {
                        tasks.push(self.send(Action::TrackToggleSolo(name.clone())));
                    }
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "'soloed' is not boolean",
                    ));
                }

                if let Some(audio_clips) = track["audio"]["clips"].as_array() {
                    for clip in audio_clips {
                        let clip_name = clip["name"].as_str().unwrap_or("").to_string();
                        let start = clip["start"].as_u64().unwrap_or(0) as usize;
                        let length = clip["length"].as_u64().unwrap_or(0) as usize;
                        let offset = clip["offset"].as_u64().unwrap_or(0) as usize;
                        let peaks_file = clip["peaks_file"].as_str().map(|s| s.to_string());

                        if clip_name.trim().is_empty() {
                            warnings.push(format!(
                                "Track '{}' has an audio clip with empty name",
                                name
                            ));
                        }
                        if length == 0 {
                            warnings.push(format!(
                                "Audio clip '{}' on track '{}' has zero length",
                                clip_name, name
                            ));
                        }
                        if clip_name.to_ascii_lowercase().ends_with(".wav") {
                            let wav_path = session_root.join(&clip_name);
                            if !wav_path.exists() {
                                warnings.push(format!(
                                    "Missing WAV file for clip '{}': {}",
                                    clip_name,
                                    wav_path.display()
                                ));
                            } else if !wav_path.is_file() {
                                warnings.push(format!(
                                    "WAV clip path is not a file '{}': {}",
                                    clip_name,
                                    wav_path.display()
                                ));
                            }

                            let mut peaks = vec![];
                            if let Some(peaks_rel) = peaks_file.as_ref() {
                                let peaks_path = session_root.join(peaks_rel);
                                if peaks_path.exists()
                                    && let Ok(loaded) = Self::read_clip_peaks_file(&peaks_path)
                                {
                                    peaks = loaded;
                                }
                            }
                            if peaks.is_empty()
                                && let Ok(computed) =
                                    Self::compute_audio_clip_peaks(&wav_path, 512)
                            {
                                peaks = computed;
                            }
                            if !peaks.is_empty() {
                                let key = Self::audio_clip_key(
                                    &name, &clip_name, start, length, offset,
                                );
                                self.pending_audio_peaks.insert(key, peaks);
                            }
                        }

                        tasks.push(self.send(Action::AddClip {
                            name: clip_name,
                            track_name: name.clone(),
                            start,
                            length,
                            offset,
                            kind: Kind::Audio,
                        }));
                    }
                }

                if let Some(midi_clips) = track["midi"]["clips"].as_array() {
                    for clip in midi_clips {
                        let clip_name = clip["name"].as_str().unwrap_or("").to_string();
                        let start = clip["start"].as_u64().unwrap_or(0) as usize;
                        let length = clip["length"].as_u64().unwrap_or(0) as usize;
                        let offset = clip["offset"].as_u64().unwrap_or(0) as usize;

                        if clip_name.trim().is_empty() {
                            warnings.push(format!(
                                "Track '{}' has a MIDI clip with empty name",
                                name
                            ));
                        }
                        if length == 0 {
                            warnings.push(format!(
                                "MIDI clip '{}' on track '{}' has zero length",
                                clip_name, name
                            ));
                        }
                        if clip_name.to_ascii_lowercase().ends_with(".mid")
                            || clip_name.to_ascii_lowercase().ends_with(".midi")
                        {
                            let mid_path = session_root.join(&clip_name);
                            if !mid_path.exists() {
                                warnings.push(format!(
                                    "Missing MIDI file for clip '{}': {}",
                                    clip_name,
                                    mid_path.display()
                                ));
                            } else if !mid_path.is_file() {
                                warnings.push(format!(
                                    "MIDI clip path is not a file '{}': {}",
                                    clip_name,
                                    mid_path.display()
                                ));
                            }
                        }

                        tasks.push(self.send(Action::AddClip {
                            name: clip_name,
                            track_name: name.clone(),
                            start,
                            length,
                            offset,
                            kind: Kind::MIDI,
                        }));
                    }
                }
            }
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "'tracks' is not an array",
            ));
        }
        if let Some(graphs) = session["graphs"].as_object() {
            for (track_name, graph_v) in graphs {
                restore_actions.push(Action::TrackClearDefaultPassthrough {
                    track_name: track_name.clone(),
                });
                let Some(plugins_arr) = graph_v["plugins"].as_array() else {
                    continue;
                };
                let mut plugin_count = 0usize;
                for p in plugins_arr {
                    let Some(uri) = p["uri"].as_str() else {
                        continue;
                    };
                    let plugin_index = plugin_count;
                    restore_actions.push(Action::TrackLoadLv2Plugin {
                        track_name: track_name.clone(),
                        plugin_uri: uri.to_string(),
                    });
                    if let Some(state) = Self::lv2_state_from_json(&p["state"]) {
                        restore_actions.push(Action::TrackSetLv2PluginState {
                            track_name: track_name.clone(),
                            instance_id: plugin_index,
                            state,
                        });
                    }
                    plugin_count += 1;
                }

                if let Some(connections_arr) = graph_v["connections"].as_array() {
                    for c in connections_arr {
                        let Some(from_node) = Self::lv2_node_from_json(&c["from_node"]) else {
                            continue;
                        };
                        let Some(to_node) = Self::lv2_node_from_json(&c["to_node"]) else {
                            continue;
                        };
                        let Some(kind) = Self::kind_from_json(&c["kind"]) else {
                            continue;
                        };
                        let from_port = c["from_port"].as_u64().unwrap_or(0) as usize;
                        let to_port = c["to_port"].as_u64().unwrap_or(0) as usize;
                        let valid_node = |n: &maolan_engine::message::Lv2GraphNode| match n {
                            maolan_engine::message::Lv2GraphNode::PluginInstance(idx) => {
                                *idx < plugin_count
                            }
                            _ => true,
                        };
                        if !valid_node(&from_node) || !valid_node(&to_node) {
                            continue;
                        }
                        match kind {
                            Kind::Audio => restore_actions.push(Action::TrackConnectLv2Audio {
                                track_name: track_name.clone(),
                                from_node,
                                from_port,
                                to_node,
                                to_port,
                            }),
                            Kind::MIDI => restore_actions.push(Action::TrackConnectLv2Midi {
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
        }
        if warnings.is_empty() {
            self.state.blocking_write().message = "Session loaded".to_string();
        } else {
            let shown = warnings.len().min(8);
            let mut msg = format!("Session loaded with {} warning(s):", warnings.len());
            for warning in warnings.iter().take(shown) {
                msg.push_str("\n- ");
                msg.push_str(warning);
            }
            if warnings.len() > shown {
                msg.push_str(&format!("\n- ... and {} more", warnings.len() - shown));
            }
            self.state.blocking_write().message = msg;
        }
        if !restore_actions.is_empty() {
            tasks.push(Self::restore_actions_task(restore_actions));
        }
        Ok(Task::batch(tasks))
    }

    pub(super) fn refresh_graphs_then_save(&mut self, path: String) -> Task<Message> {
        let track_names: Vec<String> = self
            .state
            .blocking_read()
            .tracks
            .iter()
            .map(|t| t.name.clone())
            .collect();
        self.pending_save_path = Some(path);
        self.pending_save_tracks = track_names.iter().cloned().collect();
        if self.pending_save_tracks.is_empty() {
            let Some(path) = self.pending_save_path.take() else {
                return Task::none();
            };
            if let Err(e) = self.save(path.clone()) {
                error!("{}", e);
                return Task::none();
            }
            return self.send(Action::SetSessionPath(path));
        }
        let tasks = track_names
            .into_iter()
            .map(|track_name| self.send(Action::TrackGetLv2Graph { track_name }))
            .collect::<Vec<_>>();
        Task::batch(tasks)
    }
}
