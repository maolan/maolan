#![allow(dead_code)]

use crate::audio::io::AudioIO;
use crate::hw::convert_policy;
use nix::libc;
use std::{
    fs::File,
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    sync::{Arc, Mutex},
};
use wavers::Samples;

pub use super::midi_hub::MidiHub;

mod audio_core;
mod channel;
mod consts;
mod convert;
mod driver;
mod io_util;
mod ioctl;
mod sync;

pub use self::channel::OSSChannel;
pub use self::consts::*;
pub use self::driver::HwDriver;
pub use self::ioctl::{AudioInfo, BufferInfo, add_to_sync_group, start_sync_group};
pub use crate::hw::options::HwOptions;

use self::audio_core::DoubleBufferedChannel;
use self::convert::*;
use self::io_util::*;
use self::ioctl::*;
use self::sync::{Correction, DuplexSync, FrameClock, get_or_create_duplex_sync};

#[cfg(target_endian = "little")]
const AFMT_S16_FOREIGN: u32 = AFMT_S16_BE;
#[cfg(target_endian = "big")]
const AFMT_S16_FOREIGN: u32 = AFMT_S16_LE;
#[cfg(target_endian = "little")]
const AFMT_S24_FOREIGN: u32 = AFMT_S24_BE;
#[cfg(target_endian = "big")]
const AFMT_S24_FOREIGN: u32 = AFMT_S24_LE;
#[cfg(target_endian = "little")]
const AFMT_S32_FOREIGN: u32 = AFMT_S32_BE;
#[cfg(target_endian = "big")]
const AFMT_S32_FOREIGN: u32 = AFMT_S32_LE;

#[derive(Debug)]
pub struct Audio {
    dsp: File,
    pub channels: Vec<Arc<AudioIO>>,
    pub input: bool,
    pub output_gain_linear: f32,
    pub output_balance: f32,
    pub rate: i32,
    pub format: u32,
    pub samples: usize,
    pub chsamples: usize,
    buffer: Samples<i32>,
    pub audio_info: AudioInfo,
    pub buffer_info: BufferInfo,
    frame_size_bytes: usize,
    buffer_frames_cached: i64,
    caps: i32,
    mapped: bool,
    map: *mut libc::c_void,
    map_progress_bytes: usize,
    last_published_balance: i64,
    frame_clock: FrameClock,
    frame_stamp: i64,
    sync_path: String,
    duplex_sync: Arc<Mutex<DuplexSync>>,
    correction: Correction,
    channel: DoubleBufferedChannel,
}

impl Audio {
    fn sample_format_candidates(bits: i32) -> Vec<u32> {
        fn add_pair(candidates: &mut Vec<u32>, native: u32, foreign: u32) {
            candidates.push(native);
            candidates.push(foreign);
        }

        let mut candidates = Vec::with_capacity(7);
        match bits {
            32 => {
                add_pair(&mut candidates, AFMT_S32_NE, AFMT_S32_FOREIGN);
                add_pair(&mut candidates, AFMT_S24_NE, AFMT_S24_FOREIGN);
                add_pair(&mut candidates, AFMT_S16_NE, AFMT_S16_FOREIGN);
                candidates.push(AFMT_S8);
            }
            24 => {
                add_pair(&mut candidates, AFMT_S24_NE, AFMT_S24_FOREIGN);
                add_pair(&mut candidates, AFMT_S16_NE, AFMT_S16_FOREIGN);
                candidates.push(AFMT_S8);
            }
            16 => {
                add_pair(&mut candidates, AFMT_S16_NE, AFMT_S16_FOREIGN);
                candidates.push(AFMT_S8);
            }
            8 => candidates.push(AFMT_S8),
            _ => {
                add_pair(&mut candidates, AFMT_S16_NE, AFMT_S16_FOREIGN);
                candidates.push(AFMT_S8);
            }
        }
        candidates
    }

    fn negotiate_sample_format(fd: i32, bits: i32) -> Result<u32, std::io::Error> {
        let candidates = Self::sample_format_candidates(bits);
        let mut last_errno = None;
        let mut last_unsupported = None;
        for candidate in candidates {
            let mut negotiated = candidate;
            let setfmt = unsafe { oss_set_format(fd, &mut negotiated) };
            match setfmt {
                Ok(_) => {
                    if supported_sample_format(negotiated) {
                        return Ok(negotiated);
                    }
                    last_unsupported = Some(negotiated);
                }
                Err(_) => {
                    last_errno = Some(std::io::Error::last_os_error());
                }
            }
        }
        if let Some(format) = last_unsupported {
            return Err(std::io::Error::other(format!(
                "Unsupported OSS sample format after setfmt fallback chain: {format:#x}"
            )));
        }
        Err(last_errno.unwrap_or_else(|| std::io::Error::other("OSS setfmt failed for all fallback formats")))
    }

