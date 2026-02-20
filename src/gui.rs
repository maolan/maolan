use crate::{
    add_track, connections, hw, menu,
    message::{DraggedClip, Message, Show},
    state::{ConnectionViewSelection, HW, Resizing, State, StateData, Track, View},
    toolbar,
    ui_timing::{
        PLAYHEAD_UPDATE_INTERVAL, RECORDING_PREVIEW_PEAKS_UPDATE_INTERVAL,
        RECORDING_PREVIEW_UPDATE_INTERVAL,
    },
    workspace,
};
use iced::futures::{Stream, StreamExt, io, stream};
use iced::keyboard::Event as KeyEvent;
use iced::widget::{Id, button, column, container, row, scrollable, text, text_input};
use iced::{Length, Point, Size, Subscription, Task, event, keyboard, mouse, window};
use maolan_engine::{
    self as engine,
    kind::Kind,
    message::{Action, ClipMoveFrom, ClipMoveTo, Message as EngineMessage},
};
use rfd::AsyncFileDialog;
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};
use std::{
    fs::{self, File},
    io::{BufReader, BufWriter},
    path::{Path, PathBuf},
    process::{Command, exit},
    sync::{Arc, LazyLock},
    time::Instant,
};
use tokio::sync::RwLock;
use tracing::{debug, error};
use wavers::Wav;

static CLIENT: LazyLock<engine::client::Client> = LazyLock::new(engine::client::Client::default);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct AudioClipKey {
    track_name: String,
    clip_name: String,
    start: usize,
    length: usize,
    offset: usize,
}

fn kernel_midi_label(path: &str) -> String {
    let basename = Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .to_string();

    fn sysctl_value(key: &str) -> Option<String> {
        let output = Command::new("sysctl").arg("-n").arg(key).output().ok()?;
        if !output.status.success() {
            return None;
        }
        let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
        (!value.is_empty()).then_some(value)
    }

    // FreeBSD maps umidi nodes through uaudio units on many systems.
    let dev_id: String = basename
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if !dev_id.is_empty() {
        if basename.starts_with("umidi")
            && let Some(desc) = sysctl_value(&format!("dev.uaudio.{dev_id}.%desc"))
        {
            return compact_desc(desc);
        }
        if basename.starts_with("midi")
            && let Some(desc) = sysctl_value(&format!("dev.midi.{dev_id}.%desc"))
        {
            return compact_desc(desc);
        }
    }

    let probe_keys = {
        let short = basename.split('.').next().unwrap_or(&basename).to_string();
        if short == basename {
            vec![basename.clone()]
        } else {
            vec![basename.clone(), short]
        }
    };

    if let Ok(sndstat) = fs::read_to_string("/dev/sndstat") {
        for line in sndstat.lines() {
            if !probe_keys.iter().any(|key| line.contains(key)) {
                continue;
            }
            if let (Some(start), Some(end)) = (line.find('<'), line.rfind('>'))
                && start < end
            {
                let label = line[start + 1..end].trim();
                if !label.is_empty() {
                    return label.to_string();
                }
            }
            let compact = line.trim();
            if !compact.is_empty() {
                return compact.to_string();
            }
        }
    }

    basename
}

pub struct Maolan {
    clip: Option<DraggedClip>,
    menu: menu::Menu,
    size: Size,
    state: State,
    toolbar: toolbar::Toolbar,
    track: Option<String>,
    workspace: workspace::Workspace,
    connections: connections::canvas_host::CanvasHost<connections::tracks::Graph>,
    track_plugins: connections::canvas_host::CanvasHost<connections::plugins::Graph>,
    hw: hw::HW,
    modal: Option<Show>,
    add_track: add_track::AddTrackView,
    plugin_filter: String,
    selected_lv2_plugins: BTreeSet<String>,
    session_dir: Option<PathBuf>,
    pending_save_path: Option<String>,
    pending_save_tracks: std::collections::HashSet<String>,
    pending_audio_peaks: HashMap<AudioClipKey, Vec<Vec<f32>>>,
    playing: bool,
    transport_samples: f64,
    last_playback_tick: Option<Instant>,
    playback_rate_hz: f64,
    zoom_visible_bars: f32,
    tracks_resize_hovered: bool,
    mixer_resize_hovered: bool,
    record_armed: bool,
    pending_record_after_save: bool,
    recording_preview_start_sample: Option<usize>,
    recording_preview_sample: Option<usize>,
    recording_preview_peaks: HashMap<String, Vec<Vec<f32>>>,
}

impl Default for Maolan {
    fn default() -> Self {
        let state = Arc::new(RwLock::new(StateData::default()));
        Self {
            clip: None,
            menu: menu::Menu::default(),
            size: Size::new(0.0, 0.0),
            state: state.clone(),
            toolbar: toolbar::Toolbar::new(),
            track: None,
            workspace: workspace::Workspace::new(state.clone()),
            connections: connections::canvas_host::CanvasHost::new(
                connections::tracks::Graph::new(state.clone()),
            ),
            track_plugins: connections::canvas_host::CanvasHost::new(
                connections::plugins::Graph::new(state.clone()),
            ),
            hw: hw::HW::new(state.clone()),
            modal: None,
            add_track: add_track::AddTrackView::default(),
            plugin_filter: String::new(),
            selected_lv2_plugins: BTreeSet::new(),
            session_dir: None,
            pending_save_path: None,
            pending_save_tracks: std::collections::HashSet::new(),
            pending_audio_peaks: HashMap::new(),
            playing: false,
            transport_samples: 0.0,
            last_playback_tick: None,
            playback_rate_hz: 48_000.0,
            zoom_visible_bars: 127.0,
            tracks_resize_hovered: false,
            mixer_resize_hovered: false,
            record_armed: false,
            pending_record_after_save: false,
            recording_preview_start_sample: None,
            recording_preview_sample: None,
            recording_preview_peaks: HashMap::new(),
        }
    }
}

impl Maolan {
    fn samples_per_beat(&self) -> f64 {
        self.playback_rate_hz * 0.5
    }

    fn samples_per_bar(&self) -> f64 {
        self.samples_per_beat() * 4.0
    }

    fn tracks_width_px(&self) -> f32 {
        match self.state.blocking_read().tracks_width {
            Length::Fixed(v) => v,
            _ => 200.0,
        }
    }

    fn editor_width_px(&self) -> f32 {
        (self.size.width - self.tracks_width_px() - 3.0).max(1.0)
    }

    fn pixels_per_sample(&self) -> f32 {
        let total_samples = self.samples_per_bar() * self.zoom_visible_bars as f64;
        if total_samples <= 0.0 {
            return 1.0;
        }
        (self.editor_width_px() as f64 / total_samples) as f32
    }

    fn beat_pixels(&self) -> f32 {
        (self.samples_per_beat() as f32 * self.pixels_per_sample()).max(0.01)
    }

    fn start_recording_preview(&mut self) {
        let sample = self.transport_samples.max(0.0) as usize;
        self.recording_preview_start_sample = Some(sample);
        self.recording_preview_sample = Some(sample);
        self.recording_preview_peaks.clear();
    }

