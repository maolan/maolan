mod menu;
mod message;
mod state;
mod style;
mod widget;
mod workspace;

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

use iced::futures::{Stream, io};
use iced::keyboard::Event as KeyEvent;
use iced::widget::{Id, column, text};
use iced::{Point, Subscription, Task, Theme, event, keyboard, mouse};

use iced_aw::ICED_AW_FONT_BYTES;

use engine::message::{Action, Message as EngineMessage, TrackKind};
use maolan_engine as engine;
use message::{DraggedClip, Message};
use state::{Resizing, State, StateData, Track};

static CLIENT: LazyLock<engine::client::Client> = LazyLock::new(engine::client::Client::default);

pub fn main() -> iced::Result {
    let stdout_layer =
        FmtLayer::new().with_writer(std::io::stdout.with_max_level(tracing::Level::INFO));
    // let logfile = tracing_appender::rolling::hourly("./logs", "maolan.log");
    // let (non_blocking_appender, _guard) = tracing_appender::non_blocking(logfile);
    // let file_layer = FmtLayer::new()
    //     .with_ansi(false)
    //     .with_writer(non_blocking_appender);

    tracing_subscriber::registry()
        .with(stdout_layer)
        // .with(file_layer)
        .init();

    let my_span = span!(Level::INFO, "main");
    let _enter = my_span.enter();

    iced::application(Maolan::default, Maolan::update, Maolan::view)
        .title("Maolan")
        .theme(Theme::Dark)
        .font(ICED_AW_FONT_BYTES)
        .subscription(Maolan::subscription)
        .run()
}

struct Maolan {
    menu: menu::Menu,
    workspace: workspace::Workspace,
    state: State,
    clip: Option<DraggedClip>,
}

impl Default for Maolan {
    fn default() -> Self {
        let state = Arc::new(RwLock::new(StateData::default()));
        Self {
            state: state.clone(),
            menu: menu::Menu::default(),
            workspace: workspace::Workspace::new(state.clone()),
            clip: None,
        }
    }
}

