use std::sync::Arc;
use tokio::sync::mpsc::{
    UnboundedReceiver as Receiver, UnboundedSender as Sender, unbounded_channel as channel,
};
use tokio::task::JoinHandle;

use crate::audio::track::AudioTrack;
use crate::message::{Action, Message, TrackKind};
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
    clients: Vec<Sender<Message>>,
    rx: Receiver<Message>,
    state: Arc<UnsafeMutex<State>>,
    tx: Sender<Message>,
    workers: Vec<WorkerData>,
}

impl Engine {
    pub fn new(rx: Receiver<Message>, tx: Sender<Message>) -> Self {
        Self {
            clients: vec![],
            rx,
            state: Arc::new(UnsafeMutex::new(State::default())),
            tx,
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

    fn notify_clients(&self, action: Result<Action, String>) {
        for client in &self.clients {
            client
                .send(Message::Response(action.clone()))
                .expect("Error sending response to client");
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
                            while !self.workers.is_empty() {
                                let worker = self.workers.remove(0);
                                worker
                                    .tx
                                    .send(Message::Request(a.clone()))
                                    .expect("Failed sending quit message to worker");
                                worker
                                    .handle
                                    .await
                                    .expect("Failed waiting for worker to quit");
                            }
                        }
                        Action::AddTrack {
                            ref name,
                            kind,
                            ins,
                            audio_outs,
                            midi_outs,
                        } => {
                            let tracks = &mut self.state.lock().tracks;
                            if tracks.contains_key(name) {
                                self.notify_clients(Err(format!("Track {} already exists", name)));
                                return;
                            }
                            match kind {
                                TrackKind::Audio => {
                                    tracks.insert(
                                        name.clone(),
                                        Arc::new(UnsafeMutex::new(Box::new(AudioTrack::new(
                                            name.clone(),
                                            ins,
                                            audio_outs,
                                            midi_outs,
                                        )))),
                                    );
                                }
                                TrackKind::MIDI => {
                                    self.state.lock().tracks.insert(
                                        name.clone(),
                                        Arc::new(UnsafeMutex::new(Box::new(MIDITrack::new(
                                            name.clone(),
                                            ins,
                                            audio_outs,
                                            midi_outs,
                                        )))),
                                    );
                                }
                            }
                        }
                        Action::DeleteTrack(ref name) => {
                            self.state.lock().tracks.remove(name);
                        }
                        Action::TrackLevel(ref name, level) => {
                            if let Some(track) = self.state.lock().tracks.get(name) {
                                track.lock().set_level(level);
                            }
                        }
                        Action::TrackIns(ref name, ins) => {
                            if let Some(track) = self.state.lock().tracks.get(name) {
                                track.lock().set_ins(ins);
                            }
                        }
                        Action::TrackAudioOuts(ref name, outs) => {
                            if let Some(track) = self.state.lock().tracks.get(name) {
                                track.lock().set_audio_outs(outs);
                            }
                        }
                        Action::TrackMIDIOuts(ref name, outs) => {
                            if let Some(track) = self.state.lock().tracks.get(name) {
                                track.lock().set_midi_outs(outs);
                            }
                        }
                        Action::TrackToggleArm(ref name) => {
                            if let Some(track) = self.state.lock().tracks.get(name) {
                                track.lock().arm();
                            }
                        }
                        Action::TrackToggleMute(ref name) => {
                            if let Some(track) = self.state.lock().tracks.get(name) {
                                track.lock().mute();
                            }
                        }
                        Action::TrackToggleSolo(ref name) => {
                            if let Some(track) = self.state.lock().tracks.get(name) {
                                track.lock().solo();
                            }
                        }
                        Action::ClipMove(ref clip, copy) => {
                            if let Some(from_track_handle) =
                                self.state.lock().tracks.get(&clip.from.0)
                                && let Some(to_track_handle) =
                                    self.state.lock().tracks.get(&clip.to.0)
                            {
                                let from_track = from_track_handle.lock();
                                let to_track = to_track_handle.lock();
                                if let Err(e) = to_track.add(from_track.at(clip.from.1)) {
                                    self.notify_clients(Err(e));
                                    return;
                                }
                            }
                        }
                    }
                    self.notify_clients(Ok(a.clone()));
                }
                _ => {}
            }
        }
    }
}
