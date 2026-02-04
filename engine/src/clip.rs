use crate::{audio::clip::AudioClip, midi::clip::MIDIClip};

pub enum Clip {
    AudioClip(AudioClip),
    MIDIClip(MIDIClip),
}
