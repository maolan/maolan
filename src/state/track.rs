use super::{AudioClip, MIDIClip};
use crate::message::{TrackAutomationMode, TrackAutomationTarget};
use iced::Point;
use serde::{Deserialize, Serialize};

pub use crate::consts::state_track::{
    TRACK_FOLDER_HEADER_HEIGHT, TRACK_SUBTRACK_GAP, TRACK_SUBTRACK_MIN_HEIGHT,
};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackAutomationPoint {
    pub sample: usize,
    pub value: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackAutomationLane {
    pub target: TrackAutomationTarget,
    pub visible: bool,
    pub points: Vec<TrackAutomationPoint>,
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
    #[serde(default)]
    pub midi_learn_volume: Option<maolan_engine::message::MidiLearnBinding>,
    #[serde(default)]
    pub midi_learn_balance: Option<maolan_engine::message::MidiLearnBinding>,
    #[serde(default)]
    pub midi_learn_mute: Option<maolan_engine::message::MidiLearnBinding>,
    #[serde(default)]
    pub midi_learn_solo: Option<maolan_engine::message::MidiLearnBinding>,
    #[serde(default)]
    pub midi_learn_arm: Option<maolan_engine::message::MidiLearnBinding>,
    #[serde(default)]
    pub midi_learn_input_monitor: Option<maolan_engine::message::MidiLearnBinding>,
    #[serde(default)]
    pub midi_learn_disk_monitor: Option<maolan_engine::message::MidiLearnBinding>,
    #[serde(default)]
    pub vca_master: Option<String>,
    #[serde(default)]
    pub frozen: bool,
    pub height: f32,
    #[serde(default)]
    pub primary_audio_ins: usize,
    #[serde(default)]
    pub primary_audio_outs: usize,
    pub audio: AudioData,
    pub midi: MIDIData,
    #[serde(default)]
    pub frozen_audio_backup: Vec<AudioClip>,
    #[serde(default)]
    pub frozen_midi_backup: Vec<MIDIClip>,
    #[serde(default)]
    pub frozen_render_clip: Option<String>,
    #[serde(default)]
    pub automation_lanes: Vec<TrackAutomationLane>,
    #[serde(default = "default_automation_mode")]
    pub automation_mode: TrackAutomationMode,
    #[serde(with = "PointDef")]
    pub position: Point,
}

fn default_automation_mode() -> TrackAutomationMode {
    TrackAutomationMode::Read
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
            midi_learn_volume: None,
            midi_learn_balance: None,
            midi_learn_mute: None,
            midi_learn_solo: None,
            midi_learn_arm: None,
            midi_learn_input_monitor: None,
            midi_learn_disk_monitor: None,
            vca_master: None,
            frozen: false,
            audio: AudioData::new(audio_ins, audio_outs),
            midi: MIDIData::new(midi_ins, midi_outs),
            primary_audio_ins: audio_ins,
            primary_audio_outs: audio_outs,
            frozen_audio_backup: vec![],
            frozen_midi_backup: vec![],
            frozen_render_clip: None,
            automation_lanes: vec![],
            automation_mode: TrackAutomationMode::Read,
            height: 60.0,
            position: Point::new(100.0, 100.0),
        };
        track.height = track.min_height_for_layout().max(60.0);
        track
    }

    pub fn audio_lane_count(&self) -> usize {
        // Always return 1 audio lane if we have any audio inputs
        if self.audio.ins > 0 { 1 } else { 0 }
    }

    pub fn primary_audio_ins(&self) -> usize {
        if self.primary_audio_ins == 0 && self.audio.ins > 0 {
            self.audio.ins
        } else {
            self.primary_audio_ins.min(self.audio.ins)
        }
    }

    pub fn primary_audio_outs(&self) -> usize {
        if self.primary_audio_outs == 0 && self.audio.outs > 0 {
            self.audio.outs
        } else {
            self.primary_audio_outs.min(self.audio.outs)
        }
    }

    pub fn return_count(&self) -> usize {
        self.audio.ins.saturating_sub(self.primary_audio_ins())
    }

    pub fn send_count(&self) -> usize {
        self.audio.outs.saturating_sub(self.primary_audio_outs())
    }

    pub fn midi_lane_count(&self) -> usize {
        self.midi.ins
    }

    pub fn automation_lane_count(&self) -> usize {
        self.automation_lanes
            .iter()
            .filter(|lane| lane.visible)
            .count()
    }

    pub fn total_lane_count(&self) -> usize {
        self.audio_lane_count()
            .saturating_add(self.midi_lane_count())
            .saturating_add(self.automation_lane_count())
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

    pub fn automation_lane_top(&self, lane: usize) -> f32 {
        let layout = self.lane_layout();
        let lane_span = layout.lane_height + TRACK_SUBTRACK_GAP;
        layout.header_height + (layout.audio_lanes + layout.midi_lanes + lane) as f32 * lane_span
    }
}