impl Maolan {
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
    fn load(&self, path: String) -> std::io::Result<()> {
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
                CLIENT.send(EngineMessage::Request(Action::AddTrack {
                    name: name.clone(),
                    kind,
                    ins,
                    audio_outs,
                    midi_outs,
                }));
                if let Some(value) = track["armed"].as_bool() {
                    if value {
                        CLIENT.send(EngineMessage::Request(Action::TrackToggleArm(name.clone())));
                    }
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "'armed' is not boolean",
                    ));
                }
                if let Some(value) = track["muted"].as_bool() {
                    if value {
                        CLIENT.send(EngineMessage::Request(Action::TrackToggleMute(
                            name.clone(),
                        )));
                    }
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "'muted' is not boolean",
                    ));
                }
                if let Some(value) = track["soloed"].as_bool() {
                    if value {
                        CLIENT.send(EngineMessage::Request(Action::TrackToggleSolo(
                            name.clone(),
                        )));
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
        Ok(())
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
            Message::Ignore => {
                return Task::none();
            }
            Message::Request(ref a) => {
                CLIENT.send(EngineMessage::Request(a.clone()));
                return Task::none();
            }
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
                _ => {}
            },
            Message::Response(Err(ref e)) => {
                self.state.blocking_write().message = e.clone();
                error!("Engine error: {e}");
            }
            Message::Debug(ref s) => {
                debug!("Maolan::update::debug({s})");
            }
            Message::Save(ref path) => {
                if let Err(s) = self.save(path.clone()) {
                    error!("{}", s);
                }
            }
            Message::Open(ref path) => {
                if let Err(s) = self.load(path.clone()) {
                    error!("{}", s);
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
                for name in &self.state.blocking_read().selected {
                    CLIENT.send(EngineMessage::Request(Action::DeleteTrack(name.clone())));
                }
            }
            Message::TrackResizeStart(ref name) => {
                let mut state = self.state.blocking_write();
                let height = state
                    .tracks
                    .iter()
                    .find(|t| &t.name == name)
                    .map(|t| t.height)
                    .unwrap_or(60.0);
                state.resizing = Some(Resizing::Track(name.clone(), height, state.cursor.y));
            }
            Message::TracksResizeStart => {
                self.state.blocking_write().resizing = Some(Resizing::Tracks);
            }
            Message::MixerResizeStart => {
                self.state.blocking_write().resizing = Some(Resizing::Mixer);
            }
            Message::ClipResizeStart(ref track_name, ref clip_name, is_right_side) => {
                let mut state = self.state.blocking_write();
                if let Some(track) = state.tracks.iter().find(|t| &t.name == track_name)
                    && let Some(clip) = track.clips.iter().find(|c| &c.name == clip_name)
                {
                    let initial_value = if is_right_side {
                        clip.length
                    } else {
                        clip.start
                    };
                    state.resizing = Some(Resizing::Clip(
                        track_name.clone(),
                        clip_name.clone(),
                        is_right_side,
                        initial_value,
                        state.cursor.x,
                    ));
                }
            }
            Message::MouseMoved(mouse::Event::CursorMoved { position }) => {
                self.state.blocking_write().cursor = position;
                if let Some(Resizing::Track(name, initial_height, initial_mouse_y)) =
                    &self.state.blocking_write().resizing
                {
                    let mut state = self.state.blocking_write();
                    let delta = position.y - *initial_mouse_y;
                    if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *name) {
                        track.height = (*initial_height + delta).clamp(60.0, 400.0);
                    }
                } else if let Some(Resizing::Clip(
                    track_name,
                    clip_name,
                    is_right_side,
                    initial_value,
                    initial_mouse_x,
                )) = &self.state.blocking_write().resizing
                {
                    let mut state = self.state.blocking_write();
                    let delta = position.x - initial_mouse_x;
                    if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name)
                        && let Some(clip) = track.clips.iter_mut().find(|c| c.name == *clip_name)
                    {
                        if *is_right_side {
                            clip.length = (initial_value + delta).max(10.0);
                        } else {
                            let new_start = (initial_value + delta).max(0.0);
                            let start_delta = new_start - clip.start;
                            clip.start = new_start;
                            clip.length = (clip.length - start_delta).max(10.0);
                        }
                    }
                }
            }
            Message::MouseReleased => {
                let mut state = self.state.blocking_write();
                state.resizing = None;
            }
            Message::ClipDrag(ref clip) => {
                if self.clip.is_none() {
                    let point = Point::new(clip.point.x - clip.rect.x, clip.point.y - clip.rect.y);
                    self.clip = Some(DraggedClip {
                        point,
                        ..clip.clone()
                    });
                }
            }
            Message::ClipDropped(point, rect) => {
                if let Some(clip) = &mut self.clip {
                    clip.point.x = (point.x - clip.point.x).max(0.0);
                    clip.point.y = (point.y - clip.point.y).max(0.0);
                    clip.rect = rect;
                    return iced_drop::zones_on_point(Message::HandleZones, point, None, None);
                }
            }
            Message::HandleZones(ref zones) => {
                if let Some(clip) = &self.clip
                    && let Some((to_track_name, _zone_rect)) = zones.first()
                {
                    let mut guard = self.state.blocking_write();
                    let from_track_index =
                        guard.tracks.iter().position(|t| t.name == clip.track_name);
                    let to_track_index = guard
                        .tracks
                        .iter()
                        .position(|t| Id::from(t.name.clone()) == *to_track_name);

                    if let (Some(f_idx), Some(t_idx)) = (from_track_index, to_track_index) {
                        let mut clip_copy = guard.tracks[f_idx].clips[clip.index].clone();
                        clip_copy.start = clip.point.x;
                        guard.tracks[t_idx].add_clip(clip_copy);
                    }
                }
                self.clip = None;
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
            use iced::futures::stream;

            stream::unfold(CLIENT.subscribe(), async move |mut receiver| {
                let command = match receiver.recv().await? {
                    EngineMessage::Response(e) => Message::Response(e),
                    _ => Message::Response(Err("failed to receive in unfold".to_string())),
                };

                Some((command, receiver))
            })
        }
        let engine_sub = Subscription::run(listener);

        let keyboard_sub = keyboard::listen().map(|event| match event {
            KeyEvent::KeyPressed { key, .. } => match key {
                keyboard::Key::Named(keyboard::key::Named::Shift) => Message::ShiftPressed,
                keyboard::Key::Named(keyboard::key::Named::Control) => Message::CtrlPressed,
                _ => Message::Ignore,
            },
            KeyEvent::KeyReleased { key, .. } => match key {
                keyboard::Key::Named(keyboard::key::Named::Shift) => Message::ShiftReleased,
                keyboard::Key::Named(keyboard::key::Named::Control) => Message::CtrlReleased,
                _ => Message::Ignore,
            },
            _ => Message::Ignore,
        });

        let mouse_sub = event::listen().map(|event| match event {
            event::Event::Mouse(mouse_event) => match mouse_event {
                mouse::Event::CursorMoved { .. } => Message::MouseMoved(mouse_event),
                mouse::Event::ButtonReleased(_) => Message::MouseReleased,
                _ => Message::Ignore,
            },
            _ => Message::Ignore,
        });

        Subscription::batch(vec![engine_sub, keyboard_sub, mouse_sub])
    }
}
