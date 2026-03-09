#[cfg(target_os = "windows")]
mod win32;
#[cfg(all(unix, not(target_os = "macos")))]
use crate::plugins::x11::open_editor_blocking;
#[cfg(target_os = "windows")]
use std::collections::HashSet;
#[cfg(target_os = "windows")]
use std::sync::{LazyLock, Mutex};

#[cfg(target_os = "windows")]
static ACTIVE_WINDOWS_EDITORS: LazyLock<Mutex<HashSet<usize>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

pub struct GuiVst3UiHost;

impl GuiVst3UiHost {
    pub fn new() -> Self {
        Self
    }

    #[allow(clippy::too_many_arguments)]
    pub fn open_editor(
        &mut self,
        plugin_path: &str,
        plugin_name: &str,
        plugin_id: &str,
        sample_rate_hz: f64,
        block_size: usize,
        audio_inputs: usize,
        audio_outputs: usize,
        state: Option<maolan_engine::vst3::Vst3PluginState>,
    ) -> Result<(), String> {
        #[cfg(not(any(all(unix, not(target_os = "macos")), target_os = "windows")))]
        {
            let _ = (
                plugin_path,
                plugin_name,
                plugin_id,
                sample_rate_hz,
                block_size,
                audio_inputs,
                audio_outputs,
                state,
            );
            return Err("VST3 UI hosting is only supported on Unix/X11 in this build".to_string());
        }

        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let plugin_path = plugin_path.to_string();
            let plugin_name = plugin_name.to_string();
            let plugin_id = plugin_id.to_string();
            let sample_rate_hz = sample_rate_hz.max(1.0);
            let block_size = block_size.max(1);
            std::thread::Builder::new()
                .name("vst3-ui".to_string())
                .spawn(move || {
                    if let Err(err) = open_editor_blocking(
                        &plugin_path,
                        &plugin_name,
                        &plugin_id,
                        sample_rate_hz,
                        block_size,
                        audio_inputs,
                        audio_outputs,
                        state,
                    ) {
                        eprintln!("VST3 UI error: {err}");
                    }
                })
                .map_err(|e| format!("Failed to spawn VST3 UI thread: {e}"))?;
            Ok(())
        }

        #[cfg(target_os = "windows")]
        {
            let plugin_path = plugin_path.to_string();
            let plugin_name = plugin_name.to_string();
            let plugin_id = plugin_id.to_string();
            let sample_rate_hz = sample_rate_hz.max(1.0);
            let block_size = block_size.max(1);
            std::thread::Builder::new()
                .name("vst3-ui".to_string())
                .spawn(move || {
                    if let Err(err) = win32::open_editor_blocking(
                        &plugin_path,
                        &plugin_name,
                        &plugin_id,
                        sample_rate_hz,
                        block_size,
                        audio_inputs,
                        audio_outputs,
                        state,
                    ) {
                        eprintln!("VST3 UI error: {err}");
                    }
                })
                .map_err(|e| format!("Failed to spawn VST3 UI thread: {e}"))?;
            Ok(())
        }
    }

    #[cfg(target_os = "windows")]
    pub fn open_editor_from_handle(
        &mut self,
        view_handle: usize,
        title: &str,
    ) -> Result<(), String> {
        if view_handle == 0 {
            return Err("VST3 editor view handle is null".to_string());
        }
        {
            let mut active = ACTIVE_WINDOWS_EDITORS
                .lock()
                .map_err(|_| "Failed to lock active editor set".to_string())?;
            if active.contains(&view_handle) {
                return Ok(());
            }
            active.insert(view_handle);
        }
        let title = title.to_string();
        let spawn_result = std::thread::Builder::new()
            .name("vst3-ui".to_string())
            .spawn(move || {
                if let Err(err) = win32::open_editor_from_handle_blocking(view_handle, &title) {
                    eprintln!("VST3 UI error: {err}");
                }
                if let Ok(mut active) = ACTIVE_WINDOWS_EDITORS.lock() {
                    active.remove(&view_handle);
                }
            })
            .map_err(|e| format!("Failed to spawn VST3 UI thread: {e}"));
        if let Err(e) = spawn_result {
            if let Ok(mut active) = ACTIVE_WINDOWS_EDITORS.lock() {
                active.remove(&view_handle);
            }
            return Err(e);
        }
        Ok(())
    }
}
