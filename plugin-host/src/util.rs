use std::cell::UnsafeCell;

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

    pub fn setup(&self) {}

    pub fn process(&self) {}
}

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
