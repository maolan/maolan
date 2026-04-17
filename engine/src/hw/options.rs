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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hw_options_default_values() {
        let opts: HwOptions = Default::default();
        // Default values are platform-specific
        // macOS: exclusive=false, period_frames=512, nperiods=1, sync_mode=true
        // OSS (FreeBSD/Linux): exclusive=false, period_frames=1024, nperiods=1, sync_mode=false
        assert!(!opts.ignore_hwbuf);
        assert_eq!(opts.input_latency_frames, 0);
        assert_eq!(opts.output_latency_frames, 0);
    }

    #[test]
    fn hw_options_clone() {
        let opts = HwOptions {
            exclusive: true,
            period_frames: 512,
            nperiods: 3,
            ignore_hwbuf: true,
            sync_mode: false,
            input_latency_frames: 100,
            output_latency_frames: 200,
        };
        let cloned = opts;
        assert_eq!(opts.exclusive, cloned.exclusive);
        assert_eq!(opts.period_frames, cloned.period_frames);
        assert_eq!(opts.nperiods, cloned.nperiods);
        assert_eq!(opts.ignore_hwbuf, cloned.ignore_hwbuf);
        assert_eq!(opts.sync_mode, cloned.sync_mode);
        assert_eq!(opts.input_latency_frames, cloned.input_latency_frames);
        assert_eq!(opts.output_latency_frames, cloned.output_latency_frames);
    }

    #[test]
    fn hw_options_copy() {
        let opts = HwOptions {
            exclusive: true,
            period_frames: 512,
            nperiods: 3,
            ignore_hwbuf: true,
            sync_mode: false,
            input_latency_frames: 100,
            output_latency_frames: 200,
        };
        let copied = opts;
        assert_eq!(opts.exclusive, copied.exclusive);
        assert_eq!(opts.period_frames, copied.period_frames);
    }
}
