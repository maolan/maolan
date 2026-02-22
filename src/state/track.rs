use super::{AudioClip, MIDIClip};
use crate::message::Message;
use iced::Point;
use maolan_engine::message::Action;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AudioData {
    pub clips: Vec<AudioClip>,
    pub ins: usize,
    pub outs: usize,
}

impl AudioData {
    pub fn new(ins: usize, outs: usize) -> Self {
        Self {
            clips: vec![],
            ins,
            outs,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MIDIData {
    pub clips: Vec<MIDIClip>,
    pub ins: usize,
    pub outs: usize,
}

impl MIDIData {
    pub fn new(ins: usize, outs: usize) -> Self {
        Self {
            clips: vec![],
            ins,
            outs,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "Point")]
struct PointDef {
    x: f32,
    y: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    id: usize,
    pub name: String,
    pub level: f32,
    pub balance: f32,
    #[serde(skip, default)]
    pub meter_out_db: Vec<f32>,
    pub armed: bool,
    pub muted: bool,
    pub soloed: bool,
    pub input_monitor: bool,
    pub disk_monitor: bool,
    pub height: f32,
    pub audio: AudioData,
    pub midi: MIDIData,
    #[serde(with = "PointDef")]
    pub position: Point,
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
            balance: 0.0,
            meter_out_db: vec![-90.0; audio_outs],
            armed: false,
            muted: false,
            soloed: false,
            input_monitor: false,
            disk_monitor: true,
            audio: AudioData::new(audio_ins, audio_outs),
            midi: MIDIData::new(midi_ins, midi_outs),
            height: 60.0,
            position: Point::new(100.0, 100.0),
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
                Action::TrackBalance(name, balance) => {
                    if name == self.name {
                        self.balance = balance;
                    }
                }
                Action::TrackMeters {
                    track_name,
                    output_db,
                } => {
                    if track_name == self.name {
                        self.meter_out_db = output_db;
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
                Action::TrackToggleInputMonitor(name) => {
                    if name == self.name {
                        self.input_monitor = !self.input_monitor;
                    }
                }
                Action::TrackToggleDiskMonitor(name) => {
                    if name == self.name {
                        self.disk_monitor = !self.disk_monitor;
                    }
                }
                _ => {}
            }
        }
    }
}
