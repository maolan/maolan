#![allow(dead_code)]

use crate::{audio::io::AudioIO, midi::io::MidiEvent};
use nix::libc;
use std::{
    fs::File,
    io::{ErrorKind, Read, Write},
    mem,
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    sync::Arc,
};
use wavers::Samples;

pub const AFMT_QUERY: u32 = 0x00000000;
pub const AFMT_MU_LAW: u32 = 0x00000001;
pub const AFMT_A_LAW: u32 = 0x00000002;
pub const AFMT_IMA_ADPCM: u32 = 0x00000004;
pub const AFMT_U8: u32 = 0x00000008;
pub const AFMT_S16_LE: u32 = 0x00000010;
pub const AFMT_S16_BE: u32 = 0x00000020;
pub const AFMT_S8: u32 = 0x00000040;
pub const AFMT_U16_LE: u32 = 0x00000080;
pub const AFMT_U16_BE: u32 = 0x00000100;
pub const AFMT_MPEG: u32 = 0x00000200;
pub const AFMT_AC3: u32 = 0x00000400;
pub const AFMT_S32_LE: u32 = 0x00001000;
pub const AFMT_S32_BE: u32 = 0x00002000;
pub const AFMT_U32_LE: u32 = 0x00004000;
pub const AFMT_U32_BE: u32 = 0x00008000;
pub const AFMT_S24_LE: u32 = 0x00010000;
pub const AFMT_S24_BE: u32 = 0x00020000;
pub const AFMT_U24_LE: u32 = 0x00040000;
pub const AFMT_U24_BE: u32 = 0x00080000;
pub const AFMT_STEREO: u32 = 0x10000000;
pub const AFMT_WEIRD: u32 = 0x20000000;
pub const AFMT_FULLDUPLEX: u32 = 0x80000000;

pub const AFMT_S16_NE: u32 = AFMT_S16_LE;
pub const AFMT_S32_NE: u32 = AFMT_S32_LE;

pub const PCM_ENABLE_INPUT: i32 = 0x00000001;
pub const PCM_ENABLE_OUTPUT: i32 = 0x00000002;

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
                    eprintln!("MIDI write error on {}: {}", self.path, err);
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
                    eprintln!("MIDI read error on {}: {}", self.path, err);
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
}

impl Audio {
    pub fn fd(&self) -> i32 {
        self.dsp.as_raw_fd()
    }

    pub fn start_trigger(&self) -> std::io::Result<()> {
        let trig: i32 = if self.input {
            PCM_ENABLE_INPUT
        } else {
            PCM_ENABLE_OUTPUT
        };
        unsafe { oss_set_trigger(self.dsp.as_raw_fd(), &trig) }
            .map(|_| ())
            .map_err(|_| std::io::Error::last_os_error())
    }

    pub fn new(path: &str, rate: i32, bits: i32, input: bool) -> Result<Audio, std::io::Error> {
        let mut binding = File::options();

        if input {
            binding
                .read(true)
                .write(false)
                .custom_flags(libc::O_RDONLY | libc::O_EXCL);
        } else {
            binding
                .read(false)
                .write(true)
                .custom_flags(libc::O_WRONLY | libc::O_EXCL);
        }
        let mut c = Audio {
            dsp: binding.open(path)?,
            input,
            output_gain_linear: 1.0,
            output_balance: 0.0,
            rate,
            channels: vec![],
            format: AFMT_S32_NE,
            samples: 0,
            chsamples: 0,
            buffer: Samples::new(vec![0i32; 0].into_boxed_slice()),
            audio_info: AudioInfo::new(),
            buffer_info: BufferInfo::new(),
        };
        if bits == 32 {
            c.format = AFMT_S32_NE;
        } else if bits == 16 {
            c.format = AFMT_S16_NE;
        } else if bits == 8 {
            c.format = AFMT_S8;
        } else {
            panic!("No format with {} bits", bits);
        }
        unsafe {
            let fd = c.dsp.as_raw_fd();
            let flags: i32 = 0;
            oss_get_info(fd, &mut c.audio_info).expect("Failed to get info on device");
            oss_get_caps(fd, &mut c.audio_info.caps)
                .expect("Failed to get capabilities of the device");
            oss_set_cooked(fd, &flags).expect("Failed to disable cooked mode");

            oss_set_format(fd, &mut c.format).expect("Failed to set format");
            oss_set_channels(fd, &mut c.audio_info.max_channels)
                .expect("Failed to set number of channels");
            oss_set_speed(fd, &mut c.rate).expect("Failed to set sample rate");

            if input {
                oss_input_buffer_info(fd, &mut c.buffer_info).expect("Failed to get buffer size");
            } else {
                oss_output_buffer_info(fd, &mut c.buffer_info).expect("Failed to get buffer size");
            }
        }
        if c.buffer_info.fragments < 1 {
            c.buffer_info.fragments = c.buffer_info.fragstotal;
        }
        if c.buffer_info.bytes < 1 {
            c.buffer_info.bytes = c.buffer_info.fragstotal * c.buffer_info.fragsize;
        }
        if c.buffer_info.bytes < 1 {
            panic!(
                "OSS buffer error: buffer size can not be {}",
                c.buffer_info.bytes
            );
        }
        c.samples = c.buffer_info.bytes as usize / mem::size_of::<i32>();
        c.chsamples = c.samples / c.audio_info.max_channels as usize;
        c.buffer = Samples::new(vec![0i32; c.samples].into_boxed_slice());
        c.channels.reserve(c.audio_info.max_channels as usize);
        for _ in 0..c.audio_info.max_channels {
            c.channels.push(Arc::new(AudioIO::new(c.chsamples)));
        }

        unsafe {
            let fd = c.dsp.as_raw_fd();
            let trig: i32 = if input {
                PCM_ENABLE_INPUT
            } else {
                PCM_ENABLE_OUTPUT
            };

            oss_set_trigger(fd, &trig).expect("Failed to set trigger");
        }
        Ok(c)
    }
    pub fn read(&mut self) -> std::io::Result<()> {
        let data_slice: &mut [i32] = self.buffer.as_mut();
        let bytes: &mut [u8] = unsafe {
            std::slice::from_raw_parts_mut(
                data_slice.as_mut_ptr() as *mut u8,
                std::mem::size_of_val(data_slice),
            )
        };
        self.dsp.read_exact(bytes)?;
        Ok(())
    }

