#[derive(Debug, Clone, Copy)]
pub struct HwOptions {
    pub exclusive: bool,
    pub period_frames: usize,
    pub nperiods: usize,
    pub ignore_hwbuf: bool,
    pub sync_mode: bool,
    pub input_latency_frames: usize,
    pub output_latency_frames: usize,
}

#[cfg(target_os = "macos")]
impl Default for HwOptions {
    fn default() -> Self {
        Self {
            exclusive: false,
            period_frames: 512,
            nperiods: 1,
            ignore_hwbuf: false,
            sync_mode: true,
            input_latency_frames: 0,
            output_latency_frames: 0,
        }
    }
}
