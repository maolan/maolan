use super::hw_worker::Backend;
use crate::hw::config;
use crate::hw::wasapi;

#[derive(Debug)]
pub struct WasapiBackend;

impl Backend for WasapiBackend {
    type Driver = wasapi::HwDriver;
    type MidiHub = wasapi::MidiHub;

    const LABEL: &'static str = "WASAPI";
    const WORKER_THREAD_NAME: &'static str = "wasapi-worker";
    const ASSIST_THREAD_NAME: &'static str = "wasapi-assist";
    const ASSIST_AUTONOMOUS_ENV: &'static str = config::WASAPI_ASSIST_AUTONOMOUS_ENV;
}

pub type HwWorker = super::hw_worker::HwWorker<WasapiBackend>;
