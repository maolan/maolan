#![allow(dead_code)]

use crate::{audio::io::AudioIO, midi::io::MidiEvent};
use nix::libc;
use std::{
    collections::HashMap,
    fs::File,
    io::{ErrorKind, Read, Write},
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    sync::{Arc, Mutex, OnceLock, Weak},
};
use tracing::error;
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

#[cfg(target_endian = "little")]
pub const AFMT_S16_NE: u32 = AFMT_S16_LE;
#[cfg(target_endian = "big")]
pub const AFMT_S16_NE: u32 = AFMT_S16_BE;
#[cfg(target_endian = "little")]
pub const AFMT_S24_NE: u32 = AFMT_S24_LE;
#[cfg(target_endian = "big")]
pub const AFMT_S24_NE: u32 = AFMT_S24_BE;
#[cfg(target_endian = "little")]
pub const AFMT_S32_NE: u32 = AFMT_S32_LE;
#[cfg(target_endian = "big")]
pub const AFMT_S32_NE: u32 = AFMT_S32_BE;

pub const PCM_ENABLE_INPUT: i32 = 0x00000001;
pub const PCM_ENABLE_OUTPUT: i32 = 0x00000002;

const PCM_CAP_TRIGGER: i32 = 0x00001000;
const PCM_CAP_MMAP: i32 = 0x00002000;

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

#[derive(Debug, Clone, Default)]
struct Buffer {
    data: Vec<u8>,
    pos: usize,
}

impl Buffer {
    fn with_size(size: usize) -> Self {
        Self {
            data: vec![0_u8; size],
            pos: 0,
        }
    }

    fn len(&self) -> usize {
        self.data.len()
    }

    fn progress(&self) -> usize {
        self.pos
    }

    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    fn done(&self) -> bool {
        self.pos >= self.data.len()
    }

    fn reset(&mut self) {
        self.pos = 0;
    }

    fn clear(&mut self) {
        self.data.fill(0);
        self.pos = 0;
    }

    fn advance(&mut self, bytes: usize) -> usize {
        let n = bytes.min(self.remaining());
        self.pos += n;
        n
    }

    fn rewind(&mut self, bytes: usize) -> usize {
        let n = bytes.min(self.pos);
        self.pos -= n;
        n
    }

    fn position(&mut self) -> &mut [u8] {
        let pos = self.pos;
        &mut self.data[pos..]
    }

    fn as_slice(&self) -> &[u8] {
        &self.data
    }

    fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.data
    }
}

#[derive(Debug, Clone, Copy)]
struct Correction {
    loss_min: i64,
    loss_max: i64,
    drift_min: i64,
    drift_max: i64,
    correction: i64,
}

impl Default for Correction {
    fn default() -> Self {
        Self {
            loss_min: -128,
            loss_max: 128,
            drift_min: -64,
            drift_max: 64,
            correction: 0,
        }
    }
}

impl Correction {
    fn set_drift_limits(&mut self, min: i64, max: i64) {
        self.drift_min = min.min(max);
        self.drift_max = min.max(max);
    }

    fn set_loss_limits(&mut self, min: i64, max: i64) {
        self.loss_min = min.min(max);
        self.loss_max = min.max(max);
    }

    fn clear(&mut self) {
        self.correction = 0;
    }

    fn correction(&self) -> i64 {
        self.correction
    }

    fn correct(&mut self, balance: i64, target: i64) -> i64 {
        let corrected = balance - target + self.correction;
        if corrected > self.loss_max {
            self.correction -= corrected - self.loss_max;
        } else if corrected < self.loss_min {
            self.correction += self.loss_min - corrected;
        } else if corrected > self.drift_max {
            self.correction -= 1;
        } else if corrected < self.drift_min {
            self.correction += 1;
        }
        self.correction
    }
}

#[derive(Debug, Clone, Copy)]
struct DuplexSync {
    correction: Correction,
    capture_balance: Option<i64>,
    playback_balance: Option<i64>,
    cycle_end: i64,
    playback_prefill_frames: i64,
    clock_zero: Option<libc::timespec>,
}

impl DuplexSync {
    fn new(sample_rate: i32, buffer_frames: usize) -> Self {
        let mut correction = Correction::default();
        let drift_limit = (sample_rate as i64 / 1000).max(1);
        correction.set_drift_limits(-drift_limit, drift_limit);
        let loss_limit = drift_limit.max(buffer_frames as i64 / 2);
        correction.set_loss_limits(-loss_limit, loss_limit);
        Self {
            correction,
            capture_balance: None,
            playback_balance: None,
            cycle_end: 0,
            playback_prefill_frames: 0,
            clock_zero: None,
        }
    }
}

fn duplex_registry() -> &'static Mutex<HashMap<String, Weak<Mutex<DuplexSync>>>> {
    static REG: OnceLock<Mutex<HashMap<String, Weak<Mutex<DuplexSync>>>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

fn get_or_create_duplex_sync(path: &str, sample_rate: i32, buffer_frames: usize) -> Arc<Mutex<DuplexSync>> {
    let reg = duplex_registry();
    let mut map = reg.lock().expect("duplex registry poisoned");
    if let Some(existing) = map.get(path).and_then(Weak::upgrade) {
        return existing;
    }
    let created = Arc::new(Mutex::new(DuplexSync::new(sample_rate, buffer_frames)));
    map.insert(path.to_string(), Arc::downgrade(&created));
    created
}

#[derive(Debug, Clone, Copy)]
struct FrameClock {
    zero: libc::timespec,
    sample_rate: u32,
}

impl Default for FrameClock {
    fn default() -> Self {
        Self {
            zero: libc::timespec {
                tv_sec: 0,
                tv_nsec: 0,
            },
            sample_rate: 48_000,
        }
    }
}

impl FrameClock {
    fn set_sample_rate(&mut self, sample_rate: u32) -> bool {
        if sample_rate == 0 {
            return false;
        }
        self.sample_rate = sample_rate;
        true
    }

