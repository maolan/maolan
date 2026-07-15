use super::*;
use crate::state::SlotPlayState;

impl Maolan {
    pub(super) fn stop_workspace_playback(&mut self, session_mode: bool) -> Task<Message> {
        tracing::info!("stop_workspace_playback session_mode={}", session_mode);
        self.toolbar.update(&Message::TransportStop);
        self.playing = false;
        self.paused = false;
        self.pending_transport_position = None;
        self.last_playback_tick = None;
        self.track_automation_runtime.clear();
        self.touch_automation_overrides.clear();
        self.touch_active_keys.clear();
        self.latch_automation_overrides.clear();
        self.stop_recording_preview();

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
        let session_view_active =
            matches!(self.state.blocking_read().view, crate::state::View::Session);
        match message {
            Message::TransportPlay => {
                if session_view_active {
                    return self.start_live_session_play();
                }
                // Starting editor playback while the live session is playing
                // hands over: live playback stops, editor playback starts.
                let stop_live = if self.live_session_playing {
                    self.stop_live_session_play()
                } else {
                    Task::none()
                };
                self.toolbar.update(&message);
                let was_playing = self.playing;
                self.playing = true;
                self.paused = false;
                self.pending_transport_position = None;
                self.last_playback_tick = Some(Instant::now());
                if self.record_armed {
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
                    if self.live_session_playing {
                        return self.stop_live_session_play();
                    }
                    // Pause pressed in live view while the editor is playing
                    // stops editor playback.
                    if self.playing {
                        return self.stop_workspace_playback(false);
                    }
                    return Task::none();
                }
                if self.live_session_playing {
                    // Pause pressed in the editor while the live session is
                    // playing stops it (and any editor playback).
                    return self.stop_live_session_play();
                }
                self.toolbar.update(&message);
                let was_playing = self.playing;
                self.playing = true;
                self.paused = true;
                self.pending_transport_position = None;
                self.last_playback_tick = None;

                let mut tasks = vec![self.send(Action::SetClipPlaybackEnabled(false))];
                if !was_playing {
                    tasks.push(self.send(Action::Play));
                }
                Task::batch(tasks)
            }
            Message::TransportStop => {
                if session_view_active {
                    return self.stop_live_session_play();
                }
                if self.live_session_playing {
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
                self.transport_samples = 0.0;
                self.pending_transport_position = None;
                self.track_automation_runtime.clear();
                self.touch_automation_overrides.clear();
                self.touch_active_keys.clear();
                self.latch_automation_overrides.clear();
                self.send(Action::TransportPosition(0))
            }
            Message::JumpToEnd => {
                let end_sample = {
                    let state = self.state.blocking_read();
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
                self.transport_samples = end_sample as f64;
                self.pending_transport_position = None;
                self.send(Action::TransportPosition(end_sample))
            }
            Message::PlaybackTick => {
                if tokio::runtime::Handle::try_current().is_ok() {
                    if let Some(snapshot) = CLIENT.session_runtime_snapshot() {
                        if self.live_session_playing && self.record_armed {
                            self.capture_completed_live_session_clip_passes(
                                &snapshot.completed_clip_passes,
                            );
                        }
                        let mut state = self.state.blocking_write();
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
                        let was_running = self.playing && !self.paused;
                        self.playing = snapshot.playing;
                        self.paused = snapshot.playing && !snapshot.transport_running;
                        self.transport_samples = snapshot.sample as f64;
                        self.playback_rate_hz = self.playback_rate_hz.max(1.0);
                        self.tempo_input = format!("{:.2}", snapshot.tempo_bpm);
                        self.time_signature_num_input = snapshot.tsig_num.to_string();
                        self.time_signature_denom_input = snapshot.tsig_denom.to_string();
                        self.last_sent_tempo_bpm = Some(snapshot.tempo_bpm);
                        self.last_sent_time_signature =
                            Some((snapshot.tsig_num, snapshot.tsig_denom));
                        let running = self.playing && !self.paused;
                        if running {
                            self.last_playback_tick = Some(Instant::now());
                        } else if was_running {
                            self.last_playback_tick = None;
                        }
                    }
                }
                let mut now_sample = self.transport_samples.max(0.0) as usize;
                if self.playing
                    && !self.paused
                    && let Some(last) = self.last_playback_tick
                {
                    let now = Instant::now();
                    let delta_s = now.duration_since(last).as_secs_f64();
                    self.last_playback_tick = Some(now);
                    self.transport_samples += delta_s * self.playback_rate_hz;
                    if self
                        .pending_transport_position
                        .is_some_and(|(deadline, _)| now >= deadline)
                        && let Some((deadline, sample)) = self.pending_transport_position.take()
                    {
                        let overdue_s = now.duration_since(deadline).as_secs_f64();
                        self.transport_samples = sample as f64 + overdue_s * self.playback_rate_hz;
                    }
                    now_sample = self.transport_samples.max(0.0) as usize;
                }
                let mut tasks = Vec::new();
                {
                    let state = self.state.blocking_read();
                    let (bpm, num, den) = Self::timing_at_sample(&state, now_sample);
                    let tempo_changed = self
                        .last_sent_tempo_bpm
                        .is_none_or(|prev| (prev - bpm as f64).abs() > 0.0001);
                    let ts_changed = self
                        .last_sent_time_signature
                        .is_none_or(|prev| prev != (num as u16, den as u16));
                    if tempo_changed {
                        self.tempo_input = format!("{:.2}", bpm);
                    }
                    if ts_changed {
                        self.time_signature_num_input = num.to_string();
                        self.time_signature_denom_input = den.to_string();
                    }
                    if self
                        .last_sent_tempo_bpm
                        .is_none_or(|prev| (prev - bpm as f64).abs() > 0.0001)
                    {
                        self.last_sent_tempo_bpm = Some(bpm as f64);
                        tasks.push(self.send(Action::SetTempo(bpm as f64)));
                    }
                    if self
                        .last_sent_time_signature
                        .is_none_or(|prev| prev != (num as u16, den as u16))
                    {
                        self.last_sent_time_signature = Some((num as u16, den as u16));
                        tasks.push(self.send(Action::SetTimeSignature {
                            numerator: num as u16,
                            denominator: den as u16,
                        }));
                    }
                }
                if self.playing && !self.paused {
                    let tracks = {
                        let state = self.state.blocking_read();
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
                    let actions = self.collect_track_automation_actions(now_sample, &tracks);
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
                if !self.is_dirty()
                    || self.session_restore_in_progress
                    || self.pending_save_path.is_some()
                {
                    return Task::none();
                }
                let Some(autosave_root) = self.autosave_snapshot_root() else {
                    return Task::none();
                };
                let now = Instant::now();
                if self
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
                        self.last_autosave_snapshot = Some(now);
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
                        self.state.blocking_write().message = format!("Autosave failed: {e}");
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
                self.loop_enabled = normalized.is_some();
                self.loop_range_samples = normalized;
                self.clip_snap_targets.clear();
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
                self.punch_enabled = normalized.is_some();
                self.punch_range_samples = normalized;
                self.clip_snap_targets.clear();
                self.send(Action::SetPunchRange(normalized))
            }
            _ => Task::none(),
        }
    }
}
