#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy)]
pub enum Kind {
    Audio,
    MIDI,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_audio_equality() {
        assert_eq!(Kind::Audio, Kind::Audio);
        assert_ne!(Kind::Audio, Kind::MIDI);
    }

    #[test]
    fn kind_midi_equality() {
        assert_eq!(Kind::MIDI, Kind::MIDI);
        assert_ne!(Kind::MIDI, Kind::Audio);
    }

    #[test]
    fn kind_clone() {
        let audio = Kind::Audio;
        let cloned = audio.clone();
        assert_eq!(audio, cloned);
    }

    #[test]
    fn kind_copy() {
        let audio = Kind::Audio;
        let copied = audio;
        assert_eq!(audio, copied);
    }

    #[test]
    fn kind_debug_format() {
        let audio = Kind::Audio;
        let midi = Kind::MIDI;
        assert!(format!("{:?}", audio).contains("Audio"));
        assert!(format!("{:?}", midi).contains("MIDI"));
    }

    #[test]
    fn kind_hash_consistency() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        set.insert(Kind::Audio);
        set.insert(Kind::MIDI);
        set.insert(Kind::Audio); // Duplicate

        assert_eq!(set.len(), 2);
    }
}
