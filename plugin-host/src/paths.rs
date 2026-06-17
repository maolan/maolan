use std::path::PathBuf;

pub fn home_dir() -> String {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default()
}

pub fn push_macos_audio_plugin_roots(roots: &mut Vec<PathBuf>, plugin_dir_name: &str) {
    roots.push(PathBuf::from(format!(
        "/Library/Audio/Plug-Ins/{plugin_dir_name}"
    )));
    roots.push(PathBuf::from(format!(
        "{}/Library/Audio/Plug-Ins/{plugin_dir_name}",
        home_dir()
    )));
}

pub fn push_unix_plugin_roots(roots: &mut Vec<PathBuf>, plugin_dir_name: &str) {
    roots.push(PathBuf::from(format!("/usr/lib/{plugin_dir_name}")));
    roots.push(PathBuf::from(format!("/usr/lib64/{plugin_dir_name}")));
    roots.push(PathBuf::from(format!("/usr/local/lib/{plugin_dir_name}")));
    roots.push(PathBuf::from(format!("/usr/local/lib64/{plugin_dir_name}")));
    roots.push(PathBuf::from(format!("{}/.{plugin_dir_name}", home_dir())));
    roots.push(PathBuf::from(format!(
        "{}/.local/lib/{plugin_dir_name}",
        home_dir()
    )));
}

pub fn push_windows_vst3_roots(roots: &mut Vec<PathBuf>) {
    roots.push(PathBuf::from(r"C:\Program Files\Common Files\VST3"));
    roots.push(PathBuf::from(r"C:\Program Files (x86)\Common Files\VST3"));
}

pub fn push_windows_clap_roots(roots: &mut Vec<PathBuf>) {
    if let Ok(common) = std::env::var("COMMONPROGRAMFILES") {
        roots.push(PathBuf::from(common).join("CLAP"));
    }
    if let Ok(common_x86) = std::env::var("COMMONPROGRAMFILES(X86)") {
        roots.push(PathBuf::from(common_x86).join("CLAP"));
    }
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        roots.push(PathBuf::from(local_app_data).join(r"Programs\Common\CLAP"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{LazyLock, Mutex};

    static ENV_GUARD: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    #[test]
    fn home_dir_prefers_home_over_userprofile() {
        let _guard = ENV_GUARD.lock().expect("lock env guard");
        let old_home = std::env::var("HOME").ok();
        let old_userprofile = std::env::var("USERPROFILE").ok();

        unsafe {
            std::env::set_var("HOME", "/home/tester");
            std::env::set_var("USERPROFILE", "C:/Users/tester");
        }

        let home = home_dir();

        if let Some(value) = old_home {
            unsafe { std::env::set_var("HOME", value) };
        } else {
            unsafe { std::env::remove_var("HOME") };
        }
        if let Some(value) = old_userprofile {
            unsafe { std::env::set_var("USERPROFILE", value) };
        } else {
            unsafe { std::env::remove_var("USERPROFILE") };
        }

        assert_eq!(home, "/home/tester");
    }

    #[test]
    fn push_unix_plugin_roots_adds_system_and_user_locations() {
        let _guard = ENV_GUARD.lock().expect("lock env guard");
        let old_home = std::env::var("HOME").ok();
        unsafe {
            std::env::set_var("HOME", "/home/tester");
        }

        let mut roots = Vec::new();
        push_unix_plugin_roots(&mut roots, "clap");

        if let Some(value) = old_home {
            unsafe { std::env::set_var("HOME", value) };
        } else {
            unsafe { std::env::remove_var("HOME") };
        }

        assert_eq!(
            roots,
            vec![
                PathBuf::from("/usr/lib/clap"),
                PathBuf::from("/usr/lib64/clap"),
                PathBuf::from("/usr/local/lib/clap"),
                PathBuf::from("/usr/local/lib64/clap"),
                PathBuf::from("/home/tester/.clap"),
                PathBuf::from("/home/tester/.local/lib/clap"),
            ]
        );
    }

    #[test]
    fn push_windows_vst3_roots_adds_standard_locations() {
        let mut roots = Vec::new();

        push_windows_vst3_roots(&mut roots);

        assert_eq!(
            roots,
            vec![
                PathBuf::from(r"C:\Program Files\Common Files\VST3"),
                PathBuf::from(r"C:\Program Files (x86)\Common Files\VST3"),
            ]
        );
    }

    #[test]
    fn push_windows_clap_roots_adds_standard_locations() {
        let _guard = ENV_GUARD.lock().expect("lock env guard");
        let old_common = std::env::var("COMMONPROGRAMFILES").ok();
        let old_common_x86 = std::env::var("COMMONPROGRAMFILES(X86)").ok();
        let old_local_app_data = std::env::var("LOCALAPPDATA").ok();

        unsafe {
            std::env::set_var("COMMONPROGRAMFILES", r"C:\Program Files\Common Files");
            std::env::set_var(
                "COMMONPROGRAMFILES(X86)",
                r"C:\Program Files (x86)\Common Files",
            );
            std::env::set_var("LOCALAPPDATA", r"C:\Users\tester\AppData\Local");
        }

        let mut roots = Vec::new();
        push_windows_clap_roots(&mut roots);

        if let Some(value) = old_common {
            unsafe { std::env::set_var("COMMONPROGRAMFILES", value) };
        } else {
            unsafe { std::env::remove_var("COMMONPROGRAMFILES") };
        }
        if let Some(value) = old_common_x86 {
            unsafe { std::env::set_var("COMMONPROGRAMFILES(X86)", value) };
        } else {
            unsafe { std::env::remove_var("COMMONPROGRAMFILES(X86)") };
        }
        if let Some(value) = old_local_app_data {
            unsafe { std::env::set_var("LOCALAPPDATA", value) };
        } else {
            unsafe { std::env::remove_var("LOCALAPPDATA") };
        }

        assert_eq!(
            roots,
            vec![
                PathBuf::from(r"C:\Program Files\Common Files").join("CLAP"),
                PathBuf::from(r"C:\Program Files (x86)\Common Files").join("CLAP"),
                PathBuf::from(r"C:\Users\tester\AppData\Local").join(r"Programs\Common\CLAP"),
            ]
        );
    }
}
