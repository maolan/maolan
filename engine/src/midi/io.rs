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
#[allow(clippy::upper_case_acronyms)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midi_event_new_sets_fields() {
        let event = MidiEvent::new(42, vec![0x90, 60, 100]);

        assert_eq!(event.frame, 42);
        assert_eq!(event.data, vec![0x90, 60, 100]);
    }

    #[test]
    fn connect_and_disconnect_manage_connections() {
        let target = Arc::new(UnsafeMutex::new(Box::new(MIDIIO::new())));
        let mut io = MIDIIO::new();

        io.connect(target.clone());
        assert_eq!(io.connections.len(), 1);
        assert!(Arc::ptr_eq(&io.connections[0], &target));

        assert!(io.disconnect(&target).is_ok());
        assert!(io.connections.is_empty());
    }

    #[test]
    fn disconnect_returns_error_for_missing_connection() {
        let target = Arc::new(UnsafeMutex::new(Box::new(MIDIIO::new())));
        let mut io = MIDIIO::new();

        let err = io
            .disconnect(&target)
            .expect_err("missing connection should error");

        assert_eq!(err, "Connection not found");
    }

    #[test]
    fn disconnect_removes_all_duplicate_connections_for_same_target() {
        let target = Arc::new(UnsafeMutex::new(Box::new(MIDIIO::new())));
        let mut io = MIDIIO::new();

        io.connect(target.clone());
        io.connect(target.clone());

        assert!(io.disconnect(&target).is_ok());
        assert!(io.connections.is_empty());
    }
}
