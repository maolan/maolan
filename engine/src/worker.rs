use crate::message::Message;
use std::sync::mpsc::{Receiver, Sender};

#[derive(Debug)]
pub struct Worker {
    id: usize,
    rx: Receiver<Message>,
    tx: Sender<Message>,
}

impl Worker {
    pub fn new(id: usize, rx: Receiver<Message>, tx: Sender<Message>) -> Worker {
        let worker = Worker { id, rx, tx };
        worker.send(Message::Ready(id));
        worker
    }

    pub fn send(&self, message: Message) {
        let _ = self.tx.send(message);
    }

    pub fn work(&self) {
        for message in &self.rx {
            match message {
                Message::Quit => {
                    return;
                }
                Message::ProcessAudio(t) => match t.write() {
                    Ok(mut track) => {
                        track.process();
                        match self.tx.send(Message::Finished(self.id, track.name())) {
                            Ok(_) => {}
                            Err(e) => {
                                println!("Error while sending Finished: {e}")
                            }
                        }
                    }
                    Err(e) => {
                        println!("Error locking in Worker::work: {e}")
                    }
                },
                _ => {}
            }
        }
    }
}