    pub fn fd(&self) -> i32 {
        self.dsp.as_raw_fd()
    }

    pub fn start_trigger(&self) -> std::io::Result<()> {
        if (self.caps & PCM_CAP_TRIGGER) == 0 {
            return Ok(());
        }
        let trig: i32 = if self.input {
            PCM_ENABLE_INPUT
        } else {
            PCM_ENABLE_OUTPUT
        };
        unsafe { oss_set_trigger(self.dsp.as_raw_fd(), &trig) }
            .map(|_| ())
            .map_err(|_| std::io::Error::last_os_error())
    }

    pub fn new(
        path: &str,
        rate: i32,
        bits: i32,
        input: bool,
        options: HwOptions,
    ) -> Result<Audio, std::io::Error> {
        let mut binding = File::options();

        let mut flags = libc::O_NONBLOCK;
        if input {
            flags |= libc::O_RDONLY;
            if options.exclusive {
                flags |= libc::O_EXCL;
            }
            binding.read(true).write(false).custom_flags(flags);
        } else {
            flags |= libc::O_WRONLY;
            if options.exclusive {
                flags |= libc::O_EXCL;
            }
            binding.read(false).write(true).custom_flags(flags);
        }

        let dsp = binding.open(path)?;

        let mut audio_info = AudioInfo::new();
        unsafe {
            oss_get_info(dsp.as_raw_fd(), &mut audio_info)
                .map_err(|_| std::io::Error::last_os_error())?;
        }
        let mut channels = if audio_info.max_channels > 0 {
            audio_info.max_channels
        } else {
            2_i32
        };
        let mut effective_rate = rate;
        let cooked = 0_i32;
        let format = Self::negotiate_sample_format(dsp.as_raw_fd(), bits)?;
        unsafe {
            if options.exclusive {
                oss_set_cooked(dsp.as_raw_fd(), &cooked)
                    .map_err(|_| std::io::Error::last_os_error())?;
            }
            oss_set_channels(dsp.as_raw_fd(), &mut channels)
                .map_err(|_| std::io::Error::last_os_error())?;
            oss_set_speed(dsp.as_raw_fd(), &mut effective_rate)
                .map_err(|_| std::io::Error::last_os_error())?;
        }
        if effective_rate != rate {
            return Err(std::io::Error::other(format!(
                "OSS device forced sample rate {effective_rate} (requested {rate})"
            )));
        }

        let bytes_per_sample = bytes_per_sample(format)
            .ok_or_else(|| std::io::Error::other(format!("Unsupported format: {format:#x}")))?;
        let frame_size = (channels as usize) * bytes_per_sample;
        if !options.ignore_hwbuf {
            let requested_frag_bytes = options.period_frames.saturating_mul(frame_size).max(1);
            let frag_size_pow2 = requested_frag_bytes.next_power_of_two();
            let frag_shift = frag_size_pow2.trailing_zeros() as i32;
            let mut frg = ((options.nperiods.max(1) as i32) << 16) | (frag_shift & 0xFFFF);
            unsafe {
                oss_set_fragment(dsp.as_raw_fd(), &mut frg)
                    .map_err(|_| std::io::Error::last_os_error())?;
            }
        }

        let mut buffer_info = BufferInfo::new();
        unsafe {
            if input {
                oss_input_buffer_info(dsp.as_raw_fd(), &mut buffer_info)
                    .map_err(|_| std::io::Error::last_os_error())?;
            } else {
                oss_output_buffer_info(dsp.as_raw_fd(), &mut buffer_info)
                    .map_err(|_| std::io::Error::last_os_error())?;
            }
        }

        if buffer_info.fragments < 1 {
            buffer_info.fragments = buffer_info.fragstotal;
        }
        if buffer_info.bytes < 1 {
            buffer_info.bytes = buffer_info.fragstotal * buffer_info.fragsize;
        }
        if buffer_info.bytes < 1 {
            return Err(std::io::Error::other("OSS buffer size is invalid"));
        }

        let mut caps = 0_i32;
        unsafe {
            oss_get_caps(dsp.as_raw_fd(), &mut caps)
                .map_err(|_| std::io::Error::last_os_error())?;
        }
        let mut sys = OssSysInfo::default();
        unsafe {
            oss_get_sysinfo(dsp.as_raw_fd(), &mut sys)
                .map_err(|_| std::io::Error::last_os_error())?;
        }
        if (caps & PCM_CAP_MMAP) != 0 {
            let ver = cstr_fixed_prefix(&sys.version);
            if ver.as_bytes().len() >= 7
                && ver.as_bytes()[..7].cmp(b"1302000") == std::cmp::Ordering::Less
            {
                caps &= !PCM_CAP_MMAP;
            }
        }

        let samples = (buffer_info.bytes as usize) / bytes_per_sample;
        let requested_period = options.period_frames.max(1);
        let hw_chsamples = if frame_size > 0 && buffer_info.fragsize > 0 {
            (buffer_info.fragsize as usize) / frame_size
        } else {
            0
        };
        let chsamples = if options.ignore_hwbuf {
            requested_period
        } else if hw_chsamples == 0 {
            requested_period
        } else if requested_period >= hw_chsamples && (requested_period % hw_chsamples == 0) {
            // Match JACK-like logical period while still honoring hardware fragment cadence.
            requested_period
        } else {
            hw_chsamples.max(1)
        };

        let buffer_bytes = chsamples * frame_size;
        let channel = if input {
            DoubleBufferedChannel::new_read(buffer_bytes, chsamples as i64)
        } else {
            DoubleBufferedChannel::new_write(buffer_bytes, chsamples as i64)
        };

        let mut map = std::ptr::null_mut();
        let mut mapped = false;
        if (caps & PCM_CAP_MMAP) != 0 {
            let prot = if input {
                libc::PROT_READ
            } else {
                libc::PROT_WRITE
            };
            let addr = unsafe {
                libc::mmap(
                    std::ptr::null_mut(),
                    buffer_info.bytes as usize,
                    prot,
                    libc::MAP_SHARED,
                    dsp.as_raw_fd(),
                    0,
                )
            };
            if addr != libc::MAP_FAILED {
                map = addr;
                mapped = true;
            }
        }

        let mut io_channels = Vec::with_capacity(channels as usize);
        for _ in 0..channels {
            io_channels.push(Arc::new(AudioIO::new(chsamples)));
        }

        let duplex_sync = get_or_create_duplex_sync(path, effective_rate, chsamples);
        let mut frame_clock = FrameClock::default();
        frame_clock.set_sample_rate(effective_rate as u32);
        {
            let mut sync = duplex_sync.lock().expect("duplex sync poisoned");
            if let Some(zero) = sync.clock_zero {
                frame_clock.zero = zero;
            } else {
                let _ = frame_clock.init_clock(effective_rate as u32);
                sync.clock_zero = Some(frame_clock.zero);
            }
        }

        let correction = Correction::default();

        let buffer_frames_cached = (buffer_info.bytes as usize / frame_size) as i64;
        let audio = Audio {
            dsp,
            channels: io_channels,
            input,
            output_gain_linear: 1.0,
            output_balance: 0.0,
            rate: effective_rate,
            format,
            samples,
            chsamples,
            buffer: Samples::new(vec![0_i32; chsamples * (channels as usize)].into_boxed_slice()),
            audio_info,
            buffer_info,
            frame_size_bytes: frame_size,
            buffer_frames_cached,
            caps,
            mapped,
            map,
            map_progress_bytes: 0,
            last_published_balance: i64::MIN,
            frame_clock,
            frame_stamp: 0,
            sync_path: path.to_string(),
            duplex_sync,
            correction,
            channel,
        };

        Ok(audio)
    }

