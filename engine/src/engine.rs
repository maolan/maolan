use std::sync::Arc;
use tokio::sync::mpsc::{
    UnboundedReceiver as Receiver, UnboundedSender as Sender, unbounded_channel as channel,
};
use tokio::task::JoinHandle;

use crate::audio::track::Track as AudioTrack;
use crate::message::{Action, Message};
use crate::midi::track::Track as MIDITrack;
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

#[derive(Debug)]
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
            state: Arc::new(UnsafeMutex::new(State::new())),
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
                        Action::Quit => {
                            while self.workers.len() > 0 {
                                let worker = self.workers.remove(0);
                                let _ = worker.tx.send(Message::Request(a.clone()));
                                let _ = worker.handle.await;
                            }
                        }
                        Action::AddAudioTrack(ref name, ins, _audio_outs, _midi_outs) => {
                            self.state.lock().audio.tracks.insert(
                                name.clone(),
                                Arc::new(UnsafeMutex::new(AudioTrack::new(name.clone(), ins))),
                            );
                        }
                        Action::AddMIDITrack(ref name, _midi_outs, _audio_outs) => {
                            self.state.lock().midi.tracks.insert(
                                name.clone(),
                                Arc::new(UnsafeMutex::new(MIDITrack::new(name.clone()))),
                            );
                        }
                        _ => {}
                    }
                    for client in &self.clients {
                        client
                            .send(Message::Response(a.clone()))
                            .expect("Error sending echo from engine");
                    }
                }
                _ => {}
            }
        }
    }
}
