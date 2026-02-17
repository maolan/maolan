use crate::message::{Action, Message};
use tokio::sync::mpsc::{Receiver, Sender};

#[derive(Debug)]
pub struct Worker {
    id: usize,
    rx: Receiver<Message>,
    tx: Sender<Message>,
}

impl Worker {
    pub async fn new(id: usize, rx: Receiver<Message>, tx: Sender<Message>) -> Worker {
        let worker = Worker { id, rx, tx };
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
                Message::Request(Action::Quit) => {
                    return;
                }
                Message::ProcessTrack(t) => {
                    println!("worker");
                    let track = t.lock();
                    track.process();
                    match self.tx.send(Message::Finished(self.id)).await {
                        Ok(_) => {}
                        Err(e) => {
                            println!("Error while sending Finished: {e}")
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
