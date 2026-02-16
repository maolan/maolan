use crate::{mutex::UnsafeMutex, track::Track};
use std::{collections::HashMap, sync::Arc};

#[derive(Default, Debug)]
pub struct State {
    pub tracks: HashMap<String, Arc<UnsafeMutex<Box<Track>>>>,
}
