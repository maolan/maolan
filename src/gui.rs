use crate::{
    add_track, connections, hw, menu,
    message::{DraggedClip, Message, Show},
    state::{ConnectionViewSelection, HW, Resizing, State, StateData, Track, View},
    toolbar, workspace,
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
use std::collections::BTreeSet;
use std::{
    fs::{self, File},
    io::BufReader,
    path::{Path, PathBuf},
    process::{Command, exit},
    sync::{Arc, LazyLock},
};
use tokio::sync::RwLock;
use tracing::{debug, error};

static CLIENT: LazyLock<engine::client::Client> = LazyLock::new(engine::client::Client::default);

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
        }
    }
}

impl Maolan {
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
        let mut p = PathBuf::from(path.clone());
        p.push(filename);
        fs::create_dir_all(path)?;
        let file = File::create(&p)?;
        let result = json!({
            "tracks": &self.state.blocking_read().tracks,
            "connections": &self.state.blocking_read().connections,
        });
        serde_json::to_writer_pretty(file, &result)?;
        Ok(())
    }

    fn load(&self, path: String) -> std::io::Result<Task<Message>> {
        let mut tasks = vec![];
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
                    if let Some(value) = track["audio"]["outs"].as_u64() {
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

                        tasks.push(self.send(Action::AddClip {
                            name: clip_name,
                            track_name: name.clone(),
                            start,
                            length,
                            kind: Kind::Audio,
                        }));
                    }
                }

                if let Some(midi_clips) = track["midi"]["clips"].as_array() {
                    for clip in midi_clips {
                        let clip_name = clip["name"].as_str().unwrap_or("").to_string();
                        let start = clip["start"].as_u64().unwrap_or(0) as usize;
                        let length = clip["length"].as_u64().unwrap_or(0) as usize;

                        tasks.push(self.send(Action::AddClip {
                            name: clip_name,
                            track_name: name.clone(),
                            start,
                            length,
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
        Ok(Task::batch(tasks))
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
            Message::WindowResized(size) => {
                self.size = size;
            }
            Message::Show(ref show) => {
                use crate::message::Show;
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
            Message::Cancel => self.modal = None,
            Message::Request(ref a) => return self.send(a.clone()),
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
            Message::LoadLv2Plugin(ref plugin_uri) => {
                let track_name = {
                    let state = self.state.blocking_read();
                    state
                        .lv2_graph_track
                        .clone()
                        .or_else(|| state.selected.iter().next().cloned())
                };
                if let Some(track_name) = track_name {
                    return self.send(Action::TrackLoadLv2Plugin {
                        track_name,
                        plugin_uri: plugin_uri.clone(),
                    });
                }
                self.state.blocking_write().message =
                    "Select a track before loading LV2 plugin".to_string();
            }
            Message::ShowLv2PluginUi(ref plugin_uri) => {
                let track_name = {
                    let state = self.state.blocking_read();
                    state
                        .lv2_graph_track
                        .clone()
                        .or_else(|| state.selected.iter().next().cloned())
                };
                if let Some(track_name) = track_name {
                    return self.send(Action::TrackShowLv2PluginUi {
                        track_name,
                        plugin_uri: plugin_uri.clone(),
                    });
                }
                self.state.blocking_write().message =
                    "Select a track before opening LV2 UI".to_string();
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
                    kind,
                } => {
                    let mut state = self.state.blocking_write();
                    if let Some(track) = state.tracks.iter_mut().find(|t| &t.name == track_name) {
                        match kind {
                            Kind::Audio => {
                                track.audio.clips.push(crate::state::AudioClip {
                                    name: name.clone(),
                                    start: *start,
                                    length: *length,
                                    offset: 0,
                                });
                            }
                            Kind::MIDI => {
                                track.midi.clips.push(crate::state::MIDIClip {
                                    name: name.clone(),
                                    start: *start,
                                    length: *length,
                                    offset: 0,
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
                    let mut state = self.state.blocking_write();
                    if *input {
                        state.hw_in = Some(HW {
                            channels: *channels,
                            rate: *rate,
                            input: *input,
                        });
                    } else {
                        state.hw_out = Some(HW {
                            channels: *channels,
                            rate: *rate,
                            input: *input,
                        });
                    }
                }
                Action::Lv2Plugins(plugins) => {
                    let mut state = self.state.blocking_write();
                    state.lv2_plugins = plugins.clone();
                    state.message = format!("Loaded {} LV2 plugins", state.lv2_plugins.len());
                }
                Action::TrackLoadLv2Plugin { track_name, .. }
                | Action::TrackUnloadLv2Plugin { track_name, .. }
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
                _ => {
                    println!("Response: {:?}", a);
                }
            },
            Message::Response(Err(ref e)) => {
                self.state.blocking_write().message = e.clone();
                error!("Engine error: {e}");
            }
            Message::SaveFolderSelected(ref path_opt) => {
                if let Some(path) = path_opt
                    && let Err(s) = self.save(path.to_string_lossy().to_string())
                {
                    error!("{}", s);
                }
            }
            Message::OpenFolderSelected(Some(path)) => {
                match self.load(path.to_string_lossy().to_string()) {
                    Ok(task) => return task,
                    Err(e) => {
                        error!("{}", e);
                        return Task::none();
                    }
                }
            }
            Message::ShiftPressed => {
                self.state.blocking_write().shift = true;
            }
            Message::ShiftReleased => {
                self.state.blocking_write().shift = false;
            }
            Message::CtrlPressed => {
                self.state.blocking_write().ctrl = true;
            }
            Message::CtrlReleased => {
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
                self.state.blocking_write().resizing = Some(Resizing::Tracks);
            }
            Message::MixerResizeStart => {
                self.state.blocking_write().resizing = Some(Resizing::Mixer);
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
                        let mut state = self.state.blocking_write();
                        if let Some(track) = state.tracks.iter_mut().find(|t| t.name == track_name)
                        {
                            let delta = position.x - initial_mouse_x;
                            match kind {
                                Kind::Audio => {
                                    let clip = &mut track.audio.clips[index];
                                    if is_right_side {
                                        clip.length = (initial_value + delta).max(10.0) as usize;
                                    } else {
                                        let new_start = (initial_value + delta).max(0.0);
                                        let start_delta = new_start - clip.start as f32;
                                        clip.start = new_start as usize;
                                        clip.length = (clip.length - start_delta as usize).max(10);
                                    }
                                }
                                Kind::MIDI => {
                                    let clip = &mut track.midi.clips[index];
                                    if is_right_side {
                                        clip.length = (initial_value + delta).max(10.0) as usize;
                                    } else {
                                        let new_start = (initial_value + delta).max(0.0);
                                        let start_delta = new_start - clip.start as f32;
                                        clip.start = new_start as usize;
                                        clip.length = (clip.length - start_delta as usize).max(10);
                                    }
                                }
                            }
                        }
                    }
                    Some(Resizing::Tracks) => {
                        self.state.blocking_write().tracks_width = Length::Fixed(position.x);
                    }
                    Some(Resizing::Mixer) => {
                        self.state.blocking_write().mixer_height =
                            Length::Fixed(self.size.height - position.y);
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
                                let offset = clip.end.x - clip.start.x;
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
                                let offset = clip.end.x - clip.start.x;
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
            Message::CloseTrackPlugins => {
                let mut state = self.state.blocking_write();
                state.view = View::Connections;
                state.lv2_graph_connecting = None;
                state.lv2_graph_moving_plugin = None;
                state.lv2_graph_last_plugin_click = None;
                state.lv2_graph_selected_plugin = None;
            }
            Message::RefreshTrackLv2Graph => {
                let track_name = self.state.blocking_read().lv2_graph_track.clone();
                if let Some(track_name) = track_name {
                    return self.send(Action::TrackGetLv2Graph { track_name });
                }
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
                        View::Workspace => self.workspace.view(),
                        View::Connections => self.connections.view(),
                        View::TrackPlugins => self.track_plugins.view(),
                    };

                    let mut content = column![self.menu.view(), self.toolbar.view()];
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

                    content.into()
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

        let keyboard_sub = keyboard::listen().map(|event| match event {
            KeyEvent::KeyPressed { key, .. } => match key {
                keyboard::Key::Named(keyboard::key::Named::Shift) => Message::ShiftPressed,
                keyboard::Key::Named(keyboard::key::Named::Control) => Message::CtrlPressed,
                keyboard::Key::Named(keyboard::key::Named::Delete) => Message::Remove,
                _ => Message::None,
            },
            KeyEvent::KeyReleased { key, .. } => match key {
                keyboard::Key::Named(keyboard::key::Named::Shift) => Message::ShiftReleased,
                keyboard::Key::Named(keyboard::key::Named::Control) => Message::CtrlReleased,
                _ => Message::None,
            },
            _ => Message::None,
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

        Subscription::batch(vec![engine_sub, keyboard_sub, event_sub])
    }
}
    fn compact_desc(desc: String) -> String {
        desc.split(',').next().unwrap_or(&desc).trim().to_string()
    }
