#![cfg(target_os = "macos")]

//! IOProc callback and device lifecycle for the CoreAudio backend.
//!
//! `SharedIOState` bridges the real-time IOProc callback (called by CoreAudio
//! on its own thread) with the engine's worker thread via a condvar.
//! `IoProcHandle` owns the registration and ensures cleanup on drop.

use crate::audio::io::AudioIO;
use crate::hw::ports;
use coreaudio_sys::{
    kAudioHardwareNoError, AudioBufferList, AudioDeviceCreateIOProcID,
    AudioDeviceDestroyIOProcID, AudioDeviceID, AudioDeviceIOProcID, AudioDeviceStart,
    AudioDeviceStop, AudioTimeStamp, OSStatus,
};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// Data shared between the IOProc callback and the engine worker thread.
pub struct SharedData {
    /// Monotonically increasing sequence number, incremented each IOProc call.
    pub cycle_seq: u64,
    /// Non-interleaved input frames: `[channel][frame]`.
    pub input_frames: Vec<Vec<f32>>,
    /// Non-interleaved output frames: `[channel][frame]`.
    pub output_frames: Vec<Vec<f32>>,
    /// Number of frames per period (set by the IOProc from the buffer size).
    pub period_frames: usize,
    /// Set to true if the IOProc detects an overload condition.
    pub overload: bool,
    /// Set to true if a discontinuity is detected (e.g. timestamp gap).
    pub discontinuity: bool,
    /// Host time (nanoseconds) of the last completed IOProc cycle, used for
    /// xrun gap detection.
    pub last_host_time_nanos: u64,
    /// Sample rate in Hz, needed to convert time gaps to frame counts.
    pub sample_rate: u32,
    /// Running count of detected xruns.
    pub xrun_count: u64,
}

/// Condvar-based bridge between the real-time IOProc and the worker thread.
pub struct SharedIOState {
    pub condvar: Condvar,
    pub mutex: Mutex<SharedData>,
}

impl SharedIOState {
    /// Create a new shared state with the given channel counts and initial period size.
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
            period_frames,
            overload: false,
            discontinuity: false,
            last_host_time_nanos: 0,
            sample_rate,
            xrun_count: 0,
        };
        SharedIOState {
            condvar: Condvar::new(),
            mutex: Mutex::new(data),
        }
    }

    /// Compute the frame gap from the host time delta between two consecutive
    /// IOProc cycles. Returns the number of frames beyond one period that were
    /// missed, or 0 if no xrun occurred.
    ///
    /// Mirrors `DuplexChannelApi::xrun_gap()` from `hw/oss/channel.rs`.
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

    /// Wait for the next IOProc cycle, then copy input data to AudioIO input
    /// ports and read AudioIO output ports back into the shared output buffer.
    ///
    /// Called from the engine worker thread. Mirrors OSS
    /// `DuplexChannelApi::run_cycle()` in terms of AudioIO port filling.
    pub fn run_cycle(
        &self,
        input_ports: &[Arc<AudioIO>],
        output_ports: &[Arc<AudioIO>],
        output_gain: f32,
        output_balance: f32,
    ) -> Result<(), String> {
        // 1. Lock and wait for cycle_seq to increment.
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

        // 2. Xrun detection: check elapsed time since last cycle.
        let current_nanos =
            unsafe { coreaudio_sys::AudioConvertHostTimeToNanos(coreaudio_sys::AudioGetCurrentHostTime()) };
        let gap = Self::xrun_gap(
            data.last_host_time_nanos,
            current_nanos,
            data.sample_rate,
            frames,
        );
        if gap > 0 {
            data.xrun_count += 1;
            log::warn!(
                "CoreAudio xrun detected (#{}, gap {} frames)",
                data.xrun_count,
                gap
            );

            // Fill input ports with silence.
            ports::fill_ports_from_interleaved(input_ports, frames, false, |_, _| 0.0);

            // Clear overload and discontinuity flags.
            data.overload = false;
            data.discontinuity = false;

            // Update timestamp and return early — this cycle is a recovery gap.
            data.last_host_time_nanos = current_nanos;
            return Ok(());
        }
        data.last_host_time_nanos = current_nanos;

        // 3. Copy shared input_frames[ch] -> AudioIO input ports.
        ports::fill_ports_from_interleaved(input_ports, frames, false, |ch, frame| {
            data.input_frames
                .get(ch)
                .and_then(|buf| buf.get(frame).copied())
                .unwrap_or(0.0)
        });

        // 4. Read AudioIO output ports -> shared output_frames[ch].
        //    Zero the output buffer first so unconnected channels are silent.
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

        // 5. Lock is released when `data` is dropped.
        Ok(())
    }
}

