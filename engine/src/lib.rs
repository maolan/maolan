pub mod audio;
pub mod midi;
pub mod client;
pub mod worker;
pub mod message;
pub mod state;
pub mod engine;

use std::thread;
use std::sync::mpsc::channel;

pub fn init() -> (client::Client, thread::JoinHandle<()>) {
    let (tx, rx) = channel::<message::Message>();
    let mut engine = engine::Engine::new(rx, tx.clone());
    let state = engine.state();
    let handle = thread::spawn(move || {
        engine.init();
        engine.work();
    });
    let client = client::Client::new(tx.clone(), state);
    (client, handle)
}
