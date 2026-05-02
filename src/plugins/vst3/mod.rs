#[cfg(target_os = "windows")]
use crate::plugins::win32::open_editor_with_processor as open_vst3_editor_platform;
#[cfg(all(unix, not(target_os = "macos")))]
use crate::plugins::x11::open_editor_with_processor as open_vst3_editor_platform;
use maolan_engine::vst3::Vst3Processor;
use std::sync::Arc;
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

    #[cfg(not(any(all(unix, not(target_os = "macos")), target_os = "windows")))]
    pub fn open_editor(
        &mut self,
        _track_name: &str,
        _clip_idx: Option<usize>,
        _instance_id: usize,
        _plugin_path: &str,
        _processor: Arc<Vst3Processor>,
    ) -> Result<(), String> {
        Err("VST3 UI hosting is not supported on this platform in this build".to_string())
    }

    #[cfg(any(all(unix, not(target_os = "macos")), target_os = "windows"))]
    pub fn open_editor(
        &mut self,
        track_name: &str,
        clip_idx: Option<usize>,
        instance_id: usize,
        plugin_path: &str,
        processor: Arc<Vst3Processor>,
    ) -> Result<(), String> {
        let track_name = track_name.to_string();
        let _plugin_path = plugin_path.to_string();
        let closed_tx = self.closed_tx.clone();
        std::thread::Builder::new()
            .name("vst3-ui".to_string())
            .spawn(move || {
                match open_vst3_editor_platform(processor.clone(), processor.name().to_string()) {
                    Ok(Some(state)) => {
                        let _ = closed_tx.send(Vst3UiClosedState {
                            track_name,
                            clip_idx,
                            instance_id,
                            state,
                        });
                    }
                    Ok(None) => {
                        if let Ok(state) = processor.snapshot_state() {
                            let _ = closed_tx.send(Vst3UiClosedState {
                                track_name,
                                clip_idx,
                                instance_id,
                                state,
                            });
                        }
                    }
                    Err(err) => {
                        tracing::error!("Failed to open VST3 editor: {err}");
                    }
                }
            })
            .map_err(|e| format!("Failed to spawn VST3 UI thread: {e}"))?;
        Ok(())
    }
}
