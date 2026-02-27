use crate::audio::io::AudioIO;
use crate::hw::{common, options::HwOptions, traits};
use crate::message::HwMidiEvent;
use crate::midi::io::MidiEvent;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Host, HostId, SampleFormat, SampleRate, Stream, StreamConfig};
use midir::{Ignore, MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::error;

const MIDI_IN_PREFIX: &str = "winmidi:in:";
const MIDI_OUT_PREFIX: &str = "winmidi:out:";
const WASAPI_PREFIX: &str = "wasapi:";
const ASIO_PREFIX: &str = "asio:";

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
    _input_stream: Option<Stream>,
    _output_stream: Stream,
    input_rx: Option<Receiver<Vec<f32>>>,
    output_tx: SyncSender<Vec<f32>>,
    cycle_tick_rx: Receiver<()>,
    latest_input: Vec<f32>,
    audio_ins: Vec<Arc<AudioIO>>,
    audio_outs: Vec<Arc<AudioIO>>,
    output_gain_linear: f32,
    output_balance: f32,
    sample_rate: usize,
    period_frames: usize,
    input_channels: usize,
    output_channels: usize,
    playing: bool,
}

impl HwDriver {
    pub fn new_with_options(
        device: &str,
        rate: i32,
        _bits: i32,
        options: HwOptions,
    ) -> Result<Self, String> {
        let (host, requested_name, backend_label) = select_backend_host_and_device(device)?;

        let output_device = select_output_device(&host, requested_name)
            .ok_or_else(|| format!("No matching {backend_label} output device for '{device}'"))?;
        let output_cfg = select_f32_output_config(&output_device, rate)?;

        let sample_rate = output_cfg.sample_rate.0 as usize;
        let period_frames = options.period_frames.max(1);
        let output_channels = output_cfg.channels as usize;

        let audio_outs: Vec<Arc<AudioIO>> = (0..output_channels)
            .map(|_| Arc::new(AudioIO::new(period_frames)))
            .collect();

        let maybe_input_device = select_input_device(&host, requested_name);
        let maybe_input_cfg = maybe_input_device
            .as_ref()
            .map(|d| select_f32_input_config(d, sample_rate as i32))
            .transpose()?;

        let input_channels = maybe_input_cfg
            .as_ref()
            .map(|cfg| cfg.channels as usize)
            .unwrap_or(0);
        let audio_ins: Vec<Arc<AudioIO>> = (0..input_channels)
            .map(|_| Arc::new(AudioIO::new(period_frames)))
            .collect();

        let (output_tx, output_rx) = mpsc::sync_channel::<Vec<f32>>(8);
        let (cycle_tick_tx, cycle_tick_rx) = mpsc::sync_channel::<()>(8);

        let output_stream = {
            let mut pending = Vec::<f32>::new();
            let mut pending_idx = 0usize;
            output_device
                .build_output_stream(
                    &output_cfg,
                    move |data: &mut [f32], _| {
                        for sample in data.iter_mut() {
                            loop {
                                if pending_idx < pending.len() {
                                    *sample = pending[pending_idx];
                                    pending_idx += 1;
                                    break;
                                }
                                match output_rx.try_recv() {
                                    Ok(next) => {
                                        pending = next;
                                        pending_idx = 0;
                                    }
                                    Err(_) => {
                                        *sample = 0.0;
                                        break;
                                    }
                                }
                            }
                        }
                        let _ = cycle_tick_tx.try_send(());
                    },
                    move |e| error!("{backend_label} output stream error: {e}"),
                    None,
                )
                .map_err(|e| format!("Failed to build {backend_label} output stream: {e}"))?
        };
        output_stream
            .play()
            .map_err(|e| format!("Failed to start {backend_label} output stream: {e}"))?;

        let (input_stream, input_rx) =
            if let (Some(input_device), Some(input_cfg)) = (maybe_input_device, maybe_input_cfg) {
                let (input_tx, input_rx) = mpsc::sync_channel::<Vec<f32>>(8);
                let chunk_len = period_frames.saturating_mul(input_channels.max(1));
                let input_stream = {
                    let mut stash: Vec<f32> = Vec::with_capacity(chunk_len.saturating_mul(2));
                    input_device
                        .build_input_stream(
                            &input_cfg,
                            move |data: &[f32], _| {
                                stash.extend_from_slice(data);
                                while stash.len() >= chunk_len {
                                    let chunk: Vec<f32> = stash.drain(..chunk_len).collect();
                                    let _ = input_tx.try_send(chunk);
                                }
                            },
                            move |e| error!("{backend_label} input stream error: {e}"),
                            None,
                        )
                        .map_err(|e| format!("Failed to build {backend_label} input stream: {e}"))?
                };
                input_stream
                    .play()
                    .map_err(|e| format!("Failed to start {backend_label} input stream: {e}"))?;
                (Some(input_stream), Some(input_rx))
            } else {
                (None, None)
            };

        Ok(Self {
            _input_stream: input_stream,
            _output_stream: output_stream,
            input_rx,
            output_tx,
            cycle_tick_rx,
            latest_input: vec![0.0; period_frames.saturating_mul(input_channels.max(1))],
            audio_ins,
            audio_outs,
            output_gain_linear: 1.0,
            output_balance: 0.0,
            sample_rate,
            period_frames,
            input_channels,
            output_channels,
            playing: false,
        })
    }

