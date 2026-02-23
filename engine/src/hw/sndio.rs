use super::common;
use super::error_fmt;
use super::latency;
use super::ports;
use crate::audio::io::AudioIO;
use std::ffi::CString;
use std::sync::Arc;

pub use super::midi_hub::MidiHub;
pub use super::options::HwOptions;

const SIO_PLAY: u32 = 0x1;
const SIO_REC: u32 = 0x2;
const SIO_IGNORE: u32 = 0;
const SIO_SYNC: u32 = 1;

#[cfg(target_endian = "little")]
const SIO_LE_NATIVE: u32 = 1;
#[cfg(target_endian = "big")]
const SIO_LE_NATIVE: u32 = 0;

#[repr(C)]
struct SioPar {
    bits: u32,
    bps: u32,
    sig: u32,
    le: u32,
    msb: u32,
    rchan: u32,
    pchan: u32,
    rate: u32,
    bufsz: u32,
    xrun: u32,
    round: u32,
    appbufsz: u32,
    __pad: [i32; 3],
    __magic: u32,
}

enum SioHdl {}

#[link(name = "sndio")]
unsafe extern "C" {
    fn sio_initpar(par: *mut SioPar);
    fn sio_open(name: *const std::os::raw::c_char, mode: u32, nbio: i32) -> *mut SioHdl;
    fn sio_close(hdl: *mut SioHdl);
    fn sio_setpar(hdl: *mut SioHdl, par: *mut SioPar) -> i32;
    fn sio_getpar(hdl: *mut SioHdl, par: *mut SioPar) -> i32;
    fn sio_start(hdl: *mut SioHdl) -> i32;
    fn sio_stop(hdl: *mut SioHdl) -> i32;
    fn sio_read(hdl: *mut SioHdl, addr: *mut std::os::raw::c_void, nbytes: usize) -> usize;
    fn sio_write(hdl: *mut SioHdl, addr: *const std::os::raw::c_void, nbytes: usize) -> usize;
    fn sio_eof(hdl: *mut SioHdl) -> i32;
}

pub struct HwDriver {
    hdl: *mut SioHdl,
    audio_ins: Vec<Arc<AudioIO>>,
    audio_outs: Vec<Arc<AudioIO>>,
    output_gain_linear: f32,
    output_balance: f32,
    sample_rate: i32,
    period_frames: usize,
    channels_in: usize,
    channels_out: usize,
    bits: usize,
    bps: usize,
    le: bool,
    msb: bool,
    nperiods: usize,
    sync_mode: bool,
    input_latency_frames: usize,
    output_latency_frames: usize,
    capture_buffer: Vec<u8>,
    playback_buffer: Vec<u8>,
}

impl std::fmt::Debug for HwDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HwDriver")
            .field("audio_ins", &self.audio_ins.len())
            .field("audio_outs", &self.audio_outs.len())
            .field("output_gain_linear", &self.output_gain_linear)
            .field("output_balance", &self.output_balance)
            .field("sample_rate", &self.sample_rate)
            .field("period_frames", &self.period_frames)
            .field("channels_in", &self.channels_in)
            .field("channels_out", &self.channels_out)
            .field("bits", &self.bits)
            .field("bps", &self.bps)
            .field("le", &self.le)
            .field("msb", &self.msb)
            .field("nperiods", &self.nperiods)
            .field("sync_mode", &self.sync_mode)
            .field("input_latency_frames", &self.input_latency_frames)
            .field("output_latency_frames", &self.output_latency_frames)
            .finish()
    }
}

impl Drop for HwDriver {
    fn drop(&mut self) {
        if self.hdl.is_null() {
            return;
        }
        unsafe {
            let _ = sio_stop(self.hdl);
            sio_close(self.hdl);
        }
        self.hdl = std::ptr::null_mut();
    }
}

impl Default for HwOptions {
    fn default() -> Self {
        Self {
            exclusive: false,
            period_frames: 1024,
            nperiods: 2,
            ignore_hwbuf: false,
            sync_mode: false,
            input_latency_frames: 0,
            output_latency_frames: 0,
        }
    }
}

