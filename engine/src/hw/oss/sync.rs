use nix::libc;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock, Weak},
};

#[derive(Debug, Clone, Copy)]
pub(super) struct Correction {
    pub(super) loss_min: i64,
    pub(super) loss_max: i64,
    pub(super) drift_min: i64,
    pub(super) drift_max: i64,
    pub(super) correction: i64,
}

impl Default for Correction {
    fn default() -> Self {
        Self {
            loss_min: -128,
            loss_max: 128,
            drift_min: -64,
            drift_max: 64,
            correction: 0,
        }
    }
}

impl Correction {
    pub(super) fn set_drift_limits(&mut self, min: i64, max: i64) {
        self.drift_min = min.min(max);
        self.drift_max = min.max(max);
    }

    pub(super) fn set_loss_limits(&mut self, min: i64, max: i64) {
        self.loss_min = min.min(max);
        self.loss_max = min.max(max);
    }

    pub(super) fn clear(&mut self) {
        self.correction = 0;
    }

    pub(super) fn correction(&self) -> i64 {
        self.correction
    }

    pub(super) fn correct(&mut self, balance: i64, target: i64) -> i64 {
        let corrected = balance - target + self.correction;
        if corrected > self.loss_max {
            self.correction -= corrected - self.loss_max;
        } else if corrected < self.loss_min {
            self.correction += self.loss_min - corrected;
        } else if corrected > self.drift_max {
            self.correction -= 1;
        } else if corrected < self.drift_min {
            self.correction += 1;
        }
        self.correction
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct DuplexSync {
    pub(super) correction: Correction,
    pub(super) capture_balance: Option<i64>,
    pub(super) playback_balance: Option<i64>,
    pub(super) cycle_end: i64,
    pub(super) playback_prefill_frames: i64,
    pub(super) clock_zero: Option<libc::timespec>,
}

impl DuplexSync {
    pub(super) fn new(sample_rate: i32, buffer_frames: usize) -> Self {
        let mut correction = Correction::default();
        let drift_limit = (sample_rate as i64 / 1000).max(1);
        correction.set_drift_limits(-drift_limit, drift_limit);
        let loss_limit = drift_limit.max(buffer_frames as i64 / 2);
        correction.set_loss_limits(-loss_limit, loss_limit);
        Self {
            correction,
            capture_balance: None,
            playback_balance: None,
            cycle_end: 0,
            playback_prefill_frames: 0,
            clock_zero: None,
        }
    }
}

fn duplex_registry() -> &'static Mutex<HashMap<String, Weak<Mutex<DuplexSync>>>> {
    static REG: OnceLock<Mutex<HashMap<String, Weak<Mutex<DuplexSync>>>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(super) fn get_or_create_duplex_sync(
    path: &str,
    sample_rate: i32,
    buffer_frames: usize,
) -> Arc<Mutex<DuplexSync>> {
    let reg = duplex_registry();
    let mut map = reg.lock().expect("duplex registry poisoned");
    if let Some(existing) = map.get(path).and_then(Weak::upgrade) {
        return existing;
    }
    let created = Arc::new(Mutex::new(DuplexSync::new(sample_rate, buffer_frames)));
    map.insert(path.to_string(), Arc::downgrade(&created));
    created
}

#[derive(Debug, Clone, Copy)]
pub(super) struct FrameClock {
    pub(super) zero: libc::timespec,
    pub(super) sample_rate: u32,
}

impl Default for FrameClock {
    fn default() -> Self {
        Self {
            zero: libc::timespec {
                tv_sec: 0,
                tv_nsec: 0,
            },
            sample_rate: 48_000,
        }
    }
}

impl FrameClock {
    pub(super) fn set_sample_rate(&mut self, sample_rate: u32) -> bool {
        if sample_rate == 0 {
            return false;
        }
        self.sample_rate = sample_rate;
        true
    }

    pub(super) fn init_clock(&mut self, sample_rate: u32) -> bool {
        if !self.set_sample_rate(sample_rate) {
            return false;
        }
        unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut self.zero) == 0 }
    }

