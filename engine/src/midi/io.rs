use crate::mutex::UnsafeMutex;
use std::sync::Arc;

#[derive(Clone)]
pub struct MIDIIO {
    pub connections: Vec<Arc<UnsafeMutex<Box<Self>>>>,
    pub buffer: Vec<u8>,
}

impl MIDIIO {
    pub fn new() -> Self {
        Self {
            connections: vec![],
            buffer: vec![],
        }
    }

    pub fn connect(&mut self, to: Arc<UnsafeMutex<Box<Self>>>) {
        self.connections.push(to);
    }
}
