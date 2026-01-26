#[derive(Debug)]
pub struct Track {
    pub name: String,
    pub ins: usize,
    pub audio_outs: usize,
    pub midi_outs: usize,
    pub level: f32,
    pub armed: bool,
    pub muted: bool,
    pub soloed: bool,
    pub buffer: Vec<f32>,
}

impl Track {
    pub fn new(name: String, ins: usize, audio_outs: usize, midi_outs: usize) -> Self {
        Track {
            name,
            ins,
            audio_outs,
            midi_outs,
            level: 0.0,
            armed: false,
            muted: false,
            soloed: false,
            buffer: vec![],
        }
    }

    pub fn process(&mut self) {
        self.buffer.clear();
    }
}
