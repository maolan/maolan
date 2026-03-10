use super::*;

impl Maolan {
    pub(super) fn handle_response_session_state_action(
        &mut self,
        action: &Action,
    ) -> Option<Task<Message>> {
        match action {
            Action::SetSessionPath(_) => {
                self.has_unsaved_changes = false;
                self.last_autosave_snapshot = None;
                self.pending_autosave_recovery = None;
                self.pending_open_session_dir = None;
                if let Some(path) = self.session_dir.clone() {
                    self.remember_recent_session_path(&path);
                }
                if let Some(autosave_root) = self.autosave_snapshot_root()
                    && autosave_root.exists()
                {
                    let _ = fs::remove_dir_all(&autosave_root);
                }
                if self.pending_record_after_save {
                    self.pending_record_after_save = false;
                    return Some(self.send(Action::SetRecordEnabled(true)));
                }
                Some(Task::none())
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            Action::Lv2Plugins(plugins) => {
                let mut state = self.state.blocking_write();
                state.lv2_plugins = plugins.clone();
                state.lv2_plugins_loaded = true;
                state.message = format!("Loaded {} LV2 plugins", state.lv2_plugins.len());
                Some(Task::none())
            }
            Action::Vst3Plugins(plugins) => {
                let mut state = self.state.blocking_write();
                state.vst3_plugins = plugins.clone();
                state.vst3_plugins_loaded = true;
                state.message = format!("Loaded {} VST3 plugins", state.vst3_plugins.len());
                Some(Task::none())
            }
            Action::ClapPlugins(plugins) => {
                let mut state = self.state.blocking_write();
                state.clap_plugins = plugins.clone();
                state.clap_plugins_loaded = true;
                state.message = format!("Loaded {} CLAP plugins", state.clap_plugins.len());
                Some(Task::none())
            }
            _ => None,
        }
    }
}
