use crate::message::{Action, Message};
#[cfg(unix)]
use nix::libc;
use tokio::sync::mpsc::{Receiver, Sender};
use tracing::error;

#[derive(Debug)]
pub struct Worker {
    id: usize,
    rx: Receiver<Message>,
    tx: Sender<Message>,
}

impl Worker {
    #[cfg(unix)]
    fn try_enable_realtime() -> Result<(), String> {
        // Best-effort RT priority for the OS thread running this worker task.
        // Requires appropriate system privileges (e.g. rtprio/limits).
        let thread = unsafe { libc::pthread_self() };
        let policy = libc::SCHED_FIFO;
        let param = unsafe {
            let mut p = std::mem::zeroed::<libc::sched_param>();
            p.sched_priority = 10;
            p
        };
        let rc = unsafe { libc::pthread_setschedparam(thread, policy, &param) };
        if rc == 0 {
            Ok(())
        } else {
            Err(format!("pthread_setschedparam failed with errno {}", rc))
        }
    }

    #[cfg(not(unix))]
    fn try_enable_realtime() -> Result<(), String> {
        Err("Realtime thread priority is not supported on this platform".to_string())
    }

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
        if let Err(e) = Self::try_enable_realtime() {
            error!("Worker {} realtime priority not enabled: {}", self.id, e);
        }
        while let Some(message) = self.rx.recv().await {
            match message {
                Message::Request(Action::Quit) => {
                    return;
                }
                Message::ProcessTrack(t) => {
                    let track = t.lock();
                    track.process();
                    track.audio.processing = false;
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
