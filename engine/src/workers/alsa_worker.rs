use super::hw_worker::Backend;
use crate::hw::alsa;
use crate::hw::config;

#[derive(Debug)]
pub struct AlsaBackend;

impl Backend for AlsaBackend {
    type Driver = alsa::HwDriver;
    type MidiHub = alsa::MidiHub;

    const LABEL: &'static str = "ALSA";
    const WORKER_THREAD_NAME: &'static str = "alsa-worker";
    const ASSIST_THREAD_NAME: &'static str = "alsa-assist";
    const ASSIST_AUTONOMOUS_ENV: &'static str = config::ALSA_ASSIST_AUTONOMOUS_ENV;
}

pub type HwWorker = super::hw_worker::HwWorker<AlsaBackend>;
