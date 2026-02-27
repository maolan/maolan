use super::default_vst3_search_roots;
use super::interfaces::PluginFactory;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vst3PluginInfo {
    pub id: String, // FUID as string
    pub name: String,
    pub vendor: String,
    pub path: String, // Path to .vst3 bundle
    pub category: String,
    pub version: String,
    pub audio_inputs: usize,
    pub audio_outputs: usize,
    pub has_midi_input: bool,
    pub has_midi_output: bool,
}

pub struct Vst3Host {
    plugins: Vec<Vst3PluginInfo>,
}

impl Vst3Host {
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    pub fn list_plugins(&mut self) -> Vec<Vst3PluginInfo> {
        let mut roots = default_vst3_search_roots();

        // Add paths from VST3_PATH environment variable
        if let Ok(extra) = std::env::var("VST3_PATH") {
            for p in std::env::split_paths(&extra) {
                if !p.as_os_str().is_empty() {
                    roots.push(p);
                }
            }
        }

        let mut out = Vec::new();
        for root in roots {
            collect_vst3_plugins(&root, &mut out);
        }

        // Sort by name
        out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        // Deduplicate by path
        out.dedup_by(|a, b| a.path.eq_ignore_ascii_case(&b.path));

        self.plugins = out.clone();
        out
    }

    pub fn get_plugin_info(&self, path: &str) -> Option<&Vst3PluginInfo> {
        self.plugins.iter().find(|p| p.path == path)
    }
}

fn collect_vst3_plugins(root: &Path, out: &mut Vec<Vst3PluginInfo>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else {
            continue;
        };

        if !ft.is_dir() {
            continue;
        }

        // Check if this is a .vst3 bundle
        if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("vst3"))
        {
            if let Some(info) = scan_vst3_bundle(&path) {
                out.push(info);
            }
        } else {
            // Recurse into subdirectories
            collect_vst3_plugins(&path, out);
        }
    }
}

fn scan_vst3_bundle(bundle_path: &Path) -> Option<Vst3PluginInfo> {
    // Try to load the plugin factory
    let factory = PluginFactory::from_module(bundle_path).ok()?;

    let class_count = factory.count_classes();
    if class_count == 0 {
        return None;
    }

    // Get the first class (most plugins have just one)
    let class_info = factory.get_class_info(0)?;

    // Convert TUID to string
    let id = tuid_to_string(&class_info.cid);

    Some(Vst3PluginInfo {
        id,
        name: class_info.name,
        vendor: String::new(), // We'd need IPluginFactory2 for vendor
        path: bundle_path.to_string_lossy().to_string(),
        category: class_info.category,
        version: String::new(), // We'd need to parse class_info.version
        audio_inputs: 0,        // TODO: Query bus info
        audio_outputs: 0,       // TODO: Query bus info
        has_midi_input: false,  // TODO: Query event bus info
        has_midi_output: false, // TODO: Query event bus info
    })
}

fn tuid_to_string(tuid: &[i8; 16]) -> String {
    // Convert TUID to hexadecimal string
    tuid.iter()
        .map(|&b| format!("{:02X}", b as u8))
        .collect::<Vec<_>>()
        .join("")
}

// Standalone function for backward compatibility with old API
pub fn list_plugins() -> Vec<Vst3PluginInfo> {
    let mut host = Vst3Host::new();
    host.list_plugins()
}
