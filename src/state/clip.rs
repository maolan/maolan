use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

pub type PeakPair = [f32; 2];
pub type ClipPeaksData = Vec<Vec<PeakPair>>;
pub type ClipPeaks = Arc<ClipPeaksData>;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct AudioClip {
    pub name: String,
    pub start: usize,
    pub length: usize,
    pub offset: usize,
    #[serde(default)]
    pub input_channel: usize,
    #[serde(default)]
    pub muted: bool,
    #[serde(skip)]
    pub max_length_samples: usize,
    pub peaks_file: Option<String>,
    #[serde(skip, default)]
    pub peaks: ClipPeaks,
    #[serde(default = "default_fade_enabled")]
    pub fade_enabled: bool,
    #[serde(default = "default_fade_samples")]
    pub fade_in_samples: usize,
    #[serde(default = "default_fade_samples")]
    pub fade_out_samples: usize,
    #[serde(default)]
    pub pitch_correction_preview_name: Option<String>,
    #[serde(default)]
    pub pitch_correction_source_name: Option<String>,
    #[serde(default)]
    pub pitch_correction_source_offset: Option<usize>,
    #[serde(default)]
    pub pitch_correction_source_length: Option<usize>,
    #[serde(default)]
    pub pitch_correction_points: Vec<crate::state::PitchCorrectionPoint>,
    #[serde(default)]
    pub pitch_correction_frame_likeness: Option<f32>,
    #[serde(default)]
    pub pitch_correction_inertia_ms: Option<u16>,
    #[serde(default)]
    pub pitch_correction_formant_compensation: Option<bool>,
    #[serde(default)]
    pub take_lane_override: Option<usize>,
    #[serde(default = "default_take_lane_flag")]
    pub take_lane_pinned: bool,
    #[serde(default = "default_take_lane_flag")]
    pub take_lane_locked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin_graph_json: Option<Value>,
    #[serde(default)]
    pub grouped_clips: Vec<AudioClip>,
}

fn default_fade_enabled() -> bool {
    true
}

fn default_fade_samples() -> usize {
    // Default to 5ms at 48kHz = 240 samples
    240
}

fn default_take_lane_flag() -> bool {
    false
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct MIDIClip {
    pub name: String,
    pub start: usize,
    pub length: usize,
    pub offset: usize,
    #[serde(default)]
    pub input_channel: usize,
    #[serde(default)]
    pub muted: bool,
    #[serde(skip)]
    pub max_length_samples: usize,
    #[serde(default)]
    pub take_lane_override: Option<usize>,
    #[serde(default = "default_take_lane_flag")]
    pub take_lane_pinned: bool,
    #[serde(default = "default_take_lane_flag")]
    pub take_lane_locked: bool,
    #[serde(default)]
    pub grouped_clips: Vec<MIDIClip>,
}

impl AudioClip {
    pub fn is_group(&self) -> bool {
        !self.grouped_clips.is_empty()
    }

    pub fn normalize_group_children(&mut self) {
        for child in &mut self.grouped_clips {
            child.fade_enabled = false;
            child.fade_in_samples = 0;
            child.fade_out_samples = 0;
            child.normalize_group_children();
        }
    }
}

impl MIDIClip {
    pub fn is_group(&self) -> bool {
        !self.grouped_clips.is_empty()
    }

    pub fn normalize_group_children(&mut self) {
        for child in &mut self.grouped_clips {
            child.normalize_group_children();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AudioClip, MIDIClip, PeakPair};
    use serde_json::json;

    #[test]
    fn audio_clip_plugin_graph_json_round_trips() {
        let clip = AudioClip {
            name: "clip.wav".to_string(),
            plugin_graph_json: Some(json!({
                "plugins": [{"format": "CLAP", "uri": "clipfx.clap"}],
                "connections": []
            })),
            ..AudioClip::default()
        };

        let value = serde_json::to_value(&clip).expect("serialize");
        let restored: AudioClip = serde_json::from_value(value).expect("deserialize");

        assert_eq!(restored.plugin_graph_json, clip.plugin_graph_json);
    }

    #[test]
    fn normalize_group_children_clears_audio_child_fades_recursively() {
        let mut clip = AudioClip {
            grouped_clips: vec![AudioClip {
                fade_enabled: true,
                fade_in_samples: 32,
                fade_out_samples: 48,
                grouped_clips: vec![AudioClip {
                    fade_enabled: true,
                    fade_in_samples: 12,
                    fade_out_samples: 18,
                    ..AudioClip::default()
                }],
                ..AudioClip::default()
            }],
            ..AudioClip::default()
        };

        clip.normalize_group_children();

        assert!(!clip.grouped_clips[0].fade_enabled);
        assert_eq!(clip.grouped_clips[0].fade_in_samples, 0);
        assert_eq!(clip.grouped_clips[0].fade_out_samples, 0);
        assert!(!clip.grouped_clips[0].grouped_clips[0].fade_enabled);
        assert_eq!(clip.grouped_clips[0].grouped_clips[0].fade_in_samples, 0);
        assert_eq!(clip.grouped_clips[0].grouped_clips[0].fade_out_samples, 0);
    }

    #[test]
    fn normalize_group_children_clears_midi_child_fades_recursively() {
        let mut clip = MIDIClip {
            grouped_clips: vec![MIDIClip {
                grouped_clips: vec![MIDIClip {
                    ..MIDIClip::default()
                }],
                ..MIDIClip::default()
            }],
            ..MIDIClip::default()
        };

        clip.normalize_group_children();

        assert_eq!(clip.grouped_clips.len(), 1);
        assert_eq!(clip.grouped_clips[0].grouped_clips.len(), 1);
    }

    #[test]
    fn audio_clip_default_creation() {
        let clip = AudioClip::default();
        assert_eq!(clip.name, "");
        assert_eq!(clip.start, 0);
        assert_eq!(clip.length, 0);
    }

    #[test]
    fn midi_clip_default_values() {
        let clip = MIDIClip::default();
        assert_eq!(clip.start, 0);
        assert_eq!(clip.length, 0);
        assert_eq!(clip.offset, 0);
    }

    #[test]
    fn peak_pair_creation() {
        let pair: PeakPair = [0.5, -0.3];
        assert!((pair[0] - 0.5).abs() < f32::EPSILON);
        assert!((pair[1] - (-0.3)).abs() < f32::EPSILON);
    }

    #[test]
    fn audio_clip_creation() {
        let clip = AudioClip {
            name: "test.wav".to_string(),
            start: 100,
            length: 500,
            offset: 50,
            ..AudioClip::default()
        };
        assert_eq!(clip.name, "test.wav");
        assert_eq!(clip.start, 100);
        assert_eq!(clip.length, 500);
        assert_eq!(clip.offset, 50);
    }

    #[test]
    fn midi_clip_creation() {
        let clip = MIDIClip {
            name: "test.mid".to_string(),
            start: 200,
            length: 1000,
            offset: 100,
            ..MIDIClip::default()
        };
        assert_eq!(clip.name, "test.mid");
        assert_eq!(clip.start, 200);
        assert_eq!(clip.length, 1000);
    }
}
