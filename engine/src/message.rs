// use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender as Sender;
// use crate::audio::track::Track as AudioTrack;
// use crate::midi::track::Track as MIDITrack;
// use crate::mutex::UnsafeMutex;

#[derive(Debug, Clone, PartialEq, Copy, Eq)]
pub enum TrackKind {
    Audio,
    MIDI,
}

impl std::fmt::Display for TrackKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Audio => "Audio",
            Self::MIDI => "MIDI",
        })
    }
}

#[derive(Clone, Debug)]
pub enum Action {
    Quit,
    Play,
    AddTrack {
        name: String,
        kind: TrackKind,
        ins: usize,
        audio_outs: usize,
        midi_outs: usize,
    },
    DeleteTrack(String),
    TrackLevel(String, f32),
    TrackIns(String, usize),
    TrackAudioOuts(String, usize),
    TrackMIDIOuts(String, usize),
    TrackToggleArm(String),
    TrackToggleMute(String),
    TrackToggleSolo(String),
}

#[derive(Clone, Debug)]
pub enum Message {
    Ready(usize),
    Finished(usize, String),

    // ProcessAudio(Arc<UnsafeMutex<AudioTrack>>),
    // ProcessMidi(Arc<UnsafeMutex<MIDITrack>>),
    Channel(Sender<Self>),

    Request(Action),
    Response(Result<Action, String>),
}
