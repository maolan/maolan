#![cfg(target_os = "macos")]

use crate::audio::io::AudioIO;
use crate::hw::ports;
use coreaudio_sys::{
    AudioBufferList, AudioDeviceCreateIOProcID, AudioDeviceDestroyIOProcID, AudioDeviceID,
    AudioDeviceIOProcID, AudioDeviceStart, AudioDeviceStop, AudioObjectAddPropertyListener,
    AudioObjectPropertyAddress, AudioObjectRemovePropertyListener, AudioTimeStamp, OSStatus,
    UInt32, kAudioDeviceProcessorOverload, kAudioHardwareNoError, kAudioObjectPropertyElementMain,
    kAudioObjectPropertyScopeGlobal,
};
use std::os::raw::c_void;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

const OVERLOAD_ADDRESS: AudioObjectPropertyAddress = AudioObjectPropertyAddress {
    mSelector: kAudioDeviceProcessorOverload,
    mScope: kAudioObjectPropertyScopeGlobal,
    mElement: kAudioObjectPropertyElementMain,
};

unsafe extern "C" fn overload_listener(
    _id: coreaudio_sys::AudioObjectID,
    _count: UInt32,
    _addresses: *const AudioObjectPropertyAddress,
    client_data: *mut c_void,
) -> OSStatus {
    let state = &*(client_data as *const SharedIOState);
    if let Ok(mut data) = state.mutex.lock() {
        data.overload = true;
    }
    0
}

pub struct SharedData {
    pub cycle_seq: u64,

    pub input_frames: Vec<Vec<f32>>,

    pub output_frames: Vec<Vec<f32>>,

    pub output_ready: bool,

    pub period_frames: usize,

    pub overload: bool,

    pub discontinuity: bool,

    pub last_host_time_nanos: u64,

    pub sample_rate: u32,

    pub xrun_count: u64,

    pub last_sample_time: f64,
}

pub struct SharedIOState {
    pub condvar: Condvar,
    pub mutex: Mutex<SharedData>,
}

impl SharedIOState {
    pub fn new(
        input_channels: usize,
        output_channels: usize,
        period_frames: usize,
        sample_rate: u32,
    ) -> Self {
        let data = SharedData {
            cycle_seq: 0,
            input_frames: vec![vec![0.0f32; period_frames]; input_channels],
            output_frames: vec![vec![0.0f32; period_frames]; output_channels],
            output_ready: false,
            period_frames,
            overload: false,
            discontinuity: false,
            last_host_time_nanos: 0,
            sample_rate,
            xrun_count: 0,
            last_sample_time: -1.0,
        };
        SharedIOState {
            condvar: Condvar::new(),
            mutex: Mutex::new(data),
        }
    }

    fn xrun_gap(
        last_host_time_nanos: u64,
        current_host_time_nanos: u64,
        sample_rate: u32,
        period_frames: usize,
    ) -> u64 {
        if last_host_time_nanos == 0 || current_host_time_nanos <= last_host_time_nanos {
            return 0;
        }
        let delta_nanos = current_host_time_nanos - last_host_time_nanos;
        let elapsed_frames =
            (delta_nanos as u128 * sample_rate as u128 / 1_000_000_000_u128) as u64;
        if elapsed_frames > period_frames as u64 * 2 {
            elapsed_frames - period_frames as u64
        } else {
            0
        }
    }

