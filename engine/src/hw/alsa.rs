#![allow(dead_code)]

use crate::{audio::io::AudioIO, midi::io::MidiEvent};
use alsa::pcm::{Access, Format, HwParams, PCM, State};
use alsa::{Direction, ValueOr};
use nix::libc;
use std::{
    fs::File,
    io::{ErrorKind, Read, Write},
    os::unix::fs::OpenOptionsExt,
    sync::Arc,
};
use tracing::error;

#[derive(Debug, Default)]
pub struct MidiHub {
    inputs: Vec<MidiInputDevice>,
    outputs: Vec<MidiOutputDevice>,
}

impl MidiHub {
    pub fn open_input(&mut self, path: &str) -> Result<(), String> {
        if self.inputs.iter().any(|input| input.path == path) {
            return Ok(());
        }
        let file = File::options()
            .read(true)
            .write(false)
            .custom_flags(libc::O_RDONLY | libc::O_NONBLOCK)
            .open(path)
            .map_err(|e| format!("Failed to open MIDI device '{path}': {e}"))?;
        self.inputs
            .push(MidiInputDevice::new(path.to_string(), file));
        Ok(())
    }

    pub fn open_output(&mut self, path: &str) -> Result<(), String> {
        if self.outputs.iter().any(|output| output.path == path) {
            return Ok(());
        }
        let file = File::options()
            .read(false)
            .write(true)
            .custom_flags(libc::O_WRONLY | libc::O_NONBLOCK)
            .open(path)
            .map_err(|e| format!("Failed to open MIDI output '{path}': {e}"))?;
        self.outputs
            .push(MidiOutputDevice::new(path.to_string(), file));
        Ok(())
    }

    pub fn read_events(&mut self) -> Vec<MidiEvent> {
        let mut events = Vec::with_capacity(32);
        self.read_events_into(&mut events);
        events
    }

    pub fn read_events_into(&mut self, out: &mut Vec<MidiEvent>) {
        out.clear();
        for input in &mut self.inputs {
            input.read_events_into(out);
        }
    }

    pub fn write_events(&mut self, events: &[MidiEvent]) {
        if events.is_empty() {
            return;
        }
        for output in &mut self.outputs {
            output.write_events(events);
        }
    }
}

#[derive(Debug)]
struct MidiInputDevice {
    path: String,
    file: File,
    parser: MidiParser,
}

#[derive(Debug)]
struct MidiOutputDevice {
    path: String,
    file: File,
}

impl MidiOutputDevice {
    fn new(path: String, file: File) -> Self {
        Self { path, file }
    }

    fn write_events(&mut self, events: &[MidiEvent]) {
        for event in events {
            if event.data.is_empty() {
                continue;
            }
            if let Err(err) = self.file.write_all(&event.data) {
                if err.kind() != ErrorKind::WouldBlock {
                    error!("MIDI write error on {}: {}", self.path, err);
                }
                break;
            }
        }
    }
}

impl MidiInputDevice {
    fn new(path: String, file: File) -> Self {
        Self {
            path,
            file,
            parser: MidiParser::default(),
        }
    }

