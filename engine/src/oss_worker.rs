use crate::hw::config;
use crate::hw::oss;
use crate::hw_worker::{Backend, HwWorker as GenericHwWorker};

#[derive(Debug)]
pub struct OssBackend;

impl Backend for OssBackend {
    type Driver = oss::HwDriver;
    type MidiHub = oss::MidiHub;

    const LABEL: &'static str = "OSS";
    const WORKER_THREAD_NAME: &'static str = "oss-worker";
    const ASSIST_THREAD_NAME: &'static str = "oss-assist";
    const ASSIST_AUTONOMOUS_ENV: &'static str = config::OSS_ASSIST_AUTONOMOUS_ENV;
}

pub type HwWorker = GenericHwWorker<OssBackend>;
