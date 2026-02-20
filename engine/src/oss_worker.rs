use crate::{
    hw::oss::{self, MidiHub},
    message::Message,
    mutex::UnsafeMutex,
};
use std::sync::Arc;
use tokio::sync::mpsc::{Receiver, Sender};

#[derive(Debug)]
pub struct OssWorker {
    oss_in: Arc<UnsafeMutex<oss::Audio>>,
    oss_out: Arc<UnsafeMutex<oss::Audio>>,
    midi_hub: Arc<UnsafeMutex<MidiHub>>,
    rx: Receiver<Message>,
    tx: Sender<Message>,
    pending_midi_out_events: Vec<crate::midi::io::MidiEvent>,
}

impl OssWorker {
    pub fn new(
        oss_in: Arc<UnsafeMutex<oss::Audio>>,
        oss_out: Arc<UnsafeMutex<oss::Audio>>,
        midi_hub: Arc<UnsafeMutex<MidiHub>>,
        rx: Receiver<Message>,
        tx: Sender<Message>,
    ) -> Self {
        Self {
            oss_in,
            oss_out,
            midi_hub,
            rx,
            tx,
            pending_midi_out_events: vec![],
        }
    }

    pub async fn work(mut self) {
        loop {
            match self.rx.recv().await {
                Some(msg) => match msg {
                    Message::Request(crate::message::Action::Quit) => {
                        return;
                    }
                    Message::TracksFinished => {
                        let frames = {
                            let oss_in = self.oss_in.lock();
                            oss_in.chsamples as u32
                        };
                        let midi_events = {
                            let midi_hub = self.midi_hub.lock();
                            midi_hub.read_events()
                        };
                        let mut midi_events = midi_events;
                        spread_event_frames(&mut midi_events, frames);
                        if !midi_events.is_empty()
                            && let Err(e) = self.tx.send(Message::HWMidiEvents(midi_events)).await
                        {
                            eprintln!("OSS worker failed to send HWMidiEvents to engine: {}", e);
                        }
                        {
                            let oss_in = self.oss_in.lock();
                            if let Err(e) = oss_in.read() {
                                eprintln!("OSS input read error: {}", e);
                            }
                            oss_in.process();
                        }
                        if let Err(e) = self.tx.send(Message::HWFinished).await {
                            eprintln!("OSS worker failed to send HWFinished to engine: {}", e);
                        }
                        {
                            if !self.pending_midi_out_events.is_empty() {
                                let midi_hub = self.midi_hub.lock();
                                midi_hub.write_events(&self.pending_midi_out_events);
                                self.pending_midi_out_events.clear();
                            }
                            let oss_out = self.oss_out.lock();
                            oss_out.process();
                            if let Err(e) = oss_out.write() {
                                eprintln!("OSS output write error: {}", e);
                            }
                        }
                    }
                    Message::HWMidiOutEvents(mut events) => {
                        self.pending_midi_out_events.append(&mut events);
                        self.pending_midi_out_events
                            .sort_by(|a, b| a.frame.cmp(&b.frame));
                    }
                    _ => {}
                },
                None => {
                    return;
                }
            }
        }
    }
}

fn spread_event_frames(events: &mut [crate::midi::io::MidiEvent], frames: u32) {
    if events.len() <= 1 || frames <= 1 {
        return;
    }
    let n = events.len() as u32;
    for (idx, event) in events.iter_mut().enumerate() {
        let pos = idx as u32;
        event.frame = ((pos as u64 * (frames - 1) as u64) / n as u64) as u32;
    }
}
