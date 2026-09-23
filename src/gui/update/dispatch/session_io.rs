use super::*;

impl Maolan {
    pub(super) fn handle_session_io_message(&mut self, message: Message) -> Option<Task<Message>> {
        match message {
            Message::SaveFolderSelected(ref path_opt) => {
                {
                    let mut state = self.state.write().expect("state lock poisoned");
                    state.ctrl = false;
                    state.shift = false;
                }
                if let Some(path) = path_opt {
                    self.session_dir = Some(path.clone());
                    return Some(self.refresh_graphs_then_save(path.to_string_lossy().to_string()));
                }
                if self.session_ops.pending_exit_after_save {
                    self.session_ops.pending_exit_after_save = false;
                    self.state.write().expect("state lock poisoned").message =
                        "Close cancelled".to_string();
                }
                None
            }
            Message::RecordFolderSelected(ref path_opt) => {
                {
                    let mut state = self.state.write().expect("state lock poisoned");
                    state.ctrl = false;
                    state.shift = false;
                }
                if let Some(path) = path_opt {
                    self.session_dir = Some(path.clone());
                    self.transport.record_armed = true;
                    self.transport.pending_record_after_save = true;
                    if self.transport.playing {
                        self.start_recording_preview();
                    }
                    Some(self.refresh_graphs_then_save(path.to_string_lossy().to_string()))
                } else {
                    self.transport.pending_record_after_save = false;
                    None
                }
            }
            Message::OpenFolderSelected(Some(path)) => {
                {
                    let mut state = self.state.write().expect("state lock poisoned");
                    state.ctrl = false;
                    state.shift = false;
                }
                if !path.exists() {
                    self.forget_recent_session_path(&path);
                    self.state.write().expect("state lock poisoned").message =
                        format!("Session no longer exists: {}", path.display());
                    return Some(Task::none());
                }
                if Self::has_newer_autosave_snapshot(&path, &self.session_branch) {
                    self.session_ops.pending_recovery_session_dir = Some(path.clone());
                    self.session_ops.pending_autosave_recovery = None;
                    self.session_ops.pending_open_session_dir = Some(path.clone());
                    self.modal = Some(Show::AutosaveRecovery);
                    self.state.write().expect("state lock poisoned").message =
                        "Found newer autosave snapshot for opened session.".to_string();
                    return Some(Task::none());
                } else if self
                    .session_ops
                    .pending_recovery_session_dir
                    .as_ref()
                    .is_some_and(|pending| pending == &path)
                {
                    self.session_ops.pending_recovery_session_dir = None;
                }
                self.session_ops.pending_open_session_dir = None;
                self.session_dir = Some(path.clone());
                self.session_ops.pending_autosave_recovery = None;
                self.rec.stop_recording_preview();
                self.state.write().expect("state lock poisoned").message =
                    "Loading session...".to_string();
                Some(Task::perform(async move { path }, Message::LoadSessionPath))
            }
            Message::LoadSessionPath(path) => {
                self.session_dir = Some(path.clone());
                self.rec.stop_recording_preview();
                match self.load(path.to_string_lossy().to_string()) {
                    Ok(task) => Some(Task::batch(vec![
                        task,
                        self.queue_midi_clip_preview_loads(),
                    ])),
                    Err(e) => {
                        self.state.write().expect("state lock poisoned").message =
                            format!("Failed to load session: {}", e);
                        Some(Task::none())
                    }
                }
            }
            Message::RecoverAutosaveSnapshot => {
                let startup_modal_flow = matches!(self.modal, Some(Show::AutosaveRecovery));
                if let Err(e) = self.prepare_pending_autosave_recovery() {
                    self.state.write().expect("state lock poisoned").message = e;
                    return Some(Task::none());
                }
                if startup_modal_flow {
                    self.modal = None;
                    return Some(self.apply_pending_autosave_recovery());
                }
                if let Some(pending) = self.session_ops.pending_autosave_recovery.as_mut() {
                    let selected_snapshot = pending
                        .snapshots
                        .get(pending.selected_index)
                        .cloned()
                        .unwrap_or_else(|| pending.snapshots[0].clone());
                    let preview = Self::autosave_recovery_preview_summary(
                        &pending.session_dir,
                        &selected_snapshot,
                        &self.session_branch,
                    );
                    if !pending.confirm_armed {
                        pending.confirm_armed = true;
                        self.state.write().expect("state lock poisoned").message =
                            format!("{preview}. Run Recover Autosave Snapshot again to apply.");
                        return Some(Task::none());
                    }
                }
                Some(self.apply_pending_autosave_recovery())
            }
            Message::RecoverAutosaveIgnore => {
                let deferred_open = self.session_ops.pending_open_session_dir.clone();
                self.session_ops.pending_recovery_session_dir = None;
                self.session_ops.pending_autosave_recovery = None;
                self.session_ops.pending_open_session_dir = None;
                self.modal = None;
                if let Some(path) = deferred_open {
                    self.state.write().expect("state lock poisoned").message =
                        "Loading session...".to_string();
                    Some(Task::perform(async move { path }, Message::LoadSessionPath))
                } else {
                    self.state.write().expect("state lock poisoned").message =
                        "Autosave recovery ignored".to_string();
                    Some(Task::none())
                }
            }
            _ => None,
        }
    }
}

