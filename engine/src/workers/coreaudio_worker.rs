#![cfg(target_os = "macos")]

use super::hw_worker::Backend;
use crate::hw::config;
use crate::hw::coreaudio;

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

pub type HwWorker = super::hw_worker::HwWorker<CoreAudioBackend>;
