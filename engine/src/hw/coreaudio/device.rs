#![cfg(target_os = "macos")]

use coreaudio_sys::{
    AudioBufferList, AudioDeviceID, AudioObjectGetPropertyData, AudioObjectGetPropertyDataSize,
    AudioObjectPropertyAddress, CFRelease, CFStringGetCString, CFStringRef, OSStatus, UInt32,
    kAudioDevicePropertyDeviceNameCFString, kAudioDevicePropertyStreamConfiguration,
    kAudioHardwareNoError, kAudioHardwarePropertyDevices, kAudioObjectPropertyElementMain,
    kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyScopeInput,
    kAudioObjectPropertyScopeOutput, kAudioObjectSystemObject,
};
use std::mem;
use std::os::raw::c_void;
use std::ptr;

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub id: u32,

    pub name: String,

    pub input_channels: u32,

    pub output_channels: u32,
}

pub fn list_devices() -> Vec<DeviceInfo> {
    let device_ids = match get_device_ids() {
        Some(ids) => ids,
        None => return Vec::new(),
    };

    let mut devices = Vec::with_capacity(device_ids.len());
    for id in device_ids {
        let raw_name = get_device_name(id).unwrap_or_else(|| format!("Unknown ({})", id));
        let name = format!("coreaudio:{}", raw_name);
        let input_channels = get_channel_count(id, true);
        let output_channels = get_channel_count(id, false);
        devices.push(DeviceInfo {
            id,
            name,
            input_channels,
            output_channels,
        });
    }
    devices
}

fn get_device_ids() -> Option<Vec<AudioDeviceID>> {
    let address = AudioObjectPropertyAddress {
        mSelector: kAudioHardwarePropertyDevices,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    };

    let mut size: UInt32 = 0;
    let status: OSStatus = unsafe {
        AudioObjectGetPropertyDataSize(
            kAudioObjectSystemObject,
            &address,
            0,
            ptr::null(),
            &mut size,
        )
    };
    if status != kAudioHardwareNoError as OSStatus || size == 0 {
        return None;
    }

    let count = size as usize / mem::size_of::<AudioDeviceID>();
    let mut ids: Vec<AudioDeviceID> = vec![0; count];
    let status: OSStatus = unsafe {
        AudioObjectGetPropertyData(
            kAudioObjectSystemObject,
            &address,
            0,
            ptr::null(),
            &mut size,
            ids.as_mut_ptr() as *mut c_void,
        )
    };
    if status != kAudioHardwareNoError as OSStatus {
        return None;
    }

    let actual_count = size as usize / mem::size_of::<AudioDeviceID>();
    ids.truncate(actual_count);
    Some(ids)
}

fn get_device_name(device_id: AudioDeviceID) -> Option<String> {
    let address = AudioObjectPropertyAddress {
        mSelector: kAudioDevicePropertyDeviceNameCFString,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    };

    let mut cf_name: CFStringRef = ptr::null();
    let mut size: UInt32 = mem::size_of::<CFStringRef>() as UInt32;
    let status: OSStatus = unsafe {
        AudioObjectGetPropertyData(
            device_id,
            &address,
            0,
            ptr::null(),
            &mut size,
            &mut cf_name as *mut CFStringRef as *mut c_void,
        )
    };
    if status != kAudioHardwareNoError as OSStatus || cf_name.is_null() {
        return None;
    }

    let mut buf = [0i8; 256];
    let ok = unsafe { CFStringGetCString(cf_name, buf.as_mut_ptr(), buf.len() as _, 0x0800_0100) };
    unsafe { CFRelease(cf_name as *const c_void) };
    if ok == 0 {
        return None;
    }

    let c_str = unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) };
    c_str.to_str().ok().map(|s| s.to_owned())
}

fn get_channel_count(device_id: AudioDeviceID, input: bool) -> u32 {
    let scope = if input {
        kAudioObjectPropertyScopeInput
    } else {
        kAudioObjectPropertyScopeOutput
    };

    let address = AudioObjectPropertyAddress {
        mSelector: kAudioDevicePropertyStreamConfiguration,
        mScope: scope,
        mElement: kAudioObjectPropertyElementMain,
    };

    let mut size: UInt32 = 0;
    let status: OSStatus =
        unsafe { AudioObjectGetPropertyDataSize(device_id, &address, 0, ptr::null(), &mut size) };
    if status != kAudioHardwareNoError as OSStatus || size == 0 {
        return 0;
    }

    let mut buf: Vec<u8> = vec![0u8; size as usize];
    let status: OSStatus = unsafe {
        AudioObjectGetPropertyData(
            device_id,
            &address,
            0,
            ptr::null(),
            &mut size,
            buf.as_mut_ptr() as *mut c_void,
        )
    };
    if status != kAudioHardwareNoError as OSStatus {
        return 0;
    }

    let buffer_list = buf.as_ptr() as *const AudioBufferList;
    let n_buffers = unsafe { (*buffer_list).mNumberBuffers };
    let buffers_ptr = unsafe { (*buffer_list).mBuffers.as_ptr() };

    let mut total: u32 = 0;
    for i in 0..n_buffers as usize {
        total += unsafe { (*buffers_ptr.add(i)).mNumberChannels };
    }
    total
}
