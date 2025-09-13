mod audio;
mod midi;
pub mod client;
mod worker;

use self::audio::track::Track as AudioTrack;
use self::midi::track::Track as MIDITrack;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, RwLock};
use std::thread::{self, JoinHandle};

pub enum Track {
    Audio(String),
    MIDI(String),
}

pub enum Message {
    Add(Track),
    Quit,
    Play,
    Ready(usize),
    Process(Arc<RwLock<AudioTrack>>),
    Finished(usize, String),
}

#[derive(Debug)]
pub struct State {
    pub audio: audio::State,
    pub midi: midi::State,
}

impl State {
    pub fn new() -> Self {
        State {
            audio: audio::State::new(),
            midi: midi::State::new(),
        }
    }
}

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
    state: Arc<RwLock<State>>,
    rx: Receiver<Message>,
    tx: Sender<Message>,
    workers: Vec<WorkerData>,
}

impl Engine {
    pub fn new(rx: Receiver<Message>, tx: Sender<Message>) -> Self {
        Self {
            state: Arc::new(RwLock::new(State::new())),
            rx,
            tx,
            workers: vec![],
        }
    }

    pub fn init(&mut self) {
        let max_threads = num_cpus::get();
        for id in 0..max_threads {
            let (tx, rx) = channel::<Message>();
            let tx_thread = self.tx.clone();
            let handler = thread::spawn(move || {
                let wrk = worker::Worker::new(id, rx, tx_thread);
                wrk.work();
            });
            self.workers.push(WorkerData::new(tx.clone(), handler));
        }
    }

    pub fn work(&mut self) {
        let mut ready_workers: Vec<usize> = vec![];
        for message in &self.rx {
            match message {
                Message::Play => {
                    let track;
                    {
                        track = self.state.write().unwrap().audio.tracks[""].clone();
                    }
                    match self.workers[0].tx.send(Message::Process(track)) {
                        Ok(_) => {}
                        Err(e) => {
                            println!("Error occured while sending PLAY: {e}")
                        }
                    }
                }
                Message::Quit => {
                    while self.workers.len() > 0 {
                        let worker = self.workers.remove(0);
                        let _ = worker.tx.send(Message::Quit);
                        let _ = worker.handle.join();
                    }
                    return;
                }
                Message::Ready(id) => {
                    ready_workers.push(id);
                }
                Message::Add(t) => {
                    // This should go to queue and be actualized only before start of processing
                    // of new buffer
                    match t {
                        Track::Audio(name) => {
                            self.state
                                .write()
                                .unwrap()
                                .audio
                                .tracks
                                .insert(name.clone(), Arc::new(RwLock::new(AudioTrack::new(name))));
                        }
                        Track::MIDI(name) => {
                            self.state
                                .write()
                                .unwrap()
                                .midi
                                .tracks
                                .insert(name.clone(), Arc::new(RwLock::new(MIDITrack::new(name))));
                        }
                    }
                }
                Message::Finished(_workid, _trackid) => {}
                _ => {}
            }
        }
    }

    pub fn state(&self) -> Arc<RwLock<State>> {
        self.state.clone()
    }
}

pub fn init() -> (client::Client, JoinHandle<()>) {
    let (tx, rx) = channel::<Message>();
    let mut engine = Engine::new(rx, tx.clone());
    let state = engine.state();
    let handle = thread::spawn(move || {
        engine.init();
        engine.work();
    });
    let client = client::Client::new(tx.clone(), state);
    (client, handle)
}
