#![cfg(target_os = "macos")]

//! Buffer-size and sample-rate negotiation for a CoreAudio device,
//! plus the `HwDriver` struct that the rest of the engine interacts with.

use super::device::DeviceInfo;
use super::error_fmt::ca_error;
use crate::audio::io::AudioIO;
use crate::hw::common;
use crate::hw::latency;
use crate::hw::options::HwOptions;

use super::latency::query_device_latency;

use coreaudio_sys::{
    kAudioDevicePropertyBufferFrameSize, kAudioDevicePropertyNominalSampleRate,
    kAudioDevicePropertyStreamConfiguration, kAudioHardwareNoError,
    kAudioObjectPropertyElementMain, kAudioObjectPropertyScopeGlobal,
    kAudioObjectPropertyScopeInput, kAudioObjectPropertyScopeOutput, AudioBufferList,
    AudioDeviceID, AudioObjectGetPropertyData, AudioObjectGetPropertyDataSize,
    AudioObjectPropertyAddress, AudioObjectSetPropertyData, OSStatus, UInt32,
};

use std::mem;
use std::os::raw::c_void;
use std::ptr;
use std::sync::Arc;

/// CoreAudio `HwDriver` — owns the negotiated device settings and
/// per-channel `AudioIO` buffers.
#[derive(Debug)]
pub struct HwDriver {
    device_id: AudioDeviceID,
    sample_rate: i32,
    cycle_samples: usize,
    nperiods: usize,
    sync_mode: bool,
    input_latency_frames: usize,
    output_latency_frames: usize,
    input_channels: Vec<Arc<AudioIO>>,
    output_channels: Vec<Arc<AudioIO>>,
    output_gain_linear: f32,
    output_balance: f32,
}

impl HwDriver {
    /// Open a CoreAudio device, negotiate sample rate and buffer size,
    /// and allocate per-channel `AudioIO` buffers.
    pub fn new(device: &DeviceInfo, rate: i32) -> Result<Self, String> {
        Self::new_with_options(device, rate, HwOptions::default())
    }

    pub fn new_with_options(
        device: &DeviceInfo,
        rate: i32,
        options: HwOptions,
    ) -> Result<Self, String> {
        let device_id = device.id;

        // Negotiate sample rate.
        let actual_rate = negotiate_sample_rate(device_id, rate as f64)?;

        // Negotiate buffer size.
        let actual_frames = negotiate_buffer_size(device_id, options.period_frames as u32)?;

        // Build per-channel AudioIO buffers.
        let in_count = channel_count(device_id, true);
        let out_count = channel_count(device_id, false);

        let input_channels = (0..in_count)
            .map(|_| Arc::new(AudioIO::new(actual_frames as usize)))
            .collect();
        let output_channels = (0..out_count)
            .map(|_| Arc::new(AudioIO::new(actual_frames as usize)))
            .collect();

        // Query real hardware latency from the CoreAudio HAL.
        let input_latency_frames = query_device_latency(device_id, kAudioObjectPropertyScopeInput);
        let output_latency_frames =
            query_device_latency(device_id, kAudioObjectPropertyScopeOutput);

        Ok(Self {
            device_id,
            sample_rate: actual_rate as i32,
            cycle_samples: actual_frames as usize,
            nperiods: options.nperiods.max(1),
            sync_mode: options.sync_mode,
            input_latency_frames,
            output_latency_frames,
            input_channels,
            output_channels,
            output_gain_linear: 1.0,
            output_balance: 0.0,
        })
    }

    pub fn device_id(&self) -> AudioDeviceID {
        self.device_id
    }

    pub fn input_channels(&self) -> usize {
        self.input_channels.len()
    }

    pub fn output_channels(&self) -> usize {
        self.output_channels.len()
    }

    pub fn sample_rate(&self) -> i32 {
        self.sample_rate
    }

    pub fn cycle_samples(&self) -> usize {
        self.cycle_samples
    }

    pub fn input_port(&self, idx: usize) -> Option<Arc<AudioIO>> {
        self.input_channels.get(idx).cloned()
    }

    pub fn output_port(&self, idx: usize) -> Option<Arc<AudioIO>> {
        self.output_channels.get(idx).cloned()
    }

