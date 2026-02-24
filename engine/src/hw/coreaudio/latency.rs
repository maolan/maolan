#![cfg(target_os = "macos")]

//! Query CoreAudio hardware latency properties.
//!
//! The total device latency for a given scope (input or output) is the sum of:
//!   - `kAudioDevicePropertyLatency` — fixed device-level latency in frames
//!   - `kAudioStreamPropertyLatency` — additional stream-level latency for stream 0
//!   - `kAudioDevicePropertySafetyOffset` — safety offset the HAL adds
//!
//! All three are queried per-scope (input vs output).

use coreaudio_sys::{
    kAudioDevicePropertyLatency, kAudioDevicePropertySafetyOffset, kAudioDevicePropertyStreams,
    kAudioHardwareNoError, kAudioObjectPropertyElementMain, kAudioStreamPropertyLatency,
    AudioDeviceID, AudioObjectGetPropertyData, AudioObjectGetPropertyDataSize,
    AudioObjectPropertyAddress, AudioStreamID, OSStatus, UInt32,
};

use std::mem;
use std::os::raw::c_void;
use std::ptr;

/// Query the total hardware latency (in frames) for a given scope on a device.
///
/// Returns the sum of the device latency, stream 0 latency, and safety offset.
/// Falls back to 0 for any property that cannot be read.
pub fn query_device_latency(device_id: AudioDeviceID, scope: u32) -> usize {
    let device_latency = get_u32_property(device_id, kAudioDevicePropertyLatency, scope);
    let safety_offset = get_u32_property(device_id, kAudioDevicePropertySafetyOffset, scope);
    let stream_latency = get_stream0_latency(device_id, scope);

    (device_latency + safety_offset + stream_latency) as usize
}

/// Read a UInt32 property from an audio device.
fn get_u32_property(device_id: AudioDeviceID, selector: u32, scope: u32) -> u32 {
    let address = AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: scope,
        mElement: kAudioObjectPropertyElementMain,
    };

    let mut value: UInt32 = 0;
    let mut size: UInt32 = mem::size_of::<UInt32>() as UInt32;
    let status: OSStatus = unsafe {
        AudioObjectGetPropertyData(
            device_id,
            &address,
            0,
            ptr::null(),
            &mut size,
            &mut value as *mut UInt32 as *mut c_void,
        )
    };
    if status != kAudioHardwareNoError as OSStatus {
        return 0;
    }
    value
}

/// Query `kAudioStreamPropertyLatency` on stream 0 of the given scope.
///
/// First enumerates streams via `kAudioDevicePropertyStreams`, then reads
/// the latency property on the first stream found.
fn get_stream0_latency(device_id: AudioDeviceID, scope: u32) -> u32 {
    let address = AudioObjectPropertyAddress {
        mSelector: kAudioDevicePropertyStreams,
        mScope: scope,
        mElement: kAudioObjectPropertyElementMain,
    };

    let mut size: UInt32 = 0;
    let status: OSStatus =
        unsafe { AudioObjectGetPropertyDataSize(device_id, &address, 0, ptr::null(), &mut size) };
    if status != kAudioHardwareNoError as OSStatus || size == 0 {
        return 0;
    }

    let count = size as usize / mem::size_of::<AudioStreamID>();
    if count == 0 {
        return 0;
    }

    let mut stream_ids: Vec<AudioStreamID> = vec![0; count];
    let status: OSStatus = unsafe {
        AudioObjectGetPropertyData(
            device_id,
            &address,
            0,
            ptr::null(),
            &mut size,
            stream_ids.as_mut_ptr() as *mut c_void,
        )
    };
    if status != kAudioHardwareNoError as OSStatus {
        return 0;
    }

    let stream_id = stream_ids[0];

    // kAudioStreamPropertyLatency is queried on the stream object itself,
    // with global scope.
    let stream_addr = AudioObjectPropertyAddress {
        mSelector: kAudioStreamPropertyLatency,
        mScope: scope,
        mElement: kAudioObjectPropertyElementMain,
    };

    let mut latency: UInt32 = 0;
    let mut lat_size: UInt32 = mem::size_of::<UInt32>() as UInt32;
    let status: OSStatus = unsafe {
        AudioObjectGetPropertyData(
            stream_id,
            &stream_addr,
            0,
            ptr::null(),
            &mut lat_size,
            &mut latency as *mut UInt32 as *mut c_void,
        )
    };
    if status != kAudioHardwareNoError as OSStatus {
        return 0;
    }
    latency
}