    pub fn input_channels(&self) -> usize {
        self.input_channels
    }

    pub fn output_channels(&self) -> usize {
        self.output_channels
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
        common::output_meter_db(&self.audio_outs, gain, balance)
    }

    pub fn run_cycle(&mut self) -> Result<(), String> {
        self.cycle_tick_rx
            .recv_timeout(Duration::from_millis(500))
            .map_err(|_| "Timed out waiting for WASAPI callback".to_string())?;

        let input_frames = self.period_frames;
        let input_channels = self.input_channels.max(1);
        if let Some(rx) = &self.input_rx {
            while let Ok(chunk) = rx.try_recv() {
                self.latest_input = chunk;
            }
        }

        for (ch_idx, io_port) in self.audio_ins.iter().enumerate() {
            let dst = io_port.buffer.lock();
            for frame in 0..input_frames.min(dst.len()) {
                let src_idx = frame * input_channels + ch_idx;
                let sample = self.latest_input.get(src_idx).copied().unwrap_or(0.0);
                dst[frame] = sample;
            }
            *io_port.finished.lock() = true;
        }

        let frames = self.period_frames;
        let channels = self.output_channels;
        let gain = self.output_gain_linear;
        let balance = self.output_balance;
        let mut interleaved = vec![0.0_f32; frames.saturating_mul(channels)];
        if self.playing {
            for (ch_idx, io_port) in self.audio_outs.iter().enumerate() {
                io_port.process();
                let src = io_port.buffer.lock();
                let b = common::channel_balance_gain(channels, ch_idx, balance);
                for frame in 0..frames.min(src.len()) {
                    let idx = frame * channels + ch_idx;
                    interleaved[idx] = src[frame] * gain * b;
                }
            }
        }

        let _ = self.output_tx.try_send(interleaved);
        Ok(())
    }

    pub fn run_assist_step(&mut self) -> Result<bool, String> {
        Ok(false)
    }

    pub fn channel(&mut self) -> &mut Self {
        self
    }

    pub fn set_playing(&mut self, playing: bool) {
        self.playing = playing;
    }
}

unsafe impl Send for HwDriver {}

fn select_output_device(host: &cpal::Host, requested: &str) -> Option<cpal::Device> {
    if requested.eq_ignore_ascii_case("default") || requested.is_empty() {
        return host.default_output_device();
    }
    let devices = host.output_devices().ok()?;
    for dev in devices {
        if let Ok(name) = dev.name()
            && name.eq_ignore_ascii_case(requested)
        {
            return Some(dev);
        }
    }
    None
}

fn select_input_device(host: &cpal::Host, requested: &str) -> Option<cpal::Device> {
    if requested.eq_ignore_ascii_case("default") || requested.is_empty() {
        return host.default_input_device();
    }
    let devices = host.input_devices().ok()?;
    for dev in devices {
        if let Ok(name) = dev.name()
            && name.eq_ignore_ascii_case(requested)
        {
            return Some(dev);
        }
    }
    None
}