    fn frame_size(&self) -> usize {
        self.frame_size_bytes
    }

    fn buffer_frames(&self) -> i64 {
        self.buffer_frames_cached
    }

    fn stepping(&self) -> i64 {
        self.frame_clock.stepping()
    }

    fn map_pointer(&self) -> usize {
        if self.buffer_info.bytes <= 0 {
            return 0;
        }
        self.map_progress_bytes % (self.buffer_info.bytes as usize)
    }

    fn shared_cycle_end_add(&self, delta: i64) -> i64 {
        let mut sync = self.duplex_sync.lock().expect("duplex sync poisoned");
        sync.cycle_end += delta;
        sync.cycle_end
    }

    fn shared_cycle_end_get(&self) -> i64 {
        self.duplex_sync
            .lock()
            .expect("duplex sync poisoned")
            .cycle_end
    }

    fn publish_balance(&mut self, balance: i64) {
        if self.last_published_balance == balance {
            return;
        }
        self.last_published_balance = balance;
        let mut sync = self.duplex_sync.lock().expect("duplex sync poisoned");
        if self.input {
            sync.capture_balance = Some(balance);
        } else {
            sync.playback_balance = Some(balance);
        }
    }

    fn playback_correction(&self) -> i64 {
        if self.input {
            return 0;
        }
        let mut sync = self.duplex_sync.lock().expect("duplex sync poisoned");
        match (sync.playback_balance, sync.capture_balance) {
            (Some(play), Some(capture)) => sync.correction.correct(play, capture),
            _ => 0,
        }
    }

