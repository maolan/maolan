pub mod audio;
pub mod midi;
pub mod client;
pub mod worker;
pub mod message;
pub mod state;
pub mod mutex;
mod engine;

use std::thread;
use std::sync::mpsc::{channel, Receiver};

pub fn init() -> (client::Client, thread::JoinHandle<()>, Receiver<message::Message>) {
    let (tx1, rx1) = channel::<message::Message>();
    let (tx2, rx2) = channel::<message::Message>();
    let mut engine = engine::Engine::new(rx1, tx1.clone(), tx2.clone());
    let handle = thread::spawn(move || {
        engine.init();
        engine.work();
    });
    let client = client::Client::new(tx1.clone());
    (client, handle, rx2)
}
