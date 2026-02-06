mod menu;
mod message;
mod state;
mod style;
mod widget;
mod workspace;

use rfd::AsyncFileDialog;
use serde_json::Value;
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
    Length, Pixels, Settings, Size, Subscription, Task, Theme, event, keyboard, mouse, window,
};

use iced_aw::ICED_AW_FONT_BYTES;

use engine::message::{Action, ClipMove, Message as EngineMessage, TrackKind};
use maolan_engine as engine;
use message::{DraggedClip, Message};
use state::{Resizing, State, StateData, Track};

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
    track: Option<usize>,
    workspace: workspace::Workspace,
}

impl Default for Maolan {
    fn default() -> Self {
        let state = Arc::new(RwLock::new(StateData::default()));
        Self {
            clip: None,
            menu: menu::Menu::default(),
            size: Size::new(0.0, 0.0),
            state: state.clone(),
            track: None,
            workspace: workspace::Workspace::new(state.clone()),
        }
    }
}

impl Maolan {
    fn send(&self, action: Action) -> Task<Message> {
        Task::perform(
            async move { CLIENT.send(EngineMessage::Request(action.clone())).await },
            |result| match result {
                Ok(_) => Message::SendMessageFinished(Ok(())),
                Err(_) => Message::Response(Err("Channel closed".to_string())),
            },
        )
    }
    fn save(&self, path: String) -> std::io::Result<()> {
        let filename = "session.json";
        let result = self.workspace.json();
        let mut p = PathBuf::from(path.clone());
        p.push(filename);
        fs::create_dir_all(path)?;
        let file = File::create(&p)?;
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
                let ins = {
                    if let Some(value) = track["ins"].as_u64() {
                        value as usize
                    } else {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "No 'ins' in track",
                        ));
                    }
                };
                let audio_outs = {
                    if let Some(value) = track["audio_outs"].as_u64() {
                        value as usize
                    } else {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "No 'audio_outs' in track",
                        ));
                    }
                };
                let midi_outs = {
                    if let Some(value) = track["midi_outs"].as_u64() {
                        value as usize
                    } else {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "No 'midi_outs' in track",
                        ));
                    }
                };
                let kind = {
                    if let Some(value) = track["track_kind"].as_str() {
                        if value == "Audio" {
                            TrackKind::Audio
                        } else if value == "MIDI" {
                            TrackKind::MIDI
                        } else {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidInput,
                                format!("'track_kind' value '{}' is invalid", value),
                            ));
                        }
                    } else {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "No 'midi_outs' in track",
                        ));
                    }
                };
                tasks.push(self.send(Action::AddTrack {
                    name: name.clone(),
                    kind,
                    ins,
                    audio_outs,
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
        self.workspace.update(message.clone());
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
                    kind,
                    ins,
                    audio_outs,
                    midi_outs,
                } => {
                    let tracks = &mut self.state.blocking_write().tracks;
                    tracks.push(Track::new(
                        name.clone(),
                        *kind,
                        0.0,
                        *ins,
                        *audio_outs,
                        *midi_outs,
                    ));
                }
                Action::DeleteTrack(name) => {
                    let tracks = &mut self.state.blocking_write().tracks;
                    tracks.retain(|track| track.name != *name);
                }
                Action::ClipMove(clip, copy) => {
                    let mut state = self.state.blocking_write();
                    let f_idx = &state
                        .tracks
                        .iter()
                        .position(|track| track.name == clip.from.0);
                    let t_idx = &state
                        .tracks
                        .iter()
                        .position(|track| track.name == clip.to.0);
                    if let (Some(f_idx), Some(t_idx)) = (f_idx, t_idx) {
                        let tracks = &mut state.tracks;
                        let clip_index = clip.from.1;
                        if clip_index < tracks[*f_idx].clips.len() {
                            let mut clip_copy = tracks[*f_idx].clips[clip_index].clone();
                            clip_copy.start = clip.to.1;
                            if !copy {
                                tracks[*f_idx].clips.remove(clip_index);
                            }
                            tracks[*t_idx].clips.push(clip_copy);
                        }
                    }
                }
                _ => {}
            },
            Message::Response(Err(ref e)) => {
                self.state.blocking_write().message = e.clone();
                error!("Engine error: {e}");
            }
            Message::Save(ref path) => {
                if let Err(s) = self.save(path.clone()) {
                    error!("{}", s);
                }
            }
            Message::Open(ref path) => {
                let result = self.load(path.clone());
                match result {
                    Ok(task) => return task,
                    Err(e) => {
                        error!("{}", e);
                        return Task::none();
                    }
                };
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
                if ctrl {
                    if selected {
                        self.state.blocking_write().selected.retain(|n| n != name);
                    } else {
                        self.state.blocking_write().selected.insert(name.clone());
                    }
                } else {
                    self.state.blocking_write().selected.clear();
                    self.state.blocking_write().selected.insert(name.clone());
                }
            }
            Message::DeleteSelectedTracks => {
                let mut tasks = vec![];
                for name in &self.state.blocking_read().selected {
                    tasks.push(self.send(Action::DeleteTrack(name.clone())));
                }
                return Task::batch(tasks);
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
            Message::ClipResizeStart(track_index, clip_index, is_right_side) => {
                let mut state = self.state.blocking_write();
                let track = &state.tracks[track_index];
                let clip = &track.clips[clip_index];
                let initial_value = if is_right_side {
                    clip.length
                } else {
                    clip.start
                };
                state.resizing = Some(Resizing::Clip(
                    track_index,
                    clip_index,
                    is_right_side,
                    initial_value as f32,
                    state.cursor.x,
                ));
            }
            Message::MouseMoved(mouse::Event::CursorMoved { position }) => {
                let mut state = self.state.blocking_write();
                state.cursor = position;
                match state.resizing {
                    Some(Resizing::Track(index, initial_height, initial_mouse_y)) => {
                        let delta = position.y - initial_mouse_y;
                        let track = &mut state.tracks[index];
                        track.height = (initial_height + delta).clamp(60.0, 400.0);
                    }
                    Some(Resizing::Clip(
                        track_index,
                        index,
                        is_right_side,
                        initial_value,
                        initial_mouse_x,
                    )) => {
                        let delta = position.x - initial_mouse_x;
                        let track = &mut state.tracks[track_index];
                        let clip = &mut track.clips[index];
                        if is_right_side {
                            clip.length = (initial_value + delta).max(10.0) as usize;
                        } else {
                            let new_start = (initial_value + delta).max(0.0);
                            let start_delta = new_start - clip.start as f32;
                            clip.start = new_start as usize;
                            clip.length = (clip.length - start_delta as usize).max(10);
                        }
                    }
                    Some(Resizing::Tracks) => {
                        state.tracks_width = Length::Fixed(position.x);
                    }
                    Some(Resizing::Mixer) => {
                        state.mixer_height = Length::Fixed(self.size.height - position.y);
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
                        let mut clip_copy = from_track.clips[clip.index].clone();
                        let offset = clip.end.x - clip.start.x;
                        clip_copy.start = (clip_copy.start as f32 + offset).max(0.0) as usize;
                        let task = self.send(Action::ClipMove(
                            ClipMove {
                                from: (from_track.name.clone(), clip.index),
                                to: (to_track.name.clone(), clip_copy.start),
                            },
                            state.ctrl,
                        ));
                        self.clip = None;
                        return task;
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
            _ => {}
        }
        self.update_children(&message);
        Task::none()
    }

    fn view(&self) -> iced::Element<'_, message::Message> {
        column![
            self.menu.view(),
            self.workspace.view(),
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
