use super::{AudioClip, MIDIClip};
use crate::message::Message;
use maolan_engine::message::Action;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AudioData {
    pub clips: Vec<AudioClip>,
    pub ins: Vec<usize>,
    pub outs: Vec<usize>,
}

impl AudioData {
    pub fn new(ins: usize, outs: usize) -> Self {
        Self {
            clips: vec![AudioClip::new("".to_string(), 0, 60, 0)],
            ins: vec![0; ins],
            outs: vec![0; outs],
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MIDIData {
    pub clips: Vec<MIDIClip>,
    pub ins: Vec<usize>,
    pub outs: Vec<usize>,
}

impl MIDIData {
    pub fn new(ins: usize, outs: usize) -> Self {
        Self {
            clips: vec![MIDIClip::new("".to_string(), 0, 60, 0)],
            ins: vec![0; ins],
            outs: vec![0; outs],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    id: usize,
    pub name: String,
    pub level: f32,
    pub armed: bool,
    pub muted: bool,
    pub soloed: bool,
    pub height: f32,
    pub audio: AudioData,
    pub midi: MIDIData,
}

impl Track {
    pub fn new(
        name: String,
        level: f32,
        audio_ins: usize,
        audio_outs: usize,
        midi_ins: usize,
        midi_outs: usize,
    ) -> Self {
        Self {
            id: 0,
            name,
            level,
            armed: false,
            muted: false,
            soloed: false,
            audio: AudioData::new(audio_ins, audio_outs),
            midi: MIDIData::new(midi_ins, midi_outs),
            height: 60.0,
        }
    }

    pub fn update(&mut self, message: Message) {
        if let Message::Response(Ok(a)) = message {
            match a {
                Action::TrackLevel(name, level) => {
                    if name == self.name {
                        self.level = level;
                    }
                }
                Action::TrackToggleArm(name) => {
                    if name == self.name {
                        self.armed = !self.armed;
                    }
                }
                Action::TrackToggleMute(name) => {
                    if name == self.name {
                        self.muted = !self.muted;
                    }
                }
                Action::TrackToggleSolo(name) => {
                    if name == self.name {
                        self.soloed = !self.soloed;
                    }
                }
                _ => {}
            }
        }
    }
}
