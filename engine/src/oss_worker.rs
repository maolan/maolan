use crate::{hw::oss, message::Message, mutex::UnsafeMutex};
use std::sync::Arc;
use tokio::sync::mpsc::Receiver;

#[derive(Debug)]
pub struct OssWorker {
    oss_in: Arc<UnsafeMutex<oss::Audio>>,
    oss_out: Arc<UnsafeMutex<oss::Audio>>,
    rx: Receiver<Message>,
}

impl OssWorker {
    pub fn new(
        oss_in: Arc<UnsafeMutex<oss::Audio>>,
        oss_out: Arc<UnsafeMutex<oss::Audio>>,
        rx: Receiver<Message>,
    ) -> Self {
        Self {
            oss_in,
            oss_out,
            rx,
        }
    }

    pub async fn work(mut self) {
        loop {
            tokio::select! {
                message = self.rx.recv() => {
                    if let Some(msg) = message {
                        if let Message::Request(crate::message::Action::Quit) = msg {
                            return;
                        }
                    } else {
                        return;
                    }
                }
                _ = async {
                    // Read and convert input samples
                    {
                        let oss_in = self.oss_in.lock();
                        if let Err(e) = oss_in.read() {
                            eprintln!("OSS input read error: {}", e);
                        }
                        oss_in.process();
                    }

                    // Process output channels (pull from connections)
                    {
                        let oss_out = self.oss_out.lock();
                        for channel in &oss_out.channels {
                            channel.process();
                        }
                        oss_out.process();
                        if let Err(e) = oss_out.write() {
                            eprintln!("OSS output write error: {}", e);
                        }
                    }
                } => {}
            }
        }
    }
}
