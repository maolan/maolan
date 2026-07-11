use super::{AudioClip, MIDIClip};
use crate::message::{MidiEditorViewMode, TrackAutomationMode, TrackAutomationTarget};
use iced::{Color, Point};
use serde::{Deserialize, Deserializer, Serialize};

pub use crate::consts::state_track::{
    TRACK_FOLDER_BODY_HEIGHT, TRACK_FOLDER_HEADER_HEIGHT, TRACK_MIN_HEIGHT, TRACK_SUBTRACK_GAP,
    TRACK_SUBTRACK_MIN_HEIGHT,
};

#[derive(Debug, Clone)]
pub struct TrackLaneLayout {
    pub header_height: f32,
    /// Per-lane heights in visual order: audio lanes, then MIDI lanes, then
    /// visible automation lanes. Length == audio_lanes + midi_lanes + automation_lanes
    /// (or 1 for a collapsed track).
    pub lane_heights: Vec<f32>,
    pub audio_lanes: usize,
    pub midi_lanes: usize,
    pub automation_lanes: usize,
}

impl TrackLaneLayout {
    pub fn lane_height_for(&self, kind: maolan_engine::kind::Kind, lane: usize) -> f32 {
        let fallback = TRACK_SUBTRACK_MIN_HEIGHT;
        match kind {
            maolan_engine::kind::Kind::Audio => {
                if self.audio_lanes == 0 {
                    fallback
                } else {
                    self.lane_heights
                        .get(lane.min(self.audio_lanes - 1))
                        .copied()
                        .unwrap_or(fallback)
                }
            }
            maolan_engine::kind::Kind::MIDI => {
                if self.midi_lanes == 0 {
                    fallback
                } else {
                    let idx = self.audio_lanes + lane.min(self.midi_lanes - 1);
                    self.lane_heights.get(idx).copied().unwrap_or(fallback)
                }
            }
        }
    }

    pub fn automation_lane_height(&self, lane: usize) -> f32 {
        let fallback = TRACK_SUBTRACK_MIN_HEIGHT;
        if self.automation_lanes == 0 {
            fallback
        } else {
            let idx = self.audio_lanes + self.midi_lanes + lane.min(self.automation_lanes - 1);
            self.lane_heights.get(idx).copied().unwrap_or(fallback)
        }
    }

    /// A representative positive lane height, for callers that only need a scale.
    pub fn representative_height(&self) -> f32 {
        self.lane_heights
            .iter()
            .copied()
            .fold(TRACK_SUBTRACK_MIN_HEIGHT, f32::max)
    }
}

impl Default for TrackLaneLayout {
    fn default() -> Self {
        Self {
            header_height: 0.0,
            lane_heights: vec![TRACK_SUBTRACK_MIN_HEIGHT],
            audio_lanes: 0,
            midi_lanes: 0,
            automation_lanes: 0,
        }
    }
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
    #[serde(default)]
    pub editor_view_mode: MidiEditorViewMode,
}