    pub(super) fn now(&self) -> Option<i64> {
        let mut now = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        let ok = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut now) == 0 };
        if !ok {
            return None;
        }
        let ns = (now.tv_sec - self.zero.tv_sec) as i128 * 1_000_000_000_i128
            + (now.tv_nsec - self.zero.tv_nsec) as i128;
        Some(((ns * self.sample_rate as i128) / 1_000_000_000_i128) as i64)
    }

    pub(super) fn sleep_until_frame(&self, frame: i64) -> bool {
        let ns = self.frames_to_time(frame);
        let wake = libc::timespec {
            tv_sec: self.zero.tv_sec + (self.zero.tv_nsec + ns) / 1_000_000_000,
            tv_nsec: (self.zero.tv_nsec + ns) % 1_000_000_000,
        };
        unsafe {
            libc::clock_nanosleep(
                libc::CLOCK_MONOTONIC,
                libc::TIMER_ABSTIME,
                &wake,
                std::ptr::null_mut(),
            ) == 0
        }
    }

    fn frames_to_time(&self, frames: i64) -> i64 {
        (frames.saturating_mul(1_000_000_000_i64)) / self.sample_rate as i64
    }

    pub(super) fn stepping(&self) -> i64 {
        16_i64 * (1 + (self.sample_rate as i64 / 50_000))
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ChannelState {
    pub(super) last_processing: i64,
    pub(super) last_sync: i64,
    pub(super) last_progress: i64,
    pub(super) balance: i64,
    pub(super) min_progress: i64,
    pub(super) max_progress: i64,
    pub(super) total_loss: i64,
    pub(super) sync_level: u32,
}

impl Default for ChannelState {
    fn default() -> Self {
        Self {
            last_processing: 0,
            last_sync: 0,
            last_progress: 0,
            balance: 0,
            min_progress: 0,
            max_progress: 0,
            total_loss: 0,
            sync_level: 8,
        }
    }
}

impl ChannelState {
    pub(super) fn freewheel(&self) -> bool {
        self.sync_level > 4
    }

    pub(super) fn full_resync(&self) -> bool {
        self.sync_level > 2
    }

    pub(super) fn resync(&self) -> bool {
        self.sync_level > 0
    }

    pub(super) fn mark_progress(&mut self, progress: i64, now: i64, stepping: i64) {
        if progress > 0 {
            if self.freewheel() {
                self.last_progress = now - progress - self.balance;
                if now <= self.last_processing + stepping {
                    self.sync_level = self.sync_level.saturating_sub(1);
                }
            } else if now <= self.last_processing + stepping {
                self.balance = now - (self.last_progress + progress);
                self.last_sync = now;
                if self.sync_level > 0 {
                    self.sync_level -= 1;
                }
                if progress < self.min_progress || self.min_progress == 0 {
                    self.min_progress = progress;
                }
                if progress > self.max_progress {
                    self.max_progress = progress;
                }
            } else {
                self.sync_level += 1;
            }
            self.last_progress += progress;
        }
        self.last_processing = now;
    }

    pub(super) fn mark_loss_from(&mut self, progress: i64, now: i64) -> i64 {
        let loss = (now - self.balance) - (self.last_progress + progress);
        self.mark_loss(loss)
    }

    pub(super) fn mark_loss(&mut self, loss: i64) -> i64 {
        if loss > 0 {
            self.total_loss += loss;
            self.sync_level = self.sync_level.max(6);
            loss
        } else {
            0
        }
    }

    pub(super) fn next_min_progress(&self) -> i64 {
        self.last_progress + self.min_progress + self.balance
    }

    pub(super) fn safe_wakeup(&self, oss_available: i64, buffer_frames: i64) -> i64 {
        self.next_min_progress() + buffer_frames - oss_available - self.max_progress
    }

    pub(super) fn estimated_dropout(&self, oss_available: i64, buffer_frames: i64) -> i64 {
        self.last_progress + self.balance + buffer_frames - oss_available
    }

    pub(super) fn wakeup_time(
        &self,
        sync_target: i64,
        oss_available: i64,
        buffer_frames: i64,
        stepping: i64,
    ) -> i64 {
        let mut wakeup = self.last_processing + stepping;
        if self.freewheel() || self.full_resync() {
        } else if self.resync() || wakeup + self.max_progress > sync_target {
            if self.next_min_progress() > wakeup {
                wakeup = self.next_min_progress() - stepping;
            } else if self.next_min_progress() > self.last_processing {
                wakeup = self.next_min_progress();
            }
        } else {
            wakeup = sync_target - self.max_progress;
        }

        if sync_target > self.last_processing && sync_target < wakeup {
            wakeup = sync_target;
        }

        let safe = self.safe_wakeup(oss_available, buffer_frames);
        if safe < wakeup {
            wakeup = safe.max(self.last_processing + stepping);
        }
        wakeup
    }
}
