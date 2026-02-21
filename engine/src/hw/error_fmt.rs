#[cfg(target_os = "linux")]
pub fn backend_open_error(backend: &str, direction: &str, device: &str, err: impl std::fmt::Display) -> String {
    format!("Failed to open {backend} {direction} '{device}': {err}")
}

#[cfg(target_os = "linux")]
pub fn backend_io_error(backend: &str, direction: &str, err: impl std::fmt::Display) -> String {
    format!("{backend} {direction} io error: {err}")
}

#[cfg(target_os = "linux")]
pub fn backend_rw_error(backend: &str, direction: &str, op: &str, err: impl std::fmt::Display) -> String {
    format!("{backend} {direction} {op} failed: {err}")
}
