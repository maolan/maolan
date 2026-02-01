use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Clip {
    pub name: String,
    pub start: f32,
    pub length: f32,
    pub offset: usize,
}

impl Clip {
    pub fn new(name: String, start: f32, length: f32, offset: usize) -> Self {
        Self {
            name,
            start,
            length,
            offset,
        }
    }
}
