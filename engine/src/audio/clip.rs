#[derive(Default, Clone)]
pub struct AudioClip {
    name: String,
    start: usize,
    end: usize,
    offset: usize,
}

impl AudioClip {
    pub fn new(name: String, start: usize, end: usize) -> Self {
        Self {
            name,
            start,
            end,
            offset: 0,
        }
    }
}