fn select_backend_host_and_device(device: &str) -> Result<(Host, &str, &'static str), String> {
    if let Some(name) = device.strip_prefix(ASIO_PREFIX) {
        let host = cpal::host_from_id(HostId::Asio)
            .map_err(|e| format!("ASIO host is not available: {e}"))?;
        return Ok((host, name.trim(), "ASIO"));
    }
    let requested = device.strip_prefix(WASAPI_PREFIX).unwrap_or(device).trim();
    let host = cpal::host_from_id(HostId::Wasapi).unwrap_or_else(|_| cpal::default_host());
    Ok((host, requested, "WASAPI"))
}

fn select_f32_output_config(
    device: &cpal::Device,
    requested_rate: i32,
) -> Result<StreamConfig, String> {
    let mut selected = None;
    let ranges = device
        .supported_output_configs()
        .map_err(|e| format!("Failed to query output stream configs: {e}"))?;
    for range in ranges {
        if range.sample_format() != SampleFormat::F32 {
            continue;
        }
        let min = range.min_sample_rate().0;
        let max = range.max_sample_rate().0;
        let rate = requested_rate.max(1) as u32;
        let cfg = if rate >= min && rate <= max {
            range.with_sample_rate(SampleRate(rate)).config()
        } else {
            range.with_max_sample_rate().config()
        };
        selected = Some(cfg);
        break;
    }
    selected.ok_or_else(|| "No F32 WASAPI output stream configuration was found".to_string())
}

fn select_f32_input_config(
    device: &cpal::Device,
    requested_rate: i32,
) -> Result<StreamConfig, String> {
    let mut selected = None;
    let ranges = device
        .supported_input_configs()
        .map_err(|e| format!("Failed to query input stream configs: {e}"))?;
    for range in ranges {
        if range.sample_format() != SampleFormat::F32 {
            continue;
        }
        let min = range.min_sample_rate().0;
        let max = range.max_sample_rate().0;
        let rate = requested_rate.max(1) as u32;
        let cfg = if rate >= min && rate <= max {
            range.with_sample_rate(SampleRate(rate)).config()
        } else {
            range.with_max_sample_rate().config()
        };
        selected = Some(cfg);
        break;
    }
    selected.ok_or_else(|| "No F32 WASAPI input stream configuration was found".to_string())
}

pub fn list_midi_input_devices() -> Vec<String> {
    let Ok(midi_in) = MidiInput::new("maolan-midi-list-in") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (idx, port) in midi_in.ports().iter().enumerate() {
        if let Ok(name) = midi_in.port_name(port) {
            out.push(format!("{MIDI_IN_PREFIX}{idx}:{name}"));
        }
    }
    out
}

pub fn list_midi_output_devices() -> Vec<String> {
    let Ok(midi_out) = MidiOutput::new("maolan-midi-list-out") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (idx, port) in midi_out.ports().iter().enumerate() {
        if let Ok(name) = midi_out.port_name(port) {
            out.push(format!("{MIDI_OUT_PREFIX}{idx}:{name}"));
        }
    }
    out
}

struct MidiInputDevice {
    device: String,
    _connection: MidiInputConnection<()>,
}

struct MidiOutputDevice {
    device: String,
    connection: MidiOutputConnection,
}

#[derive(Default)]
pub struct MidiHub {
    inputs: Vec<MidiInputDevice>,
    outputs: Vec<MidiOutputDevice>,
    input_events: Arc<Mutex<Vec<HwMidiEvent>>>,
}

impl MidiHub {
    pub fn open_input(&mut self, device: &str) -> Result<(), String> {
        if self.inputs.iter().any(|d| d.device == device) {
            return Ok(());
        }

        let index = parse_prefixed_index(device, MIDI_IN_PREFIX)?;
        let mut midi_in = MidiInput::new("maolan-midi-in")
            .map_err(|e| format!("Failed to initialize MIDI input: {e}"))?;
        midi_in.ignore(Ignore::None);
        let ports = midi_in.ports();
        let port = ports
            .get(index)
            .ok_or_else(|| format!("MIDI input device index out of range: {index}"))?
            .clone();

        let event_device = device.to_string();
        let queue = self.input_events.clone();
        let connection = midi_in
            .connect(
                &port,
                "maolan-midi-input",
                move |_stamp, data, _| {
                    if data.is_empty() {
                        return;
                    }
                    if let Ok(mut events) = queue.lock() {
                        events.push(HwMidiEvent {
                            device: event_device.clone(),
                            event: MidiEvent::new(0, data.to_vec()),
                        });
                    }
                },
                (),
            )
            .map_err(|e| format!("Failed to open MIDI input '{device}': {e}"))?;

        self.inputs.push(MidiInputDevice {
            device: device.to_string(),
            _connection: connection,
        });
        Ok(())
    }