impl MIDIData {
    pub fn new(ins: usize, outs: usize) -> Self {
        Self {
            clips: vec![],
            ins,
            outs,
            editor_view_mode: MidiEditorViewMode::PianoRoll,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackAutomationPoint {
    pub sample: usize,
    pub value: f32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Hash)]
pub struct EditorMarker {
    pub sample: usize,
    #[serde(default)]
    pub name: String,
}

impl<'de> Deserialize<'de> for EditorMarker {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum LegacyOrCurrent {
            Legacy(usize),
            Current {
                sample: usize,
                #[serde(default)]
                name: String,
            },
        }

        match LegacyOrCurrent::deserialize(deserializer)? {
            LegacyOrCurrent::Legacy(sample) => Ok(Self {
                sample,
                name: String::new(),
            }),
            LegacyOrCurrent::Current { sample, name } => Ok(Self { sample, name }),
        }
    }
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

mod color_option_def {
    use iced::Color;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(value: &Option<Color>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        struct ColorProxy {
            r: f32,
            g: f32,
            b: f32,
            a: f32,
        }
        match value {
            Some(c) => ColorProxy {
                r: c.r,
                g: c.g,
                b: c.b,
                a: c.a,
            }
            .serialize(serializer),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Color>, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct ColorProxy {
            r: f32,
            g: f32,
            b: f32,
            a: f32,
        }
        let proxy = Option::<ColorProxy>::deserialize(deserializer)?;
        Ok(proxy.map(|p| Color::from_rgba(p.r, p.g, p.b, p.a)))
    }
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
    pub phase_inverted: bool,
    pub soloed: bool,
    pub is_master: bool,
    pub input_monitor: Vec<bool>,
    pub disk_monitor: Vec<bool>,
    #[serde(default)]
    pub midi_input_monitor: Vec<bool>,
    #[serde(default)]
    pub midi_disk_monitor: Vec<bool>,
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
    pub frozen: bool,
    #[serde(default)]
    pub is_folder: bool,
    #[serde(default)]
    pub folder_open: bool,
    #[serde(default)]
    pub parent_track: Option<String>,
    pub height: f32,
    #[serde(default)]
    pub setup_open: bool,
    #[serde(default)]
    pub primary_audio_ins: usize,
    #[serde(default)]
    pub primary_audio_outs: usize,
    pub audio: AudioData,
    pub midi: MIDIData,
    #[serde(default)]
    pub midi_lane_channels: Vec<Option<u8>>,
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
    #[serde(default)]
    pub lane_heights: Option<Vec<f32>>,
    #[serde(with = "PointDef")]
    pub position: Point,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "color_option_def"
    )]
    pub color: Option<Color>,
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
            phase_inverted: false,
            soloed: false,
            is_master: false,
            input_monitor: vec![false; audio_ins],
            disk_monitor: vec![true; audio_ins],
            midi_input_monitor: vec![false; midi_ins],
            midi_disk_monitor: vec![true; midi_ins],
            midi_learn_volume: None,
            midi_learn_balance: None,
            midi_learn_mute: None,
            midi_learn_solo: None,
            midi_learn_arm: None,
            midi_learn_input_monitor: None,
            midi_learn_disk_monitor: None,
            frozen: false,
            is_folder: false,
            folder_open: true,
            parent_track: None,
            audio: AudioData::new(audio_ins, audio_outs),
            midi: MIDIData::new(midi_ins, midi_outs),
            midi_lane_channels: vec![None; midi_ins],
            primary_audio_ins: audio_ins,
            primary_audio_outs: audio_outs,
            frozen_audio_backup: vec![],
            frozen_midi_backup: vec![],
            frozen_render_clip: None,
            automation_lanes: vec![],
            automation_mode: TrackAutomationMode::Read,
            lane_heights: None,
            height: 82.0,
            setup_open: false,
            position: Point::new(100.0, 100.0),
            color: None,
        };
        track.height = track.min_height_for_layout().max(TRACK_MIN_HEIGHT);
        track
    }

    pub fn audio_lane_count(&self) -> usize {
        if self.is_master || self.is_folder {
            0
        } else if self.audio.ins > 0 {
            1
        } else {
            0
        }
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
        if self.is_master || self.is_folder {
            0
        } else {
            self.midi.ins
        }
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
    }

    pub fn collapsed(&self) -> bool {
        self.height < self.min_height_for_layout()
    }

    pub fn adjust_height_for_automation_lanes(
        &mut self,
        previous_lane_height: f32,
        lanes_delta: isize,
    ) {
        let min_height = self.min_height_for_layout().max(TRACK_MIN_HEIGHT);
        if self.collapsed() || lanes_delta == 0 {
            self.height = self.height.max(min_height);
            return;
        }
        // The lane set changed; drop any custom per-lane heights so the new lane
        // layout splits equally instead of keeping a now-mismatched height vector.
        self.lane_heights = None;
        let lane_step = previous_lane_height.max(TRACK_SUBTRACK_MIN_HEIGHT) + TRACK_SUBTRACK_GAP;
        self.height = (self.height + lanes_delta as f32 * lane_step).max(min_height);
    }

    /// Heights of every visual lane (audio, then MIDI, then automation) in order.
    /// Uses the stored custom heights when present and consistent with the current
    /// lane count; otherwise splits the available height equally (clamped to the
    /// per-lane minimum).
    pub fn resolved_lane_heights(&self, available: f32) -> Vec<f32> {
        let n = self.total_lane_count().max(1);
        if let Some(stored) = &self.lane_heights
            && stored.len() == n
        {
            return stored
                .iter()
                .map(|h| h.max(TRACK_SUBTRACK_MIN_HEIGHT))
                .collect();
        }
        let gaps = (n.saturating_sub(1)) as f32 * TRACK_SUBTRACK_GAP;
        let h = ((available - gaps) / n as f32).max(TRACK_SUBTRACK_MIN_HEIGHT);
        vec![h; n]
    }

    fn available_for_lanes(&self) -> f32 {
        if self.is_folder {
            self.folder_content_height().max(0.0)
        } else {
            self.height.max(0.0)
        }
    }

    pub fn lane_layout(&self) -> TrackLaneLayout {
        if self.collapsed() {
            TrackLaneLayout {
                header_height: 0.0,
                lane_heights: vec![self.height.max(1.0)],
                audio_lanes: self.audio_lane_count(),
                midi_lanes: self.midi_lane_count(),
                automation_lanes: self.automation_lane_count(),
            }
        } else {
            let lane_heights = self.resolved_lane_heights(self.available_for_lanes());
            TrackLaneLayout {
                header_height: 0.0,
                lane_heights,
                audio_lanes: self.audio_lane_count(),
                midi_lanes: self.midi_lane_count(),
                automation_lanes: self.automation_lane_count(),
            }
        }
    }

    pub fn lane_top(&self, kind: maolan_engine::kind::Kind, lane: usize) -> f32 {
        if self.collapsed() {
            return 0.0;
        }
        let layout = self.lane_layout();
        let mut y = layout.header_height;
        match kind {
            maolan_engine::kind::Kind::Audio => {
                for i in 0..lane.min(layout.audio_lanes) {
                    y += layout.lane_heights[i] + TRACK_SUBTRACK_GAP;
                }
                y
            }
            maolan_engine::kind::Kind::MIDI => {
                for i in 0..layout.audio_lanes {
                    y += layout.lane_heights[i] + TRACK_SUBTRACK_GAP;
                }
                let base = layout.audio_lanes;
                for i in 0..lane.min(layout.midi_lanes) {
                    y += layout.lane_heights[base + i] + TRACK_SUBTRACK_GAP;
                }
                y
            }
        }
    }

    pub fn lane_index_at_y(&self, kind: maolan_engine::kind::Kind, y: f32) -> usize {
        if self.collapsed() {
            return 0;
        }
        let layout = self.lane_layout();
        let local = (y - layout.header_height).max(0.0);
        match kind {
            maolan_engine::kind::Kind::Audio => {
                if layout.audio_lanes == 0 {
                    return 0;
                }
                let mut acc = 0.0;
                for i in 0..layout.audio_lanes {
                    let span = layout.lane_heights[i] + TRACK_SUBTRACK_GAP;
                    if local < acc + span {
                        return i;
                    }
                    acc += span;
                }
                layout.audio_lanes - 1
            }
            maolan_engine::kind::Kind::MIDI => {
                if layout.midi_lanes == 0 {
                    return 0;
                }
                let mut acc = 0.0;
                for i in 0..layout.audio_lanes {
                    acc += layout.lane_heights[i] + TRACK_SUBTRACK_GAP;
                }
                let midi_local = (local - acc).max(0.0);
                let base = layout.audio_lanes;
                let mut macc = 0.0;
                for i in 0..layout.midi_lanes {
                    let span = layout.lane_heights[base + i] + TRACK_SUBTRACK_GAP;
                    if midi_local < macc + span {
                        return i;
                    }
                    macc += span;
                }
                layout.midi_lanes - 1
            }
        }
    }

    pub fn automation_lane_top(&self, lane: usize) -> f32 {
        if self.collapsed() {
            return 0.0;
        }
        let layout = self.lane_layout();
        let mut y = layout.header_height;
        let base = layout.audio_lanes + layout.midi_lanes;
        for i in 0..base {
            y += layout.lane_heights[i] + TRACK_SUBTRACK_GAP;
        }
        for i in 0..lane.min(layout.automation_lanes) {
            y += layout.lane_heights[base + i] + TRACK_SUBTRACK_GAP;
        }
        y
    }

    /// Splitter drag of the divider between lane `divider` and `divider + 1`
    /// (indices into the visual lane order). Borrows `dy` from the lower lane and
    /// gives it to the upper, clamped so both stay at/above the per-lane minimum.
    /// The track's total height is unchanged.
    pub fn apply_lane_divider_drag(&mut self, divider: usize, dy: f32) {
        let initial = self.resolved_lane_heights(self.available_for_lanes());
        self.apply_lane_divider_drag_from(divider, &initial, dy);
    }

    /// Like [`Self::apply_lane_divider_drag`], but applies `dy` to a caller-supplied
    /// starting height vector. Used during interactive drags so every move event is
    /// computed relative to the drag-start heights (no cumulative error).
    pub fn apply_lane_divider_drag_from(&mut self, divider: usize, initial: &[f32], dy: f32) {
        let n = self.total_lane_count().max(1);
        if divider + 1 >= n || initial.len() != n {
            return;
        }
        let mut heights: Vec<f32> = initial
            .iter()
            .map(|h| h.max(TRACK_SUBTRACK_MIN_HEIGHT))
            .collect();
        // `dy > 0` means the divider moved down: the upper lane grows and the
        // lower lane shrinks by the same amount. Clamp so neither lane drops
        // below the per-lane minimum.
        let max_shrink_upper = heights[divider] - TRACK_SUBTRACK_MIN_HEIGHT;
        let max_shrink_lower = heights[divider + 1] - TRACK_SUBTRACK_MIN_HEIGHT;
        let clamped_dy = dy.clamp(-max_shrink_upper, max_shrink_lower);
        heights[divider] += clamped_dy;
        heights[divider + 1] -= clamped_dy;
        self.lane_heights = Some(heights);
    }

    /// Scale custom lane heights to a new total track height (used when the track's
    /// own bottom edge is dragged). No-op when lanes are in equal-split mode.
    pub fn scale_lane_heights_to(&mut self, new_total: f32) {
        let n = self.total_lane_count().max(1);
        let Some(stored) = &self.lane_heights else {
            return;
        };
        if stored.len() != n {
            self.lane_heights = None;
            return;
        }
        let gaps = (n.saturating_sub(1)) as f32 * TRACK_SUBTRACK_GAP;
        let old_sum: f32 = stored.iter().sum();
        let target_sum = (new_total - gaps).max(TRACK_SUBTRACK_MIN_HEIGHT * n as f32);
        if old_sum <= f32::EPSILON {
            self.lane_heights = None;
            return;
        }
        let scale = target_sum / old_sum;
        let mut scaled: Vec<f32> = stored
            .iter()
            .map(|h| (h * scale).max(TRACK_SUBTRACK_MIN_HEIGHT))
            .collect();
        // Absorb rounding/clamping error into the last lane so the sum stays put.
        let scaled_sum: f32 = scaled.iter().sum();
        if let Some(last) = scaled.last_mut() {
            *last = (*last + (target_sum - scaled_sum)).max(TRACK_SUBTRACK_MIN_HEIGHT);
        }
        self.lane_heights = Some(scaled);
    }

    pub fn reset_lane_heights(&mut self) {
        self.lane_heights = None;
    }

    pub fn folder_depth(&self, all_tracks: &[Track]) -> usize {
        let mut depth = 0;
        let mut current = self.parent_track.as_deref();
        while let Some(parent_name) = current {
            depth += 1;
            current = all_tracks
                .iter()
                .find(|t| t.name == parent_name)
                .and_then(|t| t.parent_track.as_deref());
        }
        depth
    }

    pub fn is_inside_closed_folder(&self, all_tracks: &[Track]) -> bool {
        let mut current = self.parent_track.as_deref();
        while let Some(parent_name) = current {
            if let Some(parent) = all_tracks.iter().find(|t| t.name == parent_name) {
                if !parent.folder_open {
                    return true;
                }
                current = parent.parent_track.as_deref();
            } else {
                break;
            }
        }
        false
    }

    pub fn folder_content_height(&self) -> f32 {
        TRACK_FOLDER_HEADER_HEIGHT + TRACK_FOLDER_BODY_HEIGHT
    }

    pub fn visible_height(&self, all_tracks: &[Track]) -> f32 {
        if self.is_inside_closed_folder(all_tracks) {
            return 0.0;
        }
        let mut height = if self.is_folder {
            self.folder_content_height()
        } else {
            self.height
        };
        if self.is_folder && self.folder_open {
            for child in all_tracks
                .iter()
                .filter(|t| t.parent_track.as_deref() == Some(self.name.as_str()))
            {
                height += child.visible_height(all_tracks);
            }
        }
        height
    }

    pub fn has_folder_children(&self, all_tracks: &[Track]) -> bool {
        all_tracks
            .iter()
            .any(|t| t.parent_track.as_deref() == Some(self.name.as_str()))
    }

    pub fn effective_muted(&self, all_tracks: &[Track]) -> bool {
        if self.muted {
            return true;
        }
        let mut current = self.parent_track.as_deref();
        while let Some(parent_name) = current {
            if let Some(parent) = all_tracks.iter().find(|t| t.name == parent_name) {
                if parent.muted {
                    return true;
                }
                current = parent.parent_track.as_deref();
            } else {
                break;
            }
        }
        false
    }

    pub fn effective_soloed(&self, all_tracks: &[Track]) -> bool {
        if self.soloed {
            return true;
        }
        let mut current = self.parent_track.as_deref();
        while let Some(parent_name) = current {
            if let Some(parent) = all_tracks.iter().find(|t| t.name == parent_name) {
                if parent.soloed {
                    return true;
                }
                current = parent.parent_track.as_deref();
            } else {
                break;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_data_new_creates_empty() {
        let data = AudioData::new(2, 2);
        assert!(data.clips.is_empty());
        assert_eq!(data.ins, 2);
        assert_eq!(data.outs, 2);
    }

    #[test]
    fn midi_data_new_creates_empty() {
        let data = MIDIData::new(1, 1);
        assert!(data.clips.is_empty());
        assert_eq!(data.ins, 1);
        assert_eq!(data.outs, 1);
    }

    #[test]
    fn track_automation_point_creation() {
        let point = TrackAutomationPoint {
            sample: 100,
            value: 0.5,
        };
        assert_eq!(point.sample, 100);
        assert!((point.value - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn track_automation_lane_default() {
        use crate::message::TrackAutomationTarget;
        let lane = TrackAutomationLane {
            target: TrackAutomationTarget::Volume,
            visible: true,
            points: vec![],
        };
        assert!(lane.visible);
        assert!(lane.points.is_empty());
    }

    #[test]
    fn track_lane_layout_creation() {
        let layout = TrackLaneLayout {
            header_height: 24.0,
            lane_heights: vec![80.0, 80.0, 80.0],
            audio_lanes: 2,
            midi_lanes: 1,
            automation_lanes: 0,
        };
        assert_eq!(layout.header_height, 24.0);
        assert_eq!(layout.audio_lanes, 2);
        assert_eq!(layout.lane_heights.len(), 3);
    }

    #[test]
    fn lane_divider_drag_borrows_from_neighbor_and_clamps_to_min() {
        use crate::message::TrackAutomationTarget;
        let mut track = Track::new("T".to_string(), 0.0, 1, 1, 0, 0);
        // One audio lane + one visible automation lane => two visual lanes.
        track.automation_lanes.push(TrackAutomationLane {
            target: TrackAutomationTarget::Volume,
            visible: true,
            points: vec![],
        });
        track.height = 200.0;
        assert_eq!(track.total_lane_count(), 2);

        let before = track.resolved_lane_heights(track.height);
        let sum_before: f32 = before.iter().sum();

        // Drag the divider down: upper lane (audio) grows, lower (automation) shrinks.
        track.apply_lane_divider_drag(0, 30.0);
        let after = track.resolved_lane_heights(track.height);
        let sum_after: f32 = after.iter().sum();

        assert!((sum_after - sum_before).abs() < 0.01);
        assert!(after[0] >= TRACK_SUBTRACK_MIN_HEIGHT - f32::EPSILON);
        assert!(after[1] >= TRACK_SUBTRACK_MIN_HEIGHT - f32::EPSILON);
        assert!((after[0] - (before[0] + 30.0)).abs() < 0.01);
        assert!((after[1] - (before[1] - 30.0)).abs() < 0.01);
        // Track height is unchanged by a lane-divider drag.
        assert!((track.height - 200.0).abs() < f32::EPSILON);
    }

    #[test]
    fn lane_heights_scale_with_track_resize_and_reset_returns_to_equal() {
        let mut track = Track::new("T".to_string(), 0.0, 1, 1, 1, 0);
        track.height = 200.0;
        // Force custom heights via a divider drag, then scale.
        track.apply_lane_divider_drag(0, 20.0);
        assert!(track.lane_heights.is_some());

        let custom = track.resolved_lane_heights(track.height);
        track.height = 300.0;
        track.scale_lane_heights_to(track.height);
        let scaled = track.resolved_lane_heights(track.height);
        // Each lane grew (track got taller) and none dropped below the minimum.
        for (c, s) in custom.iter().zip(scaled.iter()) {
            assert!(*s >= *c - 0.01);
            assert!(*s >= TRACK_SUBTRACK_MIN_HEIGHT - f32::EPSILON);
        }

        track.reset_lane_heights();
        assert!(track.lane_heights.is_none());
    }
}