/// The IOProc callback invoked by CoreAudio on its real-time thread.
///
/// # Safety
///
/// This function is called by the CoreAudio HAL. `client_data` must be a valid
/// pointer to an `Arc<SharedIOState>` that was leaked via `Arc::into_raw`.
unsafe extern "C" fn io_proc(
    _device: AudioDeviceID,
    _now: *const AudioTimeStamp,
    input_data: *const AudioBufferList,
    _input_time: *const AudioTimeStamp,
    output_data: *mut AudioBufferList,
    _output_time: *const AudioTimeStamp,
    client_data: *mut std::ffi::c_void,
) -> OSStatus {
    let state = &*(client_data as *const SharedIOState);

    let mut data = match state.mutex.lock() {
        Ok(guard) => guard,
        Err(_) => return 0, // poisoned — nothing we can do on the RT thread
    };

    // Copy input AudioBufferList channels into SharedData::input_frames
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
                        // De-interleave
                        for f in 0..copy_len {
                            dst[f] = samples[f * n_channels + sub_ch];
                        }
                    }
                    // Update period_frames if the buffer size changed
                    if ch_idx == 0 {
                        data.period_frames = frame_count;
                    }
                }
                ch_idx += 1;
            }
        }
    }

    // Copy SharedData::output_frames into output AudioBufferList channels
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
                std::slice::from_raw_parts_mut(
                    buf.mData as *mut f32,
                    frame_count * n_channels,
                )
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
                        // Interleave
                        for f in 0..copy_len {
                            samples[f * n_channels + sub_ch] = src[f];
                        }
                    }
                } else {
                    // No source channel — zero this sub-channel
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

    data.cycle_seq += 1;
    // Release lock before signalling so waiters can acquire immediately.
    drop(data);
    state.condvar.notify_one();

    0 // noErr
}

/// Owns the IOProc registration and device start/stop lifecycle.
///
/// On drop, stops the device and destroys the IOProc registration.
pub struct IoProcHandle {
    device_id: AudioDeviceID,
    proc_id: AudioDeviceIOProcID,
    /// Prevent the Arc from being dropped while CoreAudio holds the pointer.
    _state: Arc<SharedIOState>,
}

impl IoProcHandle {
    /// Register the IOProc callback and start the audio device.
    ///
    /// The caller must keep the returned `IoProcHandle` alive for as long as
    /// audio processing is needed. Dropping it will stop the device and
    /// unregister the callback.
    pub fn new(device_id: AudioDeviceID, state: Arc<SharedIOState>) -> Result<Self, String> {
        let mut proc_id: AudioDeviceIOProcID = std::ptr::null_mut();

        // Leak an Arc reference for the callback's client_data pointer.
        // We increment the strong count so the Arc in `_state` keeps it alive,
        // and we reconstruct + drop it in our Drop impl.
        let client_ptr = Arc::as_ptr(&state) as *mut std::ffi::c_void;
        // Prevent the raw pointer from becoming dangling by bumping the refcount.
        std::mem::forget(state.clone());

        let status: OSStatus = unsafe {
            AudioDeviceCreateIOProcID(device_id, Some(io_proc), client_ptr, &mut proc_id)
        };
        if status != kAudioHardwareNoError as OSStatus {
            // Drop the extra refcount we just leaked.
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
            // Clean up the IOProc registration before returning.
            unsafe {
                AudioDeviceDestroyIOProcID(device_id, proc_id);
                Arc::from_raw(client_ptr as *const SharedIOState);
            }
            return Err(format!("AudioDeviceStart failed with status {}", status));
        }

        Ok(IoProcHandle {
            device_id,
            proc_id,
            _state: state,
        })
    }
}

impl Drop for IoProcHandle {
    fn drop(&mut self) {
        unsafe {
            AudioDeviceStop(self.device_id, Some(io_proc));
            AudioDeviceDestroyIOProcID(self.device_id, self.proc_id);
            // Reclaim the extra Arc reference we leaked in new().
            let client_ptr = Arc::as_ptr(&self._state) as *const SharedIOState;
            Arc::from_raw(client_ptr);
        }
    }
}
