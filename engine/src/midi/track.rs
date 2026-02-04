use super::clip::MIDIClip;
use crate::{clip::Clip, track::Track};

#[derive(Debug)]
pub struct MIDITrack {
    name: String,
    ins: usize,
    audio_outs: usize,
    midi_outs: usize,
    level: f32,
    armed: bool,
    muted: bool,
    soloed: bool,
    clips: Vec<MIDIClip>,
}

impl MIDITrack {
    pub fn new(name: String, ins: usize, midi_outs: usize, audio_outs: usize) -> Self {
        Self {
            name,
            ins,
            audio_outs,
            midi_outs,
            level: 0.0,
            armed: false,
            muted: false,
            soloed: false,
            clips: vec![MIDIClip::new("".to_string(), 0, 60)],
        }
    }
}

impl Track for MIDITrack {
    fn process(&mut self) {
        // self.buffer.clear();
    }

    fn name(&self) -> String {
        self.name.clone()
    }
    fn set_name(&mut self, name: String) {
        self.name = name;
    }

    fn level(&self) -> f32 {
        self.level
    }
    fn set_level(&mut self, level: f32) {
        self.level = level;
    }

    fn ins(&self) -> usize {
        self.ins
    }
    fn set_ins(&mut self, ins: usize) {
        self.ins = ins;
    }

    fn audio_outs(&self) -> usize {
        self.audio_outs
    }
    fn set_audio_outs(&mut self, outs: usize) {
        self.audio_outs = outs;
    }

    fn midi_outs(&self) -> usize {
        self.midi_outs
    }
    fn set_midi_outs(&mut self, outs: usize) {
        self.midi_outs = outs;
    }

    fn arm(&mut self) {
        self.armed = !self.armed;
    }
    fn mute(&mut self) {
        self.muted = !self.muted;
    }
    fn solo(&mut self) {
        self.soloed = !self.soloed;
    }

    fn add(&mut self, clip: Clip) -> Result<usize, String> {
        match clip {
            Clip::AudioClip { .. } => Err("Tried to add audio clip to MIDI track".to_string()),
            Clip::MIDIClip { .. } => Ok(0),
        }
    }
    fn remove(&mut self, _index: usize) -> Result<usize, String> {
        Ok(0)
    }

    fn at(&self, index: usize) -> Clip {
        Clip::MIDIClip(self.clips[index].clone())
    }
}