impl Maolan {
    pub(super) fn handle_session_io_extended_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::OpenFileImporter => {
                return Task::perform(
                    async {
                        let files = AsyncFileDialog::new()
                            .set_title("Import files")
                            .add_filter("Audio/MIDI", &["wav", "ogg", "mp3", "flac", "mid", "midi"])
                            .add_filter("Audio", &["wav", "ogg", "mp3", "flac"])
                            .add_filter("MIDI", &["mid", "midi"])
                            .pick_files()
                            .await;
                        files.map(|handles| {
                            handles
                                .into_iter()
                                .map(|f| f.path().to_path_buf())
                                .collect()
                        })
                    },
                    Message::ImportFilesSelected,
                );
            }
            Message::DeleteUnusedSessionMediaFiles => {
                let Some(session_root) = self.session_dir.clone() else {
                    self.state.write().expect("state lock poisoned").message =
                        "Cleanup requires an opened/saved session folder".to_string();
                    return Task::none();
                };

                match self.delete_unused_session_media_files(&session_root) {
                    Ok(report)
                        if report.deleted_files.is_empty()
                            && report.failed_files.is_empty()
                            && report.deleted_clips.is_empty() =>
                    {
                        self.state.write().expect("state lock poisoned").message =
                            "No unused files found".to_string();
                    }
                    Ok(report) if report.failed_files.is_empty() => {
                        let mut parts = Vec::new();
                        if !report.deleted_clips.is_empty() {
                            parts.push(format!("{} unused clip(s)", report.deleted_clips.len()));
                        }
                        if !report.deleted_files.is_empty() {
                            parts.push(format!("{} unused file(s)", report.deleted_files.len()));
                        }
                        self.state.write().expect("state lock poisoned").message =
                            format!("Deleted {}", parts.join(" and "));
                    }
                    Ok(report) if report.deleted_files.is_empty() => {
                        self.state.write().expect("state lock poisoned").message = format!(
                            "Failed to delete {} unused file(s)",
                            report.failed_files.len()
                        );
                    }
                    Ok(report) => {
                        self.state.write().expect("state lock poisoned").message = format!(
                            "Deleted {} unused file(s); {} failed",
                            report.deleted_files.len(),
                            report.failed_files.len()
                        );
                    }
                    Err(e) => {
                        self.state.write().expect("state lock poisoned").message = e;
                    }
                }
            }
            Message::CollectToSession => match self.collect_to_session() {
                Ok(message) => {
                    self.state.write().expect("state lock poisoned").message = message;
                }
                Err(e) => {
                    self.state.write().expect("state lock poisoned").message = e;
                }
            },
            Message::ImportFilesSelected(Some(ref paths)) => {
                if paths.is_empty() {
                    self.state.write().expect("state lock poisoned").message =
                        "No files selected".to_string();
                    return Task::none();
                }
                let Some(session_root) = self.session_dir.clone() else {
                    self.state.write().expect("state lock poisoned").message =
                        "Import requires an opened/saved session folder".to_string();
                    return Task::none();
                };

                let _used_track_names: HashSet<String> = self
                    .state
                    .read()
                    .expect("state lock poisoned")
                    .tracks
                    .iter()
                    .map(|track| track.name.clone())
                    .collect();

                let total_files = paths.len();
                self.transfer.import_in_progress = true;
                self.transfer.import_current_file = 0;
                self.transfer.import_total_files = total_files;
                self.transfer.import_file_progress = 0.0;
                self.transfer.import_current_filename = String::new();

                let paths = paths.clone();
                let playback_rate = self.transport.playback_rate_hz.max(1.0);

                return Task::run(
                    {
                        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

                        tokio::spawn(async move {
                            let mut used_names = _used_track_names;
                            let mut failures = Vec::new();

                            for (idx, path) in paths.iter().enumerate() {
                                let file_index = idx + 1;
                                let filename = path
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("unknown")
                                    .to_string();

                                let tx_clone = tx.clone();
                                let filename_for_progress = filename.clone();
                                let mut last_progress_bucket: Option<u16> = None;
                                let mut last_operation: Option<String> = None;
                                let progress_fn =
                                    move |progress: f32, operation: Option<String>| {
                                        let clamped = progress.clamp(0.0, 1.0);
                                        let bucket = (clamped * 100.0).round() as u16;
                                        if last_progress_bucket == Some(bucket)
                                            && last_operation == operation
                                        {
                                            return;
                                        }
                                        last_progress_bucket = Some(bucket);
                                        last_operation = operation.clone();
                                        if tx_clone
                                            .send(Message::ImportProgress {
                                                file_index,
                                                total_files,
                                                file_progress: clamped,
                                                filename: filename_for_progress.clone(),
                                                operation,
                                            })
                                            .is_err()
                                        {}
                                    };

                                if Self::is_import_audio_path(path) {
                                    match Self::import_audio_to_session_wav_with_progress(
                                        path,
                                        &session_root,
                                        playback_rate.round().max(1.0) as u32,
                                        progress_fn,
                                    )
                                    .await
                                    {
                                        Ok((clip_rel, channels, length, peaks)) => {
                                            let base = Self::import_track_base_name(path);
                                            let track_name =
                                                Self::unique_track_name(&base, &mut used_names);
                                            if tx
                                                .send(Message::ImportPreparedAudioPeaks {
                                                    track_name: track_name.clone(),
                                                    clip_name: clip_rel.clone(),
                                                    start: 0,
                                                    length,
                                                    offset: 0,
                                                    peaks,
                                                })
                                                .is_err()
                                            {
                                                return;
                                            }

                                            if let Err(e) = CLIENT
                                                .send(EngineMessage::Request(Action::AddTrack {
                                                    name: track_name.clone(),
                                                    audio_ins: channels,
                                                    midi_ins: 0,
                                                    audio_outs: channels,
                                                    midi_outs: 0,
                                                    folder: false,
                                                    mixosc_addr: None,
                                                }))
                                                .await
                                            {
                                                failures.push(format!("{} ({e})", path.display()));
                                                continue;
                                            }
                                            if let Err(e) = CLIENT
                                                .send(EngineMessage::Request(Action::AddClip {
                                                    clip_id: crate::state::generate_clip_id(),
                                                    name: clip_rel,
                                                    track_name,
                                                    start: 0,
                                                    length,
                                                    offset: 0,
                                                    input_channel: 0,
                                                    muted: false,
                                                    reversed: false,
                                                    gain_db: 0.0,
                                                    peaks_file: None,
                                                    kind: Kind::Audio,
                                                    fade_enabled: true,
                                                    fade_in_samples: 240,
                                                    fade_out_samples: 240,
                                                    source_name: None,
                                                    source_offset: None,
                                                    source_length: None,
                                                    preview_name: None,
                                                    pitch_correction_points: vec![],
                                                    pitch_correction_frame_likeness: None,
                                                    pitch_correction_inertia_ms: None,
                                                    pitch_correction_formant_compensation: None,
                                                    pitch_correction_detector: Default::default(),
                                                    pitch_correction_mode: Default::default(),
                                                    plugin_graph_json: Some(
                                                        Maolan::default_clip_plugin_graph_json(
                                                            channels, channels,
                                                        ),
                                                    ),
                                                }))
                                                .await
                                            {
                                                failures.push(format!("{} ({e})", path.display()));
                                                continue;
                                            }
                                        }
                                        Err(e) => {
                                            failures.push(format!("{} ({e})", path.display()));
                                        }
                                    }
                                } else if Self::is_import_midi_path(path) {
                                    if tx
                                        .send(Message::ImportProgress {
                                            file_index,
                                            total_files,
                                            file_progress: 0.5,
                                            filename: filename.clone(),
                                            operation: Some("Copying".to_string()),
                                        })
                                        .is_err()
                                    {
                                        return;
                                    }

                                    match Self::import_midi_to_session(
                                        path,
                                        &session_root,
                                        playback_rate,
                                    ) {
                                        Ok((clip_rel, length)) => {
                                            let base = Self::import_track_base_name(path);
                                            let track_name =
                                                Self::unique_track_name(&base, &mut used_names);

                                            if let Err(e) = CLIENT
                                                .send(EngineMessage::Request(Action::AddTrack {
                                                    name: track_name.clone(),
                                                    audio_ins: 0,
                                                    midi_ins: 1,
                                                    audio_outs: 0,
                                                    midi_outs: 1,
                                                    folder: false,
                                                    mixosc_addr: None,
                                                }))
                                                .await
                                            {
                                                failures.push(format!("{} ({e})", path.display()));
                                                continue;
                                            }
                                            if let Err(e) = CLIENT
                                                .send(EngineMessage::Request(Action::AddClip {
                                                    clip_id: crate::state::generate_clip_id(),
                                                    name: clip_rel,
                                                    track_name,
                                                    start: 0,
                                                    length,
                                                    offset: 0,
                                                    input_channel: 0,
                                                    muted: false,
                                                    reversed: false,
                                                    gain_db: 0.0,
                                                    peaks_file: None,
                                                    kind: Kind::MIDI,
                                                    fade_enabled: true,
                                                    fade_in_samples: 240,
                                                    fade_out_samples: 240,
                                                    source_name: None,
                                                    source_offset: None,
                                                    source_length: None,
                                                    preview_name: None,
                                                    pitch_correction_points: vec![],
                                                    pitch_correction_frame_likeness: None,
                                                    pitch_correction_inertia_ms: None,
                                                    pitch_correction_formant_compensation: None,
                                                    pitch_correction_detector: Default::default(),
                                                    pitch_correction_mode: Default::default(),
                                                    plugin_graph_json: None,
                                                }))
                                                .await
                                            {
                                                failures.push(format!("{} ({e})", path.display()));
                                                continue;
                                            }
                                        }
                                        Err(e) => {
                                            failures.push(format!("{} ({e})", path.display()));
                                        }
                                    }

                                    if tx
                                        .send(Message::ImportProgress {
                                            file_index,
                                            total_files,
                                            file_progress: 1.0,
                                            filename: filename.clone(),
                                            operation: None,
                                        })
                                        .is_err()
                                    {
                                        return;
                                    }
                                } else {
                                    failures.push(format!(
                                        "{} (unsupported extension)",
                                        path.display()
                                    ));
                                }
                            }

                            for _err in &failures {}

                            if tx
                                .send(Message::ImportProgress {
                                    file_index: total_files,
                                    total_files,
                                    file_progress: 1.0,
                                    filename: "Done".to_string(),
                                    operation: None,
                                })
                                .is_err()
                            {
                                return;
                            }
                            let _ = tx.send(Message::ImportFinished {
                                total_files,
                                failed_files: failures.clone(),
                            });
                            drop(tx);
                        });

                        maolan_widgets::iced::futures::stream::unfold(rx, |mut rx| async move {
                            rx.recv().await.map(|msg| (msg, rx))
                        })
                    },
                    |msg| msg,
                );
            }
            Message::ImportFilesSelected(None) => {}
            Message::ImportProgress {
                file_index,
                total_files,
                file_progress,
                ref filename,
                ref operation,
            } => {
                if self.transfer.import_current_file == file_index
                    && self.transfer.import_total_files == total_files
                    && (self.transfer.import_file_progress - file_progress).abs() < f32::EPSILON
                    && self.transfer.import_current_filename == *filename
                    && self.transfer.import_current_operation == *operation
                {
                    return Task::none();
                }
                self.transfer.import_current_file = file_index;
                self.transfer.import_total_files = total_files;
                self.transfer.import_file_progress = file_progress;
                self.transfer.import_current_filename = filename.clone();
                self.transfer.import_current_operation = operation.clone();

                if file_index >= total_files && file_progress >= 1.0 {
                    self.transfer.import_in_progress = false;
                    self.state.write().expect("state lock poisoned").message =
                        format!("Imported {total_files} file(s)");
                } else {
                    let percent = (file_progress * 100.0) as usize;
                    let op_text = operation
                        .as_ref()
                        .map(|s| format!(" [{}]", s))
                        .unwrap_or_default();
                    self.state.write().expect("state lock poisoned").message = format!(
                        "Importing {}/{} ({percent}%){}: {}",
                        file_index, total_files, op_text, filename
                    );
                }
            }
            Message::ImportFinished {
                total_files,
                ref failed_files,
            } => {
                if failed_files.is_empty() {
                    self.state.write().expect("state lock poisoned").message =
                        format!("Imported {total_files} file(s)");
                } else {
                    let succeeded = total_files.saturating_sub(failed_files.len());
                    let first_error = failed_files.first().cloned().unwrap_or_default();
                    self.state.write().expect("state lock poisoned").message = format!(
                        "Imported {succeeded}/{total_files} file(s). First error: {first_error}"
                    );
                }
            }
            Message::ImportPreparedAudioPeaks {
                ref track_name,
                ref clip_name,
                start,
                length,
                offset,
                ref peaks,
            } => {
                let key = Self::audio_clip_key(track_name, clip_name, start, length, offset);
                self.pending
                    .pending_precomputed_peaks
                    .insert(key, peaks.clone());
            }
            Message::TrackTemplatesLoaded(ref track_templates, ref folder_templates) => {
                self.add_track
                    .set_available_templates(track_templates.clone());
                self.add_track
                    .set_available_folder_templates(folder_templates.clone());
                if let Some(dialog) = &mut self.apply_template.dialog {
                    dialog.available_templates = track_templates.clone();
                    dialog.available_folder_templates = folder_templates.clone();
                }
            }
            #[cfg(any(
                target_os = "linux",
                target_os = "windows",
                target_os = "freebsd",
                target_os = "openbsd",
                target_os = "macos"
            ))]
            Message::PreferencesDevicesLoaded {
                ref output_devices,
                ref input_devices,
            } => {
                let mut state = self.state.write().expect("state lock poisoned");
                if !output_devices.is_empty() {
                    state.available_hw = output_devices.clone();
                }
                if !input_devices.is_empty() {
                    state.available_input_hw = input_devices.clone();
                }
            }
            Message::DrainAudioPeakUpdates => {
                let updates = if let Ok(mut queue) = AUDIO_PEAK_UPDATES.lock() {
                    std::mem::take(&mut *queue)
                } else {
                    Vec::new()
                };
                if updates.is_empty() {
                    return Task::none();
                }

                let mut state = self.state.write().expect("state lock poisoned");
                for update in updates {
                    let key = Self::audio_clip_key(
                        &update.track_name,
                        &update.clip_name,
                        update.start,
                        update.length,
                        update.offset,
                    );
                    if update.done {
                        self.pending.pending_peak_rebuilds.remove(&key);
                        continue;
                    }
                    if update.target_bins == 0 {
                        continue;
                    }
                    if let Some(track) = state
                        .tracks
                        .iter_mut()
                        .find(|t| t.name == update.track_name)
                        && let Some(clip) = track.audio.clips.iter_mut().find(|clip| {
                            clip.name == update.clip_name
                                && clip.start == update.start
                                && clip.length == update.length
                                && clip.offset == update.offset
                        })
                    {
                        if clip.peaks.len() != update.channels
                            || clip.peaks.first().map(Vec::len).unwrap_or(0) != update.target_bins
                        {
                            clip.peaks = std::sync::Arc::new(vec![
                                vec![
                                    [0.0_f32, 0.0_f32];
                                    update.target_bins
                                ];
                                update.channels
                            ]);
                        }
                        let chunk_bins = update.peaks.first().map(Vec::len).unwrap_or(0);
                        let end = (update.bin_start + chunk_bins).min(update.target_bins);
                        if end > update.bin_start {
                            let peaks_mut = std::sync::Arc::make_mut(&mut clip.peaks);
                            for channel_idx in 0..update.channels.min(peaks_mut.len()) {
                                if let Some(src) = update.peaks.get(channel_idx) {
                                    let dst = &mut peaks_mut[channel_idx][update.bin_start..end];
                                    let n = dst.len().min(src.len());
                                    dst[..n].copy_from_slice(&src[..n]);
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        self.update_children(&message);
        Task::none()
    }
}
