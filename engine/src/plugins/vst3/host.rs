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
        out.sort_by_key(|a| a.name.to_lowercase());

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
                Some((
                    audio_inputs,
                    audio_outputs,
                    midi_inputs > 0,
                    midi_outputs > 0,
                ))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::vst3::interfaces::ClassInfo;
    use std::path::PathBuf;

    #[test]
    fn class_info_to_plugin_info_uses_capabilities_when_present() {
        let class_info = ClassInfo {
            name: "Synth".to_string(),
            category: "Instrument".to_string(),
            cid: [0x12_i8; 16],
        };

        let info = class_info_to_plugin_info(
            &class_info,
            Path::new("/tmp/Test.vst3"),
            Some((2, 4, true, false)),
        );

        assert_eq!(info.id, "12121212121212121212121212121212");
        assert_eq!(info.name, "Synth");
        assert_eq!(info.category, "Instrument");
        assert_eq!(info.path, "/tmp/Test.vst3");
        assert_eq!(info.audio_inputs, 2);
        assert_eq!(info.audio_outputs, 4);
        assert!(info.has_midi_input);
        assert!(!info.has_midi_output);
    }

    #[test]
    fn class_info_to_plugin_info_defaults_capabilities_when_missing() {
        let class_info = ClassInfo {
            name: "Fx".to_string(),
            category: "Audio Module".to_string(),
            cid: [0; 16],
        };

        let info = class_info_to_plugin_info(&class_info, Path::new("/tmp/Fx.vst3"), None);

        assert_eq!(info.audio_inputs, 0);
        assert_eq!(info.audio_outputs, 0);
        assert!(!info.has_midi_input);
        assert!(!info.has_midi_output);
    }

    #[test]
    fn tuid_to_string_formats_bytes_as_uppercase_hex() {
        let tuid = [
            0x00_i8, 0x01, 0x23, 0x45, 0x67, 0x7F, -0x80, -0x01, 0x10, 0x20, 0x30, 0x40, 0x50,
            0x60, 0x70, 0x7E,
        ];

        assert_eq!(tuid_to_string(&tuid), "00012345677F80FF102030405060707E");
    }

    #[test]
    fn get_plugin_info_returns_cached_plugin_by_exact_path() {
        let mut host = Vst3Host::new();
        host.plugins = vec![
            Vst3PluginInfo {
                id: "id-1".to_string(),
                name: "First".to_string(),
                vendor: String::new(),
                path: "/tmp/First.vst3".to_string(),
                category: "Instrument".to_string(),
                version: String::new(),
                audio_inputs: 2,
                audio_outputs: 2,
                has_midi_input: true,
                has_midi_output: false,
            },
            Vst3PluginInfo {
                id: "id-2".to_string(),
                name: "Second".to_string(),
                vendor: String::new(),
                path: "/tmp/Second.vst3".to_string(),
                category: "Fx".to_string(),
                version: String::new(),
                audio_inputs: 2,
                audio_outputs: 2,
                has_midi_input: false,
                has_midi_output: false,
            },
        ];

        let found = host
            .get_plugin_info("/tmp/Second.vst3")
            .map(|p| p.name.clone());
        let missing = host.get_plugin_info(&PathBuf::from("/tmp/missing.vst3").to_string_lossy());

        assert_eq!(found.as_deref(), Some("Second"));
        assert!(missing.is_none());
    }
}