    fn init_clock(&mut self, sample_rate: u32) -> bool {
        if !self.set_sample_rate(sample_rate) {
            return false;
        }
        unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut self.zero) == 0 }
    }

    fn now(&self) -> Option<i64> {
        let mut now = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        let ok = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut now) == 0 };
        if !ok {
            return None;
        }
        let ns = (now.tv_sec - self.zero.tv_sec) as i128 * 1_000_000_000_i128
            + (now.tv_nsec - self.zero.tv_nsec) as i128;
        Some(((ns * self.sample_rate as i128) / 1_000_000_000_i128) as i64)
    }

    fn sleep_until_frame(&self, frame: i64) -> bool {
        let ns = self.frames_to_time(frame);
        let wake = libc::timespec {
            tv_sec: self.zero.tv_sec + (self.zero.tv_nsec + ns) / 1_000_000_000,
            tv_nsec: (self.zero.tv_nsec + ns) % 1_000_000_000,
        };
        unsafe {
            libc::clock_nanosleep(libc::CLOCK_MONOTONIC, libc::TIMER_ABSTIME, &wake, std::ptr::null_mut())
                == 0
        }
    }

    fn frames_to_time(&self, frames: i64) -> i64 {
        (frames.saturating_mul(1_000_000_000_i64)) / self.sample_rate as i64
    }

    fn stepping(&self) -> i64 {
        16_i64 * (1 + (self.sample_rate as i64 / 50_000))
    }
}

#[derive(Debug, Clone, Copy)]
struct ChannelState {
    last_processing: i64,
    last_sync: i64,
    last_progress: i64,
    balance: i64,
    min_progress: i64,
    max_progress: i64,
    total_loss: i64,
    sync_level: u32,
}

impl Default for ChannelState {
    fn default() -> Self {
        Self {
            last_processing: 0,
            last_sync: 0,
            last_progress: 0,
            balance: 0,
            min_progress: 0,
            max_progress: 0,
            total_loss: 0,
            sync_level: 8,
        }
    }
}

impl ChannelState {
    fn freewheel(&self) -> bool {
        self.sync_level > 4
    }

    fn full_resync(&self) -> bool {
        self.sync_level > 2
    }

    fn resync(&self) -> bool {
        self.sync_level > 0
    }

    fn mark_progress(&mut self, progress: i64, now: i64, stepping: i64) {
        if progress > 0 {
            if self.freewheel() {
                self.last_progress = now - progress - self.balance;
                if now <= self.last_processing + stepping {
                    self.sync_level = self.sync_level.saturating_sub(1);
                }
            } else if now <= self.last_processing + stepping {
                self.balance = now - (self.last_progress + progress);
                self.last_sync = now;
                if self.sync_level > 0 {
                    self.sync_level -= 1;
                }
                if progress < self.min_progress || self.min_progress == 0 {
                    self.min_progress = progress;
                }
                if progress > self.max_progress {
                    self.max_progress = progress;
                }
            } else {
                self.sync_level += 1;
            }
            self.last_progress += progress;
        }
        self.last_processing = now;
    }

    fn mark_loss_from(&mut self, progress: i64, now: i64) -> i64 {
        let loss = (now - self.balance) - (self.last_progress + progress);
        self.mark_loss(loss)
    }

    fn mark_loss(&mut self, loss: i64) -> i64 {
        if loss > 0 {
            self.total_loss += loss;
            self.sync_level = self.sync_level.max(6);
            loss
        } else {
            0
        }
    }

    fn next_min_progress(&self) -> i64 {
        self.last_progress + self.min_progress + self.balance
    }

    fn safe_wakeup(&self, oss_available: i64, buffer_frames: i64) -> i64 {
        self.next_min_progress() + buffer_frames - oss_available - self.max_progress
    }

    fn estimated_dropout(&self, oss_available: i64, buffer_frames: i64) -> i64 {
        self.last_progress + self.balance + buffer_frames - oss_available
    }

    fn wakeup_time(
        &self,
        sync_target: i64,
        oss_available: i64,
        buffer_frames: i64,
        stepping: i64,
    ) -> i64 {
        let mut wakeup = self.last_processing + stepping;
        if self.freewheel() || self.full_resync() {
            // keep small steps
        } else if self.resync() || wakeup + self.max_progress > sync_target {
            if self.next_min_progress() > wakeup {
                wakeup = self.next_min_progress() - stepping;
            } else if self.next_min_progress() > self.last_processing {
                wakeup = self.next_min_progress();
            }
        } else {
            wakeup = sync_target - self.max_progress;
        }

        if sync_target > self.last_processing && sync_target < wakeup {
            wakeup = sync_target;
        }

        let safe = self.safe_wakeup(oss_available, buffer_frames);
        if safe < wakeup {
            wakeup = safe.max(self.last_processing + stepping);
        }
        wakeup
    }
}

#[derive(Debug, Clone)]
struct BufferRecord {
    buffer: Buffer,
    end_frames: i64,
}

impl BufferRecord {
    fn empty() -> Self {
        Self {
            buffer: Buffer::with_size(0),
            end_frames: 0,
        }
    }

    fn valid(&self) -> bool {
        self.buffer.len() > 0
    }
}

#[derive(Debug)]
struct ReadChannel {
    st: ChannelState,
    map_progress: i64,
    read_position: i64,
}

impl Default for ReadChannel {
    fn default() -> Self {
        Self {
            st: ChannelState::default(),
            map_progress: 0,
            read_position: 0,
        }
    }
}

#[derive(Debug)]
struct WriteChannel {
    st: ChannelState,
    map_progress: i64,
    write_position: i64,
}

impl Default for WriteChannel {
    fn default() -> Self {
        Self {
            st: ChannelState::default(),
            map_progress: 0,
            write_position: 0,
        }
    }
}

#[derive(Debug)]
enum ChannelKind {
    Read(ReadChannel),
    Write(WriteChannel),
}

#[derive(Debug)]
struct DoubleBufferedChannel {
    kind: ChannelKind,
    buffer_a: BufferRecord,
    buffer_b: BufferRecord,
}

impl DoubleBufferedChannel {
    fn new_read(buffer_bytes: usize, frames: i64) -> Self {
        let mut s = Self {
            kind: ChannelKind::Read(ReadChannel::default()),
            buffer_a: BufferRecord::empty(),
            buffer_b: BufferRecord::empty(),
        };
        s.set_buffer(Buffer::with_size(buffer_bytes), 0);
        s.set_buffer(Buffer::with_size(buffer_bytes), frames);
        s
    }

    fn new_write(buffer_bytes: usize, frames: i64) -> Self {
        let mut s = Self {
            kind: ChannelKind::Write(WriteChannel::default()),
            buffer_a: BufferRecord::empty(),
            buffer_b: BufferRecord::empty(),
        };
        s.set_buffer(Buffer::with_size(buffer_bytes), 0);
        s.set_buffer(Buffer::with_size(buffer_bytes), frames);
        s
    }

    fn set_buffer(&mut self, buffer: Buffer, end_frames: i64) -> bool {
        if !self.buffer_b.valid() {
            self.buffer_b = BufferRecord { buffer, end_frames };
            if !self.buffer_a.valid() {
                std::mem::swap(&mut self.buffer_a, &mut self.buffer_b);
            }
            return true;
        }
        false
    }

