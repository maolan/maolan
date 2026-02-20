use crate::{
    hw::oss::{self, MidiHub},
    message::Message,
    mutex::UnsafeMutex,
};
use nix::libc;
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
    midi_in_events: Vec<crate::midi::io::MidiEvent>,
    pending_midi_out_sorted: bool,
}

impl OssWorker {
    #[cfg(unix)]
    fn try_enable_realtime() -> Result<(), String> {
        // Best-effort RT priority for the OS thread running this worker task.
        // Requires appropriate system privileges (e.g. rtprio/limits).
        let thread = unsafe { libc::pthread_self() };
        let policy = libc::SCHED_FIFO;
        let param = libc::sched_param { sched_priority: 10 };
        let rc = unsafe { libc::pthread_setschedparam(thread, policy, &param) };
        if rc == 0 {
            Ok(())
        } else {
            Err(format!(
                "pthread_setschedparam failed with errno {}",
                rc
            ))
        }
    }

    #[cfg(not(unix))]
    fn try_enable_realtime() -> Result<(), String> {
        Err("Realtime thread priority is not supported on this platform".to_string())
    }

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
            midi_in_events: Vec::with_capacity(64),
            pending_midi_out_sorted: true,
        }
    }

    pub async fn work(mut self) {
        if let Err(e) = Self::try_enable_realtime() {
            eprintln!("OSS worker realtime priority not enabled: {}", e);
        }
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
                        {
                            let midi_hub = self.midi_hub.lock();
                            midi_hub.read_events_into(&mut self.midi_in_events);
                        }
                        spread_event_frames(&mut self.midi_in_events, frames);
                        if !self.midi_in_events.is_empty() {
                            let cap = self.midi_in_events.capacity();
                            let out = std::mem::replace(
                                &mut self.midi_in_events,
                                Vec::with_capacity(cap.max(64)),
                            );
                            if let Err(e) = self.tx.send(Message::HWMidiEvents(out)).await
                            {
                                eprintln!("OSS worker failed to send HWMidiEvents to engine: {}", e);
                            }
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
                                if !self.pending_midi_out_sorted {
                                    self.pending_midi_out_events
                                        .sort_by(|a, b| a.frame.cmp(&b.frame));
                                    self.pending_midi_out_sorted = true;
                                }
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
                        self.pending_midi_out_sorted = false;
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
