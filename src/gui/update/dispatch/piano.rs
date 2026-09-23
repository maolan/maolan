use super::*;

impl Maolan {
    pub(super) fn handle_piano_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::PianoZoomXChanged(value) => {
                self.state
                    .write()
                    .expect("state lock poisoned")
                    .piano_zoom_x = value;
                return self.sync_piano_scrollbars();
            }
            Message::PianoTimelineZoomByScroll(delta) if delta.abs() > f32::EPSILON => {
                let factor = 1.12_f32.powf(delta.abs());
                {
                    let mut state = self.state.write().expect("state lock poisoned");
                    state.piano_zoom_x = if delta > 0.0 {
                        state.piano_zoom_x * factor
                    } else {
                        state.piano_zoom_x / factor
                    }
                    .clamp(H_ZOOM_MIN, H_ZOOM_MAX);
                }
                return self.sync_piano_scrollbars();
            }
            Message::PianoZoomYChanged(value) => {
                self.state
                    .write()
                    .expect("state lock poisoned")
                    .piano_zoom_y = value;
                return self.sync_piano_scrollbars();
            }
            Message::PianoScrollChanged { x, y } => {
                let x = x.clamp(0.0, 1.0);
                let y = y.clamp(0.0, 1.0);
                let changed = {
                    let mut state = self.state.write().expect("state lock poisoned");
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
                    let mut state = self.state.write().expect("state lock poisoned");
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
                    let mut state = self.state.write().expect("state lock poisoned");
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
                    let mut state = self.state.write().expect("state lock poisoned");
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
                let mut state = self.state.write().expect("state lock poisoned");
                state.piano_controller_lane = lane;
                state.piano_sysex_panel_open =
                    matches!(lane, crate::message::PianoControllerLane::SysEx);
            }
            Message::MidiEditorViewModeSelected(mode) => {
                let state = self.state.read().expect("state lock poisoned");
                if let Some(piano) = state.piano.as_ref() {
                    let track_name = piano.track_idx.clone();
                    drop(state);
                    let mut state = self.state.write().expect("state lock poisoned");
                    if let Some(track) = state.tracks.iter_mut().find(|t| t.name == track_name) {
                        track.midi.editor_view_mode = mode;
                    }
                }
            }
            Message::PianoControllerKindSelected(kind) => {
                let mut state = self.state.write().expect("state lock poisoned");
                state.piano_controller_lane = crate::message::PianoControllerLane::Controller;
                state.piano_controller_kind = kind;
                state.piano_sysex_panel_open = false;
            }
            Message::PianoVelocityKindSelected(kind) => {
                let mut state = self.state.write().expect("state lock poisoned");
                state.piano_controller_lane = crate::message::PianoControllerLane::Velocity;
                state.piano_velocity_kind = kind;
                state.piano_sysex_panel_open = false;
            }
            Message::PianoRpnKindSelected(kind) => {
                let mut state = self.state.write().expect("state lock poisoned");
                state.piano_controller_lane = crate::message::PianoControllerLane::Rpn;
                state.piano_rpn_kind = kind;
                state.piano_sysex_panel_open = false;
            }
            Message::PianoNrpnKindSelected(kind) => {
                let mut state = self.state.write().expect("state lock poisoned");
                state.piano_controller_lane = crate::message::PianoControllerLane::Nrpn;
                state.piano_nrpn_kind = kind;
                state.piano_sysex_panel_open = false;
            }
            Message::PianoKeyPressed(note, velocity) => {
                let track_name = self
                    .state
                    .read()
                    .expect("state lock poisoned")
                    .piano
                    .as_ref()
                    .map(|p| p.track_idx.clone());
                if let Some(track_name) = track_name {
                    return self.send(Action::PianoKey {
                        track_name,
                        note,
                        velocity,
                        on: true,
                    });
                }
            }
            Message::PianoKeyReleased(note) => {
                let track_name = self
                    .state
                    .read()
                    .expect("state lock poisoned")
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
            Message::DrumKeyPressed(note, velocity) => {
                let track_name = self
                    .state
                    .read()
                    .expect("state lock poisoned")
                    .piano
                    .as_ref()
                    .map(|p| p.track_idx.clone());
                if let Some(track_name) = track_name {
                    return self.send(Action::PianoKey {
                        track_name,
                        note,
                        velocity,
                        on: true,
                    });
                }
            }
            Message::DrumKeyReleased(note) => {
                let track_name = self
                    .state
                    .read()
                    .expect("state lock poisoned")
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
                let mut state = self.state.write().expect("state lock poisoned");
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
                let mut state = self.state.write().expect("state lock poisoned");
                if let Some(ref mut dragging) = state.piano_dragging_notes {
                    dragging.current_point = position;
                }
            }
            Message::PianoNotesEndDrag => {
                let mut state = self.state.write().expect("state lock poisoned");
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
                        (self.transport.playback_rate_hz * 60.0 / tempo) * (4.0 / tsig_denom);
                    let samples_per_bar = samples_per_beat * tsig_num;
                    let total_samples =
                        (samples_per_bar * self.ui.zoom_visible_bars as f64).max(1.0);
                    let pps = ((editor_width as f64 / total_samples) as f32 * zoom_x).max(1.0e-6);

                    let delta_x = dragging.current_point.x - dragging.start_point.x;
                    let delta_y = dragging.current_point.y - dragging.start_point.y;

                    let delta_samples = (delta_x / pps) as i64;
                    let delta_pitch = -(delta_y / row_h).round() as i8;

                    let snap_sample = |sample: f64| -> usize {
                        if matches!(
                            self.timing.midi_snap_mode,
                            crate::message::SnapMode::NoSnap | crate::message::SnapMode::Clips
                        ) {
                            return sample.max(0.0) as usize;
                        }
                        self.timing
                            .midi_snap_mode
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
                                        mpe: mpe_widgets_to_engine(&note.mpe),
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
                            .map(piano_note_to_engine)
                            .collect();
                        let old_notes: Vec<maolan_engine::message::MidiNoteData> = dragging
                            .original_notes
                            .iter()
                            .map(piano_note_to_engine)
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
                let mut state = self.state.write().expect("state lock poisoned");
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
                let mut state = self.state.write().expect("state lock poisoned");
                if let Some(ref mut dragging) = state.pitch_correction_dragging_points {
                    dragging.current_point = position;
                }
            }
            Message::PitchCorrectionPointsEndDrag => {
                let mut state = self.state.write().expect("state lock poisoned");
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
                let mut state = self.state.write().expect("state lock poisoned");
                state.pitch_correction_dragging_points = None;
                state.pitch_correction_selecting_rect = Some((position, position));
            }
            Message::PitchCorrectionSelectRectDrag { position } => {
                let base_pps = self.pixels_per_sample();
                let mut state = self.state.write().expect("state lock poisoned");
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
                let mut state = self.state.write().expect("state lock poisoned");
                state.pitch_correction_selecting_rect = None;
            }
            Message::PitchCorrectionClearSelection => {
                let mut state = self.state.write().expect("state lock poisoned");
                state.pitch_correction_selected_points.clear();
                state.pitch_correction_dragging_points = None;
                state.pitch_correction_selecting_rect = None;
            }
            Message::SelectAll => {
                let view = self.state.read().expect("state lock poisoned").view.clone();
                if matches!(view, crate::state::View::Piano) {
                    let mut state = self.state.write().expect("state lock poisoned");
                    let all_notes = state
                        .piano
                        .as_ref()
                        .map(|piano| {
                            (0..piano.notes.len()).collect::<std::collections::HashSet<_>>()
                        })
                        .unwrap_or_default();
                    state.piano_selected_notes = all_notes;
                    state.piano_selecting_rect = None;
                    state.piano_dragging_notes = None;
                    return Task::none();
                }
                if matches!(view, crate::state::View::PitchCorrection) {
                    let mut state = self.state.write().expect("state lock poisoned");
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
                let mut state = self.state.write().expect("state lock poisoned");
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
                self.state
                    .write()
                    .expect("state lock poisoned")
                    .pitch_correction_inertia_ms = value.min(1000);
                return self.sync_pitch_correction_realtime();
            }
            Message::PitchCorrectionFormantCompensationChanged(enabled) => {
                self.state
                    .write()
                    .expect("state lock poisoned")
                    .pitch_correction_formant_compensation = enabled;
                return self.sync_pitch_correction_realtime();
            }
            Message::PitchCorrectionDetectorChanged(detector) => {
                self.state
                    .write()
                    .expect("state lock poisoned")
                    .pitch_correction_detector = detector;
                let target = {
                    let state = self.state.read().expect("state lock poisoned");
                    state
                        .pitch_correction
                        .as_ref()
                        .map(|pc| (pc.track_idx.clone(), pc.clip_index))
                };
                let Some((track_idx, clip_idx)) = target else {
                    return Task::none();
                };
                {
                    let mut state = self.state.write().expect("state lock poisoned");
                    let Some(clip) = state
                        .tracks
                        .iter_mut()
                        .find(|t| t.name == track_idx)
                        .and_then(|t| t.audio.clips.get_mut(clip_idx))
                    else {
                        return Task::none();
                    };
                    clip.pitch_correction_detector = detector;
                    // Force re-analysis with the new detector and drop any
                    // preview rendered from the old detector's points.
                    clip.pitch_correction_points.clear();
                    clip.pitch_correction_preview_name = None;
                }
                return self.open_clip_pitch_correction(track_idx, clip_idx);
            }
            Message::PitchCorrectionModeChanged(mode) => {
                self.state
                    .write()
                    .expect("state lock poisoned")
                    .pitch_correction_mode = mode;
                let target = {
                    let state = self.state.read().expect("state lock poisoned");
                    state
                        .pitch_correction
                        .as_ref()
                        .map(|pc| (pc.track_idx.clone(), pc.clip_index))
                };
                let Some((track_idx, clip_idx)) = target else {
                    return Task::none();
                };
                {
                    let mut state = self.state.write().expect("state lock poisoned");
                    let Some(clip) = state
                        .tracks
                        .iter_mut()
                        .find(|t| t.name == track_idx)
                        .and_then(|t| t.audio.clips.get_mut(clip_idx))
                    else {
                        return Task::none();
                    };
                    clip.pitch_correction_mode = mode;
                    if mode == maolan_engine::message::PitchCorrectionMode::Shift {
                        // Back to live shifting: drop the resynthesis preview
                        // so the engine applies the points in realtime.
                        clip.pitch_correction_preview_name = None;
                    }
                }
                // Resynth mode re-renders through sync_pitch_correction_realtime.
                return self.sync_pitch_correction_realtime();
            }
            Message::ClipPitchCorrectionResynthFinished {
                track_idx,
                clip_index,
                clip_name,
                clip_start,
                result,
            } => {
                self.generate.resynth_render_in_progress = false;
                let clip_state = {
                    let state = self.state.read().expect("state lock poisoned");
                    state
                        .tracks
                        .iter()
                        .find(|t| t.name == track_idx)
                        .and_then(|t| t.audio.clips.get(clip_index))
                        .map(|clip| (clip.name.clone(), clip.start))
                };
                let Some((current_name, current_start)) = clip_state else {
                    return Task::none();
                };
                if current_name != clip_name || current_start != clip_start {
                    return Task::none();
                }
                match result {
                    Ok((preview_name, _)) => {
                        self.state.write().expect("state lock poisoned").message =
                            format!("Resynthesized preview ready for '{clip_name}'");
                        if let Some(clip) = self
                            .state
                            .write()
                            .expect("state lock poisoned")
                            .tracks
                            .iter_mut()
                            .find(|t| t.name == track_idx)
                            .and_then(|t| t.audio.clips.get_mut(clip_index))
                        {
                            clip.pitch_correction_preview_name = Some(preview_name);
                        }
                        // Push the preview to the engine directly (sync would
                        // treat the fresh preview as stale and re-render).
                        if let Some(action) = self.current_pitch_correction_action() {
                            return self.send(action);
                        }
                        return Task::none();
                    }
                    Err(e) => {
                        self.state.write().expect("state lock poisoned").message =
                            format!("Resynthesis failed for '{clip_name}': {e}");
                        return Task::none();
                    }
                }
            }
            Message::PianoNoteResizeStart {
                note_index,
                position,
                resize_start,
            } => {
                let mut state = self.state.write().expect("state lock poisoned");
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
                let mut state = self.state.write().expect("state lock poisoned");
                if let Some(ref mut resizing) = state.piano_resizing_note {
                    resizing.current_point = position;
                }
            }
            Message::PianoNoteResizeEnd => {
                let mut state = self.state.write().expect("state lock poisoned");
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
                        (self.transport.playback_rate_hz * 60.0 / tempo) * (4.0 / tsig_denom);
                    let samples_per_bar = samples_per_beat * tsig_num;
                    let total_samples =
                        (samples_per_bar * self.ui.zoom_visible_bars as f64).max(1.0);
                    let pps = ((editor_width as f64 / total_samples) as f32 * zoom_x).max(1.0e-6);

                    let delta_x = resizing.current_point.x - resizing.start_point.x;
                    let delta_samples = (delta_x / pps) as i64;

                    let snap_sample = |sample: f64| -> usize {
                        if matches!(
                            self.timing.midi_snap_mode,
                            crate::message::SnapMode::NoSnap | crate::message::SnapMode::Clips
                        ) {
                            return sample.max(0.0) as usize;
                        }
                        self.timing
                            .midi_snap_mode
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

                        let new_note = piano_note_to_engine(note);
                        let old_note = piano_note_to_engine(original);

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
                let mut state = self.state.write().expect("state lock poisoned");
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
                    let old_note = piano_note_to_engine(note);
                    let new_velocity =
                        (i16::from(note.velocity) + i16::from(delta)).clamp(0, 127) as u8;
                    if new_velocity == note.velocity {
                        continue;
                    }
                    note.velocity = new_velocity;
                    let new_note = piano_note_to_engine(note);
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
                let mut state = self.state.write().expect("state lock poisoned");
                let Some(piano) = state.piano.as_mut() else {
                    return Task::none();
                };
                let Some(note) = piano.notes.get_mut(note_index) else {
                    return Task::none();
                };
                if note.velocity == velocity {
                    return Task::none();
                }
                let old_note = piano_note_to_engine(note);
                note.velocity = velocity;
                let new_note = piano_note_to_engine(note);
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
                let mut state = self.state.write().expect("state lock poisoned");
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
                let mut state = self.state.write().expect("state lock poisoned");
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
                let mut state = self.state.write().expect("state lock poisoned");
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
                let mut state = self.state.write().expect("state lock poisoned");
                state.piano_selected_sysex = index;
                state.piano_sysex_hex_input = index
                    .and_then(|idx| state.piano.as_ref()?.sysexes.get(idx).cloned())
                    .map(|ev| Self::format_sysex_hex(&ev.data))
                    .unwrap_or_default();
            }
            Message::PianoSysExOpenEditor(index) => {
                let mut state = self.state.write().expect("state lock poisoned");
                state.piano_controller_lane = crate::message::PianoControllerLane::SysEx;
                state.piano_selected_sysex = index;
                state.piano_sysex_hex_input = index
                    .and_then(|idx| state.piano.as_ref()?.sysexes.get(idx).cloned())
                    .map(|ev| Self::format_sysex_hex(&ev.data))
                    .unwrap_or_default();
                state.piano_sysex_panel_open = true;
            }
            Message::PianoSysExCloseEditor => {
                self.state
                    .write()
                    .expect("state lock poisoned")
                    .piano_sysex_panel_open = false;
            }
            Message::PianoSysExHexInput(ref input) => {
                self.state
                    .write()
                    .expect("state lock poisoned")
                    .piano_sysex_hex_input = input.clone();
            }
            Message::PianoSysExAdd => {
                let mut state = self.state.write().expect("state lock poisoned");
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
                let mut state = self.state.write().expect("state lock poisoned");
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
                let mut state = self.state.write().expect("state lock poisoned");
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
                let mut state = self.state.write().expect("state lock poisoned");
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
                let mut state = self.state.write().expect("state lock poisoned");
                if !state.shift {
                    state.piano_selected_notes.clear();
                }
                state.piano_selecting_rect = Some((position, position));
            }
            Message::PianoSelectRectDrag { position } => {
                let mut state = self.state.write().expect("state lock poisoned");
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
                        (self.transport.playback_rate_hz * 60.0 / tempo) * (4.0 / tsig_denom);
                    let samples_per_bar = samples_per_beat * tsig_num;
                    let total_samples =
                        (samples_per_bar * self.ui.zoom_visible_bars as f64).max(1.0);
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
                let mut state = self.state.write().expect("state lock poisoned");
                state.piano_selecting_rect = None;
            }
            Message::PianoCreateNoteStart { position, repeat } => {
                let mut state = self.state.write().expect("state lock poisoned");
                state.piano_selected_notes.clear();
                state.piano_creating_note = Some(crate::state::CreatingPianoNote {
                    start_point: position,
                    current_point: position,
                    repeat,
                });
                state.piano_painted_notes.clear();
            }
            Message::PianoCreateNoteDrag { position } => {
                let mut state = self.state.write().expect("state lock poisoned");
                let Some(mut creating) = state.piano_creating_note else {
                    return Task::none();
                };
                creating.current_point = position;
                state.piano_creating_note = Some(creating);
            }
            Message::PianoCreateNoteEnd { position } => {
                let mut state = self.state.write().expect("state lock poisoned");
                let creating = state.piano_creating_note.take();
                state.piano_creating_note = None;
                state.piano_painted_notes.clear();
                drop(state);
                let Some(creating) = creating else {
                    return Task::none();
                };
                if creating.repeat {
                    return self.insert_painted_piano_notes_between(creating.start_point, position);
                }
                return self.insert_stretched_piano_note_between(creating.start_point, position);
            }
            Message::PianoDeleteSelectedNotes => {
                let mut state = self.state.write().expect("state lock poisoned");
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
                                piano
                                    .notes
                                    .get(idx)
                                    .map(|note| (idx, piano_note_to_engine(note)))
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
                let mut state = self.state.write().expect("state lock poisoned");
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
                                piano
                                    .notes
                                    .get(idx)
                                    .map(|note| (idx, piano_note_to_engine(note)))
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
                let mut state = self.state.write().expect("state lock poisoned");
                if note_index == usize::MAX {
                    state.piano_selected_notes.clear();
                } else if state.shift {
                    if state.piano_selected_notes.contains(&note_index) {
                        state.piano_selected_notes.remove(&note_index);
                    } else {
                        state.piano_selected_notes.insert(note_index);
                    }
                } else if !state.piano_selected_notes.contains(&note_index) {
                    state.piano_selected_notes.clear();
                    state.piano_selected_notes.insert(note_index);
                }
            }
            Message::DrumNoteCreate {
                start_sample,
                end_sample,
                pitch,
                repeat,
            } => {
                if repeat {
                    self.state
                        .write()
                        .expect("state lock poisoned")
                        .piano_painted_notes
                        .clear();
                    return self.insert_painted_midi_notes_between(
                        start_sample as f64,
                        end_sample as f64,
                        pitch,
                    );
                }
                return self.insert_stretched_midi_note_between(
                    start_sample as f64,
                    end_sample as f64,
                    pitch,
                );
            }
            Message::DrumNoteDelete(note_index) => {
                let mut state = self.state.write().expect("state lock poisoned");
                if let Some(piano) = state.piano.as_mut() {
                    let track_name = piano.track_idx.clone();
                    let clip_idx = piano.clip_index;
                    if let Some(note) = piano.notes.get(note_index) {
                        let deleted_note = piano_note_to_engine(note);
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
                target_pitch,
            } => {
                let (
                    track_name,
                    clip_idx,
                    snapped_delta_samples,
                    target_indices,
                    drum_rows,
                    row_delta,
                    copy,
                ) = {
                    let state = self.state.read().expect("state lock poisoned");
                    let Some(piano) = state.piano.as_ref() else {
                        return Task::none();
                    };
                    let copy = state.ctrl;
                    let track_name = piano.track_idx.clone();
                    let clip_idx = piano.clip_index;
                    let tempo = state.tempo.max(1.0) as f64;
                    let tsig_num = state.time_signature_num.max(1) as f64;
                    let tsig_denom = state.time_signature_denom.max(1) as f64;
                    let samples_per_beat =
                        (self.transport.playback_rate_hz * 60.0 / tempo) * (4.0 / tsig_denom);
                    let samples_per_bar = samples_per_beat * tsig_num;
                    let snapped_delta_samples = Self::midi_drag_delta_samples(
                        self.timing.midi_snap_mode,
                        delta_samples,
                        samples_per_beat,
                        samples_per_bar,
                    );
                    let target_indices: Vec<usize> = if state.piano_selected_notes.is_empty() {
                        vec![note_index]
                    } else {
                        state.piano_selected_notes.iter().copied().collect()
                    };
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
                    for pitch in piano.midnam_note_names.keys() {
                        if !drum_rows.contains(pitch) {
                            drum_rows.push(*pitch);
                        }
                    }
                    if !drum_rows.contains(&target_pitch) {
                        drum_rows.push(target_pitch);
                    }
                    drum_rows.sort();
                    let row_delta = piano
                        .notes
                        .get(note_index)
                        .and_then(|note| {
                            let start_row = drum_rows.iter().position(|&p| p == note.pitch)?;
                            let target_row = drum_rows.iter().position(|&p| p == target_pitch)?;
                            Some(target_row as isize - start_row as isize)
                        })
                        .unwrap_or(0);
                    (
                        track_name,
                        clip_idx,
                        snapped_delta_samples,
                        target_indices,
                        drum_rows,
                        row_delta,
                        copy,
                    )
                };

                let mut state = self.state.write().expect("state lock poisoned");
                let Some(piano) = state.piano.as_mut() else {
                    return Task::none();
                };

                let mut note_indices = Vec::new();
                let mut new_notes = Vec::new();
                let mut old_notes = Vec::new();

                if copy {
                    let insert_base = piano.notes.len();
                    for idx in target_indices {
                        let Some(note) = piano.notes.get(idx) else {
                            continue;
                        };
                        let old_note = piano_note_to_engine(note);
                        let new_start = if snapped_delta_samples < 0 {
                            old_note
                                .start_sample
                                .saturating_sub((-snapped_delta_samples) as usize)
                        } else {
                            old_note
                                .start_sample
                                .saturating_add(snapped_delta_samples as usize)
                        };
                        let mut new_note = old_note.clone();
                        new_note.start_sample = new_start;
                        if let Some(row_idx) = drum_rows.iter().position(|&p| p == old_note.pitch) {
                            let target_row = (row_idx as isize + row_delta)
                                .clamp(0, drum_rows.len().saturating_sub(1) as isize)
                                as usize;
                            new_note.pitch = drum_rows[target_row];
                        }
                        new_notes.push(new_note);
                    }

                    state.piano_selected_notes.clear();
                    drop(state);
                    if !new_notes.is_empty() {
                        let notes = new_notes
                            .into_iter()
                            .enumerate()
                            .map(|(offset, note)| (insert_base + offset, note))
                            .collect();
                        return self.send(Action::InsertMidiNotes {
                            track_name,
                            clip_index: clip_idx,
                            notes,
                        });
                    }
                    return Task::none();
                }

                for idx in target_indices {
                    let Some(note) = piano.notes.get(idx) else {
                        continue;
                    };
                    let old_note = piano_note_to_engine(note);
                    let new_start = if snapped_delta_samples < 0 {
                        old_note
                            .start_sample
                            .saturating_sub((-snapped_delta_samples) as usize)
                    } else {
                        old_note
                            .start_sample
                            .saturating_add(snapped_delta_samples as usize)
                    };
                    if let Some(note) = piano.notes.get_mut(idx) {
                        note.start_sample = new_start;
                        if let Some(row_idx) = drum_rows.iter().position(|&p| p == note.pitch) {
                            let target_row = (row_idx as isize + row_delta)
                                .clamp(0, drum_rows.len().saturating_sub(1) as isize)
                                as usize;
                            note.pitch = drum_rows[target_row];
                        }
                    }
                    let mut new_note = old_note.clone();
                    new_note.start_sample = new_start;
                    if let Some(row_idx) = drum_rows.iter().position(|&p| p == old_note.pitch) {
                        let target_row = (row_idx as isize + row_delta)
                            .clamp(0, drum_rows.len().saturating_sub(1) as isize)
                            as usize;
                        new_note.pitch = drum_rows[target_row];
                    }

                    note_indices.push(idx);
                    new_notes.push(new_note);
                    old_notes.push(old_note);
                }

                drop(state);
                if !note_indices.is_empty() {
                    return self.send(Action::ModifyMidiNotes {
                        track_name,
                        clip_index: clip_idx,
                        note_indices,
                        new_notes,
                        old_notes,
                    });
                }
            }
            Message::DrumSelectRectStart { position } => {
                let mut state = self.state.write().expect("state lock poisoned");
                if !state.shift {
                    state.piano_selected_notes.clear();
                }
                state.piano_selecting_rect = Some((position, position));
            }
            Message::DrumSelectRectDrag { position } => {
                let mut state = self.state.write().expect("state lock poisoned");
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
                        (self.transport.playback_rate_hz * 60.0 / tempo) * (4.0 / tsig_denom);
                    let samples_per_bar = samples_per_beat * tsig_num;
                    let total_samples =
                        (samples_per_bar * self.ui.zoom_visible_bars as f64).max(1.0);
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
                let mut state = self.state.write().expect("state lock poisoned");
                state.piano_selecting_rect = None;
            }
            Message::PianoDeleteControllers {
                ref controller_indices,
            } => {
                let mut state = self.state.write().expect("state lock poisoned");
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
            Message::PianoSetMpeValue {
                note_index,
                lane,
                point_index,
                value,
            } => {
                let mut state = self.state.write().expect("state lock poisoned");
                let Some(piano) = state.piano.as_mut() else {
                    return Task::none();
                };
                let Some(note) = piano.notes.get_mut(note_index) else {
                    return Task::none();
                };
                let value_max = if matches!(lane, crate::message::PianoControllerLane::MpePitchBend)
                {
                    16383
                } else {
                    127
                };
                let clamped = (value as usize).min(value_max) as u16;
                let old_note = piano_note_to_engine(&*note);
                {
                    let curve = match lane {
                        crate::message::PianoControllerLane::MpePitchBend => {
                            &mut note.mpe.pitch_bend
                        }
                        crate::message::PianoControllerLane::MpePressure => &mut note.mpe.pressure,
                        crate::message::PianoControllerLane::MpeTimbre => &mut note.mpe.timbre,
                        _ => return Task::none(),
                    };
                    let Some(point) = curve.points.get_mut(point_index) else {
                        return Task::none();
                    };
                    if point.value == clamped {
                        return Task::none();
                    }
                    point.value = clamped;
                }
                let new_note = piano_note_to_engine(&*note);
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
            Message::PianoInsertMpePoints {
                note_index,
                lane,
                points,
            } => {
                if points.is_empty() {
                    return Task::none();
                }
                let mut state = self.state.write().expect("state lock poisoned");
                let Some(piano) = state.piano.as_mut() else {
                    return Task::none();
                };
                let Some(note) = piano.notes.get_mut(note_index) else {
                    return Task::none();
                };
                let old_note = piano_note_to_engine(&*note);
                let value_max = if matches!(lane, crate::message::PianoControllerLane::MpePitchBend)
                {
                    16383
                } else {
                    127
                };
                {
                    let curve = match lane {
                        crate::message::PianoControllerLane::MpePitchBend => {
                            &mut note.mpe.pitch_bend
                        }
                        crate::message::PianoControllerLane::MpePressure => &mut note.mpe.pressure,
                        crate::message::PianoControllerLane::MpeTimbre => &mut note.mpe.timbre,
                        _ => return Task::none(),
                    };
                    for mut point in points {
                        point.value = (point.value as usize).min(value_max) as u16;
                        curve.points.push(point);
                    }
                    curve.points.sort_unstable_by_key(|p| p.sample_offset);
                }
                let new_note = piano_note_to_engine(&*note);
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
            Message::PianoDeleteMpePoints {
                note_index,
                lane,
                point_indices,
            } => {
                let mut state = self.state.write().expect("state lock poisoned");
                let Some(piano) = state.piano.as_mut() else {
                    return Task::none();
                };
                let Some(note) = piano.notes.get_mut(note_index) else {
                    return Task::none();
                };
                let old_note = piano_note_to_engine(&*note);
                let old_len = match lane {
                    crate::message::PianoControllerLane::MpePitchBend => {
                        old_note.mpe.pitch_bend.points.len()
                    }
                    crate::message::PianoControllerLane::MpePressure => {
                        old_note.mpe.pressure.points.len()
                    }
                    crate::message::PianoControllerLane::MpeTimbre => {
                        old_note.mpe.timbre.points.len()
                    }
                    _ => return Task::none(),
                };
                {
                    let curve = match lane {
                        crate::message::PianoControllerLane::MpePitchBend => {
                            &mut note.mpe.pitch_bend
                        }
                        crate::message::PianoControllerLane::MpePressure => &mut note.mpe.pressure,
                        crate::message::PianoControllerLane::MpeTimbre => &mut note.mpe.timbre,
                        _ => return Task::none(),
                    };
                    let mut indices = point_indices.clone();
                    indices.sort_unstable();
                    indices.dedup();
                    for idx in indices.into_iter().rev() {
                        if idx < curve.points.len() {
                            curve.points.remove(idx);
                        }
                    }
                    if curve.points.len() == old_len {
                        return Task::none();
                    }
                }
                let new_note = piano_note_to_engine(&*note);
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
            Message::PianoQuantizeSelectedNotes => {
                let interval = self
                    .timing
                    .snap_interval_samples(
                        self.transport.samples_per_beat(&self.state),
                        self.transport.samples_per_bar(&self.state),
                    )
                    .max(1);
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
                    let state = self.state.read().expect("state lock poisoned");
                    (state.piano_scale_root.semitone(), state.piano_scale_minor)
                };
                return self.selected_piano_notes_edit(move |_idx, note| {
                    let mut out = note.clone();
                    out.pitch = Self::nearest_scale_pitch(note.pitch, root, minor);
                    out
                });
            }
            Message::PianoChordSelectedNotes => {
                let chord_kind = self
                    .state
                    .read()
                    .expect("state lock poisoned")
                    .piano_chord_kind;
                let state = self.state.write().expect("state lock poisoned");
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
                                mpe: Default::default(),
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
                let state = self.state.read().expect("state lock poisoned");
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
                    .read()
                    .expect("state lock poisoned")
                    .piano_velocity_shape_amount
                    .clamp(0.0, 1.0);
                let state = self.state.read().expect("state lock poisoned");
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
                let interval = self
                    .timing
                    .snap_interval_samples(
                        self.transport.samples_per_beat(&self.state),
                        self.transport.samples_per_bar(&self.state),
                    )
                    .max(1) as i64;
                let (time_amount, vel_amount) = {
                    let state = self.state.read().expect("state lock poisoned");
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
                let interval = self
                    .timing
                    .snap_interval_samples(
                        self.transport.samples_per_beat(&self.state),
                        self.transport.samples_per_bar(&self.state),
                    )
                    .max(1);
                let amount = self
                    .state
                    .read()
                    .expect("state lock poisoned")
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
                self.state
                    .write()
                    .expect("state lock poisoned")
                    .piano_humanize_time_amount = value.clamp(0.0, 1.0);
            }
            Message::PianoHumanizeVelocityAmountChanged(value) => {
                self.state
                    .write()
                    .expect("state lock poisoned")
                    .piano_humanize_velocity_amount = value.clamp(0.0, 1.0);
            }
            Message::PianoGrooveAmountChanged(value) => {
                self.state
                    .write()
                    .expect("state lock poisoned")
                    .piano_groove_amount = value.clamp(0.0, 1.0);
            }
            Message::PianoScaleRootSelected(root) => {
                self.state
                    .write()
                    .expect("state lock poisoned")
                    .piano_scale_root = root;
            }
            Message::PianoScaleMinorToggled(minor) => {
                self.state
                    .write()
                    .expect("state lock poisoned")
                    .piano_scale_minor = minor;
            }
            Message::PianoShowNoteNames(show) => {
                self.state
                    .write()
                    .expect("state lock poisoned")
                    .piano_show_note_names = show;
            }
            Message::PianoChordKindSelected(kind) => {
                self.state
                    .write()
                    .expect("state lock poisoned")
                    .piano_chord_kind = kind;
            }
            Message::PianoVelocityShapeAmountChanged(value) => {
                self.state
                    .write()
                    .expect("state lock poisoned")
                    .piano_velocity_shape_amount = value.clamp(0.0, 1.0);
            }
            Message::MidiClipPreviewLoaded {
                ref track_idx,
                clip_idx,
                ref clip_name,
                ref notes,
            } => {
                self.pending.pending_midi_clip_previews.remove(&(
                    track_idx.clone(),
                    clip_idx,
                    clip_name.clone(),
                ));
                let valid = {
                    let state = self.state.read().expect("state lock poisoned");
                    state
                        .tracks
                        .iter()
                        .find(|track| track.name == *track_idx)
                        .and_then(|track| track.midi.clips.get(clip_idx))
                        .is_some_and(|clip| clip.name == *clip_name)
                };
                if valid {
                    self.transport.midi_clip_previews.insert(
                        (track_idx.clone(), clip_idx),
                        std::sync::Arc::new(notes.clone()),
                    );
                }
            }
            Message::OpenMidiPiano {
                ref track_idx,
                clip_idx,
            } => {
                if let Some(clip_length_samples) = {
                    let state = self.state.read().expect("state lock poisoned");
                    if let Some(piano) = &state.piano
                        && piano.track_idx == *track_idx
                        && piano.clip_index == clip_idx
                    {
                        Some(piano.clip_length_samples)
                    } else {
                        None
                    }
                } {
                    let fit_zoom_x = self.piano_fit_clip_zoom_x(clip_length_samples);
                    {
                        let mut state = self.state.write().expect("state lock poisoned");
                        state.piano_zoom_x = fit_zoom_x;
                        state.piano_scroll_x = 0.0;
                        state.view = View::Piano;
                    }
                    return Task::batch(vec![self.sync_piano_scrollbars()]);
                }
                let (clip_name, clip_length, clip_start) = {
                    let state = self.state.read().expect("state lock poisoned");
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
                match Self::parse_midi_clip_for_piano(
                    &path,
                    self.transport.playback_rate_hz,
                    clip_start,
                ) {
                    Ok((notes, controllers, sysexes, parsed_len)) => {
                        let clip_length_samples = parsed_len.max(clip_length);
                        let fit_zoom_x = self.piano_fit_clip_zoom_x(clip_length_samples);
                        self.transport.midi_clip_previews.insert(
                            (track_idx.clone(), clip_idx),
                            std::sync::Arc::new(notes.clone()),
                        );
                        self.pending.pending_midi_clip_previews.remove(&(
                            track_idx.clone(),
                            clip_idx,
                            clip_name.clone(),
                        ));
                        {
                            let mut state = self.state.write().expect("state lock poisoned");
                            state.piano = Some(PianoData {
                                track_idx: track_idx.clone(),
                                clip_index: clip_idx,
                                clip_start_samples: clip_start,
                                clip_length_samples,
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
                            state.piano_zoom_x = fit_zoom_x;
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
                            #[cfg(unix)]
                            {
                                let mut tasks = tasks;
                                tasks.push(self.send(Action::TrackGetLv2Midnam {
                                    track_name: track_idx.clone(),
                                }));
                                return Task::batch(tasks);
                            }
                            #[cfg(not(unix))]
                            return Task::batch(tasks);
                        }
                    }
                    Err(e) => {
                        self.state.write().expect("state lock poisoned").message =
                            format!("Failed to open MIDI clip '{}': {}", clip_name, e);
                    }
                }
            }
            _ => {}
        }
        self.update_children(&message);
        Task::none()
    }
}
