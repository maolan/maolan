use std::sync::Arc;
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
    Ready(usize),
    Finished(usize, String),

    Add(Track),
    ProcessAudio(Arc<UnsafeMutex<AudioTrack>>),
    ProcessMidi(Arc<UnsafeMutex<MIDITrack>>),
}

