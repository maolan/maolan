use std::sync::Arc;
use tokio::sync::mpsc::{UnboundedSender as Sender};
use crate::audio::track::Track as AudioTrack;
use crate::midi::track::Track as MIDITrack;
use crate::mutex::UnsafeMutex;

#[derive(Debug, Clone)]
pub enum Track {
    Audio(String, usize),
    MIDI(String),
}

#[derive(Debug, Clone)]
pub enum Message {
    Quit,
    Play,
    Echo(String),
    Ready(usize),
    Finished(usize, String),

    Add(Track),
    ProcessAudio(Arc<UnsafeMutex<AudioTrack>>),
    ProcessMidi(Arc<UnsafeMutex<MIDITrack>>),

    Channel(Sender<Self>),
}