    pub fn open_output(&mut self, device: &str) -> Result<(), String> {
        if self.outputs.iter().any(|d| d.device == device) {
            return Ok(());
        }

        let index = parse_prefixed_index(device, MIDI_OUT_PREFIX)?;
        let midi_out = MidiOutput::new("maolan-midi-out")
            .map_err(|e| format!("Failed to initialize MIDI output: {e}"))?;
        let ports = midi_out.ports();
        let port = ports
            .get(index)
            .ok_or_else(|| format!("MIDI output device index out of range: {index}"))?
            .clone();
        let connection = midi_out
            .connect(&port, "maolan-midi-output")
            .map_err(|e| format!("Failed to open MIDI output '{device}': {e}"))?;

        self.outputs.push(MidiOutputDevice {
            device: device.to_string(),
            connection,
        });
        Ok(())
    }

    pub fn read_events_into(&mut self, out: &mut Vec<HwMidiEvent>) {
        out.clear();
        let Ok(mut queue) = self.input_events.lock() else {
            return;
        };
        out.extend(queue.drain(..));
    }

    pub fn write_events(&mut self, events: &[HwMidiEvent]) {
        if events.is_empty() {
            return;
        }
        for output in &mut self.outputs {
            for event in events {
                if event.device != output.device || event.event.data.is_empty() {
                    continue;
                }
                if let Err(err) = output.connection.send(&event.event.data) {
                    error!("MIDI write error on {}: {}", output.device, err);
                    break;
                }
            }
        }
    }
}

fn parse_prefixed_index(device: &str, prefix: &str) -> Result<usize, String> {
    let rest = device
        .strip_prefix(prefix)
        .ok_or_else(|| format!("Unsupported MIDI device id '{device}'"))?;
    let index_str = rest.split(':').next().unwrap_or("");
    index_str
        .parse::<usize>()
        .map_err(|_| format!("Invalid MIDI device id '{device}'"))
}

impl traits::HwWorkerDriver for HwDriver {
    fn cycle_samples(&self) -> usize {
        self.cycle_samples()
    }

    fn sample_rate(&self) -> i32 {
        self.sample_rate()
    }

    fn run_cycle_for_worker(&mut self) -> Result<(), String> {
        self.run_cycle()
    }

    fn run_assist_step_for_worker(&mut self) -> Result<bool, String> {
        self.run_assist_step()
    }
}

impl traits::HwDevice for HwDriver {
    fn input_channels(&self) -> usize {
        self.input_channels()
    }

    fn output_channels(&self) -> usize {
        self.output_channels()
    }

    fn sample_rate(&self) -> i32 {
        self.sample_rate()
    }

    fn cycle_samples(&self) -> usize {
        self.cycle_samples()
    }

    fn input_port(&self, idx: usize) -> Option<Arc<AudioIO>> {
        self.input_port(idx)
    }

    fn output_port(&self, idx: usize) -> Option<Arc<AudioIO>> {
        self.output_port(idx)
    }

    fn set_output_gain_balance(&mut self, gain: f32, balance: f32) {
        self.set_output_gain_balance(gain, balance);
    }

    fn output_meter_db(&self, gain: f32, balance: f32) -> Vec<f32> {
        self.output_meter_db(gain, balance)
    }

    fn latency_ranges(&self) -> ((usize, usize), (usize, usize)) {
        ((0, 0), (0, 0))
    }
}

impl traits::HwMidiHub for MidiHub {
    fn read_events_into(&mut self, out: &mut Vec<HwMidiEvent>) {
        self.read_events_into(out);
    }

    fn write_events(&mut self, events: &[HwMidiEvent]) {
        self.write_events(events);
    }
}
