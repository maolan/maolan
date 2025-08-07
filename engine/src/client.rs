use super::{Message, State};
use std::sync::mpsc::Sender;
use std::sync::{Arc, RwLock};
use std::thread::JoinHandle;

#[derive(Debug)]
pub struct Client {
    tx: Sender<Message>,
    state: Arc<RwLock<State>>,
    thread: JoinHandle<()>,
}

impl Client {
    pub fn new(
        tx: Sender<Message>,
        state: Arc<RwLock<State>>,
        thread: JoinHandle<()>,
    ) -> Self {
        Self { tx, state, thread }
    }

    pub fn send(&self, message: Message) {
        let _ = self.tx.send(message);
    }

    pub fn quit(self) {
        self.send(Message::Quit);
        let _ = self.thread.join();
    }

    pub fn add(&self) {
        self.send(Message::Add("".to_string()));
    }

    pub fn play(&self) {
        self.send(Message::Play);
    }

    pub fn state(&self) -> Arc<RwLock<State>> {
        self.state.clone()
    }
}
