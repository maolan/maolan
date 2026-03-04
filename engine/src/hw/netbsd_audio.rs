use super::common;
use super::error_fmt;
use super::latency;
use super::ports;
use crate::audio::io::AudioIO;
use nix::libc;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::sync::Arc;

pub use super::midi_hub::MidiHub;
pub use super::options::HwOptions;

const AUMODE_PLAY: u32 = 0x01;
const AUMODE_RECORD: u32 = 0x02;
const AUDIO_ENCODING_SLINEAR_LE: u32 = 6;
const AUDIO_ENCODING_SLINEAR_BE: u32 = 7;

#[cfg(target_endian = "little")]
const AUDIO_ENCODING_SLINEAR_NATIVE: u32 = AUDIO_ENCODING_SLINEAR_LE;
#[cfg(target_endian = "big")]
const AUDIO_ENCODING_SLINEAR_NATIVE: u32 = AUDIO_ENCODING_SLINEAR_BE;

#[derive(Clone, Copy, Debug)]
struct SampleFormat {
    bits: usize,
    bps: usize,
    little_endian: bool,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct AudioPrinfo {
    sample_rate: libc::c_uint,
    channels: libc::c_uint,
    precision: libc::c_uint,
    encoding: libc::c_uint,
    gain: libc::c_uint,
    port: libc::c_uint,
    seek: libc::c_uint,
    avail_ports: libc::c_uint,
    buffer_size: libc::c_uint,
    _ispare: [libc::c_uint; 1],
    samples: libc::c_uint,
    eof: libc::c_uint,
    pause: libc::c_uchar,
    error: libc::c_uchar,
    waiting: libc::c_uchar,
    balance: libc::c_uchar,
    cspare: [libc::c_uchar; 2],
    open: libc::c_uchar,
    active: libc::c_uchar,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct AudioInfo {
    play: AudioPrinfo,
    record: AudioPrinfo,
    monitor_gain: libc::c_uint,
    blocksize: libc::c_uint,
    hiwat: libc::c_uint,
    lowat: libc::c_uint,
    _ispare1: libc::c_uint,
    mode: libc::c_uint,
}

nix::ioctl_read!(audio_getinfo, b'A', 21, AudioInfo);
nix::ioctl_readwrite!(audio_setinfo, b'A', 22, AudioInfo);
nix::ioctl_read!(audio_getprops, b'A', 34, i32);

pub struct HwDriver {
    audio: std::fs::File,
    audio_ins: Vec<Arc<AudioIO>>,
    audio_outs: Vec<Arc<AudioIO>>,
    output_gain_linear: f32,
    output_balance: f32,
    sample_rate: i32,
    period_frames: usize,
    channels_in: usize,
    channels_out: usize,
    nperiods: usize,
    sync_mode: bool,
    input_latency_frames: usize,
    output_latency_frames: usize,
    in_format: SampleFormat,
    out_format: SampleFormat,
    capture_buffer: Vec<u8>,
    playback_buffer: Vec<u8>,
    playing: bool,
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
            .field("nperiods", &self.nperiods)
            .field("sync_mode", &self.sync_mode)
            .field("input_latency_frames", &self.input_latency_frames)
            .field("output_latency_frames", &self.output_latency_frames)
            .finish()
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
            "/dev/audio"
        } else {
            device
        };
        let mut open_opts = std::fs::OpenOptions::new();
        open_opts.read(true).write(true);
        let mut open_flags = 0;
        if options.exclusive {
            open_flags |= libc::O_EXCL;
        }
        if open_flags != 0 {
            open_opts.custom_flags(open_flags);
        }
        let audio = open_opts.open(requested_device).map_err(|e| {
            error_fmt::backend_open_error("audio(4)", "duplex", requested_device, e)
        })?;

        let fd = audio.as_raw_fd();
        let mut props = 0_i32;
        unsafe {
            audio_getprops(fd, &mut props).map_err(|_| {
                error_fmt::backend_io_error("audio(4)", "duplex", std::io::Error::last_os_error())
            })?;
        }
        if (props & 0x10) == 0 || (props & 0x20) == 0 {
            return Err("audio(4) device does not support full-duplex play+record".to_string());
        }

        let requested_bits = normalize_bits(bits) as u32;
        let requested_bps = bits_to_bps(requested_bits);
        let requested_channels = 2_u32;
        let requested_blocksize = if options.ignore_hwbuf {
            0_u32
        } else {
            (options
                .period_frames
                .max(1)
                .saturating_mul(requested_channels as usize)
                .saturating_mul(requested_bps as usize)) as u32
        };

        let mut set_info = audio_initinfo();
        set_info.mode = AUMODE_PLAY | AUMODE_RECORD;
        set_info.play.sample_rate = rate.max(1) as u32;
        set_info.record.sample_rate = rate.max(1) as u32;
        set_info.play.channels = requested_channels;
        set_info.record.channels = requested_channels;
        set_info.play.precision = requested_bits;
        set_info.record.precision = requested_bits;
        set_info.play.encoding = AUDIO_ENCODING_SLINEAR_NATIVE;
        set_info.record.encoding = AUDIO_ENCODING_SLINEAR_NATIVE;
        if requested_blocksize > 0 {
            set_info.blocksize = requested_blocksize;
        }

        unsafe {
            audio_setinfo(fd, &mut set_info).map_err(|_| {
                error_fmt::backend_io_error("audio(4)", "duplex", std::io::Error::last_os_error())
            })?;
        }

        let mut got_info = zeroed_audio_info();
        unsafe {
            audio_getinfo(fd, &mut got_info).map_err(|_| {
                error_fmt::backend_io_error("audio(4)", "duplex", std::io::Error::last_os_error())
            })?;
        }

        let in_format = sample_format_from_prinfo(&got_info.record)?;
        let out_format = sample_format_from_prinfo(&got_info.play)?;
        let channels_in = got_info.record.channels.max(1) as usize;
        let channels_out = got_info.play.channels.max(1) as usize;
        let sample_rate = got_info.play.sample_rate.max(1) as i32;
        let blocksize = got_info.blocksize as usize;
        let period_frames = if blocksize > 0 {
            (blocksize / (channels_out.saturating_mul(out_format.bps))).max(1)
        } else {
            options.period_frames.max(1)
        };

        let audio_ins: Vec<Arc<AudioIO>> = (0..channels_in)
            .map(|_| Arc::new(AudioIO::new(period_frames)))
            .collect();
        let audio_outs: Vec<Arc<AudioIO>> = (0..channels_out)
            .map(|_| Arc::new(AudioIO::new(period_frames)))
            .collect();

        let capture_buffer = vec![0_u8; period_frames * channels_in * in_format.bps];
        let playback_buffer = vec![0_u8; period_frames * channels_out * out_format.bps];

        Ok(Self {
            audio,
            audio_ins,
            audio_outs,
            output_gain_linear: 1.0,
            output_balance: 0.0,
            sample_rate,
            period_frames,
            channels_in,
            channels_out,
            nperiods: options.nperiods.max(1),
            sync_mode: options.sync_mode,
            input_latency_frames: options.input_latency_frames,
            output_latency_frames: options.output_latency_frames,
            in_format,
            out_format,
            capture_buffer,
            playback_buffer,
            playing: false,
        })
    }