    fn playback_prefill_frames(&self) -> i64 {
        self.duplex_sync
            .lock()
            .expect("duplex sync poisoned")
            .playback_prefill_frames
    }

    fn update_map_progress_from_count(&mut self, info: &CountInfo) -> Option<usize> {
        if self.buffer_info.bytes <= 0
            || self.buffer_info.fragsize <= 0
            || info.ptr < 0
            || info.blocks < 0
            || (info.ptr as usize) >= self.buffer_info.bytes as usize
            || ((info.ptr as usize) % self.frame_size()) != 0
        {
            return None;
        }
        let buf_bytes = self.buffer_info.bytes as usize;
        let frag_bytes = self.buffer_info.fragsize as usize;
        let ptr = info.ptr as usize;
        let mut delta = (ptr + buf_bytes - self.map_pointer()) % buf_bytes;
        let max_bytes = ((info.blocks as usize).saturating_add(1))
            .saturating_mul(frag_bytes)
            .saturating_sub(1);
        if max_bytes >= delta {
            let mut cycles = max_bytes - delta;
            cycles -= cycles % buf_bytes;
            delta += cycles;
        }
        self.map_progress_bytes = self.map_progress_bytes.saturating_add(delta);
        Some(delta)
    }

    fn read_io(&self, dst: &mut [u8], len: usize, count: &mut usize) -> std::io::Result<()> {
        read_nonblock(self.dsp.as_raw_fd(), dst, len, count)
    }

    fn write_io(&self, src: &mut [u8], len: usize, count: &mut usize) -> std::io::Result<()> {
        write_nonblock(self.dsp.as_raw_fd(), src, len, count)
    }

    fn read_map(&self, dst: &mut [u8], offset: usize, length: usize) -> usize {
        let total = self.buffer_info.bytes.max(0) as usize;
        map_read(self.map, self.mapped, total, dst, offset, length)
    }

    fn write_map(&self, src: Option<&mut [u8]>, offset: usize, length: usize) -> usize {
        let total = self.buffer_info.bytes.max(0) as usize;
        map_write(self.map, self.mapped, total, src, offset, length)
    }

    fn queued_samples(&self) -> i32 {
        let mut ptr = OssCount::default();
        let req = if self.input {
            unsafe { oss_current_iptr(self.dsp.as_raw_fd(), &mut ptr) }
        } else {
            unsafe { oss_current_optr(self.dsp.as_raw_fd(), &mut ptr) }
        };
        if req.is_ok() { ptr.fifo_samples } else { 0 }
    }

    fn get_play_underruns(&self) -> i32 {
        let mut err = AudioErrInfo::default();
        let rc = unsafe { oss_get_error(self.dsp.as_raw_fd(), &mut err) };
        if rc.is_ok() { err.play_underruns } else { 0 }
    }

    fn get_rec_overruns(&self) -> i32 {
        let mut err = AudioErrInfo::default();
        let rc = unsafe { oss_get_error(self.dsp.as_raw_fd(), &mut err) };
        if rc.is_ok() { err.rec_overruns } else { 0 }
    }

    fn check_time_and_run(&mut self) -> std::io::Result<()> {
        self.frame_stamp = self
            .frame_clock
            .now()
            .ok_or_else(|| std::io::Error::other("failed to read frame clock"))?;
        let now = self.frame_stamp;
        let wake = self.channel.wakeup_time(self, now);
        let mut processed = false;
        if now >= wake && !self.channel.total_finished(now) {
            let mut chan = std::mem::replace(
                &mut self.channel,
                if self.input {
                    DoubleBufferedChannel::new_read(0, 0)
                } else {
                    DoubleBufferedChannel::new_write(0, 0)
                },
            );
            let res = chan.process(self, now);
            self.channel = chan;
            res?;
            processed = true;
        }
        if processed {
            self.publish_balance(self.channel.balance());
        }
        Ok(())
    }

