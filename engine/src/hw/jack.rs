use crate::{audio::io::AudioIO, midi::io::MidiEvent, mutex::UnsafeMutex};
use jack::{
    AudioIn, AudioOut, Client, ClientOptions, Control, MidiIn, MidiOut, NotificationHandler, Port,
    ProcessHandler, ProcessScope, RawMidi,
};
use std::sync::Arc;
use tokio::sync::mpsc::Sender;

#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub audio_inputs: usize,
    pub audio_outputs: usize,
    pub midi_inputs: usize,
    pub midi_outputs: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            audio_inputs: 2,
            audio_outputs: 2,
            midi_inputs: 1,
            midi_outputs: 1,
        }
    }
}

#[derive(Debug, Default)]
struct Notifications;

impl NotificationHandler for Notifications {}

struct Process {
    audio_in_ports: Vec<Port<AudioIn>>,
    audio_out_ports: Vec<Port<AudioOut>>,
    midi_in_ports: Vec<Port<MidiIn>>,
    midi_out_ports: Vec<Port<MidiOut>>,
    audio_in_bridges: Vec<Arc<AudioIO>>,
    audio_out_bridges: Vec<Arc<AudioIO>>,
    midi_in_events: Arc<UnsafeMutex<Vec<MidiEvent>>>,
    midi_out_events: Arc<UnsafeMutex<Vec<MidiEvent>>>,
    output_gain_linear: Arc<UnsafeMutex<f32>>,
    output_balance: Arc<UnsafeMutex<f32>>,
    tx_engine: Sender<crate::message::Message>,
}

impl Process {
    fn copy_audio_inputs(&mut self, ps: &ProcessScope) {
        for (idx, port) in self.audio_in_ports.iter().enumerate() {
            let Some(bridge) = self.audio_in_bridges.get(idx) else {
                continue;
            };
            let src = port.as_slice(ps);
            let dst = bridge.buffer.lock();
            let n = src.len().min(dst.len());
            for i in 0..n {
                dst[i] = src[i];
            }
            if n < dst.len() {
                for item in dst.iter_mut().skip(n) {
                    *item = 0.0;
                }
            }
            *bridge.finished.lock() = true;
        }
    }

    fn copy_audio_outputs(&mut self, ps: &ProcessScope) {
        let gain = *self.output_gain_linear.lock();
        let balance = (*self.output_balance.lock()).clamp(-1.0, 1.0);
        let stereo = self.audio_out_ports.len() == 2;
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

        for (idx, port) in self.audio_out_ports.iter_mut().enumerate() {
            let dst = port.as_mut_slice(ps);
            let Some(bridge) = self.audio_out_bridges.get(idx) else {
                dst.fill(0.0);
                continue;
            };
            bridge.process();
            let src = bridge.buffer.lock();
            let n = src.len().min(dst.len());
            let balance_gain = if stereo {
                if idx == 0 { left_gain } else { right_gain }
            } else {
                1.0
            };
            for i in 0..n {
                dst[i] = src[i] * gain * balance_gain;
            }
            if n < dst.len() {
                for item in dst.iter_mut().skip(n) {
                    *item = 0.0;
                }
            }
        }
    }

    fn collect_midi_input(&mut self, ps: &ProcessScope) {
        let out = self.midi_in_events.lock();
        out.clear();
        for port in &self.midi_in_ports {
            for raw in port.iter(ps) {
                out.push(MidiEvent::new(raw.time, raw.bytes.to_vec()));
            }
        }
    }

    fn emit_midi_output(&mut self, ps: &ProcessScope) {
        if self.midi_out_ports.is_empty() {
            self.midi_out_events.lock().clear();
            return;
        }
        let events = self.midi_out_events.lock().clone();
        self.midi_out_events.lock().clear();
        if events.is_empty() {
            return;
        }
        for out_port in &mut self.midi_out_ports {
            let mut writer = out_port.writer(ps);
            for event in &events {
                let raw = RawMidi {
                    time: event.frame,
                    bytes: &event.data,
                };
                let _ = writer.write(&raw);
            }
        }
    }
}

impl ProcessHandler for Process {
    fn process(&mut self, _client: &Client, ps: &ProcessScope) -> Control {
        self.copy_audio_inputs(ps);
        self.collect_midi_input(ps);
        self.copy_audio_outputs(ps);
        self.emit_midi_output(ps);
        let _ = self.tx_engine.try_send(crate::message::Message::HWFinished);
        Control::Continue
    }
}

pub struct JackRuntime {
    client: Option<jack::AsyncClient<Notifications, Process>>,
    pub audio_ins: Vec<Arc<AudioIO>>,
    pub audio_outs: Vec<Arc<AudioIO>>,
    midi_in_events: Arc<UnsafeMutex<Vec<MidiEvent>>>,
    midi_out_events: Arc<UnsafeMutex<Vec<MidiEvent>>>,
    output_gain_linear: Arc<UnsafeMutex<f32>>,
    output_balance: Arc<UnsafeMutex<f32>>,
    midi_input_count: usize,
    midi_output_count: usize,
    pub sample_rate: usize,
    pub buffer_size: usize,
}

