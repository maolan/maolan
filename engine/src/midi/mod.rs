use std::{
    collections::HashMap,
    sync::Arc,
};

use crate::{mutex::UnsafeMutex, track::Track};

pub mod track;

pub struct State {
    pub tracks: HashMap<String, Arc<UnsafeMutex<Box<dyn Track>>>>,
}

impl State {
    pub fn new() -> Self {
        State {
            tracks: HashMap::new(),
        }
    }
}
