#[derive(Default, Debug, Clone)]
pub struct MIDIClip {
    name: String,
    start: usize,
    end: usize,
    offset: usize,
}

impl MIDIClip {
    pub fn new(name: String, start: usize, end: usize) -> Self {
        Self {
            name,
            start,
            end,
            offset: 0,
        }
    }
}
