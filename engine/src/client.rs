use std::sync::Arc;
use tokio::task::JoinHandle;

use super::init;
use super::message::Message;
use tokio::sync::mpsc::{Receiver, Sender, channel};

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
    pub async fn subscribe(&self) -> Receiver<Message> {
        let (tx, rx) = channel::<Message>(32);
        self.sender
            .send(Message::Channel(tx))
            .await
            .expect("Failed to subscribe to engine");
        rx
    }

    pub async fn send(&self, message: Message) -> Result<(), String> {
        self.sender
            .send(message)
            .await
            .map_err(|e| format!("Failed to send message from client: {:?}", e))
    }
}
