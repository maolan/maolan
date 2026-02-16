#[derive(Default, Clone, Debug)]
pub struct AudioClip {
    pub name: String,
    pub start: usize,
    pub end: usize,
    pub offset: usize,
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