    pub fn set_output_gain_balance(&mut self, gain: f32, balance: f32) {
        self.output_gain_linear = gain;
        self.output_balance = balance;
    }

    pub fn output_meter_db(&self, gain: f32, balance: f32) -> Vec<f32> {
        common::output_meter_db(&self.output_channels, gain, balance)
    }

    pub fn latency_ranges(&self) -> ((usize, usize), (usize, usize)) {
        // CoreAudio uses a single IOProc for duplex, so nperiods=1 and
        // sync_mode=true regardless of stored options.
        latency::latency_ranges(
            self.cycle_samples,
            1,
            true,
            self.input_latency_frames,
            self.output_latency_frames,
        )
    }
}

impl Drop for HwDriver {
    fn drop(&mut self) {
        // HwDriver does not own an IoProcHandle — IOProc start/stop and
        // destruction are handled by `IoProcHandle::drop` in ioproc.rs.
        //
        // If property listeners are added in the future (e.g.
        // AudioObjectAddPropertyListener for sample-rate or device-alive
        // notifications), they must be removed here with
        // AudioObjectRemovePropertyListener using the same
        // AudioObjectPropertyAddress and callback that was registered.
        //
        // Currently no listeners are registered, so nothing to clean up
        // beyond the implicit field drops (Arc<AudioIO> buffers).
    }
}

// ---------------------------------------------------------------------------
// HAL property negotiation helpers
// ---------------------------------------------------------------------------

/// Set `kAudioDevicePropertyNominalSampleRate` to `desired`, then read it
/// back.  Returns the rate the hardware actually accepted.
fn negotiate_sample_rate(device_id: AudioDeviceID, desired: f64) -> Result<f64, String> {
    let address = AudioObjectPropertyAddress {
        mSelector: kAudioDevicePropertyNominalSampleRate,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    };

    // Set requested rate.
    let mut rate: f64 = desired;
    let status: OSStatus = unsafe {
        AudioObjectSetPropertyData(
            device_id,
            &address,
            0,
            ptr::null(),
            mem::size_of::<f64>() as UInt32,
            &rate as *const f64 as *const c_void,
        )
    };
    if status != kAudioHardwareNoError as OSStatus {
        return Err(ca_error("set sample rate", status));
    }

    // Read back what the device accepted.
    let mut size: UInt32 = mem::size_of::<f64>() as UInt32;
    rate = 0.0;
    let status: OSStatus = unsafe {
        AudioObjectGetPropertyData(
            device_id,
            &address,
            0,
            ptr::null(),
            &mut size,
            &mut rate as *mut f64 as *mut c_void,
        )
    };
    if status != kAudioHardwareNoError as OSStatus {
        return Err(ca_error("get sample rate", status));
    }
    Ok(rate)
}

/// Set `kAudioDevicePropertyBufferFrameSize` to `desired`, then read it
/// back.  Returns the frame count the hardware actually accepted.
fn negotiate_buffer_size(device_id: AudioDeviceID, desired: u32) -> Result<u32, String> {
    let address = AudioObjectPropertyAddress {
        mSelector: kAudioDevicePropertyBufferFrameSize,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    };

    // Set requested buffer size.
    let mut frames: UInt32 = desired;
    let status: OSStatus = unsafe {
        AudioObjectSetPropertyData(
            device_id,
            &address,
            0,
            ptr::null(),
            mem::size_of::<UInt32>() as UInt32,
            &frames as *const UInt32 as *const c_void,
        )
    };
    if status != kAudioHardwareNoError as OSStatus {
        return Err(ca_error("set buffer frame size", status));
    }

    // Read back what the device accepted.
    let mut size: UInt32 = mem::size_of::<UInt32>() as UInt32;
    frames = 0;
    let status: OSStatus = unsafe {
        AudioObjectGetPropertyData(
            device_id,
            &address,
            0,
            ptr::null(),
            &mut size,
            &mut frames as *mut UInt32 as *mut c_void,
        )
    };
    if status != kAudioHardwareNoError as OSStatus {
        return Err(ca_error("get buffer frame size", status));
    }
    Ok(frames)
}

/// Count the number of channels on one scope of a device.
fn channel_count(device_id: AudioDeviceID, input: bool) -> usize {
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
    total as usize
}