impl HwDriver {
    pub fn new_with_options(
        device: &str,
        rate: i32,
        bits: i32,
        options: HwOptions,
    ) -> Result<Self, String> {
        let requested_device = if device.trim().is_empty() {
            "default"
        } else {
            device
        };
        let name = CString::new(requested_device)
            .map_err(|e| error_fmt::backend_open_error("sndio", "duplex", requested_device, e))?;

        let hdl = unsafe { sio_open(name.as_ptr(), SIO_PLAY | SIO_REC, 0) };
        if hdl.is_null() {
            return Err(error_fmt::backend_open_error(
                "sndio",
                "duplex",
                requested_device,
                "sio_open returned null",
            ));
        }

        let mut par = Self::desired_par(rate, bits, options);
        let configured = unsafe { sio_setpar(hdl, &mut par) };
        if configured != 1 {
            unsafe {
                sio_close(hdl);
            }
            return Err(error_fmt::backend_io_error(
                "sndio",
                "duplex",
                "sio_setpar failed",
            ));
        }

        let got = unsafe { sio_getpar(hdl, &mut par) };
        if got != 1 {
            unsafe {
                sio_close(hdl);
            }
            return Err(error_fmt::backend_io_error(
                "sndio",
                "duplex",
                "sio_getpar failed",
            ));
        }

        let started = unsafe { sio_start(hdl) };
        if started != 1 {
            unsafe {
                sio_close(hdl);
            }
            return Err(error_fmt::backend_io_error(
                "sndio",
                "duplex",
                "sio_start failed",
            ));
        }

        let sample_rate = par.rate.max(1) as i32;
        let period_frames = par.round.max(1) as usize;
        let channels_in = par.rchan.max(1) as usize;
        let channels_out = par.pchan.max(1) as usize;
        let bps = par.bps.max(1) as usize;
        let bits = par.bits.max(8) as usize;
        let le = par.le != 0;
        let msb = par.msb != 0;

        let audio_ins: Vec<Arc<AudioIO>> = (0..channels_in)
            .map(|_| Arc::new(AudioIO::new(period_frames)))
            .collect();
        let audio_outs: Vec<Arc<AudioIO>> = (0..channels_out)
            .map(|_| Arc::new(AudioIO::new(period_frames)))
            .collect();

        let capture_buffer = vec![0_u8; period_frames * channels_in * bps];
        let playback_buffer = vec![0_u8; period_frames * channels_out * bps];

        Ok(Self {
            hdl,
            audio_ins,
            audio_outs,
            output_gain_linear: 1.0,
            output_balance: 0.0,
            sample_rate,
            period_frames,
            channels_in,
            channels_out,
            bits,
            bps,
            le,
            msb,
            nperiods: options.nperiods.max(1),
            sync_mode: options.sync_mode,
            input_latency_frames: options.input_latency_frames,
            output_latency_frames: options.output_latency_frames,
            capture_buffer,
            playback_buffer,
        })
    }

    pub fn input_channels(&self) -> usize {
        self.channels_in
    }

    pub fn output_channels(&self) -> usize {
        self.channels_out
    }

    pub fn sample_rate(&self) -> i32 {
        self.sample_rate
    }

    pub fn cycle_samples(&self) -> usize {
        self.period_frames
    }

    pub fn input_port(&self, idx: usize) -> Option<Arc<AudioIO>> {
        self.audio_ins.get(idx).cloned()
    }

    pub fn output_port(&self, idx: usize) -> Option<Arc<AudioIO>> {
        self.audio_outs.get(idx).cloned()
    }

    pub fn set_output_gain_balance(&mut self, gain: f32, balance: f32) {
        self.output_gain_linear = gain.max(0.0);
        self.output_balance = balance.clamp(-1.0, 1.0);
    }

    pub fn output_meter_db(&self, gain: f32, balance: f32) -> Vec<f32> {
        common::output_meter_db(&self.audio_outs, gain, balance)
    }

    pub fn latency_ranges(&self) -> ((usize, usize), (usize, usize)) {
        latency::latency_ranges(
            self.cycle_samples(),
            self.nperiods,
            self.sync_mode,
            self.input_latency_frames,
            self.output_latency_frames,
        )
    }

    pub fn channel(&mut self) -> SndioChannel<'_> {
        SndioChannel { driver: self }
    }

    fn desired_par(rate: i32, bits: i32, options: HwOptions) -> SioPar {
        let effective_bits = normalize_bits(bits) as u32;
        let bps = default_bps(effective_bits) as u32;
        let period = options.period_frames.max(1) as u32;
        let appbuf = period.saturating_mul(options.nperiods.max(1) as u32);
        let mut par = unsafe { std::mem::zeroed::<SioPar>() };
        unsafe {
            sio_initpar(&mut par);
        }
        par.bits = effective_bits;
        par.bps = bps;
        par.sig = 1;
        par.le = SIO_LE_NATIVE;
        par.msb = 1;
        par.rchan = 2;
        par.pchan = 2;
        par.rate = rate.max(1) as u32;
        par.round = period;
        par.appbufsz = appbuf;
        par.bufsz = appbuf;
        par.xrun = if options.sync_mode {
            SIO_SYNC
        } else {
            SIO_IGNORE
        };
        par
    }

    fn run_cycle_inner(&mut self) -> Result<(), String> {
        Self::read_exact(self.hdl, &mut self.capture_buffer)?;

        let bits = self.bits;
        let bps = self.bps;
        let le = self.le;
        let msb = self.msb;
        let channels_in = self.channels_in;
        let frames = self.period_frames;

        ports::fill_ports_from_interleaved(&self.audio_ins, frames, false, |ch, frame| {
            let idx = (frame * channels_in + ch) * bps;
            decode_sample(&self.capture_buffer[idx..idx + bps], bits, bps, le, msb)
        });

        self.playback_buffer.fill(0);
        let channels_out = self.channels_out;
        ports::write_interleaved_from_ports(
            &self.audio_outs,
            frames,
            self.output_gain_linear,
            self.output_balance,
            false,
            |ch, frame, sample| {
                let idx = (frame * channels_out + ch) * bps;
                encode_sample(
                    sample,
                    bits,
                    bps,
                    le,
                    msb,
                    &mut self.playback_buffer[idx..idx + bps],
                );
            },
        );

        Self::write_exact(self.hdl, &self.playback_buffer)
    }

    fn read_exact(hdl: *mut SioHdl, out: &mut [u8]) -> Result<(), String> {
        let mut offset = 0;
        while offset < out.len() {
            let got = unsafe {
                sio_read(
                    hdl,
                    out[offset..].as_mut_ptr().cast::<std::os::raw::c_void>(),
                    out.len() - offset,
                )
            };
            if got == 0 {
                let eof = unsafe { sio_eof(hdl) } != 0;
                return Err(error_fmt::backend_io_error(
                    "sndio",
                    "capture",
                    if eof { "stream closed" } else { "short read" },
                ));
            }
            offset += got;
        }
        Ok(())
    }

    fn write_exact(hdl: *mut SioHdl, data: &[u8]) -> Result<(), String> {
        let mut offset = 0;
        while offset < data.len() {
            let wrote = unsafe {
                sio_write(
                    hdl,
                    data[offset..].as_ptr().cast::<std::os::raw::c_void>(),
                    data.len() - offset,
                )
            };
            if wrote == 0 {
                let eof = unsafe { sio_eof(hdl) } != 0;
                return Err(error_fmt::backend_io_error(
                    "sndio",
                    "playback",
                    if eof { "stream closed" } else { "short write" },
                ));
            }
            offset += wrote;
        }
        Ok(())
    }
}

