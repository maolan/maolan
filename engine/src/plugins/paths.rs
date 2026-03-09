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
