use super::message::{Message, Track};
use std::sync::mpsc::Sender;

#[derive(Debug, Clone)]
pub struct Client {
    tx: Sender<Message>,
}

impl Client {
    pub fn new(
        tx: Sender<Message>,
    ) -> Self {
        Self { tx }
    }

    pub fn send(&self, message: Message) {
        let _ = self.tx.send(message);
    }

    pub fn quit(&self) {
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
}
