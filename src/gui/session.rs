use super::{CLIENT, Maolan};
use crate::{
    message::Message,
    state::{Connection, ConnectionViewSelection},
};
use iced::{Length, Point, Task};
use maolan_engine::{
    kind::Kind,
    message::{Action, GlobalMidiLearnTarget, Message as EngineMessage},
};
use serde_json::{Value, json};
use std::{
    fs::{self, File},
    io::{self, BufReader, BufWriter},
    path::{Path, PathBuf},
};
use tracing::error;

impl Maolan {
    fn export_render_mode_to_json(mode: crate::message::ExportRenderMode) -> Value {
        Value::String(
            match mode {
                crate::message::ExportRenderMode::Mixdown => "mixdown",
                crate::message::ExportRenderMode::StemsPostFader => "stems_post_fader",
                crate::message::ExportRenderMode::StemsPreFader => "stems_pre_fader",
            }
            .to_string(),
        )
    }

    fn export_render_mode_from_json(value: Option<&Value>) -> crate::message::ExportRenderMode {
        match value.and_then(Value::as_str) {
            Some("stems_post_fader") => crate::message::ExportRenderMode::StemsPostFader,
            Some("stems_pre_fader") => crate::message::ExportRenderMode::StemsPreFader,
            _ => crate::message::ExportRenderMode::Mixdown,
        }
    }

    fn export_bit_depth_to_json(depth: crate::message::ExportBitDepth) -> Value {
        Value::String(
            match depth {
                crate::message::ExportBitDepth::Int16 => "int16",
                crate::message::ExportBitDepth::Int24 => "int24",
                crate::message::ExportBitDepth::Int32 => "int32",
                crate::message::ExportBitDepth::Float32 => "float32",
            }
            .to_string(),
        )
    }

    fn export_bit_depth_from_json(value: Option<&Value>) -> crate::message::ExportBitDepth {
        match value.and_then(Value::as_str) {
            Some("int16") => crate::message::ExportBitDepth::Int16,
            Some("int32") => crate::message::ExportBitDepth::Int32,
            Some("float32") => crate::message::ExportBitDepth::Float32,
            _ => crate::message::ExportBitDepth::Int24,
        }
    }

    fn export_mp3_mode_to_json(mode: crate::message::ExportMp3Mode) -> Value {
        Value::String(
            match mode {
                crate::message::ExportMp3Mode::Cbr => "cbr",
                crate::message::ExportMp3Mode::Vbr => "vbr",
            }
            .to_string(),
        )
    }

    fn export_mp3_mode_from_json(value: Option<&Value>) -> crate::message::ExportMp3Mode {
        match value.and_then(Value::as_str) {
            Some("vbr") => crate::message::ExportMp3Mode::Vbr,
            _ => crate::message::ExportMp3Mode::Cbr,
        }
    }

    fn export_normalize_mode_to_json(mode: crate::message::ExportNormalizeMode) -> Value {
        Value::String(
            match mode {
                crate::message::ExportNormalizeMode::Peak => "peak",
                crate::message::ExportNormalizeMode::Loudness => "loudness",
            }
            .to_string(),
        )
    }

    fn export_normalize_mode_from_json(
        value: Option<&Value>,
    ) -> crate::message::ExportNormalizeMode {
        match value.and_then(Value::as_str) {
            Some("loudness") => crate::message::ExportNormalizeMode::Loudness,
            _ => crate::message::ExportNormalizeMode::Peak,
        }
    }

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

