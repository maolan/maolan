use maolan_engine::message::Action;
use crate::message::Message;

#[derive(Debug)]
pub enum TrackType {
    Audio,
    MIDI,
}

#[derive(Debug)]
pub struct Track {
    pub name: String,
    pub level: f32,
    pub track_type: TrackType,
    pub ins: usize,
    pub audio_outs: usize,
    pub midi_outs: usize,
    pub armed: bool,
    pub muted: bool,
    pub soloed: bool,
}

impl Track {
    pub fn new(
        name: String,
        level: f32,
        ins: usize,
        track_type: TrackType,
        audio_outs: usize,
        midi_outs: usize,
    ) -> Self {
        Self {
            name,
            level,
            ins,
            track_type,
            audio_outs,
            midi_outs,
            armed: false,
            muted: false,
            soloed: false,
        }
    }
}

impl Track {
    pub fn update(&mut self, message: Message) {
        match message {
            Message::Response(Ok(a)) => {
                match a {
                    Action::TrackLevel(name, level) => {
                        if name == self.name {
                            self.level = level;
                        }
                    }
                    Action::TrackIns(name, ins) => {
                        if name == self.name {
                            self.ins = ins;
                        }
                    }
                    Action::TrackAudioOuts(name, outs) => {
                        if name == self.name {
                            self.audio_outs = outs;
                        }
                    }
                    Action::TrackMIDIOuts(name, outs) => {
                        if name == self.name {
                            self.midi_outs = outs;
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
            _ => {}
        }
    }
}
