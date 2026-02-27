pub mod host;
pub mod interfaces;
pub mod midi;
pub mod port;
pub mod processor;
pub mod state;

// Re-export main types
pub use host::{Vst3Host, Vst3PluginInfo};
pub use midi::EventBuffer;
pub use port::{BusInfo, PortBinding};
pub use processor::Vst3Processor;
pub use state::{MemoryStream, Vst3PluginState};

// Re-export from old vst3.rs for backward compatibility
pub use processor::list_plugins;

// Helper for VST3 search paths (moved from old vst3.rs)
use std::path::PathBuf;

pub fn default_vst3_search_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    #[cfg(target_os = "windows")]
    {
        roots.push(PathBuf::from(r"C:\Program Files\Common Files\VST3"));
        roots.push(PathBuf::from(r"C:\Program Files (x86)\Common Files\VST3"));
    }

    #[cfg(target_os = "macos")]
    {
        roots.push(PathBuf::from("/Library/Audio/Plug-Ins/VST3"));
        roots.push(PathBuf::from(format!(
            "{}/Library/Audio/Plug-Ins/VST3",
            home_dir()
        )));
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    {
        roots.push(PathBuf::from("/usr/lib/vst3"));
        roots.push(PathBuf::from("/usr/lib64/vst3"));
        roots.push(PathBuf::from("/usr/local/lib/vst3"));
        roots.push(PathBuf::from("/usr/local/lib64/vst3"));
        roots.push(PathBuf::from(format!("{}/.vst3", home_dir())));
        roots.push(PathBuf::from(format!("{}/.local/lib/vst3", home_dir())));
    }

    roots
}

fn home_dir() -> String {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default()
}
