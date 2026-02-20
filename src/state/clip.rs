use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct AudioClip {
    pub name: String,
    pub start: usize,
    pub length: usize,
    pub offset: usize,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct MIDIClip {
    pub name: String,
    pub start: usize,
    pub length: usize,
    pub offset: usize,
}
