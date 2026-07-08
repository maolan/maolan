use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

impl From<LaunchQuantization> for maolan_engine::message::LaunchQuantization {
    fn from(value: LaunchQuantization) -> Self {
        match value {
            LaunchQuantization::None => Self::None,
            LaunchQuantization::Beat => Self::Beat,
            LaunchQuantization::Bar => Self::Bar,
            LaunchQuantization::TwoBars => Self::TwoBars,
            LaunchQuantization::FourBars => Self::FourBars,
            LaunchQuantization::EightBars => Self::EightBars,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LaunchMode {
    Trigger,
    Gate,
    #[default]
    Toggle,
    Repeat,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LaunchQuantization {
    None,
    Beat,
    #[default]
    Bar,
    TwoBars,
    FourBars,
    EightBars,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotClipRef {
    pub clip_id: String,
    #[serde(default)]
    pub launch_mode: LaunchMode,
    #[serde(default)]
    pub launch_quantization: LaunchQuantization,
    #[serde(default = "default_loop_enabled")]
    pub loop_enabled: bool,
    #[serde(default)]
    pub loop_start_samples: usize,
    #[serde(default)]
    pub loop_end_samples: usize,
}

fn default_loop_enabled() -> bool {
    true
}

fn default_play_stop_icon() -> Option<bool> {
    Some(false)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipSlot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip: Option<SlotClipRef>,
    #[serde(default = "default_play_stop_icon")]
    pub play_stop_icon: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip_name: Option<String>,
}

impl ClipSlot {
    /// Returns `true` when this slot references a clip and is marked with the
    /// play icon, meaning it should take part in scene launches.
    pub fn is_play_enabled(&self) -> bool {
        self.play_stop_icon == Some(true) && self.clip.is_some()
    }
}

impl Default for ClipSlot {
    fn default() -> Self {
        Self {
            clip: None,
            play_stop_icon: default_play_stop_icon(),
            clip_name: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<[f32; 4]>,
    #[serde(default)]
    pub launch_quantization: LaunchQuantization,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tempo: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMatrix {
    #[serde(default = "default_scenes")]
    pub scenes: Vec<Scene>,
    #[serde(default)]
    pub slots: HashMap<String, Vec<ClipSlot>>,
}

impl Default for SessionMatrix {
    fn default() -> Self {
        Self {
            scenes: default_scenes(),
            slots: HashMap::new(),
        }
    }
}

fn default_scenes() -> Vec<Scene> {
    vec![Scene {
        name: "Scene 1".to_string(),
        color: None,
        launch_quantization: LaunchQuantization::Bar,
        tempo: None,
    }]
}

impl SessionMatrix {
    pub fn ensure_track_slots(&mut self, track_name: &str) -> &mut Vec<ClipSlot> {
        let scene_count = self.scenes.len().max(1);
        self.slots.entry(track_name.to_string()).or_insert_with(|| {
            (0..scene_count)
                .map(|_| ClipSlot {
                    clip: None,
                    play_stop_icon: default_play_stop_icon(),
                    clip_name: None,
                })
                .collect()
        })
    }

    pub fn slot_mut(&mut self, track_name: &str, scene_index: usize) -> Option<&mut ClipSlot> {
        self.slots.get_mut(track_name)?.get_mut(scene_index)
    }

    pub fn slot(&self, track_name: &str, scene_index: usize) -> Option<&ClipSlot> {
        self.slots.get(track_name)?.get(scene_index)
    }

    pub fn scene_count(&self) -> usize {
        self.scenes.len()
    }

    pub fn add_scene(&mut self) {
        let index = self.scenes.len() + 1;
        self.scenes.push(Scene {
            name: format!("Scene {}", index),
            color: None,
            launch_quantization: LaunchQuantization::Bar,
            tempo: None,
        });
        for slots in self.slots.values_mut() {
            slots.push(ClipSlot {
                clip: None,
                play_stop_icon: default_play_stop_icon(),
                clip_name: None,
            });
        }
    }

    /// Backfill missing play/stop icons so legacy slots default to stopped.
    pub fn backfill_play_stop_icons(&mut self) {
        for slots in self.slots.values_mut() {
            for slot in slots {
                if slot.play_stop_icon.is_none() {
                    slot.play_stop_icon = Some(false);
                }
            }
        }
    }

    /// Move the clip reference from one slot to another. The destination slot
    /// is overwritten if it exists. Returns the moved clip reference when a
    /// move actually happened (i.e. `from != to` and the source slot had a
    /// clip).
    pub fn move_slot(
        &mut self,
        from_track: &str,
        from_scene: usize,
        to_track: &str,
        to_scene: usize,
    ) -> Option<SlotClipRef> {
        if from_track == to_track && from_scene == to_scene {
            return None;
        }
        let (clip_ref, clip_name) = self
            .slot_mut(from_track, from_scene)
            .map(|slot| (slot.clip.take(), slot.clip_name.take()))?;
        let clip_ref = clip_ref?;
        self.ensure_track_slots(to_track);
        if let Some(slot) = self.slot_mut(to_track, to_scene) {
            slot.clip = Some(clip_ref.clone());
            slot.clip_name = clip_name;
        }
        Some(clip_ref)
    }

    /// Copy the clip reference from one slot to another. Returns `true` if the
    /// source slot had a clip and the destination slot exists (after ensuring
    /// the destination track has enough slots).
    pub fn copy_slot(
        &mut self,
        from_track: &str,
        from_scene: usize,
        to_track: &str,
        to_scene: usize,
    ) -> bool {
        let (clip_ref, clip_name) = match self.slot(from_track, from_scene) {
            Some(slot) => (slot.clip.clone(), slot.clip_name.clone()),
            None => return false,
        };
        let Some(clip_ref) = clip_ref else {
            return false;
        };
        self.ensure_track_slots(to_track);
        if let Some(slot) = self.slot_mut(to_track, to_scene) {
            slot.clip = Some(clip_ref);
            slot.clip_name = clip_name;
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SlotPlayState {
    #[default]
    Stopped,
    Queued,
    Playing,
    Stopping,
}

#[derive(Debug, Clone, Default)]
pub struct SlotRuntime {
    pub state: SlotPlayState,
    pub next_state: Option<SlotPlayState>,
    pub play_position_samples: usize,
    pub elapsed_samples: usize,
    pub launch_at_sample: Option<usize>,
    pub stop_at_sample: Option<usize>,
}

pub type SlotRuntimes = HashMap<(String, usize), SlotRuntime>;
pub type SelectedSlots = HashSet<(String, usize)>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_matrix_default_has_one_scene() {
        let matrix = SessionMatrix::default();
        assert_eq!(matrix.scenes.len(), 1);
        assert!(matrix.slots.is_empty());
    }

    #[test]
    fn ensure_track_slots_creates_one_slot_per_scene() {
        let mut matrix = SessionMatrix::default();
        matrix.ensure_track_slots("track 1");
        assert_eq!(
            matrix.slots.get("track 1").unwrap().len(),
            matrix.scenes.len()
        );
    }

    #[test]
    fn slot_round_trips_clip_reference() {
        let mut matrix = SessionMatrix::default();
        matrix.ensure_track_slots("track 1");
        matrix.slot_mut("track 1", 0).unwrap().clip = Some(SlotClipRef {
            clip_id: "clip-1".to_string(),
            launch_mode: LaunchMode::Toggle,
            launch_quantization: LaunchQuantization::Bar,
            loop_enabled: true,
            loop_start_samples: 0,
            loop_end_samples: 0,
        });
        assert_eq!(
            matrix
                .slot("track 1", 0)
                .unwrap()
                .clip
                .as_ref()
                .unwrap()
                .clip_id,
            "clip-1"
        );
    }

    #[test]
    fn session_matrix_serde_round_trip() {
        let mut matrix = SessionMatrix::default();
        matrix.ensure_track_slots("track 1");
        matrix.slot_mut("track 1", 0).unwrap().clip = Some(SlotClipRef {
            clip_id: "clip-1".to_string(),
            launch_mode: LaunchMode::Repeat,
            launch_quantization: LaunchQuantization::TwoBars,
            loop_enabled: false,
            loop_start_samples: 48000,
            loop_end_samples: 96000,
        });
        let json = serde_json::to_string(&matrix).unwrap();
        let restored: SessionMatrix = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.scenes.len(), matrix.scenes.len());
        let clip_ref = restored.slot("track 1", 0).unwrap().clip.as_ref().unwrap();
        assert_eq!(clip_ref.clip_id, "clip-1");
        assert_eq!(clip_ref.launch_mode, LaunchMode::Repeat);
        assert_eq!(clip_ref.launch_quantization, LaunchQuantization::TwoBars);
        assert!(!clip_ref.loop_enabled);
        assert_eq!(clip_ref.loop_start_samples, 48000);
        assert_eq!(clip_ref.loop_end_samples, 96000);
    }

    #[test]
    fn launch_quantization_default_is_bar() {
        assert_eq!(LaunchQuantization::default(), LaunchQuantization::Bar);
    }

    #[test]
    fn launch_mode_default_is_toggle() {
        assert_eq!(LaunchMode::default(), LaunchMode::Toggle);
    }

    #[test]
    fn clip_slot_is_play_enabled_requires_play_icon_and_clip() {
        let mut slot = ClipSlot::default();
        assert!(!slot.is_play_enabled());

        slot.play_stop_icon = Some(true);
        assert!(!slot.is_play_enabled(), "a slot still needs a clip");

        slot.clip = Some(SlotClipRef {
            clip_id: "clip-1".to_string(),
            launch_mode: LaunchMode::Toggle,
            launch_quantization: LaunchQuantization::Bar,
            loop_enabled: true,
            loop_start_samples: 0,
            loop_end_samples: 0,
        });
        assert!(slot.is_play_enabled());

        slot.play_stop_icon = Some(false);
        assert!(!slot.is_play_enabled());

        slot.play_stop_icon = None;
        assert!(!slot.is_play_enabled());
    }

    #[test]
    fn backfill_play_stop_icons_defaults_missing_to_false() {
        let mut matrix = SessionMatrix::default();
        matrix.slots.insert(
            "track 1".to_string(),
            vec![
                ClipSlot {
                    clip: None,
                    play_stop_icon: None,
                    clip_name: None,
                },
                ClipSlot {
                    clip: None,
                    play_stop_icon: Some(false),
                    clip_name: None,
                },
            ],
        );
        matrix.backfill_play_stop_icons();
        let slots = matrix.slots.get("track 1").unwrap();
        assert_eq!(slots[0].play_stop_icon, Some(false));
        assert_eq!(slots[1].play_stop_icon, Some(false));
    }

    #[test]
    fn move_slot_transfers_clip_reference() {
        let mut matrix = SessionMatrix::default();
        matrix.ensure_track_slots("track 1");
        matrix.slot_mut("track 1", 0).unwrap().clip = Some(SlotClipRef {
            clip_id: "clip-1".to_string(),
            launch_mode: LaunchMode::Toggle,
            launch_quantization: LaunchQuantization::Bar,
            loop_enabled: true,
            loop_start_samples: 0,
            loop_end_samples: 0,
        });
        matrix.add_scene();
        matrix.add_scene();
        let moved = matrix.move_slot("track 1", 0, "track 1", 2);
        assert!(moved.is_some());
        assert!(matrix.slot("track 1", 0).unwrap().clip.is_none());
        assert_eq!(
            matrix
                .slot("track 1", 2)
                .unwrap()
                .clip
                .as_ref()
                .unwrap()
                .clip_id,
            "clip-1"
        );
    }

    #[test]
    fn move_slot_to_same_location_is_no_op() {
        let mut matrix = SessionMatrix::default();
        matrix.ensure_track_slots("track 1");
        matrix.slot_mut("track 1", 0).unwrap().clip = Some(SlotClipRef {
            clip_id: "clip-1".to_string(),
            launch_mode: LaunchMode::Toggle,
            launch_quantization: LaunchQuantization::Bar,
            loop_enabled: true,
            loop_start_samples: 0,
            loop_end_samples: 0,
        });
        assert!(matrix.move_slot("track 1", 0, "track 1", 0).is_none());
        assert_eq!(
            matrix
                .slot("track 1", 0)
                .unwrap()
                .clip
                .as_ref()
                .unwrap()
                .clip_id,
            "clip-1"
        );
    }

    #[test]
    fn move_slot_from_empty_source_returns_none() {
        let mut matrix = SessionMatrix::default();
        matrix.ensure_track_slots("track 1");
        assert!(matrix.move_slot("track 1", 0, "track 1", 2).is_none());
    }

    #[test]
    fn copy_slot_creates_independent_reference() {
        let mut matrix = SessionMatrix::default();
        matrix.ensure_track_slots("track 1");
        matrix.slot_mut("track 1", 0).unwrap().clip = Some(SlotClipRef {
            clip_id: "clip-1".to_string(),
            launch_mode: LaunchMode::Toggle,
            launch_quantization: LaunchQuantization::Bar,
            loop_enabled: true,
            loop_start_samples: 0,
            loop_end_samples: 0,
        });
        matrix.add_scene();
        matrix.add_scene();
        matrix.add_scene();
        assert!(matrix.copy_slot("track 1", 0, "track 1", 3));
        assert_eq!(
            matrix
                .slot("track 1", 0)
                .unwrap()
                .clip
                .as_ref()
                .unwrap()
                .clip_id,
            "clip-1"
        );
        assert_eq!(
            matrix
                .slot("track 1", 3)
                .unwrap()
                .clip
                .as_ref()
                .unwrap()
                .clip_id,
            "clip-1"
        );
    }

    #[test]
    fn copy_slot_from_empty_source_returns_false() {
        let mut matrix = SessionMatrix::default();
        matrix.ensure_track_slots("track 1");
        assert!(!matrix.copy_slot("track 1", 0, "track 1", 3));
    }
}