    fn read_events_into(&mut self, out: &mut Vec<MidiEvent>) {
        let mut buf = [0_u8; 256];
        loop {
            match self.file.read(&mut buf) {
                Ok(0) => break,
                Ok(read) => {
                    for byte in &buf[..read] {
                        if let Some(data) = self.parser.feed(*byte) {
                            out.push(MidiEvent::new(0, data));
                        }
                    }
                }
                Err(err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(err) => {
                    error!("MIDI read error on {}: {}", self.path, err);
                    break;
                }
            }
        }
    }
}

#[derive(Debug, Default)]
struct MidiParser {
    status: Option<u8>,
    needed: usize,
    data: [u8; 2],
    len: usize,
}

impl MidiParser {
    fn feed(&mut self, byte: u8) -> Option<Vec<u8>> {
        if byte & 0x80 != 0 {
            if byte >= 0xF8 {
                return Some(vec![byte]);
            }
            self.status = Some(byte);
            self.len = 0;
            self.needed = status_data_len(byte);
            if self.needed == 0 {
                return Some(vec![byte]);
            }
            return None;
        }

        let status = self.status?;
        if self.len < self.data.len() {
            self.data[self.len] = byte;
        }
        self.len += 1;
        if self.len < self.needed {
            return None;
        }

        let mut message = Vec::with_capacity(1 + self.needed);
        message.push(status);
        message.extend_from_slice(&self.data[..self.needed]);
        self.len = 0;
        if status >= 0xF0 {
            self.status = None;
            self.needed = 0;
        }
        Some(message)
    }
}

fn status_data_len(status: u8) -> usize {
    match status {
        0x80..=0x8F | 0x90..=0x9F | 0xA0..=0xAF | 0xB0..=0xBF | 0xE0..=0xEF => 2,
        0xC0..=0xCF | 0xD0..=0xDF => 1,
        0xF1 | 0xF3 => 1,
        0xF2 => 2,
        _ => 0,
    }
}

#[derive(Debug, Clone, Copy)]
pub struct HwOptions {
    pub exclusive: bool,
    pub period_frames: usize,
    pub nperiods: usize,
    pub ignore_hwbuf: bool,
    pub sync_mode: bool,
    pub input_latency_frames: usize,
    pub output_latency_frames: usize,
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

pub struct HwDriver {
    capture: PCM,
    playback: PCM,
    audio_ins: Vec<Arc<AudioIO>>,
    audio_outs: Vec<Arc<AudioIO>>,
    output_gain_linear: f32,
    output_balance: f32,
    sample_rate: usize,
    period_frames: usize,
    channels_in: usize,
    channels_out: usize,
    nperiods: usize,
    sync_mode: bool,
    input_latency_frames: usize,
    output_latency_frames: usize,
    capture_format: SampleFormat,
    playback_format: SampleFormat,
    capture_buffer_i16: Vec<i16>,
    capture_buffer_i32: Vec<i32>,
    playback_buffer_i16: Vec<i16>,
    playback_buffer_i32: Vec<i32>,
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

impl HwDriver {
    pub fn new_with_options(
        device: &str,
        rate: i32,
        _bits: i32,
        options: HwOptions,
    ) -> Result<Self, String> {
        let capture = PCM::new(device, Direction::Capture, false)
            .map_err(|e| format!("Failed to open ALSA capture '{device}': {e}"))?;
        let playback = PCM::new(device, Direction::Playback, false)
            .map_err(|e| format!("Failed to open ALSA playback '{device}': {e}"))?;

        let period = options.period_frames.max(1);
        let nperiods = options.nperiods.max(1);
        let buffer_frames = period.saturating_mul(nperiods);

        let capture_target = desired_channels(&capture, rate as usize, period, buffer_frames);
        let playback_target = desired_channels(&playback, rate as usize, period, buffer_frames);

        let (channels_in, capture_format) = configure_pcm(
            &capture,
            rate as usize,
            capture_target,
            period,
            buffer_frames,
        )?;
        let (channels_out, playback_format) = configure_pcm(
            &playback,
            rate as usize,
            playback_target,
            period,
            buffer_frames,
        )?;

        let actual_rate = capture
            .hw_params_current()
            .map_err(|e| e.to_string())?
            .get_rate()
            .map_err(|e| e.to_string())?;

        let sample_rate = actual_rate as usize;
        let audio_ins: Vec<Arc<AudioIO>> = (0..channels_in)
            .map(|_| Arc::new(AudioIO::new(period)))
            .collect();
        let audio_outs: Vec<Arc<AudioIO>> = (0..channels_out)
            .map(|_| Arc::new(AudioIO::new(period)))
            .collect();

        Ok(Self {
            capture,
            playback,
            audio_ins,
            audio_outs,
            output_gain_linear: 1.0,
            output_balance: 0.0,
            sample_rate,
            period_frames: period,
            channels_in,
            channels_out,
            nperiods,
            sync_mode: options.sync_mode,
            input_latency_frames: options.input_latency_frames,
            output_latency_frames: options.output_latency_frames,
            capture_format,
            playback_format,
            capture_buffer_i16: vec![0; period * channels_in],
            capture_buffer_i32: vec![0; period * channels_in],
            playback_buffer_i16: vec![0; period * channels_out],
            playback_buffer_i32: vec![0; period * channels_out],
        })
    }

    pub fn input_channels(&self) -> usize {
        self.channels_in
    }

    pub fn output_channels(&self) -> usize {
        self.channels_out
    }

