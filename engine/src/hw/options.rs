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
