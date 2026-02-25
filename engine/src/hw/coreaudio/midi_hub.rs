#![cfg(target_os = "macos")]

use crate::impl_hw_midi_hub_traits;
use crate::message::HwMidiEvent;
use crate::midi::io::MidiEvent;
use coremidi::{Client, Destination, Destinations, OutputPort, PacketBuffer, Source, Sources};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tracing::{error, info};

pub struct MidiHub {
    client: Option<Client>,
    input_port: Option<coremidi::InputPort>,
    output_port: Option<OutputPort>,
    pending: Arc<Mutex<VecDeque<HwMidiEvent>>>,
    input_sources: Vec<(String, Source)>,
    output_destinations: Vec<(String, Destination)>,
}

impl Default for MidiHub {
    fn default() -> Self {
        let pending: Arc<Mutex<VecDeque<HwMidiEvent>>> =
            Arc::new(Mutex::new(VecDeque::with_capacity(256)));

        let client = Client::new("maolan").ok();

        let input_port = client.as_ref().and_then(|c| {
            let pending_cb = Arc::clone(&pending);
            c.input_port("maolan-in", move |packet_list| {
                let mut queue = match pending_cb.lock() {
                    Ok(q) => q,
                    Err(_) => return,
                };
                for packet in packet_list.iter() {
                    let data = packet.data().to_vec();
                    if !data.is_empty() {
                        queue.push_back(HwMidiEvent {
                            device: String::new(),
                            event: MidiEvent::new(0, data),
                        });
                    }
                }
            })
            .ok()
        });

        let output_port = client
            .as_ref()
            .and_then(|c| c.output_port("maolan-out").ok());

        Self {
            client,
            input_port,
            output_port,
            pending,
            input_sources: Vec::new(),
            output_destinations: Vec::new(),
        }
    }
}

impl MidiHub {
    pub fn list_sources() -> Vec<String> {
        let mut result = Vec::new();
        for src in Sources {
            let display = src.display_name().unwrap_or_default();
            result.push(format!("coreaudio:{display}"));
        }
        result
    }

    pub fn list_destinations() -> Vec<String> {
        let mut result = Vec::new();
        for dest in Destinations {
            let display = dest.display_name().unwrap_or_default();
            result.push(format!("coreaudio:{display}"));
        }
        result
    }

    pub fn open_input(&mut self, name: &str) -> Result<(), String> {
        let raw_name = name.strip_prefix("coreaudio:").unwrap_or(name);
        if self.input_sources.iter().any(|(n, _)| n == raw_name) {
            return Ok(());
        }

        let source = find_source_by_name(raw_name)
            .ok_or_else(|| format!("CoreMIDI source not found: {raw_name}"))?;

        if let Some(ref client) = self.client {
            let pending_cb = Arc::clone(&self.pending);
            let device_name = raw_name.to_string();
            match client.input_port(&format!("maolan-in-{raw_name}"), move |packet_list| {
                let mut queue = match pending_cb.lock() {
                    Ok(q) => q,
                    Err(_) => return,
                };
                for packet in packet_list.iter() {
                    let data = packet.data().to_vec();
                    if !data.is_empty() {
                        queue.push_back(HwMidiEvent {
                            device: device_name.clone(),
                            event: MidiEvent::new(0, data),
                        });
                    }
                }
            }) {
                Ok(port) => {
                    if let Err(e) = port.connect_source(&source) {
                        return Err(format!(
                            "Failed to connect CoreMIDI source '{raw_name}': {e:?}"
                        ));
                    }

                    info!("CoreMIDI input connected: {raw_name}");
                }
                Err(e) => {
                    return Err(format!("Failed to create CoreMIDI input port: {e:?}"));
                }
            }
        } else {
            return Err("CoreMIDI client not available".to_string());
        }

        self.input_sources.push((raw_name.to_string(), source));
        Ok(())
    }

    pub fn open_output(&mut self, name: &str) -> Result<(), String> {
        let raw_name = name.strip_prefix("coreaudio:").unwrap_or(name);
        if self.output_destinations.iter().any(|(n, _)| n == raw_name) {
            return Ok(());
        }

        let dest = find_destination_by_name(raw_name)
            .ok_or_else(|| format!("CoreMIDI destination not found: {raw_name}"))?;

        info!("CoreMIDI output connected: {raw_name}");
        self.output_destinations.push((raw_name.to_string(), dest));
        Ok(())
    }

    pub fn read_events_into(&mut self, out: &mut Vec<HwMidiEvent>) {
        out.clear();
        let mut queue = match self.pending.lock() {
            Ok(q) => q,
            Err(_) => return,
        };
        out.extend(queue.drain(..));
    }

    pub fn write_events(&mut self, events: &[HwMidiEvent]) {
        if events.is_empty() {
            return;
        }

        let output_port = match self.output_port {
            Some(ref port) => port,
            None => return,
        };

        for (dest_name, destination) in &self.output_destinations {
            for event in events {
                if event.device != *dest_name {
                    continue;
                }
                if event.event.data.is_empty() {
                    continue;
                }

                let packet_buf = PacketBuffer::new(0, &event.event.data);
                if let Err(e) = output_port.send(destination, &packet_buf) {
                    error!("CoreMIDI write error on {dest_name}: {e:?}");
                }
            }
        }
    }
}

fn find_source_by_name(name: &str) -> Option<Source> {
    for source in Sources {
        if let Some(display_name) = source.display_name() {
            if display_name == name {
                return Some(source);
            }
        }
    }
    None
}

fn find_destination_by_name(name: &str) -> Option<Destination> {
    for dest in Destinations {
        if let Some(display_name) = dest.display_name() {
            if display_name == name {
                return Some(dest);
            }
        }
    }
    None
}

impl_hw_midi_hub_traits!(MidiHub);
