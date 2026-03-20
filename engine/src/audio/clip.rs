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
        }
    }
}