    pub fn run_cycle(
        &self,
        input_ports: &[Arc<AudioIO>],
        output_ports: &[Arc<AudioIO>],
        output_gain: f32,
        output_balance: f32,
    ) -> Result<(), String> {
        let mut data = self
            .mutex
            .lock()
            .map_err(|e| format!("SharedIOState mutex poisoned: {}", e))?;
        let seq_before = data.cycle_seq;
        let timeout = Duration::from_secs(1);
        while data.cycle_seq == seq_before {
            let (guard, wait_result) = self
                .condvar
                .wait_timeout(data, timeout)
                .map_err(|e| format!("condvar wait failed: {}", e))?;
            data = guard;
            if wait_result.timed_out() && data.cycle_seq == seq_before {
                return Err("CoreAudio IOProc cycle timeout (1s)".to_string());
            }
        }

        let frames = data.period_frames;

        let current_nanos = unsafe {
            coreaudio_sys::AudioConvertHostTimeToNanos(coreaudio_sys::AudioGetCurrentHostTime())
        };
        let gap = Self::xrun_gap(
            data.last_host_time_nanos,
            current_nanos,
            data.sample_rate,
            frames,
        );
        if gap > 0 {
            data.xrun_count += 1;
            tracing::warn!(
                "CoreAudio xrun detected (#{}, gap {} frames)",
                data.xrun_count,
                gap
            );

            ports::fill_ports_from_interleaved(input_ports, frames, false, |_, _| 0.0);

            data.overload = false;
            data.discontinuity = false;

            data.last_host_time_nanos = current_nanos;
            return Ok(());
        }
        data.last_host_time_nanos = current_nanos;

        ports::fill_ports_from_interleaved(input_ports, frames, false, |ch, frame| {
            data.input_frames
                .get(ch)
                .and_then(|buf| buf.get(frame).copied())
                .unwrap_or(0.0)
        });

        for ch_buf in data.output_frames.iter_mut() {
            for sample in ch_buf[..frames].iter_mut() {
                *sample = 0.0;
            }
        }
        ports::write_interleaved_from_ports(
            output_ports,
            frames,
            output_gain,
            output_balance,
            false,
            |ch, frame, sample| {
                if let Some(buf) = data.output_frames.get_mut(ch) {
                    if let Some(dst) = buf.get_mut(frame) {
                        *dst = sample;
                    }
                }
            },
        );

        data.output_ready = true;

        Ok(())
    }
}

unsafe extern "C" fn io_proc(
    _device: AudioDeviceID,
    _now: *const AudioTimeStamp,
    input_data: *const AudioBufferList,
    input_time: *const AudioTimeStamp,
    output_data: *mut AudioBufferList,
    _output_time: *const AudioTimeStamp,
    client_data: *mut std::ffi::c_void,
) -> OSStatus {
    let state = &*(client_data as *const SharedIOState);

    let mut data = match state.mutex.lock() {
        Ok(guard) => guard,
        Err(_) => return 0,
    };

    if !input_time.is_null() {
        let sample_time = (*input_time).mSampleTime;
        if data.last_sample_time >= 0.0 && data.period_frames > 0 {
            let gap = (sample_time - data.last_sample_time).abs();
            if gap > 1.5 * data.period_frames as f64 {
                data.discontinuity = true;
            }
        }
        data.last_sample_time = sample_time;
    }

    if !input_data.is_null() {
        let abl = &*input_data;
        let n_buffers = abl.mNumberBuffers as usize;
        let buffers = std::slice::from_raw_parts(abl.mBuffers.as_ptr(), n_buffers);
        let mut ch_idx = 0usize;
        for buf in buffers {
            let n_channels = buf.mNumberChannels as usize;
            let frame_count = if n_channels > 0 && buf.mDataByteSize > 0 {
                buf.mDataByteSize as usize / (n_channels * std::mem::size_of::<f32>())
            } else {
                0
            };
            let samples = if !buf.mData.is_null() && frame_count > 0 {
                std::slice::from_raw_parts(buf.mData as *const f32, frame_count * n_channels)
            } else {
                &[]
            };

            for sub_ch in 0..n_channels {
                if ch_idx < data.input_frames.len() {
                    let dst = &mut data.input_frames[ch_idx];
                    let copy_len = frame_count.min(dst.len());
                    if n_channels == 1 {
                        dst[..copy_len].copy_from_slice(&samples[..copy_len]);
                    } else {
                        for f in 0..copy_len {
                            dst[f] = samples[f * n_channels + sub_ch];
                        }
                    }

                    if ch_idx == 0 {
                        data.period_frames = frame_count;
                    }
                }
                ch_idx += 1;
            }
        }
    }

    if !output_data.is_null() {
        let abl = &mut *output_data;
        let n_buffers = abl.mNumberBuffers as usize;
        let buffers = std::slice::from_raw_parts_mut(abl.mBuffers.as_mut_ptr(), n_buffers);
        let mut ch_idx = 0usize;
        for buf in buffers {
            let n_channels = buf.mNumberChannels as usize;
            let frame_count = if n_channels > 0 && buf.mDataByteSize > 0 {
                buf.mDataByteSize as usize / (n_channels * std::mem::size_of::<f32>())
            } else {
                0
            };
            let samples = if !buf.mData.is_null() && frame_count > 0 {
                std::slice::from_raw_parts_mut(buf.mData as *mut f32, frame_count * n_channels)
            } else {
                &mut []
            };

            for sub_ch in 0..n_channels {
                if ch_idx < data.output_frames.len() {
                    let src = &data.output_frames[ch_idx];
                    let copy_len = frame_count.min(src.len());
                    if n_channels == 1 {
                        samples[..copy_len].copy_from_slice(&src[..copy_len]);
                    } else {
                        for f in 0..copy_len {
                            samples[f * n_channels + sub_ch] = src[f];
                        }
                    }
                } else {
                    for f in 0..frame_count {
                        if n_channels == 1 {
                            samples[f] = 0.0;
                        } else {
                            samples[f * n_channels + sub_ch] = 0.0;
                        }
                    }
                }
                ch_idx += 1;
            }
        }
    }

    data.output_ready = false;

    data.cycle_seq += 1;

    drop(data);
    state.condvar.notify_one();

    0
}

