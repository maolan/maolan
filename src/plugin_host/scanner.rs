//! Plugin scanner wrapper and blocklist persistence.

use maolan_plugin_host::scan::ScanResult;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// A single blocklist entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlocklistEntry {
    pub path: String,
    pub error: String,
    pub timestamp: String,
}

/// Persistent plugin blocklist.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Blocklist {
    pub entries: Vec<BlocklistEntry>,
}

impl Blocklist {
    fn path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("maolan")
            .join("plugin-blocklist.json")
    }

    /// Save the blocklist to disk.
    pub fn save(&self) -> Result<(), String> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create config dir: {e}"))?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("failed to serialize blocklist: {e}"))?;
        std::fs::write(&path, json).map_err(|e| format!("failed to write blocklist: {e}"))?;
        Ok(())
    }

    /// Check if a plugin path is blocklisted.
    pub fn contains(&self, path: &str) -> bool {
        self.entries.iter().any(|e| e.path == path)
    }

    /// Add an entry if it's not already present.
    pub fn add(&mut self, path: String, error: String) {
        if self.contains(&path) {
            return;
        }
        let timestamp = chrono::Local::now().to_rfc3339();
        self.entries.push(BlocklistEntry {
            path,
            error,
            timestamp,
        });
    }
}

/// Scan a single plugin file using the `maolan-plugin-host` scanner subprocess.
///
/// Returns `Ok(ScanResult)` on success, or `Err(String)` if the scanner
/// crashes, times out, or returns invalid JSON.
pub fn scan_plugin_file(
    host_bin: &Path,
    format: &str,
    plugin_path: &str,
    timeout: Duration,
) -> Result<ScanResult, String> {
    if !host_bin.exists() {
        return Err(format!("scanner binary not found: {}", host_bin.display()));
    }

    let mut cmd = Command::new(host_bin);
    cmd.arg("--scan")
        .arg("--format")
        .arg(format)
        .arg("--path")
        .arg(plugin_path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("failed to spawn scanner: {e}"))?;

    let start = std::time::Instant::now();
    let status = loop {
        if start.elapsed() >= timeout {
            let _ = child.kill();
            return Err("scanner timed out".to_string());
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(e) => return Err(format!("failed to wait for scanner: {e}")),
        }
    };

    let mut output = Vec::new();
    if let Some(mut stdout) = child.stdout.take() {
        use std::io::Read;
        let _ = stdout.read_to_end(&mut output);
    }

    if !status.success() {
        let code = status.code().unwrap_or(-1);
        return Err(format!(
            "scanner exited with code {code} (plugin may have crashed during scan)"
        ));
    }

    let json =
        String::from_utf8(output).map_err(|e| format!("scanner output is not valid UTF-8: {e}"))?;

    serde_json::from_str(&json).map_err(|e| format!("scanner output is not valid JSON: {e}"))
}

/// Scan a plugin file, falling back to the blocklist on failure.
pub fn scan_or_blocklist(
    host_bin: &Path,
    format: &str,
    plugin_path: &str,
    blocklist: &mut Blocklist,
    timeout: Duration,
) -> Option<ScanResult> {
    match scan_plugin_file(host_bin, format, plugin_path, timeout) {
        Ok(result) => {
            if result.error.is_some() {
                let err = result.error.clone().unwrap();
                blocklist.add(plugin_path.to_string(), err);
                let _ = blocklist.save();
            }
            Some(result)
        }
        Err(e) => {
            blocklist.add(plugin_path.to_string(), e);
            let _ = blocklist.save();
            None
        }
    }
}
