pub const HW_PROFILE_ENV: &str = "MAOLAN_HW_PROFILE";
#[cfg(target_os = "freebsd")]
pub const OSS_ASSIST_AUTONOMOUS_ENV: &str = "MAOLAN_OSS_ASSIST_AUTONOMOUS";
#[cfg(target_os = "linux")]
pub const ALSA_ASSIST_AUTONOMOUS_ENV: &str = "MAOLAN_ALSA_ASSIST_AUTONOMOUS";
#[cfg(target_os = "openbsd")]
pub const SNDIO_ASSIST_AUTONOMOUS_ENV: &str = "MAOLAN_SNDIO_ASSIST_AUTONOMOUS";
#[cfg(target_os = "windows")]
pub const WASAPI_ASSIST_AUTONOMOUS_ENV: &str = "MAOLAN_WASAPI_ASSIST_AUTONOMOUS";
#[cfg(target_os = "macos")]
pub const COREAUDIO_ASSIST_AUTONOMOUS_ENV: &str = "MAOLAN_COREAUDIO_ASSIST_AUTONOMOUS";

pub fn env_flag(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .map(|v| {
            let s = v.trim().to_ascii_lowercase();
            s == "1" || s == "true" || s == "yes" || s == "on"
        })
        .unwrap_or(false)
}
