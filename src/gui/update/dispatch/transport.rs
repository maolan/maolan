use super::*;
use crate::state::SlotPlayState;

impl Maolan {
    pub(super) fn stop_workspace_playback(&mut self, session_mode: bool) -> Task<Message> {
        tracing::info!("stop_workspace_playback session_mode={}", session_mode);
        self.toolbar.update(&Message::TransportStop);
        self.transport.playing = false;
        self.transport.paused = false;
        if !session_mode {
            self.transport.start_meter_stop_decay(&self.state);
        }
        self.transport.pending_transport_position = None;
        self.transport.last_playback_tick = None;
        self.automation.track_automation_runtime.clear();
        self.automation.touch_automation_overrides.clear();
        self.automation.touch_active_keys.clear();
        self.automation.latch_automation_overrides.clear();
        self.rec.stop_recording_preview();

        let first = self.send(Action::SetClipPlaybackEnabled(true));
        if session_mode {
            first
                .chain(self.send(Action::Stop))
                .chain(self.send(Action::SessionPlay))
        } else {
            first.chain(self.send(Action::Stop))
        }
    }

    pub(super) fn handle_transport_message(&mut self, message: Message) -> Task<Message> {
        let session_view_active = matches!(
            self.state.read().expect("state lock poisoned").view,
            crate::state::View::Session
        );
        match message {
            Message::TransportPlay => {
                if session_view_active {
                    return self.start_live_session_play();
                }
                // Starting editor playback while the live session is playing
                // hands over: live playback stops, editor playback starts.
                let stop_live = if self.transport.live_session_playing {
                    self.stop_live_session_play()
                } else {
                    Task::none()
                };
                self.toolbar.update(&message);
                let was_playing = self.transport.playing;
                self.transport.stop_meter_stop_decay();
                self.transport.playing = true;
                self.transport.paused = false;
                self.transport.pending_transport_position = None;
                self.transport.last_playback_tick = Some(Instant::now());
                if self.transport.record_armed {
                    self.start_recording_preview();
                }
                let mut tasks = vec![self.send(Action::SetClipPlaybackEnabled(true))];
                if !was_playing {
                    tasks.push(self.send(Action::Play));
                }
                stop_live.chain(Task::batch(tasks))
            }
            Message::TransportPause => {
                if session_view_active {
                    if self.transport.live_session_playing {
                        return self.stop_live_session_play();
                    }
                    // Pause pressed in live view while the editor is playing
                    // stops editor playback.
                    if self.transport.playing {
                        return self.stop_workspace_playback(false);
                    }
                    return Task::none();
                }
                if self.transport.live_session_playing {
                    // Pause pressed in the editor while the live session is
                    // playing stops it (and any editor playback).
                    return self.stop_live_session_play();
                }
                self.toolbar.update(&message);
                self.transport.stop_meter_stop_decay();
                self.transport.playing = true;
                self.transport.paused = true;
                self.transport.pending_transport_position = None;
                self.transport.last_playback_tick = None;
                self.send(Action::Pause)
            }
            Message::TransportStop => {
                if session_view_active {
                    return self.stop_live_session_play();
                }
                if self.transport.live_session_playing {
                    // Stop pressed in the editor while the live session is
                    // playing stops it (and any editor playback).
                    return self.stop_live_session_play();
                }
                self.stop_workspace_playback(false)
            }
            Message::TransportPanic => {
                self.toolbar.update(&message);
                self.send(Action::Panic)
            }
            Message::JumpToStart => {
                self.transport.transport_samples = 0.0;
                self.transport.pending_transport_position = None;
                self.automation.track_automation_runtime.clear();
                self.automation.touch_automation_overrides.clear();
                self.automation.touch_active_keys.clear();
                self.automation.latch_automation_overrides.clear();
                self.send(Action::TransportPosition(0))
            }
            Message::JumpToEnd => {
                let end_sample = {
                    let state = self.state.read().expect("state lock poisoned");
                    state
                        .tracks
                        .iter()
                        .flat_map(|track| {
                            let audio = track
                                .audio
                                .clips
                                .iter()
                                .map(|clip| clip.start.saturating_add(clip.length));
                            let midi = track
                                .midi
                                .clips
                                .iter()
                                .map(|clip| clip.start.saturating_add(clip.length));
                            audio.chain(midi)
                        })
                        .max()
                        .unwrap_or(0)
                };
                self.transport.transport_samples = end_sample as f64;
                self.transport.pending_transport_position = None;
                self.send(Action::TransportPosition(end_sample))
            }
            Message::PlaybackTick => {
                if tokio::runtime::Handle::try_current().is_ok() {
                    if let Some(snapshot) = CLIENT.session_runtime_snapshot() {
                        if self.transport.live_session_playing && self.transport.record_armed {
                            self.capture_completed_live_session_clip_passes(
                                &snapshot.completed_clip_passes,
                            );
                        }
                        let mut state = self.state.write().expect("state lock poisoned");
                        state.current_scene = snapshot.current_scene;
                        let mut active_slots = HashSet::with_capacity(snapshot.slots.len());
                        for slot in snapshot.slots {
                            let key = (slot.track_name, slot.scene_index);
                            active_slots.insert(key.clone());
                            let runtime = state.slot_runtimes.entry(key).or_default();
                            runtime.state = match slot.state {
                                EngineSessionSlotState::Stopped => SlotPlayState::Stopped,
                                EngineSessionSlotState::Queued => SlotPlayState::Queued,
                                EngineSessionSlotState::Playing => SlotPlayState::Playing,
                                EngineSessionSlotState::Stopping => SlotPlayState::Stopping,
                            };
                            runtime.play_position_samples = slot.play_position_samples;
                            runtime.elapsed_samples = slot.elapsed_samples;
                        }
                        for (key, runtime) in &mut state.slot_runtimes {
                            if active_slots.contains(key) {
                                continue;
                            }
                            runtime.state = SlotPlayState::Stopped;
                            runtime.play_position_samples = 0;
                            runtime.elapsed_samples = 0;
                        }
                    }
                    if let Some(snapshot) = CLIENT.transport_snapshot() {
                        let was_running = self.transport.playing && !self.transport.paused;
                        self.transport.playing = snapshot.playing;
                        self.transport.paused = snapshot.playing && !snapshot.transport_running;
                        self.transport.transport_samples = snapshot.sample as f64;
                        self.transport.playback_rate_hz = self.transport.playback_rate_hz.max(1.0);
                        self.timing.tempo_input = format!("{:.2}", snapshot.tempo_bpm);
                        self.timing.time_signature_num_input = snapshot.tsig_num.to_string();
                        self.timing.time_signature_denom_input = snapshot.tsig_denom.to_string();
                        self.timing.last_sent_tempo_bpm = Some(snapshot.tempo_bpm);
                        self.timing.last_sent_time_signature =
                            Some((snapshot.tsig_num, snapshot.tsig_denom));
                        let running = self.transport.playing && !self.transport.paused;
                        if running {
                            self.transport.last_playback_tick = Some(Instant::now());
                        } else if was_running {
                            self.transport.last_playback_tick = None;
                        }
                    }
                }
                let mut now_sample = self.transport.transport_samples.max(0.0) as usize;
                if self.transport.playing
                    && !self.transport.paused
                    && let Some(last) = self.transport.last_playback_tick
                {
                    let now = Instant::now();
                    let delta_s = now.duration_since(last).as_secs_f64();
                    self.transport.last_playback_tick = Some(now);
                    self.transport.transport_samples += delta_s * self.transport.playback_rate_hz;
                    if self
                        .transport
                        .pending_transport_position
                        .is_some_and(|(deadline, _)| now >= deadline)
                        && let Some((deadline, sample)) =
                            self.transport.pending_transport_position.take()
                    {
                        let overdue_s = now.duration_since(deadline).as_secs_f64();
                        self.transport.transport_samples =
                            sample as f64 + overdue_s * self.transport.playback_rate_hz;
                    }
                    now_sample = self.transport.transport_samples.max(0.0) as usize;
                }
                let mut tasks = Vec::new();
                {
                    let state = self.state.read().expect("state lock poisoned");
                    let (bpm, num, den) = Self::timing_at_sample(&state, now_sample);
                    let tempo_changed = self
                        .timing
                        .last_sent_tempo_bpm
                        .is_none_or(|prev| (prev - bpm as f64).abs() > 0.0001);
                    let ts_changed = self
                        .timing
                        .last_sent_time_signature
                        .is_none_or(|prev| prev != (num as u16, den as u16));
                    if tempo_changed {
                        self.timing.tempo_input = format!("{:.2}", bpm);
                    }
                    if ts_changed {
                        self.timing.time_signature_num_input = num.to_string();
                        self.timing.time_signature_denom_input = den.to_string();
                    }
                    if self
                        .timing
                        .last_sent_tempo_bpm
                        .is_none_or(|prev| (prev - bpm as f64).abs() > 0.0001)
                    {
                        self.timing.last_sent_tempo_bpm = Some(bpm as f64);
                        tasks.push(self.send(Action::SetTempo(bpm as f64)));
                    }
                    if self
                        .timing
                        .last_sent_time_signature
                        .is_none_or(|prev| prev != (num as u16, den as u16))
                    {
                        self.timing.last_sent_time_signature = Some((num as u16, den as u16));
                        tasks.push(self.send(Action::SetTimeSignature {
                            numerator: num as u16,
                            denominator: den as u16,
                        }));
                    }
                }
                if self.transport.playing && !self.transport.paused {
                    let tracks = {
                        let state = self.state.read().expect("state lock poisoned");
                        state
                            .tracks
                            .iter()
                            .map(|track| AutomationTrackView {
                                name: track.name.clone(),
                                automation_mode: track.automation_mode,
                                automation_lanes: track.automation_lanes.clone(),
                                frozen: track.frozen,
                            })
                            .collect::<Vec<_>>()
                    };
                    let actions = self
                        .automation
                        .collect_track_automation_actions(now_sample, &tracks);
                    if !actions.is_empty() {
                        tasks.extend(actions.into_iter().map(|a| self.send(a)));
                    }
                }
                if !tasks.is_empty() {
                    return Task::batch(tasks);
                }
                Task::none()
            }
            Message::AutosaveSnapshotTick => {
                if !self.session_ops.is_dirty()
                    || self.session_ops.session_restore_in_progress
                    || self.pending.pending_save_path.is_some()
                {
                    return Task::none();
                }
                let Some(autosave_root) = self.autosave_snapshot_root() else {
                    return Task::none();
                };
                let now = Instant::now();
                if self
                    .session_ops
                    .last_autosave_snapshot
                    .is_some_and(|last| now.duration_since(last) < AUTOSAVE_SNAPSHOT_INTERVAL)
                {
                    return Task::none();
                }
                let stamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let snapshot_dir = autosave_root.join("snapshots").join(format!("{stamp}"));
                match self.save(snapshot_dir.to_string_lossy().to_string()) {
                    Ok(()) => {
                        self.session_ops.last_autosave_snapshot = Some(now);
                        let branch = self.session_branch.clone();
                        let mut snapshots = self
                            .session_dir
                            .as_ref()
                            .map(|path| Self::list_autosave_snapshots_for(path, &branch))
                            .unwrap_or_default();
                        if snapshots.len()
                            > crate::consts::gui_update_dispatch_transport::AUTOSAVE_KEEP_COUNT
                        {
                            snapshots.sort();
                            let remove_count = snapshots.len().saturating_sub(
                                crate::consts::gui_update_dispatch_transport::AUTOSAVE_KEEP_COUNT,
                            );
                            for stale in snapshots.into_iter().take(remove_count) {
                                if let Err(_err) = fs::remove_dir_all(&stale) {}
                            }
                        }
                    }
                    Err(e) => {
                        self.state.write().expect("state lock poisoned").message =
                            format!("Autosave failed: {e}");
                    }
                }
                Task::none()
            }
            Message::SetLoopRange(range) => {
                let normalized = range.and_then(|(start, end)| {
                    if end > start {
                        Some((start, end))
                    } else {
                        None
                    }
                });
                self.transport.loop_enabled = normalized.is_some();
                self.transport.loop_range_samples = normalized;
                self.drag.clip_snap_targets.clear();
                self.send(Action::SetLoopRange(normalized))
            }
            Message::SetPunchRange(range) => {
                let normalized = range.and_then(|(start, end)| {
                    if end > start {
                        Some((start, end))
                    } else {
                        None
                    }
                });
                self.transport.punch_enabled = normalized.is_some();
                self.transport.punch_range_samples = normalized;
                self.drag.clip_snap_targets.clear();
                self.send(Action::SetPunchRange(normalized))
            }
            _ => Task::none(),
        }
    }
}

