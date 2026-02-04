use std::{collections::HashMap, sync::Arc};

use crate::{mutex::UnsafeMutex, track::Track};

pub mod clip;
pub mod track;

#[derive(Default)]
pub struct State {
    pub tracks: HashMap<String, Arc<UnsafeMutex<Box<dyn Track>>>>,
}
