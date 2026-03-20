#[cfg(all(unix, not(target_os = "macos")))]
use crate::plugins::x11::open_editor_blocking;
use std::sync::mpsc;

#[derive(Debug, Clone)]
pub(crate) struct Vst3UiClosedState {
    pub track_name: String,
    pub clip_idx: Option<usize>,
    pub instance_id: usize,
    pub state: maolan_engine::vst3::Vst3PluginState,
}

pub struct GuiVst3UiHost {
    closed_tx: mpsc::Sender<Vst3UiClosedState>,
    closed_rx: mpsc::Receiver<Vst3UiClosedState>,
}

impl GuiVst3UiHost {
    pub fn new() -> Self {
        let (closed_tx, closed_rx) = mpsc::channel();
        Self {
            closed_tx,
            closed_rx,
        }
    }

    pub fn drain_closed_states(&mut self) -> Vec<Vst3UiClosedState> {
        let mut states = Vec::new();
        while let Ok(state) = self.closed_rx.try_recv() {
            states.push(state);
        }
        states
    }

    #[cfg(not(all(unix, not(target_os = "macos"))))]
    #[allow(clippy::too_many_arguments)]
    pub fn open_editor(
        &mut self,
        _track_name: &str,
        _clip_idx: Option<usize>,
        _instance_id: usize,
        _plugin_path: &str,
        _plugin_name: &str,
        _plugin_id: &str,
        _sample_rate_hz: f64,
        _block_size: usize,
        _audio_inputs: usize,
        _audio_outputs: usize,
        _state: Option<maolan_engine::vst3::Vst3PluginState>,
    ) -> Result<(), String> {
        Err("VST3 UI hosting is only supported on Unix/X11 in this build".to_string())
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[allow(clippy::too_many_arguments)]
    pub fn open_editor(
        &mut self,
        track_name: &str,
        clip_idx: Option<usize>,
        instance_id: usize,
        plugin_path: &str,
        plugin_name: &str,
        plugin_id: &str,
        sample_rate_hz: f64,
        block_size: usize,
        audio_inputs: usize,
        audio_outputs: usize,
        state: Option<maolan_engine::vst3::Vst3PluginState>,
    ) -> Result<(), String> {
        let track_name = track_name.to_string();
        let plugin_path = plugin_path.to_string();
        let plugin_name = plugin_name.to_string();
        let plugin_id = plugin_id.to_string();
        let sample_rate_hz = sample_rate_hz.max(1.0);
        let block_size = block_size.max(1);
        let closed_tx = self.closed_tx.clone();
        std::thread::Builder::new()
            .name("vst3-ui".to_string())
            .spawn(move || {
                match open_editor_blocking(
                    &plugin_path,
                    &plugin_name,
                    &plugin_id,
                    sample_rate_hz,
                    block_size,
                    audio_inputs,
                    audio_outputs,
                    state,
                ) {
                    Ok(Some(state)) => {
                        let _ = closed_tx.send(Vst3UiClosedState {
                            track_name,
                            clip_idx,
                            instance_id,
                            state,
                        });
                    }
                    Ok(None) => {}
                    Err(err) => {
                        tracing::error!("Failed to open VST3 editor: {err}");
                    }
                }
            })
            .map_err(|e| format!("Failed to spawn VST3 UI thread: {e}"))?;
        Ok(())
    }
}
