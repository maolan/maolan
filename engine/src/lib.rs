mod audio;
pub mod client;
mod engine;
mod hw;
pub mod kind;
pub mod message;
mod midi;
pub mod mutex;
pub mod plugins;
mod routing;
pub mod state;
mod track;
pub mod workers;
pub use workers::worker;

pub use plugins::clap;
#[cfg(all(unix, not(target_os = "macos")))]
pub use plugins::lv2;
pub use plugins::vst3;

use tokio::sync::mpsc::{Sender, channel};
use tokio::task::JoinHandle;

#[cfg(target_os = "macos")]
pub fn discover_coreaudio_devices() -> Vec<String> {
    hw::coreaudio::device::list_devices()
        .into_iter()
        .map(|d| d.name)
        .collect()
}

pub fn init() -> (Sender<message::Message>, JoinHandle<()>) {
    let (tx, rx) = channel::<message::Message>(32);
    let mut engine = engine::Engine::new(rx, tx.clone());
    let handle = tokio::spawn(async move {
        engine.init().await;
        engine.work().await;
    });
    (tx.clone(), handle)
}
