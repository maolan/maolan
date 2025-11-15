use std::{
    collections::HashMap,
    sync::Arc,
};

use crate::mutex::UnsafeMutex;

pub mod track;

#[derive(Debug)]
pub struct State {
    pub tracks: HashMap<String, Arc<UnsafeMutex<track::Track>>>,
}

impl State {
    pub fn new() -> Self {
        State {
            tracks: HashMap::new(),
        }
    }
}
