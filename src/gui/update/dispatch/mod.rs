use super::*;
use crate::consts::state_track::{TRACK_MIN_HEIGHT, TRACK_SUBTRACK_MIN_HEIGHT};
use crate::consts::widget_piano::PITCH_MAX;
use crate::message::{ModulatorChange, SnapMode, TrackAutomationTarget};
use maolan_engine::message::PluginGraphNode;
mod core;
mod live_session;
mod plugins;
mod response_freeze_meter;
mod response_session_state;
mod response_state;
mod response_timing_state;
mod response_track;
mod session;
mod session_io;
mod show;
mod timing;
mod track_selection;
mod transport;
mod ui;

const CLIP_EDGE_SNAP_THRESHOLD_PX: f32 = 12.0;
const TRACK_SETUP_MIN_TRACKS_WIDTH: f32 = 338.6557;

struct MoveClipSnapArgs<'a> {
    kind: Kind,
    from_track_name: &'a str,
    clip_index: usize,
    offset: f32,
    group_drag_active: bool,
    selected_group: &'a [usize],
    copy: bool,
}

impl Maolan {
    fn active_workspace_cursor(&self) -> Point {
        let state = self.state.blocking_read();
        state.editor_cursor.unwrap_or(state.cursor)
    }

    /// Removes selected plugins/connections from the currently open track plugin graph,
    /// returning `Some(task)` if anything was selected.
    fn remove_selected_track_plugin_graph_items(&self) -> Option<Task<Message>> {
        let (
            track_name,
            selected_plugins,
            selected_indices,
            connections,
            selected_connectable,
            connectable_connections,
        ) = {
            let state = self.state.blocking_read();
            (
                state.plugin_graph_track.clone(),
                state.plugin_graph_selected_plugins.clone(),
                state.plugin_graph_selected_connections.clone(),
                state.plugin_graph_connections.clone(),
                state.plugin_graph_selected_connectable_connections.clone(),
                state.connectable_connections.clone(),
            )
        };
        let track_name = track_name?;
        if !selected_plugins.is_empty() {
            let mut tasks = Vec::new();
            let mut state = self.state.blocking_write();
            let plugins_to_remove: Vec<usize> = selected_plugins.iter().copied().collect();
            for instance_id in plugins_to_remove {
                let selected_node = state
                    .plugin_graph_plugins
                    .iter()
                    .find(|p| p.instance_id == instance_id)
                    .map(|p| p.node.clone());
                if let Some(node) = selected_node {
                    let task = match node {
                        #[cfg(all(unix, not(target_os = "macos")))]
                        PluginGraphNode::Lv2PluginInstance(_) => {
                            self.send(Action::TrackUnloadLv2PluginInstance {
                                track_name: track_name.clone(),
                                instance_id,
                            })
                        }
                        PluginGraphNode::Vst3PluginInstance(_) => {
                            self.send(Action::TrackUnloadVst3PluginInstance {
                                track_name: track_name.clone(),
                                instance_id,
                            })
                        }
                        PluginGraphNode::ClapPluginInstance(_) => {
                            let plugin_id = state
                                .plugin_graph_plugins
                                .iter()
                                .find(|p| p.instance_id == instance_id)
                                .map(|p| p.plugin_id.clone())
                                .unwrap_or_default();
                            self.send(Action::TrackUnloadClapPlugin {
                                track_name: track_name.clone(),
                                plugin_id,
                            })
                        }
                        PluginGraphNode::TrackInput | PluginGraphNode::TrackOutput => Task::none(),
                    };
                    tasks.push(task);
                }
            }
            state.plugin_graph_selected_plugins.clear();
            state.plugin_graph_selected_connections.clear();
            state.plugin_graph_selected_connectable_connections.clear();
            return Some(Task::batch(tasks));
        }
        if !selected_indices.is_empty() {
            let actions = connections::selection::plugin_disconnect_actions(
                &track_name,
                &connections,
                &selected_indices,
            );
            let tasks = actions
                .into_iter()
                .map(|a| self.send(a))
                .collect::<Vec<_>>();
            let mut state = self.state.blocking_write();
            state.plugin_graph_selected_connections.clear();
            state.plugin_graph_selected_plugins.clear();
            state.plugin_graph_selected_connectable_connections.clear();
            return Some(Task::batch(tasks));
        }
        if !selected_connectable.is_empty() {
            let actions = connections::selection::connectable_disconnect_actions(
                &track_name,
                &connectable_connections,
                &selected_connectable,
            );
            let tasks = actions
                .into_iter()
                .map(|a| self.send(a))
                .collect::<Vec<_>>();
            let mut state = self.state.blocking_write();
            state.plugin_graph_selected_connectable_connections.clear();
            state.plugin_graph_selected_connections.clear();
            state.plugin_graph_selected_plugins.clear();
            return Some(Task::batch(tasks));
        }
        None
    }

    fn clip_edge_snap_threshold_samples(&self) -> f32 {
        (CLIP_EDGE_SNAP_THRESHOLD_PX / self.pixels_per_sample().max(1.0e-6)).max(1.0)
    }

    fn clip_edge_snap_enabled(&self) -> bool {
        matches!(self.snap_mode, crate::message::SnapMode::Clips)
    }

    fn nearest_clip_edge_sample(
        raw_edge: f32,
        snapped_edge: f32,
        threshold_samples: f32,
        candidate_edges: impl IntoIterator<Item = (crate::state::ClipId, usize)>,
    ) -> (f32, Option<crate::state::ClipId>, Vec<crate::state::ClipId>) {
        let mut best: Option<(f32, f32, crate::state::ClipId)> = None;
        let mut matched_targets = Vec::new();

        for (clip_id, edge) in candidate_edges {
            let edge = edge as f32;
            let distance = (raw_edge - edge).abs();
            if distance > threshold_samples {
                continue;
            }
            if !matched_targets.contains(&clip_id) {
                matched_targets.push(clip_id.clone());
            }

            let replace = match best {
                None => true,
                Some((best_distance, best_edge, _)) => {
                    distance < best_distance
                        || (distance == best_distance
                            && (edge - snapped_edge).abs() < (best_edge - snapped_edge).abs())
                }
            };

            if replace {
                best = Some((distance, edge, clip_id));
            }
        }

        if let Some((_, edge, clip_id)) = best {
            (edge.max(0.0), Some(clip_id), matched_targets)
        } else {
            (snapped_edge.max(0.0), None, matched_targets)
        }
    }

    fn snapped_clip_move_start(
        raw_start: f32,
        clip_length: f32,
        snapped_start: f32,
        threshold_samples: f32,
        candidate_edges: impl IntoIterator<Item = (crate::state::ClipId, usize)>,
    ) -> (f32, Option<crate::state::ClipId>, Vec<crate::state::ClipId>) {
        let raw_end = raw_start + clip_length;
        let mut best: Option<(f32, f32, crate::state::ClipId)> = None;
        let mut matched_targets = Vec::new();

        for (clip_id, edge) in candidate_edges {
            let edge = edge as f32;
            let candidates = [(raw_start, edge), (raw_end, edge - clip_length)];

            for (raw_edge, snapped_start_candidate) in candidates {
                if snapped_start_candidate < 0.0 {
                    continue;
                }
                let distance = (raw_edge - edge).abs();
                if distance > threshold_samples {
                    continue;
                }
                if !matched_targets.contains(&clip_id) {
                    matched_targets.push(clip_id.clone());
                }

                let replace = match best {
                    None => true,
                    Some((best_distance, best_start, _)) => {
                        distance < best_distance
                            || (distance == best_distance
                                && (snapped_start_candidate - snapped_start).abs()
                                    < (best_start - snapped_start).abs())
                    }
                };

                if replace {
                    best = Some((distance, snapped_start_candidate, clip_id.clone()));
                }
            }
        }

        if let Some((_, start, clip_id)) = best {
            (start.max(0.0), Some(clip_id), matched_targets)
        } else {
            (snapped_start.max(0.0), None, matched_targets)
        }
    }

    fn clip_snap_edges(
        &self,
        excluded_clips: &[crate::state::ClipId],
    ) -> Vec<(crate::state::ClipId, usize)> {
        let state = self.state.blocking_read();
        state
            .tracks
            .iter()
            .flat_map(|track| {
                let audio = track
                    .audio
                    .clips
                    .iter()
                    .enumerate()
                    .filter_map(|(clip_idx, clip)| {
                        let clip_id = crate::state::ClipId {
                            track_idx: track.name.clone(),
                            clip_idx,
                            kind: Kind::Audio,
                        };
                        (!excluded_clips.contains(&clip_id)).then_some([
                            (clip_id.clone(), clip.start),
                            (clip_id, clip.start.saturating_add(clip.length)),
                        ])
                    });
                let midi = track
                    .midi
                    .clips
                    .iter()
                    .enumerate()
                    .filter_map(|(clip_idx, clip)| {
                        let clip_id = crate::state::ClipId {
                            track_idx: track.name.clone(),
                            clip_idx,
                            kind: Kind::MIDI,
                        };
                        (!excluded_clips.contains(&clip_id)).then_some([
                            (clip_id.clone(), clip.start),
                            (clip_id, clip.start.saturating_add(clip.length)),
                        ])
                    });
                audio.chain(midi)
            })
            .flatten()
            .collect()
    }

    fn move_clip_snap_adjust_and_target(
        &self,
        args: MoveClipSnapArgs<'_>,
    ) -> (f32, Option<crate::state::ClipId>, Vec<crate::state::ClipId>) {
        let MoveClipSnapArgs {
            kind,
            from_track_name,
            clip_index,
            offset,
            group_drag_active,
            selected_group,
            copy,
        } = args;
        if !self.clip_edge_snap_enabled() {
            return (0.0, None, Vec::new());
        }

        let excluded_clips = if !copy {
            if group_drag_active {
                selected_group
                    .iter()
                    .map(|clip_idx| crate::state::ClipId {
                        track_idx: from_track_name.to_string(),
                        clip_idx: *clip_idx,
                        kind,
                    })
                    .collect::<Vec<_>>()
            } else {
                vec![crate::state::ClipId {
                    track_idx: from_track_name.to_string(),
                    clip_idx: clip_index,
                    kind,
                }]
            }
        } else {
            Vec::new()
        };
        let candidate_edges = self.clip_snap_edges(&excluded_clips);
        let clip_edge_snap_threshold_samples = self.clip_edge_snap_threshold_samples();
        let state = self.state.blocking_read();
        let source = state
            .tracks
            .iter()
            .find(|track| track.name == from_track_name)
            .and_then(|track| match kind {
                Kind::Audio => track
                    .audio
                    .clips
                    .get(clip_index)
                    .map(|clip| (clip.start as f32, clip.length as f32)),
                Kind::MIDI => track
                    .midi
                    .clips
                    .get(clip_index)
                    .map(|clip| (clip.start as f32, clip.length as f32)),
            });
        let Some((clip_start, clip_length)) = source else {
            return (0.0, None, Vec::new());
        };
        let raw_start = clip_start + offset;
        let snapped_start = self.snap_sample_to_bar_drag(raw_start, offset) as f32;
        let (resolved_start, snap_target, snap_targets) = Self::snapped_clip_move_start(
            raw_start,
            clip_length,
            snapped_start,
            clip_edge_snap_threshold_samples,
            candidate_edges,
        );
        let resolved_start = if raw_start < 0.0 {
            resolved_start.min(snapped_start)
        } else {
            resolved_start
        };
        (resolved_start - raw_start, snap_target, snap_targets)
    }

    pub(super) fn request_quit(&self) -> Task<Message> {
        self.send(Action::Quit)
    }

    pub(super) fn request_window_close(&mut self) -> Task<Message> {
        if self.is_dirty() {
            self.modal = Some(Show::UnsavedChanges);
            self.state.blocking_write().message =
                "Unsaved changes detected. Save, discard, or cancel.".to_string();
            Task::none()
        } else {
            self.request_quit()
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        struct SyncLogOnDrop(*mut Maolan);

        impl Drop for SyncLogOnDrop {
            fn drop(&mut self) {
                unsafe {
                    (*self.0).sync_message_log_from_state();
                }
            }
        }

        let _sync_log_on_drop = SyncLogOnDrop(self as *mut _);
        if self.handle_simple_ui_message(&message) {
            self.update_children(&message);
            return Task::none();
        }
        if let Some(task) = self.handle_core_message(&message) {
            self.update_children(&message);
            return task;
        }
        if let Some(task) = self.handle_session_io_message(message.clone()) {
            return task;
        }
        if let Some(task) = self.handle_live_session_message(message.clone()) {
            return task;
        }
        if let Some(task) = self.handle_track_selection_message(message.clone()) {
            return task;
        }
        if let Some(task) = self.handle_plugin_message(message.clone()) {
            return task;
        }
        if matches!(
            message,
            Message::ClipRenameShow { .. }
                | Message::ClipToggleFade { .. }
                | Message::ClipSetMuted { .. }
                | Message::ClipAssignToSessionSlot { .. }
                | Message::ClipOpenPitchCorrection { .. }
                | Message::UngroupClip { .. }
                | Message::GroupSelectedClips
        ) {
            self.state.blocking_write().clip_context_menu = None;
        }
        if matches!(
            message,
            Message::TrackAutomationToggleLane { .. }
                | Message::TrackRenameShow(_)
                | Message::TrackAutomationCycleMode { .. }
                | Message::TrackTemplateSaveShow(_)
                | Message::TrackFreezeToggle { .. }
                | Message::TrackFreezeFlatten { .. }
                | Message::TrackToggleFolder { .. }
                | Message::TrackSetFolder { .. }
                | Message::TrackSetParent { .. }
                | Message::TrackMidiLearnArm { .. }
                | Message::TrackMidiLearnClear { .. }
                | Message::TrackAddReturn(_)
                | Message::TrackAddSend(_)
                | Message::Request(_)
                | Message::Show(_)
        ) {
            self.state.blocking_write().track_context_menu = None;
        }
        if matches!(
            message,
            Message::SessionSlotPressed { .. }
                | Message::SessionSlotSetPlayStopIcon { .. }
                | Message::SessionScenePressed(_)
                | Message::SessionStopTrackPressed(_)
                | Message::SessionStopAllPressed
                | Message::SessionSlotDoubleClick { .. }
                | Message::SessionSlotClearRef { .. }
                | Message::SessionSlotDuplicate { .. }
                | Message::SessionSlotDragStart { .. }
                | Message::SessionSlotDropped { .. }
                | Message::SessionSlotRecord { .. }
                | Message::SessionMidiLearnArm { .. }
                | Message::SessionMidiLearnClear { .. }
                | Message::SessionSceneRenameShow(_)
                | Message::SessionSceneRemove(_)
                | Message::SessionSceneSetColor { .. }
                | Message::SessionSceneClearColor(_)
                | Message::SessionSceneSetTempo { .. }
                | Message::SessionSceneSetLaunchQuantization { .. }
        ) {
            let mut state = self.state.blocking_write();
            state.session_slot_context_menu = None;
            state.session_scene_context_menu = None;
        }
        match message {
            Message::Show(ref show) => return self.handle_show_message(show),
            Message::BranchInput(ref value) => {
                self.pending_branch_input = value.clone();
            }
            Message::BranchCreate(ref name) => {
                if let Some(session_dir) = self.session_dir.clone() {
                    let src = session_dir.join(format!("{}.json", self.session_branch));
                    let dst = session_dir.join(format!("{}.json", name));
                    if src.exists() {
                        if let Err(e) = std::fs::copy(&src, &dst) {
                            self.state.blocking_write().message =
                                format!("Failed to create branch '{}': {}", name, e);
                        } else {
                            self.session_branch = name.clone();
                            self.pending_branch_input.clear();
                            self.state.blocking_write().message =
                                format!("Created and switched to branch '{}'", name);
                        }
                    } else {
                        self.state.blocking_write().message =
                            format!("Source branch file '{}' not found", src.display());
                    }
                } else {
                    self.state.blocking_write().message =
                        "No session directory set. Save the session first.".to_string();
                }
            }
            Message::BranchSwitch(ref name) => {
                if let Some(session_dir) = self.session_dir.clone() {
                    let branch_file = session_dir.join(format!("{}.json", name));
                    if branch_file.exists() {
                        self.session_branch = name.clone();
                        self.modal = None;
                        self.stop_recording_preview();
                        self.state.blocking_write().message =
                            format!("Switching to branch '{}'...", name);
                        return Task::perform(async move { session_dir }, Message::LoadSessionPath);
                    } else {
                        self.state.blocking_write().message =
                            format!("Branch file '{}' not found", branch_file.display());
                    }
                } else {
                    self.state.blocking_write().message =
                        "No session directory set. Save the session first.".to_string();
                }
            }
            Message::BranchMerge(ref name) => {
                if let Some(session_dir) = self.session_dir.clone() {
                    if *name == self.session_branch {
                        self.state.blocking_write().message =
                            "Cannot merge a branch into itself".to_string();
                    } else {
                        let src = session_dir.join(format!("{}.json", name));
                        let dst = session_dir.join(format!("{}.json", self.session_branch));
                        if !src.exists() {
                            self.state.blocking_write().message =
                                format!("Branch file '{}' not found", src.display());
                        } else if let Err(e) = std::fs::copy(&src, &dst) {
                            self.state.blocking_write().message =
                                format!("Failed to merge branch '{}': {}", name, e);
                        } else {
                            let commit_dir = session_dir
                                .join(".maolan_commits")
                                .join(&self.session_branch);
                            if let Err(e) = std::fs::create_dir_all(&commit_dir) {
                                self.state.blocking_write().message =
                                    format!("Failed to create commit dir: {}", e);
                            } else {
                                let commit_filename =
                                    chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
                                let commit_path =
                                    commit_dir.join(format!("{}.json", commit_filename));
                                if let Err(e) = std::fs::copy(&dst, &commit_path) {
                                    self.state.blocking_write().message =
                                        format!("Failed to create merge commit: {}", e);
                                } else {
                                    self.modal = None;
                                    self.stop_recording_preview();
                                    self.state.blocking_write().message = format!(
                                        "Merged branch '{}' into '{}'",
                                        name, self.session_branch
                                    );
                                    return Task::perform(
                                        async move { session_dir },
                                        Message::LoadSessionPath,
                                    );
                                }
                            }
                        }
                    }
                } else {
                    self.state.blocking_write().message =
                        "No session directory set. Save the session first.".to_string();
                }
            }
            Message::BranchResetHard(ref name) => {
                if let Some(session_dir) = self.session_dir.clone() {
                    if *name == self.session_branch {
                        self.state.blocking_write().message =
                            "Cannot reset to the current branch".to_string();
                    } else {
                        let src = session_dir.join(format!("{}.json", name));
                        let dst = session_dir.join(format!("{}.json", self.session_branch));
                        if !src.exists() {
                            self.state.blocking_write().message =
                                format!("Branch file '{}' not found", src.display());
                        } else if let Err(e) = std::fs::copy(&src, &dst) {
                            self.state.blocking_write().message =
                                format!("Failed to reset to branch '{}': {}", name, e);
                        } else {
                            self.modal = None;
                            self.stop_recording_preview();
                            self.state.blocking_write().message = format!(
                                "Reset '{}' to state of branch '{}'",
                                self.session_branch, name
                            );
                            return Task::perform(
                                async move { session_dir },
                                Message::LoadSessionPath,
                            );
                        }
                    }
                } else {
                    self.state.blocking_write().message =
                        "No session directory set. Save the session first.".to_string();
                }
            }
            Message::BranchCopyTrack {
                ref branch,
                ref track_name,
            } => {
                if let Some(session_dir) = self.session_dir.clone() {
                    match self.copy_track_from_branch(&session_dir, branch, track_name) {
                        Ok(task) => {
                            self.modal = None;
                            self.stop_recording_preview();
                            self.state.blocking_write().message =
                                format!("Copied track '{}' from branch '{}'", track_name, branch);
                            return task;
                        }
                        Err(e) => {
                            self.state.blocking_write().message = e;
                        }
                    }
                } else {
                    self.state.blocking_write().message =
                        "No session directory set. Save the session first.".to_string();
                }
            }
            Message::AddTrack(crate::message::AddTrack::Submit)
            | Message::AddTrackFromTemplate { .. }
            | Message::ApplyTemplate(crate::message::ApplyTemplate::Submit)
            | Message::ApplyTrackTemplate { .. }
            | Message::NewFromTemplate(_)
            | Message::NewSession
            | Message::Request(_)
            | Message::RequestBatch(_)
            | Message::MeterPollTick => return self.handle_session_message(message),
            Message::EscapePressed => {
                if matches!(
                    self.modal,
                    Some(Show::AddTrack | Show::AddFolder | Show::ApplyTemplate { .. })
                ) {
                    self.modal = None;
                    self.state.blocking_write().apply_template_dialog = None;
                } else if self.state.blocking_read().marker_dialog.is_some() {
                    self.state.blocking_write().marker_dialog = None;
                } else if self
                    .state
                    .blocking_read()
                    .session_slot_context_menu
                    .is_some()
                {
                    self.state.blocking_write().session_slot_context_menu = None;
                }
            }
            Message::Cancel => {
                if self.export_in_progress {
                    self.export_cancel
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                    self.state.blocking_write().message = "Cancelling export...".to_string();
                    return self.send(Action::TrackOfflineBounceCancelAll);
                }
                self.modal = None;
                self.state.blocking_write().apply_template_dialog = None;
            }
            Message::OpenUrl(ref url) => {
                #[cfg(target_os = "macos")]
                let _ = std::process::Command::new("open").arg(url).spawn();
                #[cfg(target_os = "windows")]
                let _ = std::process::Command::new("cmd")
                    .args(["/c", "start", "", url])
                    .spawn();
                #[cfg(all(unix, not(target_os = "macos")))]
                let _ = std::process::Command::new("xdg-open").arg(url).spawn();
            }
            Message::ConfirmCloseSave => {
                self.pending_exit_after_save = true;
                self.modal = None;
                return self.handle_show_message(&Show::Save);
            }
            Message::ConfirmCloseDiscard => {
                return self.request_quit();
            }
            Message::ConfirmCloseCancel => {
                self.pending_exit_after_save = false;
                self.modal = None;
                self.state.blocking_write().message = "Close cancelled".to_string();
                return Task::none();
            }
            Message::TransportPlay
            | Message::TransportPause
            | Message::TransportStop
            | Message::TransportPanic
            | Message::JumpToStart
            | Message::JumpToEnd
            | Message::PlaybackTick
            | Message::AutosaveSnapshotTick
            | Message::SetLoopRange(_)
            | Message::SetPunchRange(_) => return self.handle_transport_message(message),
            Message::TempoAdjust(_)
            | Message::TempoPointAdd(_)
            | Message::TempoPointSelect { .. }
            | Message::TempoPointsMove { .. }
            | Message::TempoSelectionDuplicate
            | Message::TempoSelectionResetToPrevious
            | Message::TempoSelectionDelete
            | Message::TimeSignaturePointAdd(_)
            | Message::TimeSignaturePointSelect { .. }
            | Message::TimeSignaturePointsMove { .. }
            | Message::TimeSignatureSelectionDuplicate
            | Message::TimeSignatureSelectionResetToPrevious
            | Message::TimeSignatureSelectionDelete
            | Message::ClearTimingPointSelection
            | Message::TimeSignatureNumeratorAdjust(_)
            | Message::TimeSignatureDenominatorAdjust(_)
            | Message::TempoInputChanged(_)
            | Message::TempoInputCommit
            | Message::TapTempo
            | Message::TimeSignatureNumeratorInputChanged(_)
            | Message::TimeSignatureDenominatorInputChanged(_)
            | Message::TimeSignatureInputCommit => {
                return self.handle_timing_message(message);
            }
            Message::SetSnapMode(mode) => {
                self.snap_mode = mode;
            }
            Message::SetMidiSnapMode(mode) => {
                self.midi_snap_mode = mode;
            }
            Message::ToggleStepRecording => {
                self.step_recording_active = !self.step_recording_active;
                let enabled = self.step_recording_active;
                if enabled {
                    self.step_recording_cursor_samples = 0;
                }
                return self.send(Action::SetStepRecording(enabled));
            }
            Message::StepRecordNote {
                device: _,
                channel,
                pitch,
                velocity,
            } => {
                return self.handle_step_record_note(channel, pitch, velocity);
            }
            Message::SetClipSnapTargets(ref targets) => {
                self.clip_snap_targets = targets.clone();
            }
            Message::RecordingPreviewTick
                if self.playing
                    && !self.paused
                    && self.record_armed
                    && self.recording_preview_start_sample.is_some() =>
            {
                let sample = self.transport_samples.max(0.0) as usize;
                if self.punch_enabled
                    && let Some((punch_start, punch_end)) = self.punch_range_samples
                    && punch_end > punch_start
                    && (sample < punch_start || sample > punch_end)
                {
                    self.recording_preview_sample = None;
                } else {
                    self.recording_preview_sample = Some(sample);
                }
            }
            Message::RecordingPreviewPeaksTick
                if self.playing
                    && !self.paused
                    && self.record_armed
                    && self.recording_preview_start_sample.is_some() =>
            {
                let sample = self.transport_samples.max(0.0) as usize;
                if self.punch_enabled
                    && let Some((punch_start, punch_end)) = self.punch_range_samples
                    && punch_end > punch_start
                    && (sample < punch_start || sample >= punch_end)
                {
                    return Task::none();
                }
                let peaks = &mut self.recording_preview_peaks;
                let state = self.state.blocking_read();
                for track in state.tracks.iter().filter(|track| track.armed) {
                    let channels = track.audio.outs.max(1);
                    let entry = peaks
                        .entry(track.name.clone())
                        .or_insert_with(|| std::sync::Arc::new(vec![vec![]; channels]));
                    if entry.len() != channels {
                        *entry = std::sync::Arc::new(vec![vec![]; channels]);
                    }
                    let entry_mut = std::sync::Arc::make_mut(entry);
                    for (channel_idx, channel_entry) in
                        entry_mut.iter_mut().enumerate().take(channels)
                    {
                        let db = track
                            .meter_out_db
                            .get(channel_idx)
                            .copied()
                            .unwrap_or(-90.0);
                        let amp = if db <= -90.0 {
                            0.0
                        } else {
                            10.0_f32.powf(db / 20.0).clamp(0.0, 1.0)
                        };
                        channel_entry.push([-amp, amp]);
                    }
                }
            }
            Message::ZoomSliderChanged(value) => {
                self.zoom_visible_bars = crate::gui::zoom_slider_to_visible_bars(value);
                let max_scroll = self.editor_max_scroll_samples();
                self.editor_scroll_origin_samples =
                    self.editor_scroll_origin_samples.clamp(0.0, max_scroll);
                self.editor_scroll_x = self.editor_scroll_relative_x();
                return self.sync_editor_scrollbars();
            }
            Message::EditorScrollXChanged(value) => {
                let x = value.clamp(0.0, 1.0);
                if (self.editor_scroll_x - x).abs() > 0.0005 {
                    self.editor_scroll_x = x;
                    self.editor_scroll_origin_samples = self.editor_max_scroll_samples() * x as f64;
                    return self.sync_editor_scrollbars();
                }
            }
            Message::EditorScrollYChanged(value) => {
                let y = value.clamp(0.0, 1.0);
                if (self.editor_scroll_y - y).abs() > 0.0005 {
                    if matches!(
                        self.state.blocking_read().resizing,
                        Some(crate::state::Resizing::Track(..))
                    ) {
                        return Task::none();
                    }
                    self.editor_scroll_y = y;
                    return self.sync_editor_scrollbars();
                }
            }
            Message::MixerScrollXChanged(value) => {
                let x = value.clamp(0.0, 1.0);
                if (self.mixer_scroll_x - x).abs() > 0.0005 {
                    self.mixer_scroll_x = x;
                }
            }
            Message::PianoZoomXChanged(value) => {
                self.state.blocking_write().piano_zoom_x = value;
                return self.sync_piano_scrollbars();
            }
            Message::PianoZoomYChanged(value) => {
                self.state.blocking_write().piano_zoom_y = value;
                return self.sync_piano_scrollbars();
            }
            Message::PianoScrollChanged { x, y } => {
                let x = x.clamp(0.0, 1.0);
                let y = y.clamp(0.0, 1.0);
                let changed = {
                    let mut state = self.state.blocking_write();
                    let changed = (state.piano_scroll_x - x).abs() > 0.0005
                        || (state.piano_scroll_y - y).abs() > 0.0005;
                    if changed {
                        state.piano_scroll_x = x;
                        state.piano_scroll_y = y;
                    }
                    changed
                };
                if changed {
                    return self.sync_piano_scrollbars();
                }
            }
            Message::PianoScrollXChanged(value) => {
                let x = value.clamp(0.0, 1.0);
                let changed = {
                    let mut state = self.state.blocking_write();
                    let changed = (state.piano_scroll_x - x).abs() > 0.0005;
                    if changed {
                        state.piano_scroll_x = x;
                    }
                    changed
                };
                if changed {
                    return self.sync_piano_scrollbars();
                }
            }
            Message::PianoScrollYChanged(value) => {
                let y = value.clamp(0.0, 1.0);
                let changed = {
                    let mut state = self.state.blocking_write();
                    let changed = (state.piano_scroll_y - y).abs() > 0.0005;
                    if changed {
                        state.piano_scroll_y = y;
                    }
                    changed
                };
                if changed {
                    return self.sync_piano_scrollbars();
                }
            }
            Message::PianoSysExScrollYChanged(value) => {
                let y = value.clamp(0.0, 1.0);
                let changed = {
                    let mut state = self.state.blocking_write();
                    let changed = (state.piano_sysex_scroll_y - y).abs() > 0.0005;
                    if changed {
                        state.piano_sysex_scroll_y = y;
                    }
                    changed
                };
                if changed {
                    return operation::snap_to(
                        Id::new(SYSEX_SCROLL_ID),
                        operation::RelativeOffset {
                            x: None,
                            y: Some(y),
                        },
                    );
                }
            }
            Message::PianoControllerLaneSelected(lane) => {
                let mut state = self.state.blocking_write();
                state.piano_controller_lane = lane;
                state.piano_sysex_panel_open =
                    matches!(lane, crate::message::PianoControllerLane::SysEx);
            }
            Message::MidiEditorViewModeSelected(mode) => {
                let state = self.state.blocking_read();
                if let Some(piano) = state.piano.as_ref() {
                    let track_name = piano.track_idx.clone();
                    drop(state);
                    let mut state = self.state.blocking_write();
                    if let Some(track) = state.tracks.iter_mut().find(|t| t.name == track_name) {
                        track.midi.editor_view_mode = mode;
                    }
                }
            }
            Message::PianoControllerKindSelected(kind) => {
                let mut state = self.state.blocking_write();
                state.piano_controller_lane = crate::message::PianoControllerLane::Controller;
                state.piano_controller_kind = kind;
                state.piano_sysex_panel_open = false;
            }
            Message::PianoVelocityKindSelected(kind) => {
                let mut state = self.state.blocking_write();
                state.piano_controller_lane = crate::message::PianoControllerLane::Velocity;
                state.piano_velocity_kind = kind;
                state.piano_sysex_panel_open = false;
            }
            Message::PianoRpnKindSelected(kind) => {
                let mut state = self.state.blocking_write();
                state.piano_controller_lane = crate::message::PianoControllerLane::Rpn;
                state.piano_rpn_kind = kind;
                state.piano_sysex_panel_open = false;
            }
            Message::PianoNrpnKindSelected(kind) => {
                let mut state = self.state.blocking_write();
                state.piano_controller_lane = crate::message::PianoControllerLane::Nrpn;
                state.piano_nrpn_kind = kind;
                state.piano_sysex_panel_open = false;
            }
            Message::PianoKeyPressed(note) => {
                let track_name = self
                    .state
                    .blocking_read()
                    .piano
                    .as_ref()
                    .map(|p| p.track_idx.clone());
                if let Some(track_name) = track_name {
                    return self.send(Action::PianoKey {
                        track_name,
                        note,
                        velocity: 100,
                        on: true,
                    });
                }
            }
            Message::PianoKeyReleased(note) => {
                let track_name = self
                    .state
                    .blocking_read()
                    .piano
                    .as_ref()
                    .map(|p| p.track_idx.clone());
                if let Some(track_name) = track_name {
                    return self.send(Action::PianoKey {
                        track_name,
                        note,
                        velocity: 0,
                        on: false,
                    });
                }
            }
            Message::PianoNoteClick {
                note_index,
                position,
            } => {
                let mut state = self.state.blocking_write();
                let shift = state.shift;

                if shift {
                    if state.piano_selected_notes.contains(&note_index) {
                        state.piano_selected_notes.remove(&note_index);
                    } else {
                        state.piano_selected_notes.insert(note_index);
                    }
                } else {
                    if !state.piano_selected_notes.contains(&note_index) {
                        state.piano_selected_notes.clear();
                        state.piano_selected_notes.insert(note_index);
                    }
                }

                if !state.piano_selected_notes.is_empty()
                    && let Some(piano) = state.piano.as_ref()
                {
                    let selected_indices: Vec<usize> =
                        state.piano_selected_notes.iter().copied().collect();
                    let original_notes: Vec<crate::state::PianoNote> = selected_indices
                        .iter()
                        .filter_map(|&idx| piano.notes.get(idx).cloned())
                        .collect();

                    state.piano_dragging_notes = Some(crate::state::DraggingNotes {
                        note_indices: selected_indices,
                        start_point: position,
                        current_point: position,
                        original_notes,
                    });
                }
            }
            Message::PianoNotesDrag { position } => {
                let mut state = self.state.blocking_write();
                if let Some(ref mut dragging) = state.piano_dragging_notes {
                    dragging.current_point = position;
                }
            }
            Message::PianoNotesEndDrag => {
                let mut state = self.state.blocking_write();
                let copy = state.ctrl;
                if let Some(dragging) = state.piano_dragging_notes.take() {
                    let zoom_x = state.piano_zoom_x;
                    let zoom_y = state.piano_zoom_y;
                    let tempo = state.tempo.max(1.0) as f64;
                    let tsig_num = state.time_signature_num.max(1) as f64;
                    let tsig_denom = state.time_signature_denom.max(1) as f64;
                    let row_h = ((14.0 * 7.0 / 12.0) * zoom_y).max(1.0);
                    let tracks_width = match state.tracks_width {
                        Length::Fixed(v) => v,
                        _ => 200.0,
                    };
                    let editor_width = (self.size.width - tracks_width - 3.0).max(1.0);
                    let samples_per_beat =
                        (self.playback_rate_hz * 60.0 / tempo) * (4.0 / tsig_denom);
                    let samples_per_bar = samples_per_beat * tsig_num;
                    let total_samples = (samples_per_bar * self.zoom_visible_bars as f64).max(1.0);
                    let pps = ((editor_width as f64 / total_samples) as f32 * zoom_x).max(1.0e-6);

                    let delta_x = dragging.current_point.x - dragging.start_point.x;
                    let delta_y = dragging.current_point.y - dragging.start_point.y;

                    let delta_samples = (delta_x / pps) as i64;
                    let delta_pitch = -(delta_y / row_h).round() as i8;

                    let snap_sample = |sample: f64| -> usize {
                        if matches!(
                            self.midi_snap_mode,
                            crate::message::SnapMode::NoSnap | crate::message::SnapMode::Clips
                        ) {
                            return sample.max(0.0) as usize;
                        }
                        self.midi_snap_mode
                            .snap_sample_drag(
                                sample,
                                delta_samples as f64,
                                samples_per_beat,
                                samples_per_bar,
                            )
                            .max(0.0) as usize
                    };

                    if copy && let Some(piano) = state.piano.as_ref() {
                        let track_name = piano.track_idx.clone();
                        let clip_idx = piano.clip_index;
                        let insert_base = piano.notes.len();

                        let notes: Vec<(usize, maolan_engine::message::MidiNoteData)> = dragging
                            .original_notes
                            .iter()
                            .enumerate()
                            .map(|(offset, note)| {
                                let new_start =
                                    snap_sample(note.start_sample as f64 + delta_samples as f64);
                                let new_pitch =
                                    (note.pitch as i16 + delta_pitch as i16).clamp(0, 127) as u8;
                                (
                                    insert_base + offset,
                                    maolan_engine::message::MidiNoteData {
                                        start_sample: new_start,
                                        length_samples: note.length_samples,
                                        pitch: new_pitch,
                                        velocity: note.velocity,
                                        channel: note.channel,
                                    },
                                )
                            })
                            .collect();

                        state.piano_selected_notes.clear();
                        drop(state);
                        return self.send(Action::InsertMidiNotes {
                            track_name,
                            clip_index: clip_idx,
                            notes,
                        });
                    }

                    if let Some(piano) = state.piano.as_mut() {
                        let track_name = piano.track_idx.clone();
                        let clip_idx = piano.clip_index;

                        for &note_idx in &dragging.note_indices {
                            if let Some(note) = piano.notes.get_mut(note_idx) {
                                let new_start =
                                    snap_sample(note.start_sample as f64 + delta_samples as f64);
                                let new_pitch =
                                    (note.pitch as i16 + delta_pitch as i16).clamp(0, 127) as u8;
                                note.start_sample = new_start;
                                note.pitch = new_pitch;
                            }
                        }

                        let new_notes: Vec<maolan_engine::message::MidiNoteData> = dragging
                            .note_indices
                            .iter()
                            .filter_map(|&idx| piano.notes.get(idx))
                            .map(|note| maolan_engine::message::MidiNoteData {
                                start_sample: note.start_sample,
                                length_samples: note.length_samples,
                                pitch: note.pitch,
                                velocity: note.velocity,
                                channel: note.channel,
                            })
                            .collect();
                        let old_notes: Vec<maolan_engine::message::MidiNoteData> = dragging
                            .original_notes
                            .iter()
                            .map(|note| maolan_engine::message::MidiNoteData {
                                start_sample: note.start_sample,
                                length_samples: note.length_samples,
                                pitch: note.pitch,
                                velocity: note.velocity,
                                channel: note.channel,
                            })
                            .collect();

                        drop(state);
                        return self.send(Action::ModifyMidiNotes {
                            track_name,
                            clip_index: clip_idx,
                            note_indices: dragging.note_indices,
                            new_notes,
                            old_notes,
                        });
                    }
                }
            }
            Message::PitchCorrectionPointClick {
                point_index,
                position,
            } => {
                let mut state = self.state.blocking_write();
                let shift = state.shift;

                if shift {
                    if state
                        .pitch_correction_selected_points
                        .contains(&point_index)
                    {
                        state.pitch_correction_selected_points.remove(&point_index);
                    } else {
                        state.pitch_correction_selected_points.insert(point_index);
                    }
                } else if !state
                    .pitch_correction_selected_points
                    .contains(&point_index)
                {
                    state.pitch_correction_selected_points.clear();
                    state.pitch_correction_selected_points.insert(point_index);
                }

                if !state.pitch_correction_selected_points.is_empty()
                    && let Some(pitch_correction) = state.pitch_correction.as_ref()
                {
                    let point_indices: Vec<usize> = state
                        .pitch_correction_selected_points
                        .iter()
                        .copied()
                        .collect();
                    let original_points = point_indices
                        .iter()
                        .filter_map(|&idx| pitch_correction.points.get(idx).cloned())
                        .collect();
                    state.pitch_correction_dragging_points =
                        Some(crate::state::DraggingPitchCorrectionPoints {
                            point_indices,
                            start_point: position,
                            current_point: position,
                            original_points,
                        });
                }
            }
            Message::PitchCorrectionSnapToNearest { point_index } => {
                return self.snap_pitch_correction_points_to_nearest(point_index);
            }
            Message::PitchCorrectionPointsDrag { position } => {
                let mut state = self.state.blocking_write();
                if let Some(ref mut dragging) = state.pitch_correction_dragging_points {
                    dragging.current_point = position;
                }
            }
            Message::PitchCorrectionPointsEndDrag => {
                let mut state = self.state.blocking_write();
                if let Some(dragging) = state.pitch_correction_dragging_points.take() {
                    let zoom_y = state.piano_zoom_y;
                    let row_h = ((14.0 * 7.0 / 12.0) * zoom_y).max(1.0);
                    let delta_y = dragging.current_point.y - dragging.start_point.y;
                    let delta_pitch = -(delta_y / row_h);
                    if delta_pitch.abs() <= f32::EPSILON {
                        return Task::none();
                    }
                    if let Some(pitch_correction) = state.pitch_correction.as_mut() {
                        for (point_idx, original_point) in dragging
                            .point_indices
                            .iter()
                            .copied()
                            .zip(dragging.original_points.iter())
                        {
                            if let Some(point) = pitch_correction.points.get_mut(point_idx) {
                                point.target_midi_pitch = (original_point.target_midi_pitch
                                    + delta_pitch)
                                    .clamp(0.0, f32::from(PITCH_MAX) + 0.999);
                            }
                        }
                        state.message = format!(
                            "Adjusted {} pitch segment{}",
                            dragging.point_indices.len(),
                            if dragging.point_indices.len() == 1 {
                                ""
                            } else {
                                "s"
                            }
                        );
                        drop(state);
                        return self.sync_pitch_correction_realtime();
                    }
                }
            }
            Message::PitchCorrectionSelectRectStart { position } => {
                let mut state = self.state.blocking_write();
                state.pitch_correction_dragging_points = None;
                state.pitch_correction_selecting_rect = Some((position, position));
            }
            Message::PitchCorrectionSelectRectDrag { position } => {
                let base_pps = self.pixels_per_sample();
                let mut state = self.state.blocking_write();
                if let Some((start, _)) = state.pitch_correction_selecting_rect {
                    state.pitch_correction_selecting_rect = Some((start, position));
                    let shift = state.shift;
                    let Some(pitch_correction) = state.pitch_correction.as_ref() else {
                        return Task::none();
                    };
                    let left = start.x.min(position.x);
                    let right = start.x.max(position.x);
                    let top = start.y.min(position.y);
                    let bottom = start.y.max(position.y);
                    let zoom_y = state.piano_zoom_y;
                    let row_h = ((14.0 * 7.0 / 12.0) * zoom_y).max(1.0);
                    let pps = base_pps * state.piano_zoom_x.max(1.0);
                    let selected = pitch_correction
                        .points
                        .iter()
                        .enumerate()
                        .filter_map(|(idx, point)| {
                            let x = point.start_sample as f32 * pps;
                            let width = (point.length_samples as f32 * pps).max(6.0);
                            let y = (f32::from(PITCH_MAX)
                                - point.target_midi_pitch.clamp(0.0, f32::from(PITCH_MAX))
                                + 0.5)
                                * row_h;
                            let height =
                                (row_h * (0.45 + 0.35 * point.clarity.clamp(0.0, 1.0))).max(6.0);
                            let rect_left = x;
                            let rect_right = x + width;
                            let rect_top = y - height * 0.5;
                            let rect_bottom = rect_top + height;
                            (rect_left < right
                                && rect_right > left
                                && rect_top < bottom
                                && rect_bottom > top)
                                .then_some(idx)
                        })
                        .collect::<std::collections::HashSet<_>>();
                    if shift {
                        state
                            .pitch_correction_selected_points
                            .extend(selected.iter().copied());
                    } else {
                        state.pitch_correction_selected_points = selected;
                    }
                }
            }
            Message::PitchCorrectionSelectRectEnd => {
                let mut state = self.state.blocking_write();
                state.pitch_correction_selecting_rect = None;
            }
            Message::PitchCorrectionClearSelection => {
                let mut state = self.state.blocking_write();
                state.pitch_correction_selected_points.clear();
                state.pitch_correction_dragging_points = None;
                state.pitch_correction_selecting_rect = None;
            }
            Message::SelectAll => {
                let view = self.state.blocking_read().view.clone();
                if matches!(view, crate::state::View::PitchCorrection) {
                    let mut state = self.state.blocking_write();
                    let all_points = state
                        .pitch_correction
                        .as_ref()
                        .map(|pitch_correction| {
                            (0..pitch_correction.points.len())
                                .collect::<std::collections::HashSet<_>>()
                        })
                        .unwrap_or_default();
                    state.pitch_correction_selected_points = all_points;
                    state.pitch_correction_dragging_points = None;
                    state.pitch_correction_selecting_rect = None;
                    return Task::none();
                }
            }
            Message::PitchCorrectionFrameLikenessChanged(value) => {
                let clamped = value.clamp(0.05, 2.0);
                let mut state = self.state.blocking_write();
                state.pitch_correction_frame_likeness = clamped;
                if let Some(pitch_correction) = state.pitch_correction.as_mut() {
                    Self::regroup_pitch_correction_frames(pitch_correction, clamped);
                    state.pitch_correction_selected_points.clear();
                    state.pitch_correction_dragging_points = None;
                    state.pitch_correction_selecting_rect = None;
                }
                drop(state);
                return self.sync_pitch_correction_realtime();
            }
            Message::PitchCorrectionInertiaChanged(value) => {
                self.state.blocking_write().pitch_correction_inertia_ms = value.min(1000);
                return self.sync_pitch_correction_realtime();
            }
            Message::PitchCorrectionFormantCompensationChanged(enabled) => {
                self.state
                    .blocking_write()
                    .pitch_correction_formant_compensation = enabled;
                return self.sync_pitch_correction_realtime();
            }
            Message::PianoNoteResizeStart {
                note_index,
                position,
                resize_start,
            } => {
                let mut state = self.state.blocking_write();
                state.piano_selected_notes.clear();
                state.piano_selected_notes.insert(note_index);
                if let Some(piano) = state.piano.as_ref()
                    && let Some(note) = piano.notes.get(note_index)
                {
                    state.piano_resizing_note = Some(crate::state::ResizingNote {
                        note_index,
                        resize_start,
                        start_point: position,
                        current_point: position,
                        original_note: note.clone(),
                    });
                }
            }
            Message::PianoNoteResizeDrag { position } => {
                let mut state = self.state.blocking_write();
                if let Some(ref mut resizing) = state.piano_resizing_note {
                    resizing.current_point = position;
                }
            }
            Message::PianoNoteResizeEnd => {
                let mut state = self.state.blocking_write();
                if let Some(resizing) = state.piano_resizing_note.take() {
                    let zoom_x = state.piano_zoom_x;
                    let tempo = state.tempo.max(1.0) as f64;
                    let tsig_num = state.time_signature_num.max(1) as f64;
                    let tsig_denom = state.time_signature_denom.max(1) as f64;
                    let tracks_width = match state.tracks_width {
                        Length::Fixed(v) => v,
                        _ => 200.0,
                    };
                    let editor_width = (self.size.width - tracks_width - 3.0).max(1.0);
                    let samples_per_beat =
                        (self.playback_rate_hz * 60.0 / tempo) * (4.0 / tsig_denom);
                    let samples_per_bar = samples_per_beat * tsig_num;
                    let total_samples = (samples_per_bar * self.zoom_visible_bars as f64).max(1.0);
                    let pps = ((editor_width as f64 / total_samples) as f32 * zoom_x).max(1.0e-6);

                    let delta_x = resizing.current_point.x - resizing.start_point.x;
                    let delta_samples = (delta_x / pps) as i64;

                    let snap_sample = |sample: f64| -> usize {
                        if matches!(
                            self.midi_snap_mode,
                            crate::message::SnapMode::NoSnap | crate::message::SnapMode::Clips
                        ) {
                            return sample.max(0.0) as usize;
                        }
                        self.midi_snap_mode
                            .snap_sample_drag(
                                sample,
                                delta_samples as f64,
                                samples_per_beat,
                                samples_per_bar,
                            )
                            .max(0.0) as usize
                    };

                    let original = &resizing.original_note;
                    let original_end = original
                        .start_sample
                        .saturating_add(original.length_samples)
                        .max(1);
                    let (new_start, new_len) = if resizing.resize_start {
                        let max_start = original_end.saturating_sub(1) as i64;
                        let start =
                            snap_sample(original.start_sample as f64 + delta_samples as f64)
                                .min(max_start as usize);
                        (start, original_end.saturating_sub(start).max(1))
                    } else {
                        let min_end = original.start_sample.saturating_add(1) as i64;
                        let end = snap_sample(original_end as f64 + delta_samples as f64)
                            .max(min_end as usize);
                        (
                            original.start_sample,
                            end.saturating_sub(original.start_sample).max(1),
                        )
                    };

                    if let Some(piano) = state.piano.as_mut()
                        && let Some(note) = piano.notes.get_mut(resizing.note_index)
                    {
                        let track_name = piano.track_idx.clone();
                        let clip_idx = piano.clip_index;

                        note.start_sample = new_start;
                        note.length_samples = new_len;

                        let new_note = maolan_engine::message::MidiNoteData {
                            start_sample: note.start_sample,
                            length_samples: note.length_samples,
                            pitch: note.pitch,
                            velocity: note.velocity,
                            channel: note.channel,
                        };
                        let old_note = maolan_engine::message::MidiNoteData {
                            start_sample: original.start_sample,
                            length_samples: original.length_samples,
                            pitch: original.pitch,
                            velocity: original.velocity,
                            channel: original.channel,
                        };

                        drop(state);
                        return self.send(Action::ModifyMidiNotes {
                            track_name,
                            clip_index: clip_idx,
                            note_indices: vec![resizing.note_index],
                            new_notes: vec![new_note],
                            old_notes: vec![old_note],
                        });
                    }
                }
            }
            Message::PianoAdjustVelocity { note_index, delta } => {
                if delta == 0 {
                    return Task::none();
                }
                let mut state = self.state.blocking_write();
                let selected_contains = state.piano_selected_notes.contains(&note_index);
                let selected_len = state.piano_selected_notes.len();
                let mut target_indices: Vec<usize> = if selected_contains && selected_len > 1 {
                    state.piano_selected_notes.iter().copied().collect()
                } else {
                    vec![note_index]
                };
                let Some(piano) = state.piano.as_mut() else {
                    return Task::none();
                };
                if note_index >= piano.notes.len() {
                    return Task::none();
                }
                target_indices.sort_unstable();
                target_indices.dedup();

                let mut changed_indices = Vec::new();
                let mut new_notes = Vec::new();
                let mut old_notes = Vec::new();

                for idx in target_indices {
                    let Some(note) = piano.notes.get_mut(idx) else {
                        continue;
                    };
                    let old_note = maolan_engine::message::MidiNoteData {
                        start_sample: note.start_sample,
                        length_samples: note.length_samples,
                        pitch: note.pitch,
                        velocity: note.velocity,
                        channel: note.channel,
                    };
                    let new_velocity =
                        (i16::from(note.velocity) + i16::from(delta)).clamp(0, 127) as u8;
                    if new_velocity == note.velocity {
                        continue;
                    }
                    note.velocity = new_velocity;
                    let new_note = maolan_engine::message::MidiNoteData {
                        start_sample: note.start_sample,
                        length_samples: note.length_samples,
                        pitch: note.pitch,
                        velocity: note.velocity,
                        channel: note.channel,
                    };
                    changed_indices.push(idx);
                    new_notes.push(new_note);
                    old_notes.push(old_note);
                }

                if changed_indices.is_empty() {
                    return Task::none();
                }
                let track_name = piano.track_idx.clone();
                let clip_idx = piano.clip_index;
                drop(state);
                return self.send(Action::ModifyMidiNotes {
                    track_name,
                    clip_index: clip_idx,
                    note_indices: changed_indices,
                    new_notes,
                    old_notes,
                });
            }
            Message::PianoSetVelocity {
                note_index,
                velocity,
            } => {
                let mut state = self.state.blocking_write();
                let Some(piano) = state.piano.as_mut() else {
                    return Task::none();
                };
                let Some(note) = piano.notes.get_mut(note_index) else {
                    return Task::none();
                };
                if note.velocity == velocity {
                    return Task::none();
                }
                let old_note = maolan_engine::message::MidiNoteData {
                    start_sample: note.start_sample,
                    length_samples: note.length_samples,
                    pitch: note.pitch,
                    velocity: note.velocity,
                    channel: note.channel,
                };
                note.velocity = velocity;
                let new_note = maolan_engine::message::MidiNoteData {
                    start_sample: note.start_sample,
                    length_samples: note.length_samples,
                    pitch: note.pitch,
                    velocity: note.velocity,
                    channel: note.channel,
                };
                let track_name = piano.track_idx.clone();
                let clip_idx = piano.clip_index;
                drop(state);
                return self.send(Action::ModifyMidiNotes {
                    track_name,
                    clip_index: clip_idx,
                    note_indices: vec![note_index],
                    new_notes: vec![new_note],
                    old_notes: vec![old_note],
                });
            }
            Message::PianoAdjustController {
                controller_index,
                delta,
            } => {
                if delta == 0 {
                    return Task::none();
                }
                let mut state = self.state.blocking_write();
                let Some(piano) = state.piano.as_mut() else {
                    return Task::none();
                };
                let Some(ctrl) = piano.controllers.get_mut(controller_index) else {
                    return Task::none();
                };
                let old_ctrl = maolan_engine::message::MidiControllerData {
                    sample: ctrl.sample,
                    controller: ctrl.controller,
                    value: ctrl.value,
                    channel: ctrl.channel,
                };
                let new_value = (i16::from(ctrl.value) + i16::from(delta)).clamp(0, 127) as u8;
                if new_value == ctrl.value {
                    return Task::none();
                }
                ctrl.value = new_value;
                let new_ctrl = maolan_engine::message::MidiControllerData {
                    sample: ctrl.sample,
                    controller: ctrl.controller,
                    value: ctrl.value,
                    channel: ctrl.channel,
                };
                let track_name = piano.track_idx.clone();
                let clip_idx = piano.clip_index;
                drop(state);
                return self.send(Action::ModifyMidiControllers {
                    track_name,
                    clip_index: clip_idx,
                    controller_indices: vec![controller_index],
                    new_controllers: vec![new_ctrl],
                    old_controllers: vec![old_ctrl],
                });
            }
            Message::PianoSetControllerValue {
                controller_index,
                value,
            } => {
                let mut state = self.state.blocking_write();
                let Some(piano) = state.piano.as_mut() else {
                    return Task::none();
                };
                let Some(ctrl) = piano.controllers.get_mut(controller_index) else {
                    return Task::none();
                };
                if ctrl.value == value {
                    return Task::none();
                }
                let old_ctrl = maolan_engine::message::MidiControllerData {
                    sample: ctrl.sample,
                    controller: ctrl.controller,
                    value: ctrl.value,
                    channel: ctrl.channel,
                };
                ctrl.value = value;
                let new_ctrl = maolan_engine::message::MidiControllerData {
                    sample: ctrl.sample,
                    controller: ctrl.controller,
                    value: ctrl.value,
                    channel: ctrl.channel,
                };
                let track_name = piano.track_idx.clone();
                let clip_idx = piano.clip_index;
                drop(state);
                return self.send(Action::ModifyMidiControllers {
                    track_name,
                    clip_index: clip_idx,
                    controller_indices: vec![controller_index],
                    new_controllers: vec![new_ctrl],
                    old_controllers: vec![old_ctrl],
                });
            }
            Message::PianoInsertControllers { controllers } => {
                if controllers.is_empty() {
                    return Task::none();
                }
                let mut state = self.state.blocking_write();
                let Some(piano) = state.piano.as_mut() else {
                    return Task::none();
                };
                let track_name = piano.track_idx.clone();
                let clip_idx = piano.clip_index;
                let min_sample = controllers.iter().map(|c| c.sample).min().unwrap_or(0);
                let max_sample = controllers
                    .iter()
                    .map(|c| c.sample)
                    .max()
                    .unwrap_or(min_sample);
                let drawn_controllers: HashSet<u8> =
                    controllers.iter().map(|c| c.controller).collect();

                let mut delete_indices: Vec<usize> = Vec::new();
                let mut deleted_payload: Vec<(usize, maolan_engine::message::MidiControllerData)> =
                    Vec::new();
                for (idx, ctrl) in piano.controllers.iter().enumerate() {
                    if ctrl.sample < min_sample || ctrl.sample > max_sample {
                        continue;
                    }
                    if !drawn_controllers.contains(&ctrl.controller) {
                        continue;
                    }
                    delete_indices.push(idx);
                    deleted_payload.push((
                        idx,
                        maolan_engine::message::MidiControllerData {
                            sample: ctrl.sample,
                            controller: ctrl.controller,
                            value: ctrl.value,
                            channel: ctrl.channel,
                        },
                    ));
                }

                let controllers_len = piano.controllers.len();
                let payload: Vec<(usize, maolan_engine::message::MidiControllerData)> = controllers
                    .into_iter()
                    .enumerate()
                    .map(|(offset, ctrl)| {
                        (
                            controllers_len + offset,
                            maolan_engine::message::MidiControllerData {
                                sample: ctrl.sample,
                                controller: ctrl.controller,
                                value: ctrl.value,
                                channel: ctrl.channel,
                            },
                        )
                    })
                    .collect();
                drop(state);
                let mut tasks = Vec::new();
                tasks.push(self.send(Action::BeginHistoryGroup));
                if !delete_indices.is_empty() {
                    delete_indices.sort_unstable();
                    delete_indices.dedup();
                    let mut delete_indices_desc = delete_indices.clone();
                    delete_indices_desc.sort_unstable_by(|a, b| b.cmp(a));

                    tasks.push(self.send(Action::DeleteMidiControllers {
                        track_name: track_name.clone(),
                        clip_index: clip_idx,
                        controller_indices: delete_indices_desc,
                        deleted_controllers: deleted_payload,
                    }));
                }
                let insert_adjusted: Vec<(usize, maolan_engine::message::MidiControllerData)> =
                    if delete_indices.is_empty() {
                        payload
                    } else {
                        payload
                            .into_iter()
                            .enumerate()
                            .map(|(offset, (_, ctrl))| {
                                let shifted_index = controllers_len
                                    .saturating_sub(delete_indices.len())
                                    .saturating_add(offset);
                                (shifted_index, ctrl)
                            })
                            .collect()
                    };
                tasks.push(self.send(Action::InsertMidiControllers {
                    track_name,
                    clip_index: clip_idx,
                    controllers: insert_adjusted,
                }));
                tasks.push(self.send(Action::EndHistoryGroup));
                return Task::batch(tasks);
            }
            Message::PianoSysExSelect(index) => {
                let mut state = self.state.blocking_write();
                state.piano_selected_sysex = index;
                state.piano_sysex_hex_input = index
                    .and_then(|idx| state.piano.as_ref()?.sysexes.get(idx).cloned())
                    .map(|ev| Self::format_sysex_hex(&ev.data))
                    .unwrap_or_default();
            }
            Message::PianoSysExOpenEditor(index) => {
                let mut state = self.state.blocking_write();
                state.piano_controller_lane = crate::message::PianoControllerLane::SysEx;
                state.piano_selected_sysex = index;
                state.piano_sysex_hex_input = index
                    .and_then(|idx| state.piano.as_ref()?.sysexes.get(idx).cloned())
                    .map(|ev| Self::format_sysex_hex(&ev.data))
                    .unwrap_or_default();
                state.piano_sysex_panel_open = true;
            }
            Message::PianoSysExCloseEditor => {
                self.state.blocking_write().piano_sysex_panel_open = false;
            }
            Message::PianoSysExHexInput(ref input) => {
                self.state.blocking_write().piano_sysex_hex_input = input.clone();
            }
            Message::PianoSysExAdd => {
                let mut state = self.state.blocking_write();
                state.piano_sysex_panel_open = false;
                let input = state.piano_sysex_hex_input.clone();
                let payload = match Self::parse_sysex_hex(&input) {
                    Ok(v) => v,
                    Err(e) => {
                        state.message = e;
                        return Task::none();
                    }
                };
                let selected_hint = state.piano_selected_sysex;
                let Some(piano) = state.piano.as_mut() else {
                    return Task::none();
                };
                let old_sysex_events = Self::sysex_to_engine(&piano.sysexes);
                let sample = selected_hint
                    .and_then(|idx| piano.sysexes.get(idx).map(|s| s.sample))
                    .unwrap_or(0);
                piano.sysexes.push(PianoSysExPoint {
                    sample,
                    data: payload,
                });
                piano.sysexes.sort_by_key(|s| s.sample);
                let new_index = piano.sysexes.len().saturating_sub(1);
                let track_name = piano.track_idx.clone();
                let clip_index = piano.clip_index;
                let new_sysex_events = Self::sysex_to_engine(&piano.sysexes);
                let new_hex = Self::format_sysex_hex(&piano.sysexes[new_index].data);
                state.piano_selected_sysex = Some(new_index);
                state.piano_sysex_hex_input = new_hex;
                drop(state);
                return self.send(Action::SetMidiSysExEvents {
                    track_name,
                    clip_index,
                    new_sysex_events,
                    old_sysex_events,
                });
            }
            Message::PianoSysExUpdate => {
                let mut state = self.state.blocking_write();
                state.piano_sysex_panel_open = false;
                let input = state.piano_sysex_hex_input.clone();
                let payload = match Self::parse_sysex_hex(&input) {
                    Ok(v) => v,
                    Err(e) => {
                        state.message = e;
                        return Task::none();
                    }
                };
                let Some(selected_idx) = state.piano_selected_sysex else {
                    return Task::none();
                };
                let Some(piano) = state.piano.as_mut() else {
                    return Task::none();
                };
                if selected_idx >= piano.sysexes.len() {
                    return Task::none();
                }
                let old_sysex_events = Self::sysex_to_engine(&piano.sysexes);
                piano.sysexes[selected_idx].data = payload;
                let new_hex = Self::format_sysex_hex(&piano.sysexes[selected_idx].data);
                let track_name = piano.track_idx.clone();
                let clip_index = piano.clip_index;
                let new_sysex_events = Self::sysex_to_engine(&piano.sysexes);
                state.piano_sysex_hex_input = new_hex;
                drop(state);
                return self.send(Action::SetMidiSysExEvents {
                    track_name,
                    clip_index,
                    new_sysex_events,
                    old_sysex_events,
                });
            }
            Message::PianoSysExDelete => {
                let mut state = self.state.blocking_write();
                state.piano_sysex_panel_open = false;
                let Some(selected_idx) = state.piano_selected_sysex else {
                    return Task::none();
                };
                let Some(piano) = state.piano.as_mut() else {
                    return Task::none();
                };
                if selected_idx >= piano.sysexes.len() {
                    return Task::none();
                }
                let old_sysex_events = Self::sysex_to_engine(&piano.sysexes);
                piano.sysexes.remove(selected_idx);
                let (new_sel, new_hex) = if piano.sysexes.is_empty() {
                    (None, String::new())
                } else {
                    let idx = selected_idx.min(piano.sysexes.len().saturating_sub(1));
                    (Some(idx), Self::format_sysex_hex(&piano.sysexes[idx].data))
                };
                let track_name = piano.track_idx.clone();
                let clip_index = piano.clip_index;
                let new_sysex_events = Self::sysex_to_engine(&piano.sysexes);
                state.piano_selected_sysex = new_sel;
                state.piano_sysex_hex_input = new_hex;
                drop(state);
                return self.send(Action::SetMidiSysExEvents {
                    track_name,
                    clip_index,
                    new_sysex_events,
                    old_sysex_events,
                });
            }
            Message::PianoSysExMove { index, sample } => {
                let mut state = self.state.blocking_write();
                let Some(piano) = state.piano.as_mut() else {
                    return Task::none();
                };
                if index >= piano.sysexes.len() {
                    return Task::none();
                }
                let old_sysex_events = Self::sysex_to_engine(&piano.sysexes);
                let moved_data = piano.sysexes[index].data.clone();
                let new_sample = sample.min(piano.clip_length_samples.saturating_sub(1));
                piano.sysexes[index].sample = new_sample;
                piano.sysexes.sort_by_key(|s| s.sample);
                let new_sel = piano.sysexes.iter().position(|s| s.data == moved_data);
                let new_hex = new_sel
                    .and_then(|sel| piano.sysexes.get(sel))
                    .map(|ev| Self::format_sysex_hex(&ev.data))
                    .unwrap_or_default();
                let track_name = piano.track_idx.clone();
                let clip_index = piano.clip_index;
                let new_sysex_events = Self::sysex_to_engine(&piano.sysexes);
                state.piano_selected_sysex = new_sel;
                state.piano_sysex_hex_input = new_hex;
                drop(state);
                return self.send(Action::SetMidiSysExEvents {
                    track_name,
                    clip_index,
                    new_sysex_events,
                    old_sysex_events,
                });
            }
            Message::PianoSelectRectStart { position } => {
                let mut state = self.state.blocking_write();
                if !state.shift {
                    state.piano_selected_notes.clear();
                }
                state.piano_selecting_rect = Some((position, position));
            }
            Message::PianoSelectRectDrag { position } => {
                let mut state = self.state.blocking_write();
                if let Some((start, _)) = state.piano_selecting_rect {
                    state.piano_selecting_rect = Some((start, position));

                    let (zoom_x, zoom_y) = if state.piano.is_some() {
                        (state.piano_zoom_x, state.piano_zoom_y)
                    } else {
                        return Task::none();
                    };

                    let tempo = state.tempo.max(1.0) as f64;
                    let tsig_num = state.time_signature_num.max(1) as f64;
                    let tsig_denom = state.time_signature_denom.max(1) as f64;
                    let row_h = ((14.0 * 7.0 / 12.0) * zoom_y).max(1.0);
                    let tracks_width = match state.tracks_width {
                        Length::Fixed(v) => v,
                        _ => 200.0,
                    };
                    let editor_width = (self.size.width - tracks_width - 3.0).max(1.0);
                    let samples_per_beat =
                        (self.playback_rate_hz * 60.0 / tempo) * (4.0 / tsig_denom);
                    let samples_per_bar = samples_per_beat * tsig_num;
                    let total_samples = (samples_per_bar * self.zoom_visible_bars as f64).max(1.0);
                    let pps = ((editor_width as f64 / total_samples) as f32 * zoom_x).max(1.0e-6);

                    let min_x = start.x.min(position.x);
                    let max_x = start.x.max(position.x);
                    let min_y = start.y.min(position.y);
                    let max_y = start.y.max(position.y);

                    let mut selected = std::collections::HashSet::new();
                    if let Some(piano) = state.piano.as_ref() {
                        for (idx, note) in piano.notes.iter().enumerate() {
                            if note.pitch > PITCH_MAX {
                                continue;
                            }
                            let y_idx = usize::from(PITCH_MAX - note.pitch);
                            let y = y_idx as f32 * row_h + 1.0;
                            let x = note.start_sample as f32 * pps;
                            let w = (note.length_samples as f32 * pps).max(2.0);
                            let h = (row_h - 2.0).max(2.0);

                            if x + w >= min_x && x <= max_x && y + h >= min_y && y <= max_y {
                                selected.insert(idx);
                            }
                        }
                    }
                    state.piano_selected_notes = selected;
                }
            }
            Message::PianoSelectRectEnd => {
                let mut state = self.state.blocking_write();
                state.piano_selecting_rect = None;
            }
            Message::PianoCreateNoteStart { position } => {
                let mut state = self.state.blocking_write();
                state.piano_selected_notes.clear();
                state.piano_creating_note = Some((position, position));
            }
            Message::PianoCreateNoteDrag { position } => {
                let mut state = self.state.blocking_write();
                if let Some((start, _)) = state.piano_creating_note {
                    state.piano_creating_note = Some((start, position));
                }
            }
            Message::PianoCreateNoteEnd => {
                let mut state = self.state.blocking_write();
                let Some((start, end)) = state.piano_creating_note.take() else {
                    return Task::none();
                };

                let zoom_x = state.piano_zoom_x;
                let zoom_y = state.piano_zoom_y;
                let row_h = ((14.0 * 7.0 / 12.0) * zoom_y).max(1.0);
                let tracks_width = match state.tracks_width {
                    Length::Fixed(v) => v,
                    _ => 200.0,
                };
                let editor_width = (self.size.width - tracks_width - 3.0).max(1.0);
                let tempo = state.tempo.max(1.0) as f64;
                let tsig_num = state.time_signature_num.max(1) as f64;
                let tsig_denom = state.time_signature_denom.max(1) as f64;
                let samples_per_beat = (self.playback_rate_hz * 60.0 / tempo) * (4.0 / tsig_denom);
                let samples_per_bar = samples_per_beat * tsig_num;
                let total_samples = (samples_per_bar * self.zoom_visible_bars as f64).max(1.0);
                let pps = ((editor_width as f64 / total_samples) as f32 * zoom_x).max(1.0e-6);

                let x0 = start.x.min(end.x).max(0.0);
                let x1 = start.x.max(end.x).max(0.0);
                let raw_start = (x0 / pps).floor().max(0.0) as usize;
                let raw_end = (x1 / pps).ceil().max(raw_start as f32 + 1.0) as usize;
                let snap_interval = match self.midi_snap_mode {
                    crate::message::SnapMode::NoSnap => 1.0,
                    crate::message::SnapMode::Clips => 1.0,
                    crate::message::SnapMode::Bar => samples_per_bar.max(1.0),
                    crate::message::SnapMode::Beat => samples_per_beat.max(1.0),
                    crate::message::SnapMode::Eighth => (samples_per_beat / 2.0).max(1.0),
                    crate::message::SnapMode::Sixteenth => (samples_per_beat / 4.0).max(1.0),
                    crate::message::SnapMode::ThirtySecond => (samples_per_beat / 8.0).max(1.0),
                    crate::message::SnapMode::SixtyFourth => (samples_per_beat / 16.0).max(1.0),
                };
                let snap_sample = |sample: f32| -> usize {
                    if matches!(
                        self.midi_snap_mode,
                        crate::message::SnapMode::NoSnap | crate::message::SnapMode::Clips
                    ) {
                        return sample.max(0.0) as usize;
                    }
                    ((sample.max(0.0) as f64 / snap_interval).round() * snap_interval) as usize
                };
                let start_sample = snap_sample(raw_start as f32);
                let mut end_sample = snap_sample(raw_end as f32);
                let min_len = snap_interval.max(1.0) as usize;
                if end_sample <= start_sample {
                    end_sample = start_sample.saturating_add(min_len);
                }
                let length_samples = end_sample.saturating_sub(start_sample).max(min_len);

                let pitch_row = (start.y / row_h).floor();
                let pitch_row = pitch_row.clamp(0.0, f32::from(PITCH_MAX)) as usize;
                let pitch = PITCH_MAX.saturating_sub(pitch_row as u8);

                if let Some(piano) = state.piano.as_ref() {
                    let track_name = piano.track_idx.clone();
                    let clip_idx = piano.clip_index;
                    let insert_idx = piano.notes.len();
                    let note = maolan_engine::message::MidiNoteData {
                        start_sample,
                        length_samples,
                        pitch,
                        velocity: 100,
                        channel: 0,
                    };
                    state.piano_selected_notes.clear();
                    drop(state);
                    return self.send(Action::InsertMidiNotes {
                        track_name,
                        clip_index: clip_idx,
                        notes: vec![(insert_idx, note)],
                    });
                }
            }
            Message::PianoDeleteSelectedNotes => {
                let mut state = self.state.blocking_write();
                let mut selected_indices: Vec<usize> =
                    state.piano_selected_notes.iter().copied().collect();
                selected_indices.sort_unstable();

                if !selected_indices.is_empty()
                    && let Some(piano) = state.piano.as_mut()
                {
                    let track_name = piano.track_idx.clone();
                    let clip_idx = piano.clip_index;
                    let deleted_notes: Vec<(usize, maolan_engine::message::MidiNoteData)> =
                        selected_indices
                            .iter()
                            .filter_map(|&idx| {
                                piano.notes.get(idx).map(|note| {
                                    (
                                        idx,
                                        maolan_engine::message::MidiNoteData {
                                            start_sample: note.start_sample,
                                            length_samples: note.length_samples,
                                            pitch: note.pitch,
                                            velocity: note.velocity,
                                            channel: note.channel,
                                        },
                                    )
                                })
                            })
                            .collect();

                    let note_indices: Vec<usize> = selected_indices.iter().rev().copied().collect();

                    state.piano_selected_notes.clear();
                    drop(state);
                    return self.send(Action::DeleteMidiNotes {
                        track_name,
                        clip_index: clip_idx,
                        note_indices,
                        deleted_notes,
                    });
                }
            }
            Message::PianoDeleteNotes { ref note_indices } => {
                let mut state = self.state.blocking_write();
                let mut selected_indices = note_indices.clone();
                selected_indices.sort_unstable();
                selected_indices.dedup();

                if !selected_indices.is_empty()
                    && let Some(piano) = state.piano.as_mut()
                {
                    let track_name = piano.track_idx.clone();
                    let clip_idx = piano.clip_index;
                    let deleted_notes: Vec<(usize, maolan_engine::message::MidiNoteData)> =
                        selected_indices
                            .iter()
                            .filter_map(|&idx| {
                                piano.notes.get(idx).map(|note| {
                                    (
                                        idx,
                                        maolan_engine::message::MidiNoteData {
                                            start_sample: note.start_sample,
                                            length_samples: note.length_samples,
                                            pitch: note.pitch,
                                            velocity: note.velocity,
                                            channel: note.channel,
                                        },
                                    )
                                })
                            })
                            .collect();

                    let note_indices: Vec<usize> = selected_indices.iter().rev().copied().collect();
                    state.piano_selected_notes.clear();
                    drop(state);
                    return self.send(Action::DeleteMidiNotes {
                        track_name,
                        clip_index: clip_idx,
                        note_indices,
                        deleted_notes,
                    });
                }
            }
            Message::DrumNoteSelected(note_index) => {
                let mut state = self.state.blocking_write();
                if note_index == usize::MAX {
                    state.piano_selected_notes.clear();
                } else if state.shift {
                    if state.piano_selected_notes.contains(&note_index) {
                        state.piano_selected_notes.remove(&note_index);
                    } else {
                        state.piano_selected_notes.insert(note_index);
                    }
                } else {
                    state.piano_selected_notes.clear();
                    state.piano_selected_notes.insert(note_index);
                }
            }
            Message::DrumNoteCreate {
                start_sample,
                pitch,
            } => {
                let state = self.state.blocking_read();
                if let Some(piano) = state.piano.as_ref() {
                    let track_name = piano.track_idx.clone();
                    let clip_idx = piano.clip_index;
                    let tempo = state.tempo.max(1.0) as f64;
                    let tsig_num = state.time_signature_num.max(1) as f64;
                    let tsig_denom = state.time_signature_denom.max(1) as f64;
                    let samples_per_beat =
                        (self.playback_rate_hz * 60.0 / tempo) * (4.0 / tsig_denom);
                    let samples_per_bar = samples_per_beat * tsig_num;
                    let snapped_start = self.midi_snap_mode.snap_sample(
                        start_sample as f64,
                        samples_per_beat,
                        samples_per_bar,
                    ) as usize;
                    let length_samples = (samples_per_beat / 4.0).max(1.0) as usize;
                    drop(state);
                    return self.send(Action::InsertMidiNotes {
                        track_name,
                        clip_index: clip_idx,
                        notes: vec![(
                            0,
                            maolan_engine::message::MidiNoteData {
                                start_sample: snapped_start,
                                length_samples,
                                pitch,
                                velocity: 100,
                                channel: 0,
                            },
                        )],
                    });
                }
            }
            Message::DrumNoteDelete(note_index) => {
                let mut state = self.state.blocking_write();
                if let Some(piano) = state.piano.as_mut() {
                    let track_name = piano.track_idx.clone();
                    let clip_idx = piano.clip_index;
                    if let Some(note) = piano.notes.get(note_index) {
                        let deleted_note = maolan_engine::message::MidiNoteData {
                            start_sample: note.start_sample,
                            length_samples: note.length_samples,
                            pitch: note.pitch,
                            velocity: note.velocity,
                            channel: note.channel,
                        };
                        state.piano_selected_notes.clear();
                        drop(state);
                        return self.send(Action::DeleteMidiNotes {
                            track_name,
                            clip_index: clip_idx,
                            note_indices: vec![note_index],
                            deleted_notes: vec![(note_index, deleted_note)],
                        });
                    }
                }
            }
            Message::DrumNoteMove {
                note_index,
                delta_samples,
            } => {
                let state = self.state.blocking_read();
                if let Some(piano) = state.piano.as_ref()
                    && let Some(note) = piano.notes.get(note_index)
                {
                    let track_name = piano.track_idx.clone();
                    let clip_idx = piano.clip_index;
                    let old_note = maolan_engine::message::MidiNoteData {
                        start_sample: note.start_sample,
                        length_samples: note.length_samples,
                        pitch: note.pitch,
                        velocity: note.velocity,
                        channel: note.channel,
                    };
                    let tempo = state.tempo.max(1.0) as f64;
                    let tsig_num = state.time_signature_num.max(1) as f64;
                    let tsig_denom = state.time_signature_denom.max(1) as f64;
                    let samples_per_beat =
                        (self.playback_rate_hz * 60.0 / tempo) * (4.0 / tsig_denom);
                    let samples_per_bar = samples_per_beat * tsig_num;
                    let raw_start = if delta_samples < 0 {
                        old_note
                            .start_sample
                            .saturating_sub((-delta_samples) as usize)
                    } else {
                        old_note.start_sample.saturating_add(delta_samples as usize)
                    };
                    let new_start = self.midi_snap_mode.snap_sample_drag(
                        raw_start as f64,
                        delta_samples as f64,
                        samples_per_beat,
                        samples_per_bar,
                    ) as usize;
                    let new_note = maolan_engine::message::MidiNoteData {
                        start_sample: new_start,
                        length_samples: old_note.length_samples,
                        pitch: old_note.pitch,
                        velocity: old_note.velocity,
                        channel: old_note.channel,
                    };
                    drop(state);
                    return self.send(Action::ModifyMidiNotes {
                        track_name,
                        clip_index: clip_idx,
                        note_indices: vec![note_index],
                        new_notes: vec![new_note],
                        old_notes: vec![old_note],
                    });
                }
            }
            Message::DrumSelectRectStart { position } => {
                let mut state = self.state.blocking_write();
                if !state.shift {
                    state.piano_selected_notes.clear();
                }
                state.piano_selecting_rect = Some((position, position));
            }
            Message::DrumSelectRectDrag { position } => {
                let mut state = self.state.blocking_write();
                if let Some((start, _)) = state.piano_selecting_rect {
                    state.piano_selecting_rect = Some((start, position));

                    let (zoom_x, zoom_y) = if state.piano.is_some() {
                        (state.piano_zoom_x, state.piano_zoom_y)
                    } else {
                        return Task::none();
                    };

                    let row_h = (24.0 * zoom_y).max(1.0);
                    let tempo = state.tempo.max(1.0) as f64;
                    let tsig_num = state.time_signature_num.max(1) as f64;
                    let tsig_denom = state.time_signature_denom.max(1) as f64;
                    let samples_per_beat =
                        (self.playback_rate_hz * 60.0 / tempo) * (4.0 / tsig_denom);
                    let samples_per_bar = samples_per_beat * tsig_num;
                    let total_samples = (samples_per_bar * self.zoom_visible_bars as f64).max(1.0);
                    let tracks_width = match state.tracks_width {
                        Length::Fixed(v) => v,
                        _ => 200.0,
                    };
                    let editor_width = (self.size.width - tracks_width - 3.0).max(1.0);
                    let pps = ((editor_width as f64 / total_samples) as f32 * zoom_x).max(1.0e-6);

                    let min_x = start.x.min(position.x);
                    let max_x = start.x.max(position.x);
                    let min_y = start.y.min(position.y);
                    let max_y = start.y.max(position.y);

                    let mut selected = std::collections::HashSet::new();
                    if let Some(piano) = state.piano.as_ref() {
                        let mut drum_rows: Vec<u8> = piano
                            .notes
                            .iter()
                            .map(|n| n.pitch)
                            .collect::<std::collections::HashSet<_>>()
                            .into_iter()
                            .collect();
                        for (pitch, _) in crate::consts::gm_drum_map::GM_DRUM_MAP {
                            if !drum_rows.contains(pitch) {
                                drum_rows.push(*pitch);
                            }
                        }
                        drum_rows.sort();

                        for (idx, note) in piano.notes.iter().enumerate() {
                            let Some(row_idx) = drum_rows.iter().position(|&p| p == note.pitch)
                            else {
                                continue;
                            };
                            let y = row_idx as f32 * row_h + 1.0;
                            let h = (row_h - 2.0).max(2.0);
                            let x = note.start_sample as f32 * pps;
                            let w = (note.length_samples as f32 * pps).max(2.0);

                            if x + w >= min_x && x <= max_x && y + h >= min_y && y <= max_y {
                                selected.insert(idx);
                            }
                        }
                    }
                    state.piano_selected_notes = selected;
                }
            }
            Message::DrumSelectRectEnd => {
                let mut state = self.state.blocking_write();
                state.piano_selecting_rect = None;
            }
            Message::PianoDeleteControllers {
                ref controller_indices,
            } => {
                let mut state = self.state.blocking_write();
                let mut selected_indices = controller_indices.clone();
                selected_indices.sort_unstable();
                selected_indices.dedup();

                if !selected_indices.is_empty()
                    && let Some(piano) = state.piano.as_mut()
                {
                    let track_name = piano.track_idx.clone();
                    let clip_idx = piano.clip_index;
                    let deleted_controllers: Vec<(
                        usize,
                        maolan_engine::message::MidiControllerData,
                    )> = selected_indices
                        .iter()
                        .filter_map(|&idx| {
                            piano.controllers.get(idx).map(|ctrl| {
                                (
                                    idx,
                                    maolan_engine::message::MidiControllerData {
                                        sample: ctrl.sample,
                                        controller: ctrl.controller,
                                        value: ctrl.value,
                                        channel: ctrl.channel,
                                    },
                                )
                            })
                        })
                        .collect();
                    let controller_indices: Vec<usize> =
                        selected_indices.iter().rev().copied().collect();
                    drop(state);
                    return self.send(Action::DeleteMidiControllers {
                        track_name,
                        clip_index: clip_idx,
                        controller_indices,
                        deleted_controllers,
                    });
                }
            }
            Message::PianoQuantizeSelectedNotes => {
                let interval = self.snap_interval_samples().max(1);
                return self.selected_piano_notes_edit(move |_idx, note| {
                    let snapped_start =
                        ((note.start_sample.saturating_add(interval / 2)) / interval) * interval;
                    let end_sample = note.start_sample.saturating_add(note.length_samples);
                    let mut snapped_end =
                        ((end_sample.saturating_add(interval / 2)) / interval) * interval;
                    let mut out = note.clone();
                    if snapped_end <= snapped_start {
                        snapped_end = snapped_start.saturating_add(interval);
                    }
                    out.start_sample = snapped_start;
                    out.length_samples = snapped_end.saturating_sub(snapped_start).max(1);
                    out
                });
            }
            Message::PianoScaleSelectedNotes => {
                let (root, minor) = {
                    let state = self.state.blocking_read();
                    (state.piano_scale_root.semitone(), state.piano_scale_minor)
                };
                return self.selected_piano_notes_edit(move |_idx, note| {
                    let mut out = note.clone();
                    out.pitch = Self::nearest_scale_pitch(note.pitch, root, minor);
                    out
                });
            }
            Message::PianoChordSelectedNotes => {
                let chord_kind = self.state.blocking_read().piano_chord_kind;
                let state = self.state.blocking_write();
                let selected: Vec<usize> = {
                    let mut v: Vec<usize> = state.piano_selected_notes.iter().copied().collect();
                    v.sort_unstable();
                    v
                };
                if selected.is_empty() {
                    return Task::none();
                }
                let Some(piano) = state.piano.as_ref() else {
                    return Task::none();
                };
                let track_name = piano.track_idx.clone();
                let clip_index = piano.clip_index;
                let mut existing = std::collections::HashSet::<(usize, usize, u8, u8)>::new();
                for note in &piano.notes {
                    existing.insert((
                        note.start_sample,
                        note.length_samples,
                        note.pitch,
                        note.channel,
                    ));
                }
                let mut to_insert: Vec<(usize, maolan_engine::message::MidiNoteData)> = Vec::new();
                let mut next_index = piano.notes.len();
                for idx in selected {
                    let Some(note) = piano.notes.get(idx) else {
                        continue;
                    };
                    for interval in chord_kind.intervals() {
                        let pitch = note.pitch.saturating_add(*interval).min(127);
                        let key = (note.start_sample, note.length_samples, pitch, note.channel);
                        if existing.contains(&key) {
                            continue;
                        }
                        existing.insert(key);
                        to_insert.push((
                            next_index,
                            maolan_engine::message::MidiNoteData {
                                start_sample: note.start_sample,
                                length_samples: note.length_samples,
                                pitch,
                                velocity: note.velocity,
                                channel: note.channel,
                            },
                        ));
                        next_index = next_index.saturating_add(1);
                    }
                }
                drop(state);
                if to_insert.is_empty() {
                    return Task::none();
                }
                return self.send(Action::InsertMidiNotes {
                    track_name,
                    clip_index,
                    notes: to_insert,
                });
            }
            Message::PianoLegatoSelectedNotes => {
                let state = self.state.blocking_read();
                let Some(piano) = state.piano.as_ref() else {
                    return Task::none();
                };
                let mut selected: Vec<usize> = state.piano_selected_notes.iter().copied().collect();
                selected.sort_unstable();
                if selected.is_empty() {
                    return Task::none();
                }
                let mut next_start_by_idx = vec![None; piano.notes.len()];
                for (idx, note) in piano.notes.iter().enumerate() {
                    let next_start = piano
                        .notes
                        .iter()
                        .enumerate()
                        .filter(|(i, n)| {
                            *i != idx
                                && n.channel == note.channel
                                && n.pitch == note.pitch
                                && n.start_sample > note.start_sample
                        })
                        .map(|(_, n)| n.start_sample)
                        .min();
                    next_start_by_idx[idx] = next_start;
                }
                drop(state);
                return self.selected_piano_notes_edit(move |idx, note| {
                    let mut out = note.clone();
                    let next_start = next_start_by_idx.get(idx).and_then(|next| *next);
                    if let Some(next) = next_start {
                        out.length_samples = next.saturating_sub(note.start_sample).max(1);
                    }
                    out
                });
            }
            Message::PianoVelocityShapeSelectedNotes => {
                let amount = self
                    .state
                    .blocking_read()
                    .piano_velocity_shape_amount
                    .clamp(0.0, 1.0);
                let state = self.state.blocking_read();
                let Some(piano) = state.piano.as_ref() else {
                    return Task::none();
                };
                let mut selected: Vec<(usize, usize)> = state
                    .piano_selected_notes
                    .iter()
                    .copied()
                    .filter_map(|idx| piano.notes.get(idx).map(|n| (idx, n.start_sample)))
                    .collect();
                selected.sort_unstable_by_key(|(_, start)| *start);
                let rank: std::collections::HashMap<usize, usize> = selected
                    .iter()
                    .enumerate()
                    .map(|(i, (idx, _))| (*idx, i))
                    .collect();
                let total = selected.len().max(1);
                drop(state);
                return self.selected_piano_notes_edit(move |idx, note| {
                    let mut out = note.clone();
                    let pos = *rank.get(&idx).unwrap_or(&0);
                    let t = if total <= 1 {
                        0.5
                    } else {
                        pos as f32 / (total.saturating_sub(1)) as f32
                    };
                    let shaped = (35.0 + t * (120.0 - 35.0)).round().clamp(1.0, 127.0) as u8;
                    let blended = (note.velocity as f32
                        + (shaped as f32 - note.velocity as f32) * amount)
                        .round()
                        .clamp(1.0, 127.0) as u8;
                    out.velocity = blended;
                    out
                });
            }
            Message::PianoHumanizeSelectedNotes => {
                let interval = self.snap_interval_samples().max(1) as i64;
                let (time_amount, vel_amount) = {
                    let state = self.state.blocking_read();
                    (
                        state.piano_humanize_time_amount.clamp(0.0, 1.0),
                        state.piano_humanize_velocity_amount.clamp(0.0, 1.0),
                    )
                };
                let max_time_jitter = (((interval / 8).max(1)) as f32 * time_amount).round() as i64;
                let max_vel_jitter = (6.0_f32 * vel_amount).round() as i64;
                return self.selected_piano_notes_edit(move |idx, note| {
                    let mut out = note.clone();
                    let dt =
                        Self::deterministic_note_jitter(idx, note.start_sample, max_time_jitter);
                    let new_start = (note.start_sample as i64 + dt).max(0) as usize;
                    let dv = Self::deterministic_note_jitter(
                        idx ^ 0xA5A5,
                        note.length_samples,
                        max_vel_jitter,
                    ) as i16;
                    let new_vel = (i16::from(note.velocity) + dv).clamp(1, 127) as u8;
                    out.start_sample = new_start;
                    out.velocity = new_vel;
                    out
                });
            }
            Message::PianoGrooveSelectedNotes => {
                let interval = self.snap_interval_samples().max(1);
                let amount = self
                    .state
                    .blocking_read()
                    .piano_groove_amount
                    .clamp(0.0, 1.0);
                let swing = (((interval as f32) * 0.22) * amount).round().max(0.0) as usize;
                return self.selected_piano_notes_edit(move |_idx, note| {
                    let straight =
                        ((note.start_sample.saturating_add(interval / 2)) / interval) * interval;
                    let grid = straight / interval;
                    let mut out = note.clone();
                    out.start_sample = if grid % 2 == 1 {
                        straight.saturating_add(swing)
                    } else {
                        straight
                    };
                    out
                });
            }
            Message::PianoHumanizeTimeAmountChanged(value) => {
                self.state.blocking_write().piano_humanize_time_amount = value.clamp(0.0, 1.0);
            }
            Message::PianoHumanizeVelocityAmountChanged(value) => {
                self.state.blocking_write().piano_humanize_velocity_amount = value.clamp(0.0, 1.0);
            }
            Message::PianoGrooveAmountChanged(value) => {
                self.state.blocking_write().piano_groove_amount = value.clamp(0.0, 1.0);
            }
            Message::PianoScaleRootSelected(root) => {
                self.state.blocking_write().piano_scale_root = root;
            }
            Message::PianoScaleMinorToggled(minor) => {
                self.state.blocking_write().piano_scale_minor = minor;
            }
            Message::PianoChordKindSelected(kind) => {
                self.state.blocking_write().piano_chord_kind = kind;
            }
            Message::PianoVelocityShapeAmountChanged(value) => {
                self.state.blocking_write().piano_velocity_shape_amount = value.clamp(0.0, 1.0);
            }
            Message::TracksResizeHover(hovered) => {
                self.tracks_resize_hovered = hovered;
            }
            Message::LiveViewTracksResizeHover(hovered) => {
                self.live_view_tracks_resize_hovered = hovered;
            }
            Message::LiveViewLeftSplitResizeHover(hovered) => {
                self.live_view_left_split_resize_hovered = hovered;
            }
            Message::LiveViewLeftTabSelect(ref tab) => {
                self.state.blocking_write().live_view_left_tab = tab.clone();
            }
            Message::MixerResizeHover(hovered) => {
                self.mixer_resize_hovered = hovered;
            }
            Message::TransportRecordToggle => {
                self.toolbar.update(&message);
                if self.record_armed {
                    self.record_armed = false;
                    self.pending_record_after_save = false;
                    self.session_slot_record_target = None;
                    self.stop_recording_preview();
                    return self.send(Action::SetRecordEnabled(false));
                }
                if self.session_dir.is_none() {
                    self.pending_record_after_save = true;
                    return Task::perform(
                        async {
                            AsyncFileDialog::new()
                                .set_title("Select folder to save session")
                                .set_directory("/tmp")
                                .pick_folder()
                                .await
                                .map(|handle| handle.path().to_path_buf())
                        },
                        Message::RecordFolderSelected,
                    );
                }
                self.record_armed = true;
                if self.playing {
                    self.start_recording_preview();
                }
                return self.send(Action::SetRecordEnabled(true));
            }
            Message::Response(Ok(ref a)) => {
                match a {
                    Action::HistoryState { dirty } => {
                        self.engine_dirty = *dirty;
                    }
                    Action::Log { source, message } => {
                        self.info(format!("[{source}] {message}"));
                    }
                    _ if !self.session_restore_in_progress && history::should_record(a) => {
                        self.engine_dirty = true;
                    }
                    _ => {}
                }
                let mut refresh_midi_clip_previews = false;
                if let Some(task) = self.handle_response_freeze_meter_action(a) {
                    return task;
                }
                if let Some(task) = self.handle_response_session_state_action(a) {
                    return task;
                }
                if let Action::StepRecordMidiNote {
                    channel,
                    pitch,
                    velocity,
                    ..
                } = a
                {
                    return self.handle_step_record_note(*channel, *pitch, *velocity);
                }
                if let Action::SessionMidiLearnTriggered { target } = a {
                    match target {
                        maolan_engine::message::SessionMidiLearnTarget::Slot {
                            track_name,
                            scene_index,
                        } => {
                            self.toggle_session_slot(track_name, *scene_index);
                        }
                        maolan_engine::message::SessionMidiLearnTarget::Scene(scene_index) => {
                            let _task = self.launch_session_scene(*scene_index, false);
                        }
                        maolan_engine::message::SessionMidiLearnTarget::StopTrack(track_name) => {
                            self.stop_session_track(track_name);
                        }
                        maolan_engine::message::SessionMidiLearnTarget::StopAll => {
                            self.stop_all_session_clips();
                        }
                    }
                    return Task::none();
                }
                match a {
                    Action::TrackClapFileReferences {
                        track_name,
                        instance_id,
                        refs,
                    } => {
                        let plugin_ref = crate::gui::PluginInstanceRef::Track {
                            track_name: track_name.clone(),
                            instance_id: *instance_id,
                        };
                        self.handle_clap_file_references_response(&plugin_ref, refs);
                    }
                    Action::ClipClapFileReferences {
                        track_name,
                        clip_idx,
                        instance_id,
                        refs,
                    } => {
                        let plugin_ref = crate::gui::PluginInstanceRef::Clip {
                            track_name: track_name.clone(),
                            clip_idx: *clip_idx,
                            instance_id: *instance_id,
                        };
                        self.handle_clap_file_references_response(&plugin_ref, refs);
                    }
                    Action::TrackClapStateDirty {
                        track_name,
                        instance_id,
                    }
                    | Action::ClipClapStateDirty {
                        track_name,
                        clip_idx: _,
                        instance_id,
                    } => {
                        tracing::info!(%track_name, instance_id, "DAW received CLAP state dirty");
                        self.engine_dirty = true;
                    }
                    _ => {}
                }
                let handled_response_state = self.handle_response_engine_state_action(a);
                let handled_response_track = self.handle_response_track_action(a);
                let handled_response_timing = self.handle_response_timing_state_action(a);
                if matches!(a, Action::EndSessionRestore) {
                    let sync_actions = self.cleanup_session_slot_references();
                    for action in sync_actions {
                        let _ = CLIENT.sender.try_send(EngineMessage::Request(action));
                    }
                    let open_track = {
                        let state = self.state.blocking_read();
                        state
                            .plugin_graph_clip
                            .is_none()
                            .then(|| state.plugin_graph_track.clone())
                            .flatten()
                    };
                    if let Some(track_name) = open_track {
                        return self.open_track_plugins_followup(track_name);
                    }
                }
                if handled_response_track {
                    match a {
                        Action::TrackAddAudioInput(track_name) => {
                            let mut state = self.state.blocking_write();
                            if let Some(track) =
                                state.tracks.iter_mut().find(|t| t.name == *track_name)
                            {
                                track.audio.ins += 1;
                            }
                            drop(state);
                            if let Some(task) =
                                self.maybe_refresh_plugin_graph_for_track(track_name)
                            {
                                return task;
                            }
                        }
                        Action::TrackAddAudioOutput(track_name) => {
                            let mut state = self.state.blocking_write();
                            if let Some(track) =
                                state.tracks.iter_mut().find(|t| t.name == *track_name)
                            {
                                track.audio.outs += 1;
                            }
                            drop(state);
                            if let Some(task) =
                                self.maybe_refresh_plugin_graph_for_track(track_name)
                            {
                                return task;
                            }
                        }
                        Action::TrackRemoveAudioInput(track_name) => {
                            let mut state = self.state.blocking_write();
                            if let Some(track) =
                                state.tracks.iter_mut().find(|t| t.name == *track_name)
                            {
                                track.audio.ins = track.audio.ins.saturating_sub(1);
                            }
                            drop(state);
                            if let Some(task) =
                                self.maybe_refresh_plugin_graph_for_track(track_name)
                            {
                                return task;
                            }
                        }
                        Action::TrackRemoveAudioOutput(track_name) => {
                            let mut state = self.state.blocking_write();
                            if let Some(track) =
                                state.tracks.iter_mut().find(|t| t.name == *track_name)
                            {
                                track.audio.outs = track.audio.outs.saturating_sub(1);
                            }
                            drop(state);
                            if let Some(task) =
                                self.maybe_refresh_plugin_graph_for_track(track_name)
                            {
                                return task;
                            }
                        }
                        _ => {}
                    }
                }
                if !handled_response_state && !handled_response_track && !handled_response_timing {
                    match a {
                        Action::Quit => {
                            exit(0);
                        }
                        Action::AddTrack {
                            name,
                            audio_ins,
                            audio_outs,
                            midi_ins,
                            midi_outs,
                            folder,
                        } => {
                            let mut state = self.state.blocking_write();
                            let mut track = Track::new(
                                name.clone(),
                                0.0,
                                *audio_ins,
                                *audio_outs,
                                *midi_ins,
                                *midi_outs,
                            );
                            track.is_folder = *folder;
                            if let Some((lanes, mode)) = state.pending_track_automation.remove(name)
                            {
                                track.automation_lanes = lanes;
                                track.automation_mode = mode;
                            }

                            if let Some(pos) = state.tracks.iter().position(|t| t.name == *name) {
                                let existing_height = state.tracks[pos].height;
                                let min_h = track.min_height_for_layout();
                                track.height = existing_height.max(min_h);
                                state.tracks[pos] = track;
                            } else if let Some(index) = state.undo_track_indices.remove(name) {
                                let insert_index = index.min(state.tracks.len());
                                state.tracks.insert(insert_index, track);
                            } else {
                                state.tracks.push(track);
                            }

                            if let Some(position) = state.pending_track_positions.remove(name)
                                && let Some(track) =
                                    state.tracks.iter_mut().find(|t| &t.name == name)
                            {
                                track.position = position;
                            }
                            if let Some(height) = state.pending_track_heights.remove(name)
                                && let Some(track) =
                                    state.tracks.iter_mut().find(|t| &t.name == name)
                            {
                                let min_h = track.min_height_for_layout();
                                track.height = height.max(min_h);
                            }
                            if let Some((audio_backup, midi_backup, render_clip)) =
                                self.pending_track_freeze_restore.remove(name)
                                && let Some(track) =
                                    state.tracks.iter_mut().find(|t| &t.name == name)
                            {
                                track.frozen_audio_backup = audio_backup;
                                track.frozen_midi_backup = midi_backup;
                                track.frozen_render_clip = render_clip;
                            }
                            if let Some(mode) =
                                self.pending_track_midi_editor_view_mode.remove(name)
                                && let Some(track) =
                                    state.tracks.iter_mut().find(|t| &t.name == name)
                            {
                                track.midi.editor_view_mode = mode;
                            }
                            if let Some((is_folder, folder_open, parent_track)) =
                                state.pending_track_folder_state.remove(name)
                                && let Some(track) =
                                    state.tracks.iter_mut().find(|t| &t.name == name)
                            {
                                track.is_folder = is_folder;
                                track.folder_open = folder_open;
                                track.parent_track = parent_track;
                            }

                            let pending_template = state
                                .pending_track_template_loads
                                .iter()
                                .position(|(track_name, _)| track_name == name)
                                .map(|index| state.pending_track_template_loads[index].clone());
                            drop(state);

                            if let Some((template_track_name, template_name)) = pending_template
                                && template_track_name == *name
                            {
                                let mut state = self.state.blocking_write();
                                if let Some(index) = state
                                    .pending_track_template_loads
                                    .iter()
                                    .position(|(track_name, _)| track_name == name)
                                {
                                    state.pending_track_template_loads.remove(index);
                                }
                                drop(state);
                                return self.load_track_template(name.clone(), template_name);
                            }

                            let folder_load = {
                                let mut state = self.state.blocking_write();
                                let mut found = None;
                                for load in &mut state.pending_folder_template_loads {
                                    if load.remaining.remove(name) {
                                        found = Some(load.clone());
                                        break;
                                    }
                                }
                                found
                            };

                            if let Some(load) = folder_load
                                && load.remaining.is_empty()
                            {
                                let mut state = self.state.blocking_write();
                                state
                                    .pending_folder_template_loads
                                    .retain(|l| l.target_name != load.target_name);
                                drop(state);
                                return self.complete_folder_template_load(&load);
                            }

                            if !matches!(self.modal, Some(Show::AutosaveRecovery)) {
                                self.modal = None;
                            }
                        }
                        Action::RemoveTrack(name) => {
                            let mut undo_peaks = Vec::new();
                            let mut undo_source_lengths = Vec::new();
                            let mut state = self.state.blocking_write();

                            if let Some(removed_idx) =
                                state.tracks.iter().position(|t| t.name == *name)
                            {
                                if let Some(track) = state.tracks.get(removed_idx) {
                                    for clip in &track.audio.clips {
                                        let key = Self::audio_clip_key(
                                            name,
                                            &clip.name,
                                            clip.start,
                                            clip.length,
                                            clip.offset,
                                        );
                                        if !clip.peaks.is_empty() {
                                            undo_peaks.push((key.clone(), clip.peaks.clone()));
                                        }
                                        if clip.source_length_samples > 0 {
                                            undo_source_lengths
                                                .push((key, clip.source_length_samples));
                                        }
                                    }
                                }
                                state.connections.retain(|conn| {
                                    conn.from_track != *name && conn.to_track != *name
                                });
                                state.undo_track_indices.insert(name.clone(), removed_idx);
                                state.tracks.remove(removed_idx);

                                state.selected.remove(name);
                                if let ConnectionViewSelection::Tracks(set) =
                                    &mut state.connection_view_selection
                                {
                                    set.remove(name);
                                }
                                state.clap_plugins_by_track.remove(name);
                                state.clap_states_by_track.remove(name);
                                state.vst3_states_by_track.remove(name);
                                for track in &mut state.tracks {
                                    if track.parent_track.as_deref() == Some(name.as_str()) {
                                        track.parent_track = None;
                                    }
                                }
                            }
                            drop(state);
                            for (key, peaks) in undo_peaks {
                                self.undo_peaks_cache.insert(key, peaks);
                            }
                            for (key, source_len) in undo_source_lengths {
                                self.undo_source_lengths_cache.insert(key, source_len);
                            }
                        }
                        Action::ClipMove {
                            kind,
                            from,
                            to,
                            copy,
                        } => {
                            let mut state = self.state.blocking_write();

                            let from_track_idx_option: Option<usize> = state
                                .tracks
                                .iter()
                                .position(|track| track.name == from.track_name);

                            if let Some(f_idx) = from_track_idx_option {
                                let from_track = &mut state.tracks[f_idx];

                                let mut clip_to_move: Option<crate::state::AudioClip> = None;
                                let mut midi_clip_to_move: Option<crate::state::MIDIClip> = None;

                                match kind {
                                    Kind::Audio => {
                                        if from.clip_index < from_track.audio.clips.len() {
                                            if !copy {
                                                clip_to_move = Some(
                                                    from_track.audio.clips.remove(from.clip_index),
                                                );
                                            } else {
                                                clip_to_move = Some(
                                                    from_track.audio.clips[from.clip_index].clone(),
                                                );
                                            }
                                        }
                                    }
                                    Kind::MIDI => {
                                        if from.clip_index < from_track.midi.clips.len() {
                                            if !copy {
                                                midi_clip_to_move = Some(
                                                    from_track.midi.clips.remove(from.clip_index),
                                                );
                                            } else {
                                                midi_clip_to_move = Some(
                                                    from_track.midi.clips[from.clip_index].clone(),
                                                );
                                            }
                                        }
                                    }
                                }

                                if let Some(to_track) = state
                                    .tracks
                                    .iter_mut()
                                    .find(|track| track.name == to.track_name)
                                {
                                    if let Some(mut clip_data) = clip_to_move {
                                        clip_data.start = to.sample_offset;
                                        clip_data.input_channel = to.input_channel;
                                        to_track.audio.clips.push(clip_data);
                                    } else if let Some(mut midi_clip_data) = midi_clip_to_move {
                                        midi_clip_data.start = to.sample_offset;
                                        midi_clip_data.input_channel = to.input_channel;
                                        to_track.midi.clips.push(midi_clip_data);
                                    }
                                }
                            }
                            if *kind == Kind::MIDI {
                                refresh_midi_clip_previews = true;
                            }
                        }
                        Action::AddClip {
                            clip_id,
                            name,
                            track_name,
                            start,
                            length,
                            offset,
                            input_channel,
                            muted,
                            peaks_file,
                            kind,
                            fade_enabled,
                            fade_in_samples,
                            fade_out_samples,
                            source_name,
                            source_offset,
                            source_length,
                            preview_name,
                            pitch_correction_points,
                            pitch_correction_frame_likeness,
                            pitch_correction_inertia_ms,
                            pitch_correction_formant_compensation,
                            plugin_graph_json,
                        } => {
                            if self.recording_preview_start_sample.is_some() {
                                self.stop_recording_preview();
                            }
                            let key =
                                Self::audio_clip_key(track_name, name, *start, *length, *offset);
                            let mut max_length_samples = offset.saturating_add(*length);
                            let mut source_length_samples = self
                                .pending_source_lengths
                                .remove(&key)
                                .or_else(|| self.undo_source_lengths_cache.remove(&key))
                                .unwrap_or(0);
                            let mut wav_path_for_rebuild: Option<std::path::PathBuf> = None;
                            let mut peaks_path_for_load: Option<std::path::PathBuf> = None;
                            let precomputed_peaks = self
                                .pending_precomputed_peaks
                                .remove(&key)
                                .or_else(|| self.undo_peaks_cache.remove(&key));
                            let loaded_bins = 0usize;
                            if *kind == Kind::Audio {
                                peaks_path_for_load = peaks_file.as_ref().and_then(|rel| {
                                    self.session_dir
                                        .as_ref()
                                        .map(|session_root| session_root.join(rel))
                                        .filter(|path| path.exists() && path.is_file())
                                });
                                if peaks_path_for_load.is_none() {
                                    peaks_path_for_load = self.pending_peak_file_loads.remove(&key);
                                }
                                if name.to_ascii_lowercase().ends_with(".wav") {
                                    let wav_path = if std::path::Path::new(name).is_absolute() {
                                        Some(std::path::PathBuf::from(name))
                                    } else {
                                        self.session_dir
                                            .as_ref()
                                            .map(|session_root| session_root.join(name))
                                    };
                                    if let Some(wav_path) = wav_path {
                                        if wav_path.exists()
                                            && let Ok(total_samples) =
                                                Self::audio_clip_source_length(&wav_path)
                                        {
                                            max_length_samples =
                                                total_samples.saturating_sub(*offset).max(1);
                                            source_length_samples = total_samples;
                                        }
                                        wav_path_for_rebuild = Some(wav_path);
                                    }
                                }
                            }
                            let mut state = self.state.blocking_write();
                            if let Some(track) =
                                state.tracks.iter_mut().find(|t| &t.name == track_name)
                            {
                                match kind {
                                    Kind::Audio => {
                                        track.audio.clips.push(crate::state::AudioClip {
                                            id: clip_id.clone(),
                                            name: name.clone(),
                                            start: *start,
                                            length: *length,
                                            offset: *offset,
                                            input_channel: *input_channel,
                                            muted: *muted,
                                            max_length_samples,
                                            source_length_samples,
                                            peaks_file: peaks_file.clone(),
                                            peaks: precomputed_peaks.clone().unwrap_or_default(),
                                            fade_enabled: *fade_enabled,
                                            fade_in_samples: *fade_in_samples,
                                            fade_out_samples: *fade_out_samples,
                                            pitch_correction_preview_name: preview_name.clone(),
                                            pitch_correction_source_name: source_name.clone(),
                                            pitch_correction_source_offset: *source_offset,
                                            pitch_correction_source_length: *source_length,
                                            pitch_correction_points: pitch_correction_points
                                                .iter()
                                                .map(|point| crate::state::PitchCorrectionPoint {
                                                    start_sample: point.start_sample,
                                                    length_samples: point.length_samples,
                                                    detected_midi_pitch: point.detected_midi_pitch,
                                                    target_midi_pitch: point.target_midi_pitch,
                                                    clarity: point.clarity,
                                                })
                                                .collect(),
                                            pitch_correction_frame_likeness:
                                                *pitch_correction_frame_likeness,
                                            pitch_correction_inertia_ms:
                                                *pitch_correction_inertia_ms,
                                            pitch_correction_formant_compensation:
                                                *pitch_correction_formant_compensation,
                                            take_lane_override: None,
                                            take_lane_pinned: false,
                                            take_lane_locked: false,
                                            plugin_graph_json: plugin_graph_json.clone(),
                                            grouped_clips: vec![],
                                        });
                                    }
                                    Kind::MIDI => {
                                        track.midi.clips.push(crate::state::MIDIClip {
                                            id: clip_id.clone(),
                                            name: name.clone(),
                                            start: *start,
                                            length: *length,
                                            offset: *offset,
                                            input_channel: *input_channel,
                                            muted: *muted,
                                            max_length_samples,
                                            take_lane_override: None,
                                            take_lane_pinned: false,
                                            take_lane_locked: false,
                                            grouped_clips: vec![],
                                        });
                                    }
                                }
                            }
                            let session_record_target = self.session_slot_record_target.clone();
                            if let Some((target_track, target_scene)) = session_record_target
                                && target_track == *track_name
                            {
                                if let Some(slot) =
                                    state.session.slot_mut(&target_track, target_scene)
                                {
                                    slot.clip = Some(crate::state::SlotClipRef {
                                        clip_id: clip_id.clone(),
                                        launch_mode: crate::state::LaunchMode::Toggle,
                                        launch_quantization: crate::state::LaunchQuantization::Bar,
                                        loop_enabled: true,
                                        loop_start_samples: 0,
                                        loop_end_samples: 0,
                                    });
                                }
                                drop(state);
                                let _ = CLIENT.sender.try_send(EngineMessage::Request(
                                    Action::TrackSetSessionSlot {
                                        track_name: target_track,
                                        scene_index: target_scene,
                                        clip_id: Some(clip_id.clone()),
                                    },
                                ));
                            } else {
                                drop(state);
                            }
                            if *kind == Kind::Audio
                                && precomputed_peaks.is_none()
                                && loaded_bins < 32_768
                            {
                                if let Some(peaks_path) = peaks_path_for_load
                                    && let Some(task) = self.schedule_audio_peak_file_load(
                                        track_name, name, *start, *length, *offset, peaks_path,
                                    )
                                {
                                    self.update_children(&message);
                                    return task;
                                }
                                if let Some(wav_path) = wav_path_for_rebuild
                                    && let Some(task) = self.schedule_audio_peak_rebuild(
                                        track_name, name, *start, *length, *offset, wav_path,
                                    )
                                {
                                    self.update_children(&message);
                                    return task;
                                }
                            }
                            if *kind == Kind::MIDI {
                                refresh_midi_clip_previews = true;
                            }
                        }
                        Action::AddGroupedClip {
                            track_name,
                            kind,
                            audio_clip,
                            midi_clip,
                        } => {
                            let mut state = self.state.blocking_write();
                            if let Some(track) =
                                state.tracks.iter_mut().find(|t| &t.name == track_name)
                            {
                                match kind {
                                    Kind::Audio => {
                                        if let Some(clip) = audio_clip {
                                            let key = Self::audio_clip_key(
                                                track_name,
                                                &clip.name,
                                                clip.start,
                                                clip.length,
                                                clip.offset,
                                            );
                                            let mut max_length_samples =
                                                clip.offset.saturating_add(clip.length).max(1);
                                            let mut source_length_samples = self
                                                .pending_source_lengths
                                                .remove(&key)
                                                .or_else(|| {
                                                    self.undo_source_lengths_cache.remove(&key)
                                                })
                                                .unwrap_or(0);
                                            if source_length_samples > 0 {
                                                max_length_samples = source_length_samples
                                                    .saturating_sub(clip.offset)
                                                    .max(1);
                                            }
                                            if clip.name.to_ascii_lowercase().ends_with(".wav") {
                                                let wav_path = if std::path::Path::new(&clip.name)
                                                    .is_absolute()
                                                {
                                                    Some(std::path::PathBuf::from(&clip.name))
                                                } else {
                                                    self.session_dir.as_ref().map(|session_root| {
                                                        session_root.join(&clip.name)
                                                    })
                                                };
                                                if let Some(wav_path) = wav_path
                                                    && wav_path.exists()
                                                    && let Ok(total_samples) =
                                                        Self::audio_clip_source_length(&wav_path)
                                                {
                                                    max_length_samples = total_samples
                                                        .saturating_sub(clip.offset)
                                                        .max(1);
                                                    source_length_samples = total_samples;
                                                }
                                            }
                                            track.audio.clips.push(Self::audio_clip_from_data(
                                                clip,
                                                max_length_samples,
                                                source_length_samples,
                                            ));
                                        }
                                    }
                                    Kind::MIDI => {
                                        if let Some(clip) = midi_clip {
                                            let max_length_samples =
                                                clip.offset.saturating_add(clip.length).max(1);
                                            track.midi.clips.push(Self::midi_clip_from_data(
                                                clip,
                                                max_length_samples,
                                            ));
                                            refresh_midi_clip_previews = true;
                                        }
                                    }
                                }
                            }
                        }
                        Action::SetClipMuted {
                            track_name,
                            clip_index,
                            kind,
                            muted,
                        } => {
                            let mut state = self.state.blocking_write();
                            if let Some(track) =
                                state.tracks.iter_mut().find(|t| &t.name == track_name)
                            {
                                match kind {
                                    Kind::Audio => {
                                        if let Some(clip) = track.audio.clips.get_mut(*clip_index) {
                                            clip.muted = *muted;
                                        }
                                    }
                                    Kind::MIDI => {
                                        if let Some(clip) = track.midi.clips.get_mut(*clip_index) {
                                            clip.muted = *muted;
                                        }
                                    }
                                }
                            }
                        }
                        Action::SetClipSourceName {
                            track_name,
                            clip_index,
                            kind,
                            name,
                        } => {
                            let mut state = self.state.blocking_write();
                            if let Some(track) =
                                state.tracks.iter_mut().find(|t| &t.name == track_name)
                            {
                                match kind {
                                    Kind::Audio => {
                                        if let Some(clip) = track.audio.clips.get_mut(*clip_index) {
                                            clip.name = name.clone();
                                        }
                                    }
                                    Kind::MIDI => {
                                        if let Some(clip) = track.midi.clips.get_mut(*clip_index) {
                                            clip.name = name.clone();
                                        }
                                        refresh_midi_clip_previews = true;
                                    }
                                }
                            }
                        }
                        Action::SetClipPitchCorrection {
                            track_name,
                            clip_index,
                            preview_name,
                            source_name,
                            source_offset,
                            source_length,
                            pitch_correction_points,
                            pitch_correction_frame_likeness,
                            pitch_correction_inertia_ms,
                            pitch_correction_formant_compensation,
                        } => {
                            let mut state = self.state.blocking_write();
                            if let Some(track) =
                                state.tracks.iter_mut().find(|t| &t.name == track_name)
                                && let Some(clip) = track.audio.clips.get_mut(*clip_index)
                            {
                                clip.pitch_correction_preview_name = preview_name.clone();
                                clip.pitch_correction_source_name = source_name.clone();
                                clip.pitch_correction_source_offset = *source_offset;
                                clip.pitch_correction_source_length = *source_length;
                                clip.pitch_correction_points = pitch_correction_points
                                    .iter()
                                    .map(|point| crate::state::PitchCorrectionPoint {
                                        start_sample: point.start_sample,
                                        length_samples: point.length_samples,
                                        detected_midi_pitch: point.detected_midi_pitch,
                                        target_midi_pitch: point.target_midi_pitch,
                                        clarity: point.clarity,
                                    })
                                    .collect();
                                clip.pitch_correction_frame_likeness =
                                    *pitch_correction_frame_likeness;
                                clip.pitch_correction_inertia_ms = *pitch_correction_inertia_ms;
                                clip.pitch_correction_formant_compensation =
                                    *pitch_correction_formant_compensation;
                                let peak_key = Self::audio_clip_key(
                                    track_name,
                                    &clip.name,
                                    clip.start,
                                    clip.length,
                                    clip.offset,
                                );
                                if let Some(peaks) =
                                    self.pending_precomputed_peaks.remove(&peak_key)
                                {
                                    clip.peaks = peaks;
                                }
                            }
                        }
                        Action::SetClipBounds {
                            track_name,
                            clip_index,
                            kind,
                            start,
                            length,
                            offset,
                        } => {
                            let mut state = self.state.blocking_write();
                            if let Some(track) =
                                state.tracks.iter_mut().find(|t| &t.name == track_name)
                            {
                                match kind {
                                    Kind::Audio => {
                                        if let Some(clip) = track.audio.clips.get_mut(*clip_index) {
                                            clip.start = *start;
                                            clip.length = (*length).max(1);
                                            clip.offset = *offset;
                                            clip.pitch_correction_preview_name = None;
                                            clip.pitch_correction_source_name = None;
                                            clip.pitch_correction_source_offset = None;
                                            clip.pitch_correction_source_length = None;
                                            clip.pitch_correction_points.clear();
                                            clip.pitch_correction_frame_likeness = None;
                                            clip.pitch_correction_inertia_ms = None;
                                            clip.pitch_correction_formant_compensation = None;
                                        }
                                    }
                                    Kind::MIDI => {
                                        if let Some(clip) = track.midi.clips.get_mut(*clip_index) {
                                            clip.start = *start;
                                            clip.length = (*length).max(1);
                                            clip.offset = *offset;
                                        }
                                        refresh_midi_clip_previews = true;
                                    }
                                }
                            }
                        }
                        Action::SyncClipBounds {
                            track_name,
                            clip_index,
                            kind,
                            start,
                            length,
                            offset,
                        } => {
                            let mut state = self.state.blocking_write();
                            if let Some(track) =
                                state.tracks.iter_mut().find(|t| &t.name == track_name)
                            {
                                match kind {
                                    Kind::Audio => {
                                        if let Some(clip) = track.audio.clips.get_mut(*clip_index) {
                                            clip.start = *start;
                                            clip.length = (*length).max(1);
                                            clip.offset = *offset;
                                            clip.pitch_correction_preview_name = None;
                                            clip.pitch_correction_source_name = None;
                                            clip.pitch_correction_source_offset = None;
                                            clip.pitch_correction_source_length = None;
                                            clip.pitch_correction_points.clear();
                                            clip.pitch_correction_frame_likeness = None;
                                            clip.pitch_correction_inertia_ms = None;
                                            clip.pitch_correction_formant_compensation = None;
                                        }
                                    }
                                    Kind::MIDI => {
                                        if let Some(clip) = track.midi.clips.get_mut(*clip_index) {
                                            clip.start = *start;
                                            clip.length = (*length).max(1);
                                            clip.offset = *offset;
                                        }
                                        refresh_midi_clip_previews = true;
                                    }
                                }
                            }
                        }
                        Action::RemoveClip {
                            track_name,
                            kind,
                            clip_indices,
                        } => {
                            let mut undo_peaks = Vec::new();
                            let mut undo_source_lengths = Vec::new();
                            let mut removed_clip_ids = std::collections::HashSet::new();
                            let mut state = self.state.blocking_write();
                            if let Some(track) =
                                state.tracks.iter_mut().find(|t| &t.name == track_name)
                            {
                                match kind {
                                    Kind::Audio => {
                                        let mut indices = clip_indices.clone();
                                        indices.sort_unstable();
                                        indices.dedup();
                                        for &idx in &indices {
                                            if let Some(clip) = track.audio.clips.get(idx) {
                                                removed_clip_ids.insert(clip.id.clone());
                                                let key = Self::audio_clip_key(
                                                    track_name,
                                                    &clip.name,
                                                    clip.start,
                                                    clip.length,
                                                    clip.offset,
                                                );
                                                if !clip.peaks.is_empty() {
                                                    undo_peaks
                                                        .push((key.clone(), clip.peaks.clone()));
                                                }
                                                if clip.source_length_samples > 0 {
                                                    undo_source_lengths
                                                        .push((key, clip.source_length_samples));
                                                }
                                            }
                                        }
                                        for idx in indices.into_iter().rev() {
                                            if idx < track.audio.clips.len() {
                                                track.audio.clips.remove(idx);
                                            }
                                        }
                                    }
                                    Kind::MIDI => {
                                        let mut indices = clip_indices.clone();
                                        indices.sort_unstable();
                                        indices.dedup();
                                        for &idx in &indices {
                                            if let Some(clip) = track.midi.clips.get(idx) {
                                                removed_clip_ids.insert(clip.id.clone());
                                            }
                                        }
                                        for idx in indices.into_iter().rev() {
                                            if idx < track.midi.clips.len() {
                                                track.midi.clips.remove(idx);
                                            }
                                        }
                                    }
                                }
                            }
                            state.selected_clips.retain(|clip| {
                                if clip.track_idx != *track_name || clip.kind != *kind {
                                    return true;
                                }
                                !clip_indices.contains(&clip.clip_idx)
                            });
                            if *kind == Kind::MIDI {
                                refresh_midi_clip_previews = true;
                            }
                            drop(state);
                            if !removed_clip_ids.is_empty() {
                                let sync_actions =
                                    self.clear_session_slots_for_clip_ids(&removed_clip_ids);
                                for action in sync_actions {
                                    let _ = CLIENT.sender.try_send(EngineMessage::Request(action));
                                }
                            }
                            for (key, peaks) in undo_peaks {
                                self.undo_peaks_cache.insert(key, peaks);
                            }
                            for (key, source_len) in undo_source_lengths {
                                self.undo_source_lengths_cache.insert(key, source_len);
                            }
                        }
                        Action::ModifyMidiNotes {
                            track_name,
                            note_indices,
                            new_notes,
                            ..
                        } => {
                            let mut state = self.state.blocking_write();
                            if let Some(piano) = state.piano.as_mut()
                                && piano.track_idx == *track_name
                            {
                                for (note_idx, new_note) in
                                    note_indices.iter().zip(new_notes.iter())
                                {
                                    if let Some(note) = piano.notes.get_mut(*note_idx) {
                                        note.start_sample = new_note.start_sample;
                                        note.length_samples = new_note.length_samples;
                                        note.pitch = new_note.pitch;
                                        note.velocity = new_note.velocity;
                                        note.channel = new_note.channel;
                                    }
                                }
                            }
                        }
                        Action::ModifyMidiControllers {
                            track_name,
                            controller_indices,
                            new_controllers,
                            ..
                        } => {
                            let mut state = self.state.blocking_write();
                            if let Some(piano) = state.piano.as_mut()
                                && piano.track_idx == *track_name
                            {
                                for (ctrl_idx, new_ctrl) in
                                    controller_indices.iter().zip(new_controllers.iter())
                                {
                                    if let Some(ctrl) = piano.controllers.get_mut(*ctrl_idx) {
                                        ctrl.sample = new_ctrl.sample;
                                        ctrl.controller = new_ctrl.controller;
                                        ctrl.value = new_ctrl.value;
                                        ctrl.channel = new_ctrl.channel;
                                    }
                                }
                            }
                        }
                        Action::DeleteMidiControllers {
                            track_name,
                            controller_indices,
                            ..
                        } => {
                            let mut state = self.state.blocking_write();
                            if let Some(piano) = state.piano.as_mut()
                                && piano.track_idx == *track_name
                            {
                                let mut indices = controller_indices.clone();
                                indices.sort_unstable();
                                indices.dedup();
                                for idx in indices.into_iter().rev() {
                                    if idx < piano.controllers.len() {
                                        piano.controllers.remove(idx);
                                    }
                                }
                            }
                        }
                        Action::InsertMidiControllers {
                            track_name,
                            controllers,
                            ..
                        } => {
                            let mut state = self.state.blocking_write();
                            if let Some(piano) = state.piano.as_mut()
                                && piano.track_idx == *track_name
                            {
                                let mut sorted_indices: Vec<usize> =
                                    (0..controllers.len()).collect();
                                sorted_indices.sort_unstable_by_key(|&i| controllers[i].0);
                                for i in sorted_indices {
                                    let (idx, ctrl) = &controllers[i];
                                    let insert_at = (*idx).min(piano.controllers.len());
                                    piano.controllers.insert(
                                        insert_at,
                                        crate::state::PianoControllerPoint {
                                            sample: ctrl.sample,
                                            controller: ctrl.controller,
                                            value: ctrl.value,
                                            channel: ctrl.channel,
                                        },
                                    );
                                }
                            }
                        }
                        Action::SetMidiSysExEvents {
                            track_name,
                            new_sysex_events,
                            ..
                        } => {
                            let mut state = self.state.blocking_write();
                            let current_sel = state.piano_selected_sysex;
                            if let Some(piano) = state.piano.as_mut()
                                && piano.track_idx == *track_name
                            {
                                piano.sysexes = new_sysex_events
                                    .iter()
                                    .map(|ev| PianoSysExPoint {
                                        sample: ev.sample,
                                        data: ev.data.clone(),
                                    })
                                    .collect();
                                piano.sysexes.sort_by_key(|s| s.sample);
                                let new_sel = match current_sel {
                                    Some(sel) if sel < piano.sysexes.len() => Some(sel),
                                    Some(_) => piano.sysexes.len().checked_sub(1),
                                    None => None,
                                };
                                let new_hex = new_sel
                                    .and_then(|idx| piano.sysexes.get(idx))
                                    .map(|ev| Self::format_sysex_hex(&ev.data))
                                    .unwrap_or_default();
                                state.piano_selected_sysex = new_sel;
                                state.piano_sysex_hex_input = new_hex;
                            }
                        }
                        Action::DeleteMidiNotes {
                            track_name,
                            note_indices,
                            ..
                        } => {
                            let mut state = self.state.blocking_write();
                            if let Some(piano) = state.piano.as_mut()
                                && piano.track_idx == *track_name
                            {
                                let mut indices = note_indices.clone();
                                indices.sort_unstable();
                                indices.dedup();
                                for idx in indices.into_iter().rev() {
                                    if idx < piano.notes.len() {
                                        piano.notes.remove(idx);
                                    }
                                }
                                state.piano_selected_notes.clear();
                            }
                        }
                        Action::InsertMidiNotes {
                            track_name, notes, ..
                        } => {
                            let mut state = self.state.blocking_write();
                            if let Some(piano) = state.piano.as_mut()
                                && piano.track_idx == *track_name
                            {
                                let mut sorted_indices: Vec<usize> = (0..notes.len()).collect();
                                sorted_indices.sort_unstable_by_key(|&i| notes[i].0);
                                for i in sorted_indices {
                                    let (idx, note) = &notes[i];
                                    let insert_at = (*idx).min(piano.notes.len());
                                    piano.notes.insert(
                                        insert_at,
                                        crate::state::PianoNote {
                                            start_sample: note.start_sample,
                                            length_samples: note.length_samples,
                                            pitch: note.pitch,
                                            velocity: note.velocity,
                                            channel: note.channel,
                                        },
                                    );
                                }
                                state.piano_selected_notes.clear();
                            }
                        }
                        Action::TrackLoadClapPlugin {
                            track_name,
                            plugin_id,
                            ..
                        } => {
                            let plugin_name = std::path::Path::new(plugin_id)
                                .file_stem()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_else(|| plugin_id.clone());
                            {
                                let mut state = self.state.blocking_write();
                                let entry = state
                                    .clap_plugins_by_track
                                    .entry(track_name.clone())
                                    .or_default();
                                if !entry
                                    .iter()
                                    .any(|existing| existing.eq_ignore_ascii_case(plugin_id))
                                {
                                    entry.push(plugin_id.clone());
                                }
                            }
                            self.state.blocking_write().message = format!(
                                "Loaded CLAP plugin '{plugin_name}' on track '{track_name}'"
                            );
                            if let Some(task) =
                                self.maybe_refresh_plugin_graph_for_track(track_name)
                            {
                                return task;
                            }
                        }
                        Action::TrackUnloadClapPlugin {
                            track_name,
                            plugin_id,
                        } => {
                            {
                                let mut state = self.state.blocking_write();
                                if let Some(entry) = state.clap_plugins_by_track.get_mut(track_name)
                                    && let Some(pos) = entry.iter().position(|existing| {
                                        existing.eq_ignore_ascii_case(plugin_id)
                                    })
                                {
                                    entry.remove(pos);
                                }
                                if let Some(states) = state.clap_states_by_track.get_mut(track_name)
                                {
                                    states.remove(plugin_id);
                                }
                            }
                            let plugin_name = std::path::Path::new(plugin_id)
                                .file_stem()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_else(|| plugin_id.clone());
                            self.state.blocking_write().message = format!(
                                "Unloaded CLAP plugin '{plugin_name}' from track '{track_name}'"
                            );
                            if let Some(task) =
                                self.maybe_refresh_plugin_graph_for_track(track_name)
                            {
                                return task;
                            }
                        }
                        Action::TrackClapStateSnapshot {
                            track_name,
                            instance_id: _instance_id,
                            plugin_id,
                            state: clap_state,
                            ..
                        } => {
                            let state_len = clap_state.bytes.len();
                            tracing::info!(%track_name, instance_id = *_instance_id, %plugin_id, state_len, "DAW received TrackClapStateSnapshot");
                            let mut state = self.state.blocking_write();
                            state
                                .clap_states_by_track
                                .entry(track_name.clone())
                                .or_default()
                                .insert(plugin_id.clone(), clap_state.clone());
                            #[cfg(all(unix, not(target_os = "macos")))]
                            {
                                let state_json = serde_json::to_value(clap_state)
                                    .unwrap_or(serde_json::Value::Null);
                                if let Some((plugins, _)) =
                                    state.plugin_graphs_by_track.get_mut(track_name)
                                    && let Some(plugin) = plugins
                                        .iter_mut()
                                        .find(|plugin| plugin.instance_id == *_instance_id)
                                {
                                    tracing::info!(%track_name, instance_id = *_instance_id, "DAW updated plugin_graphs_by_track state");
                                    plugin.state = Some(state_json.clone());
                                } else {
                                    tracing::warn!(%track_name, instance_id = *_instance_id, "DAW could not find plugin in plugin_graphs_by_track");
                                }
                                if state.plugin_graph_clip.is_none()
                                    && state.plugin_graph_track.as_deref()
                                        == Some(track_name.as_str())
                                    && let Some(plugin) = state
                                        .plugin_graph_plugins
                                        .iter_mut()
                                        .find(|plugin| plugin.instance_id == *_instance_id)
                                {
                                    plugin.state = Some(state_json);
                                }
                            }
                        }
                        Action::ClipClapStateSnapshot {
                            track_name,
                            clip_idx,
                            instance_id,
                            plugin_id: _,
                            state: clap_state,
                            ..
                        } => {
                            let state_json =
                                serde_json::to_value(clap_state).unwrap_or(serde_json::Value::Null);
                            {
                                let mut state = self.state.blocking_write();
                                if let Some(track) = state
                                    .tracks
                                    .iter_mut()
                                    .find(|track| track.name == *track_name)
                                    && let Some(clip) = track.audio.clips.get_mut(*clip_idx)
                                    && let Some(graph_json) =
                                        Self::plugin_graph_json_with_saved_plugin_state(
                                            clip.plugin_graph_json.as_ref(),
                                            *instance_id,
                                            state_json,
                                        )
                                {
                                    clip.plugin_graph_json = Some(graph_json);
                                }
                            }
                            if self.pending_save_path.is_some() {
                                self.pending_save_clap_clips.remove(&(
                                    track_name.clone(),
                                    *clip_idx,
                                    *instance_id,
                                ));
                                if let Some(task) = self.complete_pending_save(track_name) {
                                    return task;
                                }
                            }
                        }
                        Action::TrackClapParameters {
                            track_name,
                            instance_id,
                            parameters,
                        } => {
                            let pending = self
                                .pending_add_clap_automation_instances
                                .remove(&(track_name.clone(), *instance_id));
                            {
                                let mut state = self.state.blocking_write();
                                let cached = state
                                    .plugin_parameters_by_track
                                    .entry(track_name.clone())
                                    .or_default();
                                cached.insert(
                                    *instance_id,
                                    parameters
                                        .iter()
                                        .map(|p| crate::state::PluginParameterInfo {
                                            param_id: p.id,
                                            name: p.name.clone(),
                                            min: p.min_value,
                                            max: p.max_value,
                                        })
                                        .collect(),
                                );
                                if pending
                                    && let Some(track) =
                                        state.tracks.iter_mut().find(|t| t.name == *track_name)
                                {
                                    for param in parameters {
                                        let target = TrackAutomationTarget::ClapParameter {
                                            instance_id: *instance_id,
                                            param_id: param.id,
                                            min: param.min_value,
                                            max: param.max_value,
                                        };
                                        if let Some(existing) = track
                                            .automation_lanes
                                            .iter_mut()
                                            .find(|lane| lane.target == target)
                                        {
                                            existing.visible = true;
                                        } else {
                                            track.automation_lanes.push(
                                                crate::state::TrackAutomationLane {
                                                    target,
                                                    visible: true,
                                                    points: vec![],
                                                },
                                            );
                                        }
                                    }
                                    track.height =
                                        track.min_height_for_layout().max(TRACK_MIN_HEIGHT);
                                    state.message = format!(
                                        "Added {} CLAP automation lanes on '{}'",
                                        parameters.len(),
                                        track_name
                                    );
                                }
                            }
                            if pending
                                && let Some(action) = self.track_automation_lanes_action(track_name)
                            {
                                let _ = CLIENT.sender.try_send(EngineMessage::Request(action));
                            }
                        }
                        Action::TrackVst3Parameters {
                            track_name,
                            instance_id,
                            parameters,
                        } => {
                            let pending = self
                                .pending_add_vst3_automation_instances
                                .remove(&(track_name.clone(), *instance_id));
                            {
                                let mut state = self.state.blocking_write();
                                let cached = state
                                    .plugin_parameters_by_track
                                    .entry(track_name.clone())
                                    .or_default();
                                cached.insert(
                                    *instance_id,
                                    parameters
                                        .iter()
                                        .map(|p| crate::state::PluginParameterInfo {
                                            param_id: p.id,
                                            name: p.title.clone(),
                                            min: 0.0,
                                            max: 1.0,
                                        })
                                        .collect(),
                                );
                                if pending
                                    && let Some(track) =
                                        state.tracks.iter_mut().find(|t| t.name == *track_name)
                                {
                                    for param in parameters {
                                        let target = TrackAutomationTarget::Vst3Parameter {
                                            instance_id: *instance_id,
                                            param_id: param.id,
                                        };
                                        if let Some(existing) = track
                                            .automation_lanes
                                            .iter_mut()
                                            .find(|lane| lane.target == target)
                                        {
                                            existing.visible = true;
                                        } else {
                                            track.automation_lanes.push(
                                                crate::state::TrackAutomationLane {
                                                    target,
                                                    visible: true,
                                                    points: vec![],
                                                },
                                            );
                                        }
                                    }
                                    track.height =
                                        track.min_height_for_layout().max(TRACK_MIN_HEIGHT);
                                    state.message = format!(
                                        "Added {} VST3 automation lanes on '{}'",
                                        parameters.len(),
                                        track_name
                                    );
                                }
                            }
                            if pending
                                && let Some(action) = self.track_automation_lanes_action(track_name)
                            {
                                let _ = CLIENT.sender.try_send(EngineMessage::Request(action));
                            }
                        }
                        #[cfg(all(unix, not(target_os = "macos")))]
                        Action::TrackLv2PluginControls {
                            track_name,
                            instance_id,
                            controls,
                            instance_access_handle: _,
                        } => {
                            let pending = self
                                .pending_add_lv2_automation_instances
                                .remove(&(track_name.clone(), *instance_id));
                            {
                                let mut state = self.state.blocking_write();
                                let cached = state
                                    .plugin_parameters_by_track
                                    .entry(track_name.clone())
                                    .or_default();
                                cached.insert(
                                    *instance_id,
                                    controls
                                        .iter()
                                        .map(|p| crate::state::PluginParameterInfo {
                                            param_id: p.index,
                                            name: p.name.clone(),
                                            min: p.min as f64,
                                            max: p.max as f64,
                                        })
                                        .collect(),
                                );
                                if pending
                                    && let Some(track) =
                                        state.tracks.iter_mut().find(|t| t.name == *track_name)
                                {
                                    for param in controls {
                                        let target = TrackAutomationTarget::Lv2Parameter {
                                            instance_id: *instance_id,
                                            index: param.index,
                                            min: param.min,
                                            max: param.max,
                                        };
                                        if let Some(existing) = track
                                            .automation_lanes
                                            .iter_mut()
                                            .find(|lane| lane.target == target)
                                        {
                                            existing.visible = true;
                                        } else {
                                            track.automation_lanes.push(
                                                crate::state::TrackAutomationLane {
                                                    target,
                                                    visible: true,
                                                    points: vec![],
                                                },
                                            );
                                        }
                                    }
                                    track.height =
                                        track.min_height_for_layout().max(TRACK_MIN_HEIGHT);
                                    state.message = format!(
                                        "Added {} LV2 automation lanes on '{}'",
                                        controls.len(),
                                        track_name
                                    );
                                }
                            }
                            if pending
                                && let Some(action) = self.track_automation_lanes_action(track_name)
                            {
                                let _ = CLIENT.sender.try_send(EngineMessage::Request(action));
                            }
                        }
                        Action::TrackSnapshotAllClapStates { track_name: _ } => {}
                        Action::TrackSnapshotAllClapStatesDone { track_name }
                            if self.pending_save_path.is_some() =>
                        {
                            self.pending_save_clap_tracks.remove(track_name);
                            if let Some(task) = self.complete_pending_save(track_name) {
                                return task;
                            }
                        }
                        Action::TrackClearDefaultPassthrough { track_name } => {
                            if let Some(task) =
                                self.maybe_refresh_plugin_graph_for_track(track_name)
                            {
                                return task;
                            }
                        }
                        #[cfg(all(unix, not(target_os = "macos")))]
                        Action::TrackLoadLv2Plugin { track_name, .. }
                        | Action::TrackSetLv2PluginState { track_name, .. }
                        | Action::TrackUnloadLv2PluginInstance { track_name, .. }
                        | Action::TrackSetLv2ControlValue { track_name, .. }
                        | Action::TrackLoadVst3Plugin { track_name, .. }
                        | Action::TrackUnloadVst3PluginInstance { track_name, .. }
                        | Action::TrackConnectPluginAudio { track_name, .. }
                        | Action::TrackDisconnectPluginAudio { track_name, .. }
                        | Action::TrackConnectPluginMidi { track_name, .. }
                        | Action::TrackDisconnectPluginMidi { track_name, .. } => {
                            if let Some(task) =
                                self.maybe_refresh_plugin_graph_for_track(track_name)
                            {
                                return task;
                            }
                        }
                        Action::TrackConnectAudio { track_name, .. }
                        | Action::TrackDisconnectAudio { track_name, .. }
                        | Action::TrackConnectMidi { track_name, .. }
                        | Action::TrackDisconnectMidi { track_name, .. } => {
                            if let Some(task) =
                                self.maybe_refresh_plugin_graph_for_track(track_name)
                            {
                                return task;
                            }
                        }
                        Action::TrackVst3StateSnapshot {
                            track_name,
                            instance_id,
                            state,
                        } => {
                            {
                                let mut gui_state = self.state.blocking_write();
                                gui_state
                                    .vst3_states_by_track
                                    .entry(track_name.clone())
                                    .or_default()
                                    .insert(*instance_id, state.clone());
                            }
                            #[cfg(target_os = "macos")]
                            if self.pending_save_path.is_some() {
                                self.pending_save_vst3_states
                                    .remove(&(track_name.clone(), *instance_id));
                                if let Some(task) = self.complete_pending_save(track_name) {
                                    return task;
                                }
                            }
                        }
                        Action::ClipVst3StateSnapshot {
                            track_name,
                            clip_idx,
                            instance_id,
                            state,
                        } => {
                            let state_json =
                                serde_json::to_value(state).unwrap_or(serde_json::Value::Null);
                            let mut gui_state = self.state.blocking_write();
                            if let Some(track) = gui_state
                                .tracks
                                .iter_mut()
                                .find(|track| track.name == *track_name)
                                && let Some(clip) = track.audio.clips.get_mut(*clip_idx)
                                && let Some(graph_json) =
                                    Self::plugin_graph_json_with_saved_plugin_state(
                                        clip.plugin_graph_json.as_ref(),
                                        *instance_id,
                                        state_json,
                                    )
                            {
                                clip.plugin_graph_json = Some(graph_json);
                            }
                        }
                        #[cfg(all(unix, not(target_os = "macos")))]
                        Action::TrackLv2Midnam {
                            track_name,
                            note_names,
                        } => {
                            let mut state = self.state.blocking_write();
                            if let Some(piano) = &mut state.piano
                                && piano.track_idx == *track_name
                            {
                                piano.midnam_note_names = note_names.clone();
                            }
                        }
                        Action::TrackClapNoteNames {
                            track_name,
                            note_names,
                        } => {
                            let mut state = self.state.blocking_write();
                            if let Some(piano) = &mut state.piano
                                && piano.track_idx == *track_name
                            {
                                for (note, name) in note_names.iter() {
                                    piano.midnam_note_names.insert(*note, name.clone());
                                }
                            }
                            self.workspace.set_midi_edit_midnam_note_names(note_names);
                        }
                        #[cfg(all(unix, not(target_os = "macos")))]
                        Action::ClipLv2StateSnapshot {
                            track_name,
                            clip_idx,
                            instance_id,
                            state: lv2_state,
                        } => {
                            let mut state = self.state.blocking_write();
                            if let Some(track) = state
                                .tracks
                                .iter_mut()
                                .find(|track| track.name == *track_name)
                                && let Some(clip) = track.audio.clips.get_mut(*clip_idx)
                                && let Some(graph_json) =
                                    Self::plugin_graph_json_with_saved_plugin_state(
                                        clip.plugin_graph_json.as_ref(),
                                        *instance_id,
                                        Self::lv2_state_to_json(lv2_state),
                                    )
                            {
                                clip.plugin_graph_json = Some(graph_json);
                            }
                            if state.plugin_graph_clip.as_ref().is_some_and(|target| {
                                target.track_name == *track_name && target.clip_idx == *clip_idx
                            }) && let Some(plugin) = state
                                .plugin_graph_plugins
                                .iter_mut()
                                .find(|plugin| plugin.instance_id == *instance_id)
                            {
                                plugin.state = Some(Self::lv2_state_to_json(lv2_state));
                            }
                        }
                        Action::TrackPluginGraph {
                            track_name,
                            plugins,
                            connections,
                            connectable_connections,
                        } => {
                            tracing::info!(
                                %track_name,
                                plugins = plugins.len(),
                                connections = connections.len(),
                                connectable = connectable_connections.len(),
                                restore_in_progress = self.session_restore_in_progress,
                                "received TrackPluginGraph"
                            );
                            let mut state = self.state.blocking_write();
                            let keep_restore_cache = self.session_restore_in_progress
                                && plugins.is_empty()
                                && state
                                    .plugin_graphs_by_track
                                    .get(track_name)
                                    .is_some_and(|(cached_plugins, _)| !cached_plugins.is_empty());
                            if keep_restore_cache {
                                return Task::none();
                            }
                            state
                                .plugin_graphs_by_track
                                .insert(track_name.clone(), (plugins.clone(), connections.clone()));
                            state
                                .connectable_connections_by_track
                                .insert(track_name.clone(), connectable_connections.clone());
                            if state.plugin_graph_clip.is_none()
                                && state.plugin_graph_track.as_deref() == Some(track_name.as_str())
                            {
                                state.plugin_graph_track = Some(track_name.clone());
                                state.plugin_graph_plugins = plugins.clone();
                                state.plugin_graph_connections = connections.clone();
                                state.connectable_connections = connectable_connections.clone();
                                state.plugin_graph_selected_connections.clear();
                                state.plugin_graph_selected_connectable_connections.clear();
                                state
                                    .plugin_graph_selected_plugins
                                    .retain(|id| plugins.iter().any(|p| p.instance_id == *id));
                                let track_positions = state
                                    .plugin_graph_plugin_positions
                                    .entry(track_name.clone())
                                    .or_default();
                                for (idx, plugin) in plugins.iter().enumerate() {
                                    let fallback = Point::new(200.0 + idx as f32 * 180.0, 220.0);
                                    track_positions
                                        .entry(plugin.instance_id)
                                        .or_insert(fallback);
                                }
                            }
                            drop(state);

                            let pending_queries =
                                self.queue_pending_graph_automation_queries(track_name, plugins);
                            if !pending_queries.is_empty() {
                                return Task::batch(pending_queries);
                            }

                            if self.pending_save_path.is_some() {
                                self.pending_save_tracks.remove(track_name);
                                if let Some(task) = self.complete_pending_save(track_name) {
                                    return task;
                                }
                            }
                        }
                        Action::RenameTrack { old_name, new_name } => {
                            let mut state = self.state.blocking_write();

                            if let Some(track) =
                                state.tracks.iter_mut().find(|t| t.name == *old_name)
                            {
                                track.name = new_name.clone();
                            }

                            if state.selected.remove(old_name) {
                                state.selected.insert(new_name.clone());
                            }

                            if let crate::state::ConnectionViewSelection::Tracks(tracks) =
                                &mut state.connection_view_selection
                                && tracks.remove(old_name)
                            {
                                tracks.insert(new_name.clone());
                            }

                            for conn in &mut state.connections {
                                if conn.from_track == *old_name {
                                    conn.from_track = new_name.clone();
                                }
                                if conn.to_track == *old_name {
                                    conn.to_track = new_name.clone();
                                }
                            }

                            for track in &mut state.tracks {
                                if track.parent_track.as_deref() == Some(old_name.as_str()) {
                                    track.parent_track = Some(new_name.clone());
                                }
                            }

                            if state.plugin_graph_track.as_deref() == Some(old_name) {
                                state.plugin_graph_track = Some(new_name.clone());
                            }
                            if let Some(target) = state.plugin_graph_clip.as_mut()
                                && target.track_name == *old_name
                            {
                                target.track_name = new_name.clone();
                            }

                            #[cfg(all(unix, not(target_os = "macos")))]
                            Self::rename_track_map_entry(
                                &mut state.plugin_graphs_by_track,
                                old_name,
                                new_name,
                            );
                            Self::rename_track_map_entry(
                                &mut state.clap_plugins_by_track,
                                old_name,
                                new_name,
                            );
                            Self::rename_track_map_entry(
                                &mut state.clap_states_by_track,
                                old_name,
                                new_name,
                            );
                            Self::rename_track_map_entry(
                                &mut state.vst3_states_by_track,
                                old_name,
                                new_name,
                            );
                            state.message = format!("Renamed track to '{}'", new_name);
                            refresh_midi_clip_previews = true;
                        }
                        Action::TrackSetClapParameter {
                            track_name,
                            instance_id,
                            param_id,
                            value,
                        } => {
                            let key = (track_name.clone(), None, *instance_id, *param_id);
                            self.clap_param_values.insert(key, *value);
                        }
                        _ => {}
                    }
                }
                if refresh_midi_clip_previews {
                    self.update_children(&message);
                    return self.queue_midi_clip_preview_loads();
                }
            }
            Message::Response(Err(ref e)) => {
                if !self.pending_track_freeze_bounce.is_empty() {
                    self.pending_track_freeze_bounce.clear();
                }
                self.freeze_in_progress = false;
                self.freeze_track_name = None;
                self.freeze_cancel_requested = false;
                self.pending_save_path = None;
                self.pending_save_tracks.clear();
                self.pending_save_clap_tracks.clear();
                self.pending_save_clap_clips.clear();
                #[cfg(target_os = "macos")]
                self.pending_save_vst3_states.clear();
                self.pending_save_is_template = false;
                self.error(e.clone());
            }
            Message::TrackToggleFolder { ref track_name } => {
                {
                    let mut state = self.state.blocking_write();
                    if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name) {
                        track.folder_open = !track.folder_open;
                    }
                }
                return self.send(Action::TrackToggleFolder {
                    track_name: track_name.clone(),
                });
            }
            Message::TrackSetFolder {
                ref track_name,
                is_folder,
            } => {
                if is_folder {
                    let is_master = self
                        .state
                        .blocking_read()
                        .tracks
                        .iter()
                        .find(|t| t.name == *track_name)
                        .is_some_and(|t| t.is_master);
                    if is_master {
                        self.state.blocking_write().message = format!(
                            "Track '{}' is the master track and cannot be made a folder",
                            track_name
                        );
                        return Task::none();
                    }
                }
                let mut child_tasks = vec![];
                let mut clear_audio_indices = Vec::new();
                let mut clear_midi_indices = Vec::new();
                {
                    let mut state = self.state.blocking_write();
                    if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name) {
                        let was_folder = track.is_folder;
                        track.is_folder = is_folder;
                        if is_folder {
                            track.folder_open = true;
                            if !was_folder {
                                clear_audio_indices = (0..track.audio.clips.len()).collect();
                                clear_midi_indices = (0..track.midi.clips.len()).collect();
                                track.audio.clips.clear();
                                track.midi.clips.clear();
                                track.frozen_audio_backup.clear();
                                track.frozen_midi_backup.clear();
                                track.frozen_render_clip = None;
                                track.frozen = false;
                            }
                        } else if was_folder {
                            let children: Vec<String> = state
                                .tracks
                                .iter()
                                .filter(|t| t.parent_track.as_deref() == Some(track_name.as_str()))
                                .map(|t| t.name.clone())
                                .collect();
                            for child_name in children {
                                if let Some(child) =
                                    state.tracks.iter_mut().find(|t| t.name == child_name)
                                {
                                    child.parent_track = None;
                                }
                                child_tasks.push(self.send(Action::TrackSetParent {
                                    track_name: child_name,
                                    parent_name: None,
                                }));
                            }
                        }
                    }
                }
                if !clear_audio_indices.is_empty() {
                    child_tasks.push(self.send(Action::RemoveClip {
                        track_name: track_name.clone(),
                        kind: Kind::Audio,
                        clip_indices: clear_audio_indices,
                    }));
                }
                if !clear_midi_indices.is_empty() {
                    child_tasks.push(self.send(Action::RemoveClip {
                        track_name: track_name.clone(),
                        kind: Kind::MIDI,
                        clip_indices: clear_midi_indices,
                    }));
                }
                child_tasks.push(self.send(Action::TrackSetFolder {
                    track_name: track_name.clone(),
                    is_folder,
                }));
                return Task::batch(child_tasks);
            }
            Message::TrackSetParent {
                ref track_name,
                ref parent_name,
            } => {
                let mut state = self.state.blocking_write();

                if let Some(parent) = parent_name {
                    if parent == track_name {
                        state.message = "Track cannot be its own parent".to_string();
                        return Task::none();
                    }

                    let mut current = parent.as_str();
                    loop {
                        if current == track_name.as_str() {
                            state.message = "Cannot create circular folder hierarchy".to_string();
                            return Task::none();
                        }
                        if let Some(next) = state
                            .tracks
                            .iter()
                            .find(|t| t.name == current)
                            .and_then(|t| t.parent_track.as_deref())
                        {
                            current = next;
                        } else {
                            break;
                        }
                    }
                }
                if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name) {
                    track.parent_track = parent_name.clone();
                }
                drop(state);
                return self.send(Action::TrackSetParent {
                    track_name: track_name.clone(),
                    parent_name: parent_name.clone(),
                });
            }
            Message::TrackMidiLaneChannelSelected {
                ref track_name,
                lane,
                channel,
            } => {
                return self.send(Action::TrackSetMidiLaneChannel {
                    track_name: track_name.clone(),
                    lane,
                    channel: channel.to_engine(),
                });
            }
            Message::TrackSetupToggle(ref track_name) => {
                let mut state = self.state.blocking_write();
                let mut opened_setup = false;
                if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name)
                    && !track.is_folder
                {
                    track.setup_open = !track.setup_open;
                    opened_setup = track.setup_open;
                }
                if opened_setup {
                    let current_width = match state.tracks_width {
                        Length::Fixed(width) => width,
                        _ => 0.0,
                    };
                    if current_width < TRACK_SETUP_MIN_TRACKS_WIDTH {
                        state.tracks_width = Length::Fixed(TRACK_SETUP_MIN_TRACKS_WIDTH);
                    }
                }
            }
            Message::TrackMidiSetupChannelSelected {
                ref track_name,
                channel,
            } => {
                let engine_channel = channel.to_engine();
                {
                    let mut state = self.state.blocking_write();
                    if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name) {
                        for lane_channel in track.midi_lane_channels.iter_mut() {
                            *lane_channel = engine_channel;
                        }
                    }
                }
                return self.send(Action::TrackSetMidiLaneChannel {
                    track_name: track_name.clone(),
                    lane: 0,
                    channel: engine_channel,
                });
            }
            Message::TrackAddReturn(ref track_name) => {
                self.state.blocking_write().track_context_menu = None;
                return self.send(Action::TrackAddAudioInput(track_name.clone()));
            }
            Message::TrackAddSend(ref track_name) => {
                self.state.blocking_write().track_context_menu = None;
                return self.send(Action::TrackAddAudioOutput(track_name.clone()));
            }
            Message::TrackMidiLearnArm {
                ref track_name,
                target,
            } => {
                self.state.blocking_write().message = format!(
                    "MIDI learn armed for '{}' ({:?}). Move a hardware MIDI CC control.",
                    track_name, target
                );
                return self.send(Action::TrackArmMidiLearn {
                    track_name: track_name.clone(),
                    target,
                });
            }
            Message::TrackMidiLearnClear {
                ref track_name,
                target,
            } => {
                return self.send(Action::TrackSetMidiLearnBinding {
                    track_name: track_name.clone(),
                    target,
                    binding: None,
                });
            }
            Message::GlobalMidiLearnArm { target } => {
                self.state.blocking_write().message = format!(
                    "Global MIDI learn armed for {:?}. Move a hardware MIDI CC control.",
                    target
                );
                return self.send(Action::GlobalArmMidiLearn { target });
            }
            Message::GlobalMidiLearnClear { target } => {
                return self.send(Action::SetGlobalMidiLearnBinding {
                    target,
                    binding: None,
                });
            }
            Message::SessionMidiLearnArm { target } => {
                self.state.blocking_write().message = format!(
                    "Session MIDI learn armed for {:?}. Move a hardware MIDI CC control.",
                    target
                );
                return self.send(Action::SessionArmMidiLearn { target });
            }
            Message::SessionMidiLearnClear { target } => {
                return self.send(Action::SetSessionMidiLearnBinding {
                    target,
                    binding: None,
                });
            }
            Message::TrackFreezeToggle { ref track_name } => {
                if self.freeze_in_progress {
                    if self.freeze_track_name.as_deref() == Some(track_name.as_str()) {
                        self.freeze_cancel_requested = true;
                        self.state.blocking_write().message =
                            format!("Cancel requested for freezing '{}'", track_name);
                        return self.send(Action::TrackOfflineBounceCancel {
                            track_name: track_name.clone(),
                        });
                    } else {
                        self.state.blocking_write().message = format!(
                            "Freeze in progress for '{}'",
                            self.freeze_track_name.clone().unwrap_or_default()
                        );
                    }
                    return Task::none();
                }
                let Some(session_root) = self.session_dir.clone() else {
                    self.state.blocking_write().message =
                        "Freeze requires an opened/saved session".to_string();
                    return Task::none();
                };
                let track_snapshot = {
                    let state = self.state.blocking_read();
                    state.tracks.iter().find(|t| t.name == *track_name).cloned()
                };
                let Some(track) = track_snapshot else {
                    self.state.blocking_write().message =
                        format!("Track '{}' not found", track_name);
                    return Task::none();
                };

                if track.frozen {
                    let current_audio_len = track.audio.clips.len();
                    let current_midi_len = track.midi.clips.len();
                    let restore_audio = track.frozen_audio_backup.clone();
                    let restore_midi = track.frozen_midi_backup.clone();
                    {
                        let mut state = self.state.blocking_write();
                        if let Some(track_mut) =
                            state.tracks.iter_mut().find(|t| t.name == *track_name)
                        {
                            track_mut.frozen_audio_backup.clear();
                            track_mut.frozen_midi_backup.clear();
                            track_mut.frozen_render_clip = None;
                        }
                    }
                    let mut tasks = vec![self.send(Action::BeginHistoryGroup)];
                    if current_audio_len > 0 {
                        tasks.push(self.send(Action::RemoveClip {
                            track_name: track_name.clone(),
                            kind: Kind::Audio,
                            clip_indices: (0..current_audio_len).collect(),
                        }));
                    }
                    if current_midi_len > 0 {
                        tasks.push(self.send(Action::RemoveClip {
                            track_name: track_name.clone(),
                            kind: Kind::MIDI,
                            clip_indices: (0..current_midi_len).collect(),
                        }));
                    }
                    for clip in restore_audio {
                        tasks.push(
                            self.send(Action::AddClip {
                                clip_id: clip.id,
                                name: clip.name,
                                track_name: track_name.clone(),
                                start: clip.start,
                                length: clip.length,
                                offset: clip.offset,
                                input_channel: clip.input_channel,
                                muted: clip.muted,
                                peaks_file: clip.peaks_file,
                                kind: Kind::Audio,
                                fade_enabled: clip.fade_enabled,
                                fade_in_samples: clip.fade_in_samples,
                                fade_out_samples: clip.fade_out_samples,
                                source_name: clip.pitch_correction_source_name,
                                source_offset: clip.pitch_correction_source_offset,
                                source_length: clip.pitch_correction_source_length,
                                preview_name: clip.pitch_correction_preview_name,
                                pitch_correction_points: clip
                                    .pitch_correction_points
                                    .into_iter()
                                    .map(|point| maolan_engine::message::PitchCorrectionPointData {
                                        start_sample: point.start_sample,
                                        length_samples: point.length_samples,
                                        detected_midi_pitch: point.detected_midi_pitch,
                                        target_midi_pitch: point.target_midi_pitch,
                                        clarity: point.clarity,
                                    })
                                    .collect(),
                                pitch_correction_frame_likeness: clip
                                    .pitch_correction_frame_likeness,
                                pitch_correction_inertia_ms: clip.pitch_correction_inertia_ms,
                                pitch_correction_formant_compensation: clip
                                    .pitch_correction_formant_compensation,
                                plugin_graph_json: clip.plugin_graph_json,
                            }),
                        );
                    }
                    for clip in restore_midi {
                        tasks.push(self.send(Action::AddClip {
                            clip_id: clip.id,
                            name: clip.name,
                            track_name: track_name.clone(),
                            start: clip.start,
                            length: clip.length,
                            offset: clip.offset,
                            input_channel: clip.input_channel,
                            muted: clip.muted,
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
                            plugin_graph_json: None,
                        }));
                    }
                    tasks.push(self.send(Action::TrackSetFrozen {
                        track_name: track_name.clone(),
                        frozen: false,
                    }));
                    tasks.push(self.send(Action::EndHistoryGroup));
                    return Task::batch(tasks);
                }

                if track.audio.clips.is_empty() && track.midi.clips.is_empty() {
                    self.state.blocking_write().message =
                        format!("Track '{}' has no clips to freeze", track_name);
                    return Task::none();
                }
                let corrected_clips = track
                    .audio
                    .clips
                    .iter()
                    .enumerate()
                    .filter(|(_, clip)| !clip.pitch_correction_points.is_empty())
                    .map(|(clip_index, clip)| (clip_index, clip.clone()))
                    .collect::<Vec<_>>();
                if !corrected_clips.is_empty() {
                    self.state.blocking_write().message =
                        format!("Preparing pitch-corrected freeze for '{}'", track_name);
                    return Task::perform(
                        {
                            let session_root = session_root.clone();
                            let track_name = track_name.clone();
                            async move {
                                let mut prepared_clips = Vec::new();
                                for (clip_index, clip) in corrected_clips {
                                    let source_name = clip
                                        .pitch_correction_source_name
                                        .clone()
                                        .unwrap_or_else(|| clip.name.clone());
                                    let source_path =
                                        if std::path::Path::new(&source_name).is_absolute() {
                                            std::path::PathBuf::from(&source_name)
                                        } else {
                                            session_root.join(&source_name)
                                        };
                                    let rendered =
                                        Self::render_audio_clip_pitch_correction_with_rubberband(
                                            &source_path,
                                            &session_root,
                                            &clip.name,
                                            clip.pitch_correction_source_offset
                                                .unwrap_or(clip.offset),
                                            clip.pitch_correction_source_length
                                                .unwrap_or(clip.length),
                                            &clip.pitch_correction_points,
                                            clip.pitch_correction_inertia_ms.unwrap_or(100),
                                            clip.pitch_correction_formant_compensation
                                                .unwrap_or(true),
                                            |_, _| {},
                                        )
                                        .await;
                                    let (preview_name, _, _) = match rendered {
                                        Ok(rendered) => rendered,
                                        Err(e) => {
                                            return (
                                                track_name,
                                                prepared_clips,
                                                Err::<(), String>(e.to_string()),
                                            );
                                        }
                                    };
                                    prepared_clips.push(crate::message::PreparedFreezeClip {
                                        clip_index,
                                        preview_name,
                                    });
                                }
                                (track_name, prepared_clips, Ok::<(), String>(()))
                            }
                        },
                        |(track_name, prepared_clips, result)| Message::TrackFreezePrepared {
                            track_name,
                            prepared_clips,
                            result,
                        },
                    );
                }
                let render_length = track
                    .audio
                    .clips
                    .iter()
                    .map(|clip| clip.start.saturating_add(clip.length))
                    .chain(
                        track
                            .midi
                            .clips
                            .iter()
                            .map(|clip| clip.start.saturating_add(clip.length)),
                    )
                    .max()
                    .unwrap_or(0)
                    .max(1);
                let stem = format!("{}_freeze", Self::sanitize_peak_file_component(track_name));
                let render_rel =
                    match Self::unique_import_rel_path(&session_root, "audio", &stem, "wav") {
                        Ok(path) => path,
                        Err(e) => {
                            self.state.blocking_write().message =
                                format!("Failed to prepare freeze render: {e}");
                            return Task::none();
                        }
                    };
                let render_abs = session_root.join(&render_rel).to_string_lossy().to_string();
                let mut automation_lanes = Vec::<OfflineAutomationLane>::new();
                for lane in track
                    .automation_lanes
                    .iter()
                    .filter(|lane| !lane.points.is_empty())
                {
                    let target = match lane.target {
                        crate::message::TrackAutomationTarget::Volume => {
                            OfflineAutomationTarget::Volume
                        }
                        crate::message::TrackAutomationTarget::Balance => {
                            OfflineAutomationTarget::Balance
                        }
                        crate::message::TrackAutomationTarget::MidiCc { channel, cc } => {
                            OfflineAutomationTarget::MidiCc { channel, cc }
                        }
                        #[cfg(all(unix, not(target_os = "macos")))]
                        crate::message::TrackAutomationTarget::Lv2Parameter {
                            instance_id,
                            index,
                            min,
                            max,
                        } => OfflineAutomationTarget::Lv2Parameter {
                            instance_id,
                            index,
                            min,
                            max,
                        },
                        #[cfg(not(all(unix, not(target_os = "macos"))))]
                        crate::message::TrackAutomationTarget::Lv2Parameter { .. } => continue,
                        crate::message::TrackAutomationTarget::Vst3Parameter {
                            instance_id,
                            param_id,
                        } => OfflineAutomationTarget::Vst3Parameter {
                            instance_id,
                            param_id,
                        },
                        crate::message::TrackAutomationTarget::ClapParameter {
                            instance_id,
                            param_id,
                            min,
                            max,
                        } => OfflineAutomationTarget::ClapParameter {
                            instance_id,
                            param_id,
                            min,
                            max,
                        },
                    };
                    let points = lane
                        .points
                        .iter()
                        .map(|p| OfflineAutomationPoint {
                            sample: p.sample,
                            value: p.value,
                        })
                        .collect::<Vec<_>>();
                    automation_lanes.push(OfflineAutomationLane {
                        target,
                        visible: true,
                        points,
                    });
                }
                self.pending_track_freeze_bounce.insert(
                    track_name.clone(),
                    super::super::PendingTrackFreezeBounce {
                        rendered_clip_rel: render_rel,
                        rendered_length: render_length.max(1),
                        backup_audio: track.audio.clips.clone(),
                        backup_midi: track.midi.clips.clone(),
                    },
                );
                self.freeze_in_progress = true;
                self.freeze_progress = 0.0;
                self.freeze_track_name = Some(track_name.clone());
                self.freeze_cancel_requested = false;
                self.state.blocking_write().message =
                    format!("Rendering freeze for '{}'", track_name);
                return self.send(Action::TrackOfflineBounce {
                    track_name: track_name.clone(),
                    output_path: render_abs,
                    start_sample: 0,
                    length_samples: render_length.max(1),
                    automation_lanes,
                    apply_fader: false,
                });
            }
            Message::TrackFreezePrepared {
                ref track_name,
                ref prepared_clips,
                ref result,
            } => {
                if let Err(e) = result {
                    self.state.blocking_write().message =
                        format!("Failed to prepare freeze for '{}': {e}", track_name);
                    return Task::none();
                }
                let Some(session_root) = self.session_dir.clone() else {
                    self.state.blocking_write().message =
                        "Freeze requires an opened/saved session".to_string();
                    return Task::none();
                };
                let original_track_snapshot = {
                    let state = self.state.blocking_read();
                    state.tracks.iter().find(|t| t.name == *track_name).cloned()
                };
                let Some(track) = original_track_snapshot.clone() else {
                    self.state.blocking_write().message =
                        format!("Track '{}' not found", track_name);
                    return Task::none();
                };
                let render_length = track
                    .audio
                    .clips
                    .iter()
                    .map(|clip| clip.start.saturating_add(clip.length))
                    .chain(
                        track
                            .midi
                            .clips
                            .iter()
                            .map(|clip| clip.start.saturating_add(clip.length)),
                    )
                    .max()
                    .unwrap_or(0)
                    .max(1);
                let stem = format!("{}_freeze", Self::sanitize_peak_file_component(track_name));
                let render_rel =
                    match Self::unique_import_rel_path(&session_root, "audio", &stem, "wav") {
                        Ok(path) => path,
                        Err(e) => {
                            self.state.blocking_write().message =
                                format!("Failed to prepare freeze render: {e}");
                            return Task::none();
                        }
                    };
                let render_abs = session_root.join(&render_rel).to_string_lossy().to_string();
                let mut automation_lanes = Vec::<OfflineAutomationLane>::new();
                for lane in track
                    .automation_lanes
                    .iter()
                    .filter(|lane| !lane.points.is_empty())
                {
                    let target = match lane.target {
                        crate::message::TrackAutomationTarget::Volume => {
                            OfflineAutomationTarget::Volume
                        }
                        crate::message::TrackAutomationTarget::Balance => {
                            OfflineAutomationTarget::Balance
                        }
                        crate::message::TrackAutomationTarget::MidiCc { channel, cc } => {
                            OfflineAutomationTarget::MidiCc { channel, cc }
                        }
                        #[cfg(all(unix, not(target_os = "macos")))]
                        crate::message::TrackAutomationTarget::Lv2Parameter {
                            instance_id,
                            index,
                            min,
                            max,
                        } => OfflineAutomationTarget::Lv2Parameter {
                            instance_id,
                            index,
                            min,
                            max,
                        },
                        #[cfg(not(all(unix, not(target_os = "macos"))))]
                        crate::message::TrackAutomationTarget::Lv2Parameter { .. } => continue,
                        crate::message::TrackAutomationTarget::Vst3Parameter {
                            instance_id,
                            param_id,
                        } => OfflineAutomationTarget::Vst3Parameter {
                            instance_id,
                            param_id,
                        },
                        crate::message::TrackAutomationTarget::ClapParameter {
                            instance_id,
                            param_id,
                            min,
                            max,
                        } => OfflineAutomationTarget::ClapParameter {
                            instance_id,
                            param_id,
                            min,
                            max,
                        },
                    };
                    let points = lane
                        .points
                        .iter()
                        .map(|p| OfflineAutomationPoint {
                            sample: p.sample,
                            value: p.value,
                        })
                        .collect::<Vec<_>>();
                    automation_lanes.push(OfflineAutomationLane {
                        target,
                        visible: true,
                        points,
                    });
                }
                self.pending_track_freeze_bounce.insert(
                    track_name.clone(),
                    super::super::PendingTrackFreezeBounce {
                        rendered_clip_rel: render_rel,
                        rendered_length: render_length.max(1),
                        backup_audio: original_track_snapshot
                            .as_ref()
                            .map(|t| t.audio.clips.clone())
                            .unwrap_or_default(),
                        backup_midi: original_track_snapshot
                            .as_ref()
                            .map(|t| t.midi.clips.clone())
                            .unwrap_or_default(),
                    },
                );
                self.freeze_in_progress = true;
                self.freeze_progress = 0.0;
                self.freeze_track_name = Some(track_name.clone());
                self.freeze_cancel_requested = false;
                self.state.blocking_write().message =
                    format!("Rendering freeze for '{}'", track_name);
                let mut tasks = Vec::new();
                for prepared in prepared_clips {
                    if let Some(original) = original_track_snapshot
                        .as_ref()
                        .and_then(|t| t.audio.clips.get(prepared.clip_index))
                    {
                        tasks.push(self.send(Action::SetClipPitchCorrection {
                            track_name: track_name.clone(),
                            clip_index: prepared.clip_index,
                            preview_name: Some(prepared.preview_name.clone()),
                            source_name: original.pitch_correction_source_name.clone(),
                            source_offset: original.pitch_correction_source_offset,
                            source_length: original.pitch_correction_source_length,
                            pitch_correction_points: vec![],
                            pitch_correction_frame_likeness:
                                original.pitch_correction_frame_likeness,
                            pitch_correction_inertia_ms: original.pitch_correction_inertia_ms,
                            pitch_correction_formant_compensation:
                                original.pitch_correction_formant_compensation,
                        }));
                    }
                }
                tasks.push(self.send(Action::TrackOfflineBounce {
                    track_name: track_name.clone(),
                    output_path: render_abs,
                    start_sample: 0,
                    length_samples: render_length.max(1),
                    automation_lanes,
                    apply_fader: false,
                }));
                return Task::batch(tasks);
            }
            Message::TrackFreezeFlatten { ref track_name } => {
                let is_frozen = {
                    let state = self.state.blocking_read();
                    state
                        .tracks
                        .iter()
                        .find(|t| t.name == *track_name)
                        .is_some_and(|t| t.frozen)
                };
                if !is_frozen {
                    self.state.blocking_write().message =
                        format!("Track '{}' is not frozen", track_name);
                    return Task::none();
                }
                {
                    let mut state = self.state.blocking_write();
                    if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name) {
                        track.frozen_audio_backup.clear();
                        track.frozen_midi_backup.clear();
                        track.frozen_render_clip = None;
                    }
                    state.message = format!("Flattened track '{}'", track_name);
                }
                return self.send(Action::TrackSetFrozen {
                    track_name: track_name.clone(),
                    frozen: false,
                });
            }
            Message::TrackAutomationToggleLane {
                ref track_name,
                target,
            } => {
                let mut state = self.state.blocking_write();
                if let Some(track) = state
                    .tracks
                    .iter_mut()
                    .find(|track| track.name == track_name.as_str())
                {
                    let previous_lane_height = track
                        .lane_layout()
                        .representative_height()
                        .max(TRACK_SUBTRACK_MIN_HEIGHT);
                    let previously_visible = track.automation_lane_count();
                    if let Some(lane) = track
                        .automation_lanes
                        .iter_mut()
                        .find(|lane| lane.target == target)
                    {
                        lane.visible = !lane.visible;
                    } else {
                        track
                            .automation_lanes
                            .push(crate::state::TrackAutomationLane {
                                target,
                                visible: true,
                                points: vec![],
                            });
                    }
                    let lanes_delta =
                        track.automation_lane_count() as isize - previously_visible as isize;
                    track.adjust_height_for_automation_lanes(previous_lane_height, lanes_delta);
                }
                drop(state);
                return self.send_track_automation_lanes(track_name);
            }
            Message::TrackColorChanged {
                ref track_name,
                color,
            } => {
                let engine_color = color.map(|c| maolan_engine::message::TrackColor {
                    r: c.r,
                    g: c.g,
                    b: c.b,
                    a: c.a,
                });
                return self.send(maolan_engine::message::Action::TrackSetColor {
                    track_name: track_name.clone(),
                    color: engine_color,
                });
            }
            Message::TrackColorClear(ref track_name) => {
                return self.send(maolan_engine::message::Action::TrackSetColor {
                    track_name: track_name.clone(),
                    color: None,
                });
            }
            Message::TrackAutomationCycleMode { ref track_name } => {
                let mut state = self.state.blocking_write();
                if let Some(track) = state
                    .tracks
                    .iter_mut()
                    .find(|track| track.name == track_name.as_str())
                {
                    let next_mode = match track.automation_mode {
                        TrackAutomationMode::Read => TrackAutomationMode::Touch,
                        TrackAutomationMode::Touch => TrackAutomationMode::Latch,
                        TrackAutomationMode::Latch => TrackAutomationMode::Write,
                        TrackAutomationMode::Write => TrackAutomationMode::Read,
                    };
                    track.automation_mode = next_mode;
                    state.message = format!(
                        "Track '{}' automation mode: {}",
                        track.name, track.automation_mode
                    );
                }
                drop(state);
                let key = track_name.clone();
                let mode = self
                    .state
                    .blocking_read()
                    .tracks
                    .iter()
                    .find(|track| track.name == key)
                    .map(|track| track.automation_mode);
                let mode_task = self.send_track_automation_lanes(track_name);
                match mode {
                    Some(TrackAutomationMode::Read) => {
                        self.touch_active_keys.remove(&key);
                        self.touch_automation_overrides.remove(&key);
                        self.latch_automation_overrides.remove(&key);
                    }
                    Some(TrackAutomationMode::Touch) => {
                        self.latch_automation_overrides.remove(&key);
                    }
                    Some(TrackAutomationMode::Write) => {
                        self.touch_active_keys.remove(&key);
                        self.touch_automation_overrides.remove(&key);
                    }
                    _ => {}
                }
                return mode_task;
            }
            Message::TrackAutomationAddPluginLanes {
                ref track_name,
                ref plugin_id,
                ref format,
            } => {
                match format.as_str() {
                    "CLAP" => {
                        self.pending_add_clap_automation_paths
                            .insert((track_name.clone(), plugin_id.clone()));
                    }
                    "VST3" => {
                        self.pending_add_vst3_automation_paths
                            .insert((track_name.clone(), plugin_id.clone()));
                    }
                    #[cfg(all(unix, not(target_os = "macos")))]
                    "LV2" => {
                        self.pending_add_lv2_automation_uris
                            .insert((track_name.clone(), plugin_id.clone()));
                    }
                    _ => {}
                }
                return self.send(Action::TrackGetPluginGraph {
                    track_name: track_name.clone(),
                    include_state: false,
                });
            }
            Message::TrackAutomationLaneInsertPoints {
                ref track_name,
                target,
                ref points,
            } => {
                if points.is_empty() {
                    return Task::none();
                }
                let mut state = self.state.blocking_write();
                let Some(track) = state
                    .tracks
                    .iter_mut()
                    .find(|track| track.name == track_name.as_str())
                else {
                    return Task::none();
                };
                let previous_lane_height = track
                    .lane_layout()
                    .representative_height()
                    .max(TRACK_SUBTRACK_MIN_HEIGHT);
                let previously_visible = track.automation_lane_count();

                let lane_exists = track
                    .automation_lanes
                    .iter()
                    .any(|lane| lane.target == target);
                if !lane_exists {
                    track
                        .automation_lanes
                        .push(crate::state::TrackAutomationLane {
                            target,
                            visible: true,
                            points: vec![],
                        });
                }
                let lane = track
                    .automation_lanes
                    .iter_mut()
                    .find(|lane| lane.target == target)
                    .expect("lane was just created");
                lane.visible = true;

                let min_sample = points.iter().map(|p| p.sample).min().unwrap_or(0);
                let max_sample = points.iter().map(|p| p.sample).max().unwrap_or(min_sample);

                // Replace any existing points in the drawn range so the new line
                // overrides the previous curve, matching MIDI controller lane
                // behavior. Points outside the range are kept, creating an
                // interpolation from the old line to the new line at the edges.
                lane.points
                    .retain(|point| point.sample < min_sample || point.sample > max_sample);
                for point in points {
                    lane.points.push(crate::state::TrackAutomationPoint {
                        sample: point.sample,
                        value: point.value,
                    });
                }
                lane.points.sort_unstable_by_key(|p| p.sample);

                let lanes_delta =
                    track.automation_lane_count() as isize - previously_visible as isize;
                track.adjust_height_for_automation_lanes(previous_lane_height, lanes_delta);
                drop(state);
                return self.send_track_automation_lanes(track_name);
            }
            Message::TrackAutomationLaneDeletePoint {
                ref track_name,
                target,
                sample,
            } => {
                let mut state = self.state.blocking_write();
                if let Some(track) = state
                    .tracks
                    .iter_mut()
                    .find(|track| track.name == track_name.as_str())
                    && let Some(lane) = track
                        .automation_lanes
                        .iter_mut()
                        .find(|lane| lane.target == target)
                {
                    lane.points.retain(|point| point.sample != sample);
                }
                drop(state);
                return self.send_track_automation_lanes(track_name);
            }
            Message::RemoveSelectedTracks => {
                let mut actions = vec![Action::BeginHistoryGroup];
                for name in &self.state.blocking_read().selected {
                    actions.push(Action::RemoveTrack(name.clone()));
                }
                actions.push(Action::EndHistoryGroup);
                return Self::restore_actions_task(actions);
            }
            Message::ConnectionViewSelectTrack(ref idx) => {
                let ctrl = self.state.blocking_read().ctrl;
                let mut state = self.state.blocking_write();

                match &mut state.connection_view_selection {
                    ConnectionViewSelection::Tracks(set) if ctrl => {
                        if set.contains(idx.as_str()) {
                            set.remove(idx.as_str());
                        } else {
                            set.insert(idx.clone());
                        }
                    }
                    _ => {
                        let mut set = std::collections::HashSet::new();
                        set.insert(idx.clone());
                        state.connection_view_selection = ConnectionViewSelection::Tracks(set);
                    }
                }
            }
            Message::ConnectionViewDeselectAll => {
                let mut state = self.state.blocking_write();
                state.connection_view_selection = ConnectionViewSelection::None;
                state.plugin_graph_selected_connections.clear();
                state.plugin_graph_selected_connectable_connections.clear();
                state.plugin_graph_selected_plugins.clear();
            }
            Message::ConnectionPositionsChanged => {
                self.has_unsaved_changes = true;
            }
            Message::PluginGraphControllerMenuOpen {
                track_name,
                instance_id,
                position,
            } => {
                let mut state = self.state.blocking_write();
                state.plugin_graph_controller_menu =
                    Some(crate::state::PluginControllerMenuState {
                        track_name,
                        instance_id,
                        anchor: position,
                        hovered: None,
                    });
                {
                    let track_name = state
                        .plugin_graph_controller_menu
                        .as_ref()
                        .map(|m| m.track_name.clone())
                        .unwrap_or_default();
                    let instance_id = state
                        .plugin_graph_controller_menu
                        .as_ref()
                        .map(|m| m.instance_id)
                        .unwrap_or(0);
                    let cached = state
                        .plugin_parameters_by_track
                        .get(&track_name)
                        .and_then(|cache| cache.get(&instance_id))
                        .is_some();
                    if !cached {
                        let plugin = state
                            .plugin_graph_plugins
                            .iter()
                            .find(|p| p.instance_id == instance_id)
                            .cloned();
                        if let Some(plugin) = plugin {
                            drop(state);
                            if plugin.format.eq_ignore_ascii_case("CLAP") {
                                return self.send(Action::TrackGetClapParameters {
                                    track_name,
                                    instance_id,
                                });
                            } else if plugin.format.eq_ignore_ascii_case("VST3") {
                                return self.send(Action::TrackGetVst3Parameters {
                                    track_name,
                                    instance_id,
                                });
                            }
                            #[cfg(all(unix, not(target_os = "macos")))]
                            if plugin.format.eq_ignore_ascii_case("LV2") {
                                return self.send(Action::TrackGetLv2PluginControls {
                                    track_name,
                                    instance_id,
                                });
                            }
                        }
                    }
                }
                return Task::none();
            }
            Message::PluginGraphControllerMenuClose => {
                self.state.blocking_write().plugin_graph_controller_menu = None;
            }
            Message::PluginGraphControllerMenuHover(hovered) => {
                if let Some(menu) = self
                    .state
                    .blocking_write()
                    .plugin_graph_controller_menu
                    .as_mut()
                {
                    menu.hovered = hovered;
                }
            }
            Message::PluginGraphShowController {
                track_name,
                instance_id,
                param_id,
                name,
                value,
                min,
                max,
            } => {
                self.has_unsaved_changes = true;
                let mut state = self.state.blocking_write();
                state.plugin_graph_controller_menu = None;
                let controllers = state
                    .plugin_graph_visible_controllers
                    .entry(track_name)
                    .or_default()
                    .entry(instance_id)
                    .or_default();
                if let Some(pos) = controllers.iter().position(|c| c.param_id == param_id) {
                    controllers.remove(pos);
                } else {
                    controllers.push(crate::state::ShownPluginController {
                        param_id,
                        name,
                        value,
                        min,
                        max,
                    });
                }
                return Task::none();
            }
            Message::PluginGraphHideController {
                track_name,
                instance_id,
                param_id,
            } => {
                self.has_unsaved_changes = true;
                let mut state = self.state.blocking_write();
                if let Some(controllers) = state
                    .plugin_graph_visible_controllers
                    .get_mut(&track_name)
                    .and_then(|map| map.get_mut(&instance_id))
                {
                    controllers.retain(|c| c.param_id != param_id);
                }
                return Task::none();
            }
            Message::MarkerLaneCreate { sample } => {
                self.state.blocking_write().marker_dialog = Some(crate::state::MarkerDialog {
                    sample,
                    marker_index: None,
                    name: String::new(),
                });
                return iced::widget::operation::focus(
                    crate::track_marker::MarkerView::name_input_id(),
                );
            }
            Message::MarkerNameInput(_) => {}
            Message::MarkerNameConfirm => {
                let dialog = self.state.blocking_read().marker_dialog.clone();
                let Some(dialog) = dialog else {
                    return Task::none();
                };
                let marker_name = dialog.name.trim().to_string();
                if marker_name.is_empty() {
                    return Task::none();
                }
                let mut state = self.state.blocking_write();
                let markers = &mut state.session_markers;
                if let Some(marker_index) = dialog.marker_index {
                    if let Some(marker) = markers.get_mut(marker_index) {
                        marker.name = marker_name;
                    }
                } else {
                    markers.push(crate::state::EditorMarker {
                        sample: dialog.sample,
                        name: marker_name,
                    });
                }
                markers.sort_unstable_by_key(|marker| marker.sample);
                markers.dedup_by(|a, b| a.sample == b.sample && a.name == b.name);
                state.marker_dialog = None;
            }
            Message::MarkerNameCancel => {
                self.state.blocking_write().marker_dialog = None;
            }
            Message::SelectClip {
                ref track_idx,
                clip_idx,
                kind,
            } => {
                use crate::state::ClipId;
                let ctrl = self.state.blocking_read().ctrl;
                let mut state = self.state.blocking_write();

                let clip_id = ClipId {
                    track_idx: track_idx.clone(),
                    clip_idx,
                    kind,
                };

                if ctrl {
                    if state.selected_clips.contains(&clip_id) {
                    } else {
                        state.selected_clips.insert(clip_id);
                    }
                } else {
                    let already_selected = state.selected_clips.contains(&clip_id);
                    if !already_selected {
                        state.selected_clips.clear();
                        state.selected_clips.insert(clip_id);
                    }
                }
                state.mouse_left_down = true;
                state.mouse_right_down = false;
                state.clip_click_consumed = true;
                state.clip_marquee_start = None;
                state.clip_marquee_end = None;
                state.midi_clip_create_start = None;
                state.midi_clip_create_end = None;
                let mut dragged =
                    crate::message::DraggedClip::new(kind, clip_idx, track_idx.clone());
                dragged.start = state.cursor;
                dragged.end = state.cursor;
                dragged.copy = state.ctrl;
                self.clip = Some(dragged);
            }
            Message::ClipRenameShow {
                ref track_idx,
                clip_idx,
                kind,
            } => {
                let mut state = self.state.blocking_write();

                let current_name = state
                    .tracks
                    .iter()
                    .find(|t| t.name == *track_idx)
                    .and_then(|t| match kind {
                        Kind::Audio => t.audio.clips.get(clip_idx).map(|c| c.name.clone()),
                        Kind::MIDI => t.midi.clips.get(clip_idx).map(|c| c.name.clone()),
                    })
                    .unwrap_or_default();

                let clean_name = {
                    let mut cleaned = current_name.clone();
                    if let Some(stripped) = cleaned.strip_prefix("audio/") {
                        cleaned = stripped.to_string();
                    }
                    if let Some(stripped) = cleaned.strip_suffix(".wav") {
                        cleaned = stripped.to_string();
                    }
                    cleaned
                };

                state.clip_rename_dialog = Some(crate::state::ClipRenameDialog {
                    track_idx: track_idx.clone(),
                    clip_idx,
                    kind,
                    new_name: clean_name,
                });
            }
            Message::ClipRenameInput(_) => {}
            Message::ClipRenameConfirm => {
                let dialog = self.state.blocking_read().clip_rename_dialog.clone();
                let Some(dialog) = dialog else {
                    return Task::none();
                };

                let new_name = dialog.new_name.trim().to_string();
                if new_name.is_empty() {
                    return Task::none();
                }

                let Some(session_dir) = &self.session_dir else {
                    self.state.blocking_write().message = "No session loaded".to_string();
                    self.state.blocking_write().clip_rename_dialog = None;
                    return Task::none();
                };

                let mut state = self.state.blocking_write();
                let Some(track) = state.tracks.iter().find(|t| t.name == dialog.track_idx) else {
                    state.message = format!("Track {} not found", dialog.track_idx);
                    state.clip_rename_dialog = None;
                    return Task::none();
                };

                let old_name = match dialog.kind {
                    Kind::Audio => {
                        if dialog.clip_idx >= track.audio.clips.len() {
                            state.message = "Clip not found".to_string();
                            state.clip_rename_dialog = None;
                            return Task::none();
                        }
                        track.audio.clips[dialog.clip_idx].name.clone()
                    }
                    Kind::MIDI => {
                        if dialog.clip_idx >= track.midi.clips.len() {
                            state.message = "Clip not found".to_string();
                            state.clip_rename_dialog = None;
                            return Task::none();
                        }
                        track.midi.clips[dialog.clip_idx].name.clone()
                    }
                };

                let midi_ext = std::path::Path::new(&old_name)
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|s| s.to_ascii_lowercase())
                    .filter(|ext| ext == "mid" || ext == "midi")
                    .unwrap_or_else(|| "mid".to_string());
                let new_file_name = match dialog.kind {
                    Kind::Audio => format!("audio/{}.wav", new_name),
                    Kind::MIDI => format!("midi/{}.{}", new_name, midi_ext),
                };

                if dialog.kind == Kind::Audio {
                    let new_path = session_dir.join(&new_file_name);
                    if new_path.exists() {
                        state.message = format!("File '{}' already exists", new_file_name);
                        state.clip_rename_dialog = None;
                        return Task::none();
                    }

                    let old_path = session_dir.join(&old_name);
                    if old_path.exists()
                        && let Err(e) = std::fs::rename(&old_path, &new_path)
                    {
                        state.message = format!("Failed to rename file: {}", e);
                        state.clip_rename_dialog = None;
                        return Task::none();
                    }
                }

                for track in &mut state.tracks {
                    match dialog.kind {
                        Kind::Audio => {
                            for clip in &mut track.audio.clips {
                                if clip.name == old_name {
                                    clip.name = new_file_name.clone();
                                }
                                if clip.pitch_correction_source_name.as_deref()
                                    == Some(old_name.as_str())
                                {
                                    clip.pitch_correction_source_name = Some(new_file_name.clone());
                                }
                            }
                        }
                        Kind::MIDI => {
                            for clip in &mut track.midi.clips {
                                if clip.name == old_name {
                                    clip.name = new_file_name.clone();
                                }
                            }
                        }
                    }
                }

                state.message = format!("Renamed to '{}'", new_name);
                state.clip_rename_dialog = None;
                drop(state);

                return self.send(Action::RenameClip {
                    track_name: dialog.track_idx,
                    kind: dialog.kind,
                    clip_index: dialog.clip_idx,
                    new_name,
                });
            }
            Message::ClipRenameCancel => {
                self.state.blocking_write().clip_rename_dialog = None;
            }
            Message::ClipToggleFade {
                ref track_idx,
                clip_idx,
                kind,
            } => {
                if kind == Kind::MIDI {
                    return Task::none();
                }
                let new_fade_enabled = {
                    let mut state = self.state.blocking_write();
                    if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_idx) {
                        if let Some(clip) = track.audio.clips.get_mut(clip_idx) {
                            clip.fade_enabled = !clip.fade_enabled;
                            Some(clip.fade_enabled)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                };

                if let Some(fade_enabled) = new_fade_enabled {
                    let (fade_in_samples, fade_out_samples) = {
                        let state = self.state.blocking_read();
                        if let Some(track) = state.tracks.iter().find(|t| t.name == *track_idx) {
                            if let Some(clip) = track.audio.clips.get(clip_idx) {
                                (clip.fade_in_samples, clip.fade_out_samples)
                            } else {
                                (240, 240)
                            }
                        } else {
                            (240, 240)
                        }
                    };

                    return self.send(Action::SetClipFade {
                        track_name: track_idx.clone(),
                        clip_index: clip_idx,
                        kind,
                        fade_enabled,
                        fade_in_samples,
                        fade_out_samples,
                    });
                }
            }
            Message::ClipSetMuted {
                ref track_idx,
                clip_idx,
                kind,
                muted,
            } => {
                return self.send(Action::SetClipMuted {
                    track_name: track_idx.clone(),
                    clip_index: clip_idx,
                    kind,
                    muted,
                });
            }
            Message::ClipAssignToSessionSlot {
                ref track_idx,
                clip_idx,
                kind,
            } => {
                return self.assign_clip_to_session_slot(track_idx.clone(), clip_idx, kind);
            }
            Message::GroupSelectedClips => {
                return self.group_selected_clips();
            }
            Message::UngroupClip {
                ref track_idx,
                clip_idx,
                kind,
            } => {
                return self.ungroup_clip(track_idx.clone(), clip_idx, kind);
            }
            Message::ClipOpenPitchCorrection {
                ref track_idx,
                clip_idx,
            } => {
                return self.open_clip_pitch_correction(track_idx.clone(), clip_idx);
            }
            Message::ClipOpenPitchCorrectionProgress {
                ref clip_name,
                progress,
                ref operation,
            } => {
                self.clip_pitch_correction_in_progress = true;
                self.clip_pitch_correction_progress = progress.clamp(0.0, 1.0);
                self.clip_pitch_correction_clip_name = clip_name.clone();
                self.clip_pitch_correction_operation = operation.clone();
                let percent = (progress.clamp(0.0, 1.0) * 100.0) as usize;
                self.state.blocking_write().message = if let Some(op) = operation {
                    format!("Pitch correction '{clip_name}' ({percent}%): {op}")
                } else {
                    format!("Pitch correction '{clip_name}' ({percent}%)")
                };
                return Task::none();
            }
            Message::ClipStretchFinished { request, result } => match result {
                Ok((new_name, new_length)) => {
                    let clip_state = {
                        let state = self.state.blocking_read();
                        state
                            .tracks
                            .iter()
                            .find(|t| t.name == request.track_idx)
                            .and_then(|t| t.audio.clips.get(request.clip_idx))
                            .map(|clip| (clip.name.clone(), clip.start))
                    };
                    let Some((current_name, current_start)) = clip_state else {
                        self.state.blocking_write().message =
                            "Stretched clip finished, but the original clip no longer exists"
                                .to_string();
                        return Task::none();
                    };
                    if current_name != request.clip_name || current_start != request.start {
                        self.state.blocking_write().message =
                            "Discarded stale stretched clip result".to_string();
                        return Task::none();
                    }
                    let fade_in = request.fade_in_samples.min(new_length / 2);
                    let fade_out = request.fade_out_samples.min(new_length / 2);
                    self.state.blocking_write().message = format!(
                        "Stretched audio clip '{}' to {:.2}x",
                        request.clip_name, request.stretch_ratio
                    );
                    return self.send(Action::ApplyGroupedActions(vec![
                        Action::SetClipBounds {
                            track_name: request.track_idx.clone(),
                            clip_index: request.clip_idx,
                            kind: Kind::Audio,
                            start: request.start,
                            length: new_length,
                            offset: 0,
                        },
                        Action::SetClipSourceName {
                            track_name: request.track_idx.clone(),
                            kind: Kind::Audio,
                            clip_index: request.clip_idx,
                            name: new_name,
                        },
                        Action::SetClipFade {
                            track_name: request.track_idx,
                            clip_index: request.clip_idx,
                            kind: Kind::Audio,
                            fade_enabled: request.fade_enabled,
                            fade_in_samples: fade_in,
                            fade_out_samples: fade_out,
                        },
                    ]));
                }
                Err(e) => {
                    let mut state = self.state.blocking_write();
                    if let Some(track) = state
                        .tracks
                        .iter_mut()
                        .find(|t| t.name == request.track_idx)
                        && let Some(clip) = track.audio.clips.get_mut(request.clip_idx)
                        && clip.name == request.clip_name
                    {
                        clip.start = request.original_start;
                        clip.length = request.length;
                        clip.offset = request.offset;
                    }
                    state.message = format!("Failed to stretch clip '{}': {e}", request.clip_name);
                    return Task::none();
                }
            },
            Message::ClipOpenPitchCorrectionFinished { request, result } => {
                self.clip_pitch_correction_in_progress = false;
                self.clip_pitch_correction_progress = 0.0;
                self.clip_pitch_correction_clip_name.clear();
                self.clip_pitch_correction_operation = None;
                let clip_state = {
                    let state = self.state.blocking_read();
                    state
                        .tracks
                        .iter()
                        .find(|t| t.name == request.track_idx)
                        .and_then(|t| t.audio.clips.get(request.clip_idx))
                        .map(|clip| (clip.name.clone(), clip.start))
                };
                let Some((current_name, current_start)) = clip_state else {
                    self.state.blocking_write().message =
                        "Pitch correction data finished loading, but the original clip no longer exists"
                            .to_string();
                    return Task::none();
                };
                if current_name != request.clip_name || current_start != request.start {
                    self.state.blocking_write().message =
                        "Discarded stale pitch correction result".to_string();
                    return Task::none();
                }
                match result {
                    Ok(mut pitch_correction) => {
                        let mut state = self.state.blocking_write();
                        pitch_correction.track_idx = request.track_idx.clone();
                        pitch_correction.clip_index = request.clip_idx;
                        pitch_correction.frame_likeness = request.frame_likeness;
                        state.pitch_correction = Some(pitch_correction);
                        state.pitch_correction_frame_likeness = request.frame_likeness;
                        state.pitch_correction_selected_points.clear();
                        state.pitch_correction_dragging_points = None;
                        state.pitch_correction_selecting_rect = None;
                        state.piano = None;
                        state.piano_selected_notes.clear();
                        state.piano_selected_sysex = None;
                        state.piano_sysex_hex_input.clear();
                        state.piano_sysex_panel_open = false;
                        state.piano_sysex_scroll_y = 0.0;
                        state.piano_scroll_x = 0.0;
                        state.piano_scroll_y = 0.0;
                        state.view = View::PitchCorrection;
                        state.message =
                            format!("Opened pitch correction for '{}'", request.clip_name);
                        drop(state);
                    }
                    Err(e) => {
                        self.state.blocking_write().message =
                            format!("Pitch correction failed for '{}': {e}", request.clip_name);
                    }
                }
                return Task::none();
            }
            Message::TrackRenameShow(ref track_name) => {
                let mut state = self.state.blocking_write();
                state.track_rename_dialog = Some(crate::state::TrackRenameDialog {
                    old_name: track_name.clone(),
                    new_name: track_name.clone(),
                });
            }
            Message::TrackRenameInput(_) => {}
            Message::TemplateSaveInput(_) => {
                self.template_save.update(&message);
            }
            Message::TrackRenameConfirm => {
                let dialog = self.state.blocking_read().track_rename_dialog.clone();
                let Some(dialog) = dialog else {
                    return Task::none();
                };

                let new_name = dialog.new_name.trim().to_string();
                if new_name.is_empty() || new_name == dialog.old_name {
                    return Task::none();
                }

                self.state.blocking_write().track_rename_dialog = None;

                return self.send(Action::RenameTrack {
                    old_name: dialog.old_name,
                    new_name,
                });
            }
            Message::TrackRenameCancel => {
                self.state.blocking_write().track_rename_dialog = None;
            }
            Message::TrackTemplateSaveShow(ref track_name) => {
                let mut state = self.state.blocking_write();
                state.track_template_save_dialog = Some(crate::state::TrackTemplateSaveDialog {
                    track_name: track_name.clone(),
                    name: String::new(),
                });
                drop(state);
                self.modal = Some(Show::SaveTemplateAs);
            }
            Message::TrackTemplateSaveInput(_) => {
                self.track_template_save.update(&message);
            }
            Message::TrackTemplateSaveConfirm => {
                let dialog = self
                    .state
                    .blocking_read()
                    .track_template_save_dialog
                    .clone();
                let Some(dialog) = dialog else {
                    return Task::none();
                };

                let name = dialog.name.trim().to_string();
                if name.is_empty() {
                    return Task::none();
                }

                self.state.blocking_write().track_template_save_dialog = None;
                self.modal = None;

                let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
                let template_path = format!("{}/.config/maolan/track_templates/{}", home, name);

                return self
                    .refresh_graph_then_save_track_template(dialog.track_name, template_path);
            }
            Message::TrackTemplateSaveCancel => {
                self.state.blocking_write().track_template_save_dialog = None;
                self.modal = None;
            }
            Message::TrackContextMenuHover {
                ref track_name,
                position,
            } => {
                let mut state = self.state.blocking_write();
                state.track_context_hover = Some((track_name.clone(), position));
            }
            Message::TrackContextMenuSubmenuOpen(ref submenu) => {
                let track_name = {
                    let mut state = self.state.blocking_write();
                    if let Some(menu) = &mut state.track_context_menu {
                        menu.submenu = Some(submenu.clone());
                        Some(menu.track_name.clone())
                    } else {
                        None
                    }
                };
                if let Some(track_name) = track_name {
                    let state = self.state.blocking_read();
                    let plugins = state
                        .plugin_graphs_by_track
                        .get(&track_name)
                        .map(|(plugins, _)| plugins.clone())
                        .unwrap_or_default();
                    drop(state);
                    let mut tasks = Vec::new();
                    match submenu {
                        crate::state::TrackContextSubmenu::Automation
                        | crate::state::TrackContextSubmenu::Plugin { .. } => {
                            for plugin in &plugins {
                                if plugin.format.eq_ignore_ascii_case("CLAP") {
                                    tasks.push(self.send(Action::TrackGetClapParameters {
                                        track_name: track_name.clone(),
                                        instance_id: plugin.instance_id,
                                    }));
                                } else if plugin.format.eq_ignore_ascii_case("VST3") {
                                    tasks.push(self.send(Action::TrackGetVst3Parameters {
                                        track_name: track_name.clone(),
                                        instance_id: plugin.instance_id,
                                    }));
                                }
                            }
                        }
                        _ => {}
                    }
                    if !tasks.is_empty() {
                        return Task::batch(tasks);
                    }
                }
            }
            Message::TrackContextMenuSubmenuClose => {
                let mut state = self.state.blocking_write();
                if let Some(menu) = &mut state.track_context_menu {
                    menu.submenu = None;
                }
            }
            Message::TrackContextMenuToggle(ref track_name) => {
                let mut state = self.state.blocking_write();
                if state
                    .track_context_menu
                    .as_ref()
                    .is_some_and(|menu| menu.track_name == *track_name)
                {
                    state.track_context_menu = None;
                } else {
                    let anchor = state
                        .track_context_hover
                        .as_ref()
                        .filter(|(hover_track, _)| hover_track == track_name)
                        .map(|(_, point)| *point)
                        .unwrap_or(Point::new(8.0, 24.0));
                    state.track_context_menu = Some(crate::state::TrackContextMenuState {
                        track_name: track_name.clone(),
                        anchor,
                        submenu: None,
                    });
                }
                state.clip_context_menu = None;
                state.clip_click_consumed = true;
            }
            Message::TemplateSaveConfirm => {
                let dialog = self.state.blocking_read().template_save_dialog.clone();
                let Some(dialog) = dialog else {
                    return Task::none();
                };

                let name = dialog.name.trim().to_string();
                if name.is_empty() {
                    return Task::none();
                }

                self.state.blocking_write().template_save_dialog = None;
                self.modal = None;

                let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
                let template_path = format!("{}/.config/maolan/session_templates/{}", home, name);

                return self.refresh_graphs_then_save_template(template_path);
            }
            Message::TemplateSaveCancel => {
                self.state.blocking_write().template_save_dialog = None;
                self.modal = None;
            }
            Message::DeselectAll => {
                let mut state = self.state.blocking_write();
                state.selected.clear();
                state.selected_clips.clear();
                state.track_context_menu = None;
                state.connection_view_selection = ConnectionViewSelection::None;
                state.plugin_graph_selected_connectable_connections.clear();
            }
            Message::DeselectClips => {
                let mut state = self.state.blocking_write();
                if state.clip_click_consumed {
                    state.clip_click_consumed = false;
                    return Task::none();
                }
                self.clip = None;
                if self.modal.is_none() && matches!(state.view, View::Workspace) {
                    state.mouse_left_down = true;
                }
                state.mouse_right_down = false;
                state.clip_context_menu = None;
                state.track_context_menu = None;
                state.clip_marquee_start = None;
                state.clip_marquee_end = None;
                state.midi_clip_create_start = None;
                state.midi_clip_create_end = None;
                state.selected_clips.clear();
            }
            Message::MousePressed(button)
                if self.modal.is_none()
                    && matches!(self.state.blocking_read().view, View::Workspace) =>
            {
                if button == mouse::Button::Middle {
                    return self.split_clip_at_position(self.active_workspace_cursor());
                }
                match button {
                    mouse::Button::Left => {
                        let mut state = self.state.blocking_write();
                        state.mouse_left_down = true;
                        state.clip_marquee_start = None;
                        state.clip_marquee_end = None;
                    }
                    mouse::Button::Right => {
                        let cursor = self.active_workspace_cursor();
                        let clip_hit = self.clip_at_position(cursor);
                        let anchor_for_hit = cursor;
                        let mut state = self.state.blocking_write();
                        if let Some((track_idx, kind, clip_idx)) = clip_hit {
                            let id = crate::state::ClipId {
                                track_idx,
                                clip_idx,
                                kind,
                            };
                            if state
                                .clip_context_menu
                                .as_ref()
                                .is_some_and(|menu| menu.clip == id)
                            {
                                state.clip_context_menu = None;
                            } else {
                                state.clip_context_menu =
                                    Some(crate::state::ClipContextMenuState {
                                        clip: id,
                                        anchor: anchor_for_hit,
                                    });
                            }
                            state.mouse_right_down = false;
                            state.midi_clip_create_start = None;
                            state.midi_clip_create_end = None;
                            state.clip_click_consumed = true;
                        } else {
                            state.mouse_right_down = true;
                            state.midi_clip_create_start = None;
                            state.midi_clip_create_end = None;
                            state.clip_context_menu = None;
                        }
                    }
                    _ => {}
                }
            }
            Message::ConnectionViewSelectConnection(idx) => {
                let ctrl = self.state.blocking_read().ctrl;
                let mut state = self.state.blocking_write();
                connections::selection::apply_track_connection_selection(&mut state, idx, ctrl);
            }
            Message::RemoveSelected => {
                let state = self.state.blocking_read();
                match &state.connection_view_selection {
                    ConnectionViewSelection::Tracks(set) => {
                        let mut actions = vec![Action::BeginHistoryGroup];
                        for name in set {
                            actions.push(Action::RemoveTrack(name.clone()));
                        }
                        drop(state);
                        self.state.blocking_write().connection_view_selection =
                            ConnectionViewSelection::None;
                        actions.push(Action::EndHistoryGroup);
                        return Self::restore_actions_task(actions);
                    }
                    ConnectionViewSelection::Connections(set) => {
                        let actions = connections::selection::track_disconnect_actions(&state, set);
                        let tasks = actions
                            .into_iter()
                            .map(|a| self.send(a))
                            .collect::<Vec<_>>();
                        drop(state);
                        self.state.blocking_write().connection_view_selection =
                            ConnectionViewSelection::None;
                        return Task::batch(tasks);
                    }
                    ConnectionViewSelection::None => {}
                }
            }

            Message::Remove => {
                if !self.state.blocking_read().hw_loaded {
                    return Task::none();
                }
                if !self.selected_tempo_points.is_empty() {
                    return self.update(Message::TempoSelectionDelete);
                }
                if !self.selected_time_signature_points.is_empty() {
                    return self.update(Message::TimeSignatureSelectionDelete);
                }

                let state = self.state.blocking_read();
                let view = state.view.clone();
                let has_piano_notes =
                    state.piano.is_some() && !state.piano_selected_notes.is_empty();
                drop(state);

                if matches!(view, crate::state::View::Piano) && has_piano_notes {
                    return self.update(Message::PianoDeleteSelectedNotes);
                }

                let selected_clips: Vec<_> = if matches!(view, crate::state::View::Workspace) {
                    self.state
                        .blocking_read()
                        .selected_clips
                        .iter()
                        .cloned()
                        .collect()
                } else {
                    Vec::new()
                };
                if !selected_clips.is_empty() {
                    let mut audio_by_track: std::collections::HashMap<String, Vec<usize>> =
                        std::collections::HashMap::new();
                    let mut midi_by_track: std::collections::HashMap<String, Vec<usize>> =
                        std::collections::HashMap::new();
                    for clip in selected_clips {
                        match clip.kind {
                            Kind::Audio => audio_by_track
                                .entry(clip.track_idx)
                                .or_default()
                                .push(clip.clip_idx),
                            Kind::MIDI => midi_by_track
                                .entry(clip.track_idx)
                                .or_default()
                                .push(clip.clip_idx),
                        }
                    }

                    self.state.blocking_write().selected_clips.clear();

                    let mut actions = vec![Action::BeginHistoryGroup];
                    for (track_name, mut clip_indices) in audio_by_track {
                        clip_indices.sort_unstable_by(|a, b| b.cmp(a));
                        clip_indices.dedup();
                        for clip_index in clip_indices {
                            actions.push(Action::RemoveClip {
                                track_name: track_name.clone(),
                                kind: Kind::Audio,
                                clip_indices: vec![clip_index],
                            });
                        }
                    }
                    for (track_name, mut clip_indices) in midi_by_track {
                        clip_indices.sort_unstable_by(|a, b| b.cmp(a));
                        clip_indices.dedup();
                        for clip_index in clip_indices {
                            actions.push(Action::RemoveClip {
                                track_name: track_name.clone(),
                                kind: Kind::MIDI,
                                clip_indices: vec![clip_index],
                            });
                        }
                    }
                    actions.push(Action::EndHistoryGroup);
                    return Self::restore_actions_task(actions);
                }
                let view = self.state.blocking_read().view.clone();
                match view {
                    crate::state::View::Connections => {
                        let state = self.state.blocking_read();
                        let has_folder_or_graph = state.connections_folder.is_some()
                            || state.plugin_graph_track.is_some();
                        drop(state);
                        if !has_folder_or_graph {
                            return self.update(Message::RemoveSelected);
                        }
                        if let Some(task) = self.remove_selected_track_plugin_graph_items() {
                            return task;
                        }
                        return Task::none();
                    }
                    crate::state::View::Workspace => {
                        if let Some(task) = self.remove_selected_track_plugin_graph_items() {
                            return task;
                        }
                        return self.update(Message::RemoveSelectedTracks);
                    }
                    crate::state::View::TrackPlugins => {
                        #[cfg(all(unix, not(target_os = "macos")))]
                        {
                            let (selected_plugins, selected_indices) = {
                                let state = self.state.blocking_read();
                                (
                                    state.plugin_graph_selected_plugins.clone(),
                                    state.plugin_graph_selected_connections.clone(),
                                )
                            };
                            let clip_target = self.state.blocking_read().plugin_graph_clip.clone();
                            if clip_target.is_some() {
                                let mut state = self.state.blocking_write();
                                if let Some(&instance_id) = selected_plugins.iter().next() {
                                    let Some(selected_node) = state
                                        .plugin_graph_plugins
                                        .iter()
                                        .find(|p| p.instance_id == instance_id)
                                        .map(|p| p.node.clone())
                                    else {
                                        return Task::none();
                                    };
                                    state
                                        .plugin_graph_plugins
                                        .retain(|plugin| plugin.instance_id != instance_id);
                                    state.plugin_graph_connections.retain(|connection| {
                                        connection.from_node != selected_node
                                            && connection.to_node != selected_node
                                    });
                                    state.plugin_graph_selected_plugins.clear();
                                    state.plugin_graph_selected_connections.clear();
                                    state.plugin_graph_selected_connectable_connections.clear();
                                    let sync = Self::save_open_clip_plugin_graph(&mut state);
                                    return sync
                                        .map_or_else(Task::none, |action| self.send(action));
                                }
                                let selected = selected_indices.clone();
                                let existing = state.plugin_graph_connections.clone();
                                state.plugin_graph_connections = existing
                                    .into_iter()
                                    .enumerate()
                                    .filter_map(|(idx, connection)| {
                                        (!selected.contains(&idx)).then_some(connection)
                                    })
                                    .collect();
                                state.plugin_graph_selected_connections.clear();
                                state.plugin_graph_selected_plugins.clear();
                                state.plugin_graph_selected_connectable_connections.clear();
                                let sync = Self::save_open_clip_plugin_graph(&mut state);
                                return sync.map_or_else(Task::none, |action| self.send(action));
                            }
                            if let Some(task) = self.remove_selected_track_plugin_graph_items() {
                                return task;
                            }
                        }
                    }
                    crate::state::View::Piano => {
                        return self.update(Message::RemoveSelected);
                    }
                    crate::state::View::JackConnections
                    | crate::state::View::HwInputPorts
                    | crate::state::View::HwOutputPorts => {
                        return Task::none();
                    }
                    crate::state::View::PitchCorrection
                    | crate::state::View::X32
                    | crate::state::View::Session => {
                        return Task::none();
                    }
                }
            }
            Message::TrackResizeStart(ref index) => {
                let mut state = self.state.blocking_write();
                if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *index) {
                    let height = track.height;
                    state.resizing = Some(Resizing::Track(index.clone(), height, state.cursor.y));
                }
            }
            Message::TrackResizeHover(ref track_name, hovered) => {
                let mut state = self.state.blocking_write();
                if hovered {
                    state.hovered_track_resize_handle = Some(track_name.clone());
                } else if state.hovered_track_resize_handle.as_deref() == Some(track_name.as_str())
                {
                    state.hovered_track_resize_handle = None;
                }
            }
            Message::TrackLaneResizeStart {
                ref track_name,
                divider,
            } => {
                let payload = {
                    let state = self.state.blocking_read();
                    state
                        .tracks
                        .iter()
                        .find(|t| t.name == *track_name)
                        .map(|track| {
                            let available = if track.is_folder {
                                track.folder_content_height()
                            } else {
                                track.height
                            };
                            (track.resolved_lane_heights(available), state.cursor.y)
                        })
                };
                if let Some((heights, y)) = payload {
                    self.state.blocking_write().resizing =
                        Some(Resizing::Lane(track_name.clone(), divider, heights, y));
                }
            }
            Message::TrackLaneDividerReset {
                ref track_name,
                divider: _,
            } => {
                let mut state = self.state.blocking_write();
                if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name) {
                    track.reset_lane_heights();
                }
            }
            Message::ShortcutsHint(ref hint) => {
                let mut state = self.state.blocking_write();
                state.shortcuts_hint = hint.clone();
            }
            Message::ClipResizeHandleHover {
                kind,
                ref track_idx,
                clip_idx,
                is_right_side,
                hovered: true,
            } => {
                self.state.blocking_write().hovered_clip_resize_handle =
                    Some((track_idx.clone(), clip_idx, kind, is_right_side));
            }
            Message::ClipResizeHandleHover { hovered: false, .. } => {}
            Message::TracksResizeStart => {
                let (initial_width, initial_mouse_x) = {
                    let state = self.state.blocking_read();
                    let width = match state.tracks_width {
                        Length::Fixed(v) => v,
                        _ => 200.0,
                    };
                    (width, state.cursor.x)
                };
                self.state.blocking_write().resizing =
                    Some(Resizing::Tracks(initial_width, initial_mouse_x));
            }
            Message::LiveViewTracksResizeStart => {
                let (initial_width, initial_mouse_x) = {
                    let state = self.state.blocking_read();
                    let width = match state.live_view_tracks_width {
                        Length::Fixed(v) => v,
                        _ => 200.0,
                    };
                    (width, state.cursor.x)
                };
                self.state.blocking_write().resizing =
                    Some(Resizing::LiveViewTracks(initial_width, initial_mouse_x));
            }
            Message::LiveViewLeftSplitResizeStart => {
                let (initial_split, initial_mouse_x) = {
                    let state = self.state.blocking_read();
                    let width = match state.live_view_tracks_width {
                        Length::Fixed(v) => v,
                        _ => 200.0,
                    };
                    (
                        state.live_view_left_split,
                        width * state.live_view_left_split,
                    )
                };
                self.state.blocking_write().resizing =
                    Some(Resizing::LiveViewLeftSplit(initial_split, initial_mouse_x));
            }
            Message::MixerResizeStart => {
                let (initial_height, initial_mouse_y) = {
                    let state = self.state.blocking_read();
                    let height = match state.mixer_height {
                        Length::Fixed(v) => v,
                        _ => 300.0,
                    };
                    (height, state.cursor.y)
                };
                self.state.blocking_write().resizing =
                    Some(Resizing::Mixer(initial_height, initial_mouse_y));
            }
            Message::MixerLevelEditStart(ref track_name) => {
                let level = {
                    let state = self.state.blocking_read();
                    if track_name == "hw:out" {
                        state.hw_out_level
                    } else {
                        state
                            .tracks
                            .iter()
                            .find(|t| t.name == *track_name)
                            .map(|t| t.level)
                            .unwrap_or(0.0)
                    }
                };
                self.mixer_level_edit_track = Some(track_name.clone());
                self.mixer_level_edit_input = if level <= -90.0 {
                    "-inf".to_string()
                } else {
                    format!("{level:+.1}")
                };
            }
            Message::MixerLevelEditInput(ref value) => {
                self.mixer_level_edit_input = value.clone();
            }
            Message::MixerLevelEditCommit => {
                let Some(track_name) = self.mixer_level_edit_track.clone() else {
                    return Task::none();
                };
                let mut value = self.mixer_level_edit_input.trim().to_ascii_lowercase();
                if value.ends_with("db") {
                    value.truncate(value.len().saturating_sub(2));
                }
                let value = value.trim();
                let parsed = if value == "-inf" || value == "-infinity" {
                    Some(-90.0)
                } else {
                    value.parse::<f32>().ok()
                };
                if let Some(level_db) = parsed {
                    self.mixer_level_edit_track = None;
                    self.mixer_level_edit_input.clear();
                    return self.send(Action::TrackLevel(track_name, level_db.clamp(-90.0, 20.0)));
                }
                self.state.blocking_write().message = "Invalid mixer level value".to_string();
            }
            Message::ClipResizeStart(ref kind, ref track_name, clip_index, is_right_side) => {
                self.clip = None;
                let mut state = self.state.blocking_write();
                let stretch_unavailable = state.shift && !*super::RUBBERBAND_AVAILABLE;
                if stretch_unavailable {
                    state.message = "Clip stretching is unavailable because 'rubberband' is not installed or not on PATH".to_string();
                }
                if let Some(track) = state.tracks.iter().find(|t| t.name == *track_name) {
                    match kind {
                        Kind::Audio => {
                            let Some(clip) = track.audio.clips.get(clip_index) else {
                                return Task::none();
                            };
                            if clip.take_lane_locked {
                                return Task::none();
                            }
                            let stretch_mode = state.shift && *super::RUBBERBAND_AVAILABLE;
                            let clip_start = clip.start;
                            let clip_length = clip.length.max(1);
                            let clip_offset = clip.offset;
                            let initial_value = if is_right_side {
                                clip_length
                            } else {
                                clip_start
                            };
                            state.resizing = Some(Resizing::Clip {
                                kind: *kind,
                                track_name: track_name.clone(),
                                index: clip_index,
                                is_right_side,
                                stretch_mode,
                                initial_value: initial_value as f32,
                                initial_mouse_x: state.cursor.x,
                                initial_length: clip_length as f32,
                                initial_start: clip_start,
                                initial_offset: clip_offset,
                            });
                            if stretch_mode {
                                return self.send(Action::SyncClipBounds {
                                    track_name: track_name.clone(),
                                    clip_index,
                                    kind: *kind,
                                    start: clip_start,
                                    length: clip_length,
                                    offset: clip_offset,
                                });
                            }
                        }
                        Kind::MIDI => {
                            let Some(clip) = track.midi.clips.get(clip_index) else {
                                return Task::none();
                            };
                            if clip.take_lane_locked {
                                return Task::none();
                            }
                            let initial_value = if is_right_side {
                                clip.length
                            } else {
                                clip.start
                            };
                            state.resizing = Some(Resizing::Clip {
                                kind: *kind,
                                track_name: track_name.clone(),
                                index: clip_index,
                                is_right_side,
                                stretch_mode: false,
                                initial_value: initial_value as f32,
                                initial_mouse_x: state.cursor.x,
                                initial_length: clip.length as f32,
                                initial_start: clip.start,
                                initial_offset: clip.offset,
                            });
                        }
                    }
                }
            }
            Message::FadeResizeStart {
                ref kind,
                ref track_idx,
                clip_idx,
                is_fade_out,
            } => {
                if *kind == Kind::MIDI {
                    return Task::none();
                }
                self.clip = None;
                let mut state = self.state.blocking_write();
                if let Some(track) = state.tracks.iter().find(|t| t.name == *track_idx) {
                    let initial_samples = track.audio.clips.get(clip_idx).and_then(|clip| {
                        if clip.take_lane_locked {
                            return None;
                        }
                        if is_fade_out {
                            Some(clip.fade_out_samples)
                        } else {
                            Some(clip.fade_in_samples)
                        }
                    });

                    if let Some(initial_samples) = initial_samples {
                        state.resizing = Some(Resizing::Fade {
                            kind: *kind,
                            track_name: track_idx.clone(),
                            index: clip_idx,
                            is_fade_out,
                            initial_samples,
                            initial_mouse_x: state.cursor.x,
                        });
                    }
                }
            }
            Message::MouseMoved(mouse::Event::CursorMoved { position }) => {
                const TRACK_DRAG_SCROLL_TOP_INSET: f32 = 56.0;
                const TRACK_DRAG_SCROLL_UP_HOTZONE_HEIGHT: f32 = 24.0;
                const TRACK_DRAG_SCROLL_FOOTER_HEIGHT: f32 = 16.0;
                const DRAG_SCROLL_STEP_Y: f32 = 28.0;

                let (resizing, mixer_drag_scroll_top) = {
                    let state = self.state.blocking_read();
                    let bottom_trigger_top = if self.mixer_visible {
                        match state.mixer_height {
                            Length::Fixed(height) => {
                                (self.size.height - height.max(0.0)).clamp(0.0, self.size.height)
                            }
                            _ => self.size.height,
                        }
                    } else {
                        (self.size.height - TRACK_DRAG_SCROLL_FOOTER_HEIGHT)
                            .clamp(0.0, self.size.height)
                    };
                    (state.resizing.clone(), bottom_trigger_top)
                };
                let should_scroll_up = position.y >= TRACK_DRAG_SCROLL_TOP_INSET
                    && position.y
                        <= TRACK_DRAG_SCROLL_TOP_INSET + TRACK_DRAG_SCROLL_UP_HOTZONE_HEIGHT;
                let should_scroll_down = position.y >= mixer_drag_scroll_top;
                let previous_cursor = {
                    let mut state = self.state.blocking_write();
                    let prev = state.cursor;
                    state.cursor = position;

                    if let Some(Resizing::Track(ref track_name, initial_height, initial_mouse_y)) =
                        resizing
                    {
                        let delta = position.y - initial_mouse_y;
                        if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name)
                        {
                            let min_h = track.min_height_for_layout();
                            let new_height = (initial_height + delta).clamp(min_h, 600.0);

                            if (track.height - new_height).abs() >= 0.5 {
                                track.height = new_height;
                                track.scale_lane_heights_to(new_height);
                            }
                        }
                    }
                    prev
                };
                match resizing {
                    Some(Resizing::Track(..)) => {}
                    Some(Resizing::Lane(
                        ref track_name,
                        divider,
                        ref initial_heights,
                        initial_mouse_y,
                    )) => {
                        let dy = position.y - initial_mouse_y;
                        let mut state = self.state.blocking_write();
                        if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name)
                        {
                            track.apply_lane_divider_drag_from(divider, initial_heights, dy);
                        }
                    }
                    Some(Resizing::Clip {
                        kind,
                        ref track_name,
                        index,
                        is_right_side,
                        stretch_mode,
                        initial_value,
                        initial_mouse_x,
                        initial_length,
                        initial_start,
                        initial_offset,
                    }) => {
                        let pixels_per_sample = self.pixels_per_sample().max(1.0e-6);
                        let samples_per_beat = self.samples_per_beat();
                        let samples_per_bar = self.samples_per_bar();
                        let snap_interval_samples = self
                            .snap_mode
                            .interval_samples(samples_per_beat, samples_per_bar)
                            .max(1.0) as f32;
                        let snap_sample_drag = |sample: f32, delta_samples: f32| {
                            self.snap_mode.snap_sample_drag(
                                sample as f64,
                                delta_samples as f64,
                                samples_per_beat,
                                samples_per_bar,
                            ) as f32
                        };
                        let min_length_samples = (MIN_CLIP_WIDTH_PX / pixels_per_sample)
                            .ceil()
                            .max(snap_interval_samples)
                            .max(1.0);
                        let clip_edge_snap_threshold_samples =
                            self.clip_edge_snap_threshold_samples();
                        let clip_edge_snap_enabled = self.clip_edge_snap_enabled();
                        let resize_excluded_clip = crate::state::ClipId {
                            track_idx: track_name.clone(),
                            clip_idx: index,
                            kind,
                        };
                        let candidate_edges = if clip_edge_snap_enabled {
                            self.clip_snap_edges(&[resize_excluded_clip])
                        } else {
                            Vec::new()
                        };
                        let mut clip_snap_targets = Vec::new();
                        let mut state = self.state.blocking_write();
                        if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name)
                        {
                            let delta_samples = (position.x - initial_mouse_x) / pixels_per_sample;
                            match kind {
                                Kind::Audio => {
                                    let clip = &mut track.audio.clips[index];
                                    if stretch_mode {
                                        if is_right_side {
                                            let raw_end =
                                                clip.start as f32 + initial_value + delta_samples;
                                            let (snapped_end, _snap_target, snap_targets) =
                                                Self::nearest_clip_edge_sample(
                                                    raw_end,
                                                    snap_sample_drag(raw_end, delta_samples),
                                                    clip_edge_snap_threshold_samples,
                                                    candidate_edges.iter().cloned(),
                                                );
                                            clip_snap_targets = snap_targets;
                                            let min_end = clip.start as f32 + min_length_samples;
                                            let updated_end = snapped_end.max(min_end);
                                            clip.length = updated_end.max(clip.start as f32)
                                                as usize
                                                - clip.start;
                                        } else {
                                            let right_edge = initial_start as f32 + initial_length;
                                            let max_start =
                                                (right_edge - min_length_samples).max(0.0);
                                            let raw_start = initial_value + delta_samples;
                                            let (snapped_start, _snap_target, snap_targets) =
                                                Self::nearest_clip_edge_sample(
                                                    raw_start,
                                                    snap_sample_drag(raw_start, delta_samples),
                                                    clip_edge_snap_threshold_samples,
                                                    candidate_edges.iter().cloned(),
                                                );
                                            clip_snap_targets = snap_targets;
                                            let new_start = snapped_start.clamp(0.0, max_start);
                                            let updated_length =
                                                (right_edge - new_start).max(min_length_samples);
                                            clip.start = new_start as usize;
                                            clip.length = updated_length as usize;
                                            clip.offset = initial_offset;
                                        }
                                    } else {
                                        let max_length_samples =
                                            clip.max_length_samples.max(initial_length as usize)
                                                as f32;
                                        let max_length_samples =
                                            max_length_samples.max(min_length_samples);
                                        if is_right_side {
                                            let raw_end =
                                                clip.start as f32 + initial_value + delta_samples;
                                            let (snapped_end, _snap_target, snap_targets) =
                                                Self::nearest_clip_edge_sample(
                                                    raw_end,
                                                    snap_sample_drag(raw_end, delta_samples),
                                                    clip_edge_snap_threshold_samples,
                                                    candidate_edges.iter().cloned(),
                                                );
                                            clip_snap_targets = snap_targets;
                                            let min_end = clip.start as f32 + min_length_samples;
                                            let max_end = clip.start as f32 + max_length_samples;
                                            let updated_end = snapped_end.clamp(min_end, max_end);
                                            clip.length = updated_end.max(clip.start as f32)
                                                as usize
                                                - clip.start;
                                        } else {
                                            let right_edge = initial_value + initial_length;
                                            let max_start =
                                                (right_edge - min_length_samples).max(0.0);
                                            let min_start =
                                                (right_edge - max_length_samples).max(0.0);
                                            let raw_start = initial_value + delta_samples;
                                            let (snapped_start, _snap_target, snap_targets) =
                                                Self::nearest_clip_edge_sample(
                                                    raw_start,
                                                    snap_sample_drag(raw_start, delta_samples),
                                                    clip_edge_snap_threshold_samples,
                                                    candidate_edges.iter().cloned(),
                                                );
                                            clip_snap_targets = snap_targets;
                                            let new_start =
                                                snapped_start.clamp(min_start, max_start);
                                            let updated_length = (right_edge - new_start)
                                                .clamp(min_length_samples, max_length_samples);
                                            let start_delta =
                                                new_start as isize - clip.start as isize;
                                            clip.start = new_start as usize;
                                            clip.length = updated_length as usize;
                                            if start_delta >= 0 {
                                                clip.offset = (clip.offset + start_delta as usize)
                                                    .min(
                                                        clip.max_length_samples
                                                            .saturating_sub(clip.length),
                                                    );
                                            } else {
                                                clip.offset = clip
                                                    .offset
                                                    .saturating_sub((-start_delta) as usize);
                                            }
                                        }
                                    }
                                }
                                Kind::MIDI => {
                                    let clip = &mut track.midi.clips[index];
                                    let max_length_samples =
                                        clip.max_length_samples.max(initial_length as usize) as f32;
                                    let max_length_samples =
                                        max_length_samples.max(min_length_samples);
                                    if is_right_side {
                                        let raw_end =
                                            clip.start as f32 + initial_value + delta_samples;
                                        let (snapped_end, _snap_target, snap_targets) =
                                            Self::nearest_clip_edge_sample(
                                                raw_end,
                                                snap_sample_drag(raw_end, delta_samples),
                                                clip_edge_snap_threshold_samples,
                                                candidate_edges.iter().cloned(),
                                            );
                                        clip_snap_targets = snap_targets;
                                        let min_end = clip.start as f32 + min_length_samples;
                                        let max_end = clip.start as f32 + max_length_samples;
                                        let updated_end = snapped_end.clamp(min_end, max_end);
                                        clip.length = updated_end.max(clip.start as f32) as usize
                                            - clip.start;
                                    } else {
                                        let right_edge = initial_value + initial_length;
                                        let max_start = (right_edge - min_length_samples).max(0.0);
                                        let min_start = (right_edge - max_length_samples).max(0.0);
                                        let raw_start = initial_value + delta_samples;
                                        let (snapped_start, _snap_target, snap_targets) =
                                            Self::nearest_clip_edge_sample(
                                                raw_start,
                                                snap_sample_drag(raw_start, delta_samples),
                                                clip_edge_snap_threshold_samples,
                                                candidate_edges.iter().cloned(),
                                            );
                                        clip_snap_targets = snap_targets;
                                        let new_start = snapped_start.clamp(min_start, max_start);
                                        let updated_length = (right_edge - new_start)
                                            .clamp(min_length_samples, max_length_samples);
                                        let start_delta = new_start as isize - clip.start as isize;
                                        clip.start = new_start as usize;
                                        clip.length = updated_length as usize;
                                        if start_delta >= 0 {
                                            clip.offset = (clip.offset + start_delta as usize).min(
                                                clip.max_length_samples.saturating_sub(clip.length),
                                            );
                                        } else {
                                            clip.offset =
                                                clip.offset.saturating_sub((-start_delta) as usize);
                                        }
                                    }
                                }
                            }
                        }
                        self.clip_snap_targets = clip_snap_targets;
                    }
                    Some(Resizing::Tracks(initial_width, initial_mouse_x)) => {
                        let delta = position.x - initial_mouse_x;
                        self.state.blocking_write().tracks_width =
                            Length::Fixed((initial_width + delta).max(80.0));
                    }
                    Some(Resizing::LiveViewTracks(initial_width, initial_mouse_x)) => {
                        let delta = position.x - initial_mouse_x;
                        self.state.blocking_write().live_view_tracks_width =
                            Length::Fixed((initial_width + delta).max(80.0));
                    }
                    Some(Resizing::LiveViewLeftSplit(initial_split, initial_mouse_x)) => {
                        let total_width = match self.state.blocking_read().live_view_tracks_width {
                            Length::Fixed(v) => v,
                            _ => 200.0,
                        };
                        let delta = position.x - initial_mouse_x;
                        let new_split_px = (initial_split * total_width + delta)
                            .clamp(total_width * 0.1, total_width * 0.9);
                        self.state.blocking_write().live_view_left_split =
                            new_split_px / total_width;
                    }
                    Some(Resizing::Mixer(initial_height, initial_mouse_y)) => {
                        let delta = position.y - initial_mouse_y;
                        self.state.blocking_write().mixer_height =
                            Length::Fixed((initial_height - delta).max(60.0));
                    }
                    Some(Resizing::Fade {
                        kind,
                        ref track_name,
                        index,
                        is_fade_out,
                        initial_samples,
                        initial_mouse_x,
                    }) => {
                        let pixels_per_sample = self.pixels_per_sample().max(1.0e-6);
                        let mut state = self.state.blocking_write();
                        if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name)
                        {
                            let delta_samples = if is_fade_out {
                                (initial_mouse_x - position.x) / pixels_per_sample
                            } else {
                                (position.x - initial_mouse_x) / pixels_per_sample
                            };
                            let new_fade_samples =
                                ((initial_samples as f32 + delta_samples).max(0.0) as usize)
                                    .min(96000);

                            match kind {
                                Kind::Audio => {
                                    if let Some(clip) = track.audio.clips.get_mut(index) {
                                        let max_fade = clip.length / 2;
                                        if is_fade_out {
                                            clip.fade_out_samples = new_fade_samples.min(max_fade);
                                        } else {
                                            clip.fade_in_samples = new_fade_samples.min(max_fade);
                                        }
                                    }
                                }
                                Kind::MIDI => {}
                            }
                        }
                    }
                    _ => {}
                }
                let mouse_left_down = self.state.blocking_read().mouse_left_down;
                if mouse_left_down && !matches!(resizing, Some(Resizing::Clip { .. })) {
                    if let Some(active) = self.clip.as_mut() {
                        active.end = position;
                        let mut tasks = vec![iced_drop::zones_on_point(
                            Message::HandleClipPreviewZones,
                            position,
                            None,
                            None,
                        )];
                        if should_scroll_up {
                            tasks.push(operation::scroll_by(
                                Id::new(EDITOR_SCROLL_ID),
                                operation::AbsoluteOffset {
                                    x: 0.0,
                                    y: -DRAG_SCROLL_STEP_Y,
                                },
                            ));
                            tasks.push(operation::scroll_by(
                                Id::new(TRACKS_SCROLL_ID),
                                operation::AbsoluteOffset {
                                    x: 0.0,
                                    y: -DRAG_SCROLL_STEP_Y,
                                },
                            ));
                        } else if should_scroll_down {
                            tasks.push(operation::scroll_by(
                                Id::new(EDITOR_SCROLL_ID),
                                operation::AbsoluteOffset {
                                    x: 0.0,
                                    y: DRAG_SCROLL_STEP_Y,
                                },
                            ));
                            tasks.push(operation::scroll_by(
                                Id::new(TRACKS_SCROLL_ID),
                                operation::AbsoluteOffset {
                                    x: 0.0,
                                    y: DRAG_SCROLL_STEP_Y,
                                },
                            ));
                        }
                        return Task::batch(tasks);
                    }
                    let mut state = self.state.blocking_write();
                    if state.clip_marquee_start.is_some()
                        && self.clip.is_none()
                        && !state.clip_click_consumed
                        && matches!(state.view, View::Workspace)
                        && self.modal.is_none()
                    {
                        let end = state.clip_marquee_end.unwrap_or(Point::new(0.0, 0.0));
                        let dx = position.x - previous_cursor.x;
                        let dy = position.y - previous_cursor.y;
                        state.clip_marquee_end =
                            Some(Point::new((end.x + dx).max(0.0), (end.y + dy).max(0.0)));
                    }
                }
                let mouse_right_down = self.state.blocking_read().mouse_right_down;
                if mouse_right_down
                    && !matches!(resizing, Some(Resizing::Clip { .. }))
                    && self.clip.is_none()
                    && matches!(self.state.blocking_read().view, View::Workspace)
                    && self.modal.is_none()
                {
                    let can_start = self.midi_lane_at_position(position).is_some();
                    let mut state = self.state.blocking_write();
                    if state.midi_clip_create_start.is_none() && can_start {
                        state.midi_clip_create_start = Some(position);
                        state.midi_clip_create_end = Some(position);
                    } else if state.midi_clip_create_start.is_some() {
                        let end = state.midi_clip_create_end.unwrap_or(position);
                        let dx = position.x - previous_cursor.x;
                        let dy = position.y - previous_cursor.y;
                        state.midi_clip_create_end =
                            Some(Point::new((end.x + dx).max(0.0), (end.y + dy).max(0.0)));
                    }
                }
                if self.track.is_some() && (should_scroll_up || should_scroll_down) {
                    let delta_y = if should_scroll_up {
                        -DRAG_SCROLL_STEP_Y
                    } else {
                        DRAG_SCROLL_STEP_Y
                    };
                    return Task::batch(vec![
                        operation::scroll_by(
                            Id::new(EDITOR_SCROLL_ID),
                            operation::AbsoluteOffset { x: 0.0, y: delta_y },
                        ),
                        operation::scroll_by(
                            Id::new(TRACKS_SCROLL_ID),
                            operation::AbsoluteOffset { x: 0.0, y: delta_y },
                        ),
                    ]);
                }
            }
            Message::EditorMouseMoved(position) => {
                let resizing = self.state.blocking_read().resizing.clone();
                let can_start_midi_drag = self.midi_lane_at_position(position).is_some();
                let hovered_resize_handle = self.clip_resize_handle_at_position(position);
                let cut_preview_active = self.state.blocking_read().cut_preview_active;
                let mut state = self.state.blocking_write();
                state.editor_cursor = Some(position);
                state.hovered_clip_resize_handle = hovered_resize_handle;
                if state.mouse_left_down
                    && !matches!(resizing, Some(Resizing::Clip { .. }))
                    && self.clip.is_none()
                    && !state.clip_click_consumed
                    && matches!(state.view, View::Workspace)
                    && self.modal.is_none()
                    && state.clip_marquee_start.is_none()
                {
                    state.clip_marquee_start = Some(position);
                    state.clip_marquee_end = Some(position);
                }
                if state.mouse_right_down
                    && !matches!(resizing, Some(Resizing::Clip { .. }))
                    && self.clip.is_none()
                    && matches!(state.view, View::Workspace)
                    && self.modal.is_none()
                {
                    if state.midi_clip_create_start.is_none() && can_start_midi_drag {
                        state.midi_clip_create_start = Some(position);
                        state.midi_clip_create_end = Some(position);
                    } else if state.midi_clip_create_start.is_some() {
                        state.midi_clip_create_end = Some(position);
                    }
                }
                drop(state);
                if cut_preview_active && matches!(self.state.blocking_read().view, View::Workspace)
                {
                    self.update_cut_indicator(position);
                }
            }
            Message::MouseReleased => {
                let active = std::mem::take(&mut self.touch_active_keys);
                for (track_name, keys) in active {
                    if let Some(values) = self.touch_automation_overrides.get_mut(&track_name) {
                        for key in keys {
                            values.remove(&key);
                        }
                        if values.is_empty() {
                            self.touch_automation_overrides.remove(&track_name);
                        }
                    }
                }
                if self.modal.is_some() {
                    let mut state = self.state.blocking_write();
                    state.mouse_left_down = false;
                    state.mouse_right_down = false;
                    state.clip_click_consumed = false;
                    state.clip_marquee_start = None;
                    state.clip_marquee_end = None;
                    state.midi_clip_create_start = None;
                    state.midi_clip_create_end = None;
                    self.clip = None;
                    self.clip_preview_target_track = None;
                    self.clip_preview_target_valid = false;
                    self.clip_preview_snap_adjust_samples = 0.0;
                    self.clip_snap_targets.clear();
                    return Task::none();
                }
                let (resizing, marquee_start, marquee_end, create_start, create_end) = {
                    let mut state = self.state.blocking_write();
                    state.mouse_left_down = false;
                    state.mouse_right_down = false;
                    state.clip_click_consumed = false;
                    state.session_slot_context_menu = None;
                    let resizing = state.resizing.clone();
                    let marquee_start = state.clip_marquee_start.take();
                    let marquee_end = state.clip_marquee_end.take();
                    let create_start = state.midi_clip_create_start.take();
                    let create_end = state.midi_clip_create_end.take();
                    state.resizing = None;
                    (
                        resizing,
                        marquee_start,
                        marquee_end,
                        create_start,
                        create_end,
                    )
                };
                self.clip_preview_snap_adjust_samples = 0.0;
                self.clip_snap_targets.clear();
                if let Some(Resizing::Clip {
                    kind,
                    track_name,
                    index,
                    stretch_mode,
                    initial_start,
                    initial_length,
                    initial_offset,
                    ..
                }) = resizing
                {
                    let state = self.state.blocking_read();
                    if let Some(track) = state.tracks.iter().find(|t| t.name == track_name) {
                        let (start, length, offset) = match kind {
                            Kind::Audio => {
                                if let Some(clip) = track.audio.clips.get(index) {
                                    (clip.start, clip.length, clip.offset)
                                } else {
                                    return Task::none();
                                }
                            }
                            Kind::MIDI => {
                                if let Some(clip) = track.midi.clips.get(index) {
                                    (clip.start, clip.length, clip.offset)
                                } else {
                                    return Task::none();
                                }
                            }
                        };
                        drop(state);
                        if stretch_mode && kind == Kind::Audio {
                            let stretch_ratio = length as f32 / initial_length.max(1.0);
                            let stretch_request = {
                                let state = self.state.blocking_read();
                                state
                                    .tracks
                                    .iter()
                                    .find(|t| t.name == track_name)
                                    .and_then(|track| track.audio.clips.get(index))
                                    .map(|clip| crate::message::ClipStretchRequest {
                                        track_idx: track_name.clone(),
                                        clip_idx: index,
                                        clip_name: clip.name.clone(),
                                        start,
                                        original_start: initial_start,
                                        length: initial_length.max(1.0) as usize,
                                        offset: initial_offset,
                                        input_channel: clip.input_channel,
                                        muted: clip.muted,
                                        fade_enabled: clip.fade_enabled,
                                        fade_in_samples: clip.fade_in_samples,
                                        fade_out_samples: clip.fade_out_samples,
                                        stretch_ratio,
                                    })
                            };
                            if let Some(request) = stretch_request {
                                return self.start_clip_stretch_request(request);
                            }
                            return Task::none();
                        }
                        return self.send(Action::SetClipBounds {
                            track_name,
                            clip_index: index,
                            kind,
                            start,
                            length,
                            offset,
                        });
                    }
                    return Task::none();
                }
                if let Some(Resizing::Fade {
                    kind,
                    track_name,
                    index,
                    ..
                }) = resizing
                {
                    if kind == Kind::MIDI {
                        return Task::none();
                    }

                    let state = self.state.blocking_read();
                    if let Some(track) = state.tracks.iter().find(|t| t.name == track_name) {
                        let (fade_enabled, fade_in_samples, fade_out_samples) =
                            if let Some(clip) = track.audio.clips.get(index) {
                                (
                                    clip.fade_enabled,
                                    clip.fade_in_samples,
                                    clip.fade_out_samples,
                                )
                            } else {
                                return Task::none();
                            };
                        return self.send(Action::SetClipFade {
                            track_name,
                            clip_index: index,
                            kind,
                            fade_enabled,
                            fade_in_samples,
                            fade_out_samples,
                        });
                    }
                    return Task::none();
                }
                if let (Some(start), Some(end)) = (create_start, create_end) {
                    let w = (start.x - end.x).abs();
                    let h = (start.y - end.y).abs();
                    if w > 2.0 || h > 2.0 {
                        return self.create_empty_midi_clip_from_drag(start, end);
                    }
                }
                if let (Some(start), Some(end)) = (marquee_start, marquee_end) {
                    let mut x = start.x.min(end.x);
                    let mut y = start.y.min(end.y);
                    let mut w = (start.x - end.x).abs();
                    let mut h = (start.y - end.y).abs();
                    if w > 2.0 || h > 2.0 {
                        w = w.max(2.0);
                        h = h.max(2.0);
                        x = x.max(0.0);
                        y = y.max(0.0);
                        let pps = self.pixels_per_sample().max(1.0e-6);
                        let mut y_offset = 0.0f32;
                        let mut selected = std::collections::HashSet::new();
                        let state = self.state.blocking_read();
                        for track in state
                            .tracks
                            .iter()
                            .filter(|track| track.name != METRONOME_TRACK_ID)
                        {
                            let layout = track.lane_layout();
                            for (clip_idx, clip) in track.audio.clips.iter().enumerate() {
                                let cx = clip.start as f32 * pps;
                                let cw = (clip.length as f32 * pps).max(12.0);
                                let lane =
                                    clip.input_channel.min(track.audio.ins.saturating_sub(1));
                                let cy = y_offset + track.lane_top(Kind::Audio, lane);
                                let ch = layout.lane_height_for(Kind::Audio, lane).max(1.0);
                                let intersects =
                                    cx < x + w && cx + cw > x && cy < y + h && cy + ch > y;
                                if intersects {
                                    selected.insert(crate::state::ClipId {
                                        track_idx: track.name.clone(),
                                        clip_idx,
                                        kind: Kind::Audio,
                                    });
                                }
                            }
                            for (clip_idx, clip) in track.midi.clips.iter().enumerate() {
                                let cx = clip.start as f32 * pps;
                                let cw = (clip.length as f32 * pps).max(12.0);
                                let lane = clip.input_channel.min(track.midi.ins.saturating_sub(1));
                                let cy = y_offset + track.lane_top(Kind::MIDI, lane);
                                let ch = layout.lane_height_for(Kind::MIDI, lane).max(1.0);
                                let intersects =
                                    cx < x + w && cx + cw > x && cy < y + h && cy + ch > y;
                                if intersects {
                                    selected.insert(crate::state::ClipId {
                                        track_idx: track.name.clone(),
                                        clip_idx,
                                        kind: Kind::MIDI,
                                    });
                                }
                            }
                            y_offset += track.height;
                        }
                        drop(state);
                        self.state.blocking_write().selected_clips = selected;
                        return Task::none();
                    }
                }
                if let Some(clip) = &mut self.clip {
                    let moved = (clip.end.x - clip.start.x).abs() > 2.0
                        || (clip.end.y - clip.start.y).abs() > 2.0;
                    if !moved {
                        self.clip = None;
                        self.clip_preview_target_valid = false;
                        return Task::none();
                    }
                    return iced_drop::zones_on_point(
                        Message::HandleClipZones,
                        clip.end,
                        None,
                        None,
                    );
                }
                self.clip_preview_target_track = None;
                self.clip_preview_target_valid = false;
            }
            Message::ClipDrag(ref clip) => {
                if !self.state.blocking_read().mouse_left_down {
                    return Task::none();
                }
                if self.state.blocking_read().clip_marquee_start.is_some() {
                    return Task::none();
                }
                if matches!(
                    self.state.blocking_read().resizing,
                    Some(Resizing::Clip { .. })
                ) {
                    return Task::none();
                }
                match &mut self.clip {
                    Some(active)
                        if active.kind == clip.kind
                            && active.index == clip.index
                            && active.track_index == clip.track_index =>
                    {
                        active.end = self.state.blocking_read().cursor;
                    }
                    Some(_) => {}
                    None => {
                        let mut dragged = clip.clone();
                        let cursor = self.state.blocking_read().cursor;
                        dragged.start = cursor;
                        dragged.end = cursor;
                        dragged.copy = self.state.blocking_read().ctrl;
                        self.clip = Some(dragged);
                        self.clip_preview_snap_adjust_samples = 0.0;
                        self.clip_snap_targets.clear();
                    }
                }
            }
            Message::HandleClipZones(ref zones) => {
                if let Some(clip) = &self.clip {
                    let state = self.state.blocking_read();
                    let from_track_name = &clip.track_index;
                    let to_track_zone = zones.iter().find(|(id, _)| {
                        state.tracks.iter().any(|t| Id::from(t.name.clone()) == *id)
                    });
                    let Some((to_track_id, to_track_rect)) = to_track_zone else {
                        self.clip = None;
                        self.clip_preview_target_valid = false;
                        return Task::none();
                    };

                    let from_track_option =
                        state.tracks.iter().find(|t| t.name == *from_track_name);
                    let to_track_option = state
                        .tracks
                        .iter()
                        .find(|t| Id::from(t.name.clone()) == *to_track_id);

                    if let (Some(from_track), Some(to_track)) = (from_track_option, to_track_option)
                    {
                        let kind_matches = match clip.kind {
                            Kind::Audio => {
                                !to_track.is_folder
                                    && to_track.audio.ins > 0
                                    && from_track.audio.ins == to_track.audio.ins
                            }
                            Kind::MIDI => !to_track.is_folder && to_track.midi.ins > 0,
                        };
                        if !kind_matches {
                            self.clip = None;
                            self.clip_preview_target_track = None;
                            self.clip_preview_target_valid = false;
                            self.clip_preview_snap_adjust_samples = 0.0;
                            self.clip_snap_targets.clear();
                            return Task::none();
                        }
                        let local_y = (clip.end.y - to_track_rect.y).max(0.0);
                        let target_input_channel = to_track.lane_index_at_y(clip.kind, local_y);
                        let mut selected_group: Vec<usize> = state
                            .selected_clips
                            .iter()
                            .filter(|id| id.kind == clip.kind && id.track_idx == from_track.name)
                            .map(|id| id.clip_idx)
                            .collect();
                        selected_group.sort_unstable();
                        selected_group.dedup();
                        let group_drag_active =
                            selected_group.len() > 1 && selected_group.contains(&clip.index);

                        let clip_index = clip.index;
                        match clip.kind {
                            Kind::Audio => {
                                let offset = (clip.end.x - clip.start.x)
                                    / self.pixels_per_sample().max(1.0e-6);
                                let (snap_adjust, _snap_target, snap_targets) = self
                                    .move_clip_snap_adjust_and_target(MoveClipSnapArgs {
                                        kind: clip.kind,
                                        from_track_name: &from_track.name,
                                        clip_index: clip.index,
                                        offset,
                                        group_drag_active,
                                        selected_group: &selected_group,
                                        copy: clip.copy,
                                    });
                                self.clip_snap_targets = snap_targets;
                                if group_drag_active {
                                    let mut indices = selected_group.clone();
                                    if !clip.copy {
                                        indices.sort_unstable_by(|a, b| b.cmp(a));
                                    }
                                    let mut tasks = Vec::new();
                                    for idx in indices {
                                        if idx >= from_track.audio.clips.len() {
                                            continue;
                                        }
                                        let source = &from_track.audio.clips[idx];
                                        let sample_offset =
                                            (source.start as f32 + offset + snap_adjust)
                                                .max(0.0)
                                                .round()
                                                as usize;
                                        tasks.push(self.send(Action::ClipMove {
                                            kind: clip.kind,
                                            from: ClipMoveFrom {
                                                track_name: from_track.name.clone(),
                                                clip_index: idx,
                                            },
                                            to: ClipMoveTo {
                                                track_name: to_track.name.clone(),
                                                sample_offset,
                                                input_channel: target_input_channel,
                                            },
                                            copy: clip.copy,
                                        }));
                                    }
                                    self.clip = None;
                                    self.clip_preview_target_track = None;
                                    self.clip_preview_target_valid = false;
                                    self.clip_preview_snap_adjust_samples = 0.0;
                                    self.clip_snap_targets.clear();
                                    return Task::batch(tasks);
                                }
                                if clip_index >= from_track.audio.clips.len() {
                                    self.clip = None;
                                    self.clip_preview_target_valid = false;
                                    self.clip_preview_snap_adjust_samples = 0.0;
                                    self.clip_snap_targets.clear();
                                    return Task::none();
                                }
                                let clip_index_in_from_track = clip_index;
                                let mut clip_copy =
                                    from_track.audio.clips[clip_index_in_from_track].clone();
                                clip_copy.start = (clip_copy.start as f32 + offset + snap_adjust)
                                    .max(0.0)
                                    .round()
                                    as usize;
                                let task = self.send(Action::ClipMove {
                                    kind: clip.kind,
                                    from: ClipMoveFrom {
                                        track_name: from_track.name.clone(),
                                        clip_index: clip.index,
                                    },
                                    to: ClipMoveTo {
                                        track_name: to_track.name.clone(),
                                        sample_offset: clip_copy.start,
                                        input_channel: target_input_channel,
                                    },
                                    copy: clip.copy,
                                });
                                self.clip = None;
                                self.clip_preview_target_track = None;
                                self.clip_preview_target_valid = false;
                                self.clip_preview_snap_adjust_samples = 0.0;
                                self.clip_snap_targets.clear();
                                return task;
                            }
                            Kind::MIDI => {
                                let offset = (clip.end.x - clip.start.x)
                                    / self.pixels_per_sample().max(1.0e-6);
                                let (snap_adjust, _snap_target, snap_targets) = self
                                    .move_clip_snap_adjust_and_target(MoveClipSnapArgs {
                                        kind: clip.kind,
                                        from_track_name: &from_track.name,
                                        clip_index: clip.index,
                                        offset,
                                        group_drag_active,
                                        selected_group: &selected_group,
                                        copy: clip.copy,
                                    });
                                self.clip_snap_targets = snap_targets;
                                if group_drag_active {
                                    let mut indices = selected_group.clone();
                                    if !clip.copy {
                                        indices.sort_unstable_by(|a, b| b.cmp(a));
                                    }
                                    let mut tasks = Vec::new();
                                    for idx in indices {
                                        if idx >= from_track.midi.clips.len() {
                                            continue;
                                        }
                                        let source = &from_track.midi.clips[idx];
                                        let sample_offset =
                                            (source.start as f32 + offset + snap_adjust)
                                                .max(0.0)
                                                .round()
                                                as usize;
                                        tasks.push(self.send(Action::ClipMove {
                                            kind: clip.kind,
                                            from: ClipMoveFrom {
                                                track_name: from_track.name.clone(),
                                                clip_index: idx,
                                            },
                                            to: ClipMoveTo {
                                                track_name: to_track.name.clone(),
                                                sample_offset,
                                                input_channel: target_input_channel,
                                            },
                                            copy: clip.copy,
                                        }));
                                    }
                                    self.clip = None;
                                    self.clip_preview_target_track = None;
                                    self.clip_preview_target_valid = false;
                                    self.clip_preview_snap_adjust_samples = 0.0;
                                    self.clip_snap_targets.clear();
                                    return Task::batch(tasks);
                                }
                                if clip_index >= from_track.midi.clips.len() {
                                    self.clip = None;
                                    self.clip_preview_target_valid = false;
                                    self.clip_preview_snap_adjust_samples = 0.0;
                                    self.clip_snap_targets.clear();
                                    return Task::none();
                                }
                                let clip_index_in_from_track = clip_index;
                                let mut clip_copy =
                                    from_track.midi.clips[clip_index_in_from_track].clone();
                                clip_copy.start = (clip_copy.start as f32 + offset + snap_adjust)
                                    .max(0.0)
                                    .round()
                                    as usize;
                                let task = self.send(Action::ClipMove {
                                    kind: clip.kind,
                                    from: ClipMoveFrom {
                                        track_name: from_track.name.clone(),
                                        clip_index: clip.index,
                                    },
                                    to: ClipMoveTo {
                                        track_name: to_track.name.clone(),
                                        sample_offset: clip_copy.start,
                                        input_channel: target_input_channel,
                                    },
                                    copy: clip.copy,
                                });
                                self.clip = None;
                                self.clip_preview_target_track = None;
                                self.clip_preview_target_valid = false;
                                self.clip_preview_snap_adjust_samples = 0.0;
                                self.clip_snap_targets.clear();
                                return task;
                            }
                        }
                    }
                }
                self.clip = None;
                self.clip_preview_target_track = None;
                self.clip_preview_target_valid = false;
                self.clip_preview_snap_adjust_samples = 0.0;
                self.clip_snap_targets.clear();
                return Task::none();
            }
            Message::HandleClipPreviewZones(ref zones) => {
                if let Some(clip) = &self.clip {
                    let state = self.state.blocking_read();
                    let from_track = state.tracks.iter().find(|t| t.name == clip.track_index);
                    let mut track_zone_ids = zones.iter().filter_map(|(id, _)| {
                        state
                            .tracks
                            .iter()
                            .find(|t| Id::from(t.name.clone()) == *id)
                            .map(|track| (id, track.name.as_str()))
                    });
                    let to_track_id = track_zone_ids
                        .clone()
                        .find(|(_, track_name)| *track_name != clip.track_index.as_str())
                        .map(|(id, _)| id)
                        .or_else(|| track_zone_ids.next().map(|(id, _)| id));
                    let Some(to_track_id) = to_track_id else {
                        self.clip_preview_target_track = None;
                        self.clip_preview_target_valid = false;
                        self.clip_preview_snap_adjust_samples = 0.0;
                        self.clip_snap_targets.clear();
                        return Task::none();
                    };
                    let to_track = state
                        .tracks
                        .iter()
                        .find(|t| Id::from(t.name.clone()) == *to_track_id);
                    if let Some(to_track) = to_track {
                        let kind_matches = match clip.kind {
                            Kind::Audio => {
                                if let Some(from_track) = from_track {
                                    to_track.audio.ins > 0
                                        && from_track.audio.ins == to_track.audio.ins
                                } else {
                                    false
                                }
                            }
                            Kind::MIDI => to_track.midi.ins > 0,
                        };
                        if kind_matches {
                            self.clip_preview_target_track = Some(to_track.name.clone());
                            self.clip_preview_target_valid = true;
                            let mut selected_group: Vec<usize> = state
                                .selected_clips
                                .iter()
                                .filter(|id| {
                                    id.kind == clip.kind && id.track_idx == clip.track_index
                                })
                                .map(|id| id.clip_idx)
                                .collect();
                            selected_group.sort_unstable();
                            selected_group.dedup();
                            let group_drag_active =
                                selected_group.len() > 1 && selected_group.contains(&clip.index);
                            let offset =
                                (clip.end.x - clip.start.x) / self.pixels_per_sample().max(1.0e-6);
                            let (snap_adjust, _snap_target, snap_targets) = self
                                .move_clip_snap_adjust_and_target(MoveClipSnapArgs {
                                    kind: clip.kind,
                                    from_track_name: &clip.track_index,
                                    clip_index: clip.index,
                                    offset,
                                    group_drag_active,
                                    selected_group: &selected_group,
                                    copy: clip.copy,
                                });
                            self.clip_preview_snap_adjust_samples = snap_adjust;
                            self.clip_snap_targets = snap_targets;
                        } else {
                            self.clip_preview_target_track = Some(to_track.name.clone());
                            self.clip_preview_target_valid = false;
                            self.clip_preview_snap_adjust_samples = 0.0;
                            self.clip_snap_targets.clear();
                        }
                    } else {
                        self.clip_preview_target_track = None;
                        self.clip_preview_target_valid = false;
                        self.clip_preview_snap_adjust_samples = 0.0;
                        self.clip_snap_targets.clear();
                    }
                } else {
                    self.clip_preview_target_track = None;
                    self.clip_preview_target_valid = false;
                    self.clip_preview_snap_adjust_samples = 0.0;
                    self.clip_snap_targets.clear();
                }
            }
            Message::TrackDrag { index, position } => {
                const TRACK_DRAG_SCROLL_UP_HOTZONE_HEIGHT: f32 = 24.0;

                let state = self.state.blocking_read();
                if index < state.tracks.len() {
                    let track_name = state.tracks[index].name.clone();
                    drop(state);
                    self.track = Some(track_name);

                    if position.y <= TRACK_DRAG_SCROLL_UP_HOTZONE_HEIGHT {
                        return Task::batch(vec![
                            operation::scroll_by(
                                Id::new(EDITOR_SCROLL_ID),
                                operation::AbsoluteOffset { x: 0.0, y: -28.0 },
                            ),
                            operation::scroll_by(
                                Id::new(TRACKS_SCROLL_ID),
                                operation::AbsoluteOffset { x: 0.0, y: -28.0 },
                            ),
                        ]);
                    }
                }
            }
            Message::TrackDropped(point, _rect) => {
                if self.track.is_some() {
                    return iced_drop::zones_on_point(Message::HandleTrackZones, point, None, None);
                }
                self.track = None;
            }
            Message::HandleTrackZones(ref zones) => {
                if let Some(dragged_name) = self.track.clone() {
                    let dragged_id = Id::from(dragged_name.clone());

                    // Dropped on empty space -> move the track to the root.
                    if zones.is_empty() {
                        let mut state = self.state.blocking_write();
                        if let Some(dragged_index) =
                            state.tracks.iter().position(|t| t.name == *dragged_name)
                        {
                            let mut moved_track = state.tracks.remove(dragged_index);
                            let had_parent = moved_track.parent_track.is_some();
                            moved_track.parent_track = None;
                            state.tracks.push(moved_track);
                            drop(state);
                            self.track = None;
                            if had_parent {
                                return self.send(Action::TrackSetParent {
                                    track_name: dragged_name.clone(),
                                    parent_name: None,
                                });
                            }
                        }
                        self.track = None;
                        return Task::none();
                    }

                    let target_zone = {
                        let state = self.state.blocking_read();
                        zones.iter().find(|(zone_id, _)| {
                            *zone_id != dragged_id
                                && state
                                    .tracks
                                    .iter()
                                    .any(|track| Id::from(track.name.clone()) == *zone_id)
                        })
                    };
                    if let Some((track_id, _)) = target_zone {
                        let mut state = self.state.blocking_write();
                        if let Some(dragged_index) =
                            state.tracks.iter().position(|t| t.name == *dragged_name)
                        {
                            let target_name = state
                                .tracks
                                .iter()
                                .find(|t| Id::from(t.name.clone()) == *track_id)
                                .map(|t| t.name.clone());

                            if let Some(target_name) = target_name {
                                if target_name == *dragged_name {
                                    // Dropped on itself; nothing to do.
                                } else if state
                                    .tracks
                                    .iter()
                                    .any(|t| t.name == target_name && t.is_folder)
                                {
                                    // Dropping onto a folder makes the dragged track a child.
                                    let mut current = target_name.as_str();
                                    let mut circular = false;
                                    while !circular {
                                        if let Some(parent) = state
                                            .tracks
                                            .iter()
                                            .find(|t| t.name == current)
                                            .and_then(|t| t.parent_track.as_deref())
                                        {
                                            if parent == dragged_name.as_str() {
                                                circular = true;
                                            } else {
                                                current = parent;
                                            }
                                        } else {
                                            break;
                                        }
                                    }

                                    if !circular {
                                        let mut moved_track = state.tracks.remove(dragged_index);
                                        moved_track.parent_track = Some(target_name.clone());

                                        if let Some(target_index) =
                                            state.tracks.iter().position(|t| t.name == target_name)
                                        {
                                            let target_depth = state.tracks[target_index]
                                                .folder_depth(&state.tracks);
                                            let mut insert_index = target_index + 1;
                                            while insert_index < state.tracks.len() {
                                                let depth = state.tracks[insert_index]
                                                    .folder_depth(&state.tracks);
                                                if depth > target_depth {
                                                    insert_index += 1;
                                                } else {
                                                    break;
                                                }
                                            }
                                            state.tracks.insert(insert_index, moved_track);
                                        } else {
                                            state.tracks.push(moved_track);
                                        }

                                        let mut tasks: Vec<Task<Message>> = vec![];
                                        if let Some(folder) =
                                            state.tracks.iter_mut().find(|t| t.name == target_name)
                                            && !folder.folder_open
                                        {
                                            folder.folder_open = true;
                                            tasks.push(self.send(Action::TrackToggleFolder {
                                                track_name: target_name.clone(),
                                            }));
                                        }
                                        drop(state);
                                        tasks.push(self.send(Action::TrackSetParent {
                                            track_name: dragged_name.clone(),
                                            parent_name: Some(target_name),
                                        }));
                                        self.track = None;
                                        return Task::batch(tasks);
                                    }
                                } else {
                                    // Dropping onto a non-folder reorders the track and removes it
                                    // from any folder.
                                    let mut moved_track = state.tracks.remove(dragged_index);
                                    let had_parent = moved_track.parent_track.is_some();
                                    moved_track.parent_track = None;
                                    let to_index = state
                                        .tracks
                                        .iter()
                                        .position(|t| Id::from(t.name.clone()) == *track_id);

                                    if let Some(t_idx) = to_index {
                                        state.tracks.insert(t_idx, moved_track);
                                    } else {
                                        state.tracks.push(moved_track);
                                    }

                                    drop(state);
                                    if had_parent {
                                        self.track = None;
                                        return self.send(Action::TrackSetParent {
                                            track_name: dragged_name.clone(),
                                            parent_name: None,
                                        });
                                    }
                                }
                            }
                        }
                    } else if !zones.iter().any(|(zone_id, _)| *zone_id == dragged_id) {
                        // The workspace drop zone was hit without a track beneath the pointer.
                        let mut state = self.state.blocking_write();
                        if let Some(dragged_index) =
                            state.tracks.iter().position(|t| t.name == *dragged_name)
                        {
                            let mut moved_track = state.tracks.remove(dragged_index);
                            let had_parent = moved_track.parent_track.is_some();
                            moved_track.parent_track = None;
                            state.tracks.push(moved_track);
                            drop(state);
                            self.track = None;
                            if had_parent {
                                return self.send(Action::TrackSetParent {
                                    track_name: dragged_name.clone(),
                                    parent_name: None,
                                });
                            }
                        }
                    }
                }
                self.track = None;
            }
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
                    self.state.blocking_write().message =
                        "Cleanup requires an opened/saved session folder".to_string();
                    return Task::none();
                };

                match self.delete_unused_session_media_files(&session_root) {
                    Ok(report)
                        if report.deleted_files.is_empty() && report.failed_files.is_empty() =>
                    {
                        self.state.blocking_write().message = "No unused files found".to_string();
                    }
                    Ok(report) if report.failed_files.is_empty() => {
                        self.state.blocking_write().message =
                            format!("Deleted {} unused file(s)", report.deleted_files.len());
                    }
                    Ok(report) if report.deleted_files.is_empty() => {
                        self.state.blocking_write().message = format!(
                            "Failed to delete {} unused file(s)",
                            report.failed_files.len()
                        );
                    }
                    Ok(report) => {
                        self.state.blocking_write().message = format!(
                            "Deleted {} unused file(s); {} failed",
                            report.deleted_files.len(),
                            report.failed_files.len()
                        );
                    }
                    Err(e) => {
                        self.state.blocking_write().message = e;
                    }
                }
            }
            Message::CollectToSession => match self.collect_to_session() {
                Ok(message) => {
                    self.state.blocking_write().message = message;
                }
                Err(e) => {
                    self.state.blocking_write().message = e;
                }
            },
            Message::ImportFilesSelected(Some(ref paths)) => {
                if paths.is_empty() {
                    self.state.blocking_write().message = "No files selected".to_string();
                    return Task::none();
                }
                let Some(session_root) = self.session_dir.clone() else {
                    self.state.blocking_write().message =
                        "Import requires an opened/saved session folder".to_string();
                    return Task::none();
                };

                let _used_track_names: HashSet<String> = self
                    .state
                    .blocking_read()
                    .tracks
                    .iter()
                    .map(|track| track.name.clone())
                    .collect();

                let total_files = paths.len();
                self.import_in_progress = true;
                self.import_current_file = 0;
                self.import_total_files = total_files;
                self.import_file_progress = 0.0;
                self.import_current_filename = String::new();

                let paths = paths.clone();
                let playback_rate = self.playback_rate_hz.max(1.0);

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

                        iced::futures::stream::unfold(rx, |mut rx| async move {
                            rx.recv().await.map(|msg| (msg, rx))
                        })
                    },
                    |msg| msg,
                );
            }
            Message::ImportFilesSelected(None) => {}
            Message::GenerateAudioModelSelected(model) => {
                self.generate_audio_model = model;
            }
            Message::GenerateAudioPromptAction(ref action) => {
                self.generate_audio_prompt_editor.perform(action.clone());
            }

            Message::GenerateAudioTagsInput(ref value) => {
                self.generate_audio_tags_input = value.clone();
            }

            Message::GenerateAudioBackendSelected(backend) => {
                self.generate_audio_backend = backend;
            }

            Message::GenerateAudioCfgScaleInput(ref value) => {
                self.generate_audio_cfg_scale_input = value.clone();
            }
            Message::GenerateAudioStepsInput(value) => {
                self.generate_audio_steps_input = value;
            }
            Message::GenerateAudioSecondsTotalInput(value) => {
                self.generate_audio_seconds_total_input = value;
            }
            Message::GenerateAudioCancel => {
                #[cfg(unix)]
                if let Some(pid) = self.generate_audio_process_id.take() {
                    use nix::sys::signal::{self, Signal};
                    use nix::unistd::Pid;
                    let _ = signal::kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
                }
                if let Some(handle) = self.generate_audio_abort_handle.take() {
                    handle.abort();
                }
                self.generate_audio_in_progress = false;
                self.generate_audio_progress = 0.0;
                self.generate_audio_operation = None;
                self.info("Generation cancelled");
            }
            Message::GenerateAudioSubmit => {
                if self.generate_audio_in_progress {
                    return Task::none();
                }
                let Some(session_root) = self.session_dir.clone() else {
                    self.warning("Generated audio requires an opened/saved session");
                    return Task::none();
                };

                let prompt = self.generate_audio_prompt_editor.text().trim().to_string();
                if prompt.is_empty() {
                    self.warning("Prompt cannot be empty");
                    return Task::none();
                }
                let cfg_scale = match self.generate_audio_cfg_scale_input.trim().parse::<f32>() {
                    Ok(value) if value.is_finite() && (0.0..=20.0).contains(&value) => value,
                    _ => {
                        self.warning("CFG scale must be a number between 0 and 20");
                        return Task::none();
                    }
                };
                let ode_steps = self.generate_audio_steps_input;
                if ode_steps == 0 || ode_steps > 50 {
                    self.warning("ODE steps must be between 1 and 50");
                    return Task::none();
                }
                let transport_sample = self.transport_samples.max(0.0) as usize;
                let (bpm, time_signature_num, time_signature_denom) = {
                    let state = self.state.blocking_read();
                    Self::timing_at_sample(&state, transport_sample)
                };
                let output_stem = super::super::Maolan::sanitize_generated_track_base_name(&prompt);
                let request = super::super::BurnGenerateRequest {
                    model: self.generate_audio_model,
                    prompt,
                    output_path: {
                        let output_rel = match super::super::Maolan::unique_import_rel_path(
                            &session_root,
                            "audio",
                            &output_stem,
                            "wav",
                        ) {
                            Ok(rel) => rel,
                            Err(err) => {
                                self.error(format!(
                                    "Failed to prepare generated output path: {err}"
                                ));
                                return Task::none();
                            }
                        };
                        session_root.join(output_rel)
                    },
                    tags: {
                        Some(super::super::Maolan::generate_audio_tags_with_timing(
                            self.generate_audio_tags_input.trim(),
                            bpm,
                            time_signature_num,
                            time_signature_denom,
                        ))
                    },
                    backend: self.generate_audio_backend,
                    cfg_scale,
                    ode_steps,
                    length: self.generate_audio_seconds_total_input.saturating_mul(1000),
                };

                let _used_track_names: HashSet<String> = self
                    .state
                    .blocking_read()
                    .tracks
                    .iter()
                    .map(|track| track.name.clone())
                    .collect();

                self.generate_audio_in_progress = true;
                self.generate_audio_progress = 0.0;
                self.generate_audio_operation = Some("Launching generate".to_string());

                let (_pid, socket) = match super::super::Maolan::spawn_generate_process(&request) {
                    Ok((_pid, socket)) => (_pid, socket),
                    Err(err) => {
                        self.generate_audio_in_progress = false;
                        self.error(err);
                        return Task::none();
                    }
                };
                #[cfg(unix)]
                {
                    self.generate_audio_process_id = Some(_pid);
                }

                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                let task_handle = tokio::spawn(async move {
                    let _ = tx.send(Message::GenerateAudioProgress {
                        progress: 0.05,
                        operation: Some("Generating audio".to_string()),
                    });

                    let tx_progress = tx.clone();
                    match tokio::task::spawn_blocking(move || {
                        super::super::Maolan::communicate_with_generate_process(
                            socket,
                            |phase, progress, operation| {
                                let _ = tx_progress.send(Message::GenerateAudioProgress {
                                    progress: progress.clamp(0.0, 1.0),
                                    operation: Some(format!("{}: {}", phase, operation)),
                                });
                            },
                        )
                    })
                    .await
                    {
                        Ok(Ok(())) => {}
                        Ok(Err(err)) => {
                            let _ = tx.send(Message::GenerateAudioFinished(Err(err)));
                            return;
                        }
                        Err(err) => {
                            let _ = tx.send(Message::GenerateAudioFinished(Err(format!(
                                "generate task failed: {err}"
                            ))));
                            return;
                        }
                    };
                    #[cfg(unix)]
                    {
                        let mut used_names = _used_track_names;
                        let base_name = super::super::Maolan::sanitize_generated_track_base_name(
                            &request.prompt,
                        );
                        let output_path = request.output_path.clone();
                        let tx_clone = tx.clone();
                        let mut last_progress_bucket: Option<u16> = None;
                        let mut last_operation: Option<String> = None;
                        let mut progress_fn = move |progress: f32, operation: Option<String>| {
                            let adjusted = 0.55 + progress.clamp(0.0, 1.0) * 0.45;
                            let bucket = (adjusted * 100.0).round() as u16;
                            if last_progress_bucket == Some(bucket) && last_operation == operation {
                                return;
                            }
                            last_progress_bucket = Some(bucket);
                            last_operation = operation.clone();
                            let _ = tx_clone.send(Message::GenerateAudioProgress {
                                progress: adjusted,
                                operation,
                            });
                        };

                        progress_fn(0.95, Some("Calculating peaks".to_string()));
                        let generated_file_result = tokio::task::spawn_blocking(move || {
                            let clip_rel = output_path
                            .strip_prefix(&session_root)
                            .ok()
                            .and_then(|path| path.to_str())
                            .map(|path| path.replace('\\', "/"))
                            .ok_or_else(|| {
                                format!(
                                    "Generated audio path '{}' is outside the session directory",
                                    output_path.display()
                                )
                            })?;
                            let channels =
                                super::super::Maolan::audio_clip_channel_count(&output_path)
                                    .map_err(|e| {
                                        format!(
                                            "Failed to inspect generated audio channels '{}': {e}",
                                            output_path.display()
                                        )
                                    })?;
                            let length =
                                super::super::Maolan::audio_clip_source_length(&output_path)
                                    .map_err(|e| {
                                        format!(
                                            "Failed to read generated audio length '{}': {e}",
                                            output_path.display()
                                        )
                                    })?;
                            let peaks =
                                super::super::Maolan::compute_audio_clip_peaks(&output_path)
                                    .map_err(|e| {
                                        format!(
                                            "Failed to compute generated audio peaks '{}': {e}",
                                            output_path.display()
                                        )
                                    })?;
                            Ok::<_, String>((clip_rel, channels, length.max(1), peaks))
                        })
                        .await;

                        progress_fn(1.0, Some("Complete".to_string()));

                        let (clip_rel, channels, length, peaks): (
                            String,
                            usize,
                            usize,
                            crate::state::ClipPeaks,
                        ) = match generated_file_result {
                            Ok(Ok(result)) => result,
                            Ok(Err(err)) => {
                                let _ = tx.send(Message::GenerateAudioFinished(Err(err)));
                                return;
                            }
                            Err(err) => {
                                let _ = tx.send(Message::GenerateAudioFinished(Err(format!(
                                    "Failed to inspect generated audio: {err}"
                                ))));
                                return;
                            }
                        };

                        let track_name =
                            super::super::Maolan::unique_track_name(&base_name, &mut used_names);
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

                        if let Err(err) = CLIENT
                            .send(EngineMessage::Request(Action::AddTrack {
                                name: track_name.clone(),
                                audio_ins: channels,
                                midi_ins: 0,
                                audio_outs: channels,
                                midi_outs: 0,
                                folder: false,
                            }))
                            .await
                        {
                            let _ = tx.send(Message::GenerateAudioFinished(Err(format!(
                                "Failed to add generated track: {err}"
                            ))));
                            return;
                        }

                        if let Err(err) = CLIENT
                            .send(EngineMessage::Request(Action::AddClip {
                                clip_id: crate::state::generate_clip_id(),
                                name: clip_rel,
                                track_name: track_name.clone(),
                                start: 0,
                                length,
                                offset: 0,
                                input_channel: 0,
                                muted: false,
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
                                plugin_graph_json: Some(
                                    super::super::Maolan::default_clip_plugin_graph_json(
                                        channels, channels,
                                    ),
                                ),
                            }))
                            .await
                        {
                            let _ = tx.send(Message::GenerateAudioFinished(Err(format!(
                                "Failed to add generated clip: {err}"
                            ))));
                            return;
                        }

                        let _ = tx.send(Message::GenerateAudioProgress {
                            progress: 1.0,
                            operation: Some("Complete".to_string()),
                        });
                        let _ = tx.send(Message::GenerateAudioFinished(Ok(track_name)));
                    }
                    #[cfg(not(unix))]
                    {
                        let _ = tx.send(Message::GenerateAudioFinished(Err(
                            "Generated audio is only available on Unix platforms".to_string(),
                        )));
                    }
                });

                self.generate_audio_abort_handle = Some(task_handle.abort_handle());

                return Task::run(
                    iced::futures::stream::unfold(rx, |mut rx| async move {
                        rx.recv().await.map(|msg| (msg, rx))
                    }),
                    |msg| msg,
                );
            }
            Message::GenerateAudioProgress {
                progress,
                ref operation,
            } => {
                self.generate_audio_progress = progress.clamp(0.0, 1.0);
                self.generate_audio_operation = operation.clone();
                if let Some(operation) = operation {
                    self.state.blocking_write().message = operation.clone();
                }
            }
            Message::GenerateAudioFinished(ref result) => {
                self.generate_audio_in_progress = false;
                self.generate_audio_progress = if result.is_ok() { 1.0 } else { 0.0 };
                self.generate_audio_operation = None;
                self.generate_audio_abort_handle = None;
                #[cfg(unix)]
                {
                    self.generate_audio_process_id = None;
                }
                match result {
                    Ok(track_name) => {
                        self.modal = None;
                        self.info(format!("Generated audio imported to track '{track_name}'"));
                    }
                    Err(err) => {
                        self.error(err.to_string());
                    }
                }
            }
            Message::OpenExporter => {
                if self.session_dir.is_none() {
                    self.state.blocking_write().message =
                        "Export requires an opened/saved session".to_string();
                    return Task::none();
                }
                let nearest_rate = crate::consts::gui_mod::STANDARD_EXPORT_SAMPLE_RATES
                    .iter()
                    .min_by_key(|rate| {
                        (i64::from(**rate) - self.playback_rate_hz.round() as i64).abs()
                    })
                    .copied()
                    .unwrap_or(48_000);
                self.export_sample_rate_hz = nearest_rate;
                self.normalize_export_hw_out_ports();
                self.export_format_mp3 =
                    self.export_format_mp3 && self.export_mp3_supported_for_current_settings();
                self.modal = Some(crate::message::Show::ExportSettings);
            }
            Message::MidiLearnMappingsPanelToggle => {
                self.midi_mappings_panel_open = !self.midi_mappings_panel_open;
                if self.midi_mappings_panel_open {
                    return self.send(Action::RequestMidiLearnMappingsReport);
                }
            }
            Message::MidiLearnMappingsReportRequest => {
                return self.send(Action::RequestMidiLearnMappingsReport);
            }
            Message::MidiLearnMappingsExportRequest => match self.export_midi_mappings_file() {
                Ok(path) => {
                    self.state.blocking_write().message =
                        format!("Exported MIDI mappings: {}", path.display());
                }
                Err(e) => {
                    self.state.blocking_write().message = e;
                }
            },
            Message::MidiLearnMappingsImportRequest => match self.import_midi_mappings_actions() {
                Ok(actions) => {
                    let mut tasks = Vec::with_capacity(actions.len() + 2);
                    tasks.push(self.send(Action::BeginHistoryGroup));
                    for action in actions {
                        tasks.push(self.send(action));
                    }
                    tasks.push(self.send(Action::EndHistoryGroup));
                    self.state.blocking_write().message = "Imported MIDI mappings".to_string();
                    return Task::batch(tasks);
                }
                Err(e) => {
                    self.state.blocking_write().message = e;
                }
            },
            Message::MidiLearnMappingsClearAllRequest => {
                return self.send(Action::ClearAllMidiLearnBindings);
            }
            Message::ExportSettingsConfirm => {
                let master_ceiling = self.export_master_limiter_ceiling_input.parse::<f32>().ok();
                let Some(master_ceiling) = master_ceiling else {
                    self.state.blocking_write().message =
                        "Master limiter ceiling must be a number in dBTP".to_string();
                    return Task::none();
                };
                if !(-20.0..=0.0).contains(&master_ceiling) {
                    self.state.blocking_write().message =
                        "Master limiter ceiling must be between -20.0 and 0.0 dBTP".to_string();
                    return Task::none();
                }
                if self.export_normalize {
                    match self.export_normalize_mode {
                        ExportNormalizeMode::Peak => {
                            let target = self.export_normalize_dbfs_input.parse::<f32>().ok();
                            let Some(target) = target else {
                                self.state.blocking_write().message =
                                    "Normalize target must be a number in dBFS".to_string();
                                return Task::none();
                            };
                            if !(-60.0..=0.0).contains(&target) {
                                self.state.blocking_write().message =
                                    "Normalize target must be between -60.0 and 0.0 dBFS"
                                        .to_string();
                                return Task::none();
                            }
                        }
                        ExportNormalizeMode::Loudness => {
                            let lufs = self.export_normalize_lufs_input.parse::<f32>().ok();
                            let dbtp = self.export_normalize_dbtp_input.parse::<f32>().ok();
                            let (Some(lufs), Some(dbtp)) = (lufs, dbtp) else {
                                self.state.blocking_write().message =
                                    "Loudness mode requires numeric LUFS and dBTP values"
                                        .to_string();
                                return Task::none();
                            };
                            if !(-70.0..=-5.0).contains(&lufs) {
                                self.state.blocking_write().message =
                                    "LUFS target must be between -70.0 and -5.0".to_string();
                                return Task::none();
                            }
                            if !(-20.0..=0.0).contains(&dbtp) {
                                self.state.blocking_write().message =
                                    "dBTP ceiling must be between -20.0 and 0.0".to_string();
                                return Task::none();
                            }
                        }
                    }
                }
                let selected_formats = self.selected_export_formats();
                if selected_formats.is_empty() {
                    self.state.blocking_write().message =
                        "Select at least one export format".to_string();
                    return Task::none();
                }
                if self.export_format_mp3 && !self.export_mp3_supported_for_current_settings() {
                    self.state.blocking_write().message =
                        "MP3 export supports only mono or stereo".to_string();
                    return Task::none();
                }
                if matches!(self.export_render_mode, ExportRenderMode::Mixdown)
                    && self.export_hw_out_ports.is_empty()
                {
                    self.state.blocking_write().message =
                        "Select at least one hw:out port for mixdown export".to_string();
                    return Task::none();
                }
                if self.export_format_ogg {
                    let ogg_quality = self.export_ogg_quality_input.parse::<f32>().ok();
                    let Some(ogg_quality) = ogg_quality else {
                        self.state.blocking_write().message =
                            "OGG quality must be a number between -0.1 and 1.0".to_string();
                        return Task::none();
                    };
                    if !(-0.1..=1.0).contains(&ogg_quality) {
                        self.state.blocking_write().message =
                            "OGG quality must be between -0.1 and 1.0".to_string();
                        return Task::none();
                    }
                }
                self.modal = None;
                return Task::perform(
                    async move {
                        AsyncFileDialog::new()
                            .set_title("Export Audio")
                            .add_filter("Audio", &["wav", "mp3", "ogg", "flac"])
                            .set_file_name("export")
                            .save_file()
                            .await
                            .map(|handle| handle.path().to_path_buf())
                    },
                    Message::ExportFileSelected,
                );
            }
            Message::ExportFileSelected(Some(ref path)) => {
                let Some(session_root) = self.session_dir.clone() else {
                    self.state.blocking_write().message =
                        "Export requires an opened/saved session".to_string();
                    return Task::none();
                };

                let sample_rate = self.export_sample_rate_hz as i32;
                let export_bit_depth = self.export_bit_depth;
                let export_normalize = self.export_normalize;
                let normalize_mode = self.export_normalize_mode;
                let normalize_target_dbfs = self
                    .export_normalize_dbfs_input
                    .parse::<f32>()
                    .ok()
                    .unwrap_or(0.0);
                let normalize_target_lufs = self
                    .export_normalize_lufs_input
                    .parse::<f32>()
                    .ok()
                    .unwrap_or(-23.0);
                let normalize_true_peak_dbtp = self
                    .export_normalize_dbtp_input
                    .parse::<f32>()
                    .ok()
                    .unwrap_or(-1.0);
                let normalize_tp_limiter = self.export_normalize_tp_limiter;
                let export_master_limiter = self.export_master_limiter;
                let export_master_limiter_ceiling_dbtp = self
                    .export_master_limiter_ceiling_input
                    .parse::<f32>()
                    .ok()
                    .unwrap_or(-1.0);
                let export_realtime_fallback = self.export_realtime_fallback;
                let export_formats = self.selected_export_formats();
                if export_formats.is_empty() {
                    self.state.blocking_write().message =
                        "Select at least one export format".to_string();
                    return Task::none();
                }
                if self.export_format_mp3 && !self.export_mp3_supported_for_current_settings() {
                    self.state.blocking_write().message =
                        "MP3 export supports only mono or stereo".to_string();
                    return Task::none();
                }
                let export_path = Self::export_base_path(path.clone());
                let export_mp3_mode = self.export_mp3_mode;
                let export_mp3_bitrate_kbps = self.export_mp3_bitrate_kbps;
                let export_ogg_quality = self
                    .export_ogg_quality_input
                    .parse::<f32>()
                    .ok()
                    .unwrap_or(0.6);
                let selected_hw_out_ports = self.export_hw_out_ports.iter().copied().collect();
                let state_clone = self.state.clone();
                let render_mode = self.export_render_mode;
                let (
                    metadata_author,
                    metadata_album,
                    metadata_year,
                    metadata_track_number,
                    metadata_genre,
                ) = {
                    let state = self.state.blocking_read();
                    (
                        state.session_author.trim().to_string(),
                        state.session_album.trim().to_string(),
                        state.session_year.trim().parse::<u32>().ok(),
                        state.session_track_number.trim().parse::<u32>().ok(),
                        state.session_genre.trim().to_string(),
                    )
                };

                self.export_in_progress = true;
                self.export_progress = 0.0;
                self.export_operation = Some("Preparing".to_string());
                self.export_cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                let export_cancel = self.export_cancel.clone();

                self.export_pending_bounces.clear();
                if matches!(
                    render_mode,
                    crate::message::ExportRenderMode::StemsPostFader
                ) {
                    let state_guard = self.state.blocking_read();
                    let has_solo = state_guard.tracks.iter().any(|t| t.soloed);
                    let selected_set: std::collections::HashSet<String> =
                        state_guard.selected.iter().cloned().collect();
                    for track in &state_guard.tracks {
                        if selected_set.contains(&track.name)
                            && !track.muted
                            && (!has_solo || track.soloed)
                        {
                            self.export_pending_bounces.insert(track.name.clone());
                        }
                    }
                }
                let bounce_notify = std::sync::Arc::new(tokio::sync::Notify::new());
                self.export_bounce_notify = Some(bounce_notify.clone());

                return Task::run(
                    {
                        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                        tokio::spawn(async move {
                            let tx_clone = tx.clone();
                            let mut last_progress_bucket: Option<u16> = None;
                            let mut last_operation: Option<String> = None;
                            let progress_fn = move |progress: f32, operation: Option<String>| {
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
                                    .send(Message::ExportProgress {
                                        progress: clamped,
                                        operation,
                                    })
                                    .is_err()
                                {}
                            };

                            let options = super::super::ExportSessionOptions {
                                export_path: export_path.clone(),
                                sample_rate,
                                formats: export_formats,
                                render_mode,
                                selected_hw_out_ports,
                                realtime_fallback: export_realtime_fallback,
                                bit_depth: export_bit_depth,
                                mp3_mode: export_mp3_mode,
                                mp3_bitrate_kbps: export_mp3_bitrate_kbps,
                                ogg_quality: export_ogg_quality,
                                normalize: export_normalize,
                                normalize_target_dbfs,
                                normalize_mode,
                                normalize_target_lufs,
                                normalize_true_peak_dbtp,
                                normalize_tp_limiter,
                                master_limiter: export_master_limiter,
                                master_limiter_ceiling_dbtp: export_master_limiter_ceiling_dbtp,
                                metadata_author,
                                metadata_album,
                                metadata_year,
                                metadata_track_number,
                                metadata_genre,
                                state: state_clone,
                                session_root: session_root.clone(),
                            };
                            let result = Self::export_session(
                                &options,
                                &export_cancel,
                                Some(bounce_notify),
                                progress_fn,
                            )
                            .await;

                            if let Err(e) = result {
                                if tx
                                    .send(Message::ExportProgress {
                                        progress: 0.0,
                                        operation: Some(format!("Error: {}", e)),
                                    })
                                    .is_err()
                                {
                                    return;
                                }
                            } else if tx
                                .send(Message::ExportProgress {
                                    progress: 1.0,
                                    operation: Some("Complete".to_string()),
                                })
                                .is_err()
                            {
                                return;
                            }
                            drop(tx);
                        });

                        iced::futures::stream::unfold(rx, |mut rx| async move {
                            rx.recv().await.map(|msg| (msg, rx))
                        })
                    },
                    |msg| msg,
                );
            }
            Message::ExportFileSelected(None) => {}
            Message::ExportProgress {
                progress,
                ref operation,
            } => {
                if (self.export_progress - progress).abs() < f32::EPSILON
                    && self.export_operation == *operation
                {
                    return Task::none();
                }
                self.export_progress = progress;
                self.export_operation = operation.clone();

                if let Some(op) = operation
                    && op.starts_with("Error:")
                {
                    self.export_in_progress = false;
                    self.state.blocking_write().message = op.clone();
                } else if progress >= 1.0 {
                    self.export_in_progress = false;
                    self.state.blocking_write().message = operation
                        .clone()
                        .unwrap_or_else(|| "Export complete".to_string());
                } else if let Some(op) = operation {
                    let percent = (progress * 100.0) as usize;
                    self.state.blocking_write().message = format!("Exporting ({percent}%): {}", op);
                } else {
                    let percent = (progress * 100.0) as usize;
                    self.state.blocking_write().message = format!("Exporting ({percent}%)...");
                }
            }
            Message::PreferencesSampleRateSelected(rate) => {
                self.prefs_export_sample_rate_hz = rate;
            }
            Message::PreferencesSnapModeSelected(mode) => {
                self.prefs_snap_mode = mode;
            }
            Message::PreferencesMidiSnapModeSelected(mode) => {
                self.prefs_midi_snap_mode = mode;
            }
            Message::PreferencesBitDepthSelected(bits) => {
                self.prefs_audio_bit_depth = bits;
            }
            Message::PreferencesOscEnabledToggled(enabled) => {
                self.prefs_osc_enabled = enabled;
            }
            Message::PreferencesOutputDeviceSelected(ref device) => {
                self.prefs_default_output_device_id =
                    (device.id != super::super::PREF_DEVICE_AUTO_ID).then(|| device.id.clone());
            }
            Message::PreferencesInputDeviceSelected(ref device) => {
                self.prefs_default_input_device_id =
                    (device.id != super::super::PREF_DEVICE_AUTO_ID).then(|| device.id.clone());
            }
            Message::PreferencesSave => {
                let mut cfg = crate::config::Config::load().unwrap_or_default();
                cfg.osc_enabled = self.prefs_osc_enabled;
                cfg.default_export_sample_rate_hz = self.prefs_export_sample_rate_hz;
                cfg.default_snap_mode = self.prefs_snap_mode;
                cfg.default_midi_snap_mode = self.prefs_midi_snap_mode;
                cfg.default_audio_bit_depth = self.prefs_audio_bit_depth;
                cfg.default_output_device_id = self.prefs_default_output_device_id.clone();
                cfg.default_input_device_id = self.prefs_default_input_device_id.clone();
                let prefs = super::super::AppPreferences {
                    osc_enabled: cfg.osc_enabled,
                    default_export_sample_rate_hz: cfg.default_export_sample_rate_hz,
                    default_snap_mode: cfg.default_snap_mode,
                    default_midi_snap_mode: cfg.default_midi_snap_mode,
                    default_audio_bit_depth: cfg.default_audio_bit_depth,
                    default_output_device_id: cfg.default_output_device_id.clone(),
                    default_input_device_id: cfg.default_input_device_id.clone(),
                    recent_session_paths: cfg.recent_session_paths.clone(),
                };
                match cfg.save().map_err(|e| e.to_string()) {
                    Ok(()) => {
                        self.export_sample_rate_hz = self.prefs_export_sample_rate_hz;
                        self.snap_mode = self.prefs_snap_mode;
                        self.midi_snap_mode = self.prefs_midi_snap_mode;
                        let task = self.send(Action::SetOscEnabled(self.prefs_osc_enabled));
                        {
                            let mut state = self.state.blocking_write();
                            Self::apply_preferred_devices_to_state(&mut state, &prefs);
                        }
                        self.modal = None;
                        self.info("Preferences saved: ~/.config/maolan/config.toml");
                        return task;
                    }
                    Err(e) => {
                        self.error(format!("Failed to save preferences: {e}"));
                    }
                }
            }
            Message::SessionMetadataAuthorInput(ref value) => {
                self.state.blocking_write().session_author = value.clone();
            }
            Message::SessionMetadataAlbumInput(ref value) => {
                self.state.blocking_write().session_album = value.clone();
            }
            Message::SessionMetadataYearInput(ref value) => {
                self.state.blocking_write().session_year = value
                    .chars()
                    .filter(|c| c.is_ascii_digit())
                    .collect::<String>();
            }
            Message::SessionMetadataTrackNumberInput(ref value) => {
                self.state.blocking_write().session_track_number = value
                    .chars()
                    .filter(|c| c.is_ascii_digit())
                    .collect::<String>();
            }
            Message::SessionMetadataGenreInput(ref value) => {
                self.state.blocking_write().session_genre = value.clone();
            }
            Message::SessionMetadataSave => {
                {
                    let mut state = self.state.blocking_write();
                    state.session_author = state.session_author.trim().to_string();
                    state.session_album = state.session_album.trim().to_string();
                    state.session_year = state.session_year.trim().to_string();
                    state.session_track_number = state.session_track_number.trim().to_string();
                    state.session_genre = state.session_genre.trim().to_string();
                    state.message = "Session metadata updated".to_string();
                }
                self.has_unsaved_changes = true;
                self.modal = None;
            }
            Message::ImportProgress {
                file_index,
                total_files,
                file_progress,
                ref filename,
                ref operation,
            } => {
                if self.import_current_file == file_index
                    && self.import_total_files == total_files
                    && (self.import_file_progress - file_progress).abs() < f32::EPSILON
                    && self.import_current_filename == *filename
                    && self.import_current_operation == *operation
                {
                    return Task::none();
                }
                self.import_current_file = file_index;
                self.import_total_files = total_files;
                self.import_file_progress = file_progress;
                self.import_current_filename = filename.clone();
                self.import_current_operation = operation.clone();

                if file_index >= total_files && file_progress >= 1.0 {
                    self.import_in_progress = false;
                    self.state.blocking_write().message = format!("Imported {total_files} file(s)");
                } else {
                    let percent = (file_progress * 100.0) as usize;
                    let op_text = operation
                        .as_ref()
                        .map(|s| format!(" [{}]", s))
                        .unwrap_or_default();
                    self.state.blocking_write().message = format!(
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
                    self.state.blocking_write().message = format!("Imported {total_files} file(s)");
                } else {
                    let succeeded = total_files.saturating_sub(failed_files.len());
                    let first_error = failed_files.first().cloned().unwrap_or_default();
                    self.state.blocking_write().message = format!(
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
                self.pending_precomputed_peaks.insert(key, peaks.clone());
            }
            Message::TrackTemplatesLoaded(ref track_templates, ref folder_templates) => {
                self.add_track
                    .set_available_templates(track_templates.clone());
                self.add_track
                    .set_available_folder_templates(folder_templates.clone());
                if let Some(dialog) = &mut self.state.blocking_write().apply_template_dialog {
                    dialog.available_templates = track_templates.clone();
                    dialog.available_folder_templates = folder_templates.clone();
                }
            }
            #[cfg(any(
                target_os = "linux",
                target_os = "windows",
                target_os = "freebsd",
                target_os = "openbsd"
            ))]
            Message::PreferencesDevicesLoaded {
                ref output_devices,
                ref input_devices,
            } => {
                let mut state = self.state.blocking_write();
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

                let mut state = self.state.blocking_write();
                for update in updates {
                    let key = Self::audio_clip_key(
                        &update.track_name,
                        &update.clip_name,
                        update.start,
                        update.length,
                        update.offset,
                    );
                    if update.done {
                        self.pending_peak_rebuilds.remove(&key);
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
            Message::Workspace => {
                let stop_task = if self.live_session_playing {
                    self.stop_live_session_play()
                } else {
                    Task::none()
                };
                let mut state = self.state.blocking_write();
                state.view = View::Workspace;
                state.pitch_correction = None;
                state.pitch_correction_selected_points.clear();
                state.pitch_correction_dragging_points = None;
                state.pitch_correction_selecting_rect = None;
                drop(state);
                let view_task = self.queue_midi_clip_preview_loads();
                return Task::batch(vec![stop_task, view_task]);
            }
            Message::ToggleMixerVisibility => {
                self.mixer_visible = !self.mixer_visible;
                if !self.mixer_visible {
                    self.mixer_resize_hovered = false;
                }
            }
            Message::ToggleTracksVisibility => {
                self.tracks_visible = !self.tracks_visible;
                if !self.tracks_visible {
                    self.tracks_resize_hovered = false;
                }
            }
            Message::ToggleEditorVisibility => {
                self.editor_visible = !self.editor_visible;
            }
            Message::ToggleToolbarVisibility => {
                self.toolbar_visible = !self.toolbar_visible;
            }
            Message::X32 => {
                let mut state = self.state.blocking_write();
                state.view = crate::state::View::X32;
            }
            Message::Session => {
                let mut state = self.state.blocking_write();
                state.view = crate::state::View::Session;
                let track_names: Vec<String> =
                    state.tracks.iter().map(|t| t.name.clone()).collect();
                for track_name in track_names {
                    state.session.ensure_track_slots(&track_name);
                }
                state.session.backfill_play_stop_icons();
            }
            Message::HwMixer(msg) => {
                return mixosc::app::update(&mut self.hw_mixer, msg).map(Message::HwMixer);
            }
            Message::ToggleLogVisibility => {
                self.show_log_window = !self.show_log_window;
            }
            Message::ToggleShortcutsPane => {
                self.shortcuts_pane_visible = !self.shortcuts_pane_visible;
            }
            Message::ToggleModulatorsPane => {
                self.modulators_pane_visible = !self.modulators_pane_visible;
                if !self.modulators_pane_visible {
                    self.selected_modulator_id = None;
                    self.state.blocking_write().selected_modulator_id = None;
                }
            }
            Message::ModulatorAdd => {
                let id = self.modulators.iter().map(|m| m.id).max().unwrap_or(0) + 1;
                self.modulators.push(crate::state::Modulator::new(id));
                self.selected_modulator_id = Some(id);
                self.state.blocking_write().selected_modulator_id = Some(id);
                self.has_unsaved_changes = true;
                return self.send_modulators_to_engine();
            }
            Message::ModulatorRemove(id) => {
                self.modulators.retain(|m| m.id != id);
                if self.selected_modulator_id == Some(id) {
                    self.selected_modulator_id = None;
                    self.state.blocking_write().selected_modulator_id = None;
                }
                self.has_unsaved_changes = true;
                return self.send_modulators_to_engine();
            }
            Message::ModulatorSelect(id) => {
                self.selected_modulator_id = id;
                self.state.blocking_write().selected_modulator_id = id;
            }
            Message::ModulatorToggleTarget { id, ref target } => {
                if let Some(m) = self.modulators.iter_mut().find(|m| m.id == id) {
                    let pos = m
                        .targets
                        .iter()
                        .position(|t| t.matches_target(&target.track_name, &target.target));
                    if let Some(pos) = pos {
                        m.targets.remove(pos);
                    } else {
                        m.targets.push(target.clone());
                    }
                }
                self.has_unsaved_changes = true;
                return self.send_modulators_to_engine();
            }
            Message::ModulatorToggleSelectedTarget { ref target } => {
                if let Some(id) = self.selected_modulator_id {
                    if let Some(m) = self.modulators.iter_mut().find(|m| m.id == id) {
                        let pos = m
                            .targets
                            .iter()
                            .position(|t| t.matches_target(&target.track_name, &target.target));
                        if let Some(pos) = pos {
                            m.targets.remove(pos);
                        } else {
                            m.targets.push(target.clone());
                        }
                    }
                    self.has_unsaved_changes = true;
                    return self.send_modulators_to_engine();
                }
            }
            Message::ModulatorUpdate { id, ref change } => {
                if let Some(m) = self.modulators.iter_mut().find(|m| m.id == id) {
                    match change {
                        ModulatorChange::Name(v) => m.name = v.clone(),
                        ModulatorChange::Shape(v) => m.shape = *v,
                        ModulatorChange::Rate(v) => m.rate = *v,
                        ModulatorChange::Phase(v) => m.phase = *v,
                        ModulatorChange::Enabled(v) => m.enabled = *v,
                        ModulatorChange::Targets(v) => m.targets = v.clone(),
                    }
                }
                self.has_unsaved_changes = true;
                return self.send_modulators_to_engine();
            }
            Message::ModulatorTargetShow {
                modulator_id,
                ref track_name,
                target,
            } => {
                let (default_min, default_max) = target.default_range();
                let existing = self
                    .modulators
                    .iter()
                    .find(|m| m.id == modulator_id)
                    .and_then(|m| {
                        m.targets
                            .iter()
                            .find(|t| t.matches_target(track_name, &target))
                    });
                let existing_bool = existing.is_some();
                let (min_input, max_input) = existing
                    .map_or((default_min.to_string(), default_max.to_string()), |t| {
                        (t.min.to_string(), t.max.to_string())
                    });
                self.state.blocking_write().modulator_target_dialog =
                    Some(crate::state::ModulatorTargetDialog {
                        modulator_id,
                        track_name: track_name.clone(),
                        target,
                        min_input,
                        max_input,
                        existing: existing_bool,
                    });
            }
            Message::ModulatorTargetMinInput(_) | Message::ModulatorTargetMaxInput(_) => {
                self.modulator_target_dialog.update(&message);
            }
            Message::ModulatorTargetConfirm => {
                let dialog = self.state.blocking_read().modulator_target_dialog.clone();
                let Some(dialog) = dialog else {
                    return Task::none();
                };

                let Ok(min) = dialog.min_input.trim().parse::<f32>() else {
                    self.state.blocking_write().message = "Invalid min value".to_string();
                    return Task::none();
                };
                let Ok(max) = dialog.max_input.trim().parse::<f32>() else {
                    self.state.blocking_write().message = "Invalid max value".to_string();
                    return Task::none();
                };

                if let Some(m) = self
                    .modulators
                    .iter_mut()
                    .find(|m| m.id == dialog.modulator_id)
                {
                    if let Some(target) = m
                        .targets
                        .iter_mut()
                        .find(|t| t.matches_target(&dialog.track_name, &dialog.target))
                    {
                        target.min = min;
                        target.max = max;
                    } else {
                        m.targets.push(crate::state::ModulatorTarget {
                            track_name: dialog.track_name,
                            target: dialog.target,
                            min,
                            max,
                        });
                    }
                }
                self.state.blocking_write().modulator_target_dialog = None;
                self.has_unsaved_changes = true;
                return self.send_modulators_to_engine();
            }
            Message::ModulatorTargetCancel => {
                self.state.blocking_write().modulator_target_dialog = None;
            }
            Message::ModulatorTargetRemove {
                modulator_id,
                ref track_name,
                target,
            } => {
                if let Some(m) = self.modulators.iter_mut().find(|m| m.id == modulator_id) {
                    m.targets.retain(|t| !t.matches_target(track_name, &target));
                }
                self.state.blocking_write().modulator_target_dialog = None;
                self.has_unsaved_changes = true;
                return self.send_modulators_to_engine();
            }
            Message::ToggleCutIndicator => {
                let cursor = self.active_workspace_cursor();
                let mut state = self.state.blocking_write();
                state.cut_preview_active = !state.cut_preview_active;
                let now_active = state.cut_preview_active;
                drop(state);

                if now_active && matches!(self.state.blocking_read().view, View::Workspace) {
                    self.update_cut_indicator(cursor);
                } else {
                    let mut state = self.state.blocking_write();
                    state.cut_indicator = None;
                }
            }
            Message::LogViewAction(ref action) if !action.is_edit() => {
                self.log_viewer_content.perform(action.clone());
            }
            Message::Connections => {
                let mut state = self.state.blocking_write();
                state.view = View::Connections;
                state.connections_folder = None;
                state.connection_view_selection = ConnectionViewSelection::None;
            }
            Message::MidiClipPreviewLoaded {
                ref track_idx,
                clip_idx,
                ref clip_name,
                ref notes,
            } => {
                self.pending_midi_clip_previews.remove(&(
                    track_idx.clone(),
                    clip_idx,
                    clip_name.clone(),
                ));
                let valid = {
                    let state = self.state.blocking_read();
                    state
                        .tracks
                        .iter()
                        .find(|track| track.name == *track_idx)
                        .and_then(|track| track.midi.clips.get(clip_idx))
                        .is_some_and(|clip| clip.name == *clip_name)
                };
                if valid {
                    self.midi_clip_previews.insert(
                        (track_idx.clone(), clip_idx),
                        std::sync::Arc::new(notes.clone()),
                    );
                }
            }
            Message::OpenMidiPiano {
                ref track_idx,
                clip_idx,
            } => {
                {
                    let state = self.state.blocking_read();
                    if let Some(piano) = &state.piano
                        && piano.track_idx == *track_idx
                        && piano.clip_index == clip_idx
                    {
                        drop(state);
                        {
                            let mut state = self.state.blocking_write();
                            state.view = View::Piano;
                        }
                        return Task::batch(vec![self.sync_piano_scrollbars()]);
                    }
                }
                let (clip_name, clip_length, clip_start) = {
                    let state = self.state.blocking_read();
                    let Some(track) = state.tracks.iter().find(|t| t.name == *track_idx) else {
                        return Task::none();
                    };
                    let Some(clip) = track.midi.clips.get(clip_idx) else {
                        return Task::none();
                    };
                    (clip.name.clone(), clip.length.max(1), clip.start)
                };
                let path = {
                    let clip_path = std::path::PathBuf::from(&clip_name);
                    if clip_path.is_absolute() {
                        clip_path
                    } else if let Some(session) = &self.session_dir {
                        session.join(&clip_name)
                    } else {
                        clip_path
                    }
                };
                match Self::parse_midi_clip_for_piano(&path, self.playback_rate_hz, clip_start) {
                    Ok((notes, controllers, sysexes, parsed_len)) => {
                        self.midi_clip_previews.insert(
                            (track_idx.clone(), clip_idx),
                            std::sync::Arc::new(notes.clone()),
                        );
                        self.pending_midi_clip_previews.remove(&(
                            track_idx.clone(),
                            clip_idx,
                            clip_name.clone(),
                        ));
                        {
                            let mut state = self.state.blocking_write();
                            state.piano = Some(PianoData {
                                track_idx: track_idx.clone(),
                                clip_index: clip_idx,
                                clip_start_samples: clip_start,
                                clip_length_samples: parsed_len.max(clip_length),
                                notes,
                                controllers,
                                sysexes,
                                midnam_note_names: HashMap::new(),
                            });
                            state.piano_selected_sysex = None;
                            state.piano_sysex_hex_input.clear();
                            state.piano_sysex_panel_open = false;
                            state.piano_sysex_scroll_y = 0.0;
                            state.pitch_correction = None;
                            state.pitch_correction_selected_points.clear();
                            state.pitch_correction_dragging_points = None;
                            state.pitch_correction_selecting_rect = None;
                            state.piano_scroll_x = 0.0;
                            state.piano_scroll_y = 0.0;
                            state.view = View::Piano;
                        }
                        {
                            let tasks = vec![
                                self.send(Action::TrackGetClapNoteNames {
                                    track_name: track_idx.clone(),
                                }),
                                self.sync_piano_scrollbars(),
                            ];
                            #[cfg(all(unix, not(target_os = "macos")))]
                            {
                                let mut tasks = tasks;
                                tasks.push(self.send(Action::TrackGetLv2Midnam {
                                    track_name: track_idx.clone(),
                                }));
                                return Task::batch(tasks);
                            }
                            #[cfg(not(all(unix, not(target_os = "macos"))))]
                            return Task::batch(tasks);
                        }
                    }
                    Err(e) => {
                        self.state.blocking_write().message =
                            format!("Failed to open MIDI clip '{}': {}", clip_name, e);
                    }
                }
            }
            Message::OpenTrackPlugins(ref track_name) => {
                {
                    let mut state = self.state.blocking_write();
                    state.view = View::TrackPlugins;
                    state.plugin_graph_track = Some(track_name.clone());
                    state.plugin_graph_clip = None;
                    Self::reset_track_plugin_view_state(&mut state);
                    if let Some((plugins, connections)) =
                        state.plugin_graphs_by_track.get(track_name).cloned()
                    {
                        state.plugin_graph_plugins = plugins.clone();
                        state.plugin_graph_connections = connections;
                        let track_positions = state
                            .plugin_graph_plugin_positions
                            .entry(track_name.clone())
                            .or_default();
                        for (idx, plugin) in plugins.iter().enumerate() {
                            let fallback = Point::new(200.0 + idx as f32 * 180.0, 220.0);
                            track_positions
                                .entry(plugin.instance_id)
                                .or_insert(fallback);
                        }
                    }
                }
                return self.open_track_plugins_followup(track_name.clone());
            }
            Message::OpenFolderConnections(ref folder_name) => {
                let mut state = self.state.blocking_write();
                state.view = View::Connections;
                state.connections_folder = Some(folder_name.clone());
                state.connection_view_selection = ConnectionViewSelection::None;
                state.message = format!("Opened folder connections: {}", folder_name);
                state.plugin_graph_track = Some(folder_name.clone());
                state.plugin_graph_clip = None;
                Self::reset_track_plugin_view_state(&mut state);
                if let Some((plugins, connections)) =
                    state.plugin_graphs_by_track.get(folder_name).cloned()
                {
                    state.plugin_graph_plugins = plugins.clone();
                    state.plugin_graph_connections = connections;
                    let track_positions = state
                        .plugin_graph_plugin_positions
                        .entry(folder_name.clone())
                        .or_default();
                    for (idx, plugin) in plugins.iter().enumerate() {
                        let fallback = Point::new(200.0 + idx as f32 * 180.0, 220.0);
                        track_positions
                            .entry(plugin.instance_id)
                            .or_insert(fallback);
                    }
                }

                let node_size = 140.0_f32;
                let spacing = 40.0_f32;
                let start_x = 80.0_f32;
                let folder_y = 80.0_f32;
                let children_y = folder_y + node_size + spacing;
                let canvas_w = self.size.width.max(600.0);
                let default_position = Point::new(100.0, 100.0);

                if let Some(folder_idx) = state.tracks.iter().position(|t| t.name == *folder_name)
                    && state.tracks[folder_idx].position == default_position
                {
                    state.tracks[folder_idx].position = Point::new(start_x, folder_y);

                    let child_names: Vec<String> = state
                        .tracks
                        .iter()
                        .filter(|t| t.parent_track.as_deref() == Some(folder_name.as_str()))
                        .map(|t| t.name.clone())
                        .collect();

                    if !child_names.is_empty() {
                        let cols = ((canvas_w - start_x * 2.0) / (node_size + spacing))
                            .floor()
                            .max(1.0) as usize;
                        let cols = cols.min(child_names.len());
                        for (i, name) in child_names.iter().enumerate() {
                            let col = i % cols;
                            let row = i / cols;
                            let x = start_x + col as f32 * (node_size + spacing);
                            let y = children_y + row as f32 * (node_size + spacing);
                            if let Some(child) = state.tracks.iter_mut().find(|t| t.name == *name)
                                && child.position == default_position
                            {
                                child.position = Point::new(x, y);
                            }
                        }
                    }
                }
                return self.send(Action::TrackGetPluginGraph {
                    track_name: folder_name.clone(),
                    include_state: false,
                });
            }
            Message::SessionViewConnectionsOpen(ref track_name) => {
                let mut state = self.state.blocking_write();
                state.session_view_connections = Some(track_name.clone());
                state.connection_view_selection = ConnectionViewSelection::None;
                state.message = format!("Opened live view connections: {}", track_name);
                return self.load_track_connection_view(&mut state, track_name);
            }
            Message::SessionViewConnectionsClose => {
                let mut state = self.state.blocking_write();
                state.session_view_connections = None;
                state.plugin_graph_track = None;
            }
            Message::EditorConnectionsOpen(ref track_name) => {
                let mut state = self.state.blocking_write();
                state.editor_connections = Some(track_name.clone());
                state.connection_view_selection = ConnectionViewSelection::None;
                state.message = format!("Opened editor connections: {}", track_name);
                return self.load_track_connection_view(&mut state, track_name);
            }
            Message::EditorConnectionsClose => {
                let mut state = self.state.blocking_write();
                state.editor_connections = None;
                state.plugin_graph_track = None;
            }
            Message::OpenJackConnections => {
                let mut state = self.state.blocking_write();
                state.view = View::JackConnections;
                state.jack_connecting = None;
                drop(state);
                return self.send(Action::JackGetGraph);
            }
            Message::CloseJackConnections => {
                let mut state = self.state.blocking_write();
                state.view = View::Connections;
                state.jack_connecting = None;
            }
            Message::JackPortClick { port, is_output } => {
                let mut state = self.state.blocking_write();
                if is_output {
                    state.jack_connecting = Some(port);
                    state.message = "Select a JACK input port to connect".to_string();
                    return Task::none();
                }
                let Some(source) = state.jack_connecting.take() else {
                    state.message = "Select a JACK output port first".to_string();
                    return Task::none();
                };
                drop(state);
                return self.send(Action::JackConnect {
                    source,
                    destination: port,
                });
            }
            Message::JackDisconnect {
                source,
                destination,
            } => {
                return self.send(Action::JackDisconnect {
                    source,
                    destination,
                });
            }
            Message::OpenHwPorts { input } => {
                let mut state = self.state.blocking_write();
                state.view = if input {
                    View::HwInputPorts
                } else {
                    View::HwOutputPorts
                };
            }
            Message::OpenClipPlugins {
                track_idx: _track_idx,
                clip_idx: _clip_idx,
            } => {
                #[cfg(all(unix, not(target_os = "macos")))]
                {
                    return self.open_clip_plugin_view(_track_idx.clone(), _clip_idx);
                }
                #[cfg(not(all(unix, not(target_os = "macos"))))]
                {
                    return Task::none();
                }
            }
            _ => {}
        }
        self.update_children(&message);
        Task::none()
    }

    fn handle_step_record_note(&mut self, channel: u8, pitch: u8, velocity: u8) -> Task<Message> {
        let piano = match self.state.blocking_read().piano.as_ref() {
            Some(p) => p.clone(),
            None => return Task::none(),
        };
        let track_name = piano.track_idx.clone();
        let clip_idx = piano.clip_index;
        let clip_length = piano.clip_length_samples;
        let insert_idx = piano.notes.len();

        let samples_per_beat = self.samples_per_beat();
        let samples_per_bar = self.samples_per_bar();
        let interval = match self.midi_snap_mode {
            SnapMode::NoSnap | SnapMode::Clips => (samples_per_beat / 4.0).max(1.0) as usize,
            mode => mode
                .interval_samples(samples_per_beat, samples_per_bar)
                .max(1.0) as usize,
        };

        let start_sample = self.step_recording_cursor_samples.clamp(0, clip_length);
        let end_sample = start_sample.saturating_add(interval).min(clip_length);
        if end_sample <= start_sample {
            return Task::none();
        }

        let note = maolan_engine::message::MidiNoteData {
            start_sample,
            length_samples: end_sample.saturating_sub(start_sample),
            pitch,
            velocity,
            channel,
        };

        self.step_recording_cursor_samples = end_sample;

        self.send(Action::InsertMidiNotes {
            track_name,
            clip_index: clip_idx,
            notes: vec![(insert_idx, note)],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::Point;

    #[test]
    fn active_workspace_cursor_prefers_editor_cursor() {
        let app = Maolan {
            state: {
                let state = crate::state::State::default();
                {
                    let mut guard = state.blocking_write();
                    guard.cursor = Point::new(10.0, 20.0);
                    guard.editor_cursor = Some(Point::new(30.0, 40.0));
                }
                state
            },
            ..Maolan::default()
        };

        assert_eq!(app.active_workspace_cursor(), Point::new(30.0, 40.0));
    }

    #[test]
    fn active_workspace_cursor_falls_back_to_global_cursor() {
        let app = Maolan {
            state: {
                let state = crate::state::State::default();
                state.blocking_write().cursor = Point::new(10.0, 20.0);
                state
            },
            ..Maolan::default()
        };

        assert_eq!(app.active_workspace_cursor(), Point::new(10.0, 20.0));
    }

    #[test]
    fn nearest_clip_edge_sample_prefers_candidate_within_threshold() {
        let (snapped, target, targets) = Maolan::nearest_clip_edge_sample(
            98.0,
            96.0,
            4.0,
            [("a", 70usize), ("b", 100usize), ("c", 130usize)]
                .into_iter()
                .map(|(name, edge)| {
                    (
                        crate::state::ClipId {
                            track_idx: name.to_string(),
                            clip_idx: 0,
                            kind: Kind::Audio,
                        },
                        edge,
                    )
                }),
        );
        assert_eq!(snapped, 100.0);
        assert_eq!(target.unwrap().track_idx, "b");
        assert_eq!(targets.len(), 1);
    }

    #[test]
    fn nearest_clip_edge_sample_falls_back_to_grid_when_no_edge_is_close() {
        let (snapped, target, targets) = Maolan::nearest_clip_edge_sample(
            98.0,
            96.0,
            1.0,
            [("a", 70usize), ("b", 100usize), ("c", 130usize)]
                .into_iter()
                .map(|(name, edge)| {
                    (
                        crate::state::ClipId {
                            track_idx: name.to_string(),
                            clip_idx: 0,
                            kind: Kind::Audio,
                        },
                        edge,
                    )
                }),
        );
        assert_eq!(snapped, 96.0);
        assert!(target.is_none());
        assert!(targets.is_empty());
    }

    #[test]
    fn snapped_clip_move_start_can_snap_using_right_edge() {
        let (snapped, target, targets) = Maolan::snapped_clip_move_start(
            70.0,
            20.0,
            64.0,
            2.0,
            std::iter::once((
                crate::state::ClipId {
                    track_idx: "target".to_string(),
                    clip_idx: 0,
                    kind: Kind::Audio,
                },
                90usize,
            )),
        );
        assert_eq!(snapped, 70.0);
        assert_eq!(target.unwrap().track_idx, "target");
        assert_eq!(targets.len(), 1);
    }

    #[test]
    fn snapped_clip_move_start_uses_clip_edge_over_grid_when_close() {
        let (snapped, target, targets) = Maolan::snapped_clip_move_start(
            91.0,
            12.0,
            96.0,
            2.0,
            [("left", 90usize), ("right", 140usize)]
                .into_iter()
                .map(|(name, edge)| {
                    (
                        crate::state::ClipId {
                            track_idx: name.to_string(),
                            clip_idx: 0,
                            kind: Kind::Audio,
                        },
                        edge,
                    )
                }),
        );
        assert_eq!(snapped, 90.0);
        assert_eq!(target.unwrap().track_idx, "left");
        assert_eq!(targets.len(), 1);
    }

    #[test]
    fn snapped_clip_move_start_does_not_snap_right_when_dragging_past_session_start() {
        let raw_start = -8.0;
        let snapped_start = 0.0;
        let (resolved_start, _target, _targets) = Maolan::snapped_clip_move_start(
            raw_start,
            100.0,
            snapped_start,
            20.0,
            std::iter::once((
                crate::state::ClipId {
                    track_idx: "other".to_string(),
                    clip_idx: 0,
                    kind: Kind::Audio,
                },
                105usize,
            )),
        );

        assert!(resolved_start > 0.0);
        let clamped_start = if raw_start < 0.0 {
            resolved_start.min(snapped_start)
        } else {
            resolved_start
        };
        assert_eq!(clamped_start, 0.0);
    }

    #[test]
    fn generate_audio_progress_message_uses_normalized_scale() {
        let mut app = Maolan {
            generate_audio_in_progress: true,
            ..Maolan::default()
        };

        let _ = app.update(Message::GenerateAudioProgress {
            progress: 0.42,
            operation: Some("Generating".to_string()),
        });

        assert!((app.generate_audio_progress - 0.42).abs() < f32::EPSILON);
        assert_eq!(app.generate_audio_operation.as_deref(), Some("Generating"));
    }

    #[test]
    fn modulator_target_confirm_adds_volume_target() {
        let mut app = Maolan {
            modulators: vec![crate::state::Modulator::new(1)],
            ..Maolan::default()
        };
        app.state.blocking_write().modulator_target_dialog =
            Some(crate::state::ModulatorTargetDialog {
                modulator_id: 1,
                track_name: "Drums".to_string(),
                target: crate::message::TrackAutomationTarget::Volume,
                min_input: "-90".to_string(),
                max_input: "20".to_string(),
                existing: false,
            });

        let _ = app.update(Message::ModulatorTargetConfirm);

        let m = app.modulators.iter().find(|m| m.id == 1).unwrap();
        assert_eq!(m.targets.len(), 1);
        assert_eq!(m.targets[0].track_name, "Drums");
        assert_eq!(
            m.targets[0].target,
            crate::message::TrackAutomationTarget::Volume
        );
        assert!((m.targets[0].min - -90.0).abs() < f32::EPSILON);
        assert!((m.targets[0].max - 20.0).abs() < f32::EPSILON);
        assert!(app.state.blocking_read().modulator_target_dialog.is_none());
    }
}
