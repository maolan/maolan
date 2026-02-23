use crate::hw::config;
use crate::hw::wasapi;
use crate::hw_worker::{Backend, HwWorker as GenericHwWorker};

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

pub type HwWorker = GenericHwWorker<WasapiBackend>;
