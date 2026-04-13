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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Use a mutex to prevent tests from running in parallel and interfering with each other
    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn env_flag_returns_false_for_missing_var() {
        let _guard = ENV_MUTEX.lock().unwrap();
        assert!(!env_flag("MAOLAN_TEST_NONEXISTENT_VAR_12345"));
    }

    #[test]
    fn env_flag_returns_true_for_one() {
        let _guard = ENV_MUTEX.lock().unwrap();
        unsafe {
            std::env::set_var("MAOLAN_TEST_FLAG_ONE", "1");
        }
        assert!(env_flag("MAOLAN_TEST_FLAG_ONE"));
        unsafe {
            std::env::remove_var("MAOLAN_TEST_FLAG_ONE");
        }
    }

    #[test]
    fn env_flag_returns_true_for_true() {
        let _guard = ENV_MUTEX.lock().unwrap();
        unsafe {
            std::env::set_var("MAOLAN_TEST_FLAG_TRUE", "true");
        }
        assert!(env_flag("MAOLAN_TEST_FLAG_TRUE"));
        unsafe {
            std::env::remove_var("MAOLAN_TEST_FLAG_TRUE");
        }
    }

    #[test]
    fn env_flag_returns_true_for_yes() {
        let _guard = ENV_MUTEX.lock().unwrap();
        unsafe {
            std::env::set_var("MAOLAN_TEST_FLAG_YES", "yes");
        }
        assert!(env_flag("MAOLAN_TEST_FLAG_YES"));
        unsafe {
            std::env::remove_var("MAOLAN_TEST_FLAG_YES");
        }
    }

    #[test]
    fn env_flag_returns_true_for_on() {
        let _guard = ENV_MUTEX.lock().unwrap();
        unsafe {
            std::env::set_var("MAOLAN_TEST_FLAG_ON", "on");
        }
        assert!(env_flag("MAOLAN_TEST_FLAG_ON"));
        unsafe {
            std::env::remove_var("MAOLAN_TEST_FLAG_ON");
        }
    }

    #[test]
    fn env_flag_returns_true_for_uppercase() {
        let _guard = ENV_MUTEX.lock().unwrap();
        unsafe {
            std::env::set_var("MAOLAN_TEST_FLAG_UPPER", "TRUE");
        }
        assert!(env_flag("MAOLAN_TEST_FLAG_UPPER"));
        unsafe {
            std::env::remove_var("MAOLAN_TEST_FLAG_UPPER");
        }
    }

    #[test]
    fn env_flag_returns_true_for_mixed_case() {
        let _guard = ENV_MUTEX.lock().unwrap();
        unsafe {
            std::env::set_var("MAOLAN_TEST_FLAG_MIXED", "True");
        }
        assert!(env_flag("MAOLAN_TEST_FLAG_MIXED"));
        unsafe {
            std::env::remove_var("MAOLAN_TEST_FLAG_MIXED");
        }
    }

    #[test]
    fn env_flag_returns_false_for_zero() {
        let _guard = ENV_MUTEX.lock().unwrap();
        unsafe {
            std::env::set_var("MAOLAN_TEST_FLAG_ZERO", "0");
        }
        assert!(!env_flag("MAOLAN_TEST_FLAG_ZERO"));
        unsafe {
            std::env::remove_var("MAOLAN_TEST_FLAG_ZERO");
        }
    }

    #[test]
    fn env_flag_returns_false_for_false() {
        let _guard = ENV_MUTEX.lock().unwrap();
        unsafe {
            std::env::set_var("MAOLAN_TEST_FLAG_FALSE", "false");
        }
        assert!(!env_flag("MAOLAN_TEST_FLAG_FALSE"));
        unsafe {
            std::env::remove_var("MAOLAN_TEST_FLAG_FALSE");
        }
    }

    #[test]
    fn env_flag_returns_false_for_empty() {
        let _guard = ENV_MUTEX.lock().unwrap();
        unsafe {
            std::env::set_var("MAOLAN_TEST_FLAG_EMPTY", "");
        }
        assert!(!env_flag("MAOLAN_TEST_FLAG_EMPTY"));
        unsafe {
            std::env::remove_var("MAOLAN_TEST_FLAG_EMPTY");
        }
    }

    #[test]
    fn env_flag_trims_whitespace() {
        let _guard = ENV_MUTEX.lock().unwrap();
        unsafe {
            std::env::set_var("MAOLAN_TEST_FLAG_TRIM", "  true  ");
        }
        assert!(env_flag("MAOLAN_TEST_FLAG_TRIM"));
        unsafe {
            std::env::remove_var("MAOLAN_TEST_FLAG_TRIM");
        }
    }

    #[test]
    fn hw_profile_env_constant() {
        assert_eq!(HW_PROFILE_ENV, "MAOLAN_HW_PROFILE");
    }
}
