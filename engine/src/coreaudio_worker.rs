#![cfg(target_os = "macos")]

use crate::hw::config;
use crate::hw::coreaudio;
use crate::hw_worker::{Backend, HwWorker as GenericHwWorker};

#[derive(Debug)]
pub struct CoreAudioBackend;

impl Backend for CoreAudioBackend {
    type Driver = coreaudio::HwDriver;
    type MidiHub = coreaudio::MidiHub;

    const LABEL: &'static str = "CoreAudio";
    const WORKER_THREAD_NAME: &'static str = "coreaudio-worker";
    const ASSIST_THREAD_NAME: &'static str = "coreaudio-assist";
    const ASSIST_AUTONOMOUS_ENV: &'static str = config::COREAUDIO_ASSIST_AUTONOMOUS_ENV;
}

pub type HwWorker = GenericHwWorker<CoreAudioBackend>;
