use super::*;

impl Maolan {
    pub(super) fn handle_response_session_state_action(
        &mut self,
        action: &Action,
    ) -> Option<Task<Message>> {
        match action {
            Action::SetSessionPath(_) => {
                self.has_unsaved_changes = false;
                self.engine_dirty = false;
                self.last_autosave_snapshot = None;
                self.pending_autosave_recovery = None;
                self.pending_open_session_dir = None;
                self.modal = None;
                if let Some(path) = self.session_dir.clone() {
                    self.remember_recent_session_path(&path);
                }
                if let Some(autosave_root) = self.autosave_snapshot_root()
                    && autosave_root.exists()
                    && let Err(_err) = fs::remove_dir_all(&autosave_root)
                {}
                if self.pending_exit_after_save {
                    self.pending_exit_after_save = false;
                    return Some(self.request_quit());
                }
                if self.pending_record_after_save {
                    self.pending_record_after_save = false;
                    return Some(self.send(Action::SetRecordEnabled(true)));
                }
                Some(Task::none())
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            Action::Lv2Plugins(plugins) => {
                let blocklist = crate::plugin_blocklist::Blocklist::load();
                let (filtered, blocked): (Vec<_>, Vec<_>) =
                    plugins.iter().cloned().partition(|p| {
                        !blocklist.is_blocked(&p.bundle_uri) && !blocklist.is_blocked(&p.uri)
                    });
                let mut state = self.state.blocking_write();
                state.lv2_plugins = filtered;
                state.lv2_plugins_loaded = true;
                state.lv2_plugins_unavailable = false;
                state.message = format!(
                    "Loaded {} LV2 plugins ({} blocklisted)",
                    state.lv2_plugins.len(),
                    blocked.len()
                );
                Some(Task::none())
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            Action::Lv2PluginsUnavailable { error } => {
                let mut state = self.state.blocking_write();
                state.lv2_plugins = vec![];
                state.lv2_plugins_loaded = true;
                state.lv2_plugins_unavailable = true;
                state.message = format!("LV2 plugin scan unavailable: {error}");
                Some(Task::none())
            }
            Action::Vst3Plugins(plugins) => {
                let blocklist = crate::plugin_blocklist::Blocklist::load();
                let (filtered, blocked): (Vec<_>, Vec<_>) = plugins
                    .iter()
                    .cloned()
                    .partition(|p| !blocklist.is_blocked(&p.path));
                let mut state = self.state.blocking_write();
                state.vst3_plugins = filtered;
                state.vst3_plugins_loaded = true;
                state.vst3_plugins_unavailable = false;
                state.message = format!(
                    "Loaded {} VST3 plugins ({} blocklisted)",
                    state.vst3_plugins.len(),
                    blocked.len()
                );
                Some(Task::none())
            }
            Action::Vst3PluginsUnavailable { error } => {
                let mut state = self.state.blocking_write();
                state.vst3_plugins = vec![];
                state.vst3_plugins_loaded = true;
                state.vst3_plugins_unavailable = true;
                state.message = format!("VST3 plugin scan unavailable: {error}");
                Some(Task::none())
            }
            Action::ClapPlugins(plugins) => {
                let blocklist = crate::plugin_blocklist::Blocklist::load();
                let (filtered, blocked): (Vec<_>, Vec<_>) = plugins
                    .iter()
                    .cloned()
                    .partition(|p| !blocklist.is_blocked(&p.path));
                let mut state = self.state.blocking_write();
                state.clap_plugins = filtered;
                state.clap_plugins_loaded = true;
                state.clap_plugins_unavailable = false;
                state.message = format!(
                    "Loaded {} CLAP plugins ({} blocklisted)",
                    state.clap_plugins.len(),
                    blocked.len()
                );
                Some(Task::none())
            }
            Action::ClapPluginsUnavailable { error } => {
                let mut state = self.state.blocking_write();
                state.clap_plugins = vec![];
                state.clap_plugins_loaded = true;
                state.clap_plugins_unavailable = true;
                state.message = format!("CLAP plugin scan unavailable: {error}");
                Some(Task::none())
            }
            _ => None,
        }
    }
}
