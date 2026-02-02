use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Clip {
    pub id: usize,
    pub name: String,
    pub start: f32,
    pub length: f32,
    pub offset: usize,
}

impl Clip {
    pub fn new(id: usize, name: String, start: f32, length: f32, offset: usize) -> Self {
        Self {
            id,
            name,
            start,
            length,
            offset,
        }
    }
}
