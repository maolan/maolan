use serde::{Serialize, Deserialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Clip {
    pub name: String,
    pub start: f32,
    pub offset: usize,
    pub length: f32,
}

impl Clip {
    pub fn new(name: String, start: f32, offset: usize, length: f32) -> Self {
        Self {
            name,
            start,
            offset,
            length,
        }
    }
}