pub struct IoProcHandle {
    device_id: AudioDeviceID,
    proc_id: AudioDeviceIOProcID,

    has_overload_listener: bool,

    _state: Arc<SharedIOState>,
}

impl IoProcHandle {
    pub fn new(device_id: AudioDeviceID, state: Arc<SharedIOState>) -> Result<Self, String> {
        let mut proc_id: AudioDeviceIOProcID = None;

        let client_ptr = Arc::as_ptr(&state) as *mut std::ffi::c_void;

        std::mem::forget(state.clone());

        let status: OSStatus = unsafe {
            AudioDeviceCreateIOProcID(device_id, Some(io_proc), client_ptr, &mut proc_id)
        };
        if status != kAudioHardwareNoError as OSStatus {
            unsafe {
                Arc::from_raw(client_ptr as *const SharedIOState);
            }
            return Err(format!(
                "AudioDeviceCreateIOProcID failed with status {}",
                status
            ));
        }

        let status: OSStatus = unsafe { AudioDeviceStart(device_id, Some(io_proc)) };
        if status != kAudioHardwareNoError as OSStatus {
            unsafe {
                AudioDeviceDestroyIOProcID(device_id, proc_id);
                Arc::from_raw(client_ptr as *const SharedIOState);
            }
            return Err(format!("AudioDeviceStart failed with status {}", status));
        }

        let overload_status: OSStatus = unsafe {
            AudioObjectAddPropertyListener(
                device_id,
                &OVERLOAD_ADDRESS,
                Some(overload_listener),
                client_ptr,
            )
        };
        if overload_status != kAudioHardwareNoError as OSStatus {
            tracing::warn!(
                "Failed to register overload listener (status {}); xrun detection may be limited",
                overload_status
            );
        }

        Ok(IoProcHandle {
            device_id,
            proc_id,
            has_overload_listener: overload_status == kAudioHardwareNoError as OSStatus,
            _state: state,
        })
    }
}

impl Drop for IoProcHandle {
    fn drop(&mut self) {
        unsafe {
            AudioDeviceStop(self.device_id, Some(io_proc));
            AudioDeviceDestroyIOProcID(self.device_id, self.proc_id);

            if self.has_overload_listener {
                let client_ptr = Arc::as_ptr(&self._state) as *mut c_void;
                AudioObjectRemovePropertyListener(
                    self.device_id,
                    &OVERLOAD_ADDRESS,
                    Some(overload_listener),
                    client_ptr,
                );
            }

            let client_ptr = Arc::as_ptr(&self._state) as *const SharedIOState;
            Arc::from_raw(client_ptr);
        }
    }
}
