use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct AudioClip {
    pub name: String,
    pub start: usize,
    pub length: usize,
    pub offset: usize,
    #[serde(skip)]
    pub max_length_samples: usize,
    pub peaks_file: Option<String>,
    #[serde(skip, default)]
    pub peaks: Vec<Vec<f32>>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct MIDIClip {
    pub name: String,
    pub start: usize,
    pub length: usize,
    pub offset: usize,
    #[serde(skip)]
    pub max_length_samples: usize,
}
