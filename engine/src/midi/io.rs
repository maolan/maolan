use crate::mutex::UnsafeMutex;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MidiEvent {
    pub frame: u32,
    pub data: Vec<u8>,
}

impl MidiEvent {
    pub fn new(frame: u32, data: Vec<u8>) -> Self {
        Self { frame, data }
    }
}

#[derive(Clone, Debug)]
pub struct MIDIIO {
    pub connections: Vec<Arc<UnsafeMutex<Box<Self>>>>,
    pub buffer: Vec<MidiEvent>,
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

    pub fn disconnect(&mut self, to: &Arc<UnsafeMutex<Box<Self>>>) -> Result<(), String> {
        let original_len = self.connections.len();
        self.connections.retain(|conn| !Arc::ptr_eq(conn, to));

        if self.connections.len() < original_len {
            Ok(())
        } else {
            Err("Connection not found".to_string())
        }
    }
}
