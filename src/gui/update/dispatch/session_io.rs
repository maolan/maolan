use super::*;

impl Maolan {
    pub(super) fn handle_session_io_message(&mut self, message: Message) -> Option<Task<Message>> {
        match message {
            Message::SaveFolderSelected(ref path_opt) => {
                {
                    let mut state = self.state.blocking_write();
                    state.ctrl = false;
                    state.shift = false;
                }
                if let Some(path) = path_opt {
                    self.session_dir = Some(path.clone());
                    return Some(self.refresh_graphs_then_save(path.to_string_lossy().to_string()));
                }
                if self.pending_exit_after_save {
                    self.pending_exit_after_save = false;
                    self.state.blocking_write().message = "Close cancelled".to_string();
                }
                None
            }
            Message::RecordFolderSelected(ref path_opt) => {
                {
                    let mut state = self.state.blocking_write();
                    state.ctrl = false;
                    state.shift = false;
                }
                if let Some(path) = path_opt {
                    self.session_dir = Some(path.clone());
                    self.record_armed = true;
                    self.pending_record_after_save = true;
                    if self.playing {
                        self.start_recording_preview();
                    }
                    Some(self.refresh_graphs_then_save(path.to_string_lossy().to_string()))
                } else {
                    self.pending_record_after_save = false;
                    None
                }
            }
            Message::OpenFolderSelected(Some(path)) => {
                {
                    let mut state = self.state.blocking_write();
                    state.ctrl = false;
                    state.shift = false;
                }
                if Self::has_newer_autosave_snapshot(&path) {
                    self.pending_recovery_session_dir = Some(path.clone());
                    self.pending_autosave_recovery = None;
                    self.pending_open_session_dir = Some(path.clone());
                    self.modal = Some(Show::AutosaveRecovery);
                    self.state.blocking_write().message =
                        "Found newer autosave snapshot for opened session.".to_string();
                    return Some(Task::none());
                } else if self
                    .pending_recovery_session_dir
                    .as_ref()
                    .is_some_and(|pending| pending == &path)
                {
                    self.pending_recovery_session_dir = None;
                }
                self.pending_open_session_dir = None;
                self.session_dir = Some(path.clone());
                self.pending_autosave_recovery = None;
                self.stop_recording_preview();
                self.state.blocking_write().message = "Loading session...".to_string();
                Some(Task::perform(async move { path }, Message::LoadSessionPath))
            }
            Message::LoadSessionPath(path) => {
                self.session_dir = Some(path.clone());
                self.stop_recording_preview();
                match self.load(path.to_string_lossy().to_string()) {
                    Ok(task) => Some(Task::batch(vec![
                        task,
                        self.queue_midi_clip_preview_loads(),
                    ])),
                    Err(e) => {
                        error!("{}", e);
                        self.state.blocking_write().message =
                            format!("Failed to load session: {}", e);
                        Some(Task::none())
                    }
                }
            }
            Message::RecoverAutosaveSnapshot => {
                let startup_modal_flow = matches!(self.modal, Some(Show::AutosaveRecovery));
                if let Err(e) = self.prepare_pending_autosave_recovery() {
                    self.state.blocking_write().message = e;
                    return Some(Task::none());
                }
                if startup_modal_flow {
                    self.modal = None;
                    return Some(self.apply_pending_autosave_recovery());
                }
                if let Some(pending) = self.pending_autosave_recovery.as_mut() {
                    let selected_snapshot = pending
                        .snapshots
                        .get(pending.selected_index)
                        .cloned()
                        .unwrap_or_else(|| pending.snapshots[0].clone());
                    let preview = Self::autosave_recovery_preview_summary(
                        &pending.session_dir,
                        &selected_snapshot,
                    );
                    if !pending.confirm_armed {
                        pending.confirm_armed = true;
                        self.state.blocking_write().message =
                            format!("{preview}. Run Recover Autosave Snapshot again to apply.");
                        return Some(Task::none());
                    }
                }
                Some(self.apply_pending_autosave_recovery())
            }
            Message::RecoverAutosaveIgnore => {
                let deferred_open = self.pending_open_session_dir.clone();
                self.pending_recovery_session_dir = None;
                self.pending_autosave_recovery = None;
                self.pending_open_session_dir = None;
                self.modal = None;
                if let Some(path) = deferred_open {
                    self.state.blocking_write().message = "Loading session...".to_string();
                    Some(Task::perform(async move { path }, Message::LoadSessionPath))
                } else {
                    self.state.blocking_write().message = "Autosave recovery ignored".to_string();
                    Some(Task::none())
                }
            }
            _ => None,
        }
    }
}
