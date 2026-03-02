use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct AudioClip {
    pub name: String,
    pub start: usize,
    pub length: usize,
    pub offset: usize,
    #[serde(default)]
    pub input_channel: usize,
    #[serde(skip)]
    pub max_length_samples: usize,
    pub peaks_file: Option<String>,
    #[serde(skip, default)]
    pub peaks: Vec<Vec<f32>>,
    #[serde(default = "default_fade_enabled")]
    pub fade_enabled: bool,
    #[serde(default = "default_fade_samples")]
    pub fade_in_samples: usize,
    #[serde(default = "default_fade_samples")]
    pub fade_out_samples: usize,
}

fn default_fade_enabled() -> bool {
    true
}

fn default_fade_samples() -> usize {
    // Default to 5ms at 48kHz = 240 samples
    240
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct MIDIClip {
    pub name: String,
    pub start: usize,
    pub length: usize,
    pub offset: usize,
    #[serde(default)]
    pub input_channel: usize,
    #[serde(skip)]
    pub max_length_samples: usize,
    #[serde(default = "default_fade_enabled")]
    pub fade_enabled: bool,
    #[serde(default = "default_fade_samples")]
    pub fade_in_samples: usize,
    #[serde(default = "default_fade_samples")]
    pub fade_out_samples: usize,
}