    fn take_buffer(&mut self) -> Buffer {
        std::mem::swap(&mut self.buffer_a, &mut self.buffer_b);
        std::mem::take(&mut self.buffer_b.buffer)
    }

    fn reset_buffers(&mut self, end_frames: i64, frame_size: usize) {
        if self.buffer_a.valid() {
            self.buffer_a.buffer.clear();
            self.buffer_a.end_frames = end_frames;
        }
        if self.buffer_b.valid() {
            self.buffer_b.buffer.clear();
            self.buffer_b.end_frames = end_frames + (self.buffer_b.buffer.len() / frame_size) as i64;
        }
    }

    fn end_frames(&self) -> i64 {
        if self.buffer_a.valid() {
            self.buffer_a.end_frames
        } else {
            0
        }
    }

    fn process(
        &mut self,
        audio: &mut Audio,
        now: i64,
    ) -> std::io::Result<()> {
        let now = now - (now % audio.stepping());
        match &mut self.kind {
            ChannelKind::Read(read) => {
                Self::process_read(audio, read, &mut self.buffer_a, now)?;
                if self.buffer_a.buffer.done() && self.buffer_b.valid() {
                    Self::process_read(audio, read, &mut self.buffer_b, now)?;
                }
            }
            ChannelKind::Write(write) => {
                Self::process_write(audio, write, &mut self.buffer_a, now)?;
                if self.buffer_a.buffer.done() && self.buffer_b.valid() {
                    Self::process_write(audio, write, &mut self.buffer_b, now)?;
                }
            }
        }
        Ok(())
    }

    fn process_read(
        audio: &mut Audio,
        read: &mut ReadChannel,
        rec: &mut BufferRecord,
        now: i64,
    ) -> std::io::Result<()> {
        if read.st.last_processing != now {
            if audio.mapped {
                let mut info = CountInfo::default();
                let rc = unsafe { oss_get_iptr(audio.dsp.as_raw_fd(), &mut info) };
                if rc.is_ok() {
                    if let Some(delta) = audio.update_map_progress_from_count(&info) {
                        read.map_progress += (delta / audio.frame_size()) as i64;
                    }
                    let progress = read.map_progress - read.st.last_progress;
                    let available = read.st.last_progress + progress - read.read_position;
                    let loss = read.st.mark_loss(available - audio.buffer_frames());
                    read.st.mark_progress(progress, now, audio.stepping());
                    if loss > 0 {
                        read.read_position = read.st.last_progress - audio.buffer_frames();
                    }
                }
            } else {
                let queued = audio.queued_samples() as i64;
                let overdue = now - read.st.estimated_dropout(queued, audio.buffer_frames());
                if (overdue > 0 && audio.get_rec_overruns() > 0) || overdue > read.st.max_progress {
                    let progress = audio.buffer_frames() - queued;
                    let loss = read.st.mark_loss_from(progress, now);
                    read.st.mark_progress(progress + loss, now, audio.stepping());
                    read.read_position = read.st.last_progress - audio.buffer_frames();
                } else {
                    let progress = queued - (read.st.last_progress - read.read_position);
                    read.st.mark_progress(progress, now, audio.stepping());
                    read.read_position = read.st.last_progress - queued;
                }
            }
        }

        let position = rec.end_frames - (rec.buffer.remaining() / audio.frame_size()) as i64;
        if position < read.read_position {
            let skip_frames = (read.read_position - position) as usize;
            let skip = rec.buffer.advance(skip_frames * audio.frame_size());
            if skip > 0 {
                rec.buffer.position().fill(0);
            }
        } else if position > read.read_position {
            let rewind_frames = (position - read.read_position) as usize;
            rec.buffer.rewind(rewind_frames * audio.frame_size());
        }

        if audio.mapped {
            let cur_position = rec.end_frames - (rec.buffer.remaining() / audio.frame_size()) as i64;
            let mut oldest = read.st.last_progress - audio.buffer_frames();
            if read.map_progress < audio.buffer_frames() {
                oldest = read.st.last_progress - read.map_progress;
            }
            if cur_position >= oldest && cur_position < read.st.last_progress && !rec.buffer.done() {
                let offset = (read.st.last_progress - cur_position) as usize;
                let mut len = rec.buffer.remaining().min(offset * audio.frame_size());
                let pointer = (read.map_progress as usize).saturating_sub(offset) % (audio.buffer_frames() as usize);
                len = audio.read_map(rec.buffer.position(), pointer * audio.frame_size(), len);
                rec.buffer.advance(len);
                read.read_position = rec.end_frames - (rec.buffer.remaining() / audio.frame_size()) as i64;
            }
        } else if audio.queued_samples() > 0 && !rec.buffer.done() {
            let mut bytes_read = 0_usize;
            let remaining = rec.buffer.remaining();
            audio.read_io(rec.buffer.position(), remaining, &mut bytes_read)?;
            read.read_position += (bytes_read / audio.frame_size()) as i64;
            rec.buffer.advance(bytes_read);
        }

        if read.st.freewheel() && now >= rec.end_frames + read.st.balance && !rec.buffer.done() {
            rec.buffer.position().fill(0);
            let advanced = rec.buffer.advance(rec.buffer.remaining());
            read.read_position += (advanced / audio.frame_size()) as i64;
        }

        Ok(())
    }

