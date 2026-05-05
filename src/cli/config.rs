use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CliConfig {
    pub default_audio_bit_depth: usize,
    pub default_export_sample_rate_hz: u32,
    pub osc_enabled: bool,
    pub default_output_device_id: Option<String>,
    pub default_input_device_id: Option<String>,
}

impl CliConfig {
    pub fn load() -> Result<Self, String> {
        let config_path = config_path()?;
        if !config_path.exists() {
            return Ok(Self::default());
        }
        let contents = std::fs::read_to_string(&config_path)
            .map_err(|err| format!("Failed to read {}: {err}", config_path.display()))?;
        toml::from_str(&contents)
            .map_err(|err| format!("Failed to parse {}: {err}", config_path.display()))
    }
}

fn config_path() -> Result<PathBuf, String> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| "Could not determine home directory".to_string())?;
    Ok(PathBuf::from(home)
        .join(".config")
        .join("maolan")
        .join("config.toml"))
}

pub fn load_session_end_sample(session_dir: &Path) -> Result<usize, String> {
    let session = load_session_json(session_dir)?;
    Ok(session
        .get("tracks")
        .and_then(serde_json::Value::as_array)
        .map(|tracks| tracks.iter().map(track_end_sample).max().unwrap_or(0))
        .unwrap_or(0))
}

fn load_session_json(session_dir: &Path) -> Result<serde_json::Value, String> {
    let session_path = session_dir.join("session.json");
    let file = std::fs::File::open(&session_path)
        .map_err(|err| format!("Failed to open {}: {err}", session_path.display()))?;
    let reader = std::io::BufReader::new(file);
    serde_json::from_reader(reader)
        .map_err(|err| format!("Failed to parse {}: {err}", session_path.display()))
}

fn track_end_sample(track: &serde_json::Value) -> usize {
    ["audio", "midi"]
        .into_iter()
        .filter_map(|kind| track.get(kind))
        .filter_map(|section| section.get("clips").and_then(serde_json::Value::as_array))
        .flat_map(|clips| clips.iter())
        .map(clip_end_sample)
        .max()
        .unwrap_or(0)
}

fn clip_end_sample(clip: &serde_json::Value) -> usize {
    let own_end = clip
        .get("start")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0)
        .saturating_add(
            clip.get("length")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0),
        ) as usize;
    let child_end = clip
        .get("grouped_clips")
        .and_then(serde_json::Value::as_array)
        .map(|clips| clips.iter().map(clip_end_sample).max().unwrap_or(0))
        .unwrap_or(0);
    own_end.max(child_end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_session_end_sample_uses_latest_audio_midi_and_grouped_clip_end() {
        let dir = std::env::temp_dir().join(format!(
            "maolan-cli-support-end-sample-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        std::fs::write(
            dir.join("session.json"),
            serde_json::json!({
                "tracks": [{
                    "name": "Track 1",
                    "audio": {"ins": 2, "outs": 2, "clips": [{
                        "name": "Audio A",
                        "start": 100,
                        "length": 50
                    }, {
                        "name": "Grouped",
                        "start": 10,
                        "length": 5,
                        "grouped_clips": [{
                            "name": "Child",
                            "start": 400,
                            "length": 25
                        }]
                    }]},
                    "midi": {"ins": 1, "outs": 1, "clips": [{
                        "name": "Midi A",
                        "start": 250,
                        "length": 100
                    }]}
                }]
            })
            .to_string(),
        )
        .expect("write session");

        assert_eq!(load_session_end_sample(&dir).expect("end sample"), 425);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
