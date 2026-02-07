use super::clip::{AudioClip, MIDIClip};

pub struct AudioData {
    pub clips: Vec<AudioClip>,
    pub ins: Vec<usize>,
    pub outs: Vec<usize>,
}

impl AudioData {
    pub fn new(ins: usize, outs: usize) -> Self {
        Self {
            clips: vec![AudioClip::new("".to_string(), 0, 60)],
            ins: vec![0; ins],
            outs: vec![0; outs],
        }
    }
}

pub struct MIDIData {
    pub clips: Vec<MIDIClip>,
    pub ins: Vec<usize>,
    pub outs: Vec<usize>,
}

impl MIDIData {
    pub fn new(ins: usize, outs: usize) -> Self {
        Self {
            clips: vec![MIDIClip::new("".to_string(), 0, 60)],
            ins: vec![0; ins],
            outs: vec![0; outs],
        }
    }
}

pub struct Track {
    pub name: String,
    pub level: f32,
    pub armed: bool,
    pub muted: bool,
    pub soloed: bool,
    pub audio: AudioData,
    pub midi: MIDIData,
}

impl Track {
    pub fn new(
        name: String,
        audio_ins: usize,
        audio_outs: usize,
        midi_ins: usize,
        midi_outs: usize,
    ) -> Self {
        Self {
            name,
            level: 0.0,
            armed: false,
            muted: false,
            soloed: false,
            audio: AudioData::new(audio_ins, audio_outs),
            midi: MIDIData::new(midi_ins, midi_outs),
        }
    }

    pub fn process(&mut self) {}

    pub fn name(&self) -> String {
        self.name.clone()
    }
    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }

    pub fn level(&self) -> f32 {
        self.level
    }
    pub fn set_level(&mut self, level: f32) {
        self.level = level;
    }

    pub fn arm(&mut self) {
        self.armed = !self.armed;
    }
    pub fn mute(&mut self) {
        self.muted = !self.muted;
    }
    pub fn solo(&mut self) {
        self.soloed = !self.soloed;
    }

    // fn add(&mut self, clip: Clip) -> Result<usize, String>;
    // fn remove(&mut self, index: usize) -> Result<usize, String>;
    // fn at(&self, index: usize) -> Result<Clip, String>;
}