pub struct SndioChannel<'a> {
    driver: &'a mut HwDriver,
}

impl<'a> SndioChannel<'a> {
    pub fn run_cycle(&mut self) -> Result<(), String> {
        self.driver.run_cycle_inner()
    }

    pub fn run_assist_step(&mut self) -> Result<bool, String> {
        Ok(false)
    }
}

fn normalize_bits(bits: i32) -> usize {
    match bits {
        8 => 8,
        16 => 16,
        24 => 24,
        32 => 32,
        _ => 32,
    }
}

fn default_bps(bits: u32) -> usize {
    if bits <= 8 {
        1
    } else if bits <= 16 {
        2
    } else {
        4
    }
}

fn decode_sample(src: &[u8], bits: usize, bps: usize, le: bool, msb: bool) -> f32 {
    let container_bits = bps.saturating_mul(8);
    let mut value = decode_container(src, bps, le);

    if bits < container_bits && msb {
        value >>= (container_bits - bits) as u32;
    }
    if bits < 32 {
        let shift = (32 - bits) as u32;
        value = (value << shift) >> shift;
    }

    let denom = if bits >= 32 {
        2_147_483_648.0_f32
    } else {
        ((1_i64 << (bits - 1)) as f32).max(1.0)
    };
    (value as f32 / denom).clamp(-1.0, 1.0)
}

fn encode_sample(sample: f32, bits: usize, bps: usize, le: bool, msb: bool, dst: &mut [u8]) {
    let container_bits = bps.saturating_mul(8);
    let max = if bits >= 32 {
        i32::MAX as i64
    } else {
        (1_i64 << (bits - 1)) - 1
    };
    let min = if bits >= 32 {
        i32::MIN as i64
    } else {
        -(1_i64 << (bits - 1))
    };

    let mut value = (sample.clamp(-1.0, 1.0) * max as f32).round() as i64;
    value = value.clamp(min, max);

    let mut stored = value as i32;
    if bits < container_bits && msb {
        stored <<= (container_bits - bits) as u32;
    }
    encode_container(stored, bps, le, dst);
}

fn decode_container(src: &[u8], bps: usize, le: bool) -> i32 {
    match bps {
        1 => i8::from_ne_bytes([src[0]]) as i32,
        2 => {
            let v = [src[0], src[1]];
            if le {
                i16::from_le_bytes(v) as i32
            } else {
                i16::from_be_bytes(v) as i32
            }
        }
        4 => {
            let v = [src[0], src[1], src[2], src[3]];
            if le {
                i32::from_le_bytes(v)
            } else {
                i32::from_be_bytes(v)
            }
        }
        _ => 0,
    }
}

fn encode_container(value: i32, bps: usize, le: bool, dst: &mut [u8]) {
    match bps {
        1 => {
            dst[0] = value as i8 as u8;
        }
        2 => {
            let bytes = if le {
                (value as i16).to_le_bytes()
            } else {
                (value as i16).to_be_bytes()
            };
            dst[0] = bytes[0];
            dst[1] = bytes[1];
        }
        4 => {
            let bytes = if le {
                value.to_le_bytes()
            } else {
                value.to_be_bytes()
            };
            dst[0] = bytes[0];
            dst[1] = bytes[1];
            dst[2] = bytes[2];
            dst[3] = bytes[3];
        }
        _ => {}
    }
}

crate::impl_hw_worker_traits_for_driver!(HwDriver);
crate::impl_hw_device_for_driver!(HwDriver);
crate::impl_hw_midi_hub_traits!(MidiHub);

unsafe impl Send for HwDriver {}
unsafe impl Sync for HwDriver {}
