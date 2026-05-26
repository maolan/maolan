pub mod host;
pub mod interfaces;
pub mod midi;
pub mod port;
pub mod processor;
pub mod state;

pub use host::{Vst3Host, Vst3PluginInfo};
pub use midi::EventBuffer;
pub use port::{BusInfo, PortBinding};
pub use processor::{Vst3Processor, Vst3TransportInfo};
pub use state::{MemoryStream, Vst3PluginState, ibstream_ptr};

pub use processor::list_plugins;

#[cfg(any(
    target_os = "macos",
    target_os = "linux",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "windows"
))]
use crate::paths;
use std::path::PathBuf;

pub fn default_vst3_search_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    #[cfg(target_os = "macos")]
    {
        paths::push_macos_audio_plugin_roots(&mut roots, "VST3");
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
    {
        paths::push_unix_plugin_roots(&mut roots, "vst3");
    }

    #[cfg(target_os = "windows")]
    {
        paths::push_windows_vst3_roots(&mut roots);
    }

    roots
}