    fn process_write(
        audio: &mut Audio,
        write: &mut WriteChannel,
        rec: &mut BufferRecord,
        now: i64,
    ) -> std::io::Result<()> {
        if write.st.last_processing != now {
            if audio.mapped {
                let mut info = CountInfo::default();
                let rc = unsafe { oss_get_optr(audio.dsp.as_raw_fd(), &mut info) };
                if rc.is_ok() {
                    let delta = audio.update_map_progress_from_count(&info).unwrap_or(0);
                    let progress = (delta / audio.frame_size()) as i64;
                    if progress > 0 {
                        let start = (write.map_progress as usize % audio.buffer_frames() as usize) * audio.frame_size();
                        audio.write_map(None, start, (progress as usize) * audio.frame_size());
                        write.map_progress += progress;
                    }
                    let loss = write.st.mark_loss(write.st.last_progress + progress - write.write_position);
                    write.st.mark_progress(progress, now, audio.stepping());
                    if loss > 0 {
                        write.write_position = write.st.last_progress;
                    }
                }
            } else {
                let queued = audio.queued_samples() as i64;
                let overdue = now - write.st.estimated_dropout(queued, audio.buffer_frames());
                if (overdue > 0 && audio.get_play_underruns() > 0) || overdue > write.st.max_progress {
                    let progress = write.write_position - write.st.last_progress;
                    let loss = write.st.mark_loss_from(progress, now);
                    write.st.mark_progress(progress + loss, now, audio.stepping());
                    write.write_position = write.st.last_progress;
                } else {
                    let progress = (write.write_position - write.st.last_progress) - queued;
                    write.st.mark_progress(progress, now, audio.stepping());
                    write.write_position = write.st.last_progress + queued;
                }
            }
        }

        let position = rec.end_frames - (rec.buffer.remaining() / audio.frame_size()) as i64;
        if position > write.write_position {
            let rewind = rec
                .buffer
                .rewind(((position - write.write_position) as usize) * audio.frame_size());
            if rewind > 0 {
                let _ = rewind;
            }
        } else if position < write.write_position {
            rec.buffer
                .advance(((write.write_position - position) as usize) * audio.frame_size());
        }

        if audio.mapped {
            let pos = rec.end_frames - (rec.buffer.remaining() / audio.frame_size()) as i64;
            if !rec.buffer.done() && pos >= write.st.last_progress && pos < write.st.last_progress + audio.buffer_frames() {
                let offset = (pos - write.st.last_progress) as usize;
                let pointer = ((write.map_progress as usize) + offset) % audio.buffer_frames() as usize;
                let mut len = ((audio.buffer_frames() as usize).saturating_sub(offset)) * audio.frame_size();
                len = len.min(rec.buffer.remaining());
                let written = audio.write_map(Some(rec.buffer.position()), pointer * audio.frame_size(), len);
                rec.buffer.advance(written);
                write.write_position = rec.end_frames - (rec.buffer.remaining() / audio.frame_size()) as i64;
            }
        } else if audio.queued_samples() < audio.buffer_frames() as i32 && !rec.buffer.done() {
            let mut bytes_written = 0_usize;
            let remaining = rec.buffer.remaining();
            audio.write_io(rec.buffer.position(), remaining, &mut bytes_written)?;
            write.write_position += (bytes_written / audio.frame_size()) as i64;
            rec.buffer.advance(bytes_written);
        }

        if write.st.freewheel() && now >= rec.end_frames + write.st.balance && !rec.buffer.done() {
            rec.buffer.advance(rec.buffer.remaining());
        }

        Ok(())
    }

    fn wakeup_time(&self, audio: &Audio, now: i64) -> i64 {
        let sync_frames = if self.buffer_a.valid() {
            match &self.kind {
                ChannelKind::Read(read) => self.buffer_a.end_frames + read.st.balance,
                ChannelKind::Write(write) => self.buffer_a.end_frames + write.st.balance,
            }
        } else {
            i64::MAX
        };

        match &self.kind {
            ChannelKind::Read(read) => read.st.wakeup_time(
                sync_frames,
                (read.st.last_progress - read.read_position).clamp(0, audio.buffer_frames()),
                audio.buffer_frames(),
                audio.stepping(),
            ),
            ChannelKind::Write(write) => write.st.wakeup_time(
                sync_frames,
                (write.st.last_progress + audio.buffer_frames() - write.write_position)
                    .clamp(0, audio.buffer_frames()),
                audio.buffer_frames(),
                audio.stepping(),
            ),
        }
        .max(now)
    }

    fn finished(&self, now: i64) -> bool {
        if !self.buffer_a.valid() {
            return true;
        }
        match &self.kind {
            ChannelKind::Read(read) => {
                (self.buffer_a.end_frames + read.st.balance) <= now && self.buffer_a.buffer.done()
            }
            ChannelKind::Write(write) => {
                (self.buffer_a.end_frames + write.st.balance) <= now && self.buffer_a.buffer.done()
            }
        }
    }

    fn total_finished(&self, now: i64) -> bool {
        if !self.buffer_a.valid() {
            return true;
        }
        let end = if self.buffer_b.valid() {
            match &self.kind {
                ChannelKind::Read(read) => self.buffer_b.end_frames + read.st.balance,
                ChannelKind::Write(write) => self.buffer_b.end_frames + write.st.balance,
            }
        } else {
            match &self.kind {
                ChannelKind::Read(read) => self.buffer_a.end_frames + read.st.balance,
                ChannelKind::Write(write) => self.buffer_a.end_frames + write.st.balance,
            }
        };
        end <= now && self.buffer_a.buffer.done() && self.buffer_b.buffer.done()
    }

    fn total_end(&self) -> i64 {
        if !self.buffer_a.valid() {
            return 0;
        }
        if self.buffer_b.valid() {
            match &self.kind {
                ChannelKind::Read(read) => self.buffer_b.end_frames + read.st.balance,
                ChannelKind::Write(write) => self.buffer_b.end_frames + write.st.balance,
            }
        } else {
            match &self.kind {
                ChannelKind::Read(read) => self.buffer_a.end_frames + read.st.balance,
                ChannelKind::Write(write) => self.buffer_a.end_frames + write.st.balance,
            }
        }
    }

    fn balance(&self) -> i64 {
        match &self.kind {
            ChannelKind::Read(read) => read.st.balance,
            ChannelKind::Write(write) => write.st.balance,
        }
    }