    pub fn sample_rate(&self) -> i32 {
        self.sample_rate as i32
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
        let ch_count = self.audio_outs.len();
        let b = if ch_count == 2 {
            balance.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        let mut out = Vec::with_capacity(ch_count);
        for (channel_idx, channel) in self.audio_outs.iter().enumerate() {
            let balance_gain = if ch_count == 2 {
                if channel_idx == 0 {
                    (1.0 - b).clamp(0.0, 1.0)
                } else {
                    (1.0 + b).clamp(0.0, 1.0)
                }
            } else {
                1.0
            };
            let buf = channel.buffer.lock();
            let mut peak = 0.0_f32;
            for &sample in buf.iter() {
                let v = if sample >= 0.0 { sample } else { -sample };
                if v > peak {
                    peak = v;
                }
            }
            let peak = peak * gain * balance_gain;
            let meter = if peak <= 1.0e-6 {
                -90.0
            } else {
                (20.0 * peak.log10()).clamp(-90.0, 20.0)
            };
            out.push(meter);
        }
        out
    }

    pub fn run_cycle(&mut self) -> Result<(), String> {
        let frames = self.period_frames;
        match self.capture_format {
            SampleFormat::S16 => {
                let in_io = self
                    .capture
                    .io_i16()
                    .map_err(|e| format!("ALSA capture io error: {e}"))?;
                if let Err(e) = in_io.readi(&mut self.capture_buffer_i16) {
                    if self.capture.state() == State::XRun {
                        let _ = self.capture.prepare();
                    }
                    return Err(format!("ALSA capture read failed: {e}"));
                }
            }
            SampleFormat::S32 => {
                let in_io = self
                    .capture
                    .io_i32()
                    .map_err(|e| format!("ALSA capture io error: {e}"))?;
                if let Err(e) = in_io.readi(&mut self.capture_buffer_i32) {
                    if self.capture.state() == State::XRun {
                        let _ = self.capture.prepare();
                    }
                    return Err(format!("ALSA capture read failed: {e}"));
                }
            }
        }

        match self.capture_format {
            SampleFormat::S16 => {
                for (ch, io) in self.audio_ins.iter().enumerate() {
                    let dst = io.buffer.lock();
                    for frame in 0..frames {
                        let idx = frame * self.channels_in + ch;
                        let sample = self.capture_buffer_i16.get(idx).copied().unwrap_or(0);
                        dst[frame] = (sample as f32) / 32768.0;
                    }
                    *io.finished.lock() = true;
                }
            }
            SampleFormat::S32 => {
                for (ch, io) in self.audio_ins.iter().enumerate() {
                    let dst = io.buffer.lock();
                    for frame in 0..frames {
                        let idx = frame * self.channels_in + ch;
                        let sample = self.capture_buffer_i32.get(idx).copied().unwrap_or(0);
                        dst[frame] = (sample as f32) / 2147483648.0;
                    }
                    *io.finished.lock() = true;
                }
            }
        }

        let gain = self.output_gain_linear;
        let balance = self.output_balance;
        let stereo = self.audio_outs.len() == 2;
        let left_gain = if stereo {
            (1.0 - balance).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let right_gain = if stereo {
            (1.0 + balance).clamp(0.0, 1.0)
        } else {
            1.0
        };

        match self.playback_format {
            SampleFormat::S16 => {
                for (ch, io) in self.audio_outs.iter().enumerate() {
                    io.process();
                    let src = io.buffer.lock();
                    let balance_gain = if stereo {
                        if ch == 0 { left_gain } else { right_gain }
                    } else {
                        1.0
                    };
                    for frame in 0..frames {
                        let idx = frame * self.channels_out + ch;
                        let sample = src.get(frame).copied().unwrap_or(0.0) * gain * balance_gain;
                        let v = (sample.clamp(-1.0, 1.0) * 32767.0) as i16;
                        if let Some(dst) = self.playback_buffer_i16.get_mut(idx) {
                            *dst = v;
                        }
                    }
                }
                let out_io = self
                    .playback
                    .io_i16()
                    .map_err(|e| format!("ALSA playback io error: {e}"))?;
                if let Err(e) = out_io.writei(&self.playback_buffer_i16) {
                    if self.playback.state() == State::XRun {
                        let _ = self.playback.prepare();
                    }
                    return Err(format!("ALSA playback write failed: {e}"));
                }
            }
            SampleFormat::S32 => {
                for (ch, io) in self.audio_outs.iter().enumerate() {
                    io.process();
                    let src = io.buffer.lock();
                    let balance_gain = if stereo {
                        if ch == 0 { left_gain } else { right_gain }
                    } else {
                        1.0
                    };
                    for frame in 0..frames {
                        let idx = frame * self.channels_out + ch;
                        let sample = src.get(frame).copied().unwrap_or(0.0) * gain * balance_gain;
                        let v = (sample.clamp(-1.0, 1.0) * 2147483647.0) as i32;
                        if let Some(dst) = self.playback_buffer_i32.get_mut(idx) {
                            *dst = v;
                        }
                    }
                }
                let out_io = self
                    .playback
                    .io_i32()
                    .map_err(|e| format!("ALSA playback io error: {e}"))?;
                if let Err(e) = out_io.writei(&self.playback_buffer_i32) {
                    if self.playback.state() == State::XRun {
                        let _ = self.playback.prepare();
                    }
                    return Err(format!("ALSA playback write failed: {e}"));
                }
            }
        }

        Ok(())
    }

    pub fn latency_ranges(&self) -> ((usize, usize), (usize, usize)) {
        let period = self.cycle_samples();
        let input = (period / 2) + self.input_latency_frames;
        let mut output = (period / 2) + self.output_latency_frames;
        output += self.nperiods * period;
        if !self.sync_mode {
            output += period;
        }
        ((input, input), (output, output))
    }

    pub fn channel(&mut self) -> OSSChannel<'_> {
        OSSChannel { driver: self }
    }
}

pub struct OSSChannel<'a> {
    driver: &'a mut HwDriver,
}

impl<'a> OSSChannel<'a> {
    pub fn run_cycle(&mut self) -> std::io::Result<()> {
        self.driver
            .run_cycle()
            .map_err(|e| std::io::Error::other(e))
    }

