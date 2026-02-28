use super::{AudioClip, MIDIClip};
use crate::message::Message;
use iced::Point;
use maolan_engine::message::Action;
use serde::{Deserialize, Serialize};

pub const TRACK_FOLDER_HEADER_HEIGHT: f32 = 24.0;
pub const TRACK_SUBTRACK_GAP: f32 = 2.0;
pub const TRACK_SUBTRACK_MIN_HEIGHT: f32 = 20.0;

#[derive(Debug, Clone, Copy)]
pub struct TrackLaneLayout {
    pub header_height: f32,
    pub lane_height: f32,
    pub audio_lanes: usize,
    pub midi_lanes: usize,
}

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
        let mut track = Self {
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
        };
        track.height = track.min_height_for_layout().max(60.0);
        track
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

    pub fn audio_lane_count(&self) -> usize {
        // Always return 1 audio lane if we have any audio inputs
        if self.audio.ins > 0 {
            1
        } else {
            0
        }
    }

    pub fn midi_lane_count(&self) -> usize {
        self.midi.ins
    }

    pub fn total_lane_count(&self) -> usize {
        self.audio_lane_count()
            .saturating_add(self.midi_lane_count())
    }

    pub fn min_height_for_layout(&self) -> f32 {
        let lanes = self.total_lane_count().max(1);
        TRACK_FOLDER_HEADER_HEIGHT
            + (lanes as f32 * TRACK_SUBTRACK_MIN_HEIGHT)
            + ((lanes.saturating_sub(1)) as f32 * TRACK_SUBTRACK_GAP)
            + 8.0
    }

    pub fn lane_layout(&self) -> TrackLaneLayout {
        let total_lanes = self.total_lane_count().max(1);
        let available = (self.height - TRACK_FOLDER_HEADER_HEIGHT - 8.0).max(0.0);
        let gaps = (total_lanes.saturating_sub(1)) as f32 * TRACK_SUBTRACK_GAP;
        let lane_height = ((available - gaps) / total_lanes as f32).max(TRACK_SUBTRACK_MIN_HEIGHT);
        TrackLaneLayout {
            header_height: TRACK_FOLDER_HEADER_HEIGHT,
            lane_height,
            audio_lanes: self.audio_lane_count(),
            midi_lanes: self.midi_lane_count(),
        }
    }

    pub fn lane_top(&self, kind: maolan_engine::kind::Kind, lane: usize) -> f32 {
        let layout = self.lane_layout();
        let mut y = layout.header_height;
        match kind {
            maolan_engine::kind::Kind::Audio => {
                y + lane.min(layout.audio_lanes.saturating_sub(1)) as f32
                    * (layout.lane_height + TRACK_SUBTRACK_GAP)
            }
            maolan_engine::kind::Kind::MIDI => {
                y += layout.audio_lanes as f32 * (layout.lane_height + TRACK_SUBTRACK_GAP);
                y + lane.min(layout.midi_lanes.saturating_sub(1)) as f32
                    * (layout.lane_height + TRACK_SUBTRACK_GAP)
            }
        }
    }

    pub fn lane_index_at_y(&self, kind: maolan_engine::kind::Kind, y: f32) -> usize {
        let layout = self.lane_layout();
        let lane_span = layout.lane_height + TRACK_SUBTRACK_GAP;
        let local = (y - layout.header_height).max(0.0);
        match kind {
            maolan_engine::kind::Kind::Audio => {
                if layout.audio_lanes == 0 {
                    0
                } else {
                    ((local / lane_span).floor() as usize).min(layout.audio_lanes - 1)
                }
            }
            maolan_engine::kind::Kind::MIDI => {
                let midi_local = local - (layout.audio_lanes as f32 * lane_span);
                if layout.midi_lanes == 0 {
                    0
                } else {
                    ((midi_local.max(0.0) / lane_span).floor() as usize).min(layout.midi_lanes - 1)
                }
            }
        }
    }
}
