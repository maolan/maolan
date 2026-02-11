mod connections;
mod menu;
mod message;
mod state;
mod style;
mod toolbar;
mod widget;
mod workspace;

use rfd::AsyncFileDialog;
use serde_json::{Value, json};
use std::fs::{self, File};
use std::io::BufReader;
use std::path::PathBuf;
use std::process::exit;
use std::sync::{Arc, LazyLock};
use tokio::sync::RwLock;
use tracing::{Level, debug, error, span};
use tracing_subscriber::{
    fmt::{Layer as FmtLayer, writer::MakeWriterExt},
    prelude::*,
};

use iced::futures::{Stream, StreamExt, io, stream};
use iced::keyboard::Event as KeyEvent;
use iced::widget::{Id, column, text};
use iced::{
    Length, Pixels, Point, Settings, Size, Subscription, Task, Theme, event, keyboard, mouse,
    window,
};

use iced_aw::ICED_AW_FONT_BYTES;

use engine::{
    kind::Kind,
    message::{Action, ClipMoveFrom, ClipMoveTo, Message as EngineMessage},
};
use maolan_engine as engine;
use message::{DraggedClip, Message};
use state::{Resizing, State, StateData, Track, View};

static CLIENT: LazyLock<engine::client::Client> = LazyLock::new(engine::client::Client::default);

pub fn main() -> iced::Result {
    let stdout_layer =
        FmtLayer::new().with_writer(std::io::stdout.with_max_level(tracing::Level::INFO));

    tracing_subscriber::registry().with(stdout_layer).init();

    let my_span = span!(Level::INFO, "main");
    let _enter = my_span.enter();
    let settings = Settings {
        default_text_size: Pixels(16.0),
        ..Default::default()
    };

    iced::application(Maolan::default, Maolan::update, Maolan::view)
        .title("Maolan")
        .settings(settings)
        .theme(Theme::Dark)
        .font(ICED_AW_FONT_BYTES)
        .subscription(Maolan::subscription)
        .run()
}

struct Maolan {
    clip: Option<DraggedClip>,
    menu: menu::Menu,
    size: Size,
    state: State,
    toolbar: toolbar::Toolbar,
    track: Option<usize>,
    workspace: workspace::Workspace,
    connections: connections::Connections,
}

impl Default for Maolan {
    fn default() -> Self {
        let state = Arc::new(RwLock::new(StateData::default()));
        Self {
            clip: None,
            menu: menu::Menu::default(),
            size: Size::new(0.0, 0.0),
            state: state.clone(),
            toolbar: toolbar::Toolbar::new(state.clone()),
            track: None,
            workspace: workspace::Workspace::new(state.clone()),
            connections: connections::Connections::new(state.clone()),
        }
    }
}

impl Maolan {
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

                // Load audio clips
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

                // Load MIDI clips
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

    fn update_children(&mut self, message: &message::Message) {
        self.menu.update(message.clone());
        self.toolbar.update(message.clone());
        self.workspace.update(message.clone());
        self.connections.update(message.clone());
        for track in &mut self.state.blocking_write().tracks {
            track.update(message.clone());
        }
    }

