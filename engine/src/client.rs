use super::{Message, State, Track};
use std::sync::mpsc::Sender;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone)]
pub struct Client {
    tx: Sender<Message>,
    state: Arc<RwLock<State>>,
}

impl Client {
    pub fn new(
        tx: Sender<Message>,
        state: Arc<RwLock<State>>,
    ) -> Self {
        Self { tx, state }
    }

    pub fn send(&self, message: Message) {
        let _ = self.tx.send(message);
    }

    pub fn quit(self) {
        self.send(Message::Quit);
    }

    pub fn add_audio_track(&self, name: String, channels: usize) {
        self.send(Message::Add(Track::Audio(name, channels)));
    }

    pub fn add_midi_track(&self, name: String) {
        self.send(Message::Add(Track::MIDI(name)));
    }

    pub fn play(&self) {
        self.send(Message::Play);
    }

    pub fn state(&self) -> Arc<RwLock<State>> {
        self.state.clone()
    }
}
