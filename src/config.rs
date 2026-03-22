use crate::consts::audio_defaults;
use crate::message::SnapMode;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub font_size: f32,
    pub mixer_height: f32,
    pub track_width: f32,
    pub osc_enabled: bool,
    pub default_export_sample_rate_hz: u32,
    pub default_snap_mode: SnapMode,
    pub default_audio_bit_depth: usize,
    pub default_output_device_id: Option<String>,
    pub default_input_device_id: Option<String>,
    pub recent_session_paths: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            font_size: 16.0,
            mixer_height: 300.0,
            track_width: 200.0,
            osc_enabled: false,
            default_export_sample_rate_hz: audio_defaults::SAMPLE_RATE_HZ as u32,
            default_snap_mode: SnapMode::Bar,
            default_audio_bit_depth: audio_defaults::BIT_DEPTH,
            default_output_device_id: None,
            default_input_device_id: None,
            recent_session_paths: Vec::new(),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = Self::config_path()?;
        Self::load_from_path(&config_path)
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = Self::config_path()?;
        self.save_to_path(&config_path)
    }

    fn load_from_path(config_path: &PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        if !config_path.exists() {
            let config = Self::default();
            config.save_to_path(config_path)?;
            return Ok(config);
        }

        let contents = fs::read_to_string(config_path)?;
        let config: Config = toml::from_str(&contents)?;
        Ok(config)
    }

    fn save_to_path(&self, config_path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut config = self.clone();
        if let Some(existing) = load_existing_config(config_path)? {
            if config.default_output_device_id.is_none() {
                config.default_output_device_id = existing.default_output_device_id;
            }
            if config.default_input_device_id.is_none() {
                config.default_input_device_id = existing.default_input_device_id;
            }
        }

        let toml_string = toml::to_string_pretty(&config)?;
        fs::write(config_path, toml_string)?;
        Ok(())
    }

    fn config_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map_err(|_| "Could not determine home directory")?;

        let mut path = PathBuf::from(home);
        path.push(".config");
        path.push("maolan");
        path.push("config.toml");
        Ok(path)
    }
}

fn load_existing_config(
    config_path: &PathBuf,
) -> Result<Option<Config>, Box<dyn std::error::Error>> {
    if !config_path.exists() {
        return Ok(None);
    }
    let contents = fs::read_to_string(config_path)?;
    Ok(Some(toml::from_str(&contents)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_preserves_existing_device_ids_when_unmodified() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let temp_root = std::env::temp_dir().join(format!("maolan-config-test-{unique}"));
        let config_dir = temp_root.join(".config").join("maolan");
        fs::create_dir_all(&config_dir).expect("config dir");
        let config_path = config_dir.join("config.toml");
        fs::write(
            &config_path,
            r#"
font_size = 16.0
mixer_height = 300.0
track_width = 200.0
osc_enabled = true
default_export_sample_rate_hz = 48000
default_snap_mode = "Bar"
default_audio_bit_depth = 32
default_output_device_id = "out-dev"
default_input_device_id = "in-dev"
recent_session_paths = ["/tmp/old"]
"#,
        )
        .expect("seed config");

        let config = Config {
            recent_session_paths: vec!["/tmp/new".to_string()],
            ..Config::default()
        };
        config.save_to_path(&config_path).expect("save config");

        let saved = fs::read_to_string(&config_path).expect("read config");
        let parsed: Config = toml::from_str(&saved).expect("parse saved config");
        assert_eq!(parsed.default_output_device_id.as_deref(), Some("out-dev"));
        assert_eq!(parsed.default_input_device_id.as_deref(), Some("in-dev"));
        assert_eq!(parsed.recent_session_paths, vec!["/tmp/new".to_string()]);

        let _ = fs::remove_dir_all(temp_root);
    }
}