    fn sleep(&self) -> bool {
        let wakeup = self.channel.wakeup_time(self, self.frame_stamp);
        if wakeup > self.frame_stamp {
            return self.frame_clock.sleep_until_frame(wakeup);
        }
        true
    }

    fn xrun_gap(&self) -> i64 {
        let max_end = self.channel.total_end();
        if max_end < self.frame_stamp {
            self.frame_stamp - max_end
        } else {
            0
        }
    }

    pub fn read(&mut self) -> std::io::Result<()> {
        if !self.input {
            return Ok(());
        }

        let mut cycle_end = self.shared_cycle_end_add(self.chsamples as i64);
        self.check_time_and_run()?;

        let xrun = self.xrun_gap();
        if xrun > 0 {
            let skip = xrun + self.chsamples as i64;
            cycle_end = self.shared_cycle_end_add(skip);
            self.channel
                .reset_buffers(self.channel.end_frames() + skip, self.frame_size());
        }

        while !self.channel.finished(self.frame_stamp) {
            if !(self.sleep() && self.check_time_and_run().is_ok()) {
                return Err(std::io::Error::other("capture wait failed"));
            }
        }

        let mut buf = self.channel.take_buffer();
        if self
            .channels
            .iter()
            .any(crate::hw::ports::has_audio_connections)
        {
            convert_in_to_i32_connected(
                self.format,
                self.chsamples,
                buf.as_slice(),
                self.buffer.as_mut(),
                &self.channels,
            );
        }
        buf.reset();
        let end = cycle_end + self.chsamples as i64;
        if !self.channel.set_buffer(buf, end) {
            return Err(std::io::Error::other("failed to requeue capture buffer"));
        }

        self.check_time_and_run()?;
        Ok(())
    }

    pub fn write(&mut self) -> std::io::Result<()> {
        if self.input {
            return Ok(());
        }

        self.check_time_and_run()?;
        let xrun = self.xrun_gap();
        if xrun > 0 {
            let skip = xrun + self.chsamples as i64;
            self.channel
                .reset_buffers(self.channel.end_frames() + skip, self.frame_size());
        }

        while !self.channel.finished(self.frame_stamp) {
            if !(self.sleep() && self.check_time_and_run().is_ok()) {
                return Err(std::io::Error::other("playback wait failed"));
            }
        }

        let mut buf = self.channel.take_buffer();
        convert_out_from_i32_interleaved(
            self.format,
            self.channels.len(),
            self.chsamples,
            self.buffer.as_mut(),
            buf.as_mut_slice(),
        );

        let mut end = self.shared_cycle_end_get() + self.chsamples as i64;
        end += self.playback_prefill_frames();
        end += self.playback_correction();
        if !self.channel.set_buffer(buf, end) {
            return Err(std::io::Error::other("failed to requeue playback buffer"));
        }

        self.check_time_and_run()?;
        Ok(())
    }

    pub fn process(&mut self) {
        let num_channels = self.channels.len();
        let all_connected = self
            .channels
            .iter()
            .all(crate::hw::ports::has_audio_connections);

        if self.input {
            let norm_factor = convert_policy::F32_FROM_I32_MAX;
            let data_slice: &mut [i32] = self.buffer.as_mut();
            crate::hw::ports::fill_ports_from_interleaved(
                &self.channels,
                self.chsamples,
                !all_connected,
                |ch_idx, frame| {
                    let source_idx = frame * num_channels + ch_idx;
                    data_slice[source_idx] as f32 * norm_factor
                },
            );
        } else {
            let scale_factor = convert_policy::F32_TO_I32_MAX;
            let output_gain = self.output_gain_linear;
            let data_i32 = self.buffer.as_mut();
            if !all_connected {
                data_i32.fill(0);
            }
            crate::hw::ports::write_interleaved_from_ports(
                &self.channels,
                self.chsamples,
                output_gain,
                self.output_balance,
                !all_connected,
                |ch_idx, frame, sample| {
                    let target_idx = frame * num_channels + ch_idx;
                    data_i32[target_idx] = (sample.clamp(-1.0, 1.0) * scale_factor) as i32;
                },
            );
        }
    }
}

impl Drop for Audio {
    fn drop(&mut self) {
        if self.mapped && !self.map.is_null() && self.buffer_info.bytes > 0 {
            unsafe {
                let _ = libc::munmap(self.map, self.buffer_info.bytes as usize);
            }
            self.map = std::ptr::null_mut();
        }
    }
}
