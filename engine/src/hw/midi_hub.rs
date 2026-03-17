#![allow(dead_code)]

use crate::message::HwMidiEvent;
use crate::midi::io::MidiEvent;
use nix::libc;
use std::{
    fs::File,
    io::{ErrorKind, Read, Write},
    os::unix::fs::OpenOptionsExt,
    thread,
    time::{Duration, Instant},
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

    pub fn read_events(&mut self) -> Vec<HwMidiEvent> {
        let mut events = Vec::with_capacity(32);
        self.read_events_into(&mut events);
        events
    }

    pub fn read_events_into(&mut self, out: &mut Vec<HwMidiEvent>) {
        out.clear();
        for input in &mut self.inputs {
            input.read_events_into(out);
        }
    }

    pub fn write_events(&mut self, events: &[HwMidiEvent]) {
        if events.is_empty() {
            return;
        }
        for output in &mut self.outputs {
            output.write_events(events);
        }
    }

    pub fn write_events_blocking(&mut self, events: &[HwMidiEvent], timeout: Duration) {
        if events.is_empty() {
            return;
        }
        for output in &mut self.outputs {
            output.write_events_blocking(events, timeout);
        }
    }

    pub fn output_devices(&self) -> Vec<String> {
        self.outputs
            .iter()
            .map(|output| output.path.clone())
            .collect()
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

    fn write_events(&mut self, events: &[HwMidiEvent]) {
        for event in events {
            if event.device != self.path {
                continue;
            }
            let midi_event = &event.event;
            if midi_event.data.is_empty() {
                continue;
            }
            if let Err(err) = self.file.write_all(&midi_event.data) {
                if err.kind() != ErrorKind::WouldBlock {
                    error!("MIDI write error on {}: {}", self.path, err);
                }
                break;
            }
        }
    }

    fn write_events_blocking(&mut self, events: &[HwMidiEvent], timeout: Duration) {
        for event in events {
            if event.device != self.path {
                continue;
            }
            let midi_event = &event.event;
            if midi_event.data.is_empty() {
                continue;
            }
            let deadline = Instant::now() + timeout;
            loop {
                match self.file.write_all(&midi_event.data) {
                    Ok(()) => break,
                    Err(err)
                        if err.kind() == ErrorKind::WouldBlock && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(1));
                    }
                    Err(err) => {
                        error!("Blocking MIDI write error on {}: {}", self.path, err);
                        break;
                    }
                }
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

    fn read_events_into(&mut self, out: &mut Vec<HwMidiEvent>) {
        let mut buf = [0_u8; 256];
        loop {
            match self.file.read(&mut buf) {
                Ok(0) => break,
                Ok(read) => {
                    for byte in &buf[..read] {
                        if let Some(data) = self.parser.feed(*byte) {
                            out.push(HwMidiEvent {
                                device: self.path.clone(),
                                event: MidiEvent::new(0, data),
                            });
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
    in_sysex: bool,
    sysex: Vec<u8>,
}

impl MidiParser {
    fn feed(&mut self, byte: u8) -> Option<Vec<u8>> {
        if byte & 0x80 != 0 {
            if self.in_sysex {
                if byte == 0xF7 {
                    self.sysex.push(byte);
                    self.in_sysex = false;
                    return Some(std::mem::take(&mut self.sysex));
                }
                // Realtime can be interleaved in SysEx without ending it.
                if byte >= 0xF8 {
                    return Some(vec![byte]);
                }
                // Any other status interrupts an unterminated SysEx.
                self.in_sysex = false;
                self.sysex.clear();
            }
            if byte >= 0xF8 {
                return Some(vec![byte]);
            }
            if byte == 0xF0 {
                self.in_sysex = true;
                self.sysex.clear();
                self.sysex.push(byte);
                self.status = None;
                self.needed = 0;
                self.len = 0;
                return None;
            }
            self.status = Some(byte);
            self.len = 0;
            self.needed = status_data_len(byte);
            if self.needed == 0 {
                return Some(vec![byte]);
            }
            return None;
        }

        if self.in_sysex {
            self.sysex.push(byte);
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
        0xC0..=0xDF => 1,
        0xF1 | 0xF3 => 1,
        0xF2 => 2,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::MidiParser;

    #[test]
    fn parser_collects_sysex_message() {
        let mut parser = MidiParser::default();
        let bytes = [0xF0, 0x7D, 0x01, 0x02, 0xF7];
        let mut out = Vec::new();
        for b in bytes {
            if let Some(msg) = parser.feed(b) {
                out.push(msg);
            }
        }
        assert_eq!(out, vec![vec![0xF0, 0x7D, 0x01, 0x02, 0xF7]]);
    }

    #[test]
    fn parser_keeps_realtime_while_in_sysex() {
        let mut parser = MidiParser::default();
        let bytes = [0xF0, 0x7D, 0xF8, 0x01, 0xF7];
        let mut out = Vec::new();
        for b in bytes {
            if let Some(msg) = parser.feed(b) {
                out.push(msg);
            }
        }
        assert_eq!(out, vec![vec![0xF8], vec![0xF0, 0x7D, 0x01, 0xF7]]);
    }
}
