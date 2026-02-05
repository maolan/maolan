pub mod audio;
pub mod client;
mod clip;
mod engine;
pub mod message;
pub mod midi;
pub mod mutex;
pub mod state;
mod track;
pub mod worker;

use tokio::sync::mpsc::{Sender, channel};
use tokio::task::JoinHandle;

pub fn init() -> (Sender<message::Message>, JoinHandle<()>) {
    let (tx, rx) = channel::<message::Message>(32);
    let mut engine = engine::Engine::new(rx, tx.clone());
    let handle = tokio::spawn(async move {
        engine.init().await;
        engine.work().await;
    });
    (tx.clone(), handle)
}
