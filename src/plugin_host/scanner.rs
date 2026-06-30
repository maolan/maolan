use maolan_plugin_host::scan::{ScanDiagnostic, ScanOutput, ScanResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlocklistEntry {
    pub path: String,
    pub error: String,
    pub timestamp: String,
}

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

    pub fn contains(&self, path: &str) -> bool {
        self.entries.iter().any(|e| e.path == path)
    }

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
    append_parent_log_level(&mut cmd);

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("failed to spawn scanner: {e}"))?;

    let stdout_handle = child.stdout.take().map(|mut stdout| {
        std::thread::spawn(move || {
            use std::io::Read;
            let mut buf = Vec::new();
            let _ = stdout.read_to_end(&mut buf);
            buf
        })
    });

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

    let output = stdout_handle
        .map(|h| h.join().unwrap_or_default())
        .unwrap_or_default();

    if !status.success() {
        let code = status.code().unwrap_or(-1);
        return Err(format!(
            "scanner exited with code {code} (plugin may have crashed during scan)"
        ));
    }

    let json =
        String::from_utf8(output).map_err(|e| format!("scanner output is not valid UTF-8: {e}"))?;

    let output: ScanOutput<ScanResult> = serde_json::from_str(&json)
        .map_err(|e| format!("scanner output is not valid JSON: {e}"))?;

    log_scan_diagnostics(&output.errors, tracing::Level::ERROR);
    log_scan_diagnostics(&output.warnings, tracing::Level::WARN);

    Ok(output.data)
}

fn log_scan_diagnostics(diagnostics: &[ScanDiagnostic], level: tracing::Level) {
    for diagnostic in diagnostics {
        let message = &diagnostic.message;
        let plugin_uri = diagnostic.plugin_uri.as_deref().unwrap_or("-");
        let plugin_name = diagnostic.plugin_name.as_deref().unwrap_or("-");
        let bundle_uri = diagnostic.bundle_uri.as_deref().unwrap_or("-");
        if level == tracing::Level::ERROR {
            tracing::error!(%message, %plugin_uri, %plugin_name, %bundle_uri, "plugin scan diagnostic");
        } else {
            tracing::warn!(%message, %plugin_uri, %plugin_name, %bundle_uri, "plugin scan diagnostic");
        }
    }
}

fn append_parent_log_level(cmd: &mut Command) {
    let parent_args: Vec<String> = std::env::args().collect();
    if let Some(pos) = parent_args.iter().position(|a| a == "--log-level")
        && pos + 1 < parent_args.len()
    {
        cmd.arg("--log-level").arg(&parent_args[pos + 1]);
    }
}

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
