#![cfg(target_os = "macos")]

use coreaudio_sys::{
    AudioDeviceID, AudioObjectGetPropertyData, AudioObjectGetPropertyDataSize,
    AudioObjectPropertyAddress, AudioStreamID, OSStatus, UInt32, kAudioDevicePropertyLatency,
    kAudioDevicePropertySafetyOffset, kAudioDevicePropertyStreams, kAudioHardwareNoError,
    kAudioObjectPropertyElementMain, kAudioStreamPropertyLatency,
};

use std::mem;
use std::os::raw::c_void;
use std::ptr;

pub fn query_device_latency(device_id: AudioDeviceID, scope: u32) -> usize {
    let device_latency = get_u32_property(device_id, kAudioDevicePropertyLatency, scope);
    let safety_offset = get_u32_property(device_id, kAudioDevicePropertySafetyOffset, scope);
    let stream_latency = get_stream0_latency(device_id, scope);

    (device_latency + safety_offset + stream_latency) as usize
}

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
