//! Opt-in GUI timing diagnostics. Avoid formatting complete messages: they
//! may contain entire sessions or waveform buffers.
use std::sync::OnceLock;
use std::time::Instant;

pub(super) fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("MAOLAN_UI_TRACE").is_ok_and(|v| v != "0"))
}

pub(super) struct Span {
    started: Option<Instant>,
    operation: &'static str,
    message: Option<std::mem::Discriminant<crate::message::Message>>,
}

impl Span {
    pub(super) fn new(operation: &'static str, message: Option<&crate::message::Message>) -> Self {
        Self {
            started: enabled().then(Instant::now),
            operation,
            message: message.map(std::mem::discriminant),
        }
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        if let Some(started) = self.started {
            let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
            if elapsed_ms >= 8.0 {
                tracing::info!(operation = self.operation, message = ?self.message, elapsed_ms,
                    "ui_trace slow operation");
            }
        }
    }
}

pub(super) fn playback_tick(sample: f64, running: bool) {
    if !enabled() {
        return;
    }
    // GUI updates run on one thread, so tracing does not need a shared lock.
    thread_local! {
        static LAST_TICK: std::cell::Cell<Option<(Instant, f64)>> = const { std::cell::Cell::new(None) };
    }
    LAST_TICK.with(|last| {
        let now = Instant::now();
        let previous = last.replace(running.then_some((now, sample)));
        tracing::info!(
            sample,
            running,
            tick_gap_ms = previous.map(|(at, _)| now.duration_since(at).as_secs_f64() * 1000.0),
            sample_delta = previous.map(|(_, previous)| sample - previous),
            "ui_trace playback tick"
        );
    });
}
