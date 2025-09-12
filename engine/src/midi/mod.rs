use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

pub mod track;

#[derive(Debug)]
pub struct State {
    pub tracks: HashMap<String, Arc<RwLock<track::Track>>>,
}

impl State {
    pub fn new() -> Self {
        State {
            tracks: HashMap::new(),
        }
    }
}
