use crate::{mutex::UnsafeMutex, track::Track};
use std::{collections::HashMap, sync::Arc};

#[derive(Default, Debug)]
pub struct State {
    pub tracks: HashMap<String, Arc<UnsafeMutex<Box<Track>>>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_default_creates_empty() {
        let state = State::default();
        assert!(state.tracks.is_empty());
    }

    #[test]
    fn state_debug_format() {
        let state = State::default();
        let debug_str = format!("{:?}", state);
        assert!(debug_str.contains("State"));
        assert!(debug_str.contains("tracks"));
    }

    #[test]
    fn state_new_is_default() {
        let state1 = State::default();
        let state2 = State {
            tracks: HashMap::new(),
        };
        assert_eq!(state1.tracks.len(), state2.tracks.len());
    }
}
