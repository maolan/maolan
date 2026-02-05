use crate::message::{Action, Message};
use tokio::sync::mpsc::{Receiver, Sender};

#[derive(Debug)]
pub struct Worker {
    _id: usize,
    rx: Receiver<Message>,
    tx: Sender<Message>,
}

impl Worker {
    pub async fn new(id: usize, rx: Receiver<Message>, tx: Sender<Message>) -> Worker {
        let worker = Worker { _id: id, rx, tx };
        worker.send(Message::Ready(id)).await;
        worker
    }

    pub async fn send(&self, message: Message) {
        self.tx
            .send(message)
            .await
            .expect("Failed to send message from worker");
    }

    pub async fn work(&mut self) {
        while let Some(message) = self.rx.recv().await {
            match message {
                Message::Request(a) => match a {
                    Action::Quit => {
                        return;
                    }
                    _ => {}
                },
                // Message::ProcessAudio(t) => {
                //     let track = t.lock();
                //     match self.tx.send(Message::Finished(self.id, track.name())) {
                //         Ok(_) => {}
                //         Err(e) => {
                //             println!("Error while sending Finished: {e}")
                //         }
                //     }
                // }
                _ => {}
            }
        }
    }
}
