use serde::{Deserialize, Serialize};
use std::cell::UnsafeCell;
use vst3::Steinberg::{IBStreamTrait, kResultOk, kResultFalse};

type TResult = i32;

/// VST3 plugin state snapshot
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Vst3PluginState {
    pub plugin_id: String,
    pub component_state: Vec<u8>,
    pub controller_state: Vec<u8>,
}

/// Memory-based stream for VST3 state I/O
/// Uses UnsafeCell for interior mutability as required by IBStreamTrait
pub struct MemoryStream {
    data: UnsafeCell<Vec<u8>>,
    position: UnsafeCell<usize>,
}

impl MemoryStream {
    pub fn new() -> Self {
        Self {
            data: UnsafeCell::new(Vec::new()),
            position: UnsafeCell::new(0),
        }
    }

    pub fn from_bytes(data: &[u8]) -> Self {
        Self {
            data: UnsafeCell::new(data.to_vec()),
            position: UnsafeCell::new(0),
        }
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.data.into_inner()
    }

    pub fn as_ibstream_mut(&mut self) -> &mut dyn IBStreamTrait {
        self as &mut dyn IBStreamTrait
    }

    // Helper methods for safe access (used in unsafe blocks)
    unsafe fn data_mut(&self) -> &mut Vec<u8> {
        unsafe { &mut *self.data.get() }
    }

    unsafe fn position_mut(&self) -> &mut usize {
        unsafe { &mut *self.position.get() }
    }

    unsafe fn data_ref(&self) -> &Vec<u8> {
        unsafe { &*self.data.get() }
    }

    unsafe fn position_ref(&self) -> &usize {
        unsafe { &*self.position.get() }
    }
}

impl IBStreamTrait for MemoryStream {
    unsafe fn read(&self, buffer: *mut std::os::raw::c_void, num_bytes: i32, num_bytes_read: *mut i32) -> TResult {
        if buffer.is_null() || num_bytes < 0 {
            return kResultFalse;
        }

        let bytes_to_read = num_bytes as usize;
        let data = unsafe { self.data_ref() };
        let position = unsafe { *self.position_ref() };
        let available = data.len().saturating_sub(position);
        let actual_read = bytes_to_read.min(available);

        if actual_read == 0 {
            if !num_bytes_read.is_null() {
                unsafe { *num_bytes_read = 0; }
            }
            return kResultFalse;
        }

        // Copy data from internal buffer to provided buffer
        let src_slice = &data[position..position + actual_read];
        let dst_slice = unsafe { std::slice::from_raw_parts_mut(buffer as *mut u8, actual_read) };
        dst_slice.copy_from_slice(src_slice);

        unsafe { *self.position_mut() += actual_read; }

        if !num_bytes_read.is_null() {
            unsafe { *num_bytes_read = actual_read as i32; }
        }

        kResultOk
    }

    unsafe fn write(&self, buffer: *mut std::os::raw::c_void, num_bytes: i32, num_bytes_written: *mut i32) -> TResult {
        if buffer.is_null() || num_bytes < 0 {
            return kResultFalse;
        }

        let bytes_to_write = num_bytes as usize;
        let src_slice = unsafe { std::slice::from_raw_parts(buffer as *mut u8, bytes_to_write) };

        let data = unsafe { self.data_mut() };
        let position = unsafe { *self.position_ref() };

        // Ensure capacity
        let required_len = position + bytes_to_write;
        if required_len > data.len() {
            data.resize(required_len, 0);
        }

        // Write data
        data[position..position + bytes_to_write].copy_from_slice(src_slice);
        unsafe { *self.position_mut() += bytes_to_write; }

        if !num_bytes_written.is_null() {
            unsafe { *num_bytes_written = bytes_to_write as i32; }
        }

        kResultOk
    }

    unsafe fn seek(&self, pos: i64, mode: i32, result: *mut i64) -> TResult {
        let current_pos = unsafe { *self.position_ref() };
        let data_len = unsafe { self.data_ref().len() };

        let new_position = match mode {
            0 => {
                // kIBSeekSet - absolute position from start
                if pos < 0 {
                    return kResultFalse;
                }
                pos as usize
            }
            1 => {
                // kIBSeekCur - relative to current position
                if pos < 0 {
                    current_pos.saturating_sub((-pos) as usize)
                } else {
                    current_pos.saturating_add(pos as usize)
                }
            }
            2 => {
                // kIBSeekEnd - relative to end
                if pos > 0 {
                    return kResultFalse;
                }
                data_len.saturating_sub((-pos) as usize)
            }
            _ => return kResultFalse,
        };

        unsafe { *self.position_mut() = new_position; }

        if !result.is_null() {
            unsafe { *result = new_position as i64; }
        }

        kResultOk
    }

    unsafe fn tell(&self, pos: *mut i64) -> TResult {
        if pos.is_null() {
            return kResultFalse;
        }

        unsafe { *pos = *self.position_ref() as i64; }
        kResultOk
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_stream_write_read() {
        let stream = MemoryStream::new();
        let test_data = b"Hello, VST3!";

        unsafe {
            let mut written = 0;
            let result = stream.write(
                test_data.as_ptr() as *mut std::os::raw::c_void,
                test_data.len() as i32,
                &mut written,
            );
            assert_eq!(result, kResultOk);
            assert_eq!(written, test_data.len() as i32);
        }

        // Seek back to start
        unsafe {
            let mut new_pos = 0;
            stream.seek(0, 0, &mut new_pos);
            assert_eq!(new_pos, 0);
        }

        // Read back
        let mut read_buffer = vec![0u8; test_data.len()];
        unsafe {
            let mut read_count = 0;
            let result = stream.read(
                read_buffer.as_mut_ptr() as *mut _,
                test_data.len() as i32,
                &mut read_count,
            );
            assert_eq!(result, kResultOk);
            assert_eq!(read_count, test_data.len() as i32);
        }

        assert_eq!(&read_buffer, test_data);
    }

    #[test]
    fn test_memory_stream_seek() {
        let stream = MemoryStream::from_bytes(b"0123456789");

        // Seek to position 5
        unsafe {
            let mut pos = 0;
            stream.seek(5, 0, &mut pos);
            assert_eq!(pos, 5);
        }

        // Tell should return 5
        unsafe {
            let mut pos = 0;
            stream.tell(&mut pos);
            assert_eq!(pos, 5);
        }

        // Seek relative forward
        unsafe {
            let mut pos = 0;
            stream.seek(2, 1, &mut pos);
            assert_eq!(pos, 7);
        }

        // Seek from end
        unsafe {
            let mut pos = 0;
            stream.seek(-3, 2, &mut pos);
            assert_eq!(pos, 7);
        }
    }

    #[test]
    fn test_plugin_state_serialization() {
        let state = Vst3PluginState {
            plugin_id: "com.example.plugin".to_string(),
            component_state: vec![1, 2, 3, 4],
            controller_state: vec![5, 6, 7, 8],
        };

        let json = serde_json::to_string(&state).unwrap();
        let deserialized: Vst3PluginState = serde_json::from_str(&json).unwrap();

        assert_eq!(state.plugin_id, deserialized.plugin_id);
        assert_eq!(state.component_state, deserialized.component_state);
        assert_eq!(state.controller_state, deserialized.controller_state);
    }
}