    fn update(&mut self, message: message::Message) -> Task<Message> {
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
                    Show::AddTrack => {}
                }
            }
            Message::Request(ref a) => return self.send(a.clone()),
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
                }
                Action::RemoveTrack(name) => {
                    let tracks = &mut self.state.blocking_write().tracks;
                    tracks.retain(|track| track.name != *name);
                }
                Action::ClipMove {
                    kind,
                    from,
                    to,
                    copy,
                } => {
                    let mut state = self.state.blocking_write();
                    let f_idx = &state
                        .tracks
                        .iter()
                        .position(|track| track.name == from.track_name);
                    let t_idx = &state
                        .tracks
                        .iter()
                        .position(|track| track.name == to.track_name);
                    if let (Some(f_idx), Some(t_idx)) = (f_idx, t_idx) {
                        let tracks = &mut state.tracks;
                        let clip_index = from.clip_index;
                        match kind {
                            Kind::Audio => {
                                if clip_index < tracks[*f_idx].audio.clips.len() {
                                    let mut clip_copy =
                                        tracks[*f_idx].audio.clips[clip_index].clone();
                                    clip_copy.start = to.sample_offset;
                                    if !copy {
                                        tracks[*f_idx].audio.clips.remove(clip_index);
                                    }
                                    tracks[*t_idx].audio.clips.push(clip_copy);
                                }
                            }
                            Kind::MIDI => {
                                if clip_index < tracks[*f_idx].midi.clips.len() {
                                    let mut clip_copy =
                                        tracks[*f_idx].midi.clips[clip_index].clone();
                                    clip_copy.start = to.sample_offset;
                                    if !copy {
                                        tracks[*f_idx].midi.clips.remove(clip_index);
                                    }
                                    tracks[*t_idx].midi.clips.push(clip_copy);
                                }
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
                    let from_idx = state
                        .tracks
                        .iter()
                        .position(|track| &track.name == from_track);
                    let to_idx = state
                        .tracks
                        .iter()
                        .position(|track| &track.name == to_track);

                    if let (Some(from_idx), Some(to_idx)) = (from_idx, to_idx) {
                        state.connections.push(crate::state::Connection {
                            from_track: from_idx,
                            from_port: *from_port,
                            to_track: to_idx,
                            to_port: *to_port,
                            kind: *kind,
                        });
                        state.message = format!(
                            "Connected {} port {} to {} port {}",
                            from_track, from_port, to_track, to_port
                        );
                    }
                }
                Action::Disconnect {
                    from_track,
                    from_port,
                    to_track,
                    to_port,
                    kind,
                } => {
                    let mut state = self.state.blocking_write();
                    let from_idx = state
                        .tracks
                        .iter()
                        .position(|track| &track.name == from_track);
                    let to_idx = state
                        .tracks
                        .iter()
                        .position(|track| &track.name == to_track);

                    if let (Some(from_idx), Some(to_idx)) = (from_idx, to_idx) {
                        // Find and remove the matching connection
                        state.connections.retain(|conn| {
                            !(conn.from_track == from_idx
                                && conn.from_port == *from_port
                                && conn.to_track == to_idx
                                && conn.to_port == *to_port
                                && conn.kind == *kind)
                        });
                        state.message = format!(
                            "Disconnected {} port {} from {} port {}",
                            from_track, from_port, to_track, to_port
                        );
                    }
                }
                _ => {}
            },
            Message::Response(Err(ref e)) => {
                self.state.blocking_write().message = e.clone();
                error!("Engine error: {e}");
            }
            Message::SaveFolderSelected(ref path_opt) => {
                if let Some(path) = path_opt {
                    if let Err(s) = self.save(path.to_string_lossy().to_string()) {
                        error!("{}", s);
                    }
                }
            }
            Message::OpenFolderSelected(ref path_opt) => {
                if let Some(path) = path_opt {
                    let result = self.load(path.to_string_lossy().to_string());
                    match result {
                        Ok(task) => return task,
                        Err(e) => {
                            error!("{}", e);
                            return Task::none();
                        }
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
                use crate::state::ConnectionViewSelection;
                let ctrl = self.state.blocking_read().ctrl;
                let selected = self.state.blocking_read().selected.contains(name);
                let mut state = self.state.blocking_write();

                // Find track index
                let track_idx = state.tracks.iter().position(|t| &t.name == name);

                if ctrl {
                    if selected {
                        state.selected.retain(|n| n != name);
                        // Also remove from connections view selection
                        if let Some(idx) = track_idx {
                            if let ConnectionViewSelection::Tracks(set) = &mut state.connection_view_selection {
                                set.remove(&idx);
                            }
                        }
                    } else {
                        state.selected.insert(name.clone());
                        // Also add to connections view selection
                        if let Some(idx) = track_idx {
                            match &mut state.connection_view_selection {
                                ConnectionViewSelection::Tracks(set) => {
                                    set.insert(idx);
                                }
                                _ => {
                                    let mut set = std::collections::HashSet::new();
                                    set.insert(idx);
                                    state.connection_view_selection = ConnectionViewSelection::Tracks(set);
                                }
                            }
                        }
                    }
                } else {
                    state.selected.clear();
                    state.selected.insert(name.clone());
                    // Also sync to connections view selection
                    if let Some(idx) = track_idx {
                        let mut set = std::collections::HashSet::new();
                        set.insert(idx);
                        state.connection_view_selection = ConnectionViewSelection::Tracks(set);
                    }
                }
            }
            Message::RemoveSelectedTracks => {
                let mut tasks = vec![];
                for name in &self.state.blocking_read().selected {
                    tasks.push(self.send(Action::RemoveTrack(name.clone())));
                }
                return Task::batch(tasks);
            }
            Message::ConnectionViewSelectTrack(idx) => {
                use crate::state::ConnectionViewSelection;
                let ctrl = self.state.blocking_read().ctrl;
                let mut state = self.state.blocking_write();

                // Get track name before modifying state
                let track_name = state.tracks.get(idx).map(|t| t.name.clone());

                match &mut state.connection_view_selection {
                    ConnectionViewSelection::Tracks(set) if ctrl => {
                        // Ctrl + click: toggle in current selection
                        if set.contains(&idx) {
                            set.remove(&idx);
                            // Also remove from workspace selection
                            if let Some(name) = track_name {
                                state.selected.remove(&name);
                            }
                        } else {
                            set.insert(idx);
                            // Also add to workspace selection
                            if let Some(name) = track_name {
                                state.selected.insert(name);
                            }
                        }
                    }
                    _ => {
                        // New selection or switching from connections to tracks
                        let mut set = std::collections::HashSet::new();
                        set.insert(idx);
                        state.connection_view_selection = ConnectionViewSelection::Tracks(set);

                        // Sync to workspace selection
                        state.selected.clear();
                        if let Some(name) = track_name {
                            state.selected.insert(name);
                        }
                    }
                }
            }
            Message::SelectClip { track_idx, clip_idx, kind } => {
                use crate::state::ClipId;
                let ctrl = self.state.blocking_read().ctrl;
                let mut state = self.state.blocking_write();

                let clip_id = ClipId {
                    track_idx,
                    clip_idx,
                    kind,
                };

                if ctrl {
                    // Toggle clip in selection
                    if state.selected_clips.contains(&clip_id) {
                        state.selected_clips.remove(&clip_id);
                    } else {
                        state.selected_clips.insert(clip_id);
                    }
                } else {
                    // Replace selection with this clip
                    state.selected_clips.clear();
                    state.selected_clips.insert(clip_id);
                }
            }
            Message::DeselectAll => {
                use crate::state::ConnectionViewSelection;
                let mut state = self.state.blocking_write();
                state.selected.clear();
                state.selected_clips.clear();
                state.connection_view_selection = ConnectionViewSelection::None;
            }
            Message::ConnectionViewSelectConnection(idx) => {
                use crate::state::ConnectionViewSelection;
                let ctrl = self.state.blocking_read().ctrl;
                let mut state = self.state.blocking_write();

                match &mut state.connection_view_selection {
                    ConnectionViewSelection::Connections(set) if ctrl => {
                        // Ctrl + click: toggle in current selection
                        if set.contains(&idx) {
                            set.remove(&idx);
                        } else {
                            set.insert(idx);
                        }
                    }
                    _ => {
                        // New selection or switching from tracks to connections
                        let mut set = std::collections::HashSet::new();
                        set.insert(idx);
                        state.connection_view_selection = ConnectionViewSelection::Connections(set);
                    }
                }
            }
            Message::RemoveSelected => {
                use crate::state::ConnectionViewSelection;
                let state = self.state.blocking_read();
                match &state.connection_view_selection {
                    ConnectionViewSelection::Tracks(set) => {
                        let mut tasks = vec![];
                        for &idx in set {
                            if let Some(track) = state.tracks.get(idx) {
                                tasks.push(self.send(Action::RemoveTrack(track.name.clone())));
                            }
                        }
                        drop(state);
                        self.state.blocking_write().connection_view_selection =
                            ConnectionViewSelection::None;
                        return Task::batch(tasks);
                    }
                    ConnectionViewSelection::Connections(set) => {
                        let mut tasks = vec![];
                        for &idx in set {
                            if let Some(conn) = state.connections.get(idx)
                                && let (Some(from_track), Some(to_track)) = (
                                    state.tracks.get(conn.from_track),
                                    state.tracks.get(conn.to_track),
                                )
                            {
                                tasks.push(self.send(Action::Disconnect {
                                    from_track: from_track.name.clone(),
                                    from_port: conn.from_port,
                                    to_track: to_track.name.clone(),
                                    to_port: conn.to_port,
                                    kind: conn.kind,
                                }));
                            }
                        }
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
                }
            }
            Message::TrackResizeStart(index) => {
                let mut state = self.state.blocking_write();
                let height = state.tracks[index].height;
                state.resizing = Some(Resizing::Track(index, height, state.cursor.y));
            }
            Message::TracksResizeStart => {
                self.state.blocking_write().resizing = Some(Resizing::Tracks);
            }
            Message::MixerResizeStart => {
                self.state.blocking_write().resizing = Some(Resizing::Mixer);
            }
            Message::ClipResizeStart(ref kind, track_index, clip_index, is_right_side) => {
                let mut state = self.state.blocking_write();
                let track = &state.tracks[track_index];
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
                            track_index,
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
                            track_index,
                            clip_index,
                            is_right_side,
                            initial_value as f32,
                            state.cursor.x,
                        ));
                    }
                }
            }
            Message::MouseMoved(mouse::Event::CursorMoved { position }) => {
                let resizing = self.state.blocking_read().resizing.clone();
                self.state.blocking_write().cursor = position;
                match resizing {
                    Some(Resizing::Track(index, initial_height, initial_mouse_y)) => {
                        let mut state = self.state.blocking_write();
                        let delta = position.y - initial_mouse_y;
                        let track = &mut state.tracks[index];
                        track.height = (initial_height + delta).clamp(60.0, 400.0);
                    }
                    Some(Resizing::Clip(
                        kind,
                        track_index,
                        index,
                        is_right_side,
                        initial_value,
                        initial_mouse_x,
                    )) => {
                        let mut state = self.state.blocking_write();
                        let delta = position.x - initial_mouse_x;
                        let track = &mut state.tracks[track_index];
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
                    && let Some((to_track_name, _)) = zones.first()
                {
                    let state = self.state.blocking_read();
                    let f_idx = clip.track_index;
                    let to_track_index = state
                        .tracks
                        .iter()
                        .position(|t| Id::from(t.name.clone()) == *to_track_name);

                    if let Some(t_idx) = to_track_index {
                        let from_track = &state.tracks[f_idx];
                        let to_track = &state.tracks[t_idx];
                        match clip.kind {
                            Kind::Audio => {
                                let mut clip_copy = from_track.audio.clips[clip.index].clone();
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
                                let mut clip_copy = from_track.midi.clips[clip.index].clone();
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
                    self.track = Some(index);
                }
            }
            Message::TrackDropped(point, _rect) => {
                if self.track.is_some() {
                    return iced_drop::zones_on_point(Message::HandleTrackZones, point, None, None);
                }
            }
            Message::HandleTrackZones(ref zones) => {
                if let Some(index) = &self.track
                    && let Some((track_name, _)) = zones.first()
                {
                    let mut state = self.state.blocking_write();
                    if *index < state.tracks.len() {
                        let clip = state.tracks.remove(*index);
                        let to_index = state
                            .tracks
                            .iter()
                            .position(|t| Id::from(t.name.clone()) == *track_name);
                        if let Some(t_idx) = to_index {
                            state.tracks.insert(t_idx, clip);
                        } else {
                            state.tracks.insert(*index, clip);
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
            _ => {}
        }
        self.update_children(&message);
        Task::none()
    }

    fn view(&self) -> iced::Element<'_, message::Message> {
        let view = match self.state.blocking_read().view {
            View::Workspace => self.workspace.view(),
            View::Connections => self.connections.view(),
        };
        column![
            self.menu.view(),
            self.toolbar.view(),
            view,
            text(format!(
                "Last message: {}",
                self.state.blocking_read().message
            ))
        ]
        .into()
    }

    fn subscription(&self) -> Subscription<message::Message> {
        fn listener() -> impl Stream<Item = message::Message> {
            stream::once(CLIENT.subscribe()).flat_map(|receiver| {
                stream::unfold(receiver, |mut rx| async move {
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
                })
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
