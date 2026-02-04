use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Clip {
    pub name: String,
    pub start: usize,
    pub length: usize,
    pub offset: usize,
}

impl Clip {
    pub fn new(name: String, start: usize, length: usize, offset: usize) -> Self {
        Self {
            name,
            start,
            length,
            offset,
        }
    }
}
