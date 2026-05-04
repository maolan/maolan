#[cfg(target_os = "windows")]
use crate::plugins::win32::open_editor_with_processor as open_vst3_editor_platform;
#[cfg(all(unix, not(target_os = "macos")))]
use crate::plugins::x11::open_editor_with_processor as open_vst3_editor_platform;
use maolan_engine::vst3::Vst3Processor;
use std::sync::Arc;
#[cfg(unix)]
use std::sync::mpsc;

#[derive(Debug, Clone)]
#[cfg(unix)]
pub(crate) struct Vst3UiClosedState {
    pub track_name: String,
    pub clip_idx: Option<usize>,
    pub instance_id: usize,
    pub state: maolan_engine::vst3::Vst3PluginState,
}

pub struct GuiVst3UiHost {
    #[cfg(unix)]
    closed_tx: mpsc::Sender<Vst3UiClosedState>,
    #[cfg(unix)]
    closed_rx: mpsc::Receiver<Vst3UiClosedState>,
}

impl GuiVst3UiHost {
    pub fn new() -> Self {
        #[cfg(unix)]
        {
            let (closed_tx, closed_rx) = mpsc::channel();
            Self {
                closed_tx,
                closed_rx,
            }
        }
        #[cfg(not(unix))]
        {
            Self {}
        }
    }

    #[cfg(unix)]
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
        _track_name: &str,
        _clip_idx: Option<usize>,
        _instance_id: usize,
        _plugin_path: &str,
        processor: Arc<Vst3Processor>,
    ) -> Result<(), String> {
        let _track_name = _track_name.to_string();
        #[cfg(unix)]
        let closed_tx = self.closed_tx.clone();
        std::thread::Builder::new()
            .name("vst3-ui".to_string())
            .spawn(move || {
                match open_vst3_editor_platform(processor.clone(), processor.name().to_string()) {
                    Ok(Some(_state)) => {
                        #[cfg(unix)]
                        let _ = closed_tx.send(Vst3UiClosedState {
                            track_name: _track_name,
                            clip_idx: _clip_idx,
                            instance_id: _instance_id,
                            state: _state,
                        });
                    }
                    Ok(None) => {
                        if let Ok(_state) = processor.snapshot_state() {
                            #[cfg(unix)]
                            let _ = closed_tx.send(Vst3UiClosedState {
                                track_name: _track_name,
                                clip_idx: _clip_idx,
                                instance_id: _instance_id,
                                state: _state,
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
