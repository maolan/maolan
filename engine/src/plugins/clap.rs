use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClapPluginInfo {
    pub name: String,
    pub path: String,
}

pub fn list_plugins() -> Vec<ClapPluginInfo> {
    let mut roots = default_clap_search_roots();

    if let Ok(extra) = std::env::var("CLAP_PATH") {
        for p in std::env::split_paths(&extra) {
            if !p.as_os_str().is_empty() {
                roots.push(p);
            }
        }
    }

    let mut out = Vec::new();
    for root in roots {
        collect_clap_plugins(&root, &mut out);
    }

    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out.dedup_by(|a, b| a.path.eq_ignore_ascii_case(&b.path));
    out
}

fn collect_clap_plugins(root: &Path, out: &mut Vec<ClapPluginInfo>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else {
            continue;
        };

        if ft.is_dir() {
            collect_clap_plugins(&path, out);
            continue;
        }

        if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("clap"))
        {
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string_lossy().to_string());
            out.push(ClapPluginInfo {
                name,
                path: path.to_string_lossy().to_string(),
            });
        }
    }
}

fn default_clap_search_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    #[cfg(target_os = "windows")]
    {
        roots.push(PathBuf::from(r"C:\Program Files\Common Files\CLAP"));
        roots.push(PathBuf::from(r"C:\Program Files (x86)\Common Files\CLAP"));
    }

    #[cfg(target_os = "macos")]
    {
        roots.push(PathBuf::from("/Library/Audio/Plug-Ins/CLAP"));
        roots.push(PathBuf::from(format!(
            "{}/Library/Audio/Plug-Ins/CLAP",
            home_dir()
        )));
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
    {
        roots.push(PathBuf::from("/usr/lib/clap"));
        roots.push(PathBuf::from("/usr/local/lib/clap"));
        roots.push(PathBuf::from(format!("{}/.clap", home_dir())));
        roots.push(PathBuf::from(format!("{}/.local/lib/clap", home_dir())));
    }

    roots
}

fn home_dir() -> String {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default()
}
