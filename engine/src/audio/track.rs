#[derive(Debug)]
pub struct Track {
    name: String,
    // channels: usize,
    buffer: Vec<f32>,
}

impl Track {
    pub fn new(name: String, _channels: usize) -> Self {
        Track {
            name,
            // channels,
            buffer: vec![],
        }
    }

    pub fn process(&mut self) {
        self.buffer.clear();
    }

    pub fn name(&self) -> String {
        self.name.clone()
    }

    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }
}
