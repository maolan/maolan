pub mod audio;
pub mod client;
mod engine;
pub mod message;
pub mod midi;
pub mod mutex;
pub mod state;
pub mod worker;

use tokio::sync::mpsc::{UnboundedReceiver as Receiver, unbounded_channel as channel};
use tokio::task::JoinHandle;

pub fn init() -> (client::Client, JoinHandle<()>, Receiver<message::Message>) {
    let (tx1, rx1) = channel::<message::Message>();
    let (tx2, rx2) = channel::<message::Message>();
    let mut engine = engine::Engine::new(rx1, tx1.clone(), tx2.clone());
    let handle = tokio::spawn(async move {
        engine.init().await;
        engine.work().await;
    });
    let client = client::Client::new(tx1.clone());
    (client, handle, rx2)
}
