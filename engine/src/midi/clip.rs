#[derive(Default, Clone, Debug)]
pub struct MIDIClip {
    pub name: String,
    pub start: usize,
    pub end: usize,
    pub offset: usize,
    pub input_channel: usize,
    pub muted: bool,
    pub fade_enabled: bool,
    pub fade_in_samples: usize,
    pub fade_out_samples: usize,
    pub grouped_clips: Vec<MIDIClip>,
}

impl MIDIClip {
    pub fn new(name: String, start: usize, end: usize) -> Self {
        Self {
            name,
            start,
            end,
            offset: 0,
            input_channel: 0,
            muted: false,
            fade_enabled: true,
            fade_in_samples: 240,
            fade_out_samples: 240,
            grouped_clips: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MIDIClip;

    #[test]
    fn new_midi_clip_uses_expected_defaults() {
        let clip = MIDIClip::new("clip.mid".to_string(), 8, 64);

        assert_eq!(clip.name, "clip.mid");
        assert_eq!(clip.start, 8);
        assert_eq!(clip.end, 64);
        assert_eq!(clip.offset, 0);
        assert_eq!(clip.input_channel, 0);
        assert!(!clip.muted);
        assert!(clip.fade_enabled);
        assert_eq!(clip.fade_in_samples, 240);
        assert_eq!(clip.fade_out_samples, 240);
        assert!(clip.grouped_clips.is_empty());
    }
}
