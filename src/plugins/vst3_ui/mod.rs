#[cfg(all(unix, not(target_os = "macos")))]
mod x11;
#[cfg(target_os = "windows")]
mod win32;

pub struct GuiVst3UiHost;

impl GuiVst3UiHost {
    pub fn new() -> Self {
        Self
    }

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
        let state = state;
        std::thread::Builder::new()
            .name("vst3-ui".to_string())
            .spawn(move || {
                if let Err(err) = x11::open_editor_blocking(
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
            win32::open_editor_blocking(
                plugin_path,
                plugin_name,
                plugin_id,
                sample_rate_hz.max(1.0),
                block_size.max(1),
                audio_inputs,
                audio_outputs,
                state,
            )
        }
    }
}
