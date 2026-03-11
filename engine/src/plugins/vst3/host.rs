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

impl Default for Vst3Host {
    fn default() -> Self {
        Self::new()
    }
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
        if ft.is_symlink() {
            continue;
        }

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
    let factory = PluginFactory::from_module(bundle_path).ok()?;
    let class_count = factory.count_classes();
    if class_count == 0 {
        return None;
    }

    let mut fallback = None;
    for index in 0..class_count {
        let Some(class_info) = factory.get_class_info(index) else {
            continue;
        };
        if fallback.is_none() {
            fallback = Some(class_info_to_plugin_info(&class_info, bundle_path, None));
        }

        let Ok(mut instance) = factory.create_instance(&class_info.cid) else {
            continue;
        };
        let capabilities = match instance.initialize(&factory) {
            Ok(()) => {
                let (audio_inputs, audio_outputs) = instance.main_audio_channel_counts();
                let (midi_inputs, midi_outputs) = instance.event_bus_counts();
                let _ = instance.terminate();
                Some((audio_inputs, audio_outputs, midi_inputs > 0, midi_outputs > 0))
            }
            Err(_) => None,
        };
        return Some(class_info_to_plugin_info(
            &class_info,
            bundle_path,
            capabilities,
        ));
    }

    fallback
}

fn class_info_to_plugin_info(
    class_info: &super::interfaces::ClassInfo,
    bundle_path: &Path,
    capabilities: Option<(usize, usize, bool, bool)>,
) -> Vst3PluginInfo {
    let (audio_inputs, audio_outputs, has_midi_input, has_midi_output) =
        capabilities.unwrap_or((0, 0, false, false));

    Vst3PluginInfo {
        id: tuid_to_string(&class_info.cid),
        name: class_info.name.clone(),
        vendor: String::new(),
        path: bundle_path.to_string_lossy().to_string(),
        category: class_info.category.clone(),
        version: String::new(),
        audio_inputs,
        audio_outputs,
        has_midi_input,
        has_midi_output,
    }
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
