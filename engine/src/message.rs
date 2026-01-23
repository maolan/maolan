// use std::sync::Arc;
use tokio::sync::mpsc::{UnboundedSender as Sender};
// use crate::audio::track::Track as AudioTrack;
// use crate::midi::track::Track as MIDITrack;
// use crate::mutex::UnsafeMutex;

#[derive(Clone, Debug)]
pub enum Action {
    Quit,
    Play,
    Echo(String),
    Error(String),
    AddAudioTrack(String, usize, usize, usize),
    AddMIDITrack(String, usize, usize),
}

#[derive(Clone, Debug)]
pub enum Message {
    Ready(usize),
    Finished(usize, String),

    // ProcessAudio(Arc<UnsafeMutex<AudioTrack>>),
    // ProcessMidi(Arc<UnsafeMutex<MIDITrack>>),

    Channel(Sender<Self>),

    Request(Action),
    Response(Action),
}
