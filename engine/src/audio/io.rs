use crate::mutex::UnsafeMutex;
use std::sync::Arc;

#[derive(Clone)]
pub struct AudioIO {
    pub connections: Vec<Arc<UnsafeMutex<Box<Self>>>>,
    pub buffer: Vec<f32>,
}

impl AudioIO {
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
