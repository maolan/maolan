use crate::audio::State as AudioState;
use crate::midi::State as MIDIState;

#[derive(Debug)]
pub struct State {
    pub audio: AudioState,
    pub midi: MIDIState,
}

impl State {
    pub fn new() -> Self {
        State {
            audio: AudioState::new(),
            midi: MIDIState::new(),
        }
    }
}