    pub fn run_assist_step(&mut self) -> std::io::Result<bool> {
        Ok(false)
    }
}

fn desired_channels(pcm: &PCM, rate: usize, period_frames: usize, buffer_frames: usize) -> usize {
    let _ = (rate, period_frames, buffer_frames);
    let Ok(hwp) = HwParams::any(pcm) else {
        return 2;
    };
    if hwp.set_access(Access::RWInterleaved).is_err() {
        return 2;
    }
    hwp.get_channels_max().map(|v| v as usize).unwrap_or(2).max(1)
}

fn configure_pcm(
    pcm: &PCM,
    rate: usize,
    channels: usize,
    period_frames: usize,
    buffer_frames: usize,
) -> Result<(usize, SampleFormat), String> {
    let hwp = HwParams::any(pcm).map_err(|e| e.to_string())?;
    hwp.set_access(Access::RWInterleaved)
        .map_err(|e| e.to_string())?;
    let format = choose_best_format(&hwp)?;
    let target = (channels.max(1)) as u32;
    let _chosen_channels = match hwp.set_channels_near(target) {
        Ok(v) if v > 0 => v,
        _ => {
            hwp.set_channels(2)
                .map_err(|e| e.to_string())?;
            2
        }
    };
    hwp.set_rate(rate as u32, ValueOr::Nearest)
        .map_err(|e| e.to_string())?;
    let _actual_period = hwp
        .set_period_size_near(period_frames as i64, ValueOr::Nearest)
        .map_err(|e| e.to_string())?;
    let _actual_buffer = hwp
        .set_buffer_size_near(buffer_frames as i64)
        .map_err(|e| e.to_string())?;
    pcm.hw_params(&hwp).map_err(|e| e.to_string())?;

    let swp = pcm.sw_params_current().map_err(|e| e.to_string())?;
    let cur = pcm.hw_params_current().map_err(|e| e.to_string())?;
    let actual_buffer = cur.get_buffer_size().map_err(|e| e.to_string())?;
    let actual_period = cur.get_period_size().map_err(|e| e.to_string())?;
    let start_threshold = actual_buffer.saturating_sub(actual_period) as u32;
    swp.set_start_threshold(start_threshold as i64)
        .map_err(|e| e.to_string())?;
    swp.set_avail_min(actual_period as i64)
        .map_err(|e| e.to_string())?;
    pcm.sw_params(&swp).map_err(|e| e.to_string())?;
    pcm.prepare().map_err(|e| e.to_string())?;

    let actual_channels = pcm
        .hw_params_current()
        .map_err(|e| e.to_string())?
        .get_channels()
        .map_err(|e| e.to_string())? as usize;

    Ok((actual_channels.max(1), format))
}

#[derive(Debug, Clone, Copy)]
enum SampleFormat {
    S16,
    S32,
}

fn choose_best_format(hwp: &HwParams<'_>) -> Result<SampleFormat, String> {
    match hwp.set_format(Format::s32()) {
        Ok(()) => return Ok(SampleFormat::S32),
        Err(e32) => {
            match hwp.set_format(Format::s16()) {
                Ok(()) => return Ok(SampleFormat::S16),
                Err(e16) => {
                    return Err(format!(
                        "No supported integer PCM format (s32/s16). set s32 error: {e32}; set s16 error: {e16}.",
                    ));
                }
            }
        }
    }
}
