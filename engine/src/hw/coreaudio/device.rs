#![cfg(target_os = "macos")]

//! AudioDeviceID enumeration via the CoreAudio HAL.

use coreaudio_sys::{
    kAudioHardwareNoError, kAudioHardwarePropertyDevices,
    kAudioDevicePropertyDeviceNameCFString, kAudioDevicePropertyStreamConfiguration,
    kAudioObjectPropertyElementMain, kAudioObjectPropertyScopeGlobal,
    kAudioObjectPropertyScopeInput, kAudioObjectPropertyScopeOutput,
    kAudioObjectSystemObject, AudioBufferList, AudioDeviceID, AudioObjectGetPropertyData,
    AudioObjectGetPropertyDataSize, AudioObjectPropertyAddress, CFRelease, CFStringGetCString,
    CFStringRef, OSStatus, UInt32,
};
use std::mem;
use std::os::raw::c_void;
use std::ptr;

/// Information about a single CoreAudio device.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    /// The AudioDeviceID as returned by the HAL.
    pub id: u32,
    /// Human-readable name prefixed with `coreaudio:`.
    pub name: String,
    /// Number of input channels.
    pub input_channels: u32,
    /// Number of output channels.
    pub output_channels: u32,
}

/// Enumerate all CoreAudio audio devices on the system.
///
/// Returns an empty `Vec` if enumeration fails rather than panicking,
/// since device listing is non-critical and may run at startup.
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

/// Query kAudioHardwarePropertyDevices on the system object.
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
    // Adjust count in case the returned size differs.
    let actual_count = size as usize / mem::size_of::<AudioDeviceID>();
    ids.truncate(actual_count);
    Some(ids)
}

/// Retrieve the human-readable name of a device via kAudioDevicePropertyDeviceNameCFString.
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
    let ok = unsafe {
        CFStringGetCString(
            cf_name,
            buf.as_mut_ptr(),
            buf.len() as _,
            0x0600_0100, // kCFStringEncodingUTF8
        )
    };
    unsafe { CFRelease(cf_name as *const c_void) };
    if ok == 0 {
        return None;
    }

    let c_str = unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) };
    c_str.to_str().ok().map(|s| s.to_owned())
}

/// Count the number of channels for either the input or output scope of a device
/// by querying kAudioDevicePropertyStreamConfiguration.
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
    let status: OSStatus = unsafe {
        AudioObjectGetPropertyDataSize(device_id, &address, 0, ptr::null(), &mut size)
    };
    if status != kAudioHardwareNoError as OSStatus || size == 0 {
        return 0;
    }

    // Allocate a byte buffer large enough for the AudioBufferList.
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
