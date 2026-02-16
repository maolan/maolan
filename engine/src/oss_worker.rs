use crate::{hw::oss, message::Message, mutex::UnsafeMutex};
use std::sync::Arc;
use tokio::sync::mpsc::{Receiver, Sender};

#[derive(Debug)]
pub struct OssWorker {
    oss_in: Arc<UnsafeMutex<oss::Audio>>,
    oss_out: Arc<UnsafeMutex<oss::Audio>>,
    rx: Receiver<Message>,
    tx: Sender<Message>,
}

impl OssWorker {
    pub fn new(
        oss_in: Arc<UnsafeMutex<oss::Audio>>,
        oss_out: Arc<UnsafeMutex<oss::Audio>>,
        rx: Receiver<Message>,
        tx: Sender<Message>,
    ) -> Self {
        Self {
            oss_in,
            oss_out,
            rx,
            tx,
        }
    }

    pub async fn work(mut self) {
        loop {
            match self.rx.recv().await {
                Some(msg) => match msg {
                    Message::Request(crate::message::Action::Quit) => {
                        return;
                    }
                    Message::TracksFinished => {
                        {
                            let oss_in = self.oss_in.lock();
                            if let Err(e) = oss_in.read() {
                                eprintln!("OSS input read error: {}", e);
                            }
                            oss_in.process();
                        }
                        if let Err(e) = self.tx.send(Message::HWFinished).await {
                            eprintln!("OSS worker failed to send HWFinished to engine: {}", e);
                        }
                        {
                            let oss_out = self.oss_out.lock();
                            oss_out.process();
                            if let Err(e) = oss_out.write() {
                                eprintln!("OSS output write error: {}", e);
                            }
                        }
                    }
                    _ => {}
                },
                None => {
                    return;
                }
            }
        }
    }
}
