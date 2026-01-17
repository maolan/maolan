pub mod audio;
pub mod client;
mod engine;
pub mod message;
pub mod midi;
pub mod mutex;
pub mod state;
pub mod worker;

use tokio::sync::mpsc::{unbounded_channel as channel, UnboundedSender as Sender};
use tokio::task::JoinHandle;

pub fn init() -> (Sender<message::Message>, JoinHandle<()>) {
    let (tx, rx) = channel::<message::Message>();
    let mut engine = engine::Engine::new(rx, tx.clone());
    let handle = tokio::spawn(async move {
        engine.init().await;
        engine.work().await;
    });
    (tx.clone(), handle)
}
