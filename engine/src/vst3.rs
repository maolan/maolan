use crate::audio::io::AudioIO;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Vst3PluginInfo {
    pub id: String,
    pub name: String,
    pub path: String,
}

#[derive(Debug)]
pub struct Vst3Processor {
    path: String,
    name: String,
    audio_inputs: Vec<Arc<AudioIO>>,
    audio_outputs: Vec<Arc<AudioIO>>,
}

impl Vst3Processor {
    pub fn new(sample_frames: usize, path: &str, audio_inputs: usize, audio_outputs: usize) -> Self {
        let in_count = audio_inputs.max(1);
        let out_count = audio_outputs.max(1);
        let name = Path::new(path)
            .file_stem()
            .or_else(|| Path::new(path).file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown VST3")
            .to_string();

        Self {
            path: path.to_string(),
            name,
            audio_inputs: (0..in_count).map(|_| Arc::new(AudioIO::new(sample_frames))).collect(),
            audio_outputs: (0..out_count).map(|_| Arc::new(AudioIO::new(sample_frames))).collect(),
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn audio_inputs(&self) -> &[Arc<AudioIO>] {
        &self.audio_inputs
    }

    pub fn audio_outputs(&self) -> &[Arc<AudioIO>] {
        &self.audio_outputs
    }

    pub fn setup_audio_ports(&self) {
        for port in &self.audio_inputs {
            port.setup();
        }
        for port in &self.audio_outputs {
            port.setup();
        }
    }

    pub fn process_with_audio_io(&self, frames: usize) {
        for input in &self.audio_inputs {
            input.process();
        }

        for (out_idx, output) in self.audio_outputs.iter().enumerate() {
            let out_buf = output.buffer.lock();
            out_buf.fill(0.0);
            if self.audio_inputs.is_empty() {
                *output.finished.lock() = true;
                continue;
            }

            // Placeholder processing path: this runs in the track graph and can be
            // swapped for real VST3 component processing while preserving routing.
            let input = &self.audio_inputs[out_idx % self.audio_inputs.len()];
            let in_buf = input.buffer.lock();
            for (o, i) in out_buf.iter_mut().zip(in_buf.iter()).take(frames) {
                *o = *i;
            }
            *output.finished.lock() = true;
        }
    }
}

pub fn list_plugins() -> Vec<Vst3PluginInfo> {
    let mut roots = default_vst3_search_roots();
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
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out.dedup_by(|a, b| a.path.eq_ignore_ascii_case(&b.path));
    out
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

        if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("vst3")) {
            let name = path
                .file_stem()
                .or_else(|| path.file_name())
                .and_then(|s| s.to_str())
                .unwrap_or("Unknown VST3")
                .to_string();
            let path_str = path.to_string_lossy().to_string();
            out.push(Vst3PluginInfo {
                id: path_str.clone(),
                name,
                path: path_str,
            });
        } else {
            collect_vst3_plugins(&path, out);
        }
    }
}

fn default_vst3_search_roots() -> Vec<PathBuf> {
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

    #[cfg(target_os = "linux")]
    {
        roots.push(PathBuf::from("/usr/lib/vst3"));
        roots.push(PathBuf::from("/usr/local/lib/vst3"));
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

