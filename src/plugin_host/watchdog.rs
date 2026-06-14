use maolan_plugin_host::protocol::ShmHeader;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

pub struct Watchdog {
    last_heartbeat: u32,
    last_check: Instant,
    timeout: Duration,

    pub failure_count: u32,
}

impl Watchdog {
    pub fn new(timeout: Duration) -> Self {
        Self {
            last_heartbeat: 0,
            last_check: Instant::now(),
            timeout,
            failure_count: 0,
        }
    }

    pub fn is_alive(&mut self, header: &ShmHeader) -> bool {
        let now = Instant::now();
        if now.duration_since(self.last_check) < self.timeout {
            return true;
        }

        let current = header.heartbeat.load(Ordering::Acquire);
        if current != self.last_heartbeat {
            self.last_heartbeat = current;
            self.last_check = now;
            self.failure_count = 0;
            true
        } else {
            self.failure_count += 1;
            false
        }
    }
}