impl JackRuntime {
    pub fn new(
        client_name: &str,
        config: Config,
        tx_engine: Sender<crate::message::Message>,
    ) -> Result<Self, String> {
        let (client, _status) = Client::new(client_name, ClientOptions::NO_START_SERVER)
            .map_err(|e| format!("Failed to create JACK client '{client_name}': {e}"))?;
        let sample_rate = client.sample_rate() as usize;
        let buffer_size = client.buffer_size() as usize;

        let audio_ins: Vec<Arc<AudioIO>> = (0..config.audio_inputs)
            .map(|_| Arc::new(AudioIO::new(buffer_size)))
            .collect();
        let audio_outs: Vec<Arc<AudioIO>> = (0..config.audio_outputs)
            .map(|_| Arc::new(AudioIO::new(buffer_size)))
            .collect();

        let mut audio_in_ports = Vec::with_capacity(config.audio_inputs);
        for i in 0..config.audio_inputs {
            let p = client
                .register_port(&format!("in_{}", i + 1), AudioIn::default())
                .map_err(|e| format!("Failed to register JACK audio input port {}: {e}", i + 1))?;
            audio_in_ports.push(p);
        }

        let mut audio_out_ports = Vec::with_capacity(config.audio_outputs);
        for i in 0..config.audio_outputs {
            let p = client
                .register_port(&format!("out_{}", i + 1), AudioOut::default())
                .map_err(|e| format!("Failed to register JACK audio output port {}: {e}", i + 1))?;
            audio_out_ports.push(p);
        }

        let mut midi_in_ports = Vec::with_capacity(config.midi_inputs);
        for i in 0..config.midi_inputs {
            let p = client
                .register_port(&format!("midi_in_{}", i + 1), MidiIn::default())
                .map_err(|e| format!("Failed to register JACK MIDI input port {}: {e}", i + 1))?;
            midi_in_ports.push(p);
        }

        let mut midi_out_ports = Vec::with_capacity(config.midi_outputs);
        for i in 0..config.midi_outputs {
            let p = client
                .register_port(&format!("midi_out_{}", i + 1), MidiOut::default())
                .map_err(|e| format!("Failed to register JACK MIDI output port {}: {e}", i + 1))?;
            midi_out_ports.push(p);
        }

        let midi_in_events = Arc::new(UnsafeMutex::new(Vec::<MidiEvent>::new()));
        let midi_out_events = Arc::new(UnsafeMutex::new(Vec::<MidiEvent>::new()));
        let output_gain_linear = Arc::new(UnsafeMutex::new(1.0_f32));
        let output_balance = Arc::new(UnsafeMutex::new(0.0_f32));

        let process = Process {
            audio_in_ports,
            audio_out_ports,
            midi_in_ports,
            midi_out_ports,
            audio_in_bridges: audio_ins.clone(),
            audio_out_bridges: audio_outs.clone(),
            midi_in_events: midi_in_events.clone(),
            midi_out_events: midi_out_events.clone(),
            output_gain_linear: output_gain_linear.clone(),
            output_balance: output_balance.clone(),
            tx_engine,
        };

        let client = client
            .activate_async(Notifications, process)
            .map_err(|e| format!("Failed to activate JACK client: {e}"))?;

        Ok(Self {
            client: Some(client),
            audio_ins,
            audio_outs,
            midi_in_events,
            midi_out_events,
            output_gain_linear,
            output_balance,
            midi_input_count: config.midi_inputs,
            midi_output_count: config.midi_outputs,
            sample_rate,
            buffer_size,
        })
    }

    pub fn read_events_into(&self, out: &mut Vec<MidiEvent>) {
        let src = self.midi_in_events.lock();
        out.clear();
        out.extend(src.iter().cloned());
    }

    pub fn write_events(&self, events: &[MidiEvent]) {
        let dst = self.midi_out_events.lock();
        dst.clear();
        dst.extend_from_slice(events);
    }

    pub fn set_output_gain_linear(&self, gain: f32) {
        *self.output_gain_linear.lock() = gain.max(0.0);
    }

    pub fn set_output_balance(&self, balance: f32) {
        *self.output_balance.lock() = balance.clamp(-1.0, 1.0);
    }

    pub fn midi_input_devices(&self) -> Vec<String> {
        (0..self.midi_input_count)
            .map(|idx| format!("jack:midi_in_{}", idx + 1))
            .collect()
    }

    pub fn midi_output_devices(&self) -> Vec<String> {
        (0..self.midi_output_count)
            .map(|idx| format!("jack:midi_out_{}", idx + 1))
            .collect()
    }
}

impl Drop for JackRuntime {
    fn drop(&mut self) {
        if let Some(client) = self.client.take() {
            let _ = client.deactivate();
        }
    }
}
