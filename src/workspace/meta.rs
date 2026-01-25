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
        }
    }
}

impl Track {
    pub fn update(&mut self, message: Message) {
        match message {
            Message::Request(a) => {
                match a {
                    Action::TrackLevel(name, level) => {
                        if name == self.name {
                            self.level = level;
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}
