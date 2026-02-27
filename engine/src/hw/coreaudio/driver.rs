#![cfg(target_os = "macos")]

use super::error_fmt::ca_error;
use super::ioproc::{IoProcHandle, SharedIOState};
use crate::audio::io::AudioIO;
use crate::hw::common;
use crate::hw::latency;
use crate::hw::options::HwOptions;

use super::latency::query_device_latency;

use coreaudio_sys::{
    AudioBufferList, AudioDeviceID, AudioObjectGetPropertyData, AudioObjectGetPropertyDataSize,
    AudioObjectPropertyAddress, AudioObjectSetPropertyData, OSStatus, UInt32,
    kAudioDevicePropertyBufferFrameSize, kAudioDevicePropertyNominalSampleRate,
    kAudioDevicePropertyStreamConfiguration, kAudioHardwareNoError,
    kAudioObjectPropertyElementMain, kAudioObjectPropertyScopeGlobal,
    kAudioObjectPropertyScopeInput, kAudioObjectPropertyScopeOutput,
};

use std::mem;
use std::os::raw::c_void;
use std::ptr;
use std::sync::Arc;

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
    io_state: Arc<SharedIOState>,
    io_proc_handle: Option<IoProcHandle>,
}

impl HwDriver {
    pub fn new(name: &str, rate: i32) -> Result<Self, String> {
        Self::new_with_options(name, rate, 0, HwOptions::default())
    }

    pub fn new_with_options(
        name: &str,
        rate: i32,
        _bits: i32,
        options: HwOptions,
    ) -> Result<Self, String> {
        let devices = super::device::list_devices();
        let device = devices
            .iter()
            .find(|d| d.name == name)
            .ok_or_else(|| format!("CoreAudio device not found: {name}"))?;
        let device_id = device.id;

        let actual_rate = negotiate_sample_rate(device_id, rate as f64)?;

        let actual_frames = negotiate_buffer_size(device_id, options.period_frames as u32)?;

        let in_count = channel_count(device_id, true);
        let out_count = channel_count(device_id, false);

        let input_channels = (0..in_count)
            .map(|_| Arc::new(AudioIO::new(actual_frames as usize)))
            .collect();
        let output_channels = (0..out_count)
            .map(|_| Arc::new(AudioIO::new(actual_frames as usize)))
            .collect();

        let input_latency_frames = query_device_latency(device_id, kAudioObjectPropertyScopeInput);
        let output_latency_frames =
            query_device_latency(device_id, kAudioObjectPropertyScopeOutput);

        let io_state = Arc::new(SharedIOState::new(
            in_count,
            out_count,
            actual_frames as usize,
            actual_rate as u32,
        ));
        let io_proc_handle = IoProcHandle::new(device_id, Arc::clone(&io_state))?;

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
            io_state,
            io_proc_handle: Some(io_proc_handle),
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
        let _ = self.io_proc_handle.take();
    }
}

impl crate::hw::traits::HwWorkerDriver for HwDriver {
    fn cycle_samples(&self) -> usize {
        self.cycle_samples
    }

    fn sample_rate(&self) -> i32 {
        self.sample_rate
    }

    fn run_cycle_for_worker(&mut self) -> Result<(), String> {
        self.io_state.run_cycle(
            &self.input_channels,
            &self.output_channels,
            self.output_gain_linear,
            self.output_balance,
        )
    }

    fn run_assist_step_for_worker(&mut self) -> Result<bool, String> {
        std::thread::sleep(std::time::Duration::from_millis(1));
        Ok(true)
    }
}

crate::impl_hw_device_for_driver!(HwDriver);

fn negotiate_sample_rate(device_id: AudioDeviceID, desired: f64) -> Result<f64, String> {
    let address = AudioObjectPropertyAddress {
        mSelector: kAudioDevicePropertyNominalSampleRate,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    };

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

fn negotiate_buffer_size(device_id: AudioDeviceID, desired: u32) -> Result<u32, String> {
    let address = AudioObjectPropertyAddress {
        mSelector: kAudioDevicePropertyBufferFrameSize,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    };

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
