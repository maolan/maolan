#[derive(Debug)]
pub struct Track {
    pub name: String,
    pub level: f32,
    pub buffer: Vec<f32>,
}

impl Track {
    pub fn new(name: String) -> Self {
        Track {
            name,
            level: 0.0,
            buffer: vec![],
        }
    }

    pub fn process(&mut self) {
        self.buffer.clear();
    }
}
