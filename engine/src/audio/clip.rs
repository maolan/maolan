use crate::message::PitchCorrectionPointData;
use serde_json::Value;

#[derive(Default, Clone, Debug)]
pub struct AudioClip {
    pub name: String,
    pub start: usize,
    pub end: usize,
    pub offset: usize,
    pub input_channel: usize,
    pub muted: bool,
    pub peaks_file: Option<String>,
    pub fade_enabled: bool,
    pub fade_in_samples: usize,
    pub fade_out_samples: usize,
    pub pitch_correction_preview_name: Option<String>,
    pub pitch_correction_source_name: Option<String>,
    pub pitch_correction_source_offset: Option<usize>,
    pub pitch_correction_source_length: Option<usize>,
    pub pitch_correction_points: Vec<PitchCorrectionPointData>,
    pub pitch_correction_frame_likeness: Option<f32>,
    pub pitch_correction_inertia_ms: Option<u16>,
    pub pitch_correction_formant_compensation: Option<bool>,
    pub plugin_graph_json: Option<Value>,
    pub grouped_clips: Vec<AudioClip>,
}

impl AudioClip {
    pub fn new(name: String, start: usize, end: usize) -> Self {
        Self {
            name,
            start,
            end,
            offset: 0,
            input_channel: 0,
            muted: false,
            peaks_file: None,
            fade_enabled: true,
            fade_in_samples: 240, // 5ms at 48kHz
            fade_out_samples: 240,
            pitch_correction_preview_name: None,
            pitch_correction_source_name: None,
            pitch_correction_source_offset: None,
            pitch_correction_source_length: None,
            pitch_correction_points: Vec::new(),
            pitch_correction_frame_likeness: None,
            pitch_correction_inertia_ms: None,
            pitch_correction_formant_compensation: None,
            plugin_graph_json: None,
            grouped_clips: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AudioClip;

    #[test]
    fn new_audio_clip_uses_expected_defaults() {
        let clip = AudioClip::new("clip.wav".to_string(), 12, 96);

        assert_eq!(clip.name, "clip.wav");
        assert_eq!(clip.start, 12);
        assert_eq!(clip.end, 96);
        assert_eq!(clip.offset, 0);
        assert_eq!(clip.input_channel, 0);
        assert!(!clip.muted);
        assert_eq!(clip.peaks_file, None);
        assert!(clip.fade_enabled);
        assert_eq!(clip.fade_in_samples, 240);
        assert_eq!(clip.fade_out_samples, 240);
        assert!(clip.pitch_correction_preview_name.is_none());
        assert!(clip.pitch_correction_source_name.is_none());
        assert!(clip.pitch_correction_points.is_empty());
        assert!(clip.plugin_graph_json.is_none());
        assert!(clip.grouped_clips.is_empty());
    }
}