    fn stop_recording_preview(&mut self) {
        self.recording_preview_start_sample = None;
        self.recording_preview_sample = None;
        self.recording_preview_peaks.clear();
    }

    fn recording_preview_bounds(&self) -> Option<(usize, usize)> {
        let start = self.recording_preview_start_sample?;
        let current = self.recording_preview_sample?;
        if current > start {
            Some((start, current))
        } else {
            None
        }
    }

    fn audio_clip_key(
        track_name: &str,
        clip_name: &str,
        start: usize,
        length: usize,
        offset: usize,
    ) -> AudioClipKey {
        AudioClipKey {
            track_name: track_name.to_string(),
            clip_name: clip_name.to_string(),
            start,
            length,
            offset,
        }
    }

    fn sanitize_peak_file_component(value: &str) -> String {
        let mut out = String::with_capacity(value.len());
        for ch in value.chars() {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                out.push(ch);
            } else {
                out.push('_');
            }
        }
        if out.is_empty() {
            "clip".to_string()
        } else {
            out
        }
    }

    fn build_peak_file_rel(track_name: &str, clip_idx: usize, clip_name: &str) -> String {
        let track = Self::sanitize_peak_file_component(track_name);
        let clip = Self::sanitize_peak_file_component(clip_name);
        format!("peaks/{}_{:04}_{}.json", track, clip_idx, clip)
    }

    fn read_clip_peaks_file(path: &Path) -> std::io::Result<Vec<Vec<f32>>> {
        let file = File::open(path)?;
        let json: Value = serde_json::from_reader(BufReader::new(file))?;
        let peaks_val = &json["peaks"];

        // Backward-compatible with legacy single-channel format: "peaks": [..]
        if peaks_val.as_array().is_some_and(|arr| {
            arr.first().is_some_and(|first| first.is_number())
        }) {
            let mono = peaks_val
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(Value::as_f64)
                        .map(|v| v as f32)
                        .collect::<Vec<f32>>()
                })
                .unwrap_or_default();
            return Ok(if mono.is_empty() { vec![] } else { vec![mono] });
        }

        let per_channel = peaks_val
            .as_array()
            .map(|channels| {
                channels
                    .iter()
                    .map(|channel| {
                        channel
                            .as_array()
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(Value::as_f64)
                                    .map(|v| v as f32)
                                    .collect::<Vec<f32>>()
                            })
                            .unwrap_or_default()
                    })
                    .collect::<Vec<Vec<f32>>>()
            })
            .unwrap_or_default();
        Ok(per_channel)
    }

    fn compute_audio_clip_peaks(path: &Path, bins: usize) -> std::io::Result<Vec<Vec<f32>>> {
        let mut wav = Wav::<f32>::from_path(path).map_err(|e| {
            io::Error::other(format!("Failed to open WAV '{}': {e}", path.display()))
        })?;
        let channels = wav.n_channels().max(1) as usize;
        let samples: wavers::Samples<f32> = wav
            .read()
            .map_err(|e| io::Error::other(format!("WAV read error '{}': {e}", path.display())))?;
        let mut per_channel_abs = vec![Vec::with_capacity(samples.len() / channels + 1); channels];
        for frame in samples.chunks(channels) {
            for (channel_idx, sample) in frame.iter().enumerate() {
                per_channel_abs[channel_idx].push(sample.abs());
            }
        }

        if per_channel_abs.iter().all(|ch| ch.is_empty()) {
            return Ok(vec![]);
        }
        let target_bins = bins.max(16);
        let mut peaks = vec![vec![0.0_f32; target_bins]; channels];
        for channel_idx in 0..channels {
            let samples = &per_channel_abs[channel_idx];
            if samples.is_empty() {
                continue;
            }
            for (i, peak) in samples.iter().enumerate() {
                let bin = (i * target_bins) / samples.len();
                let idx = bin.min(target_bins - 1);
                peaks[channel_idx][idx] = peaks[channel_idx][idx].max(*peak);
            }
        }
        Ok(peaks)
    }

    fn lv2_node_to_json(
        node: &maolan_engine::message::Lv2GraphNode,
        id_to_index: &std::collections::HashMap<usize, usize>,
    ) -> Option<Value> {
        use maolan_engine::message::Lv2GraphNode;
        match node {
            Lv2GraphNode::TrackInput => Some(json!({"type":"track_input"})),
            Lv2GraphNode::TrackOutput => Some(json!({"type":"track_output"})),
            Lv2GraphNode::PluginInstance(id) => id_to_index
                .get(id)
                .copied()
                .map(|idx| json!({"type":"plugin","plugin_index":idx})),
        }
    }

    fn lv2_node_from_json(v: &Value) -> Option<maolan_engine::message::Lv2GraphNode> {
        use maolan_engine::message::Lv2GraphNode;
        let t = v["type"].as_str()?;
        match t {
            "track_input" => Some(Lv2GraphNode::TrackInput),
            "track_output" => Some(Lv2GraphNode::TrackOutput),
            "plugin" => Some(Lv2GraphNode::PluginInstance(
                v["plugin_index"].as_u64()? as usize,
            )),
            _ => None,
        }
    }

    fn kind_to_json(kind: Kind) -> Value {
        match kind {
            Kind::Audio => json!("audio"),
            Kind::MIDI => json!("midi"),
        }
    }

    fn kind_from_json(v: &Value) -> Option<Kind> {
        match v.as_str()? {
            "audio" => Some(Kind::Audio),
            "midi" => Some(Kind::MIDI),
            _ => None,
        }
    }

    fn lv2_state_to_json(state: &maolan_engine::message::Lv2PluginState) -> Value {
        let port_values = state
            .port_values
            .iter()
            .map(|p| json!({"index": p.index, "value": p.value}))
            .collect::<Vec<_>>();
        let properties = state
            .properties
            .iter()
            .map(|p| {
                json!({
                    "key_uri": p.key_uri,
                    "type_uri": p.type_uri,
                    "flags": p.flags,
                    "value": p.value,
                })
            })
            .collect::<Vec<_>>();
        json!({
            "port_values": port_values,
            "properties": properties,
        })
    }

    fn lv2_state_from_json(v: &Value) -> Option<maolan_engine::message::Lv2PluginState> {
        let port_values = v["port_values"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        Some(maolan_engine::message::Lv2StatePortValue {
                            index: item["index"].as_u64()? as u32,
                            value: item["value"].as_f64()? as f32,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let properties = v["properties"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        Some(maolan_engine::message::Lv2StateProperty {
                            key_uri: item["key_uri"].as_str()?.to_string(),
                            type_uri: item["type_uri"].as_str()?.to_string(),
                            flags: item["flags"].as_u64().unwrap_or(0) as u32,
                            value: item["value"]
                                .as_array()?
                                .iter()
                                .map(|b| b.as_u64().unwrap_or(0) as u8)
                                .collect(),
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Some(maolan_engine::message::Lv2PluginState {
            port_values,
            properties,
        })
    }

    fn restore_actions_task(actions: Vec<Action>) -> Task<Message> {
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

    fn track_plugin_list_view(&self) -> iced::Element<'_, Message> {
        let state = self.state.blocking_read();
        let title = state
            .lv2_graph_track
            .clone()
            .unwrap_or_else(|| "(no track)".to_string());

        let mut lv2_list = column![];
        let filter = self.plugin_filter.trim().to_lowercase();
        for plugin in &state.lv2_plugins {
            if !filter.is_empty() {
                let name = plugin.name.to_lowercase();
                let uri = plugin.uri.to_lowercase();
                if !name.contains(&filter) && !uri.contains(&filter) {
                    continue;
                }
            }
            let is_selected = self.selected_lv2_plugins.contains(&plugin.uri);
            let row_content = row![
                text(if is_selected { "[x]" } else { "[ ]" }),
                text(format!(
                    "{} (a:{}/{}, m:{}/{})",
                    plugin.name,
                    plugin.audio_inputs,
                    plugin.audio_outputs,
                    plugin.midi_inputs,
                    plugin.midi_outputs
                ))
                .width(Length::Fill),
            ]
            .spacing(8)
            .width(Length::Fill);

            let row_button = if is_selected {
                button(row_content).style(button::primary)
            } else {
                button(row_content).style(button::text)
            };
            lv2_list = lv2_list.push(
                row_button
                    .width(Length::Fill)
                    .on_press(Message::SelectLv2Plugin(plugin.uri.clone())),
            );
        }

        let load_button = if self.selected_lv2_plugins.is_empty() {
            button("Load")
        } else {
            button(text(format!("Load ({})", self.selected_lv2_plugins.len())))
                .on_press(Message::LoadSelectedLv2Plugins)
        };

        container(
            column![
                text(format!("Track Plugins: {title}")),
                text_input("Filter plugins...", &self.plugin_filter)
                    .on_input(Message::FilterLv2Plugins)
                    .width(Length::Fill),
                scrollable(lv2_list).height(Length::Fill),
                row![
                    load_button,
                    button("Close").on_press(Message::Cancel).style(button::secondary),
                ]
                .spacing(10),
            ]
            .spacing(10),
        )
        .padding(20)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    fn send(&self, action: Action) -> Task<Message> {
        Task::perform(
            async move { CLIENT.send(EngineMessage::Request(action)).await },
            |result| match result {
                Ok(_) => Message::SendMessageFinished(Ok(())),
                Err(_) => Message::Response(Err("Channel closed".to_string())),
            },
        )
    }
    fn save(&self, path: String) -> std::io::Result<()> {
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

    fn load(&mut self, path: String) -> std::io::Result<Task<Message>> {
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

    fn refresh_graphs_then_save(&mut self, path: String) -> Task<Message> {
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

    fn update_children(&mut self, message: &Message) {
        self.menu.update(message.clone());
        self.toolbar.update(message.clone());
        self.workspace.update(message.clone());
        self.connections.update(message.clone());
        self.track_plugins.update(message.clone());
        self.add_track.update(message.clone());
        for track in &mut self.state.blocking_write().tracks {
            track.update(message.clone());
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::None => {
                return Task::none();
            }
            Message::ToggleTransport => {
                if !self.state.blocking_read().hw_loaded {
                    return Task::none();
                }
                if self.playing {
                    self.playing = false;
                    self.last_playback_tick = None;
                    self.stop_recording_preview();
                    return self.send(Action::Stop);
                }
                self.playing = true;
                self.last_playback_tick = Some(Instant::now());
                if self.record_armed {
                    self.start_recording_preview();
                }
                return self.send(Action::Play);
            }
            Message::WindowResized(size) => {
                self.size = size;
            }
            Message::Show(ref show) => {
                use crate::message::Show;
                if !self.state.blocking_read().hw_loaded && matches!(show, Show::Save | Show::Open)
                {
                    return Task::none();
                }
                match show {
                    Show::Save => {
                        return Task::perform(
                            async {
                                AsyncFileDialog::new()
                                    .set_title("Select folder to save session")
                                    .set_directory("/tmp")
                                    .pick_folder()
                                    .await
                                    .map(|handle| handle.path().to_path_buf())
                            },
                            Message::SaveFolderSelected,
                        );
                    }
                    Show::Open => {
                        return Task::perform(
                            async {
                                AsyncFileDialog::new()
                                    .set_title("Select folder to open session")
                                    .set_directory("/tmp")
                                    .pick_folder()
                                    .await
                                    .map(|handle| handle.path().to_path_buf())
                            },
                            Message::OpenFolderSelected,
                        );
                    }
                    Show::AddTrack => {
                        self.modal = Some(Show::AddTrack);
                    }
                    Show::TrackPluginList => {
                        self.modal = Some(Show::TrackPluginList);
                        self.selected_lv2_plugins.clear();
                    }
                }
            }
            Message::NewSession => {
                if !self.state.blocking_read().hw_loaded {
                    return Task::none();
                }
                self.playing = false;
                self.transport_samples = 0.0;
                self.last_playback_tick = None;
                self.record_armed = false;
                self.pending_record_after_save = false;
                self.pending_save_path = None;
                self.pending_save_tracks.clear();
                self.pending_audio_peaks.clear();
                self.session_dir = None;
                self.stop_recording_preview();

                let existing_tracks: Vec<String> = self
                    .state
                    .blocking_read()
                    .tracks
                    .iter()
                    .map(|t| t.name.clone())
                    .collect();
                let mut tasks = vec![
                    self.send(Action::Stop),
                    self.send(Action::SetRecordEnabled(false)),
                ];
                for name in existing_tracks {
                    tasks.push(self.send(Action::RemoveTrack(name)));
                }
                {
                    let mut state = self.state.blocking_write();
                    state.connections.clear();
                    state.selected.clear();
                    state.selected_clips.clear();
                    state.connection_view_selection = ConnectionViewSelection::None;
                    state.lv2_graph_track = None;
                    state.lv2_graph_plugins.clear();
                    state.lv2_graph_connections.clear();
                    state.lv2_graphs_by_track.clear();
                    state.message = "New session".to_string();
                }
                return Task::batch(tasks);
            }
            Message::Cancel => self.modal = None,
            Message::Request(ref a) => return self.send(a.clone()),
            Message::TransportPlay => {
                self.playing = true;
                self.last_playback_tick = Some(Instant::now());
                if self.record_armed {
                    self.start_recording_preview();
                }
                return self.send(Action::Play);
            }
            Message::TransportPause => {
                self.playing = false;
                self.last_playback_tick = None;
                self.stop_recording_preview();
                return self.send(Action::Stop);
            }
            Message::TransportStop => {
                self.playing = false;
                self.last_playback_tick = None;
                self.stop_recording_preview();
                return self.send(Action::Stop);
            }
            Message::PlaybackTick => {
                if self.playing && let Some(last) = self.last_playback_tick {
                    let now = Instant::now();
                    let delta_s = now.duration_since(last).as_secs_f64();
                    self.last_playback_tick = Some(now);
                    self.transport_samples += delta_s * self.playback_rate_hz;
                }
            }
            Message::RecordingPreviewTick => {
                if self.playing && self.record_armed && self.recording_preview_start_sample.is_some() {
                    self.recording_preview_sample = Some(self.transport_samples.max(0.0) as usize);
                }
            }
            Message::RecordingPreviewPeaksTick => {
                if self.playing && self.record_armed && self.recording_preview_start_sample.is_some() {
                    let tracks = self.state.blocking_read().tracks.clone();
                    for track in tracks.iter().filter(|t| t.armed) {
                        let channels = track.audio.outs.max(1);
                        let entry = self
                            .recording_preview_peaks
                            .entry(track.name.clone())
                            .or_insert_with(|| vec![vec![]; channels]);
                        if entry.len() != channels {
                            entry.resize_with(channels, Vec::new);
                        }
                        for channel_idx in 0..channels {
                            let db = track.meter_out_db.get(channel_idx).copied().unwrap_or(-90.0);
                            let amp = if db <= -90.0 {
                                0.0
                            } else {
                                10.0_f32.powf(db / 20.0).clamp(0.0, 1.0)
                            };
                            entry[channel_idx].push(amp);
                        }
                    }
                }
            }
            Message::ZoomVisibleBarsChanged(value) => {
                self.zoom_visible_bars = value.clamp(1.0, 256.0);
            }
            Message::TracksResizeHover(hovered) => {
                self.tracks_resize_hovered = hovered;
            }
            Message::MixerResizeHover(hovered) => {
                self.mixer_resize_hovered = hovered;
            }
            Message::TransportRecordToggle => {
                if self.record_armed {
                    self.record_armed = false;
                    self.pending_record_after_save = false;
                    self.stop_recording_preview();
                    return self.send(Action::SetRecordEnabled(false));
                }
                if self.session_dir.is_none() {
                    self.pending_record_after_save = true;
                    return Task::perform(
                        async {
                            AsyncFileDialog::new()
                                .set_title("Select folder to save session")
                                .set_directory("/tmp")
                                .pick_folder()
                                .await
                                .map(|handle| handle.path().to_path_buf())
                        },
                        Message::RecordFolderSelected,
                    );
                }
                self.record_armed = true;
                if self.playing {
                    self.start_recording_preview();
                }
                return self.send(Action::SetRecordEnabled(true));
            }
            Message::RefreshLv2Plugins => return self.send(Action::ListLv2Plugins),
            Message::FilterLv2Plugins(ref query) => {
                self.plugin_filter = query.clone();
            }
            Message::SelectLv2Plugin(ref plugin_uri) => {
                if self.selected_lv2_plugins.contains(plugin_uri) {
                    self.selected_lv2_plugins.remove(plugin_uri);
                } else {
                    self.selected_lv2_plugins.insert(plugin_uri.clone());
                }
            }
            Message::LoadSelectedLv2Plugins => {
                let track_name = {
                    let state = self.state.blocking_read();
                    state
                        .lv2_graph_track
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
                    return Task::batch(tasks);
                }
                self.state.blocking_write().message =
                    "Select a track before loading LV2 plugin".to_string();
            }
            Message::SendMessageFinished(ref result) => match result {
                Ok(_) => debug!("Sent successfully!"),
                Err(e) => error!("Error: {}", e),
            },
            Message::Response(Ok(ref a)) => match a {
                Action::Quit => {
                    exit(0);
                }
                Action::AddTrack {
                    name,
                    audio_ins,
                    audio_outs,
                    midi_ins,
                    midi_outs,
                } => {
                    let mut state = self.state.blocking_write();
                    state.tracks.push(Track::new(
                        name.clone(),
                        0.0,
                        *audio_ins,
                        *audio_outs,
                        *midi_ins,
                        *midi_outs,
                    ));

                    if let Some(position) = state.pending_track_positions.remove(name)
                        && let Some(track) = state.tracks.iter_mut().find(|t| &t.name == name)
                    {
                        track.position = position;
                    }
                    if let Some(height) = state.pending_track_heights.remove(name)
                        && let Some(track) = state.tracks.iter_mut().find(|t| &t.name == name)
                    {
                        track.height = height;
                    }
                    self.modal = None;
                }
                Action::RemoveTrack(name) => {
                    let mut state = self.state.blocking_write();

                    if let Some(removed_idx) = state.tracks.iter().position(|t| t.name == *name) {
                        state
                            .connections
                            .retain(|conn| conn.from_track != *name && conn.to_track != *name);
                        state.tracks.remove(removed_idx);

                        state.selected.remove(name);
                        if let ConnectionViewSelection::Tracks(set) =
                            &mut state.connection_view_selection
                        {
                            set.remove(name);
                        }
                    }
                }

                Action::ClipMove {
                    kind,
                    from,
                    to,
                    copy,
                } => {
                    let mut state = self.state.blocking_write();

                    // Find the track by name
                    let from_track_idx_option: Option<usize> = state
                        .tracks
                        .iter()
                        .position(|track| track.name == from.track_name);

                    if let Some(f_idx) = from_track_idx_option {
                        // Get mutable borrow of from_track outside the main loop
                        let from_track = &mut state.tracks[f_idx];

                        let mut clip_to_move: Option<crate::state::AudioClip> = None;
                        let mut midi_clip_to_move: Option<crate::state::MIDIClip> = None;

                        match kind {
                            Kind::Audio => {
                                if from.clip_index < from_track.audio.clips.len() {
                                    if !copy {
                                        clip_to_move =
                                            Some(from_track.audio.clips.remove(from.clip_index));
                                    } else {
                                        clip_to_move =
                                            Some(from_track.audio.clips[from.clip_index].clone());
                                    }
                                }
                            }
                            Kind::MIDI => {
                                if from.clip_index < from_track.midi.clips.len() {
                                    if !copy {
                                        midi_clip_to_move =
                                            Some(from_track.midi.clips.remove(from.clip_index));
                                    } else {
                                        midi_clip_to_move =
                                            Some(from_track.midi.clips[from.clip_index].clone());
                                    }
                                }
                            }
                        }

                        // Now find the to_track and add the clip
                        if let Some(to_track) = state
                            .tracks
                            .iter_mut()
                            .find(|track| track.name == to.track_name)
                        {
                            if let Some(mut clip_data) = clip_to_move {
                                clip_data.start = to.sample_offset;
                                to_track.audio.clips.push(clip_data);
                            } else if let Some(mut midi_clip_data) = midi_clip_to_move {
                                midi_clip_data.start = to.sample_offset;
                                to_track.midi.clips.push(midi_clip_data);
                            }
                        }
                    }
                }
                Action::AddClip {
                    name,
                    track_name,
                    start,
                    length,
                    offset,
                    kind,
                } => {
                    let mut audio_peaks = vec![];
                    if *kind == Kind::Audio {
                        let key =
                            Self::audio_clip_key(track_name, name, *start, *length, *offset);
                        audio_peaks = self.pending_audio_peaks.remove(&key).unwrap_or_default();
                        if audio_peaks.is_empty()
                            && name.to_ascii_lowercase().ends_with(".wav")
                            && let Some(session_root) = &self.session_dir
                        {
                            let wav_path = session_root.join(name);
                            if wav_path.exists()
                                && let Ok(computed) = Self::compute_audio_clip_peaks(&wav_path, 512)
                            {
                                audio_peaks = computed;
                            }
                        }
                    }
                    let mut state = self.state.blocking_write();
                    if let Some(track) = state.tracks.iter_mut().find(|t| &t.name == track_name) {
                        match kind {
                            Kind::Audio => {
                                track.audio.clips.push(crate::state::AudioClip {
                                    name: name.clone(),
                                    start: *start,
                                    length: *length,
                                    offset: *offset,
                                    peaks_file: None,
                                    peaks: audio_peaks,
                                });
                            }
                            Kind::MIDI => {
                                track.midi.clips.push(crate::state::MIDIClip {
                                    name: name.clone(),
                                    start: *start,
                                    length: *length,
                                    offset: *offset,
                                });
                            }
                        }
                    }
                }
                Action::Connect {
                    from_track,
                    from_port,
                    to_track,
                    to_port,
                    kind,
                } => {
                    let mut state = self.state.blocking_write();

                    state.connections.push(crate::state::Connection {
                        from_track: from_track.clone(),
                        from_port: *from_port,
                        to_track: to_track.clone(),
                        to_port: *to_port,
                        kind: *kind,
                    });
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
                }

                Action::OpenAudioDevice(s) => {
                    self.state.blocking_write().message = format!("Opened device {s}");
                    self.state.blocking_write().hw_loaded = true;
                }
                Action::OpenMidiInputDevice(s) => {
                    let mut state = self.state.blocking_write();
                    if !state.opened_midi_in_hw.iter().any(|name| name == s) {
                        state.opened_midi_in_hw.push(s.clone());
                    }
                    state
                        .midi_hw_labels
                        .entry(s.clone())
                        .or_insert_with(|| kernel_midi_label(s));
                    state.message = format!("Opened MIDI input {s}");
                }
                Action::OpenMidiOutputDevice(s) => {
                    let mut state = self.state.blocking_write();
                    if !state.opened_midi_out_hw.iter().any(|name| name == s) {
                        state.opened_midi_out_hw.push(s.clone());
                    }
                    state
                        .midi_hw_labels
                        .entry(s.clone())
                        .or_insert_with(|| kernel_midi_label(s));
                    state.message = format!("Opened MIDI output {s}");
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
                Action::TrackLevel(name, level) => {
                    if name == "hw:out" {
                        self.state.blocking_write().hw_out_level = *level;
                    }
                }
                Action::TrackToggleMute(name) => {
                    if name == "hw:out" {
                        let mut state = self.state.blocking_write();
                        state.hw_out_muted = !state.hw_out_muted;
                    }
                }
                Action::TrackMeters {
                    track_name,
                    output_db,
                } => {
                    if track_name == "hw:out" {
                        self.state.blocking_write().hw_out_meter_db = output_db.clone();
                    }
                }
                Action::SetSessionPath(_) => {
                    if self.pending_record_after_save {
                        self.pending_record_after_save = false;
                        return self.send(Action::SetRecordEnabled(true));
                    }
                }
                Action::TransportPosition(sample) => {
                    self.transport_samples = *sample as f64;
                    if self.playing {
                        self.last_playback_tick = Some(Instant::now());
                    }
                }
                Action::Lv2Plugins(plugins) => {
                    let mut state = self.state.blocking_write();
                    state.lv2_plugins = plugins.clone();
                    state.message = format!("Loaded {} LV2 plugins", state.lv2_plugins.len());
                }
                Action::TrackLoadLv2Plugin { track_name, .. }
                | Action::TrackClearDefaultPassthrough { track_name, .. }
                | Action::TrackSetLv2PluginState { track_name, .. }
                | Action::TrackUnloadLv2PluginInstance { track_name, .. }
                | Action::TrackConnectLv2Audio { track_name, .. }
                | Action::TrackDisconnectLv2Audio { track_name, .. }
                | Action::TrackConnectLv2Midi { track_name, .. }
                | Action::TrackDisconnectLv2Midi { track_name, .. } => {
                    let lv2_track = self.state.blocking_read().lv2_graph_track.clone();
                    if lv2_track.as_deref() == Some(track_name.as_str()) {
                        return self.send(Action::TrackGetLv2Graph {
                            track_name: track_name.clone(),
                        });
                    }
                }
                Action::TrackLv2Graph {
                    track_name,
                    plugins,
                    connections,
                } => {
                    let mut state = self.state.blocking_write();
                    state
                        .lv2_graphs_by_track
                        .insert(track_name.clone(), (plugins.clone(), connections.clone()));
                    if state.lv2_graph_track.as_deref() == Some(track_name.as_str()) {
                        state.lv2_graph_track = Some(track_name.clone());
                        state.lv2_graph_plugins = plugins.clone();
                        state.lv2_graph_connections = connections.clone();
                        state.lv2_graph_selected_connections.clear();
                        state.lv2_graph_selected_plugin = state
                            .lv2_graph_selected_plugin
                            .filter(|id| plugins.iter().any(|p| p.instance_id == *id));
                        let mut new_positions = std::collections::HashMap::new();
                        for (idx, plugin) in plugins.iter().enumerate() {
                            let fallback = Point::new(200.0 + idx as f32 * 180.0, 220.0);
                            let pos = state
                                .lv2_graph_plugin_positions
                                .get(&plugin.instance_id)
                                .copied()
                                .unwrap_or(fallback);
                            new_positions.insert(plugin.instance_id, pos);
                        }
                        state.lv2_graph_plugin_positions = new_positions;
                    }
                    drop(state);

                    if self.pending_save_path.is_some() {
                        self.pending_save_tracks.remove(track_name);
                        if self.pending_save_tracks.is_empty() {
                            let path = self.pending_save_path.take().unwrap_or_default();
                            if !path.is_empty() {
                                if let Err(e) = self.save(path.clone()) {
                                    error!("{}", e);
                                } else {
                                    return self.send(Action::SetSessionPath(path));
                                }
                            }
                        }
                    }
                }
                _ => {
                    // Intentionally ignore responses that do not need explicit GUI handling.
                }
            },
            Message::Response(Err(ref e)) => {
                self.state.blocking_write().message = e.clone();
                error!("Engine error: {e}");
            }
            Message::SaveFolderSelected(ref path_opt) => {
                if let Some(path) = path_opt {
                    self.session_dir = Some(path.clone());
                    return self.refresh_graphs_then_save(path.to_string_lossy().to_string());
                }
            }
            Message::RecordFolderSelected(ref path_opt) => {
                if let Some(path) = path_opt {
                    self.session_dir = Some(path.clone());
                    self.record_armed = true;
                    self.pending_record_after_save = true;
                    if self.playing {
                        self.start_recording_preview();
                    }
                    return self.refresh_graphs_then_save(path.to_string_lossy().to_string());
                } else {
                    self.pending_record_after_save = false;
                }
            }
            Message::OpenFolderSelected(Some(path)) => {
                self.session_dir = Some(path.clone());
                self.stop_recording_preview();
                match self.load(path.to_string_lossy().to_string()) {
                    Ok(task) => return task,
                    Err(e) => {
                        error!("{}", e);
                        return Task::none();
                    }
                }
            }
            Message::ShiftPressed => {
                if !self.state.blocking_read().hw_loaded {
                    return Task::none();
                }
                self.state.blocking_write().shift = true;
            }
            Message::ShiftReleased => {
                if !self.state.blocking_read().hw_loaded {
                    return Task::none();
                }
                self.state.blocking_write().shift = false;
            }
            Message::CtrlPressed => {
                if !self.state.blocking_read().hw_loaded {
                    return Task::none();
                }
                self.state.blocking_write().ctrl = true;
            }
            Message::CtrlReleased => {
                if !self.state.blocking_read().hw_loaded {
                    return Task::none();
                }
                self.state.blocking_write().ctrl = false;
            }
            Message::SelectTrack(ref name) => {
                let ctrl = self.state.blocking_read().ctrl;
                let selected = self.state.blocking_read().selected.contains(name);
                let mut state = self.state.blocking_write();

                if ctrl {
                    if selected {
                        state.selected.remove(name);
                        if let ConnectionViewSelection::Tracks(set) =
                            &mut state.connection_view_selection
                        {
                            set.remove(name);
                        }
                    } else {
                        state.selected.insert(name.clone());
                        if let ConnectionViewSelection::Tracks(set) =
                            &mut state.connection_view_selection
                        {
                            set.insert(name.clone());
                        }
                    }
                } else {
                    state.selected.clear();
                    state.selected.insert(name.clone());
                    let mut set = std::collections::HashSet::new();
                    set.insert(name.clone());
                    state.connection_view_selection = ConnectionViewSelection::Tracks(set);
                }
            }
            Message::RemoveSelectedTracks => {
                let mut tasks = vec![];
                for name in &self.state.blocking_read().selected {
                    tasks.push(self.send(Action::RemoveTrack(name.clone())));
                }
                return Task::batch(tasks);
            }
            Message::ConnectionViewSelectTrack(ref idx) => {
                let ctrl = self.state.blocking_read().ctrl;
                let mut state = self.state.blocking_write();

                match &mut state.connection_view_selection {
                    ConnectionViewSelection::Tracks(set) if ctrl => {
                        if set.contains(idx.as_str()) {
                            set.remove(idx.as_str());
                            state.selected.remove(idx.as_str());
                        } else {
                            set.insert(idx.clone());
                            state.selected.insert(idx.clone());
                        }
                    }
                    _ => {
                        let mut set = std::collections::HashSet::new();
                        set.insert(idx.clone());
                        state.connection_view_selection = ConnectionViewSelection::Tracks(set);
                        state.selected.clear();
                        state.selected.insert(idx.clone());
                    }
                }
            }
            Message::SelectClip {
                ref track_idx,
                clip_idx,
                kind,
            } => {
                use crate::state::ClipId;
                let ctrl = self.state.blocking_read().ctrl;
                let mut state = self.state.blocking_write();

                let clip_id = ClipId {
                    track_idx: track_idx.clone(),
                    clip_idx,
                    kind,
                };

                if ctrl {
                    if state.selected_clips.contains(&clip_id) {
                        state.selected_clips.remove(&clip_id);
                    } else {
                        state.selected_clips.insert(clip_id);
                    }
                } else {
                    state.selected_clips.clear();
                    state.selected_clips.insert(clip_id);
                }
            }
            Message::DeselectAll => {
                let mut state = self.state.blocking_write();
                state.selected.clear();
                state.selected_clips.clear();
                state.connection_view_selection = ConnectionViewSelection::None;
            }
            Message::ConnectionViewSelectConnection(idx) => {
                let ctrl = self.state.blocking_read().ctrl;
                let mut state = self.state.blocking_write();
                connections::selection::apply_track_connection_selection(&mut state, idx, ctrl);
            }
            Message::RemoveSelected => {
                let state = self.state.blocking_read();
                match &state.connection_view_selection {
                    ConnectionViewSelection::Tracks(set) => {
                        let mut tasks = vec![];
                        for name in set {
                            tasks.push(self.send(Action::RemoveTrack(name.clone())));
                        }
                        drop(state);
                        self.state.blocking_write().connection_view_selection =
                            ConnectionViewSelection::None;
                        return Task::batch(tasks);
                    }
                    ConnectionViewSelection::Connections(set) => {
                        let actions = connections::selection::track_disconnect_actions(&state, set);
                        let tasks = actions
                            .into_iter()
                            .map(|a| self.send(a))
                            .collect::<Vec<_>>();
                        drop(state);
                        self.state.blocking_write().connection_view_selection =
                            ConnectionViewSelection::None;
                        return Task::batch(tasks);
                    }
                    ConnectionViewSelection::None => {}
                }
            }

            Message::Remove => {
                if !self.state.blocking_read().hw_loaded {
                    return Task::none();
                }
                let view = self.state.blocking_read().view.clone();
                match view {
                    crate::state::View::Connections => {
                        return self.update(Message::RemoveSelected);
                    }
                    crate::state::View::Workspace => {
                        return self.update(Message::RemoveSelectedTracks);
                    }
                    crate::state::View::TrackPlugins => {
                        let (track_name, selected_plugin, selected_indices, connections) = {
                            let state = self.state.blocking_read();
                            (
                                state.lv2_graph_track.clone(),
                                state.lv2_graph_selected_plugin,
                                state.lv2_graph_selected_connections.clone(),
                                state.lv2_graph_connections.clone(),
                            )
                        };
                        if let Some(track_name) = track_name {
                            if let Some(instance_id) = selected_plugin {
                                self.state.blocking_write().lv2_graph_selected_plugin = None;
                                self.state
                                    .blocking_write()
                                    .lv2_graph_selected_connections
                                    .clear();
                                return self.send(Action::TrackUnloadLv2PluginInstance {
                                    track_name,
                                    instance_id,
                                });
                            }
                            let actions = connections::selection::plugin_disconnect_actions(
                                &track_name,
                                &connections,
                                &selected_indices,
                            );
                            let tasks = actions
                                .into_iter()
                                .map(|a| self.send(a))
                                .collect::<Vec<_>>();
                            self.state
                                .blocking_write()
                                .lv2_graph_selected_connections
                                .clear();
                            self.state.blocking_write().lv2_graph_selected_plugin = None;
                            return Task::batch(tasks);
                        }
                    }
                }
            }
            Message::TrackResizeStart(ref index) => {
                let mut state = self.state.blocking_write();
                if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *index) {
                    let height = track.height;
                    state.resizing = Some(Resizing::Track(index.clone(), height, state.cursor.y));
                }
            }
            Message::TracksResizeStart => {
                let (initial_width, initial_mouse_x) = {
                    let state = self.state.blocking_read();
                    let width = match state.tracks_width {
                        Length::Fixed(v) => v,
                        _ => 200.0,
                    };
                    (width, state.cursor.x)
                };
                self.state.blocking_write().resizing =
                    Some(Resizing::Tracks(initial_width, initial_mouse_x));
            }
            Message::MixerResizeStart => {
                let (initial_height, initial_mouse_y) = {
                    let state = self.state.blocking_read();
                    let height = match state.mixer_height {
                        Length::Fixed(v) => v,
                        _ => 300.0,
                    };
                    (height, state.cursor.y)
                };
                self.state.blocking_write().resizing =
                    Some(Resizing::Mixer(initial_height, initial_mouse_y));
            }
            Message::ClipResizeStart(ref kind, ref track_name, clip_index, is_right_side) => {
                let mut state = self.state.blocking_write();
                if let Some(track) = state.tracks.iter().find(|t| t.name == *track_name) {
                    match kind {
                        Kind::Audio => {
                            let clip = &track.audio.clips[clip_index];
                            let initial_value = if is_right_side {
                                clip.length
                            } else {
                                clip.start
                            };
                            state.resizing = Some(Resizing::Clip(
                                *kind,
                                track_name.clone(),
                                clip_index,
                                is_right_side,
                                initial_value as f32,
                                state.cursor.x,
                            ));
                        }
                        Kind::MIDI => {
                            let clip = &track.midi.clips[clip_index];
                            let initial_value = if is_right_side {
                                clip.length
                            } else {
                                clip.start
                            };
                            state.resizing = Some(Resizing::Clip(
                                *kind,
                                track_name.clone(),
                                clip_index,
                                is_right_side,
                                initial_value as f32,
                                state.cursor.x,
                            ));
                        }
                    }
                }
            }
            Message::MouseMoved(mouse::Event::CursorMoved { position }) => {
                let resizing = self.state.blocking_read().resizing.clone();
                self.state.blocking_write().cursor = position;
                match resizing {
                    Some(Resizing::Track(track_name, initial_height, initial_mouse_y)) => {
                        let mut state = self.state.blocking_write();
                        let delta = position.y - initial_mouse_y;
                        if let Some(track) = state.tracks.iter_mut().find(|t| t.name == track_name)
                        {
                            track.height = (initial_height + delta).clamp(60.0, 400.0);
                        }
                    }
                    Some(Resizing::Clip(
                        kind,
                        track_name,
                        index,
                        is_right_side,
                        initial_value,
                        initial_mouse_x,
                    )) => {
                        let pixels_per_sample = self.pixels_per_sample().max(1.0e-6);
                        let mut state = self.state.blocking_write();
                        if let Some(track) = state.tracks.iter_mut().find(|t| t.name == track_name)
                        {
                            let delta_samples = (position.x - initial_mouse_x) / pixels_per_sample;
                            match kind {
                                Kind::Audio => {
                                    let clip = &mut track.audio.clips[index];
                                    if is_right_side {
                                        clip.length =
                                            (initial_value + delta_samples).max(10.0) as usize;
                                    } else {
                                        let new_start = (initial_value + delta_samples).max(0.0);
                                        let start_delta = new_start - clip.start as f32;
                                        clip.start = new_start as usize;
                                        clip.length = (clip.length - start_delta as usize).max(10);
                                    }
                                }
                                Kind::MIDI => {
                                    let clip = &mut track.midi.clips[index];
                                    if is_right_side {
                                        clip.length =
                                            (initial_value + delta_samples).max(10.0) as usize;
                                    } else {
                                        let new_start = (initial_value + delta_samples).max(0.0);
                                        let start_delta = new_start - clip.start as f32;
                                        clip.start = new_start as usize;
                                        clip.length = (clip.length - start_delta as usize).max(10);
                                    }
                                }
                            }
                        }
                    }
                    Some(Resizing::Tracks(initial_width, initial_mouse_x)) => {
                        let delta = position.x - initial_mouse_x;
                        self.state.blocking_write().tracks_width =
                            Length::Fixed((initial_width + delta).max(80.0));
                    }
                    Some(Resizing::Mixer(initial_height, initial_mouse_y)) => {
                        let delta = position.y - initial_mouse_y;
                        self.state.blocking_write().mixer_height =
                            Length::Fixed((initial_height - delta).max(60.0));
                    }
                    _ => {}
                }
            }
            Message::MouseReleased => {
                let mut state = self.state.blocking_write();
                state.resizing = None;
            }
            Message::ClipDrag(ref clip) => {
                if self.clip.is_none() {
                    self.clip = Some(clip.clone());
                }
            }
            Message::ClipDropped(point, _rect) => {
                if let Some(clip) = &mut self.clip {
                    clip.end = point;
                    return iced_drop::zones_on_point(Message::HandleClipZones, point, None, None);
                }
            }
            Message::HandleClipZones(ref zones) => {
                if let Some(clip) = &self.clip
                    && let Some((to_track_id, _)) = zones.first()
                {
                    let state = self.state.blocking_read();
                    let from_track_name = &clip.track_index;

                    let from_track_option =
                        state.tracks.iter().find(|t| t.name == *from_track_name);
                    let to_track_option = state
                        .tracks
                        .iter()
                        .find(|t| Id::from(t.name.clone()) == *to_track_id);

                    if let (Some(from_track), Some(to_track)) = (from_track_option, to_track_option)
                    {
                        let clip_index = clip.index;
                        match clip.kind {
                            Kind::Audio => {
                                let clip_index_in_from_track = clip_index;
                                let mut clip_copy =
                                    from_track.audio.clips[clip_index_in_from_track].clone();
                                let offset = (clip.end.x - clip.start.x)
                                    / self.pixels_per_sample().max(1.0e-6);
                                clip_copy.start =
                                    (clip_copy.start as f32 + offset).max(0.0) as usize;
                                let task = self.send(Action::ClipMove {
                                    kind: clip.kind,
                                    from: ClipMoveFrom {
                                        track_name: from_track.name.clone(),
                                        clip_index: clip.index,
                                    },
                                    to: ClipMoveTo {
                                        track_name: to_track.name.clone(),
                                        sample_offset: clip_copy.start,
                                    },
                                    copy: state.ctrl,
                                });
                                self.clip = None;
                                return task;
                            }
                            Kind::MIDI => {
                                let clip_index_in_from_track = clip_index;
                                let mut clip_copy =
                                    from_track.midi.clips[clip_index_in_from_track].clone();
                                let offset = (clip.end.x - clip.start.x)
                                    / self.pixels_per_sample().max(1.0e-6);
                                clip_copy.start =
                                    (clip_copy.start as f32 + offset).max(0.0) as usize;
                                let task = self.send(Action::ClipMove {
                                    kind: clip.kind,
                                    from: ClipMoveFrom {
                                        track_name: from_track.name.clone(),
                                        clip_index: clip.index,
                                    },
                                    to: ClipMoveTo {
                                        track_name: to_track.name.clone(),
                                        sample_offset: clip_copy.start,
                                    },
                                    copy: state.ctrl,
                                });
                                self.clip = None;
                                return task;
                            }
                        }
                    }
                }
            }
            Message::TrackDrag(index) => {
                if self.track.is_none() {
                    let state = self.state.blocking_read();
                    if index < state.tracks.len() {
                        self.track = Some(state.tracks[index].name.clone());
                    }
                }
            }
            Message::TrackDropped(point, _rect) => {
                if self.track.is_some() {
                    return iced_drop::zones_on_point(Message::HandleTrackZones, point, None, None);
                }
            }
            Message::HandleTrackZones(ref zones) => {
                if let Some(index_name) = &self.track
                    && let Some((track_id, _)) = zones.first()
                {
                    let mut state = self.state.blocking_write();
                    if let Some(index) = state.tracks.iter().position(|t| t.name == *index_name) {
                        let moved_track = state.tracks.remove(index);
                        let to_index = state
                            .tracks
                            .iter()
                            .position(|t| Id::from(t.name.clone()) == *track_id); // Compare Id with Id

                        if let Some(t_idx) = to_index {
                            state.tracks.insert(t_idx, moved_track);
                        } else {
                            // If target track not found, insert back to original position (or end)
                            // For simplicity, let's insert it at the end if target not found
                            state.tracks.push(moved_track);
                        }
                    }
                }
            }
            Message::OpenFileImporter => {
                return Task::perform(
                    async {
                        let files = AsyncFileDialog::new()
                            .set_title("Import files")
                            .add_filter("wav", &["wav"])
                            .pick_files()
                            .await;
                        files.map(|handles| {
                            handles
                                .into_iter()
                                .map(|f| f.path().to_path_buf())
                                .collect()
                        })
                    },
                    Message::ImportFilesSelected,
                );
            }
            Message::ImportFilesSelected(Some(ref paths)) => {
                for _path in paths {
                    // TODO
                }
            }
            Message::Workspace => {
                self.state.blocking_write().view = View::Workspace;
            }
            Message::Connections => {
                self.state.blocking_write().view = View::Connections;
            }
            Message::OpenTrackPlugins(track_name) => {
                {
                    let mut state = self.state.blocking_write();
                    state.view = View::TrackPlugins;
                    state.lv2_graph_track = Some(track_name.clone());
                    state.lv2_graph_connecting = None;
                    state.lv2_graph_moving_plugin = None;
                    state.lv2_graph_last_plugin_click = None;
                    state.lv2_graph_selected_plugin = None;
                }
                return self.send(Action::TrackGetLv2Graph { track_name });
            }
            Message::HWSelected(ref hw) => {
                self.state.blocking_write().selected_hw = Some(hw.to_string());
            }
            Message::StartMovingTrackAndSelect(moving_track, track_name) => {
                let mut state = self.state.blocking_write();
                state.moving_track = Some(moving_track);
                return Task::perform(async {}, move |_| {
                    Message::ConnectionViewSelectTrack(track_name)
                });
            }
            _ => {}
        }
        self.update_children(&message);
        Task::none()
    }

    pub fn view(&self) -> iced::Element<'_, Message> {
        let state = self.state.blocking_read();
        if state.hw_loaded {
            match self.modal {
                Some(Show::AddTrack) => self.add_track.view(),
                Some(Show::TrackPluginList) => self.track_plugin_list_view(),
                _ => {
                    let view = match state.view {
                        View::Workspace => self.workspace.view(
                            Some(self.transport_samples),
                            self.pixels_per_sample(),
                            self.beat_pixels(),
                            self.zoom_visible_bars,
                            self.tracks_resize_hovered,
                            self.mixer_resize_hovered,
                            self.recording_preview_bounds(),
                            Some(self.recording_preview_peaks.clone()),
                        ),
                        View::Connections => self.connections.view(),
                        View::TrackPlugins => self.track_plugins.view(),
                    };

                    let mut content = column![self.menu.view(), self.toolbar.view(self.playing, self.record_armed)];
                    if matches!(state.view, View::TrackPlugins) {
                        content = content.push(
                            container(
                                row![button("Plugin List").on_press(Message::Show(
                                    Show::TrackPluginList
                                ))]
                                .spacing(8),
                            )
                            .padding(8),
                        );
                    }
                    content = content.push(view);
                    content = content.push(text(format!("Last message: {}", state.message)));
                    container(content)
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .into()
                }
            }
        } else {
            column![
                self.hw.audio_view(),
                text(format!("Last message: {}", state.message)),
            ]
            .into()
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        fn listener() -> impl Stream<Item = Message> {
            stream::once(CLIENT.subscribe()).flat_map(|receiver| {
                stream::once(async { Message::RefreshLv2Plugins }).chain(stream::unfold(
                    receiver,
                    |mut rx| async move {
                        match rx.recv().await {
                            Some(m) => match m {
                                EngineMessage::Response(r) => {
                                    let result = Message::Response(r);
                                    Some((result, rx))
                                }
                                _ => Some((Message::None, rx)),
                            },
                            None => None,
                        }
                    },
                ))
            })
        }
        let engine_sub = Subscription::run(listener);

        let keyboard_sub = keyboard::listen().map(|event| {
            match event {
            KeyEvent::KeyPressed { key, modifiers, .. } => {
                if modifiers.control()
                    && let keyboard::Key::Character(ch) = &key
                {
                    let s = ch.to_ascii_lowercase();
                    if s == "n" {
                        return Message::NewSession;
                    }
                    if s == "o" {
                        return Message::Show(Show::Open);
                    }
                    if s == "s" {
                        return Message::Show(Show::Save);
                    }
                }
                match key {
                    keyboard::Key::Named(keyboard::key::Named::Space) => Message::ToggleTransport,
                    keyboard::Key::Named(keyboard::key::Named::Shift) => Message::ShiftPressed,
                    keyboard::Key::Named(keyboard::key::Named::Control) => Message::CtrlPressed,
                    keyboard::Key::Named(keyboard::key::Named::Delete) => Message::Remove,
                    _ => Message::None,
                }
            }
            KeyEvent::KeyReleased { key, .. } => match key {
                keyboard::Key::Named(keyboard::key::Named::Shift) => Message::ShiftReleased,
                keyboard::Key::Named(keyboard::key::Named::Control) => Message::CtrlReleased,
                _ => Message::None,
            },
            _ => Message::None,
            }
        });

        let event_sub = event::listen().map(|event| match event {
            event::Event::Mouse(mouse_event) => match mouse_event {
                mouse::Event::CursorMoved { .. } => Message::MouseMoved(mouse_event),
                mouse::Event::ButtonReleased(_) => Message::MouseReleased,
                _ => Message::None,
            },
            event::Event::Window(window::Event::Resized(size)) => Message::WindowResized(size),
            _ => Message::None,
        });

        let playback_sub = if self.playing {
            iced::time::every(PLAYHEAD_UPDATE_INTERVAL).map(|_| Message::PlaybackTick)
        } else {
            Subscription::none()
        };
        let recording_preview_sub = if self.playing
            && self.record_armed
            && self.recording_preview_start_sample.is_some()
        {
            iced::time::every(RECORDING_PREVIEW_UPDATE_INTERVAL)
                .map(|_| Message::RecordingPreviewTick)
        } else {
            Subscription::none()
        };
        let recording_preview_peaks_sub = if self.playing
            && self.record_armed
            && self.recording_preview_start_sample.is_some()
        {
            iced::time::every(RECORDING_PREVIEW_PEAKS_UPDATE_INTERVAL)
                .map(|_| Message::RecordingPreviewPeaksTick)
        } else {
            Subscription::none()
        };

        Subscription::batch(vec![
            engine_sub,
            keyboard_sub,
            event_sub,
            playback_sub,
            recording_preview_sub,
            recording_preview_peaks_sub,
        ])
    }
}
    fn compact_desc(desc: String) -> String {
        desc.split(',').next().unwrap_or(&desc).trim().to_string()
    }
