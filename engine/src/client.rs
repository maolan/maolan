use std::sync::Arc;
use tokio::task::JoinHandle;

use super::init;
use super::message::Message;
use tokio::sync::mpsc::{
    UnboundedReceiver as Receiver, UnboundedSender as Sender, unbounded_channel as channel,
};

#[derive(Debug, Clone)]
pub struct Client {
    pub sender: Sender<Message>,
    _handle: Arc<JoinHandle<()>>,
}

impl Default for Client {
    fn default() -> Self {
        let (sender, handle) = init();
        Self {
            sender,
            _handle: Arc::new(handle),
        }
    }
}

impl Client {
    pub fn subscribe(&self) -> Receiver<Message> {
        let (tx, rx) = channel::<Message>();
        self.sender
            .send(Message::Channel(tx))
            .expect("Failed to subscribe to engine");
        rx
    }

    pub fn send(&self, message: Message) {
        self.sender
            .send(message)
            .expect("Failed to send message {message}");
    }
}