    pub(super) fn save_template(&self, path: String) -> std::io::Result<()> {
        use tracing::info;
        info!("Saving template to: {}", path);
        let filename = "session.json";
        let template_root = PathBuf::from(path.clone());
        fs::create_dir_all(&path)?;
        fs::create_dir_all(template_root.join("plugins"))?;
        fs::create_dir_all(template_root.join("audio"))?;
        fs::create_dir_all(template_root.join("midi"))?;
        fs::create_dir_all(template_root.join("peaks"))?;
        info!("Created template directories in: {}", path);

        let mut p = template_root.clone();
        p.push(filename);
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

        // Serialize tracks but exclude clips
        let mut tracks_json = serde_json::to_value(&state.tracks).map_err(io::Error::other)?;
        if let Some(tracks) = tracks_json.as_array_mut() {
            for track in tracks.iter_mut() {
                // Clear audio clips
                if let Some(audio) = track.get_mut("audio").and_then(Value::as_object_mut) {
                    audio.insert("clips".to_string(), Value::Array(vec![]));
                }
                // Clear MIDI clips
                if let Some(midi) = track.get_mut("midi").and_then(Value::as_object_mut) {
                    midi.insert("clips".to_string(), Value::Array(vec![]));
                }
                if let Some(obj) = track.as_object_mut() {
                    obj.insert("frozen".to_string(), Value::Bool(false));
                    obj.insert("frozen_audio_backup".to_string(), Value::Array(vec![]));
                    obj.insert("frozen_midi_backup".to_string(), Value::Array(vec![]));
                    obj.insert("frozen_render_clip".to_string(), Value::Null);
                }
            }
        }

        #[cfg(all(unix, not(target_os = "macos")))]
        let graphs = {
            let mut graphs = serde_json::Map::new();
            for (track_name, (plugins, connections)) in &state.plugin_graphs_by_track {
                let id_to_index: std::collections::HashMap<usize, usize> = plugins
                    .iter()
                    .enumerate()
                    .map(|(idx, p)| (p.instance_id, idx))
                    .collect();
                let plugins_json: Vec<Value> = plugins
                    .iter()
                    .map(|p| {
                        let state_json = p.state.clone().unwrap_or(Value::Null);
                        json!({"format": p.format, "uri": p.uri, "state": state_json})
                    })
                    .collect();
                let conns_json: Vec<Value> = connections
                    .iter()
                    .filter_map(|c| {
                        let from_node = Self::plugin_node_to_json(&c.from_node, &id_to_index)?;
                        let to_node = Self::plugin_node_to_json(&c.to_node, &id_to_index)?;
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
            Value::Object(graphs)
        };
        #[cfg(target_os = "macos")]
        let graphs = Value::Object(serde_json::Map::new());

        let metadata_year = state.session_year.trim().parse::<u64>().ok();
        let metadata_track_number = state.session_track_number.trim().parse::<u64>().ok();
        let export_hw_out_ports: Vec<usize> = self.export_hw_out_ports.iter().copied().collect();

        let result = json!({
            "tracks": tracks_json,
            "connections": &state.connections,
            "graphs": graphs,
            "metadata": {
                "author": state.session_author.clone(),
                "album": state.session_album.clone(),
                "year": metadata_year,
                "track_number": metadata_track_number,
                "genre": state.session_genre.clone(),
            },
            "transport": {
                "loop_range_samples": self.loop_range_samples.map(|(start, end)| vec![start, end]),
                "loop_enabled": self.loop_enabled,
                "punch_range_samples": self.punch_range_samples.map(|(start, end)| vec![start, end]),
                "punch_enabled": self.punch_enabled,
                "sample_rate_hz": state.hw_sample_rate_hz,
                "tempo": state.tempo,
                "time_signature_num": state.time_signature_num,
                "time_signature_denom": state.time_signature_denom,
                "tempo_points": state
                    .tempo_points
                    .iter()
                    .map(|p| json!({ "sample": p.sample, "bpm": p.bpm }))
                    .collect::<Vec<_>>(),
                "time_signature_points": state
                    .time_signature_points
                    .iter()
                    .map(|p| {
                        json!({
                            "sample": p.sample,
                            "numerator": p.numerator,
                            "denominator": p.denominator
                        })
                    })
                    .collect::<Vec<_>>(),
            },
            "ui": {
                "tracks_width": tracks_width,
                "mixer_height": mixer_height,
                "zoom_visible_bars": self.zoom_visible_bars,
                "snap_mode": self.snap_mode,
                "midi_snap_mode": self.midi_snap_mode,
            },
            "export": {
                "sample_rate_hz": self.export_sample_rate_hz,
                "format_wav": self.export_format_wav,
                "format_mp3": self.export_format_mp3,
                "format_ogg": self.export_format_ogg,
                "format_flac": self.export_format_flac,
                "bit_depth": Self::export_bit_depth_to_json(self.export_bit_depth),
                "mp3_mode": Self::export_mp3_mode_to_json(self.export_mp3_mode),
                "mp3_bitrate_kbps": self.export_mp3_bitrate_kbps,
                "ogg_quality_input": self.export_ogg_quality_input,
                "render_mode": Self::export_render_mode_to_json(self.export_render_mode),
                "hw_out_ports": export_hw_out_ports,
                "realtime_fallback": self.export_realtime_fallback,
                "normalize": self.export_normalize,
                "normalize_mode": Self::export_normalize_mode_to_json(self.export_normalize_mode),
                "normalize_dbfs_input": self.export_normalize_dbfs_input,
                "normalize_lufs_input": self.export_normalize_lufs_input,
                "normalize_dbtp_input": self.export_normalize_dbtp_input,
                "normalize_tp_limiter": self.export_normalize_tp_limiter,
                "master_limiter": self.export_master_limiter,
                "master_limiter_ceiling_input": self.export_master_limiter_ceiling_input,
            },
            "midi_learn_global": {
                "play_pause": state.global_midi_learn_play_pause,
                "stop": state.global_midi_learn_stop,
                "record_toggle": state.global_midi_learn_record_toggle,
            }
        });
        serde_json::to_writer_pretty(file, &result)?;
        info!("Template saved successfully to: {}", path);
        Ok(())
    }

    pub(super) fn refresh_graph_then_save_track_template(
        &mut self,
        track_name: String,
        path: String,
    ) -> Task<Message> {
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            self.pending_save_path = Some(path.clone());
            self.pending_save_tracks = std::iter::once(track_name.clone()).collect();
            self.pending_save_clap_tracks = std::iter::once(track_name.clone()).collect();
            self.pending_save_is_template = false; // We'll handle this differently

            let tasks = vec![
                self.send(Action::TrackGetPluginGraph {
                    track_name: track_name.clone(),
                }),
                self.send(Action::TrackSnapshotAllClapStates {
                    track_name: track_name.clone(),
                }),
            ];

            // Store the track name for later use
            self.state.blocking_write().message =
                format!("Saving track template for {}", track_name);

            Task::batch(tasks)
        }

        #[cfg(target_os = "macos")]
        {
            self.save_track_as_template(&track_name, path)
        }
    }

    pub(super) fn load_track_template(
        &self,
        track_name: String,
        template_name: String,
    ) -> Task<Message> {
        use std::fs::File;
        use std::io::BufReader;
        use tracing::info;

        info!(
            "Loading track template '{}' for track '{}'",
            template_name, track_name
        );

        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let template_path = format!(
            "{}/.config/maolan/track_templates/{}/track.json",
            home, template_name
        );

        let file = match File::open(&template_path) {
            Ok(f) => f,
            Err(e) => {
                return Task::done(Message::Response(Err(format!(
                    "Failed to open template: {}",
                    e
                ))));
            }
        };

        let reader = BufReader::new(file);
        let json: serde_json::Value = match serde_json::from_reader(reader) {
            Ok(j) => j,
            Err(e) => {
                return Task::done(Message::Response(Err(format!(
                    "Failed to parse template: {}",
                    e
                ))));
            }
        };

        let mut restore_actions = vec![];

        // Load LV2 plugin graph
        #[cfg(all(unix, not(target_os = "macos")))]
        if let Some(graph) = json.get("graph").and_then(|g| g.as_object()) {
            let mut runtime_nodes = Vec::new();
            let mut next_lv2_instance_id = 0usize;
            let mut next_clap_instance_id = 0usize;

            if let Some(plugins) = graph.get("plugins").and_then(|p| p.as_array()) {
                for plugin in plugins {
                    if let Some(uri) = plugin.get("uri").and_then(|u| u.as_str()) {
                        match plugin.get("format").and_then(Value::as_str) {
                            Some("LV2") => {
                                let instance_id = next_lv2_instance_id;
                                next_lv2_instance_id += 1;
                                runtime_nodes.push(
                                    maolan_engine::message::PluginGraphNode::Lv2PluginInstance(
                                        instance_id,
                                    ),
                                );
                                restore_actions.push(Action::TrackLoadLv2Plugin {
                                    track_name: track_name.clone(),
                                    plugin_uri: uri.to_string(),
                                });
                                if let Some(state) =
                                    plugin.get("state").and_then(Self::lv2_state_from_json)
                                {
                                    restore_actions.push(Action::TrackSetLv2PluginState {
                                        track_name: track_name.clone(),
                                        instance_id,
                                        state,
                                    });
                                }
                            }
                            Some("CLAP") => {
                                let instance_id = next_clap_instance_id;
                                next_clap_instance_id += 1;
                                runtime_nodes.push(
                                    maolan_engine::message::PluginGraphNode::ClapPluginInstance(
                                        instance_id,
                                    ),
                                );
                                restore_actions.push(Action::TrackLoadClapPlugin {
                                    track_name: track_name.clone(),
                                    plugin_path: uri.to_string(),
                                });
                                if let Some(state) =
                                    plugin.get("state").and_then(Self::clap_state_from_json)
                                {
                                    restore_actions.push(Action::TrackClapRestoreState {
                                        track_name: track_name.clone(),
                                        instance_id,
                                        state,
                                    });
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }

            // Load plugin graph connections
            if let Some(connections) = graph.get("connections").and_then(|c| c.as_array()) {
                for conn in connections {
                    let Some(from_node) = Self::plugin_node_from_json_with_runtime_nodes(
                        &conn["from_node"],
                        &runtime_nodes,
                    ) else {
                        continue;
                    };
                    let Some(to_node) = Self::plugin_node_from_json_with_runtime_nodes(
                        &conn["to_node"],
                        &runtime_nodes,
                    ) else {
                        continue;
                    };
                    let Some(kind) = Self::kind_from_json(&conn["kind"]) else {
                        continue;
                    };

                    let from_port = conn["from_port"].as_u64().unwrap_or(0) as usize;
                    let to_port = conn["to_port"].as_u64().unwrap_or(0) as usize;

                    restore_actions.push(match kind {
                        Kind::Audio => Action::TrackConnectPluginAudio {
                            track_name: track_name.clone(),
                            from_node,
                            from_port,
                            to_node,
                            to_port,
                        },
                        Kind::MIDI => Action::TrackConnectPluginMidi {
                            track_name: track_name.clone(),
                            from_node,
                            from_port,
                            to_node,
                            to_port,
                        },
                    });
                }
            }
        }

        if restore_actions.is_empty() {
            Task::done(Message::None)
        } else {
            Self::restore_actions_task(restore_actions)
        }
    }

    pub(super) fn save_track_as_template(&self, track_name: &str, path: String) -> Task<Message> {
        use tracing::info;

        // Do all the work synchronously before spawning the task
        let result = (|| -> std::io::Result<()> {
            info!("Saving track template to: {}", path);
            let template_root = PathBuf::from(&path);
            fs::create_dir_all(&path)?;
            fs::create_dir_all(template_root.join("plugins"))?;
            info!("Created track template directories in: {}", path);

            let mut p = template_root.clone();
            p.push("track.json");
            let file = File::create(&p)?;

            let state = self.state.blocking_read();

            // Find the specific track
            let track = state
                .tracks
                .iter()
                .find(|t| t.name == track_name)
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Track not found"))?;

            // Serialize the track but exclude clips
            let mut track_json = serde_json::to_value(track).map_err(io::Error::other)?;
            if let Some(audio) = track_json.get_mut("audio").and_then(Value::as_object_mut) {
                audio.insert("clips".to_string(), Value::Array(vec![]));
            }
            if let Some(midi) = track_json.get_mut("midi").and_then(Value::as_object_mut) {
                midi.insert("clips".to_string(), Value::Array(vec![]));
            }

            // Get plugin graph for this track
            let graph = {
                #[cfg(all(unix, not(target_os = "macos")))]
                {
                    if let Some((plugins, connections)) =
                        state.plugin_graphs_by_track.get(track_name)
                    {
                        let id_to_index: std::collections::HashMap<usize, usize> = plugins
                            .iter()
                            .enumerate()
                            .map(|(idx, p)| (p.instance_id, idx))
                            .collect();
                        let plugins_json: Vec<Value> = plugins
                            .iter()
                            .map(|p| {
                                let state_json = p.state.clone().unwrap_or(Value::Null);
                                json!({"format": p.format, "uri": p.uri, "state": state_json})
                            })
                            .collect();
                        let conns_json: Vec<Value> = connections
                            .iter()
                            .filter_map(|c| {
                                let from_node =
                                    Self::plugin_node_to_json(&c.from_node, &id_to_index)?;
                                let to_node = Self::plugin_node_to_json(&c.to_node, &id_to_index)?;
                                Some(json!({
                                    "from_node": from_node,
                                    "from_port": c.from_port,
                                    "to_node": to_node,
                                    "to_port": c.to_port,
                                    "kind": Self::kind_to_json(c.kind),
                                }))
                            })
                            .collect();
                        json!({
                            "plugins": plugins_json,
                            "connections": conns_json,
                        })
                    } else {
                        Value::Null
                    }
                }
                #[cfg(target_os = "macos")]
                {
                    Value::Null
                }
            };

            // Get connections involving this track
            let track_connections: Vec<&crate::state::Connection> = state
                .connections
                .iter()
                .filter(|c| c.from_track == track_name || c.to_track == track_name)
                .collect();

            let result = json!({
                "track": track_json,
                "graph": graph,
                "connections": track_connections,
            });

            serde_json::to_writer_pretty(file, &result)?;
            info!("Track template saved successfully to: {}", path);
            Ok(())
        })();

        if let Err(e) = result {
            Task::done(Message::Response(Err(format!(
                "Failed to save track template: {}",
                e
            ))))
        } else {
            Task::done(Message::None)
        }
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
        fs::create_dir_all(session_root.join("pitch"))?;
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
                                && let Ok(computed) = Self::compute_audio_clip_peaks(&wav_path)
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
                                "peaks": peaks.as_ref(),
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
                            if in_session.exists() { in_session } else { p }
                        }
                    };
                    let basename = Path::new(name)
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("clip.mid");
                    let rel = format!("midi/{basename}");
                    let dst_path = session_root.join(&rel);
                    if src_path.exists()
                        && src_path.is_file()
                        && src_path != dst_path
                        && let Err(err) = fs::copy(&src_path, &dst_path)
                    {
                        error!(
                            "Failed to copy MIDI clip '{}' to '{}': {err}",
                            src_path.display(),
                            dst_path.display()
                        );
                    }
                    clip["name"] = Value::String(rel);
                }
            }
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        let graphs = {
            let mut graphs = serde_json::Map::new();
            for (track_name, (plugins, connections)) in &state.plugin_graphs_by_track {
                let id_to_index: std::collections::HashMap<usize, usize> = plugins
                    .iter()
                    .enumerate()
                    .map(|(idx, p)| (p.instance_id, idx))
                    .collect();
                let plugins_json: Vec<Value> = plugins
                    .iter()
                    .map(|p| {
                        let state_json = p.state.clone().unwrap_or(Value::Null);
                        json!({"format": p.format, "uri": p.uri, "state": state_json})
                    })
                    .collect();
                let conns_json: Vec<Value> = connections
                    .iter()
                    .filter_map(|c| {
                        let from_node = Self::plugin_node_to_json(&c.from_node, &id_to_index)?;
                        let to_node = Self::plugin_node_to_json(&c.to_node, &id_to_index)?;
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
            Value::Object(graphs)
        };
        #[cfg(target_os = "macos")]
        let graphs = Value::Object(serde_json::Map::new());
        let metadata_year = state.session_year.trim().parse::<u64>().ok();
        let metadata_track_number = state.session_track_number.trim().parse::<u64>().ok();
        let export_hw_out_ports: Vec<usize> = self.export_hw_out_ports.iter().copied().collect();
        let result = json!({
            "tracks": tracks_json,
            "connections": &state.connections,
            "graphs": graphs,
            "metadata": {
                "author": state.session_author.clone(),
                "album": state.session_album.clone(),
                "year": metadata_year,
                "track_number": metadata_track_number,
                "genre": state.session_genre.clone(),
            },
            "transport": {
                "loop_range_samples": self.loop_range_samples.map(|(start, end)| vec![start, end]),
                "loop_enabled": self.loop_enabled,
                "punch_range_samples": self.punch_range_samples.map(|(start, end)| vec![start, end]),
                "punch_enabled": self.punch_enabled,
                "sample_rate_hz": state.hw_sample_rate_hz,
                "tempo": state.tempo,
                "time_signature_num": state.time_signature_num,
                "time_signature_denom": state.time_signature_denom,
                "tempo_points": state
                    .tempo_points
                    .iter()
                    .map(|p| json!({ "sample": p.sample, "bpm": p.bpm }))
                    .collect::<Vec<_>>(),
                "time_signature_points": state
                    .time_signature_points
                    .iter()
                    .map(|p| {
                        json!({
                            "sample": p.sample,
                            "numerator": p.numerator,
                            "denominator": p.denominator
                        })
                    })
                    .collect::<Vec<_>>(),
            },
            "ui": {
                "tracks_width": tracks_width,
                "mixer_height": mixer_height,
                "zoom_visible_bars": self.zoom_visible_bars,
                "snap_mode": self.snap_mode,
                "midi_snap_mode": self.midi_snap_mode,
            },
            "export": {
                "sample_rate_hz": self.export_sample_rate_hz,
                "format_wav": self.export_format_wav,
                "format_mp3": self.export_format_mp3,
                "format_ogg": self.export_format_ogg,
                "format_flac": self.export_format_flac,
                "bit_depth": Self::export_bit_depth_to_json(self.export_bit_depth),
                "mp3_mode": Self::export_mp3_mode_to_json(self.export_mp3_mode),
                "mp3_bitrate_kbps": self.export_mp3_bitrate_kbps,
                "ogg_quality_input": self.export_ogg_quality_input,
                "render_mode": Self::export_render_mode_to_json(self.export_render_mode),
                "hw_out_ports": export_hw_out_ports,
                "realtime_fallback": self.export_realtime_fallback,
                "normalize": self.export_normalize,
                "normalize_mode": Self::export_normalize_mode_to_json(self.export_normalize_mode),
                "normalize_dbfs_input": self.export_normalize_dbfs_input,
                "normalize_lufs_input": self.export_normalize_lufs_input,
                "normalize_dbtp_input": self.export_normalize_dbtp_input,
                "normalize_tp_limiter": self.export_normalize_tp_limiter,
                "master_limiter": self.export_master_limiter,
                "master_limiter_ceiling_input": self.export_master_limiter_ceiling_input,
            },
            "midi_learn_global": {
                "play_pause": state.global_midi_learn_play_pause,
                "stop": state.global_midi_learn_stop,
                "record_toggle": state.global_midi_learn_record_toggle,
            }
        });
        serde_json::to_writer_pretty(file, &result)?;
        Ok(())
    }

    pub(super) fn load(&mut self, path: String) -> std::io::Result<Task<Message>> {
        let mut restore_actions: Vec<Action> = vec![
            Action::BeginSessionRestore,
            Action::SetSessionPath(path.clone()),
            Action::SetGlobalMidiLearnBinding {
                target: GlobalMidiLearnTarget::PlayPause,
                binding: None,
            },
            Action::SetGlobalMidiLearnBinding {
                target: GlobalMidiLearnTarget::Stop,
                binding: None,
            },
            Action::SetGlobalMidiLearnBinding {
                target: GlobalMidiLearnTarget::RecordToggle,
                binding: None,
            },
        ];
        let mut frozen_tracks: Vec<String> = Vec::new();
        let mut pending_vca_assignments: Vec<(String, String)> = Vec::new();
        let mut warnings: Vec<String> = Vec::new();
        let session_root = PathBuf::from(path.clone());
        self.pending_peak_file_loads.clear();
        self.pending_peak_rebuilds.clear();
        self.midi_clip_previews.clear();
        self.pending_midi_clip_previews.clear();
        self.pending_track_freeze_restore.clear();
        self.pending_track_freeze_bounce.clear();
        let existing_tracks: Vec<String> = self
            .state
            .blocking_read()
            .tracks
            .iter()
            .map(|t| t.name.clone())
            .collect();
        for name in existing_tracks {
            restore_actions.push(Action::RemoveTrack(name));
        }
        {
            let mut state = self.state.blocking_write();
            state.connections.clear();
            state.selected.clear();
            state.selected_clips.clear();
            state.connection_view_selection = ConnectionViewSelection::None;
            state.clap_plugins_by_track.clear();
            state.clap_states_by_track.clear();
            state.vst3_states_by_track.clear();
            state.global_midi_learn_play_pause = None;
            state.global_midi_learn_stop = None;
            state.global_midi_learn_record_toggle = None;
            state.session_author.clear();
            state.session_album.clear();
            state.session_year.clear();
            state.session_track_number.clear();
            state.session_genre.clear();
            #[cfg(all(unix, not(target_os = "macos")))]
            state.plugin_graphs_by_track.clear();
        }
        let filename = "session.json";
        let mut p = PathBuf::from(path.clone());
        p.push(filename);
        let file = File::open(&p)?;
        let reader = BufReader::new(file);
        let session: Value = serde_json::from_reader(reader)?;
        if let Some(global_ml) = session.get("midi_learn_global").and_then(Value::as_object) {
            if let Ok(binding) =
                serde_json::from_value::<Option<maolan_engine::message::MidiLearnBinding>>(
                    global_ml.get("play_pause").cloned().unwrap_or(Value::Null),
                )
                && binding.is_some()
            {
                restore_actions.push(Action::SetGlobalMidiLearnBinding {
                    target: GlobalMidiLearnTarget::PlayPause,
                    binding,
                });
            }
            if let Ok(binding) = serde_json::from_value::<
                Option<maolan_engine::message::MidiLearnBinding>,
            >(global_ml.get("stop").cloned().unwrap_or(Value::Null))
                && binding.is_some()
            {
                restore_actions.push(Action::SetGlobalMidiLearnBinding {
                    target: GlobalMidiLearnTarget::Stop,
                    binding,
                });
            }
            if let Ok(binding) =
                serde_json::from_value::<Option<maolan_engine::message::MidiLearnBinding>>(
                    global_ml
                        .get("record_toggle")
                        .cloned()
                        .unwrap_or(Value::Null),
                )
                && binding.is_some()
            {
                restore_actions.push(Action::SetGlobalMidiLearnBinding {
                    target: GlobalMidiLearnTarget::RecordToggle,
                    binding,
                });
            }
        }
        if let Some(metadata) = session.get("metadata").and_then(Value::as_object) {
            let mut state = self.state.blocking_write();
            state.session_author = metadata
                .get("author")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            state.session_album = metadata
                .get("album")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            state.session_year = metadata
                .get("year")
                .and_then(Value::as_u64)
                .map(|v| v.to_string())
                .or_else(|| {
                    metadata
                        .get("year")
                        .and_then(Value::as_str)
                        .and_then(|s| s.trim().parse::<u64>().ok().map(|v| v.to_string()))
                })
                .unwrap_or_default();
            state.session_track_number = metadata
                .get("track_number")
                .and_then(Value::as_u64)
                .map(|v| v.to_string())
                .or_else(|| {
                    metadata
                        .get("track_number")
                        .and_then(Value::as_str)
                        .and_then(|s| s.trim().parse::<u64>().ok().map(|v| v.to_string()))
                })
                .unwrap_or_default();
            state.session_genre = metadata
                .get("genre")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
        }
        if let Some(export) = session.get("export").and_then(Value::as_object) {
            self.export_sample_rate_hz = export
                .get("sample_rate_hz")
                .and_then(Value::as_u64)
                .map(|v| v.max(1) as u32)
                .unwrap_or(self.export_sample_rate_hz);
            self.export_format_wav = export
                .get("format_wav")
                .and_then(Value::as_bool)
                .unwrap_or(self.export_format_wav);
            self.export_format_mp3 = export
                .get("format_mp3")
                .and_then(Value::as_bool)
                .unwrap_or(self.export_format_mp3);
            self.export_format_ogg = export
                .get("format_ogg")
                .and_then(Value::as_bool)
                .unwrap_or(self.export_format_ogg);
            self.export_format_flac = export
                .get("format_flac")
                .and_then(Value::as_bool)
                .unwrap_or(self.export_format_flac);
            self.export_bit_depth = Self::export_bit_depth_from_json(export.get("bit_depth"));
            self.export_mp3_mode = Self::export_mp3_mode_from_json(export.get("mp3_mode"));
            self.export_mp3_bitrate_kbps = export
                .get("mp3_bitrate_kbps")
                .and_then(Value::as_u64)
                .map(|v| v.clamp(8, u16::MAX as u64) as u16)
                .unwrap_or(self.export_mp3_bitrate_kbps);
            self.export_ogg_quality_input = export
                .get("ogg_quality_input")
                .and_then(Value::as_str)
                .unwrap_or(&self.export_ogg_quality_input)
                .to_string();
            self.export_render_mode = Self::export_render_mode_from_json(export.get("render_mode"));
            self.export_hw_out_ports = export
                .get("hw_out_ports")
                .and_then(Value::as_array)
                .map(|ports| {
                    ports
                        .iter()
                        .filter_map(Value::as_u64)
                        .map(|port| port as usize)
                        .collect()
                })
                .unwrap_or_else(|| self.default_export_hw_out_ports());
            self.export_realtime_fallback = export
                .get("realtime_fallback")
                .and_then(Value::as_bool)
                .unwrap_or(self.export_realtime_fallback);
            self.export_normalize = export
                .get("normalize")
                .and_then(Value::as_bool)
                .unwrap_or(self.export_normalize);
            self.export_normalize_mode =
                Self::export_normalize_mode_from_json(export.get("normalize_mode"));
            self.export_normalize_dbfs_input = export
                .get("normalize_dbfs_input")
                .and_then(Value::as_str)
                .unwrap_or(&self.export_normalize_dbfs_input)
                .to_string();
            self.export_normalize_lufs_input = export
                .get("normalize_lufs_input")
                .and_then(Value::as_str)
                .unwrap_or(&self.export_normalize_lufs_input)
                .to_string();
            self.export_normalize_dbtp_input = export
                .get("normalize_dbtp_input")
                .and_then(Value::as_str)
                .unwrap_or(&self.export_normalize_dbtp_input)
                .to_string();
            self.export_normalize_tp_limiter = export
                .get("normalize_tp_limiter")
                .and_then(Value::as_bool)
                .unwrap_or(self.export_normalize_tp_limiter);
            self.export_master_limiter = export
                .get("master_limiter")
                .and_then(Value::as_bool)
                .unwrap_or(self.export_master_limiter);
            self.export_master_limiter_ceiling_input = export
                .get("master_limiter_ceiling_input")
                .and_then(Value::as_str)
                .unwrap_or(&self.export_master_limiter_ceiling_input)
                .to_string();
        } else {
            self.export_hw_out_ports = self.default_export_hw_out_ports();
        }
        self.normalize_export_hw_out_ports();
        if !self.export_mp3_supported_for_current_settings() {
            self.export_format_mp3 = false;
        }

        let transport = session.get("transport").ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "No 'transport' in session")
        })?;
        let parse_range = |key: &str| -> std::io::Result<Option<(usize, usize)>> {
            let value = transport.get(key).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("No 'transport.{key}' in session"),
                )
            })?;
            if value.is_null() {
                return Ok(None);
            }
            let arr = value.as_array().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("'transport.{key}' is not an array or null"),
                )
            })?;
            if arr.len() != 2 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("'transport.{key}' must have exactly 2 items"),
                ));
            }
            let start = arr[0].as_u64().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("'transport.{key}[0]' is not an unsigned integer"),
                )
            })? as usize;
            let end = arr[1].as_u64().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("'transport.{key}[1]' is not an unsigned integer"),
                )
            })? as usize;
            if end <= start {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("'transport.{key}' must satisfy end > start"),
                ));
            }
            Ok(Some((start, end)))
        };
        let parse_enabled = |key: &str| -> std::io::Result<bool> {
            transport.get(key).and_then(Value::as_bool).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("No boolean 'transport.{key}' in session"),
                )
            })
        };

        let loaded_loop_range = parse_range("loop_range_samples")?;
        let loaded_loop_enabled = parse_enabled("loop_enabled")?;
        let loaded_punch_range = parse_range("punch_range_samples")?;
        let loaded_punch_enabled = parse_enabled("punch_enabled")?;
        let loaded_sample_rate_hz = transport
            .get("sample_rate_hz")
            .and_then(Value::as_i64)
            .map(|v| v.max(1) as i32);

        self.loop_range_samples = loaded_loop_range;
        self.loop_enabled = loaded_loop_enabled;
        self.punch_range_samples = loaded_punch_range;
        self.punch_enabled = loaded_punch_enabled;

        restore_actions.push(Action::SetLoopRange(loaded_loop_range));
        restore_actions.push(Action::SetLoopEnabled(loaded_loop_enabled));
        restore_actions.push(Action::SetPunchRange(loaded_punch_range));
        restore_actions.push(Action::SetPunchEnabled(loaded_punch_enabled));

        if let Some(session_rate_hz) = loaded_sample_rate_hz {
            let (hw_loaded, hw_rate_hz) = {
                let st = self.state.blocking_read();
                (st.hw_loaded, st.hw_sample_rate_hz.max(1))
            };
            if hw_loaded && hw_rate_hz != session_rate_hz {
                warnings.push(format!(
                    "Session sample rate is {} Hz, hardware is {} Hz",
                    session_rate_hz, hw_rate_hz
                ));
            }
        }

        // Load transport timing fields if present, with sensible defaults.
        let loaded_tempo = transport
            .get("tempo")
            .and_then(Value::as_f64)
            .unwrap_or(120.0) as f32;
        let loaded_num = transport
            .get("time_signature_num")
            .and_then(Value::as_u64)
            .map(|n| n.clamp(1, 16) as u8)
            .unwrap_or(4);
        let loaded_denom = match transport
            .get("time_signature_denom")
            .and_then(Value::as_u64)
            .unwrap_or(4)
        {
            2 => 2,
            4 => 4,
            8 => 8,
            16 => 16,
            _ => 4,
        };
        let mut loaded_tempo_points = transport
            .get("tempo_points")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|entry| {
                        let sample = entry.get("sample")?.as_u64()? as usize;
                        let bpm = entry.get("bpm")?.as_f64()? as f32;
                        Some(crate::state::TempoPoint {
                            sample,
                            bpm: bpm.clamp(20.0, 300.0),
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut loaded_time_signature_points = transport
            .get("time_signature_points")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|entry| {
                        let sample = entry.get("sample")?.as_u64()? as usize;
                        let numerator = entry.get("numerator")?.as_u64()?.clamp(1, 16) as u8;
                        let denominator = match entry.get("denominator")?.as_u64()? {
                            2 => 2,
                            4 => 4,
                            8 => 8,
                            16 => 16,
                            _ => return None,
                        };
                        Some(crate::state::TimeSignaturePoint {
                            sample,
                            numerator,
                            denominator,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if loaded_tempo_points.is_empty() {
            loaded_tempo_points.push(crate::state::TempoPoint {
                sample: 0,
                bpm: loaded_tempo,
            });
        }
        if loaded_time_signature_points.is_empty() {
            loaded_time_signature_points.push(crate::state::TimeSignaturePoint {
                sample: 0,
                numerator: loaded_num,
                denominator: loaded_denom,
            });
        }
        loaded_tempo_points.sort_unstable_by_key(|p| p.sample);
        loaded_time_signature_points.sort_unstable_by_key(|p| p.sample);
        if loaded_tempo_points
            .first()
            .is_some_and(|first| first.sample != 0)
        {
            loaded_tempo_points.insert(
                0,
                crate::state::TempoPoint {
                    sample: 0,
                    bpm: loaded_tempo,
                },
            );
        }
        if loaded_time_signature_points
            .first()
            .is_some_and(|first| first.sample != 0)
        {
            loaded_time_signature_points.insert(
                0,
                crate::state::TimeSignaturePoint {
                    sample: 0,
                    numerator: loaded_num,
                    denominator: loaded_denom,
                },
            );
        }
        {
            let mut state = self.state.blocking_write();
            state.tempo = loaded_tempo;
            state.time_signature_num = loaded_num;
            state.time_signature_denom = loaded_denom;
            state.tempo_points = loaded_tempo_points;
            state.time_signature_points = loaded_time_signature_points;
        }
        self.tempo_input = format!("{:.2}", loaded_tempo);
        self.time_signature_num_input = loaded_num.to_string();
        self.time_signature_denom_input = loaded_denom.to_string();
        self.last_sent_tempo_bpm = Some(loaded_tempo as f64);
        self.last_sent_time_signature = Some((loaded_num as u16, loaded_denom as u16));
        restore_actions.push(Action::SetTempo(loaded_tempo as f64));
        restore_actions.push(Action::SetTimeSignature {
            numerator: loaded_num as u16,
            denominator: loaded_denom as u16,
        });

        {
            let mut state = self.state.blocking_write();
            state.pending_track_positions.clear();
            state.pending_track_heights.clear();

            let tracks_width = session["ui"]["tracks_width"].as_f64().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "No 'ui.tracks_width' in session",
                )
            })?;
            let mixer_height = session["ui"]["mixer_height"].as_f64().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "No 'ui.mixer_height' in session",
                )
            })?;
            state.tracks_width = Length::Fixed(tracks_width as f32);
            state.mixer_height = Length::Fixed(mixer_height as f32);
        }
        self.zoom_visible_bars = session["ui"]["zoom_visible_bars"]
            .as_f64()
            .map(|zoom| {
                (zoom as f32).clamp(
                    crate::gui::MIN_ZOOM_VISIBLE_BARS,
                    crate::gui::MAX_ZOOM_VISIBLE_BARS,
                )
            })
            .unwrap_or(self.zoom_visible_bars);
        if let Some(_mode) = session["ui"]["snap_mode"].as_str()
            && let Ok(mode) = serde_json::from_value::<crate::message::SnapMode>(
                session["ui"]["snap_mode"].clone(),
            )
        {
            self.snap_mode = mode;
        }
        if let Some(_mode) = session["ui"]["midi_snap_mode"].as_str()
            && let Ok(mode) = serde_json::from_value::<crate::message::SnapMode>(
                session["ui"]["midi_snap_mode"].clone(),
            )
        {
            self.midi_snap_mode = mode;
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
                let primary_audio_ins = track
                    .get("primary_audio_ins")
                    .and_then(|value| value.as_u64())
                    .map(|value| value as usize)
                    .unwrap_or(audio_ins);
                let primary_audio_outs = track
                    .get("primary_audio_outs")
                    .and_then(|value| value.as_u64())
                    .map(|value| value as usize)
                    .unwrap_or(audio_outs);
                restore_actions.push(Action::AddTrack {
                    name: name.clone(),
                    audio_ins: primary_audio_ins.min(audio_ins),
                    audio_outs: primary_audio_outs.min(audio_outs),
                    midi_ins,
                    midi_outs,
                });
                for _ in primary_audio_ins.min(audio_ins)..audio_ins {
                    restore_actions.push(Action::TrackAddAudioInput(name.clone()));
                }
                for _ in primary_audio_outs.min(audio_outs)..audio_outs {
                    restore_actions.push(Action::TrackAddAudioOutput(name.clone()));
                }
                if let Some(value) = track["level"].as_f64()
                    && value.is_finite()
                {
                    restore_actions.push(Action::TrackLevel(name.clone(), value as f32));
                }
                if let Some(value) = track["balance"].as_f64()
                    && value.is_finite()
                {
                    restore_actions.push(Action::TrackBalance(name.clone(), value as f32));
                }
                if let Some(value) = track["armed"].as_bool() {
                    if value {
                        restore_actions.push(Action::TrackToggleArm(name.clone()));
                    }
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "'armed' is not boolean",
                    ));
                }
                if let Some(value) = track["muted"].as_bool() {
                    if value {
                        restore_actions.push(Action::TrackToggleMute(name.clone()));
                    }
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "'muted' is not boolean",
                    ));
                }
                if let Some(value) = track["soloed"].as_bool() {
                    if value {
                        restore_actions.push(Action::TrackToggleSolo(name.clone()));
                    }
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "'soloed' is not boolean",
                    ));
                }
                if let Some(value) = track["input_monitor"].as_bool() {
                    if value {
                        restore_actions.push(Action::TrackToggleInputMonitor(name.clone()));
                    }
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "'input_monitor' is not boolean",
                    ));
                }
                if let Some(value) = track["disk_monitor"].as_bool() {
                    if !value {
                        restore_actions.push(Action::TrackToggleDiskMonitor(name.clone()));
                    }
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "'disk_monitor' is not boolean",
                    ));
                }
                if let Some(value) = track.get("midi_lane_channels")
                    && let Ok(channels) = serde_json::from_value::<Vec<Option<u8>>>(value.clone())
                {
                    for (lane, channel) in channels.into_iter().enumerate() {
                        restore_actions.push(Action::TrackSetMidiLaneChannel {
                            track_name: name.clone(),
                            lane,
                            channel,
                        });
                    }
                }
                if track["frozen"].as_bool().unwrap_or(false) {
                    frozen_tracks.push(name.clone());
                }
                if let Some(master_name) = track["vca_master"].as_str() {
                    pending_vca_assignments.push((name.clone(), master_name.to_string()));
                }
                if let Ok(binding) = serde_json::from_value::<
                    Option<maolan_engine::message::MidiLearnBinding>,
                >(track["midi_learn_volume"].clone())
                    && binding.is_some()
                {
                    restore_actions.push(Action::TrackSetMidiLearnBinding {
                        track_name: name.clone(),
                        target: maolan_engine::message::TrackMidiLearnTarget::Volume,
                        binding,
                    });
                }
                if let Ok(binding) = serde_json::from_value::<
                    Option<maolan_engine::message::MidiLearnBinding>,
                >(track["midi_learn_balance"].clone())
                    && binding.is_some()
                {
                    restore_actions.push(Action::TrackSetMidiLearnBinding {
                        track_name: name.clone(),
                        target: maolan_engine::message::TrackMidiLearnTarget::Balance,
                        binding,
                    });
                }
                if let Ok(binding) = serde_json::from_value::<
                    Option<maolan_engine::message::MidiLearnBinding>,
                >(track["midi_learn_mute"].clone())
                    && binding.is_some()
                {
                    restore_actions.push(Action::TrackSetMidiLearnBinding {
                        track_name: name.clone(),
                        target: maolan_engine::message::TrackMidiLearnTarget::Mute,
                        binding,
                    });
                }
                if let Ok(binding) = serde_json::from_value::<
                    Option<maolan_engine::message::MidiLearnBinding>,
                >(track["midi_learn_solo"].clone())
                    && binding.is_some()
                {
                    restore_actions.push(Action::TrackSetMidiLearnBinding {
                        track_name: name.clone(),
                        target: maolan_engine::message::TrackMidiLearnTarget::Solo,
                        binding,
                    });
                }
                if let Ok(binding) = serde_json::from_value::<
                    Option<maolan_engine::message::MidiLearnBinding>,
                >(track["midi_learn_arm"].clone())
                    && binding.is_some()
                {
                    restore_actions.push(Action::TrackSetMidiLearnBinding {
                        track_name: name.clone(),
                        target: maolan_engine::message::TrackMidiLearnTarget::Arm,
                        binding,
                    });
                }
                if let Ok(binding) = serde_json::from_value::<
                    Option<maolan_engine::message::MidiLearnBinding>,
                >(track["midi_learn_input_monitor"].clone())
                    && binding.is_some()
                {
                    restore_actions.push(Action::TrackSetMidiLearnBinding {
                        track_name: name.clone(),
                        target: maolan_engine::message::TrackMidiLearnTarget::InputMonitor,
                        binding,
                    });
                }
                if let Ok(binding) = serde_json::from_value::<
                    Option<maolan_engine::message::MidiLearnBinding>,
                >(track["midi_learn_disk_monitor"].clone())
                    && binding.is_some()
                {
                    restore_actions.push(Action::TrackSetMidiLearnBinding {
                        track_name: name.clone(),
                        target: maolan_engine::message::TrackMidiLearnTarget::DiskMonitor,
                        binding,
                    });
                }
                let frozen_audio_backup: Vec<crate::state::AudioClip> =
                    serde_json::from_value(track["frozen_audio_backup"].clone())
                        .unwrap_or_default();
                let frozen_midi_backup: Vec<crate::state::MIDIClip> =
                    serde_json::from_value(track["frozen_midi_backup"].clone()).unwrap_or_default();
                let frozen_render_clip =
                    track["frozen_render_clip"].as_str().map(|s| s.to_string());
                if !frozen_audio_backup.is_empty()
                    || !frozen_midi_backup.is_empty()
                    || frozen_render_clip.is_some()
                {
                    self.pending_track_freeze_restore.insert(
                        name.clone(),
                        (frozen_audio_backup, frozen_midi_backup, frozen_render_clip),
                    );
                }

                if let Some(audio_clips) = track["audio"]["clips"].as_array() {
                    for clip in audio_clips {
                        let clip_name = clip["name"].as_str().unwrap_or("").to_string();
                        let start = clip["start"].as_u64().unwrap_or(0) as usize;
                        let length = clip["length"].as_u64().unwrap_or(0) as usize;
                        let offset = clip["offset"].as_u64().unwrap_or(0) as usize;
                        let input_channel = clip["input_channel"].as_u64().unwrap_or(0) as usize;
                        let muted = clip["muted"].as_bool().unwrap_or(false);
                        let peaks_file = clip["peaks_file"].as_str().map(|s| s.to_string());
                        let fade_enabled = clip["fade_enabled"].as_bool().unwrap_or(true);
                        let fade_in_samples =
                            clip["fade_in_samples"].as_u64().unwrap_or(240) as usize;
                        let fade_out_samples =
                            clip["fade_out_samples"].as_u64().unwrap_or(240) as usize;

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

                            if let Some(peaks_rel) = peaks_file.as_ref() {
                                let peaks_path = session_root.join(peaks_rel);
                                if peaks_path.exists() && peaks_path.is_file() {
                                    let key = Self::audio_clip_key(
                                        &name, &clip_name, start, length, offset,
                                    );
                                    self.pending_peak_file_loads.insert(key, peaks_path);
                                }
                            }
                        }

                        if clip
                            .get("grouped_clips")
                            .and_then(serde_json::Value::as_array)
                            .is_some_and(|children| !children.is_empty())
                            && let Ok(grouped_clip) =
                                serde_json::from_value::<crate::state::AudioClip>(clip.clone())
                        {
                            restore_actions.push(Action::AddGroupedClip {
                                track_name: name.clone(),
                                kind: Kind::Audio,
                                audio_clip: Some(Self::audio_clip_to_data(&grouped_clip)),
                                midi_clip: None,
                            });
                            continue;
                        }

                        restore_actions.push(Action::AddClip {
                            name: clip_name,
                            track_name: name.clone(),
                            start,
                            length,
                            offset,
                            input_channel,
                            muted,
                            peaks_file: peaks_file.clone(),
                            kind: Kind::Audio,
                            fade_enabled,
                            fade_in_samples,
                            fade_out_samples,
                            source_name: clip["pitch_correction_source_name"]
                                .as_str()
                                .map(str::to_string),
                            source_offset: clip["pitch_correction_source_offset"]
                                .as_u64()
                                .map(|v| v as usize),
                            source_length: clip["pitch_correction_source_length"]
                                .as_u64()
                                .map(|v| v as usize),
                            preview_name: clip["pitch_correction_preview_name"]
                                .as_str()
                                .map(str::to_string),
                            pitch_correction_points: clip["pitch_correction_points"]
                                .as_array()
                                .map(|points| {
                                    points
                                        .iter()
                                        .map(|point| {
                                            maolan_engine::message::PitchCorrectionPointData {
                                                start_sample: point["start_sample"]
                                                    .as_u64()
                                                    .unwrap_or(0)
                                                    as usize,
                                                length_samples: point["length_samples"]
                                                    .as_u64()
                                                    .unwrap_or(0)
                                                    as usize,
                                                detected_midi_pitch: point["detected_midi_pitch"]
                                                    .as_f64()
                                                    .unwrap_or(0.0)
                                                    as f32,
                                                target_midi_pitch: point["target_midi_pitch"]
                                                    .as_f64()
                                                    .unwrap_or(0.0)
                                                    as f32,
                                                clarity: point["clarity"].as_f64().unwrap_or(0.0)
                                                    as f32,
                                            }
                                        })
                                        .collect()
                                })
                                .unwrap_or_default(),
                            pitch_correction_frame_likeness:
                                clip["pitch_correction_frame_likeness"]
                                    .as_f64()
                                    .map(|v| v as f32),
                            pitch_correction_inertia_ms: clip["pitch_correction_inertia_ms"]
                                .as_u64()
                                .map(|v| v as u16),
                            pitch_correction_formant_compensation:
                                clip["pitch_correction_formant_compensation"].as_bool(),
                            plugin_graph_json: clip
                                .get("plugin_graph_json")
                                .filter(|value| !value.is_null())
                                .cloned()
                                .or_else(|| {
                                    Some(Self::default_clip_plugin_graph_json(
                                        audio_ins, audio_outs,
                                    ))
                                }),
                        });
                    }
                }

                if let Some(midi_clips) = track["midi"]["clips"].as_array() {
                    for clip in midi_clips {
                        let clip_name = clip["name"].as_str().unwrap_or("").to_string();
                        let start = clip["start"].as_u64().unwrap_or(0) as usize;
                        let length = clip["length"].as_u64().unwrap_or(0) as usize;
                        let offset = clip["offset"].as_u64().unwrap_or(0) as usize;
                        let input_channel = clip["input_channel"].as_u64().unwrap_or(0) as usize;
                        let muted = clip["muted"].as_bool().unwrap_or(false);

                        if clip_name.trim().is_empty() {
                            warnings
                                .push(format!("Track '{}' has a MIDI clip with empty name", name));
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

                        if clip
                            .get("grouped_clips")
                            .and_then(serde_json::Value::as_array)
                            .is_some_and(|children| !children.is_empty())
                            && let Ok(grouped_clip) =
                                serde_json::from_value::<crate::state::MIDIClip>(clip.clone())
                        {
                            restore_actions.push(Action::AddGroupedClip {
                                track_name: name.clone(),
                                kind: Kind::MIDI,
                                audio_clip: None,
                                midi_clip: Some(Self::midi_clip_to_data(&grouped_clip)),
                            });
                            continue;
                        }

                        restore_actions.push(Action::AddClip {
                            name: clip_name,
                            track_name: name.clone(),
                            start,
                            length,
                            offset,
                            input_channel,
                            muted,
                            peaks_file: None,
                            kind: Kind::MIDI,
                            fade_enabled: true,
                            fade_in_samples: 240,
                            fade_out_samples: 240,
                            source_name: None,
                            source_offset: None,
                            source_length: None,
                            preview_name: None,
                            pitch_correction_points: vec![],
                            pitch_correction_frame_likeness: None,
                            pitch_correction_inertia_ms: None,
                            pitch_correction_formant_compensation: None,
                            plugin_graph_json: None,
                        });
                    }
                }
            }
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "'tracks' is not an array",
            ));
        }

        if let Some(connections_value) = session.get("connections") {
            match serde_json::from_value::<Vec<Connection>>(connections_value.clone()) {
                Ok(saved_connections) => {
                    for conn in saved_connections {
                        restore_actions.push(Action::Connect {
                            from_track: conn.from_track,
                            from_port: conn.from_port,
                            to_track: conn.to_track,
                            to_port: conn.to_port,
                            kind: conn.kind,
                        });
                    }
                }
                Err(e) => {
                    warnings.push(format!("Failed to parse 'connections': {e}"));
                }
            }
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        if let Some(graphs) = session["graphs"].as_object() {
            for (track_name, graph_v) in graphs {
                restore_actions.push(Action::TrackClearDefaultPassthrough {
                    track_name: track_name.clone(),
                });
                let Some(plugins_arr) = graph_v["plugins"].as_array() else {
                    continue;
                };
                let mut runtime_nodes = Vec::new();
                let mut next_lv2_instance_id = 0usize;
                let mut next_clap_instance_id = 0usize;
                for p in plugins_arr {
                    let Some(uri) = p["uri"].as_str() else {
                        continue;
                    };
                    match p.get("format").and_then(Value::as_str) {
                        Some("LV2") => {
                            let instance_id = next_lv2_instance_id;
                            next_lv2_instance_id += 1;
                            runtime_nodes.push(
                                maolan_engine::message::PluginGraphNode::Lv2PluginInstance(
                                    instance_id,
                                ),
                            );
                            restore_actions.push(Action::TrackLoadLv2Plugin {
                                track_name: track_name.clone(),
                                plugin_uri: uri.to_string(),
                            });
                            if let Some(state) = Self::lv2_state_from_json(&p["state"]) {
                                restore_actions.push(Action::TrackSetLv2PluginState {
                                    track_name: track_name.clone(),
                                    instance_id,
                                    state,
                                });
                            }
                        }
                        Some("CLAP") => {
                            let instance_id = next_clap_instance_id;
                            next_clap_instance_id += 1;
                            runtime_nodes.push(
                                maolan_engine::message::PluginGraphNode::ClapPluginInstance(
                                    instance_id,
                                ),
                            );
                            restore_actions.push(Action::TrackLoadClapPlugin {
                                track_name: track_name.clone(),
                                plugin_path: uri.to_string(),
                            });
                            if let Some(state) = Self::clap_state_from_json(&p["state"]) {
                                restore_actions.push(Action::TrackClapRestoreState {
                                    track_name: track_name.clone(),
                                    instance_id,
                                    state,
                                });
                            }
                        }
                        _ => {}
                    }
                }

                if let Some(connections_arr) = graph_v["connections"].as_array() {
                    for c in connections_arr {
                        let Some(from_node) = Self::plugin_node_from_json_with_runtime_nodes(
                            &c["from_node"],
                            &runtime_nodes,
                        ) else {
                            continue;
                        };
                        let Some(to_node) = Self::plugin_node_from_json_with_runtime_nodes(
                            &c["to_node"],
                            &runtime_nodes,
                        ) else {
                            continue;
                        };
                        let Some(kind) = Self::kind_from_json(&c["kind"]) else {
                            continue;
                        };
                        let from_port = c["from_port"].as_u64().unwrap_or(0) as usize;
                        let to_port = c["to_port"].as_u64().unwrap_or(0) as usize;
                        match kind {
                            Kind::Audio => restore_actions.push(Action::TrackConnectPluginAudio {
                                track_name: track_name.clone(),
                                from_node,
                                from_port,
                                to_node,
                                to_port,
                            }),
                            Kind::MIDI => restore_actions.push(Action::TrackConnectPluginMidi {
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
        for track_name in frozen_tracks {
            restore_actions.push(Action::TrackSetFrozen {
                track_name,
                frozen: true,
            });
        }
        for (track_name, master_track) in pending_vca_assignments {
            restore_actions.push(Action::TrackSetVcaMaster {
                track_name,
                master_track: Some(master_track),
            });
        }
        restore_actions.push(Action::EndSessionRestore);
        Ok(Self::restore_actions_task(restore_actions))
    }

    pub(super) fn refresh_graphs_then_save_template(&mut self, path: String) -> Task<Message> {
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let track_names: Vec<String> = self
                .state
                .blocking_read()
                .tracks
                .iter()
                .map(|t| t.name.clone())
                .collect();
            self.pending_save_path = Some(path);
            self.pending_save_tracks = track_names.iter().cloned().collect();
            self.pending_save_clap_tracks = track_names.iter().cloned().collect();
            self.pending_save_is_template = true;
            if self.pending_save_tracks.is_empty() {
                let Some(path) = self.pending_save_path.take() else {
                    return Task::none();
                };
                if let Err(e) = self.save_template(path.clone()) {
                    error!("{}", e);
                    self.state.blocking_write().message = format!("Failed to save template: {}", e);
                    return Task::none();
                }
                self.state.blocking_write().message = "Template saved".to_string();
                return Task::none();
            }
            let tasks = track_names
                .into_iter()
                .flat_map(|track_name| {
                    vec![
                        self.send(Action::TrackGetPluginGraph {
                            track_name: track_name.clone(),
                        }),
                        self.send(Action::TrackSnapshotAllClapStates { track_name }),
                    ]
                })
                .collect::<Vec<_>>();
            Task::batch(tasks)
        }
        #[cfg(target_os = "macos")]
        {
            let track_names: Vec<String> = self
                .state
                .blocking_read()
                .tracks
                .iter()
                .map(|t| t.name.clone())
                .collect();
            self.pending_save_path = Some(path);
            self.pending_save_tracks = track_names.iter().cloned().collect();
            self.pending_save_clap_tracks = track_names.iter().cloned().collect();
            self.pending_save_is_template = true;
            self.pending_save_vst3_states.clear();
            {
                let state = self.state.blocking_read();
                for track_name in &track_names {
                    if let Some((plugins, _)) = state.plugin_graphs_by_track.get(track_name) {
                        for plugin in plugins
                            .iter()
                            .filter(|plugin| plugin.format.eq_ignore_ascii_case("VST3"))
                        {
                            self.pending_save_vst3_states
                                .insert((track_name.clone(), plugin.instance_id));
                        }
                    }
                }
            }
            if self.pending_save_tracks.is_empty() {
                let Some(path) = self.pending_save_path.take() else {
                    return Task::none();
                };
                if let Err(e) = self.save_template(path.clone()) {
                    error!("{}", e);
                    self.state.blocking_write().message = format!("Failed to save template: {}", e);
                    return Task::none();
                }
                self.state.blocking_write().message = "Template saved".to_string();
                return Task::none();
            }
            let tasks = track_names
                .into_iter()
                .flat_map(|track_name| {
                    let mut actions = vec![
                        self.send(Action::TrackGetPluginGraph {
                            track_name: track_name.clone(),
                        }),
                        self.send(Action::TrackSnapshotAllClapStates {
                            track_name: track_name.clone(),
                        }),
                    ];
                    {
                        let state = self.state.blocking_read();
                        if let Some((plugins, _)) = state.plugin_graphs_by_track.get(&track_name) {
                            for plugin in plugins
                                .iter()
                                .filter(|plugin| plugin.format.eq_ignore_ascii_case("VST3"))
                            {
                                actions.push(self.send(Action::TrackVst3SnapshotState {
                                    track_name: track_name.clone(),
                                    instance_id: plugin.instance_id,
                                }));
                            }
                        }
                    }
                    actions
                })
                .collect::<Vec<_>>();
            Task::batch(tasks)
        }
    }

    pub(super) fn refresh_graphs_then_save(&mut self, path: String) -> Task<Message> {
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let track_names: Vec<String> = self
                .state
                .blocking_read()
                .tracks
                .iter()
                .map(|t| t.name.clone())
                .collect();
            self.pending_save_path = Some(path);
            self.pending_save_tracks = track_names.iter().cloned().collect();
            self.pending_save_clap_tracks = track_names.iter().cloned().collect();
            self.pending_save_is_template = false;
            if self.pending_save_tracks.is_empty() {
                let Some(path) = self.pending_save_path.take() else {
                    return Task::none();
                };
                if let Err(e) = self.save(path.clone()) {
                    error!("{}", e);
                    self.pending_exit_after_save = false;
                    self.state.blocking_write().message = format!("Failed to save session: {}", e);
                    return Task::none();
                }
                return self.send(Action::SetSessionPath(path));
            }
            let tasks = track_names
                .into_iter()
                .flat_map(|track_name| {
                    vec![
                        self.send(Action::TrackGetPluginGraph {
                            track_name: track_name.clone(),
                        }),
                        self.send(Action::TrackSnapshotAllClapStates { track_name }),
                    ]
                })
                .collect::<Vec<_>>();
            Task::batch(tasks)
        }
        #[cfg(target_os = "macos")]
        {
            let track_names: Vec<String> = self
                .state
                .blocking_read()
                .tracks
                .iter()
                .map(|t| t.name.clone())
                .collect();
            self.pending_save_path = Some(path);
            self.pending_save_tracks = track_names.iter().cloned().collect();
            self.pending_save_clap_tracks = track_names.iter().cloned().collect();
            self.pending_save_is_template = false;
            self.pending_save_vst3_states.clear();
            {
                let state = self.state.blocking_read();
                for track_name in &track_names {
                    if let Some((plugins, _)) = state.plugin_graphs_by_track.get(track_name) {
                        for plugin in plugins
                            .iter()
                            .filter(|plugin| plugin.format.eq_ignore_ascii_case("VST3"))
                        {
                            self.pending_save_vst3_states
                                .insert((track_name.clone(), plugin.instance_id));
                        }
                    }
                }
            }
            if self.pending_save_tracks.is_empty() {
                let Some(path) = self.pending_save_path.take() else {
                    return Task::none();
                };
                if let Err(e) = self.save(path.clone()) {
                    error!("{}", e);
                    self.pending_exit_after_save = false;
                    self.state.blocking_write().message = format!("Failed to save session: {}", e);
                    return Task::none();
                }
                return self.send(Action::SetSessionPath(path));
            }
            let tasks = track_names
                .into_iter()
                .flat_map(|track_name| {
                    let mut actions = vec![
                        self.send(Action::TrackGetPluginGraph {
                            track_name: track_name.clone(),
                        }),
                        self.send(Action::TrackSnapshotAllClapStates {
                            track_name: track_name.clone(),
                        }),
                    ];
                    {
                        let state = self.state.blocking_read();
                        if let Some((plugins, _)) = state.plugin_graphs_by_track.get(&track_name) {
                            for plugin in plugins
                                .iter()
                                .filter(|plugin| plugin.format.eq_ignore_ascii_case("VST3"))
                            {
                                actions.push(self.send(Action::TrackVst3SnapshotState {
                                    track_name: track_name.clone(),
                                    instance_id: plugin.instance_id,
                                }));
                            }
                        }
                    }
                    actions
                })
                .collect::<Vec<_>>();
            Task::batch(tasks)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn export_render_mode_to_json_mixdown() {
        let result = Maolan::export_render_mode_to_json(crate::message::ExportRenderMode::Mixdown);
        assert_eq!(result, json!("mixdown"));
    }

    #[test]
    fn export_render_mode_to_json_stems_post_fader() {
        let result =
            Maolan::export_render_mode_to_json(crate::message::ExportRenderMode::StemsPostFader);
        assert_eq!(result, json!("stems_post_fader"));
    }

    #[test]
    fn export_render_mode_to_json_stems_pre_fader() {
        let result =
            Maolan::export_render_mode_to_json(crate::message::ExportRenderMode::StemsPreFader);
        assert_eq!(result, json!("stems_pre_fader"));
    }

    #[test]
    fn export_render_mode_from_json_mixdown() {
        let result = Maolan::export_render_mode_from_json(Some(&json!("mixdown")));
        assert!(matches!(result, crate::message::ExportRenderMode::Mixdown));
    }

    #[test]
    fn export_render_mode_from_json_stems_post_fader() {
        let result = Maolan::export_render_mode_from_json(Some(&json!("stems_post_fader")));
        assert!(matches!(
            result,
            crate::message::ExportRenderMode::StemsPostFader
        ));
    }

    #[test]
    fn export_render_mode_from_json_stems_pre_fader() {
        let result = Maolan::export_render_mode_from_json(Some(&json!("stems_pre_fader")));
        assert!(matches!(
            result,
            crate::message::ExportRenderMode::StemsPreFader
        ));
    }

    #[test]
    fn export_render_mode_from_json_defaults_to_mixdown() {
        let result = Maolan::export_render_mode_from_json(Some(&json!("unknown")));
        assert!(matches!(result, crate::message::ExportRenderMode::Mixdown));
        let result = Maolan::export_render_mode_from_json(None);
        assert!(matches!(result, crate::message::ExportRenderMode::Mixdown));
    }

    #[test]
    fn export_bit_depth_to_json_int16() {
        let result = Maolan::export_bit_depth_to_json(crate::message::ExportBitDepth::Int16);
        assert_eq!(result, json!("int16"));
    }

    #[test]
    fn export_bit_depth_to_json_int24() {
        let result = Maolan::export_bit_depth_to_json(crate::message::ExportBitDepth::Int24);
        assert_eq!(result, json!("int24"));
    }

    #[test]
    fn export_bit_depth_to_json_int32() {
        let result = Maolan::export_bit_depth_to_json(crate::message::ExportBitDepth::Int32);
        assert_eq!(result, json!("int32"));
    }

    #[test]
    fn export_bit_depth_to_json_float32() {
        let result = Maolan::export_bit_depth_to_json(crate::message::ExportBitDepth::Float32);
        assert_eq!(result, json!("float32"));
    }

    #[test]
    fn export_bit_depth_from_json_int16() {
        let result = Maolan::export_bit_depth_from_json(Some(&json!("int16")));
        assert!(matches!(result, crate::message::ExportBitDepth::Int16));
    }

    #[test]
    fn export_bit_depth_from_json_int32() {
        let result = Maolan::export_bit_depth_from_json(Some(&json!("int32")));
        assert!(matches!(result, crate::message::ExportBitDepth::Int32));
    }

    #[test]
    fn export_bit_depth_from_json_float32() {
        let result = Maolan::export_bit_depth_from_json(Some(&json!("float32")));
        assert!(matches!(result, crate::message::ExportBitDepth::Float32));
    }

    #[test]
    fn export_bit_depth_from_json_defaults_to_int24() {
        let result = Maolan::export_bit_depth_from_json(Some(&json!("unknown")));
        assert!(matches!(result, crate::message::ExportBitDepth::Int24));
        let result = Maolan::export_bit_depth_from_json(None);
        assert!(matches!(result, crate::message::ExportBitDepth::Int24));
    }

    #[test]
    fn export_mp3_mode_to_json_cbr() {
        let result = Maolan::export_mp3_mode_to_json(crate::message::ExportMp3Mode::Cbr);
        assert_eq!(result, json!("cbr"));
    }

    #[test]
    fn export_mp3_mode_to_json_vbr() {
        let result = Maolan::export_mp3_mode_to_json(crate::message::ExportMp3Mode::Vbr);
        assert_eq!(result, json!("vbr"));
    }

    #[test]
    fn export_mp3_mode_from_json_cbr() {
        let result = Maolan::export_mp3_mode_from_json(Some(&json!("cbr")));
        assert!(matches!(result, crate::message::ExportMp3Mode::Cbr));
    }

    #[test]
    fn export_mp3_mode_from_json_vbr() {
        let result = Maolan::export_mp3_mode_from_json(Some(&json!("vbr")));
        assert!(matches!(result, crate::message::ExportMp3Mode::Vbr));
    }

    #[test]
    fn export_mp3_mode_from_json_defaults_to_cbr() {
        let result = Maolan::export_mp3_mode_from_json(Some(&json!("unknown")));
        assert!(matches!(result, crate::message::ExportMp3Mode::Cbr));
        let result = Maolan::export_mp3_mode_from_json(None);
        assert!(matches!(result, crate::message::ExportMp3Mode::Cbr));
    }

    #[test]
    fn export_normalize_mode_to_json_peak() {
        let result =
            Maolan::export_normalize_mode_to_json(crate::message::ExportNormalizeMode::Peak);
        assert_eq!(result, json!("peak"));
    }

    #[test]
    fn export_normalize_mode_to_json_loudness() {
        let result =
            Maolan::export_normalize_mode_to_json(crate::message::ExportNormalizeMode::Loudness);
        assert_eq!(result, json!("loudness"));
    }

    #[test]
    fn export_normalize_mode_from_json_peak() {
        let result = Maolan::export_normalize_mode_from_json(Some(&json!("peak")));
        assert!(matches!(result, crate::message::ExportNormalizeMode::Peak));
    }

    #[test]
    fn export_normalize_mode_from_json_loudness() {
        let result = Maolan::export_normalize_mode_from_json(Some(&json!("loudness")));
        assert!(matches!(
            result,
            crate::message::ExportNormalizeMode::Loudness
        ));
    }

    #[test]
    fn export_normalize_mode_from_json_defaults_to_peak() {
        let result = Maolan::export_normalize_mode_from_json(Some(&json!("unknown")));
        assert!(matches!(result, crate::message::ExportNormalizeMode::Peak));
        let result = Maolan::export_normalize_mode_from_json(None);
        assert!(matches!(result, crate::message::ExportNormalizeMode::Peak));
    }

    #[test]
    fn kind_to_json_audio() {
        assert_eq!(Maolan::kind_to_json(Kind::Audio), json!("audio"));
    }

    #[test]
    fn kind_to_json_midi() {
        assert_eq!(Maolan::kind_to_json(Kind::MIDI), json!("midi"));
    }

    #[test]
    fn kind_from_json_audio() {
        let result = Maolan::kind_from_json(&json!("audio"));
        assert_eq!(result, Some(Kind::Audio));
    }

    #[test]
    fn kind_from_json_midi() {
        let result = Maolan::kind_from_json(&json!("midi"));
        assert_eq!(result, Some(Kind::MIDI));
    }

    #[test]
    fn kind_from_json_unknown() {
        let result = Maolan::kind_from_json(&json!("unknown"));
        assert_eq!(result, None);
    }
}
