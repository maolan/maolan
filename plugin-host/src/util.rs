//! Simple utility types extracted from maolan-engine for standalone plugin hosting.

use std::cell::UnsafeCell;

/// Single-threaded mutex (no actual locking, just a cell wrapper).
/// The plugin host processes audio on one thread, so this is safe.
pub struct SimpleMutex<T> {
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Send for SimpleMutex<T> {}
unsafe impl<T: Send> Sync for SimpleMutex<T> {}

impl<T> SimpleMutex<T> {
    pub fn new(data: T) -> Self {
        SimpleMutex {
            data: UnsafeCell::new(data),
        }
    }

    #[allow(clippy::mut_from_ref)]
    pub fn lock(&self) -> &mut T {
        unsafe { &mut *self.data.get() }
    }
}

/// Audio port buffer used by in-process plugin wrappers.
pub struct AudioPort {
    pub buffer: std::sync::Arc<SimpleMutex<Vec<f32>>>,
    pub finished: std::sync::Arc<SimpleMutex<bool>>,
}

impl AudioPort {
    pub fn new(size: usize) -> Self {
        Self {
            buffer: std::sync::Arc::new(SimpleMutex::new(vec![0.0; size])),
            finished: std::sync::Arc::new(SimpleMutex::new(false)),
        }
    }

    pub fn setup(&self) {
        // No-op for standalone AudioPort (connections are managed externally).
    }

    pub fn process(&self) {
        // No-op for standalone AudioPort (audio is copied directly from SHM).
    }
}

/// MIDI event with sample frame offset.
#[derive(Debug, Clone, PartialEq)]
pub struct MidiEvent {
    pub frame: u32,
    pub data: Vec<u8>,
}

impl MidiEvent {
    pub fn new(frame: u32, data: Vec<u8>) -> Self {
        Self { frame, data }
    }
}
