#[derive(Default, Clone, Debug)]
pub struct AudioClip {
    pub name: String,
    pub start: usize,
    pub end: usize,
    pub offset: usize,
    pub input_channel: usize,
    pub muted: bool,
    pub fade_enabled: bool,
    pub fade_in_samples: usize,
    pub fade_out_samples: usize,
}

impl AudioClip {
    pub fn new(name: String, start: usize, end: usize) -> Self {
        Self {
            name,
            start,
            end,
            offset: 0,
            input_channel: 0,
            muted: false,
            fade_enabled: true,
            fade_in_samples: 240, // 5ms at 48kHz
            fade_out_samples: 240,
        }
    }
}
