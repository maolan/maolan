use std::sync::Arc;
use tokio::sync::mpsc::{
    UnboundedReceiver as Receiver, UnboundedSender as Sender, unbounded_channel as channel,
};
use tokio::task::JoinHandle;

use crate::audio::track::AudioTrack;
use crate::message::{Action, Message};
use crate::midi::track::MIDITrack;
use crate::mutex::UnsafeMutex;
use crate::state::State;
use crate::worker::Worker;

#[derive(Debug)]
struct WorkerData {
    tx: Sender<Message>,
    handle: JoinHandle<()>,
}

impl WorkerData {
    pub fn new(tx: Sender<Message>, handle: JoinHandle<()>) -> Self {
        Self { tx, handle }
    }
}

pub struct Engine {
    state: Arc<UnsafeMutex<State>>,
    rx: Receiver<Message>,
    tx: Sender<Message>,
    clients: Vec<Sender<Message>>,
    workers: Vec<WorkerData>,
}

impl Engine {
    pub fn new(rx: Receiver<Message>, tx: Sender<Message>) -> Self {
        Self {
            state: Arc::new(UnsafeMutex::new(State::default())),
            rx,
            tx,
            clients: vec![],
            workers: vec![],
        }
    }

    pub async fn init(&mut self) {
        let max_threads = num_cpus::get();
        for id in 0..max_threads {
            let (tx, rx) = channel::<Message>();
            let tx_thread = self.tx.clone();
            let handler = tokio::spawn(async move {
                let mut wrk = Worker::new(id, rx, tx_thread);
                wrk.work().await;
            });
            self.workers.push(WorkerData::new(tx.clone(), handler));
        }
    }

    pub async fn work(&mut self) {
        let mut ready_workers: Vec<usize> = vec![];
        while let Some(message) = self.rx.recv().await {
            match message {
                // Message::Play => {
                //     let track;
                //     {
                //         track = self.state.lock().audio.tracks[""].clone();
                //     }
                //     match self.workers[0].tx.send(Message::ProcessAudio(track)) {
                //         Ok(_) => {}
                //         Err(e) => {
                //             println!("Error occured while sending PLAY: {e}")
                //         }
                //     }
                // }
                Message::Ready(id) => {
                    ready_workers.push(id);
                }
                Message::Finished(_workid, _trackid) => {}
                Message::Channel(s) => {
                    self.clients.push(s);
                }

                Message::Request(a) => {
                    match a {
                        Action::Play => {}
                        Action::Quit => {
                            while self.workers.len() > 0 {
                                let worker = self.workers.remove(0);
                                let _ = worker.tx.send(Message::Request(a.clone()));
                                let _ = worker.handle.await;
                            }
                        }
                        Action::AddAudioTrack {
                            ref name,
                            ins,
                            audio_outs,
                            midi_outs,
                        } => {
                            self.state.lock().tracks.insert(
                                name.clone(),
                                Arc::new(UnsafeMutex::new(Box::new(AudioTrack::new(
                                    name.clone(),
                                    ins,
                                    audio_outs,
                                    midi_outs,
                                )))),
                            );
                        }
                        Action::AddMIDITrack {
                            ref name,
                            ins,
                            midi_outs,
                            audio_outs,
                        } => {
                            self.state.lock().tracks.insert(
                                name.clone(),
                                Arc::new(UnsafeMutex::new(Box::new(MIDITrack::new(
                                    name.clone(),
                                    ins,
                                    midi_outs,
                                    audio_outs,
                                )))),
                            );
                        }
                        Action::TrackLevel(ref name, value) => {
                            for (_, track) in &self.state.lock().tracks {
                                if *name == track.lock().name() {
                                    track.lock().set_level(value);
                                }
                            }
                        }
                        Action::TrackIns(ref name, ins) => {
                            for (_, track) in &self.state.lock().tracks {
                                if *name == track.lock().name() {
                                    track.lock().set_ins(ins);
                                }
                            }
                        }
                        Action::TrackAudioOuts(ref name, outs) => {
                            for (_, track) in &self.state.lock().tracks {
                                if *name == track.lock().name() {
                                    track.lock().set_audio_outs(outs);
                                }
                            }
                        }
                        Action::TrackMIDIOuts(ref name, outs) => {
                            for (_, track) in &self.state.lock().tracks {
                                if *name == track.lock().name() {
                                    track.lock().set_midi_outs(outs);
                                }
                            }
                        }
                        Action::TrackToggleArm(ref name) => {
                            for (_, track) in &self.state.lock().tracks {
                                if *name == track.lock().name() {
                                    track.lock().arm();
                                }
                            }
                        }
                        Action::TrackToggleMute(ref name) => {
                            for (_, track) in &self.state.lock().tracks {
                                if *name == track.lock().name() {
                                    track.lock().mute();
                                }
                            }
                        }
                        Action::TrackToggleSolo(ref name) => {
                            for (_, track) in &self.state.lock().tracks {
                                if *name == track.lock().name() {
                                    track.lock().solo();
                                }
                            }
                        }
                    }
                    for client in &self.clients {
                        client
                            .send(Message::Response(Ok(a.clone())))
                            .expect("Error sending echo from engine");
                    }
                }
                _ => {}
            }
        }
    }
}
