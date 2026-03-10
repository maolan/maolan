use super::*;

impl Maolan {
    pub(super) fn handle_show_message(&mut self, show: &Show) -> Task<Message> {
        if !self.state.blocking_read().hw_loaded
            && matches!(show, Show::Save | Show::SaveAs | Show::SaveTemplateAs | Show::Open)
        {
            return Task::none();
        }
        {
            let mut state = self.state.blocking_write();
            state.ctrl = false;
            state.shift = false;
        }
        match show {
            Show::Save => {
                if let Some(path) = &self.session_dir {
                    return self.refresh_graphs_then_save(path.to_string_lossy().to_string());
                }
                Task::perform(
                    async {
                        AsyncFileDialog::new()
                            .set_title("Select folder to save session")
                            .set_directory("/tmp")
                            .pick_folder()
                            .await
                            .map(|handle| handle.path().to_path_buf())
                    },
                    Message::SaveFolderSelected,
                )
            }
            Show::SaveAs => Task::perform(
                async {
                    AsyncFileDialog::new()
                        .set_title("Select folder to save session")
                        .set_directory("/tmp")
                        .pick_folder()
                        .await
                        .map(|handle| handle.path().to_path_buf())
                },
                Message::SaveFolderSelected,
            ),
            Show::SaveTemplateAs => {
                self.state.blocking_write().template_save_dialog =
                    Some(crate::state::TemplateSaveDialog {
                        name: String::new(),
                    });
                self.modal = Some(Show::SaveTemplateAs);
                Task::none()
            }
            Show::Open => Task::perform(
                async {
                    AsyncFileDialog::new()
                        .set_title("Select folder to open session")
                        .set_directory("/tmp")
                        .pick_folder()
                        .await
                        .map(|handle| handle.path().to_path_buf())
                },
                Message::OpenFolderSelected,
            ),
            Show::AddTrack => {
                self.modal = Some(Show::AddTrack);
                let track_templates = crate::gui::scan_track_templates();
                self.add_track.set_available_templates(track_templates);
                Task::none()
            }
            Show::TrackPluginList => {
                self.modal = Some(Show::TrackPluginList);
                #[cfg(all(unix, not(target_os = "macos")))]
                self.selected_lv2_plugins.clear();
                self.selected_vst3_plugins.clear();
                self.selected_clap_plugins.clear();
                Task::none()
            }
            Show::ExportSettings => {
                self.modal = Some(Show::ExportSettings);
                Task::none()
            }
            Show::SessionMetadata => {
                self.modal = Some(Show::SessionMetadata);
                Task::none()
            }
            Show::Preferences => {
                self.modal = Some(Show::Preferences);
                Task::none()
            }
            Show::AutosaveRecovery => {
                self.modal = Some(Show::AutosaveRecovery);
                Task::none()
            }
        }
    }
}
