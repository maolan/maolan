use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct AudioClip {
    pub name: String,
    pub start: usize,
    pub length: usize,
    pub offset: usize,
}

impl AudioClip {
    pub fn new(name: String, start: usize, length: usize, offset: usize) -> Self {
        Self {
            name,
            start,
            length,
            offset,
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct MIDIClip {
    pub name: String,
    pub start: usize,
    pub length: usize,
    pub offset: usize,
}

impl MIDIClip {
    pub fn new(name: String, start: usize, length: usize, offset: usize) -> Self {
        Self {
            name,
            start,
            length,
            offset,
        }
    }
}