    pub fn new(device: &str, rate: i32, bits: i32) -> Result<Self, String> {
        Self::new_with_options(device, rate, bits, HwOptions::default())
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

    pub fn set_playing(&mut self, playing: bool) {
        self.playing = playing;
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

    pub fn channel(&mut self) -> NetBsdAudioChannel<'_> {
        NetBsdAudioChannel { driver: self }
    }

    fn run_cycle_inner(&mut self) -> Result<(), String> {
        read_exact_file(&mut self.audio, &mut self.capture_buffer)
            .map_err(|e| error_fmt::backend_io_error("audio(4)", "capture", e))?;

        let in_fmt = self.in_format;
        let channels_in = self.channels_in;
        let frames = self.period_frames;

        ports::fill_ports_from_interleaved(&self.audio_ins, frames, false, |ch, frame| {
            let idx = (frame * channels_in + ch) * in_fmt.bps;
            decode_sample(&self.capture_buffer[idx..idx + in_fmt.bps], in_fmt)
        });

        self.playback_buffer.fill(0);
        let out_fmt = self.out_format;
        let channels_out = self.channels_out;
        ports::write_interleaved_from_ports(
            &self.audio_outs,
            frames,
            self.output_gain_linear,
            self.output_balance,
            false,
            |ch, frame, sample| {
                let idx = (frame * channels_out + ch) * out_fmt.bps;
                encode_sample(
                    sample,
                    out_fmt,
                    &mut self.playback_buffer[idx..idx + out_fmt.bps],
                );
            },
        );

        write_all_file(&mut self.audio, &self.playback_buffer)
            .map_err(|e| error_fmt::backend_io_error("audio(4)", "playback", e))?;
        Ok(())
    }
}

pub struct NetBsdAudioChannel<'a> {
    driver: &'a mut HwDriver,
}

impl<'a> NetBsdAudioChannel<'a> {
    pub fn run_cycle(&mut self) -> Result<(), String> {
        self.driver.run_cycle_inner()
    }

    pub fn run_assist_step(&mut self) -> Result<bool, String> {
        Ok(false)
    }
}

fn zeroed_audio_info() -> AudioInfo {
    unsafe { std::mem::zeroed() }
}

