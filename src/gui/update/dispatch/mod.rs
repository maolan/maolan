use super::*;
mod core;
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

impl Maolan {
    pub fn update(&mut self, message: Message) -> Task<Message> {
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
        if let Some(task) = self.handle_track_selection_message(message.clone()) {
            return task;
        }
        if let Some(task) = self.handle_plugin_message(message.clone()) {
            return task;
        }
        if !matches!(message, Message::WindowCloseRequested) {
            self.close_confirm_pending = false;
        }
        if matches!(
            message,
            Message::ClipRenameShow { .. }
                | Message::ClipToggleFade { .. }
                | Message::ClipSetMuted { .. }
                | Message::ClipWarpReset { .. }
                | Message::ClipWarpHalfSpeed { .. }
                | Message::ClipWarpDoubleSpeed { .. }
                | Message::ClipWarpAddMarker { .. }
                | Message::ClipSetActiveTake { .. }
                | Message::ClipCycleActiveTake { .. }
                | Message::ClipUnmuteTakesInRange { .. }
                | Message::ClipTakeLanePinToggle { .. }
                | Message::ClipTakeLaneLockToggle { .. }
                | Message::ClipTakeLaneMove { .. }
        ) {
            self.state.blocking_write().clip_context_menu = None;
        }
        if matches!(
            message,
            Message::TrackAutomationAddLane { .. }
                | Message::TrackRenameShow(_)
                | Message::TrackAutomationToggle { .. }
                | Message::TrackAutomationCycleMode { .. }
                | Message::TrackTemplateSaveShow(_)
                | Message::TrackFreezeToggle { .. }
                | Message::TrackFreezeFlatten { .. }
                | Message::TrackSetVcaMaster { .. }
                | Message::TrackAuxSendLevelAdjust { .. }
                | Message::TrackAuxSendPanAdjust { .. }
                | Message::TrackAuxSendTogglePrePost { .. }
                | Message::TrackMidiLearnArm { .. }
                | Message::TrackMidiLearnClear { .. }
        ) {
            self.state.blocking_write().track_context_menu = None;
        }
        match message {
            Message::Show(ref show) => return self.handle_show_message(show),
            Message::AddTrackFromTemplate { .. }
            | Message::NewFromTemplate(_)
            | Message::NewSession
            | Message::Request(_)
            | Message::MeterPollTick => return self.handle_session_message(message),
            Message::Cancel => self.modal = None,
            Message::TransportPlay
            | Message::TransportPause
            | Message::TransportStop
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
            | Message::TimeSignatureNumeratorInputChanged(_)
            | Message::TimeSignatureDenominatorInputChanged(_)
            | Message::TimeSignatureInputCommit => {
                return self.handle_timing_message(message);
            }
            Message::SetSnapMode(mode) => {
                self.snap_mode = mode;
            }
            Message::RecordingPreviewTick => {
                if self.playing
                    && !self.paused
                    && self.record_armed
                    && self.recording_preview_start_sample.is_some()
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
            }
            Message::RecordingPreviewPeaksTick => {
                if self.playing
                    && !self.paused
                    && self.record_armed
                    && self.recording_preview_start_sample.is_some()
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
            }
            Message::ZoomVisibleBarsChanged(value) => {
                self.zoom_visible_bars = value.clamp(1.0, 256.0);
                return self.sync_editor_scrollbars();
            }
            Message::EditorScrollChanged { x, y } => {
                let x = x.clamp(0.0, 1.0);
                let y = y.clamp(0.0, 1.0);
                let x_changed = (self.editor_scroll_x - x).abs() > 0.0005;
                let y_changed = (self.editor_scroll_y - y).abs() > 0.0005;
                if x_changed || y_changed {
                    self.editor_scroll_x = x;
                    self.editor_scroll_y = y;
                    return self.sync_editor_scrollbars();
                }
            }
            Message::EditorScrollXChanged(value) => {
                let x = value.clamp(0.0, 1.0);
                if (self.editor_scroll_x - x).abs() > 0.0005 {
                    self.editor_scroll_x = x;
                    return self.sync_editor_scrollbars();
                }
            }
            Message::EditorScrollYChanged(value) => {
                let y = value.clamp(0.0, 1.0);
                if (self.editor_scroll_y - y).abs() > 0.0005 {
                    self.editor_scroll_y = y;
                    return self.sync_editor_scrollbars();
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
            Message::PianoControllerLaneSelected(lane) => {
                let mut state = self.state.blocking_write();
                state.piano_controller_lane = lane;
                state.piano_sysex_panel_open =
                    matches!(lane, crate::message::PianoControllerLane::SysEx);
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
                    // Toggle selection with shift
                    if state.piano_selected_notes.contains(&note_index) {
                        state.piano_selected_notes.remove(&note_index);
                    } else {
                        state.piano_selected_notes.insert(note_index);
                    }
                } else {
                    // Keep current multi-selection if clicking inside it, otherwise replace selection.
                    if !state.piano_selected_notes.contains(&note_index) {
                        state.piano_selected_notes.clear();
                        state.piano_selected_notes.insert(note_index);
                    }
                }

                // Start dragging if notes are selected
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
                                    (note.start_sample as i64 + delta_samples).max(0) as usize;
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

                        // Modify the notes in place
                        for &note_idx in &dragging.note_indices {
                            if let Some(note) = piano.notes.get_mut(note_idx) {
                                let new_start =
                                    (note.start_sample as i64 + delta_samples).max(0) as usize;
                                let new_pitch =
                                    (note.pitch as i16 + delta_pitch as i16).clamp(0, 127) as u8;
                                note.start_sample = new_start;
                                note.pitch = new_pitch;
                            }
                        }

                        // Build new notes for engine action
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

                    let original = &resizing.original_note;
                    let original_end = original
                        .start_sample
                        .saturating_add(original.length_samples)
                        .max(1);
                    let (new_start, new_len) = if resizing.resize_start {
                        let max_start = original_end.saturating_sub(1) as i64;
                        let start =
                            (original.start_sample as i64 + delta_samples).clamp(0, max_start);
                        let start = start as usize;
                        (start, original_end.saturating_sub(start).max(1))
                    } else {
                        let min_end = original.start_sample.saturating_add(1) as i64;
                        let end = (original_end as i64 + delta_samples).max(min_end) as usize;
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
                let drawn_channels: HashSet<u8> = controllers.iter().map(|c| c.channel).collect();

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
                    if !drawn_channels.contains(&ctrl.channel) {
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

                    // Update selection based on rectangle
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
                            if note.pitch > 119 {
                                continue;
                            }
                            let y_idx = (119 - note.pitch) as usize;
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
                let snap_interval = match self.snap_mode {
                    crate::message::SnapMode::NoSnap => 1.0,
                    crate::message::SnapMode::Bar => samples_per_bar.max(1.0),
                    crate::message::SnapMode::Beat => samples_per_beat.max(1.0),
                    crate::message::SnapMode::Eighth => (samples_per_beat / 2.0).max(1.0),
                    crate::message::SnapMode::Sixteenth => (samples_per_beat / 4.0).max(1.0),
                    crate::message::SnapMode::ThirtySecond => (samples_per_beat / 8.0).max(1.0),
                    crate::message::SnapMode::SixtyFourth => (samples_per_beat / 16.0).max(1.0),
                };
                let snap_sample = |sample: f32| -> usize {
                    if matches!(self.snap_mode, crate::message::SnapMode::NoSnap) {
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
                let pitch_row = pitch_row.clamp(0.0, 119.0) as usize;
                let pitch = 119_u8.saturating_sub(pitch_row as u8);

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
            Message::PianoQuantizeSelectedNotes => {
                let interval = self.snap_interval_samples().max(1);
                let strength = self
                    .state
                    .blocking_read()
                    .piano_quantize_strength
                    .clamp(0.0, 1.0);
                return self.selected_piano_notes_edit(move |_idx, note| {
                    let snapped =
                        ((note.start_sample.saturating_add(interval / 2)) / interval) * interval;
                    let mut out = note.clone();
                    if strength >= 0.999 {
                        out.start_sample = snapped;
                    } else {
                        let cur = note.start_sample as f32;
                        let dst = snapped as f32;
                        out.start_sample = (cur + (dst - cur) * strength).round().max(0.0) as usize;
                    }
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
            Message::PianoQuantizeStrengthChanged(value) => {
                self.state.blocking_write().piano_quantize_strength = value.clamp(0.0, 1.0);
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
            Message::MixerResizeHover(hovered) => {
                self.mixer_resize_hovered = hovered;
            }
            Message::TransportRecordToggle => {
                self.toolbar.update(&message);
                if self.record_armed {
                    self.record_armed = false;
                    self.pending_record_after_save = false;
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
                if !self.session_restore_in_progress && history::should_record(a) {
                    self.has_unsaved_changes = true;
                }
                let mut refresh_midi_clip_previews = false;
                if let Some(task) = self.handle_response_freeze_meter_action(a) {
                    return task;
                }
                if let Some(task) = self.handle_response_session_state_action(a) {
                    return task;
                }
                let handled_response_state = self.handle_response_engine_state_action(a);
                let handled_response_track = self.handle_response_track_action(a);
                let handled_response_timing = self.handle_response_timing_state_action(a);
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
                        } => {
                            let mut state = self.state.blocking_write();
                            state.tracks.push(Track::new(
                                name.clone(),
                                0.0,
                                *audio_ins,
                                *audio_outs,
                                *midi_ins,
                                *midi_outs,
                            ));

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
                                let min_h = track.min_height_for_layout().max(60.0);
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

                            // Check if we need to load a template for this track
                            let pending_template = state.pending_track_template_load.clone();
                            drop(state);

                            if let Some((template_track_name, template_name)) = pending_template
                                && template_track_name == *name
                            {
                                self.state.blocking_write().pending_track_template_load = None;
                                return self.load_track_template(name.clone(), template_name);
                            }

                            if !matches!(self.modal, Some(Show::AutosaveRecovery)) {
                                self.modal = None;
                            }
                        }
                        Action::RemoveTrack(name) => {
                            let mut state = self.state.blocking_write();

                            if let Some(removed_idx) =
                                state.tracks.iter().position(|t| t.name == *name)
                            {
                                state.connections.retain(|conn| {
                                    conn.from_track != *name && conn.to_track != *name
                                });
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
                                    if track.vca_master.as_deref() == Some(name.as_str()) {
                                        track.vca_master = None;
                                    }
                                    track.aux_sends.retain(|send| send.aux_track != *name);
                                }
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
                            name,
                            track_name,
                            start,
                            length,
                            offset,
                            input_channel,
                            muted,
                            kind,
                            fade_enabled,
                            fade_in_samples,
                            fade_out_samples,
                            warp_markers,
                        } => {
                            let mut audio_peaks = crate::state::ClipPeaks::default();
                            let mut max_length_samples = offset.saturating_add(*length);
                            let mut wav_path_for_rebuild: Option<std::path::PathBuf> = None;
                            let mut peaks_path_for_load: Option<std::path::PathBuf> = None;
                            let mut loaded_bins = 0usize;
                            if *kind == Kind::Audio {
                                let key = Self::audio_clip_key(
                                    track_name, name, *start, *length, *offset,
                                );
                                audio_peaks =
                                    self.pending_audio_peaks.remove(&key).unwrap_or_default();
                                peaks_path_for_load = self.pending_peak_file_loads.remove(&key);
                                loaded_bins = audio_peaks.iter().map(Vec::len).max().unwrap_or(0);
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
                                            name: name.clone(),
                                            start: *start,
                                            length: *length,
                                            offset: *offset,
                                            input_channel: *input_channel,
                                            muted: *muted,
                                            max_length_samples,
                                            peaks_file: None,
                                            peaks: audio_peaks,
                                            fade_enabled: *fade_enabled,
                                            fade_in_samples: *fade_in_samples,
                                            fade_out_samples: *fade_out_samples,
                                            warp_markers: warp_markers.clone(),
                                            take_lane_override: None,
                                            take_lane_pinned: false,
                                            take_lane_locked: false,
                                        });
                                    }
                                    Kind::MIDI => {
                                        track.midi.clips.push(crate::state::MIDIClip {
                                            name: name.clone(),
                                            start: *start,
                                            length: *length,
                                            offset: *offset,
                                            input_channel: *input_channel,
                                            muted: *muted,
                                            max_length_samples,
                                            fade_enabled: *fade_enabled,
                                            fade_in_samples: *fade_in_samples,
                                            fade_out_samples: *fade_out_samples,
                                            take_lane_override: None,
                                            take_lane_pinned: false,
                                            take_lane_locked: false,
                                        });
                                    }
                                }
                            }
                            drop(state);
                            if *kind == Kind::Audio && loaded_bins < 32_768 {
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
                        Action::SetAudioClipWarpMarkers {
                            track_name,
                            clip_index,
                            warp_markers,
                        } => {
                            let mut state = self.state.blocking_write();
                            if let Some(track) =
                                state.tracks.iter_mut().find(|t| &t.name == track_name)
                                && let Some(clip) = track.audio.clips.get_mut(*clip_index)
                            {
                                clip.warp_markers = warp_markers.clone();
                            }
                        }
                        Action::RemoveClip {
                            track_name,
                            kind,
                            clip_indices,
                        } => {
                            let mut state = self.state.blocking_write();
                            if let Some(track) =
                                state.tracks.iter_mut().find(|t| &t.name == track_name)
                            {
                                match kind {
                                    Kind::Audio => {
                                        let mut indices = clip_indices.clone();
                                        indices.sort_unstable();
                                        indices.dedup();
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
                            plugin_path,
                        } => {
                            let plugin_name = std::path::Path::new(plugin_path)
                                .file_stem()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_else(|| plugin_path.clone());
                            {
                                let mut state = self.state.blocking_write();
                                let entry = state
                                    .clap_plugins_by_track
                                    .entry(track_name.clone())
                                    .or_default();
                                if !entry
                                    .iter()
                                    .any(|existing| existing.eq_ignore_ascii_case(plugin_path))
                                {
                                    entry.push(plugin_path.clone());
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
                            plugin_path,
                        } => {
                            {
                                let mut state = self.state.blocking_write();
                                if let Some(entry) = state.clap_plugins_by_track.get_mut(track_name)
                                    && let Some(pos) = entry.iter().position(|existing| {
                                        existing.eq_ignore_ascii_case(plugin_path)
                                    })
                                {
                                    entry.remove(pos);
                                }
                                if let Some(states) = state.clap_states_by_track.get_mut(track_name)
                                {
                                    states.remove(plugin_path);
                                }
                            }
                            let plugin_name = std::path::Path::new(plugin_path)
                                .file_stem()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_else(|| plugin_path.clone());
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
                            plugin_path,
                            state: clap_state,
                            ..
                        } => {
                            let mut state = self.state.blocking_write();
                            state
                                .clap_states_by_track
                                .entry(track_name.clone())
                                .or_default()
                                .insert(plugin_path.clone(), clap_state.clone());
                        }
                        Action::TrackClapParameters {
                            track_name,
                            instance_id,
                            parameters,
                        } => {
                            if self
                                .pending_add_clap_automation_instances
                                .remove(&(track_name.clone(), *instance_id))
                            {
                                let mut state = self.state.blocking_write();
                                if let Some(track) =
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
                                    track.height = track.min_height_for_layout().max(60.0);
                                    state.message = format!(
                                        "Added {} CLAP automation lanes on '{}'",
                                        parameters.len(),
                                        track_name
                                    );
                                }
                            }
                        }
                        Action::TrackVst3Parameters {
                            track_name,
                            instance_id,
                            parameters,
                        } => {
                            if self
                                .pending_add_vst3_automation_instances
                                .remove(&(track_name.clone(), *instance_id))
                            {
                                let mut state = self.state.blocking_write();
                                if let Some(track) =
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
                                    track.height = track.min_height_for_layout().max(60.0);
                                    state.message = format!(
                                        "Added {} VST3 automation lanes on '{}'",
                                        parameters.len(),
                                        track_name
                                    );
                                }
                            }
                        }
                        #[cfg(any(target_os = "windows", target_os = "macos"))]
                        Action::TrackSnapshotAllClapStates { track_name: _ } => {}
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
                            #[cfg(any(target_os = "windows", target_os = "macos"))]
                            if self.pending_save_path.is_some() {
                                self.pending_save_vst3_states
                                    .remove(&(track_name.clone(), *instance_id));
                                if self.pending_save_ready() {
                                    let path = self.pending_save_path.take().unwrap_or_default();
                                    let is_template = self.pending_save_is_template;
                                    self.pending_save_is_template = false;
                                    if !path.is_empty() {
                                        if is_template {
                                            if let Err(e) = self.save_template(path.clone()) {
                                                error!("{}", e);
                                                self.state.blocking_write().message =
                                                    format!("Failed to save template: {}", e);
                                            } else {
                                                self.state.blocking_write().message =
                                                    "Template saved".to_string();
                                                let templates = crate::gui::scan_templates();
                                                self.state.blocking_write().available_templates =
                                                    templates.clone();
                                                self.menu.update_templates(templates);
                                            }
                                        } else if let Err(e) = self.save(path.clone()) {
                                            error!("{}", e);
                                        } else {
                                            return self.send(Action::SetSessionPath(path));
                                        }
                                    }
                                }
                            }
                            if let Some(pending) = self.pending_vst3_ui_open.clone()
                                && &pending.track_name == track_name
                                && pending.instance_id == *instance_id
                            {
                                let (sample_rate_hz, block_size) = {
                                    let st = self.state.blocking_read();
                                    (self.playback_rate_hz.max(1.0), st.oss_period_frames.max(1))
                                };
                                if let Err(e) = self.vst3_ui_host.open_editor(
                                    &pending.plugin_path,
                                    &pending.plugin_name,
                                    &pending.plugin_id,
                                    sample_rate_hz,
                                    block_size,
                                    pending.audio_inputs,
                                    pending.audio_outputs,
                                    Some(state.clone()),
                                ) {
                                    self.state.blocking_write().message = e;
                                }
                                self.pending_vst3_ui_open = None;
                            }
                        }
                        #[cfg(target_os = "windows")]
                        Action::TrackVst3EditorHandle {
                            track_name: _,
                            instance_id: _,
                            controller_handle,
                            title,
                        } => {
                            if let Err(e) = self
                                .vst3_ui_host
                                .open_editor_from_handle(*controller_handle, title)
                            {
                                self.state.blocking_write().message = e;
                            }
                        }
                        #[cfg(target_os = "windows")]
                        Action::TrackLoadVst3Plugin { track_name, .. }
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
                        #[cfg(all(unix, not(target_os = "macos")))]
                        Action::TrackLv2PluginControls {
                            track_name,
                            instance_id,
                            controls,
                            instance_access_handle,
                        } => {
                            if self
                                .pending_add_lv2_automation_instances
                                .remove(&(track_name.clone(), *instance_id))
                            {
                                let mut state = self.state.blocking_write();
                                if let Some(track) =
                                    state.tracks.iter_mut().find(|t| t.name == *track_name)
                                {
                                    for control in controls {
                                        let target = TrackAutomationTarget::Lv2Parameter {
                                            instance_id: *instance_id,
                                            index: control.index,
                                            min: control.min,
                                            max: control.max,
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
                                    track.height = track.min_height_for_layout().max(60.0);
                                    state.message = format!(
                                        "Added {} LV2 automation lanes on '{}'",
                                        controls.len(),
                                        track_name
                                    );
                                }
                                return Task::none();
                            }
                            let (plugin_name, plugin_uri) = {
                                let state = self.state.blocking_read();
                                state
                                    .plugin_graph_plugins
                                    .iter()
                                    .find(|plugin| plugin.instance_id == *instance_id)
                                    .map(|plugin| (plugin.name.clone(), plugin.uri.clone()))
                                    .unwrap_or_else(|| {
                                        (format!("LV2 #{instance_id}"), String::new())
                                    })
                            };
                            if let Err(err) = self.lv2_ui_host.open_editor(
                                track_name.clone(),
                                *instance_id,
                                plugin_name,
                                plugin_uri,
                                controls.clone(),
                                *instance_access_handle,
                                CLIENT.clone(),
                            ) {
                                self.state.blocking_write().message = err;
                            }
                        }
                        #[cfg(any(target_os = "windows", all(unix, not(target_os = "macos"))))]
                        Action::TrackPluginGraph {
                            track_name,
                            plugins,
                            connections,
                        } => {
                            let mut state = self.state.blocking_write();
                            state
                                .plugin_graphs_by_track
                                .insert(track_name.clone(), (plugins.clone(), connections.clone()));
                            if state.plugin_graph_track.as_deref() == Some(track_name.as_str()) {
                                state.plugin_graph_track = Some(track_name.clone());
                                state.plugin_graph_plugins = plugins.clone();
                                state.plugin_graph_connections = connections.clone();
                                state.plugin_graph_selected_connections.clear();
                                state.plugin_graph_selected_plugin = state
                                    .plugin_graph_selected_plugin
                                    .filter(|id| plugins.iter().any(|p| p.instance_id == *id));
                                let mut new_positions = std::collections::HashMap::new();
                                for (idx, plugin) in plugins.iter().enumerate() {
                                    let fallback = Point::new(200.0 + idx as f32 * 180.0, 220.0);
                                    let pos = state
                                        .plugin_graph_plugin_positions
                                        .get(&plugin.instance_id)
                                        .copied()
                                        .unwrap_or(fallback);
                                    new_positions.insert(plugin.instance_id, pos);
                                }
                                state.plugin_graph_plugin_positions = new_positions;
                            }
                            drop(state);

                            let pending_queries =
                                self.queue_pending_graph_automation_queries(track_name, plugins);
                            if !pending_queries.is_empty() {
                                return Task::batch(pending_queries);
                            }

                            if self.pending_save_path.is_some() {
                                self.pending_save_tracks.remove(track_name);
                                if self.pending_save_ready() {
                                    let path = self.pending_save_path.take().unwrap_or_default();
                                    let is_template = self.pending_save_is_template;
                                    self.pending_save_is_template = false;
                                    if !path.is_empty() {
                                        if is_template {
                                            if let Err(e) = self.save_template(path.clone()) {
                                                error!("{}", e);
                                                self.state.blocking_write().message =
                                                    format!("Failed to save template: {}", e);
                                            } else {
                                                self.state.blocking_write().message =
                                                    "Template saved".to_string();
                                                // Rescan templates and update menu
                                                let templates = crate::gui::scan_templates();
                                                self.state.blocking_write().available_templates =
                                                    templates.clone();
                                                self.menu.update_templates(templates);
                                            }
                                        } else {
                                            // Check if this is a single-track template save
                                            // (path contains /track_templates/)
                                            if path.contains("/track_templates/") {
                                                return self
                                                    .save_track_as_template(track_name, path);
                                            } else if let Err(e) = self.save(path.clone()) {
                                                error!("{}", e);
                                            } else {
                                                return self.send(Action::SetSessionPath(path));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Action::RenameTrack { old_name, new_name } => {
                            let mut state = self.state.blocking_write();
                            // Update track name in GUI state
                            if let Some(track) =
                                state.tracks.iter_mut().find(|t| t.name == *old_name)
                            {
                                track.name = new_name.clone();
                            }
                            // Update selected tracks
                            if state.selected.remove(old_name) {
                                state.selected.insert(new_name.clone());
                            }
                            // Update connection view selection
                            if let crate::state::ConnectionViewSelection::Tracks(tracks) =
                                &mut state.connection_view_selection
                                && tracks.remove(old_name)
                            {
                                tracks.insert(new_name.clone());
                            }
                            // Update connections
                            for conn in &mut state.connections {
                                if conn.from_track == *old_name {
                                    conn.from_track = new_name.clone();
                                }
                                if conn.to_track == *old_name {
                                    conn.to_track = new_name.clone();
                                }
                            }
                            // Update LV2 graph track reference
                            if state.plugin_graph_track.as_deref() == Some(old_name) {
                                state.plugin_graph_track = Some(new_name.clone());
                            }
                            // Update LV2 graphs by track
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
                            for track in &mut state.tracks {
                                if track.vca_master.as_deref() == Some(old_name.as_str()) {
                                    track.vca_master = Some(new_name.clone());
                                }
                            }
                            state.message = format!("Renamed track to '{}'", new_name);
                            refresh_midi_clip_previews = true;
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
                self.pending_diagnostics_bundle_export = false;
                self.diagnostics_bundle_wait_session_report = false;
                self.diagnostics_bundle_wait_midi_report = false;
                self.state.blocking_write().message = e.clone();
                error!("Engine error: {e}");
            }
            Message::TrackSetVcaMaster {
                ref track_name,
                ref master_track,
            } => {
                if master_track.as_deref() == Some(track_name.as_str()) {
                    self.state.blocking_write().message =
                        "Track cannot be its own VCA master".to_string();
                    return Task::none();
                }
                return self.send(Action::TrackSetVcaMaster {
                    track_name: track_name.clone(),
                    master_track: master_track.clone(),
                });
            }
            Message::TrackCreateAuxReturnFromSelection => {
                let (selected_tracks, max_outs, max_midi_outs, existing_names) = {
                    let state = self.state.blocking_read();
                    let selected = state.selected.iter().cloned().collect::<Vec<_>>();
                    let mut audio_outs = 2usize;
                    let mut midi_outs = 0usize;
                    for track in &state.tracks {
                        if state.selected.contains(&track.name) {
                            audio_outs = audio_outs.max(track.audio.outs.max(1));
                            midi_outs = midi_outs.max(track.midi.outs);
                        }
                    }
                    (
                        selected,
                        audio_outs.max(1),
                        midi_outs,
                        state
                            .tracks
                            .iter()
                            .map(|t| t.name.clone())
                            .collect::<std::collections::HashSet<_>>(),
                    )
                };
                if selected_tracks.is_empty() {
                    self.state.blocking_write().message =
                        "Select one or more tracks first".to_string();
                    return Task::none();
                }
                let mut idx = 1usize;
                let aux_name = loop {
                    let candidate = format!("Aux Return {idx}");
                    if !existing_names.contains(&candidate) {
                        break candidate;
                    }
                    idx = idx.saturating_add(1);
                };
                let mut tasks = vec![self.send(Action::BeginHistoryGroup)];
                tasks.push(self.send(Action::AddTrack {
                    name: aux_name.clone(),
                    audio_ins: max_outs,
                    midi_ins: 0,
                    audio_outs: max_outs,
                    midi_outs: max_midi_outs,
                }));
                for track_name in &selected_tracks {
                    tasks.push(self.send(Action::Connect {
                        from_track: track_name.clone(),
                        from_port: 0,
                        to_track: aux_name.clone(),
                        to_port: 0,
                        kind: Kind::Audio,
                    }));
                }
                tasks.push(self.send(Action::Connect {
                    from_track: aux_name.clone(),
                    from_port: 0,
                    to_track: "hw:out".to_string(),
                    to_port: 0,
                    kind: Kind::Audio,
                }));
                tasks.push(self.send(Action::EndHistoryGroup));
                {
                    let mut state = self.state.blocking_write();
                    for track in &mut state.tracks {
                        if selected_tracks.iter().any(|name| name == &track.name)
                            && !track.aux_sends.iter().any(|s| s.aux_track == aux_name)
                        {
                            track.aux_sends.push(crate::state::AuxSend {
                                aux_track: aux_name.clone(),
                                level_db: 0.0,
                                pan: 0.0,
                                pre_fader: false,
                            });
                        }
                    }
                }
                self.state.blocking_write().message = format!(
                    "Created '{}' and connected selected tracks as sends",
                    aux_name
                );
                return Task::batch(tasks);
            }
            Message::TrackAuxSendLevelAdjust {
                ref track_name,
                ref aux_track,
                delta_db,
            } => {
                let mut state = self.state.blocking_write();
                if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name)
                    && let Some(send) = track
                        .aux_sends
                        .iter_mut()
                        .find(|s| s.aux_track == *aux_track)
                {
                    send.level_db = (send.level_db + delta_db).clamp(-60.0, 12.0);
                    state.message = format!(
                        "Aux send {} -> {} level {:.1} dB",
                        track_name, aux_track, send.level_db
                    );
                }
                return Task::none();
            }
            Message::TrackAuxSendPanAdjust {
                ref track_name,
                ref aux_track,
                delta,
            } => {
                let mut state = self.state.blocking_write();
                if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name)
                    && let Some(send) = track
                        .aux_sends
                        .iter_mut()
                        .find(|s| s.aux_track == *aux_track)
                {
                    send.pan = (send.pan + delta).clamp(-1.0, 1.0);
                    state.message = format!(
                        "Aux send {} -> {} pan {:.2}",
                        track_name, aux_track, send.pan
                    );
                }
                return Task::none();
            }
            Message::TrackAuxSendTogglePrePost {
                ref track_name,
                ref aux_track,
            } => {
                let mut state = self.state.blocking_write();
                if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name)
                    && let Some(send) = track
                        .aux_sends
                        .iter_mut()
                        .find(|s| s.aux_track == *aux_track)
                {
                    send.pre_fader = !send.pre_fader;
                    state.message = format!(
                        "Aux send {} -> {} mode {}",
                        track_name,
                        aux_track,
                        if send.pre_fader {
                            "Pre-Fader"
                        } else {
                            "Post-Fader"
                        }
                    );
                }
                return Task::none();
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
                        tasks.push(self.send(Action::AddClip {
                            name: clip.name,
                            track_name: track_name.clone(),
                            start: clip.start,
                            length: clip.length,
                            offset: clip.offset,
                            input_channel: clip.input_channel,
                            muted: clip.muted,
                            kind: Kind::Audio,
                            fade_enabled: clip.fade_enabled,
                            fade_in_samples: clip.fade_in_samples,
                            fade_out_samples: clip.fade_out_samples,
                            warp_markers: clip.warp_markers,
                        }));
                    }
                    for clip in restore_midi {
                        tasks.push(self.send(Action::AddClip {
                            name: clip.name,
                            track_name: track_name.clone(),
                            start: clip.start,
                            length: clip.length,
                            offset: clip.offset,
                            input_channel: clip.input_channel,
                            muted: clip.muted,
                            kind: Kind::MIDI,
                            fade_enabled: clip.fade_enabled,
                            fade_in_samples: clip.fade_in_samples,
                            fade_out_samples: clip.fade_out_samples,
                            warp_markers: vec![],
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
                        crate::message::TrackAutomationTarget::Mute => {
                            OfflineAutomationTarget::Mute
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
                    automation_lanes.push(OfflineAutomationLane { target, points });
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
                });
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
            Message::TrackAutomationToggle { ref track_name } => {
                let mut state = self.state.blocking_write();
                if let Some(track) = state
                    .tracks
                    .iter_mut()
                    .find(|track| track.name == track_name.as_str())
                {
                    let any_visible = track.automation_lanes.iter().any(|lane| lane.visible);
                    if any_visible {
                        for lane in &mut track.automation_lanes {
                            lane.visible = false;
                        }
                    } else if track.automation_lanes.is_empty() {
                        track
                            .automation_lanes
                            .push(crate::state::TrackAutomationLane {
                                target: crate::message::TrackAutomationTarget::Volume,
                                visible: true,
                                points: vec![],
                            });
                    } else {
                        for lane in &mut track.automation_lanes {
                            lane.visible = true;
                        }
                    }
                    track.height = track.min_height_for_layout().max(60.0);
                }
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
            }
            Message::TrackAutomationAddLane {
                ref track_name,
                target,
            } => {
                let mut state = self.state.blocking_write();
                if let Some(track) = state
                    .tracks
                    .iter_mut()
                    .find(|track| track.name == track_name.as_str())
                {
                    if let Some(lane) = track
                        .automation_lanes
                        .iter_mut()
                        .find(|lane| lane.target == target)
                    {
                        lane.visible = true;
                    } else {
                        track
                            .automation_lanes
                            .push(crate::state::TrackAutomationLane {
                                target,
                                visible: true,
                                points: vec![],
                            });
                    }
                    track.height = track.min_height_for_layout().max(60.0);
                }
            }
            Message::TrackAutomationAddClapLanes {
                ref track_name,
                ref plugin_path,
            } => {
                return self.request_plugin_automation_lanes(track_name, plugin_path, "CLAP");
            }
            Message::TrackAutomationAddVst3Lanes {
                ref track_name,
                ref plugin_path,
            } => {
                return self.request_plugin_automation_lanes(track_name, plugin_path, "VST3");
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            Message::TrackAutomationAddLv2Lanes {
                ref track_name,
                ref plugin_uri,
            } => {
                let instance_id = self.find_plugin_graph_instance_id(track_name, "LV2", plugin_uri);
                if let Some(instance_id) = instance_id {
                    self.pending_add_lv2_automation_instances
                        .insert((track_name.clone(), instance_id));
                    return self.send(Action::TrackGetLv2PluginControls {
                        track_name: track_name.clone(),
                        instance_id,
                    });
                }
                self.pending_add_lv2_automation_uris
                    .insert((track_name.clone(), plugin_uri.clone()));
                return self
                    .maybe_refresh_plugin_graph_for_track(track_name)
                    .unwrap_or_else(Task::none);
            }
            Message::TrackAutomationLaneHover {
                ref track_name,
                target,
                position,
            } => {
                let mut state = self.state.blocking_write();
                state.automation_lane_hover = Some((track_name.clone(), target, position));
            }
            Message::TrackAutomationLaneInsertPoint {
                ref track_name,
                target,
            } => {
                let pixels_per_sample = self.pixels_per_sample().max(1.0e-6);
                let mut state = self.state.blocking_write();
                let Some((hover_track, hover_target, hover_position)) = state
                    .automation_lane_hover
                    .as_ref()
                    .map(|(name, target, position)| (name.as_str(), *target, *position))
                else {
                    return Task::none();
                };
                if hover_track != track_name.as_str() || hover_target != target {
                    return Task::none();
                }
                if let Some(track) = state
                    .tracks
                    .iter_mut()
                    .find(|track| track.name == track_name.as_str())
                {
                    let lane_height = track.lane_layout().lane_height.max(12.0);
                    let lane_value_h = (lane_height - 6.0).max(1.0);
                    let value = (1.0 - ((hover_position.y - 3.0) / lane_value_h)).clamp(0.0, 1.0);
                    let sample = ((hover_position.x / pixels_per_sample).round().max(0.0)) as usize;

                    if let Some(lane) = track
                        .automation_lanes
                        .iter_mut()
                        .find(|lane| lane.target == target)
                    {
                        if let Some(existing) = lane.points.iter_mut().find(|p| p.sample == sample)
                        {
                            existing.value = value;
                        } else {
                            lane.points
                                .push(crate::state::TrackAutomationPoint { sample, value });
                            lane.points.sort_unstable_by_key(|p| p.sample);
                        }
                        lane.visible = true;
                    } else {
                        track
                            .automation_lanes
                            .push(crate::state::TrackAutomationLane {
                                target,
                                visible: true,
                                points: vec![crate::state::TrackAutomationPoint { sample, value }],
                            });
                    }
                }
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
                            state.selected.remove(idx.as_str());
                        } else {
                            set.insert(idx.clone());
                            state.selected.insert(idx.clone());
                        }
                    }
                    _ => {
                        let mut set = std::collections::HashSet::new();
                        set.insert(idx.clone());
                        state.connection_view_selection = ConnectionViewSelection::Tracks(set);
                        state.selected.clear();
                        state.selected.insert(idx.clone());
                    }
                }
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
                // Get current clip name
                let current_name = state
                    .tracks
                    .iter()
                    .find(|t| t.name == *track_idx)
                    .and_then(|t| match kind {
                        Kind::Audio => t.audio.clips.get(clip_idx).map(|c| c.name.clone()),
                        Kind::MIDI => t.midi.clips.get(clip_idx).map(|c| c.name.clone()),
                    })
                    .unwrap_or_default();

                // Clean the name for editing (remove audio/ prefix and .wav suffix)
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
            Message::ClipRenameInput(_) => {
                // Handled by ClipRenameView
            }
            Message::ClipRenameConfirm => {
                let dialog = self.state.blocking_read().clip_rename_dialog.clone();
                let Some(dialog) = dialog else {
                    return Task::none();
                };

                let new_name = dialog.new_name.trim().to_string();
                if new_name.is_empty() {
                    return Task::none();
                }

                // Get session directory and old clip name
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

                // Build new file name.
                // MIDI clip files are intentionally NOT renamed on disk here; they are persisted on save.
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
                    // Audio clip files are renamed immediately.
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

                // Update all clip instances in the GUI state
                for track in &mut state.tracks {
                    match dialog.kind {
                        Kind::Audio => {
                            for clip in &mut track.audio.clips {
                                if clip.name == old_name {
                                    clip.name = new_file_name.clone();
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

                // Now update the engine by sending a RenameClip action
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
                let new_fade_enabled = {
                    let mut state = self.state.blocking_write();
                    if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_idx) {
                        match kind {
                            Kind::Audio => {
                                if let Some(clip) = track.audio.clips.get_mut(clip_idx) {
                                    clip.fade_enabled = !clip.fade_enabled;
                                    Some(clip.fade_enabled)
                                } else {
                                    None
                                }
                            }
                            Kind::MIDI => {
                                if let Some(clip) = track.midi.clips.get_mut(clip_idx) {
                                    clip.fade_enabled = !clip.fade_enabled;
                                    Some(clip.fade_enabled)
                                } else {
                                    None
                                }
                            }
                        }
                    } else {
                        None
                    }
                };

                if let Some(fade_enabled) = new_fade_enabled {
                    // Get the fade samples from the clip
                    let (fade_in_samples, fade_out_samples) = {
                        let state = self.state.blocking_read();
                        if let Some(track) = state.tracks.iter().find(|t| t.name == *track_idx) {
                            match kind {
                                Kind::Audio => {
                                    if let Some(clip) = track.audio.clips.get(clip_idx) {
                                        (clip.fade_in_samples, clip.fade_out_samples)
                                    } else {
                                        (240, 240)
                                    }
                                }
                                Kind::MIDI => {
                                    if let Some(clip) = track.midi.clips.get(clip_idx) {
                                        (clip.fade_in_samples, clip.fade_out_samples)
                                    } else {
                                        (240, 240)
                                    }
                                }
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
            Message::ClipWarpReset {
                ref track_idx,
                clip_idx,
            } => {
                return self.send(Action::SetAudioClipWarpMarkers {
                    track_name: track_idx.clone(),
                    clip_index: clip_idx,
                    warp_markers: vec![],
                });
            }
            Message::ClipWarpHalfSpeed {
                ref track_idx,
                clip_idx,
            } => {
                let clip_len = {
                    let state = self.state.blocking_read();
                    state
                        .tracks
                        .iter()
                        .find(|t| t.name == *track_idx)
                        .and_then(|t| t.audio.clips.get(clip_idx))
                        .map(|c| c.length)
                };
                if let Some(clip_len) = clip_len {
                    return self.send(Action::SetAudioClipWarpMarkers {
                        track_name: track_idx.clone(),
                        clip_index: clip_idx,
                        warp_markers: Self::warp_markers_for_speed(clip_len, 0.5),
                    });
                }
            }
            Message::ClipWarpDoubleSpeed {
                ref track_idx,
                clip_idx,
            } => {
                let clip_len = {
                    let state = self.state.blocking_read();
                    state
                        .tracks
                        .iter()
                        .find(|t| t.name == *track_idx)
                        .and_then(|t| t.audio.clips.get(clip_idx))
                        .map(|c| c.length)
                };
                if let Some(clip_len) = clip_len {
                    return self.send(Action::SetAudioClipWarpMarkers {
                        track_name: track_idx.clone(),
                        clip_index: clip_idx,
                        warp_markers: Self::warp_markers_for_speed(clip_len, 2.0),
                    });
                }
            }
            Message::ClipWarpAddMarker {
                ref track_idx,
                clip_idx,
            } => {
                let marker_state = {
                    let state = self.state.blocking_read();
                    state
                        .tracks
                        .iter()
                        .find(|t| t.name == *track_idx)
                        .and_then(|t| t.audio.clips.get(clip_idx))
                        .map(|c| (c.length, c.warp_markers.clone()))
                };
                if let Some((clip_len, markers)) = marker_state {
                    return self.send(Action::SetAudioClipWarpMarkers {
                        track_name: track_idx.clone(),
                        clip_index: clip_idx,
                        warp_markers: Self::add_warp_marker_between(&markers, clip_len),
                    });
                }
            }
            Message::ClipSetActiveTake {
                ref track_idx,
                clip_idx,
                kind,
            } => {
                let updates = {
                    let state = self.state.blocking_read();
                    let Some(track) = state.tracks.iter().find(|t| t.name == *track_idx) else {
                        return Task::none();
                    };
                    match kind {
                        Kind::Audio => {
                            let Some(selected) = track.audio.clips.get(clip_idx) else {
                                return Task::none();
                            };
                            let selected_end = selected.start.saturating_add(selected.length);
                            track
                                .audio
                                .clips
                                .iter()
                                .enumerate()
                                .filter_map(|(idx, clip)| {
                                    let end = clip.start.saturating_add(clip.length);
                                    (!clip.take_lane_locked
                                        && selected.start < end
                                        && clip.start < selected_end)
                                        .then_some((idx, idx != clip_idx))
                                })
                                .collect::<Vec<_>>()
                        }
                        Kind::MIDI => {
                            let Some(selected) = track.midi.clips.get(clip_idx) else {
                                return Task::none();
                            };
                            let selected_end = selected.start.saturating_add(selected.length);
                            track
                                .midi
                                .clips
                                .iter()
                                .enumerate()
                                .filter_map(|(idx, clip)| {
                                    let end = clip.start.saturating_add(clip.length);
                                    (!clip.take_lane_locked
                                        && selected.start < end
                                        && clip.start < selected_end)
                                        .then_some((idx, idx != clip_idx))
                                })
                                .collect::<Vec<_>>()
                        }
                    }
                };
                if updates.is_empty() {
                    return Task::none();
                }
                let mut tasks = vec![self.send(Action::BeginHistoryGroup)];
                for (idx, should_mute) in updates {
                    tasks.push(self.send(Action::SetClipMuted {
                        track_name: track_idx.clone(),
                        clip_index: idx,
                        kind,
                        muted: should_mute,
                    }));
                }
                tasks.push(self.send(Action::EndHistoryGroup));
                return Task::batch(tasks);
            }
            Message::ClipCycleActiveTake {
                ref track_idx,
                clip_idx,
                kind,
            } => {
                let updates = {
                    let state = self.state.blocking_read();
                    let Some(track) = state.tracks.iter().find(|t| t.name == *track_idx) else {
                        return Task::none();
                    };
                    let mut group: Vec<(usize, usize, bool)> = match kind {
                        Kind::Audio => {
                            let Some(selected) = track.audio.clips.get(clip_idx) else {
                                return Task::none();
                            };
                            let selected_end = selected.start.saturating_add(selected.length);
                            track
                                .audio
                                .clips
                                .iter()
                                .enumerate()
                                .filter_map(|(idx, clip)| {
                                    let end = clip.start.saturating_add(clip.length);
                                    (!clip.take_lane_locked
                                        && selected.start < end
                                        && clip.start < selected_end)
                                        .then_some((idx, clip.start, clip.muted))
                                })
                                .collect()
                        }
                        Kind::MIDI => {
                            let Some(selected) = track.midi.clips.get(clip_idx) else {
                                return Task::none();
                            };
                            let selected_end = selected.start.saturating_add(selected.length);
                            track
                                .midi
                                .clips
                                .iter()
                                .enumerate()
                                .filter_map(|(idx, clip)| {
                                    let end = clip.start.saturating_add(clip.length);
                                    (!clip.take_lane_locked
                                        && selected.start < end
                                        && clip.start < selected_end)
                                        .then_some((idx, clip.start, clip.muted))
                                })
                                .collect()
                        }
                    };
                    if group.is_empty() {
                        return Task::none();
                    }
                    group.sort_by_key(|(idx, start, _)| (*start, *idx));
                    let current_pos = group
                        .iter()
                        .position(|(idx, _, _)| *idx == clip_idx)
                        .or_else(|| group.iter().position(|(_, _, muted)| !*muted))
                        .unwrap_or(0);
                    let next_pos = (current_pos + 1) % group.len();
                    let next_idx = group[next_pos].0;
                    group
                        .iter()
                        .map(|(idx, _, _)| (*idx, *idx != next_idx))
                        .collect::<Vec<_>>()
                };
                if updates.is_empty() {
                    return Task::none();
                }
                let mut tasks = vec![self.send(Action::BeginHistoryGroup)];
                for (idx, should_mute) in updates {
                    tasks.push(self.send(Action::SetClipMuted {
                        track_name: track_idx.clone(),
                        clip_index: idx,
                        kind,
                        muted: should_mute,
                    }));
                }
                tasks.push(self.send(Action::EndHistoryGroup));
                return Task::batch(tasks);
            }
            Message::ClipUnmuteTakesInRange {
                ref track_idx,
                clip_idx,
                kind,
            } => {
                let updates = {
                    let state = self.state.blocking_read();
                    let Some(track) = state.tracks.iter().find(|t| t.name == *track_idx) else {
                        return Task::none();
                    };
                    match kind {
                        Kind::Audio => {
                            let Some(selected) = track.audio.clips.get(clip_idx) else {
                                return Task::none();
                            };
                            let selected_end = selected.start.saturating_add(selected.length);
                            track
                                .audio
                                .clips
                                .iter()
                                .enumerate()
                                .filter_map(|(idx, clip)| {
                                    let end = clip.start.saturating_add(clip.length);
                                    (!clip.take_lane_locked
                                        && selected.start < end
                                        && clip.start < selected_end)
                                        .then_some((idx, false))
                                })
                                .collect::<Vec<_>>()
                        }
                        Kind::MIDI => {
                            let Some(selected) = track.midi.clips.get(clip_idx) else {
                                return Task::none();
                            };
                            let selected_end = selected.start.saturating_add(selected.length);
                            track
                                .midi
                                .clips
                                .iter()
                                .enumerate()
                                .filter_map(|(idx, clip)| {
                                    let end = clip.start.saturating_add(clip.length);
                                    (!clip.take_lane_locked
                                        && selected.start < end
                                        && clip.start < selected_end)
                                        .then_some((idx, false))
                                })
                                .collect::<Vec<_>>()
                        }
                    }
                };
                if updates.is_empty() {
                    return Task::none();
                }
                let mut tasks = vec![self.send(Action::BeginHistoryGroup)];
                for (idx, should_mute) in updates {
                    tasks.push(self.send(Action::SetClipMuted {
                        track_name: track_idx.clone(),
                        clip_index: idx,
                        kind,
                        muted: should_mute,
                    }));
                }
                tasks.push(self.send(Action::EndHistoryGroup));
                return Task::batch(tasks);
            }
            Message::ClipTakeLanePinToggle {
                ref track_idx,
                clip_idx,
                kind,
            } => {
                let mut state = self.state.blocking_write();
                let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_idx) else {
                    return Task::none();
                };
                match kind {
                    Kind::Audio => {
                        if clip_idx >= track.audio.clips.len() {
                            return Task::none();
                        }
                        let current_take = {
                            let (take_idx, _) = Self::assign_take_lanes(
                                &track.audio.clips,
                                |_| 0,
                                |clip| clip.start,
                                |clip| clip.length,
                                |clip| clip.take_lane_override,
                            );
                            take_idx.get(clip_idx).copied().unwrap_or(0)
                        };
                        let clip = &mut track.audio.clips[clip_idx];
                        if clip.take_lane_pinned {
                            clip.take_lane_pinned = false;
                            if !clip.take_lane_locked {
                                clip.take_lane_override = None;
                            }
                        } else {
                            clip.take_lane_pinned = true;
                            clip.take_lane_override = Some(current_take);
                        }
                    }
                    Kind::MIDI => {
                        if clip_idx >= track.midi.clips.len() {
                            return Task::none();
                        }
                        let lane_count = track.midi.ins.max(1);
                        let (take_idx, _) = Self::assign_take_lanes(
                            &track.midi.clips,
                            |clip| clip.input_channel.min(lane_count.saturating_sub(1)),
                            |clip| clip.start,
                            |clip| clip.length,
                            |clip| clip.take_lane_override,
                        );
                        let current_take = take_idx.get(clip_idx).copied().unwrap_or(0);
                        let clip = &mut track.midi.clips[clip_idx];
                        if clip.take_lane_pinned {
                            clip.take_lane_pinned = false;
                            if !clip.take_lane_locked {
                                clip.take_lane_override = None;
                            }
                        } else {
                            clip.take_lane_pinned = true;
                            clip.take_lane_override = Some(current_take);
                        }
                    }
                }
            }
            Message::ClipTakeLaneLockToggle {
                ref track_idx,
                clip_idx,
                kind,
            } => {
                let mut state = self.state.blocking_write();
                let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_idx) else {
                    return Task::none();
                };
                match kind {
                    Kind::Audio => {
                        let Some(clip) = track.audio.clips.get_mut(clip_idx) else {
                            return Task::none();
                        };
                        clip.take_lane_locked = !clip.take_lane_locked;
                    }
                    Kind::MIDI => {
                        let Some(clip) = track.midi.clips.get_mut(clip_idx) else {
                            return Task::none();
                        };
                        clip.take_lane_locked = !clip.take_lane_locked;
                    }
                }
            }
            Message::ClipTakeLaneMove {
                ref track_idx,
                clip_idx,
                kind,
                delta,
            } => {
                let mut state = self.state.blocking_write();
                let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_idx) else {
                    return Task::none();
                };
                match kind {
                    Kind::Audio => {
                        if clip_idx >= track.audio.clips.len() {
                            return Task::none();
                        }
                        let (take_idx, _) = Self::assign_take_lanes(
                            &track.audio.clips,
                            |_| 0,
                            |clip| clip.start,
                            |clip| clip.length,
                            |clip| clip.take_lane_override,
                        );
                        let current_take = take_idx.get(clip_idx).copied().unwrap_or(0);
                        let clip = &mut track.audio.clips[clip_idx];
                        if clip.take_lane_locked {
                            return Task::none();
                        }
                        let next_take = if delta.is_negative() {
                            current_take.saturating_sub(delta.unsigned_abs() as usize)
                        } else {
                            current_take.saturating_add(delta as usize)
                        };
                        clip.take_lane_override = Some(next_take);
                        clip.take_lane_pinned = true;
                    }
                    Kind::MIDI => {
                        if clip_idx >= track.midi.clips.len() {
                            return Task::none();
                        }
                        let lane_count = track.midi.ins.max(1);
                        let (take_idx, _) = Self::assign_take_lanes(
                            &track.midi.clips,
                            |clip| clip.input_channel.min(lane_count.saturating_sub(1)),
                            |clip| clip.start,
                            |clip| clip.length,
                            |clip| clip.take_lane_override,
                        );
                        let current_take = take_idx.get(clip_idx).copied().unwrap_or(0);
                        let clip = &mut track.midi.clips[clip_idx];
                        if clip.take_lane_locked {
                            return Task::none();
                        }
                        let next_take = if delta.is_negative() {
                            current_take.saturating_sub(delta.unsigned_abs() as usize)
                        } else {
                            current_take.saturating_add(delta as usize)
                        };
                        clip.take_lane_override = Some(next_take);
                        clip.take_lane_pinned = true;
                    }
                }
            }
            Message::TrackRenameShow(ref track_name) => {
                let mut state = self.state.blocking_write();
                state.track_rename_dialog = Some(crate::state::TrackRenameDialog {
                    old_name: track_name.clone(),
                    new_name: track_name.clone(),
                });
            }
            Message::TrackRenameInput(_) => {
                // Handled by TrackRenameView
            }
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

                // Send rename action to engine
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

                // Construct path: ~/.config/maolan/track_templates/<name>
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

                // Construct path: ~/.config/maolan/session_templates/<name>
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
            Message::MousePressed(button) => {
                if self.modal.is_none()
                    && matches!(self.state.blocking_read().view, View::Workspace)
                {
                    if button == mouse::Button::Middle {
                        let cursor = self.state.blocking_read().cursor;
                        return self.split_clip_at_position(cursor);
                    }
                    match button {
                        mouse::Button::Left => {
                            let mut state = self.state.blocking_write();
                            state.mouse_left_down = true;
                            state.clip_marquee_start = None;
                            state.clip_marquee_end = None;
                        }
                        mouse::Button::Right => {
                            let cursor = {
                                let state = self.state.blocking_read();
                                state.editor_cursor.unwrap_or(state.cursor)
                            };
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
                // Check if we're in piano view with selected notes
                let state = self.state.blocking_read();
                let view = state.view.clone();
                let has_piano_notes =
                    state.piano.is_some() && !state.piano_selected_notes.is_empty();
                drop(state);

                if matches!(view, crate::state::View::Piano) && has_piano_notes {
                    return self.update(Message::PianoDeleteSelectedNotes);
                }

                let selected_clips: Vec<_> = self
                    .state
                    .blocking_read()
                    .selected_clips
                    .iter()
                    .cloned()
                    .collect();
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
                        return self.update(Message::RemoveSelected);
                    }
                    crate::state::View::Workspace => {
                        return self.update(Message::RemoveSelectedTracks);
                    }
                    crate::state::View::TrackPlugins => {
                        #[cfg(any(target_os = "windows", all(unix, not(target_os = "macos"))))]
                        {
                            let (track_name, selected_plugin, selected_indices, connections) = {
                                let state = self.state.blocking_read();
                                (
                                    state.plugin_graph_track.clone(),
                                    state.plugin_graph_selected_plugin,
                                    state.plugin_graph_selected_connections.clone(),
                                    state.plugin_graph_connections.clone(),
                                )
                            };
                            if let Some(track_name) = track_name {
                                if let Some(instance_id) = selected_plugin {
                                    self.state.blocking_write().plugin_graph_selected_plugin = None;
                                    self.state
                                        .blocking_write()
                                        .plugin_graph_selected_connections
                                        .clear();
                                    let selected_node = self
                                        .state
                                        .blocking_read()
                                        .plugin_graph_plugins
                                        .iter()
                                        .find(|p| p.instance_id == instance_id)
                                        .map(|p| p.node.clone());
                                    if let Some(node) = selected_node {
                                        return match node {
                                            #[cfg(all(unix, not(target_os = "macos")))]
                                            PluginGraphNode::Lv2PluginInstance(_) => {
                                                self.send(Action::TrackUnloadLv2PluginInstance {
                                                    track_name,
                                                    instance_id,
                                                })
                                            }
                                            #[cfg(target_os = "windows")]
                                            PluginGraphNode::Lv2PluginInstance(_) => Task::none(),
                                            PluginGraphNode::Vst3PluginInstance(_) => {
                                                self.send(Action::TrackUnloadVst3PluginInstance {
                                                    track_name,
                                                    instance_id,
                                                })
                                            }
                                            PluginGraphNode::ClapPluginInstance(_) => {
                                                let plugin_path = self
                                                    .state
                                                    .blocking_read()
                                                    .plugin_graph_plugins
                                                    .iter()
                                                    .find(|p| p.instance_id == instance_id)
                                                    .map(|p| p.uri.clone())
                                                    .unwrap_or_default();
                                                self.send(Action::TrackUnloadClapPlugin {
                                                    track_name,
                                                    plugin_path,
                                                })
                                            }
                                            PluginGraphNode::TrackInput
                                            | PluginGraphNode::TrackOutput => Task::none(),
                                        };
                                    }
                                    return Task::none();
                                }
                                let actions = connections::selection::plugin_disconnect_actions(
                                    &track_name,
                                    &connections,
                                    &selected_indices,
                                );
                                let tasks = actions
                                    .into_iter()
                                    .map(|a| self.send(a))
                                    .collect::<Vec<_>>();
                                self.state
                                    .blocking_write()
                                    .plugin_graph_selected_connections
                                    .clear();
                                self.state.blocking_write().plugin_graph_selected_plugin = None;
                                return Task::batch(tasks);
                            }
                        }
                    }
                    crate::state::View::Piano => {
                        return self.update(Message::RemoveSelected);
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
            Message::ClipResizeHandleHover {
                ref kind,
                ref track_idx,
                clip_idx,
                is_right_side,
                hovered,
            } => {
                let mut state = self.state.blocking_write();
                if hovered {
                    state.hovered_clip_resize_handle =
                        Some((track_idx.clone(), clip_idx, *kind, is_right_side));
                } else if state.hovered_clip_resize_handle.as_ref().is_some_and(
                    |(active_track, active_clip, active_kind, active_right)| {
                        active_track == track_idx
                            && *active_clip == clip_idx
                            && *active_kind == *kind
                            && *active_right == is_right_side
                    },
                ) {
                    state.hovered_clip_resize_handle = None;
                }
            }
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
            Message::ClipResizeStart(ref kind, ref track_name, clip_index, is_right_side) => {
                self.clip = None;
                let mut state = self.state.blocking_write();
                if let Some(track) = state.tracks.iter().find(|t| t.name == *track_name) {
                    match kind {
                        Kind::Audio => {
                            let Some(clip) = track.audio.clips.get(clip_index) else {
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
                                initial_value: initial_value as f32,
                                initial_mouse_x: state.cursor.x,
                                initial_length: clip.length as f32,
                            });
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
                                initial_value: initial_value as f32,
                                initial_mouse_x: state.cursor.x,
                                initial_length: clip.length as f32,
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
                self.clip = None;
                let mut state = self.state.blocking_write();
                if let Some(track) = state.tracks.iter().find(|t| t.name == *track_idx) {
                    let initial_samples = match kind {
                        Kind::Audio => track.audio.clips.get(clip_idx).and_then(|clip| {
                            if clip.take_lane_locked {
                                return None;
                            }
                            if is_fade_out {
                                Some(clip.fade_out_samples)
                            } else {
                                Some(clip.fade_in_samples)
                            }
                        }),
                        Kind::MIDI => track.midi.clips.get(clip_idx).and_then(|clip| {
                            if clip.take_lane_locked {
                                return None;
                            }
                            if is_fade_out {
                                Some(clip.fade_out_samples)
                            } else {
                                Some(clip.fade_in_samples)
                            }
                        }),
                    };

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
                let resizing = self.state.blocking_read().resizing.clone();
                let previous_cursor = {
                    let mut state = self.state.blocking_write();
                    let prev = state.cursor;
                    state.cursor = position;
                    prev
                };
                match resizing {
                    Some(Resizing::Track(ref track_name, initial_height, initial_mouse_y)) => {
                        let mut state = self.state.blocking_write();
                        let delta = position.y - initial_mouse_y;
                        if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name)
                        {
                            let min_h = track.min_height_for_layout();
                            track.height = (initial_height + delta).clamp(min_h, 600.0);
                        }
                    }
                    Some(Resizing::Clip {
                        kind,
                        ref track_name,
                        index,
                        is_right_side,
                        initial_value,
                        initial_mouse_x,
                        initial_length,
                    }) => {
                        let pixels_per_sample = self.pixels_per_sample().max(1.0e-6);
                        let min_length_samples =
                            (MIN_CLIP_WIDTH_PX / pixels_per_sample).ceil().max(1.0);
                        let mut state = self.state.blocking_write();
                        if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name)
                        {
                            let delta_samples = (position.x - initial_mouse_x) / pixels_per_sample;
                            match kind {
                                Kind::Audio => {
                                    let clip = &mut track.audio.clips[index];
                                    let max_length_samples =
                                        clip.max_length_samples.max(initial_length as usize) as f32;
                                    if is_right_side {
                                        let updated_length = (initial_value + delta_samples)
                                            .clamp(min_length_samples, max_length_samples);
                                        clip.length = updated_length as usize;
                                    } else {
                                        let right_edge = initial_value + initial_length;
                                        let max_start = (right_edge - min_length_samples).max(0.0);
                                        let min_start = (right_edge - max_length_samples).max(0.0);
                                        let new_start = (initial_value + delta_samples)
                                            .clamp(min_start, max_start);
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
                                Kind::MIDI => {
                                    let clip = &mut track.midi.clips[index];
                                    let max_length_samples =
                                        clip.max_length_samples.max(initial_length as usize) as f32;
                                    if is_right_side {
                                        let updated_length = (initial_value + delta_samples)
                                            .clamp(min_length_samples, max_length_samples);
                                        clip.length = updated_length as usize;
                                    } else {
                                        let right_edge = initial_value + initial_length;
                                        let max_start = (right_edge - min_length_samples).max(0.0);
                                        let min_start = (right_edge - max_length_samples).max(0.0);
                                        let new_start = (initial_value + delta_samples)
                                            .clamp(min_start, max_start);
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
                    }
                    Some(Resizing::Tracks(initial_width, initial_mouse_x)) => {
                        let delta = position.x - initial_mouse_x;
                        self.state.blocking_write().tracks_width =
                            Length::Fixed((initial_width + delta).max(80.0));
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
                                // For fade-out, dragging left (negative) increases fade length
                                (initial_mouse_x - position.x) / pixels_per_sample
                            } else {
                                // For fade-in, dragging right (positive) increases fade length
                                (position.x - initial_mouse_x) / pixels_per_sample
                            };
                            let new_fade_samples =
                                ((initial_samples as f32 + delta_samples).max(0.0) as usize)
                                    .min(96000); // Max 2 seconds at 48kHz

                            match kind {
                                Kind::Audio => {
                                    if let Some(clip) = track.audio.clips.get_mut(index) {
                                        let max_fade = clip.length / 2; // Can't fade more than half the clip
                                        if is_fade_out {
                                            clip.fade_out_samples = new_fade_samples.min(max_fade);
                                        } else {
                                            clip.fade_in_samples = new_fade_samples.min(max_fade);
                                        }
                                    }
                                }
                                Kind::MIDI => {
                                    if let Some(clip) = track.midi.clips.get_mut(index) {
                                        let max_fade = clip.length / 2; // Can't fade more than half the clip
                                        if is_fade_out {
                                            clip.fade_out_samples = new_fade_samples.min(max_fade);
                                        } else {
                                            clip.fade_in_samples = new_fade_samples.min(max_fade);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
                let mouse_left_down = self.state.blocking_read().mouse_left_down;
                if mouse_left_down && !matches!(resizing, Some(Resizing::Clip { .. })) {
                    if let Some(active) = self.clip.as_mut() {
                        active.end = position;
                        return iced_drop::zones_on_point(
                            Message::HandleClipPreviewZones,
                            position,
                            None,
                            None,
                        );
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
            }
            Message::EditorMouseMoved(position) => {
                let resizing = self.state.blocking_read().resizing.clone();
                let can_start_midi_drag = self.midi_lane_at_position(position).is_some();
                let mut state = self.state.blocking_write();
                state.cursor = position;
                state.editor_cursor = Some(position);
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
                    return Task::none();
                }
                let (resizing, marquee_start, marquee_end, create_start, create_end) = {
                    let mut state = self.state.blocking_write();
                    state.mouse_left_down = false;
                    state.mouse_right_down = false;
                    state.clip_click_consumed = false;
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
                if matches!(resizing, Some(Resizing::Clip { .. })) {
                    return Task::none();
                }
                if let Some(Resizing::Fade {
                    kind,
                    track_name,
                    index,
                    ..
                }) = resizing
                {
                    // Send updated fade values to engine
                    let state = self.state.blocking_read();
                    if let Some(track) = state.tracks.iter().find(|t| t.name == track_name) {
                        let (fade_enabled, fade_in_samples, fade_out_samples) = match kind {
                            Kind::Audio => {
                                if let Some(clip) = track.audio.clips.get(index) {
                                    (
                                        clip.fade_enabled,
                                        clip.fade_in_samples,
                                        clip.fade_out_samples,
                                    )
                                } else {
                                    return Task::none();
                                }
                            }
                            Kind::MIDI => {
                                if let Some(clip) = track.midi.clips.get(index) {
                                    (
                                        clip.fade_enabled,
                                        clip.fade_in_samples,
                                        clip.fade_out_samples,
                                    )
                                } else {
                                    return Task::none();
                                }
                            }
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
                        for track in &state.tracks {
                            let layout = track.lane_layout();
                            let lane_clip_h = (layout.lane_height - 6.0).max(12.0);
                            for (clip_idx, clip) in track.audio.clips.iter().enumerate() {
                                let cx = clip.start as f32 * pps;
                                let cw = (clip.length as f32 * pps).max(12.0);
                                let lane =
                                    clip.input_channel.min(track.audio.ins.saturating_sub(1));
                                let cy = y_offset + track.lane_top(Kind::Audio, lane) + 3.0;
                                let ch = lane_clip_h.max(1.0);
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
                                let cy = y_offset + track.lane_top(Kind::MIDI, lane) + 3.0;
                                let ch = lane_clip_h.max(1.0);
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
                                to_track.audio.ins > 0 && from_track.audio.ins == to_track.audio.ins
                            }
                            Kind::MIDI => to_track.midi.ins > 0,
                        };
                        if !kind_matches {
                            self.clip = None;
                            self.clip_preview_target_track = None;
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
                                            self.snap_sample_to_bar(source.start as f32 + offset);
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
                                    return Task::batch(tasks);
                                }
                                if clip_index >= from_track.audio.clips.len() {
                                    self.clip = None;
                                    return Task::none();
                                }
                                let clip_index_in_from_track = clip_index;
                                let mut clip_copy =
                                    from_track.audio.clips[clip_index_in_from_track].clone();
                                clip_copy.start =
                                    self.snap_sample_to_bar(clip_copy.start as f32 + offset);
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
                                return task;
                            }
                            Kind::MIDI => {
                                let offset = (clip.end.x - clip.start.x)
                                    / self.pixels_per_sample().max(1.0e-6);
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
                                            self.snap_sample_to_bar(source.start as f32 + offset);
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
                                    return Task::batch(tasks);
                                }
                                if clip_index >= from_track.midi.clips.len() {
                                    self.clip = None;
                                    return Task::none();
                                }
                                let clip_index_in_from_track = clip_index;
                                let mut clip_copy =
                                    from_track.midi.clips[clip_index_in_from_track].clone();
                                clip_copy.start =
                                    self.snap_sample_to_bar(clip_copy.start as f32 + offset);
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
                                return task;
                            }
                        }
                    }
                }
                self.clip = None;
                self.clip_preview_target_track = None;
                return Task::none();
            }
            Message::HandleClipPreviewZones(ref zones) => {
                if let Some(clip) = &self.clip {
                    let state = self.state.blocking_read();
                    let from_track = state.tracks.iter().find(|t| t.name == clip.track_index);
                    let to_track_id = zones.iter().map(|(id, _)| id).find(|id| {
                        state
                            .tracks
                            .iter()
                            .any(|t| Id::from(t.name.clone()) == **id)
                    });
                    let Some(to_track_id) = to_track_id else {
                        self.clip_preview_target_track = None;
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
                        } else {
                            self.clip_preview_target_track = None;
                        }
                    } else {
                        self.clip_preview_target_track = None;
                    }
                } else {
                    self.clip_preview_target_track = None;
                }
            }
            Message::TrackDrag(index) => {
                let state = self.state.blocking_read();
                if index < state.tracks.len() {
                    self.track = Some(state.tracks[index].name.clone());
                }
            }
            Message::TrackDropped(point, _rect) => {
                if self.track.is_some() {
                    return iced_drop::zones_on_point(Message::HandleTrackZones, point, None, None);
                }
                self.track = None;
            }
            Message::HandleTrackZones(ref zones) => {
                if let Some(index_name) = &self.track {
                    let dragged_id = Id::from(index_name.clone());
                    let target_zone = zones
                        .iter()
                        .find(|(zone_id, _)| *zone_id != dragged_id)
                        .or_else(|| zones.first());
                    if let Some((track_id, _)) = target_zone {
                        let mut state = self.state.blocking_write();
                        if let Some(index) = state.tracks.iter().position(|t| t.name == *index_name)
                        {
                            let moved_track = state.tracks.remove(index);
                            let to_index = state
                                .tracks
                                .iter()
                                .position(|t| Id::from(t.name.clone()) == *track_id);

                            if let Some(t_idx) = to_index {
                                state.tracks.insert(t_idx, moved_track);
                            } else {
                                state.tracks.push(moved_track);
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

                let used_track_names: HashSet<String> = self
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
                            let mut used_names = used_track_names;
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
                                        // Reduce UI/queue churn from high-frequency decode callbacks.
                                        let clamped = progress.clamp(0.0, 1.0);
                                        let bucket = (clamped * 100.0).round() as u16;
                                        if last_progress_bucket == Some(bucket)
                                            && last_operation == operation
                                        {
                                            return;
                                        }
                                        last_progress_bucket = Some(bucket);
                                        last_operation = operation.clone();
                                        let _ = tx_clone.send(Message::ImportProgress {
                                            file_index,
                                            total_files,
                                            file_progress: clamped,
                                            filename: filename_for_progress.clone(),
                                            operation,
                                        });
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
                                        Ok((clip_rel, channels, length)) => {
                                            let base = Self::import_track_base_name(path);
                                            let track_name =
                                                Self::unique_track_name(&base, &mut used_names);

                                            if let Err(e) = CLIENT
                                                .send(EngineMessage::Request(Action::AddTrack {
                                                    name: track_name.clone(),
                                                    audio_ins: channels,
                                                    midi_ins: 0,
                                                    audio_outs: channels,
                                                    midi_outs: 0,
                                                }))
                                                .await
                                            {
                                                failures.push(format!("{} ({e})", path.display()));
                                                continue;
                                            }
                                            if let Err(e) = CLIENT
                                                .send(EngineMessage::Request(Action::AddClip {
                                                    name: clip_rel,
                                                    track_name,
                                                    start: 0,
                                                    length,
                                                    offset: 0,
                                                    input_channel: 0,
                                                    muted: false,
                                                    kind: Kind::Audio,
                                                    fade_enabled: true,
                                                    fade_in_samples: 240,
                                                    fade_out_samples: 240,
                                                    warp_markers: vec![],
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
                                    let _ = tx.send(Message::ImportProgress {
                                        file_index,
                                        total_files,
                                        file_progress: 0.5,
                                        filename: filename.clone(),
                                        operation: Some("Copying".to_string()),
                                    });

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
                                                }))
                                                .await
                                            {
                                                failures.push(format!("{} ({e})", path.display()));
                                                continue;
                                            }
                                            if let Err(e) = CLIENT
                                                .send(EngineMessage::Request(Action::AddClip {
                                                    name: clip_rel,
                                                    track_name,
                                                    start: 0,
                                                    length,
                                                    offset: 0,
                                                    input_channel: 0,
                                                    muted: false,
                                                    kind: Kind::MIDI,
                                                    fade_enabled: true,
                                                    fade_in_samples: 240,
                                                    fade_out_samples: 240,
                                                    warp_markers: vec![],
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

                                    let _ = tx.send(Message::ImportProgress {
                                        file_index,
                                        total_files,
                                        file_progress: 1.0,
                                        filename: filename.clone(),
                                        operation: None,
                                    });
                                } else {
                                    failures.push(format!(
                                        "{} (unsupported extension)",
                                        path.display()
                                    ));
                                }
                            }

                            for err in &failures {
                                error!("Import failed: {err}");
                            }

                            let _ = tx.send(Message::ImportProgress {
                                file_index: total_files,
                                total_files,
                                file_progress: 1.0,
                                filename: "Done".to_string(),
                                operation: None,
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
            Message::OpenExporter => {
                if self.session_dir.is_none() {
                    self.state.blocking_write().message =
                        "Export requires an opened/saved session".to_string();
                    return Task::none();
                }
                let nearest_rate = Self::STANDARD_EXPORT_SAMPLE_RATES
                    .iter()
                    .min_by_key(|rate| {
                        (i64::from(**rate) - self.playback_rate_hz.round() as i64).abs()
                    })
                    .copied()
                    .unwrap_or(48_000);
                self.export_sample_rate_hz = nearest_rate;
                self.modal = Some(crate::message::Show::ExportSettings);
            }
            Message::ExportDiagnosticsBundleRequest => {
                self.pending_diagnostics_bundle_export = true;
                self.diagnostics_bundle_wait_session_report = true;
                self.diagnostics_bundle_wait_midi_report = true;
                return Task::batch(vec![
                    self.send(Action::RequestSessionDiagnostics),
                    self.send(Action::RequestMidiLearnMappingsReport),
                ]);
            }
            Message::SessionDiagnosticsRequest => {
                return self.send(Action::RequestSessionDiagnostics);
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
                let export_path = Self::export_base_path(path.clone());
                let export_mp3_mode = self.export_mp3_mode;
                let export_mp3_bitrate_kbps = self.export_mp3_bitrate_kbps;
                let export_ogg_quality = self
                    .export_ogg_quality_input
                    .parse::<f32>()
                    .ok()
                    .unwrap_or(0.6);
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

                return Task::run(
                    {
                        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                        tokio::spawn(async move {
                            let tx_clone = tx.clone();
                            let mut last_progress_bucket: Option<u16> = None;
                            let mut last_operation: Option<String> = None;
                            let progress_fn = move |progress: f32, operation: Option<String>| {
                                // Reduce UI/queue churn from high-frequency callbacks
                                let clamped = progress.clamp(0.0, 1.0);
                                let bucket = (clamped * 100.0).round() as u16;
                                if last_progress_bucket == Some(bucket)
                                    && last_operation == operation
                                {
                                    return;
                                }
                                last_progress_bucket = Some(bucket);
                                last_operation = operation.clone();
                                let _ = tx_clone.send(Message::ExportProgress {
                                    progress: clamped,
                                    operation,
                                });
                            };

                            let options = super::super::ExportSessionOptions {
                                export_path: export_path.clone(),
                                sample_rate,
                                formats: export_formats,
                                render_mode,
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
                            let result = Self::export_session(&options, progress_fn).await;

                            if let Err(e) = result {
                                let _ = tx.send(Message::ExportProgress {
                                    progress: 0.0,
                                    operation: Some(format!("Error: {}", e)),
                                });
                            } else {
                                let _ = tx.send(Message::ExportProgress {
                                    progress: 1.0,
                                    operation: Some("Complete".to_string()),
                                });
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
                cfg.default_export_sample_rate_hz = self.prefs_export_sample_rate_hz;
                cfg.default_snap_mode = self.prefs_snap_mode;
                cfg.default_output_device_id = self.prefs_default_output_device_id.clone();
                cfg.default_input_device_id = self.prefs_default_input_device_id.clone();
                let prefs = super::super::AppPreferences {
                    default_export_sample_rate_hz: cfg.default_export_sample_rate_hz,
                    default_snap_mode: cfg.default_snap_mode,
                    default_output_device_id: cfg.default_output_device_id.clone(),
                    default_input_device_id: cfg.default_input_device_id.clone(),
                    recent_session_paths: cfg.recent_session_paths.clone(),
                };
                match cfg.save().map_err(|e| e.to_string()) {
                    Ok(()) => {
                        self.export_sample_rate_hz = self.prefs_export_sample_rate_hz;
                        self.snap_mode = self.prefs_snap_mode;
                        {
                            let mut state = self.state.blocking_write();
                            Self::apply_preferred_devices_to_state(&mut state, &prefs);
                        }
                        self.modal = None;
                        self.state.blocking_write().message =
                            "Preferences saved: ~/.config/maolan/config.toml".to_string();
                    }
                    Err(e) => {
                        self.state.blocking_write().message =
                            format!("Failed to save preferences: {e}");
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
                let mut state = self.state.blocking_write();
                state.view = View::Workspace;
                drop(state);
                return self.queue_midi_clip_preview_loads();
            }
            Message::ToggleMixerVisibility => {
                self.mixer_visible = !self.mixer_visible;
                if !self.mixer_visible {
                    self.mixer_resize_hovered = false;
                }
            }
            Message::Connections => {
                let mut state = self.state.blocking_write();
                state.view = View::Connections;
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
                let (clip_name, clip_length) = {
                    let state = self.state.blocking_read();
                    let Some(track) = state.tracks.iter().find(|t| t.name == *track_idx) else {
                        return Task::none();
                    };
                    let Some(clip) = track.midi.clips.get(clip_idx) else {
                        return Task::none();
                    };
                    (clip.name.clone(), clip.length.max(1))
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
                match Self::parse_midi_clip_for_piano(&path, self.playback_rate_hz) {
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
                                clip_length_samples: parsed_len.max(clip_length),
                                notes,
                                controllers,
                                sysexes,
                                midnam_note_names: HashMap::new(),
                            });
                            state.piano_selected_sysex = None;
                            state.piano_sysex_hex_input.clear();
                            state.piano_sysex_panel_open = false;
                            state.piano_scroll_x = 0.0;
                            state.piano_scroll_y = 0.0;
                            state.view = View::Piano;
                        }
                        #[cfg(all(unix, not(target_os = "macos")))]
                        {
                            let _ = self.send(Action::TrackGetLv2Midnam {
                                track_name: track_idx.clone(),
                            });
                        }
                        return self.sync_piano_scrollbars();
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
                    Self::reset_track_plugin_view_state(&mut state);
                }
                return self.open_track_plugins_followup(track_name.clone());
            }
            _ => {}
        }
        self.update_children(&message);
        Task::none()
    }
}
