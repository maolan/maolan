use super::{audio::track::AudioTrack, midi::track::MIDITrack};

#[derive(Debug)]
pub struct Track {
    pub name: String,
    pub level: f32,
    pub armed: bool,
    pub muted: bool,
    pub soloed: bool,
    pub audio: AudioTrack,
    pub midi: MIDITrack,
}

impl Track {
    pub fn new(
        name: String,
        audio_ins: usize,
        audio_outs: usize,
        midi_ins: usize,
        midi_outs: usize,
        buffer_size: usize,
    ) -> Self {
        Self {
            name,
            level: 0.0,
            armed: false,
            muted: false,
            soloed: false,
            audio: AudioTrack::new(audio_ins, audio_outs, buffer_size),
            midi: MIDITrack::new(midi_ins, midi_outs),
        }
    }

    pub fn setup(&mut self) {
        self.audio.setup();
    }

    pub fn process(&mut self) {
        self.midi.process();
        self.audio.process();
    }

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
}
