use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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

    #[cfg(test)]
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

    pub fn load() -> Self {
        let path = Self::path();
        if !path.exists() {
            return Self::default();
        }
        match std::fs::read_to_string(&path) {
            Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn contains(&self, path: &str) -> bool {
        self.entries.iter().any(|e| e.path == path)
    }

    pub fn is_blocked(&self, key: &str) -> bool {
        self.contains(key)
    }

    #[cfg(test)]
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
