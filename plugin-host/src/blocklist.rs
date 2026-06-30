use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlocklistEntry {
    pub path: String,
    #[serde(default)]
    pub error: String,
    #[serde(default)]
    pub timestamp: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Blocklist {
    pub entries: Vec<BlocklistEntry>,
}

impl Blocklist {
    fn default_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("maolan")
            .join("plugin-blocklist.json")
    }

    pub fn load() -> Self {
        Self::load_from(&Self::default_path())
    }

    pub fn load_from(path: &std::path::Path) -> Self {
        if !path.exists() {
            return Self::default();
        }
        match std::fs::read_to_string(path) {
            Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn is_blocked(&self, key: &str) -> bool {
        self.entries.iter().any(|e| e.path == key)
    }
}
