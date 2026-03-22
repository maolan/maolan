use super::hw_worker::Backend;
use crate::hw::config;
use crate::hw::sndio;

#[derive(Debug)]
pub struct SndioBackend;

impl Backend for SndioBackend {
    type Driver = sndio::HwDriver;
    type MidiHub = sndio::MidiHub;

    const LABEL: &'static str = "sndio";
    const WORKER_THREAD_NAME: &'static str = "sndio-worker";
    const ASSIST_THREAD_NAME: &'static str = "sndio-assist";
    const ASSIST_AUTONOMOUS_ENV: &'static str = config::SNDIO_ASSIST_AUTONOMOUS_ENV;
}

pub type HwWorker = super::hw_worker::HwWorker<SndioBackend>;