    pub fn write(&mut self) -> std::io::Result<()> {
        let data_i32 = self.buffer.as_mut();
        let bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(
                data_i32.as_ptr() as *const u8,
                std::mem::size_of_val(data_i32),
            )
        };
        self.dsp.write_all(bytes)?;
        Ok(())
    }

    pub fn process(&mut self) {
        let num_channels = self.channels.len();

        if self.input {
            let norm_factor = 1.0 / i32::MAX as f32;
            let data_slice: &mut [i32] = self.buffer.as_mut();

            for (ch_idx, io_port) in self.channels.iter().enumerate() {
                let channel_buf_lock = io_port.buffer.lock();
                let channel_samples = channel_buf_lock.as_mut();

                for (i, sample) in channel_samples.iter_mut().enumerate().take(self.chsamples) {
                    let source_idx = i * num_channels + ch_idx;
                    *sample = data_slice[source_idx] as f32 * norm_factor;
                }
                *io_port.finished.lock() = true;
            }
        } else {
            let scale_factor = i32::MAX as f32;
            let output_gain = self.output_gain_linear;
            let (left_balance, right_balance) = if num_channels == 2 {
                let b = self.output_balance.clamp(-1.0, 1.0);
                ((1.0 - b).clamp(0.0, 1.0), (1.0 + b).clamp(0.0, 1.0))
            } else {
                (1.0, 1.0)
            };
            let data_i32 = self.buffer.as_mut();

            for (ch_idx, io_port) in self.channels.iter().enumerate() {
                io_port.process();
                let channel_buf_lock = io_port.buffer.lock();
                let channel_samples = channel_buf_lock.as_ref();

                for (i, &sample) in channel_samples.iter().enumerate().take(self.chsamples) {
                    let target_idx = i * num_channels + ch_idx;
                    let balance_gain = if num_channels == 2 {
                        if ch_idx == 0 {
                            left_balance
                        } else {
                            right_balance
                        }
                    } else {
                        1.0
                    };
                    data_i32[target_idx] =
                        ((sample * output_gain * balance_gain).clamp(-1.0, 1.0) * scale_factor) as i32;
                }
            }
        }
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct AudioInfo {
    pub dev: libc::c_int,
    pub name: [libc::c_char; 64],
    pub busy: libc::c_int,
    pub pid: libc::c_int,
    pub caps: libc::c_int,
    pub iformats: libc::c_int,
    pub oformats: libc::c_int,
    pub magic: libc::c_int,
    pub cmd: [libc::c_char; 64],
    pub card_number: libc::c_int,
    pub port_number: libc::c_int,
    pub mixer_dev: libc::c_int,
    pub legacy_device: libc::c_int,
    pub enabled: libc::c_int,
    pub flags: libc::c_int,
    pub min_rate: libc::c_int,
    pub max_rate: libc::c_int,
    pub min_channels: libc::c_int,
    pub max_channels: libc::c_int,
    pub binding: libc::c_int,
    pub rate_source: libc::c_int,
    pub handle: [libc::c_char; 32],
    pub nrates: libc::c_uint,
    pub rates: [libc::c_uint; 20],
    pub song_name: [libc::c_char; 64],
    pub label: [libc::c_char; 16],
    pub latency: libc::c_int,
    pub devnode: [libc::c_char; 32],
    pub next_play_engine: libc::c_int,
    pub next_rec_engine: libc::c_int,
    pub filler: [libc::c_int; 184],
}

impl AudioInfo {
    pub fn new() -> Self {
        Self {
            dev: 0,
            name: [0; 64],
            busy: 0,
            pid: 0,
            caps: 0,
            iformats: 0,
            oformats: 0,
            magic: 0,
            cmd: [0; 64],
            card_number: 0,
            port_number: 0,
            mixer_dev: 0,
            legacy_device: 0,
            enabled: 0,
            flags: 0,
            min_rate: 0,
            max_rate: 0,
            min_channels: 0,
            max_channels: 0,
            binding: 0,
            rate_source: 0,
            handle: [0; 32],
            nrates: 0,
            rates: [0; 20],
            song_name: [0; 64],
            label: [0; 16],
            latency: 0,
            devnode: [0; 32],
            next_play_engine: 0,
            next_rec_engine: 0,
            filler: [0; 184],
        }
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct BufferInfo {
    pub fragments: libc::c_int,
    pub fragstotal: libc::c_int,
    pub fragsize: libc::c_int,
    pub bytes: libc::c_int,
}

impl BufferInfo {
    pub fn new() -> BufferInfo {
        BufferInfo {
            fragments: 0,
            fragstotal: 0,
            fragsize: 0,
            bytes: 0,
        }
    }
}
#[repr(C)]
struct OssSyncGroup {
    pub id: libc::c_int,
    pub mode: libc::c_int,
    pub filler: [libc::c_int; 16],
}

impl OssSyncGroup {
    pub fn new() -> Self {
        Self {
            id: 0,
            mode: 0,
            filler: [0; 16],
        }
    }
}

const SNDCTL_DSP_MAGIC: u8 = b'P';
const SNDCTL_DSP_SPEED: u8 = 2;
const SNDCTL_DSP_SETFMT: u8 = 5;
const SNDCTL_DSP_CHANNELS: u8 = 6;
const SNDCTL_DSP_GETOSPACE: u8 = 12;
const SNDCTL_DSP_GETISPACE: u8 = 13;
const SNDCTL_DSP_GETCAPS: u8 = 15;
const SNDCTL_DSP_SETTRIGGER: u8 = 16;
const SNDCTL_DSP_SYNCGROUP: u8 = 28;
const SNDCTL_DSP_SYNCSTART: u8 = 29;
const SNDCTL_DSP_COOKEDMODE: u8 = 30;
nix::ioctl_readwrite!(oss_set_channels, SNDCTL_DSP_MAGIC, SNDCTL_DSP_CHANNELS, i32);
nix::ioctl_read!(
    oss_output_buffer_info,
    SNDCTL_DSP_MAGIC,
    SNDCTL_DSP_GETOSPACE,
    BufferInfo
);
nix::ioctl_read!(
    oss_input_buffer_info,
    SNDCTL_DSP_MAGIC,
    SNDCTL_DSP_GETISPACE,
    BufferInfo
);
nix::ioctl_read!(oss_get_caps, SNDCTL_DSP_MAGIC, SNDCTL_DSP_GETCAPS, i32);
nix::ioctl_readwrite!(oss_set_format, SNDCTL_DSP_MAGIC, SNDCTL_DSP_SETFMT, u32);
nix::ioctl_readwrite!(oss_set_speed, SNDCTL_DSP_MAGIC, SNDCTL_DSP_SPEED, i32);
nix::ioctl_write_ptr!(oss_set_cooked, SNDCTL_DSP_MAGIC, SNDCTL_DSP_COOKEDMODE, i32);
nix::ioctl_write_ptr!(
    oss_set_trigger,
    SNDCTL_DSP_MAGIC,
    SNDCTL_DSP_SETTRIGGER,
    i32
);
nix::ioctl_write_ptr!(oss_start_group, SNDCTL_DSP_MAGIC, SNDCTL_DSP_SYNCSTART, i32);
nix::ioctl_readwrite!(
    oss_add_sync_group,
    SNDCTL_DSP_MAGIC,
    SNDCTL_DSP_SYNCGROUP,
    OssSyncGroup
);

const SNDCTL_INFO_MAGIC: u8 = b'X';
const SNDCTL_ENGINEINFO: u8 = 12;
nix::ioctl_readwrite!(
    oss_get_info,
    SNDCTL_INFO_MAGIC,
    SNDCTL_ENGINEINFO,
    AudioInfo
);

pub fn add_to_sync_group(fd: i32, group: i32, input: bool) -> i32 {
    let mut sync_group = OssSyncGroup::new();
    sync_group.id = group;
    if input {
        sync_group.mode = PCM_ENABLE_INPUT;
    } else {
        sync_group.mode = PCM_ENABLE_OUTPUT;
    }
    unsafe {
        oss_add_sync_group(fd, &mut sync_group).expect("Failed to set sync group");
    }
    sync_group.id
}

pub fn start_sync_group(fd: i32, group: i32) -> std::io::Result<()> {
    let mut id = group;
    unsafe { oss_start_group(fd, &mut id) }
        .map(|_| ())
        .map_err(|_| std::io::Error::last_os_error())
}

unsafe impl Send for Audio {}
unsafe impl Sync for Audio {}