fn audio_initinfo() -> AudioInfo {
    let mut info = zeroed_audio_info();
    unsafe {
        std::ptr::write_bytes((&mut info as *mut AudioInfo).cast::<u8>(), 0xff, 1);
    }
    info
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

fn bits_to_bps(bits: u32) -> u32 {
    if bits <= 8 {
        1
    } else if bits <= 16 {
        2
    } else if bits <= 24 {
        3
    } else {
        4
    }
}

fn sample_format_from_prinfo(pr: &AudioPrinfo) -> Result<SampleFormat, String> {
    let bits = pr.precision as usize;
    let bps = bits_to_bps(pr.precision) as usize;
    let little_endian = match pr.encoding {
        AUDIO_ENCODING_SLINEAR_LE => true,
        AUDIO_ENCODING_SLINEAR_BE => false,
        _ => {
            return Err(format!(
                "audio(4) negotiated unsupported encoding {}",
                pr.encoding
            ));
        }
    };
    if !(1..=4).contains(&bps) {
        return Err(format!(
            "audio(4) negotiated unsupported sample bytes-per-sample {}",
            bps
        ));
    }
    if !(8..=32).contains(&bits) {
        return Err(format!(
            "audio(4) negotiated unsupported sample precision {}",
            bits
        ));
    }
    Ok(SampleFormat {
        bits,
        bps,
        little_endian,
    })
}

fn decode_sample(src: &[u8], fmt: SampleFormat) -> f32 {
    let mut sample = decode_signed(src, fmt.bps, fmt.little_endian);
    if fmt.bits < 32 {
        let shift = (32 - fmt.bits) as u32;
        sample = (sample << shift) >> shift;
    }
    let denom = if fmt.bits == 32 {
        2_147_483_648.0_f32
    } else {
        ((1_i64 << (fmt.bits - 1)) as f32).max(1.0)
    };
    (sample as f32 / denom).clamp(-1.0, 1.0)
}

fn encode_sample(sample: f32, fmt: SampleFormat, dst: &mut [u8]) {
    let max = if fmt.bits == 32 {
        i32::MAX as i64
    } else {
        (1_i64 << (fmt.bits - 1)) - 1
    };
    let min = if fmt.bits == 32 {
        i32::MIN as i64
    } else {
        -(1_i64 << (fmt.bits - 1))
    };
    let mut value = (sample.clamp(-1.0, 1.0) * max as f32).round() as i64;
    value = value.clamp(min, max);
    encode_signed(value as i32, fmt.bps, fmt.little_endian, dst);
}

fn decode_signed(src: &[u8], bps: usize, little_endian: bool) -> i32 {
    match bps {
        1 => i8::from_ne_bytes([src[0]]) as i32,
        2 => {
            let bytes = [src[0], src[1]];
            if little_endian {
                i16::from_le_bytes(bytes) as i32
            } else {
                i16::from_be_bytes(bytes) as i32
            }
        }
        3 => {
            let raw = if little_endian {
                (src[0] as i32) | ((src[1] as i32) << 8) | ((src[2] as i32) << 16)
            } else {
                (src[2] as i32) | ((src[1] as i32) << 8) | ((src[0] as i32) << 16)
            };
            if (raw & 0x0080_0000) != 0 {
                raw | !0x00ff_ffff
            } else {
                raw
            }
        }
        4 => {
            let bytes = [src[0], src[1], src[2], src[3]];
            if little_endian {
                i32::from_le_bytes(bytes)
            } else {
                i32::from_be_bytes(bytes)
            }
        }
        _ => 0,
    }
}

fn encode_signed(value: i32, bps: usize, little_endian: bool, dst: &mut [u8]) {
    match bps {
        1 => {
            dst[0] = value as i8 as u8;
        }
        2 => {
            let bytes = if little_endian {
                (value as i16).to_le_bytes()
            } else {
                (value as i16).to_be_bytes()
            };
            dst[0] = bytes[0];
            dst[1] = bytes[1];
        }
        3 => {
            if little_endian {
                dst[0] = value as u8;
                dst[1] = (value >> 8) as u8;
                dst[2] = (value >> 16) as u8;
            } else {
                dst[0] = (value >> 16) as u8;
                dst[1] = (value >> 8) as u8;
                dst[2] = value as u8;
            }
        }
        4 => {
            let bytes = if little_endian {
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

fn read_exact_file(file: &mut std::fs::File, mut out: &mut [u8]) -> std::io::Result<()> {
    while !out.is_empty() {
        match file.read(out) {
            Ok(0) => return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof)),
            Ok(n) => out = &mut out[n..],
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

fn write_all_file(file: &mut std::fs::File, mut data: &[u8]) -> std::io::Result<()> {
    while !data.is_empty() {
        match file.write(data) {
            Ok(0) => return Err(std::io::Error::from(std::io::ErrorKind::WriteZero)),
            Ok(n) => data = &data[n..],
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

crate::impl_hw_worker_traits_for_driver!(HwDriver);
crate::impl_hw_device_for_driver!(HwDriver);
crate::impl_hw_midi_hub_traits!(MidiHub);

unsafe impl Send for HwDriver {}
unsafe impl Sync for HwDriver {}
