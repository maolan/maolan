//! Watchdog for monitoring plugin-host processes and recovering from crashes.

use maolan_plugin_host::protocol::ShmHeader;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

/// Monitors a plugin-host process via the shared-memory heartbeat atomic.
pub struct Watchdog {
    last_heartbeat: u32,
    last_check: Instant,
    timeout: Duration,
    /// Number of consecutive timeouts / crashes.
    pub failure_count: u32,
}

impl Watchdog {
    /// Create a new watchdog with the given timeout.
    /// A typical audio-block timeout is 2× the expected block duration
    /// (e.g. ~11 ms at 256 samples / 48 kHz).
    pub fn new(timeout: Duration) -> Self {
        Self {
            last_heartbeat: 0,
            last_check: Instant::now(),
            timeout,
            failure_count: 0,
        }
    }

    /// Check the heartbeat in `header`. Returns `true` if the host is alive.
    /// Returns `false` if the heartbeat has not changed since the last check
    /// and the timeout has expired.
    pub fn is_alive(&mut self, header: &ShmHeader) -> bool {
        let now = Instant::now();
        if now.duration_since(self.last_check) < self.timeout {
            // Too soon to declare dead; assume alive.
            return true;
        }

        let current = header.heartbeat.load(Ordering::Acquire);
        if current != self.last_heartbeat {
            // Heartbeat changed since last check → host is alive.
            self.last_heartbeat = current;
            self.last_check = now;
            self.failure_count = 0;
            true
        } else {
            // Heartbeat unchanged and timeout expired → dead.
            self.failure_count += 1;
            false
        }
    }
}
