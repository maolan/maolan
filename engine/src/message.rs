use crate::{kind::Kind, mutex::UnsafeMutex, track::Track};
use std::sync::Arc;
use tokio::sync::mpsc::Sender;

#[derive(Clone, Debug)]
pub struct ClipMoveFrom {
    pub track_name: String,
    pub clip_index: usize,
}

#[derive(Clone, Debug)]
pub struct ClipMoveTo {
    pub track_name: String,
    pub sample_offset: usize,
}

#[derive(Clone, Debug)]
pub enum Action {
    Quit,
    Play,
    AddTrack {
        name: String,
        audio_ins: usize,
        midi_ins: usize,
        audio_outs: usize,
        midi_outs: usize,
    },
    AddClip {
        name: String,
        track_name: String,
        start: usize,
        length: usize,
        kind: Kind,
    },
    RemoveClip(usize, String, Kind),
    RemoveTrack(String),
    TrackLevel(String, f32),
    TrackToggleArm(String),
    TrackToggleMute(String),
    TrackToggleSolo(String),
    ClipMove {
        kind: Kind,
        from: ClipMoveFrom,
        to: ClipMoveTo,
        copy: bool,
    },
    Connect {
        from_track: String,
        from_port: usize,
        to_track: String,
        to_port: usize,
        kind: Kind,
    },
    Disconnect {
        from_track: String,
        from_port: usize,
        to_track: String,
        to_port: usize,
        kind: Kind,
    },
    OpenAudioDevice(String),
    HWInfo {
        channels: usize,
        rate: usize,
        input: bool,
    },
}

#[derive(Clone, Debug)]
pub enum Message {
    Ready(usize),
    Finished(usize, String),
    TracksFinished,

    ProcessTrack(Arc<UnsafeMutex<Box<Track>>>),
    Channel(Sender<Self>),

    Request(Action),
    Response(Result<Action, String>),
    HWFinished,
}
