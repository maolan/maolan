use super::hw_worker::Backend;
use crate::hw::config;
use crate::hw::netbsd_audio;

#[derive(Debug)]
pub struct NetBsdAudioBackend;

impl Backend for NetBsdAudioBackend {
    type Driver = netbsd_audio::HwDriver;
    type MidiHub = netbsd_audio::MidiHub;

    const LABEL: &'static str = "audio(4)";
    const WORKER_THREAD_NAME: &'static str = "audio4-worker";
    const ASSIST_THREAD_NAME: &'static str = "audio4-assist";
    const ASSIST_AUTONOMOUS_ENV: &'static str = config::NETBSD_AUDIO_ASSIST_AUTONOMOUS_ENV;
}

pub type HwWorker = super::hw_worker::HwWorker<NetBsdAudioBackend>;