impl Maolan {
    pub(super) fn handle_transport_control_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SetSnapMode(mode) => {
                self.timing.snap_mode = mode;
            }
            Message::SetMidiSnapMode(mode) => {
                self.timing.midi_snap_mode = mode;
            }
            Message::ToggleStepRecording => {
                self.transport.step_recording_active = !self.transport.step_recording_active;
                let enabled = self.transport.step_recording_active;
                if enabled {
                    self.transport.step_recording_cursor_samples = 0;
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
                self.drag.clip_snap_targets = targets.clone();
            }
            Message::RecordingPreviewTick
                if self.transport.playing
                    && !self.transport.paused
                    && self.transport.record_armed
                    && self.rec.recording_preview_start_sample.is_some() =>
            {
                let sample = self.transport.transport_samples.max(0.0) as usize;
                if self.transport.punch_enabled
                    && let Some((punch_start, punch_end)) = self.transport.punch_range_samples
                    && punch_end > punch_start
                    && (sample < punch_start || sample > punch_end)
                {
                    self.rec.recording_preview_sample = None;
                } else {
                    self.rec.recording_preview_sample = Some(sample);
                }
            }
            Message::RecordingPreviewPeaksTick
                if self.transport.playing
                    && !self.transport.paused
                    && self.transport.record_armed
                    && self.rec.recording_preview_start_sample.is_some() =>
            {
                let sample = self.transport.transport_samples.max(0.0) as usize;
                if self.transport.punch_enabled
                    && let Some((punch_start, punch_end)) = self.transport.punch_range_samples
                    && punch_end > punch_start
                    && (sample < punch_start || sample >= punch_end)
                {
                    return Task::none();
                }
                let peaks = &mut self.rec.recording_preview_peaks;
                let state = self.state.read().expect("state lock poisoned");
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
            Message::TransportRecordToggle => {
                self.toolbar.update(&message);
                if self.transport.record_armed {
                    self.transport.record_armed = false;
                    self.transport.pending_record_after_save = false;
                    self.drag.session_slot_record_target = None;
                    self.transport.live_session_record_start_sample = None;
                    self.transport.recorded_live_session_clip_passes.clear();
                    self.rec.stop_recording_preview();
                    return self.send(Action::SetRecordEnabled(false));
                }
                if self.session_dir.is_none() {
                    self.transport.pending_record_after_save = true;
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
                self.transport.record_armed = true;
                self.transport.live_session_record_start_sample =
                    if self.transport.live_session_playing {
                        Some(
                            CLIENT
                                .session_runtime_snapshot()
                                .map(|snapshot| snapshot.session_sample)
                                .unwrap_or(0),
                        )
                    } else {
                        Some(0)
                    };
                self.transport.recorded_live_session_clip_passes.clear();
                if self.transport.playing {
                    self.start_recording_preview();
                }
                return self.send(Action::SetRecordEnabled(true));
            }
            _ => {}
        }
        self.update_children(&message);
        Task::none()
    }
}
