use crate::message::{Action, Message};
use tokio::sync::mpsc::{UnboundedReceiver as Receiver, UnboundedSender as Sender};

#[derive(Debug)]
pub struct Worker {
    _id: usize,
    rx: Receiver<Message>,
    tx: Sender<Message>,
}

impl Worker {
    pub fn new(id: usize, rx: Receiver<Message>, tx: Sender<Message>) -> Worker {
        let worker = Worker { _id: id, rx, tx };
        worker.send(Message::Ready(id));
        worker
    }

    pub fn send(&self, message: Message) {
        self.tx
            .send(message)
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
