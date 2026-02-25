#[cfg(target_os = "linux")]
mod alsa_worker;
mod audio;
pub mod client;
#[cfg(target_os = "macos")]
mod coreaudio_worker;
mod engine;
mod hw;
mod hw_worker;
pub mod kind;
pub mod lv2;
pub mod message;
mod midi;
pub mod mutex;
#[cfg(target_os = "freebsd")]
mod oss_worker;
mod routing;
#[cfg(target_os = "openbsd")]
mod sndio_worker;
pub mod state;
mod track;
#[cfg(target_os = "windows")]
mod wasapi_worker;
pub mod worker;

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