    fn set_balance(&mut self, balance: i64) {
        match &mut self.kind {
            ChannelKind::Read(read) => read.st.balance = balance,
            ChannelKind::Write(write) => write.st.balance = balance,
        }
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
        options: OSSOptions,
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
        let mut format = match bits {
            32 => AFMT_S32_NE,
            24 => AFMT_S24_NE,
            16 => AFMT_S16_NE,
            8 => AFMT_S8,
            _ => AFMT_S16_NE,
        };

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
        unsafe {
            if options.exclusive {
                oss_set_cooked(dsp.as_raw_fd(), &cooked)
                    .map_err(|_| std::io::Error::last_os_error())?;
            }
            oss_set_format(dsp.as_raw_fd(), &mut format)
                .map_err(|_| std::io::Error::last_os_error())?;
            if !supported_sample_format(format) {
                return Err(std::io::Error::other(format!(
                    "Unsupported OSS sample format after setfmt: {format:#x}"
                )));
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
            oss_get_caps(dsp.as_raw_fd(), &mut caps).map_err(|_| std::io::Error::last_os_error())?;
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
            let prot = if input { libc::PROT_READ } else { libc::PROT_WRITE };
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
        if len == 0 {
            return Ok(());
        }
        let n = unsafe {
            libc::read(
                self.dsp.as_raw_fd(),
                dst.as_ptr() as *mut libc::c_void,
                len,
            )
        };
        if n >= 0 {
            *count += n as usize;
            return Ok(());
        }
        let e = std::io::Error::last_os_error();
        if e.kind() == ErrorKind::WouldBlock {
            return Ok(());
        }
        Err(e)
    }

    fn write_io(&self, src: &mut [u8], len: usize, count: &mut usize) -> std::io::Result<()> {
        if len == 0 {
            return Ok(());
        }
        let n = unsafe {
            libc::write(
                self.dsp.as_raw_fd(),
                src.as_ptr() as *const libc::c_void,
                len,
            )
        };
        if n >= 0 {
            *count += n as usize;
            return Ok(());
        }
        let e = std::io::Error::last_os_error();
        if e.kind() == ErrorKind::WouldBlock {
            return Ok(());
        }
        Err(e)
    }

    fn read_map(&self, dst: &mut [u8], mut offset: usize, mut length: usize) -> usize {
        if !self.mapped || self.map.is_null() || length == 0 || self.buffer_info.bytes <= 0 {
            return 0;
        }
        let total = self.buffer_info.bytes as usize;
        offset %= total;
        if length > total {
            length = total;
        }
        let mut copied = 0;
        while length > 0 {
            let take = (total - offset).min(length);
            unsafe {
                std::ptr::copy_nonoverlapping(
                    (self.map as *const u8).add(offset),
                    dst[copied..].as_ptr() as *mut u8,
                    take,
                );
            }
            copied += take;
            length -= take;
            offset = 0;
        }
        copied
    }

    fn write_map(&self, src: Option<&mut [u8]>, mut offset: usize, mut length: usize) -> usize {
        if !self.mapped || self.map.is_null() || length == 0 || self.buffer_info.bytes <= 0 {
            return 0;
        }
        let total = self.buffer_info.bytes as usize;
        offset %= total;
        if length > total {
            length = total;
        }
        let mut copied = 0;
        while length > 0 {
            let take = (total - offset).min(length);
            unsafe {
                if let Some(data) = src.as_ref() {
                    std::ptr::copy_nonoverlapping(
                        data[copied..].as_ptr(),
                        (self.map as *mut u8).add(offset),
                        take,
                    );
                } else {
                    std::ptr::write_bytes((self.map as *mut u8).add(offset), 0, take);
                }
            }
            copied += take;
            length -= take;
            offset = 0;
        }
        copied
    }

    fn queued_samples(&self) -> i32 {
        let mut ptr = OssCount::default();
        let req = if self.input {
            unsafe { oss_current_iptr(self.dsp.as_raw_fd(), &mut ptr) }
        } else {
            unsafe { oss_current_optr(self.dsp.as_raw_fd(), &mut ptr) }
        };
        if req.is_ok() {
            ptr.fifo_samples
        } else {
            0
        }
    }

    fn get_play_underruns(&self) -> i32 {
        let mut err = AudioErrInfo::default();
        let rc = unsafe { oss_get_error(self.dsp.as_raw_fd(), &mut err) };
        if rc.is_ok() {
            err.play_underruns
        } else {
            0
        }
    }

    fn get_rec_overruns(&self) -> i32 {
        let mut err = AudioErrInfo::default();
        let rc = unsafe { oss_get_error(self.dsp.as_raw_fd(), &mut err) };
        if rc.is_ok() {
            err.rec_overruns
        } else {
            0
        }
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
        if self.channels.iter().any(has_audio_connections) {
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
        let all_connected = self.channels.iter().all(has_audio_connections);

        if self.input {
            let norm_factor = 1.0 / i32::MAX as f32;
            let data_slice: &mut [i32] = self.buffer.as_mut();

            if all_connected {
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
                for (ch_idx, io_port) in self.channels.iter().enumerate() {
                    if !has_audio_connections(io_port) {
                        *io_port.finished.lock() = true;
                        continue;
                    }
                    let channel_buf_lock = io_port.buffer.lock();
                    let channel_samples = channel_buf_lock.as_mut();

                    for (i, sample) in channel_samples.iter_mut().enumerate().take(self.chsamples) {
                        let source_idx = i * num_channels + ch_idx;
                        *sample = data_slice[source_idx] as f32 * norm_factor;
                    }
                    *io_port.finished.lock() = true;
                }
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
            if !all_connected {
                data_i32.fill(0);
            }

            if all_connected {
                for (ch_idx, io_port) in self.channels.iter().enumerate() {
                    io_port.process();
                    let channel_buf_lock = io_port.buffer.lock();
                    let channel_samples = channel_buf_lock.as_ref();
                    let balance_gain = if num_channels == 2 {
                        if ch_idx == 0 {
                            left_balance
                        } else {
                            right_balance
                        }
                    } else {
                        1.0
                    };
                    for (i, &sample) in channel_samples.iter().enumerate().take(self.chsamples) {
                        let target_idx = i * num_channels + ch_idx;
                        data_i32[target_idx] =
                            ((sample * output_gain * balance_gain).clamp(-1.0, 1.0) * scale_factor)
                                as i32;
                    }
                }
            } else {
                for (ch_idx, io_port) in self.channels.iter().enumerate() {
                    if !has_audio_connections(io_port) {
                        continue;
                    }
                    io_port.process();
                    let channel_buf_lock = io_port.buffer.lock();
                    let channel_samples = channel_buf_lock.as_ref();
                    let balance_gain = if num_channels == 2 {
                        if ch_idx == 0 {
                            left_balance
                        } else {
                            right_balance
                        }
                    } else {
                        1.0
                    };
                    for (i, &sample) in channel_samples.iter().enumerate().take(self.chsamples) {
                        let target_idx = i * num_channels + ch_idx;
                        data_i32[target_idx] =
                            ((sample * output_gain * balance_gain).clamp(-1.0, 1.0) * scale_factor)
                                as i32;
                    }
                }
            }
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

fn bytes_per_sample(format: u32) -> Option<usize> {
    match format {
        AFMT_S16_LE | AFMT_S16_BE => Some(2),
        AFMT_S24_LE | AFMT_S24_BE => Some(3),
        AFMT_S32_LE | AFMT_S32_BE => Some(4),
        AFMT_S8 => Some(1),
        _ => None,
    }
}

fn supported_sample_format(format: u32) -> bool {
    matches!(
        format,
        AFMT_S16_LE | AFMT_S16_BE | AFMT_S24_LE | AFMT_S24_BE | AFMT_S32_LE | AFMT_S32_BE
    )
}

fn cstr_fixed_prefix<const N: usize>(buf: &[libc::c_char; N]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(N);
    let bytes: Vec<u8> = buf[..len].iter().map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn has_audio_connections(port: &Arc<AudioIO>) -> bool {
    port.connection_count
        .load(std::sync::atomic::Ordering::Relaxed)
        > 0
}

fn convert_in_to_i32_interleaved(
    format: u32,
    channels: usize,
    frames: usize,
    src: &[u8],
    dst: &mut [i32],
) {
    if format == AFMT_S32_NE {
        let samples = frames
            .saturating_mul(channels)
            .min(dst.len())
            .min(src.len() / 4);
        let bytes = samples * 4;
        unsafe {
            std::ptr::copy_nonoverlapping(src.as_ptr(), dst.as_mut_ptr().cast::<u8>(), bytes);
        }
        return;
    }
    let bps = bytes_per_sample(format).unwrap_or(4);
    let n = frames.saturating_mul(channels);
    for i in 0..n.min(dst.len()) {
        let o = i * bps;
        dst[i] = match format {
            AFMT_S16_LE => i16::from_le_bytes([src[o], src[o + 1]]) as i32 * 65536,
            AFMT_S16_BE => i16::from_be_bytes([src[o], src[o + 1]]) as i32 * 65536,
            AFMT_S24_LE => {
                let v = ((src[o + 2] as i32) << 24)
                    | ((src[o + 1] as i32) << 16)
                    | ((src[o] as i32) << 8);
                v >> 8
            }
            AFMT_S24_BE => {
                let v = ((src[o] as i32) << 24)
                    | ((src[o + 1] as i32) << 16)
                    | ((src[o + 2] as i32) << 8);
                v >> 8
            }
            AFMT_S32_LE => i32::from_le_bytes([src[o], src[o + 1], src[o + 2], src[o + 3]]),
            AFMT_S32_BE => i32::from_be_bytes([src[o], src[o + 1], src[o + 2], src[o + 3]]),
            AFMT_S8 => (src[o] as i8 as i32) << 24,
            _ => 0,
        };
    }
}

fn convert_in_to_i32_connected(
    format: u32,
    frames: usize,
    src: &[u8],
    dst: &mut [i32],
    channels: &[Arc<AudioIO>],
) {
    if channels.iter().all(has_audio_connections) {
        convert_in_to_i32_interleaved(format, channels.len(), frames, src, dst);
        return;
    }
    let bps = bytes_per_sample(format).unwrap_or(4);
    let channel_count = channels.len();
    for (ch, port) in channels.iter().enumerate() {
        if !has_audio_connections(port) {
            continue;
        }
        for frame in 0..frames {
            let i = frame * channel_count + ch;
            if i >= dst.len() {
                continue;
            }
            let o = i * bps;
            dst[i] = match format {
                AFMT_S16_LE => i16::from_le_bytes([src[o], src[o + 1]]) as i32 * 65536,
                AFMT_S16_BE => i16::from_be_bytes([src[o], src[o + 1]]) as i32 * 65536,
                AFMT_S24_LE => {
                    let v = ((src[o + 2] as i32) << 24)
                        | ((src[o + 1] as i32) << 16)
                        | ((src[o] as i32) << 8);
                    v >> 8
                }
                AFMT_S24_BE => {
                    let v = ((src[o] as i32) << 24)
                        | ((src[o + 1] as i32) << 16)
                        | ((src[o + 2] as i32) << 8);
                    v >> 8
                }
                AFMT_S32_LE => i32::from_le_bytes([src[o], src[o + 1], src[o + 2], src[o + 3]]),
                AFMT_S32_BE => i32::from_be_bytes([src[o], src[o + 1], src[o + 2], src[o + 3]]),
                AFMT_S8 => (src[o] as i8 as i32) << 24,
                _ => 0,
            };
        }
    }
}

fn convert_out_from_i32_interleaved(
    format: u32,
    channels: usize,
    frames: usize,
    src: &mut [i32],
    dst: &mut [u8],
) {
    if format == AFMT_S32_NE {
        let samples = frames
            .saturating_mul(channels)
            .min(src.len())
            .min(dst.len() / 4);
        let bytes = samples * 4;
        unsafe {
            std::ptr::copy_nonoverlapping(src.as_ptr().cast::<u8>(), dst.as_mut_ptr(), bytes);
        }
        return;
    }
    let bps = bytes_per_sample(format).unwrap_or(4);
    let n = frames.saturating_mul(channels);
    for i in 0..n.min(src.len()) {
        let o = i * bps;
        let s = src[i];
        match format {
            AFMT_S16_LE => {
                let v = (s >> 16) as i16;
                dst[o..o + 2].copy_from_slice(&v.to_le_bytes());
            }
            AFMT_S16_BE => {
                let v = (s >> 16) as i16;
                dst[o..o + 2].copy_from_slice(&v.to_be_bytes());
            }
            AFMT_S24_LE => {
                let v = s >> 8;
                dst[o] = v as u8;
                dst[o + 1] = (v >> 8) as u8;
                dst[o + 2] = (v >> 16) as u8;
            }
            AFMT_S24_BE => {
                let v = s >> 8;
                dst[o] = (v >> 16) as u8;
                dst[o + 1] = (v >> 8) as u8;
                dst[o + 2] = v as u8;
            }
            AFMT_S32_LE => {
                dst[o..o + 4].copy_from_slice(&s.to_le_bytes());
            }
            AFMT_S32_BE => {
                dst[o..o + 4].copy_from_slice(&s.to_be_bytes());
            }
            AFMT_S8 => {
                dst[o] = (s >> 24) as i8 as u8;
            }
            _ => {}
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
#[derive(Debug, Default)]
struct CountInfo {
    bytes: libc::c_int,
    blocks: libc::c_int,
    ptr: libc::c_int,
}

#[repr(C)]
#[derive(Debug, Default)]
struct OssCount {
    samples: i64,
    fifo_samples: libc::c_int,
    filler: [libc::c_int; 32],
}

#[repr(C)]
#[derive(Debug, Default)]
struct AudioErrInfo {
    play_underruns: libc::c_int,
    rec_overruns: libc::c_int,
    play_ptradjust: libc::c_uint,
    rec_ptradjust: libc::c_uint,
    play_errorcount: libc::c_int,
    rec_errorcount: libc::c_int,
    play_lasterror: libc::c_int,
    rec_lasterror: libc::c_int,
    play_errorparm: libc::c_long,
    rec_errorparm: libc::c_long,
    filler: [libc::c_int; 16],
}

#[repr(C)]
#[derive(Debug)]
struct OssSysInfo {
    product: [libc::c_char; 32],
    version: [libc::c_char; 32],
    versionnum: libc::c_int,
    options: [libc::c_char; 128],
    numaudios: libc::c_int,
    openedaudio: [libc::c_int; 8],
    numsynths: libc::c_int,
    nummidis: libc::c_int,
    numtimers: libc::c_int,
    nummixers: libc::c_int,
    openedmidi: [libc::c_int; 8],
    numcards: libc::c_int,
    numaudioengines: libc::c_int,
    license: [libc::c_char; 16],
    revision_info: [libc::c_char; 256],
    filler: [libc::c_int; 172],
}

impl Default for OssSysInfo {
    fn default() -> Self {
        Self {
            product: [0; 32],
            version: [0; 32],
            versionnum: 0,
            options: [0; 128],
            numaudios: 0,
            openedaudio: [0; 8],
            numsynths: 0,
            nummidis: 0,
            numtimers: 0,
            nummixers: 0,
            openedmidi: [0; 8],
            numcards: 0,
            numaudioengines: 0,
            license: [0; 16],
            revision_info: [0; 256],
            filler: [0; 172],
        }
    }
}

#[repr(C)]
#[derive(Debug)]
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
const SNDCTL_DSP_SETFRAGMENT: u8 = 10;
const SNDCTL_DSP_GETOSPACE: u8 = 12;
const SNDCTL_DSP_GETISPACE: u8 = 13;
const SNDCTL_DSP_GETCAPS: u8 = 15;
const SNDCTL_DSP_SETTRIGGER: u8 = 16;
const SNDCTL_DSP_GETIPTR: u8 = 17;
const SNDCTL_DSP_GETOPTR: u8 = 18;
const SNDCTL_DSP_GETERROR: u8 = 25;
const SNDCTL_DSP_SYNCGROUP: u8 = 28;
const SNDCTL_DSP_SYNCSTART: u8 = 29;
const SNDCTL_DSP_COOKEDMODE: u8 = 30;
const SNDCTL_DSP_CURRENT_IPTR: u8 = 35;
const SNDCTL_DSP_CURRENT_OPTR: u8 = 36;

nix::ioctl_readwrite!(oss_set_channels, SNDCTL_DSP_MAGIC, SNDCTL_DSP_CHANNELS, i32);
nix::ioctl_readwrite!(
    oss_set_fragment,
    SNDCTL_DSP_MAGIC,
    SNDCTL_DSP_SETFRAGMENT,
    i32
);
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
nix::ioctl_read!(oss_get_iptr, SNDCTL_DSP_MAGIC, SNDCTL_DSP_GETIPTR, CountInfo);
nix::ioctl_read!(oss_get_optr, SNDCTL_DSP_MAGIC, SNDCTL_DSP_GETOPTR, CountInfo);
nix::ioctl_read!(oss_get_error, SNDCTL_DSP_MAGIC, SNDCTL_DSP_GETERROR, AudioErrInfo);
nix::ioctl_read!(
    oss_current_iptr,
    SNDCTL_DSP_MAGIC,
    SNDCTL_DSP_CURRENT_IPTR,
    OssCount
);
nix::ioctl_read!(
    oss_current_optr,
    SNDCTL_DSP_MAGIC,
    SNDCTL_DSP_CURRENT_OPTR,
    OssCount
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
const SNDCTL_SYSINFO: u8 = 1;
nix::ioctl_readwrite!(
    oss_get_info,
    SNDCTL_INFO_MAGIC,
    SNDCTL_ENGINEINFO,
    AudioInfo
);
nix::ioctl_read!(
    oss_get_sysinfo,
    SNDCTL_INFO_MAGIC,
    SNDCTL_SYSINFO,
    OssSysInfo
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
        let _ = oss_add_sync_group(fd, &mut sync_group);
    }
    sync_group.id
}

pub fn start_sync_group(fd: i32, group: i32) -> std::io::Result<()> {
    let mut id = group;
    unsafe { oss_start_group(fd, &mut id) }
        .map(|_| ())
        .map_err(|_| std::io::Error::last_os_error())
}

#[derive(Debug)]
pub struct OSSDriver {
    capture: Audio,
    playback: Audio,
    nperiods: usize,
    sync_mode: bool,
    input_latency_frames: usize,
    output_latency_frames: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct OSSOptions {
    pub exclusive: bool,
    pub period_frames: usize,
    pub nperiods: usize,
    pub ignore_hwbuf: bool,
    pub sync_mode: bool,
    pub input_latency_frames: usize,
    pub output_latency_frames: usize,
}

impl Default for OSSOptions {
    fn default() -> Self {
        Self {
            exclusive: false,
            period_frames: 1024,
            nperiods: 1,
            ignore_hwbuf: false,
            sync_mode: false,
            input_latency_frames: 0,
            output_latency_frames: 0,
        }
    }
}

impl OSSDriver {
    pub fn new(path: &str, rate: i32, bits: i32) -> std::io::Result<Self> {
        Self::new_with_options(path, rate, bits, OSSOptions::default())
    }

    pub fn new_with_options(
        path: &str,
        rate: i32,
        bits: i32,
        options: OSSOptions,
    ) -> std::io::Result<Self> {
        let capture = Audio::new(path, rate, bits, true, options)?;
        let playback = Audio::new(path, rate, bits, false, options)?;
        let mut driver = Self {
            capture,
            playback,
            nperiods: options.nperiods.max(1),
            sync_mode: options.sync_mode,
            input_latency_frames: options.input_latency_frames,
            output_latency_frames: options.output_latency_frames,
        };
        driver.apply_playback_prefill();
        Ok(driver)
    }

    pub fn input_fd(&self) -> i32 {
        self.capture.fd()
    }

    pub fn output_fd(&self) -> i32 {
        self.playback.fd()
    }

    pub fn input_channels(&self) -> usize {
        self.capture.channels.len()
    }

    pub fn output_channels(&self) -> usize {
        self.playback.channels.len()
    }

    pub fn sample_rate(&self) -> i32 {
        self.playback.rate
    }

    pub fn cycle_samples(&self) -> usize {
        self.playback.chsamples
    }

    pub fn input_port(&self, idx: usize) -> Option<Arc<AudioIO>> {
        self.capture.channels.get(idx).cloned()
    }

    pub fn output_port(&self, idx: usize) -> Option<Arc<AudioIO>> {
        self.playback.channels.get(idx).cloned()
    }

    pub fn set_output_gain_balance(&mut self, gain: f32, balance: f32) {
        self.playback.output_gain_linear = gain;
        self.playback.output_balance = balance;
    }

    pub fn output_meter_db(&self, gain: f32, balance: f32) -> Vec<f32> {
        self.playback
            .channels
            .iter()
            .enumerate()
            .map(|(channel_idx, channel)| {
                let balance_gain = if self.playback.channels.len() == 2 {
                    let b = balance.clamp(-1.0, 1.0);
                    if channel_idx == 0 {
                        (1.0 - b).clamp(0.0, 1.0)
                    } else {
                        (1.0 + b).clamp(0.0, 1.0)
                    }
                } else {
                    1.0
                };
                let buf = channel.buffer.lock();
                let peak = buf
                    .iter()
                    .fold(0.0_f32, |acc, sample| acc.max(sample.abs()))
                    * gain
                    * balance_gain;
                if peak <= 1.0e-6 {
                    -90.0
                } else {
                    (20.0 * peak.log10()).clamp(-90.0, 20.0)
                }
            })
            .collect::<Vec<f32>>()
    }

    pub fn start_input_trigger(&self) -> std::io::Result<()> {
        self.capture.start_trigger()
    }

    pub fn start_output_trigger(&self) -> std::io::Result<()> {
        self.playback.start_trigger()
    }

    pub fn channel(&mut self) -> OSSChannel<'_> {
        OSSChannel {
            capture: &mut self.capture,
            playback: &mut self.playback,
        }
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

    fn apply_playback_prefill(&mut self) {
        let period = self.cycle_samples() as i64;
        let mut prefill = (self.nperiods as i64).saturating_mul(period);
        if !self.sync_mode {
            prefill = prefill.saturating_add(period);
        }
        let mut sync = self
            .capture
            .duplex_sync
            .lock()
            .expect("duplex sync poisoned");
        sync.playback_prefill_frames = prefill.max(0);
    }
}

pub struct OSSChannel<'a> {
    capture: &'a mut Audio,
    playback: &'a mut Audio,
}

impl<'a> OSSChannel<'a> {
    pub fn run_cycle(&mut self) -> std::io::Result<()> {
        DuplexChannelApi::new(self.capture, self.playback)?.run_cycle()
    }

    pub fn run_assist_step(&mut self) -> std::io::Result<bool> {
        let mut api = DuplexChannelApi::new(self.capture, self.playback)?;
        api.check_time_and_run()?;
        if api.all_finished() {
            return Ok(false);
        }
        api.sleep()?;
        api.check_time_and_run()?;
        Ok(true)
    }
}

struct DuplexChannelApi<'a> {
    capture: &'a mut Audio,
    playback: &'a mut Audio,
    now: i64,
}

impl<'a> DuplexChannelApi<'a> {
    fn new(capture: &'a mut Audio, playback: &'a mut Audio) -> std::io::Result<Self> {
        if !capture.input || playback.input {
            return Err(std::io::Error::other(
                "run_duplex_cycle expects (capture=input, playback=output)",
            ));
        }
        Ok(Self {
            capture,
            playback,
            now: 0,
        })
    }

    fn run_cycle(&mut self) -> std::io::Result<()> {
        let frames = self.capture.chsamples as i64;
        let mut cycle_end = self.capture.shared_cycle_end_add(frames);
        self.check_time_and_run()?;

        let xrun = self.xrun_gap();
        if xrun > 0 {
            let skip = xrun + frames;
            cycle_end = self.capture.shared_cycle_end_add(skip);
            self.capture
                .channel
                .reset_buffers(self.capture.channel.end_frames() + skip, self.capture.frame_size());
            self.playback
                .channel
                .reset_buffers(self.playback.channel.end_frames() + skip, self.playback.frame_size());
        }

        while !self.capture.channel.finished(self.now) {
            self.sleep()?;
            self.check_time_and_run()?;
        }

        let mut inbuf = self.capture.channel.take_buffer();
        if self.capture.channels.iter().any(has_audio_connections) {
            convert_in_to_i32_connected(
                self.capture.format,
                self.capture.chsamples,
                inbuf.as_slice(),
                self.capture.buffer.as_mut(),
                &self.capture.channels,
            );
        }
        inbuf.reset();
        let in_end = cycle_end + frames;
        if !self.capture.channel.set_buffer(inbuf, in_end) {
            return Err(std::io::Error::other("failed to requeue capture buffer"));
        }
        self.capture.process();

        self.check_time_and_run()?;

        while !self.playback.channel.finished(self.now) {
            self.sleep()?;
            self.check_time_and_run()?;
        }

        self.playback.process();
        let mut outbuf = self.playback.channel.take_buffer();
        convert_out_from_i32_interleaved(
            self.playback.format,
            self.playback.channels.len(),
            self.playback.chsamples,
            self.playback.buffer.as_mut(),
            outbuf.as_mut_slice(),
        );
        let mut out_end = self.capture.shared_cycle_end_get() + frames;
        out_end += self.playback.playback_prefill_frames();
        out_end += self.playback.playback_correction();
        if !self.playback.channel.set_buffer(outbuf, out_end) {
            return Err(std::io::Error::other("failed to requeue playback buffer"));
        }

        self.check_time_and_run()?;
        Ok(())
    }

    fn process_one_now(audio: &mut Audio, now: i64) -> std::io::Result<()> {
        audio.frame_stamp = now;
        let wake = audio.channel.wakeup_time(audio, now);
        let mut processed = false;
        if now >= wake && !audio.channel.total_finished(now) {
            let mut chan = std::mem::replace(
                &mut audio.channel,
                if audio.input {
                    DoubleBufferedChannel::new_read(0, 0)
                } else {
                    DoubleBufferedChannel::new_write(0, 0)
                },
            );
            let res = chan.process(audio, now);
            audio.channel = chan;
            res?;
            processed = true;
        }
        if processed {
            audio.publish_balance(audio.channel.balance());
        }
        Ok(())
    }

    fn check_time_and_run(&mut self) -> std::io::Result<()> {
        self.now = self
            .capture
            .frame_clock
            .now()
            .ok_or_else(|| std::io::Error::other("failed to read frame clock"))?;
        Self::process_one_now(self.capture, self.now)?;
        Self::process_one_now(self.playback, self.now)?;
        Ok(())
    }

    fn xrun_gap(&self) -> i64 {
        let max_end = self
            .capture
            .channel
            .total_end()
            .max(self.playback.channel.total_end());
        if max_end < self.now {
            self.now - max_end
        } else {
            0
        }
    }

    fn all_finished(&self) -> bool {
        self.capture.channel.total_finished(self.now) && self.playback.channel.total_finished(self.now)
    }

    fn sleep(&self) -> std::io::Result<()> {
        let wake = self
            .capture
            .channel
            .wakeup_time(self.capture, self.capture.frame_stamp)
            .min(
                self.playback
                    .channel
                    .wakeup_time(self.playback, self.playback.frame_stamp),
            );
        let now = self.capture.frame_stamp.max(self.playback.frame_stamp);
        if wake > now && !self.capture.frame_clock.sleep_until_frame(wake) {
            return Err(std::io::Error::other("duplex sleep failed"));
        }
        Ok(())
    }
}

unsafe impl Send for Audio {}
unsafe impl Sync for Audio {}
