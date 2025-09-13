use std::sync::{Arc, RwLock};
use crate::audio::track::Track as AudioTrack;
use crate::midi::track::Track as MIDITrack;

pub enum Track {
    Audio(String, usize),
    MIDI(String),
}

pub enum Message {
    Quit,
    Play,
    Ready(usize),
    Finished(usize, String),

    Add(Track),
    ProcessAudio(Arc<RwLock<AudioTrack>>),
    ProcessMidi(Arc<RwLock<MIDITrack>>),
}

