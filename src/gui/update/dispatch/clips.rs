use super::*;

impl Maolan {
    pub(super) fn handle_clips_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::MarkerLaneCreate { sample } => {
                self.track_marker.open(crate::state::MarkerDialog {
                    sample,
                    marker_index: None,
                    name: String::new(),
                });
                return maolan_widgets::iced::widget::operation::focus(
                    crate::track_marker::MarkerView::name_input_id(),
                );
            }
            Message::MarkerNameInput(_) => {}
            Message::MarkerNameConfirm => {
                let dialog = self.track_marker.dialog.clone();
                let Some(dialog) = dialog else {
                    return Task::none();
                };
                let marker_name = dialog.name.trim().to_string();
                if marker_name.is_empty() {
                    return Task::none();
                }
                let mut state = self.state.write().expect("state lock poisoned");
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
                self.track_marker.close();
            }
            Message::MarkerNameCancel => {
                self.track_marker.close();
            }
            Message::SelectClip {
                ref track_idx,
                clip_idx,
                kind,
            } => {
                use crate::state::ClipId;
                let ctrl = self.state.read().expect("state lock poisoned").ctrl;
                let mut state = self.state.write().expect("state lock poisoned");

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
                self.drag.clip = Some(dragged);
            }
            Message::ClipRenameShow {
                ref track_idx,
                clip_idx,
                kind,
            } => {
                let current_name = self
                    .state
                    .read()
                    .expect("state lock poisoned")
                    .tracks
                    .iter()
                    .find(|t| t.name == *track_idx)
                    .and_then(|t| match kind {
                        Kind::Audio => t.audio.clips.get(clip_idx).map(|c| c.name.clone()),
                        Kind::MIDI => t.midi.clips.get(clip_idx).map(|c| c.name.clone()),
                    })
                    .unwrap_or_default();

                let clean_name = crate::clip_rename::clean_clip_name(&current_name);

                self.clip_rename.open(crate::state::ClipRenameDialog {
                    track_idx: track_idx.clone(),
                    clip_idx,
                    kind,
                    new_name: clean_name,
                });
            }
            Message::ClipRenameInput(_) => {}
            Message::ClipRenameConfirm => {
                let dialog = self.clip_rename.dialog.clone();
                let Some(dialog) = dialog else {
                    return Task::none();
                };

                let new_name = dialog.new_name.trim().to_string();
                if new_name.is_empty() {
                    return Task::none();
                }

                let Some(session_dir) = self.session_dir.clone() else {
                    self.state.write().expect("state lock poisoned").message =
                        "No session loaded".to_string();
                    self.clip_rename.close();
                    return Task::none();
                };

                let Some((_id, old_name, matching_id_count, _used_names)) =
                    self.clip_identity_context(&dialog.track_idx, dialog.clip_idx, dialog.kind)
                else {
                    let mut state = self.state.write().expect("state lock poisoned");
                    state.message = "Clip not found".to_string();
                    self.clip_rename.close();
                    return Task::none();
                };
                let new_file_name =
                    Self::clip_file_name_for_rename(dialog.kind, &old_name, &new_name);
                let new_path = session_dir.join(&new_file_name);
                if new_path.exists() {
                    let mut state = self.state.write().expect("state lock poisoned");
                    state.message = format!("File '{}' already exists", new_file_name);
                    self.clip_rename.close();
                    return Task::none();
                }

                if dialog.kind == Kind::Audio && matching_id_count <= 1 {
                    let old_path = session_dir.join(&old_name);
                    if old_path.exists()
                        && let Err(e) = std::fs::rename(&old_path, &new_path)
                    {
                        let mut state = self.state.write().expect("state lock poisoned");
                        state.message = format!("Failed to rename file: {}", e);
                        self.clip_rename.close();
                        return Task::none();
                    }
                }

                {
                    let mut state = self.state.write().expect("state lock poisoned");
                    state.message = format!("Renamed to '{}'", new_name);
                    self.clip_rename.close();
                }

                return self.send(Action::RenameClip {
                    track_name: dialog.track_idx,
                    kind: dialog.kind,
                    clip_index: dialog.clip_idx,
                    new_name,
                });
            }
            Message::ClipRenameCancel => {
                self.clip_rename.close();
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
                    let mut state = self.state.write().expect("state lock poisoned");
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
                        let state = self.state.read().expect("state lock poisoned");
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
            Message::ClipReverse {
                ref track_idx,
                clip_idx,
                kind,
            } => {
                return self.reverse_clips_from_context_menu(track_idx.clone(), clip_idx, kind);
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
            Message::ClipExportPitchCorrectionMidi {
                ref track_idx,
                clip_idx,
            } => {
                let Some(file_name) = ({
                    let state = self.state.read().expect("state lock poisoned");
                    let clip = state
                        .tracks
                        .iter()
                        .find(|t| t.name == *track_idx)
                        .and_then(|t| t.audio.clips.get(clip_idx));
                    clip.and_then(|clip| {
                        let has_points = !clip.pitch_correction_points.is_empty()
                            || state.pitch_correction.as_ref().is_some_and(|pitch| {
                                pitch.track_idx == *track_idx
                                    && pitch.clip_index == clip_idx
                                    && !pitch.points.is_empty()
                            });
                        has_points.then(|| {
                            let stem = std::path::Path::new(&clip.name)
                                .file_stem()
                                .and_then(|stem| stem.to_str())
                                .unwrap_or("pitch");
                            format!("{stem}_pitch.mid")
                        })
                    })
                }) else {
                    self.state.write().expect("state lock poisoned").message =
                        "No pitch correction data to export".to_string();
                    return Task::none();
                };
                let track_idx = track_idx.clone();
                return Task::perform(
                    async move {
                        let path = AsyncFileDialog::new()
                            .set_title("Export Pitch MIDI")
                            .add_filter("MIDI", &["mid", "midi"])
                            .set_file_name(file_name)
                            .save_file()
                            .await
                            .map(|handle| handle.path().to_path_buf());
                        (track_idx, clip_idx, path)
                    },
                    |(track_idx, clip_idx, path)| Message::ClipPitchCorrectionMidiFileSelected {
                        track_idx,
                        clip_idx,
                        path,
                    },
                );
            }
            Message::ClipPitchCorrectionMidiFileSelected {
                ref track_idx,
                clip_idx,
                path: Some(ref path),
            } => {
                let (clip_name, points) = {
                    let state = self.state.read().expect("state lock poisoned");
                    let Some((clip_name, clip_points)) = state
                        .tracks
                        .iter()
                        .find(|t| t.name == *track_idx)
                        .and_then(|t| t.audio.clips.get(clip_idx))
                        .map(|clip| (clip.name.clone(), clip.pitch_correction_points.clone()))
                    else {
                        self.state.write().expect("state lock poisoned").message =
                            "Audio clip not found".to_string();
                        return Task::none();
                    };
                    let points = state
                        .pitch_correction
                        .as_ref()
                        .filter(|pitch| {
                            pitch.track_idx == *track_idx && pitch.clip_index == clip_idx
                        })
                        .map(|pitch| pitch.points.clone())
                        .filter(|points| !points.is_empty())
                        .unwrap_or(clip_points);
                    (clip_name, points)
                };
                if points.is_empty() {
                    self.state.write().expect("state lock poisoned").message =
                        format!("No pitch correction data to export for '{clip_name}'");
                    return Task::none();
                }
                match Self::write_pitch_correction_midi_file(
                    path,
                    self.transport.playback_rate_hz.max(1.0) as u32,
                    &points,
                ) {
                    Ok(note_count) => {
                        self.state.write().expect("state lock poisoned").message = format!(
                            "Exported {note_count} pitch MIDI note(s): {}",
                            path.display()
                        );
                    }
                    Err(err) => {
                        self.state.write().expect("state lock poisoned").message =
                            format!("Failed to export pitch MIDI for '{clip_name}': {err}");
                    }
                }
                return Task::none();
            }
            Message::ClipPitchCorrectionMidiFileSelected { path: None, .. } => {
                return Task::none();
            }
            Message::ClipOpenPitchCorrectionProgress {
                ref clip_name,
                progress,
                ref operation,
            } => {
                self.generate.clip_pitch_correction_in_progress = true;
                self.generate.clip_pitch_correction_progress = progress.clamp(0.0, 1.0);
                self.generate.clip_pitch_correction_clip_name = clip_name.clone();
                self.generate.clip_pitch_correction_operation = operation.clone();
                let percent = (progress.clamp(0.0, 1.0) * 100.0) as usize;
                self.state.write().expect("state lock poisoned").message =
                    if let Some(op) = operation {
                        format!("Pitch correction '{clip_name}' ({percent}%): {op}")
                    } else {
                        format!("Pitch correction '{clip_name}' ({percent}%)")
                    };
                return Task::none();
            }
            Message::ClipStretchFinished { request, result } => match result {
                Ok((new_name, new_length)) => {
                    let clip_state = {
                        let state = self.state.read().expect("state lock poisoned");
                        state
                            .tracks
                            .iter()
                            .find(|t| t.name == request.track_idx)
                            .and_then(|t| t.audio.clips.get(request.clip_idx))
                            .map(|clip| (clip.name.clone(), clip.start))
                    };
                    let Some((current_name, current_start)) = clip_state else {
                        self.state.write().expect("state lock poisoned").message =
                            "Stretched clip finished, but the original clip no longer exists"
                                .to_string();
                        return Task::none();
                    };
                    if current_name != request.clip_name || current_start != request.start {
                        self.state.write().expect("state lock poisoned").message =
                            "Discarded stale stretched clip result".to_string();
                        return Task::none();
                    }
                    let fade_in = request.fade_in_samples.min(new_length / 2);
                    let fade_out = request.fade_out_samples.min(new_length / 2);
                    self.state.write().expect("state lock poisoned").message = format!(
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
                    let mut state = self.state.write().expect("state lock poisoned");
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
                self.generate.clip_pitch_correction_in_progress = false;
                self.generate.clip_pitch_correction_progress = 0.0;
                self.generate.clip_pitch_correction_clip_name.clear();
                self.generate.clip_pitch_correction_operation = None;
                let clip_state = {
                    let state = self.state.read().expect("state lock poisoned");
                    state
                        .tracks
                        .iter()
                        .find(|t| t.name == request.track_idx)
                        .and_then(|t| t.audio.clips.get(request.clip_idx))
                        .map(|clip| (clip.name.clone(), clip.start))
                };
                let Some((current_name, current_start)) = clip_state else {
                    self.state.write().expect("state lock poisoned").message =
                        "Pitch correction data finished loading, but the original clip no longer exists"
                            .to_string();
                    return Task::none();
                };
                if current_name != request.clip_name || current_start != request.start {
                    self.state.write().expect("state lock poisoned").message =
                        "Discarded stale pitch correction result".to_string();
                    return Task::none();
                }
                match result {
                    Ok(mut pitch_correction) => {
                        let mut state = self.state.write().expect("state lock poisoned");
                        pitch_correction.track_idx = request.track_idx.clone();
                        pitch_correction.clip_index = request.clip_idx;
                        pitch_correction.frame_likeness = request.frame_likeness;
                        state.pitch_correction = Some(pitch_correction);
                        state.pitch_correction_frame_likeness = request.frame_likeness;
                        state.pitch_correction_detector = request.detector;
                        let clip_mode = state
                            .tracks
                            .iter()
                            .find(|t| t.name == request.track_idx)
                            .and_then(|t| t.audio.clips.get(request.clip_idx))
                            .map(|clip| clip.pitch_correction_mode);
                        state.pitch_correction_mode = clip_mode.unwrap_or_default();
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
                        if clip_mode == Some(maolan_engine::message::PitchCorrectionMode::Resynth) {
                            return self.start_resynth_preview_render(
                                request.track_idx.clone(),
                                request.clip_idx,
                            );
                        }
                    }
                    Err(e) => {
                        self.state.write().expect("state lock poisoned").message =
                            format!("Pitch correction failed for '{}': {e}", request.clip_name);
                    }
                }
                return Task::none();
            }
            Message::MousePressed(button)
                if self.modal.is_none()
                    && matches!(
                        self.state.read().expect("state lock poisoned").view,
                        View::Workspace
                    ) =>
            {
                if button == mouse::Button::Middle {
                    return self.split_clip_at_position(self.active_workspace_cursor());
                }
                match button {
                    mouse::Button::Left => {
                        let mut state = self.state.write().expect("state lock poisoned");
                        state.mouse_left_down = true;
                        state.clip_marquee_start = None;
                        state.clip_marquee_end = None;
                    }
                    mouse::Button::Right => {
                        let cursor = self.active_workspace_cursor();
                        let clip_hit = self.clip_at_position(cursor);
                        let anchor_for_hit = cursor;
                        let mut state = self.state.write().expect("state lock poisoned");
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
            Message::ClipResizeStart(ref kind, ref track_name, clip_index, is_right_side) => {
                self.drag.clip = None;
                let mut state = self.state.write().expect("state lock poisoned");
                if let Some(track) = state.tracks.iter().find(|t| t.name == *track_name) {
                    match kind {
                        Kind::Audio => {
                            let Some(clip) = track.audio.clips.get(clip_index) else {
                                return Task::none();
                            };
                            if clip.take_lane_locked {
                                return Task::none();
                            }
                            let stretch_mode = state.shift;
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
                self.drag.clip = None;
                let mut state = self.state.write().expect("state lock poisoned");
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
                    let state = self.state.read().expect("state lock poisoned");
                    let bottom_trigger_top = if self.ui.mixer_visible {
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
                    let mut state = self.state.write().expect("state lock poisoned");
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
                        let mut state = self.state.write().expect("state lock poisoned");
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
                        let samples_per_beat = self.transport.samples_per_beat(&self.state);
                        let samples_per_bar = self.transport.samples_per_bar(&self.state);
                        let snap_interval_samples = self
                            .timing
                            .snap_mode
                            .interval_samples(samples_per_beat, samples_per_bar)
                            .max(1.0) as f32;
                        let snap_sample_drag = |sample: f32, delta_samples: f32| {
                            self.timing.snap_mode.snap_sample_drag(
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
                        let clip_edge_snap_enabled = self.timing.clip_edge_snap_enabled();
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
                        let mut state = self.state.write().expect("state lock poisoned");
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
                                    // MIDI clips have no intrinsic upper
                                    // bound: they can be extended past their
                                    // content, leaving empty space.
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
                                        clip.length = updated_end.max(clip.start as f32) as usize
                                            - clip.start;
                                    } else {
                                        let right_edge = initial_value + initial_length;
                                        let max_start = (right_edge - min_length_samples).max(0.0);
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
                                        let start_delta = new_start as isize - clip.start as isize;
                                        clip.start = new_start as usize;
                                        clip.length = updated_length as usize;
                                        if start_delta >= 0 {
                                            clip.offset = (clip.offset + start_delta as usize)
                                                .min(clip.max_length_samples);
                                        } else {
                                            clip.offset =
                                                clip.offset.saturating_sub((-start_delta) as usize);
                                        }
                                    }
                                }
                            }
                        }
                        self.drag.clip_snap_targets = clip_snap_targets;
                    }
                    Some(Resizing::Tracks(initial_width, initial_mouse_x)) => {
                        let delta = position.x - initial_mouse_x;
                        self.state
                            .write()
                            .expect("state lock poisoned")
                            .tracks_width = Length::Fixed((initial_width + delta).max(80.0));
                    }
                    Some(Resizing::Mixer(initial_height, initial_mouse_y)) => {
                        let delta = position.y - initial_mouse_y;
                        self.state
                            .write()
                            .expect("state lock poisoned")
                            .mixer_height = Length::Fixed((initial_height - delta).max(60.0));
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
                        let mut state = self.state.write().expect("state lock poisoned");
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
                let mouse_left_down = self
                    .state
                    .read()
                    .expect("state lock poisoned")
                    .mouse_left_down;
                if mouse_left_down && !matches!(resizing, Some(Resizing::Clip { .. })) {
                    if let Some(active) = self.drag.clip.as_mut() {
                        active.end = position;
                        let mut tasks = vec![maolan_widgets::iced_drop::zones_on_point(
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
                    let mut state = self.state.write().expect("state lock poisoned");
                    if state.clip_marquee_start.is_some()
                        && self.drag.clip.is_none()
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
                let mouse_right_down = self
                    .state
                    .read()
                    .expect("state lock poisoned")
                    .mouse_right_down;
                if mouse_right_down
                    && !matches!(resizing, Some(Resizing::Clip { .. }))
                    && self.drag.clip.is_none()
                    && matches!(
                        self.state.read().expect("state lock poisoned").view,
                        View::Workspace
                    )
                    && self.modal.is_none()
                {
                    let can_start = self.midi_lane_at_position(position).is_some();
                    let mut state = self.state.write().expect("state lock poisoned");
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
                let resizing = self
                    .state
                    .read()
                    .expect("state lock poisoned")
                    .resizing
                    .clone();
                let can_start_midi_drag = self.midi_lane_at_position(position).is_some();
                let hovered_resize_handle = self.clip_resize_handle_at_position(position);
                let cut_preview_active = self
                    .state
                    .read()
                    .expect("state lock poisoned")
                    .cut_preview_active;
                let mut state = self.state.write().expect("state lock poisoned");
                state.editor_cursor = Some(position);
                state.hovered_clip_resize_handle = hovered_resize_handle;
                if state.mouse_left_down
                    && !matches!(resizing, Some(Resizing::Clip { .. }))
                    && self.drag.clip.is_none()
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
                    && self.drag.clip.is_none()
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
                if cut_preview_active
                    && matches!(
                        self.state.read().expect("state lock poisoned").view,
                        View::Workspace
                    )
                {
                    self.update_cut_indicator(position);
                }
            }
            Message::MouseReleased => {
                let active = std::mem::take(&mut self.automation.touch_active_keys);
                for (track_name, keys) in active {
                    if let Some(values) = self
                        .automation
                        .touch_automation_overrides
                        .get_mut(&track_name)
                    {
                        for key in keys {
                            values.remove(&key);
                        }
                        if values.is_empty() {
                            self.automation
                                .touch_automation_overrides
                                .remove(&track_name);
                        }
                    }
                }
                if self.modal.is_some() {
                    let mut state = self.state.write().expect("state lock poisoned");
                    state.mouse_left_down = false;
                    state.mouse_right_down = false;
                    state.clip_click_consumed = false;
                    state.clip_marquee_start = None;
                    state.clip_marquee_end = None;
                    state.midi_clip_create_start = None;
                    state.midi_clip_create_end = None;
                    self.drag.clip = None;
                    self.drag.clip_preview_target_track = None;
                    self.drag.clip_preview_target_valid = false;
                    self.drag.clip_preview_snap_adjust_samples = 0.0;
                    self.drag.clip_snap_targets.clear();
                    return Task::none();
                }
                let (resizing, marquee_start, marquee_end, create_start, create_end) = {
                    let mut state = self.state.write().expect("state lock poisoned");
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
                self.drag.clip_preview_snap_adjust_samples = 0.0;
                self.drag.clip_snap_targets.clear();
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
                    let state = self.state.read().expect("state lock poisoned");
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
                                let state = self.state.read().expect("state lock poisoned");
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
                        let action = Action::SetClipBounds {
                            track_name,
                            clip_index: index,
                            kind,
                            start,
                            length,
                            offset,
                        };
                        let initial_length = initial_length.max(1.0) as usize;
                        if length != initial_length {
                            return self
                                .send(self.action_with_confirmed_clip_length_change(action));
                        }
                        return self.send(action);
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

                    let state = self.state.read().expect("state lock poisoned");
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
                        let state = self.state.read().expect("state lock poisoned");
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
                        self.state
                            .write()
                            .expect("state lock poisoned")
                            .selected_clips = selected;
                        return Task::none();
                    }
                }
                if let Some(clip) = &mut self.drag.clip {
                    let moved = (clip.end.x - clip.start.x).abs() > 2.0
                        || (clip.end.y - clip.start.y).abs() > 2.0;
                    if !moved {
                        self.drag.clip = None;
                        self.drag.clip_preview_target_valid = false;
                        return Task::none();
                    }
                    return maolan_widgets::iced_drop::zones_on_point(
                        Message::HandleClipZones,
                        clip.end,
                        None,
                        None,
                    );
                }
                self.drag.clip_preview_target_track = None;
                self.drag.clip_preview_target_valid = false;
            }
            Message::ClipDrag(ref clip) => {
                if !self
                    .state
                    .read()
                    .expect("state lock poisoned")
                    .mouse_left_down
                {
                    return Task::none();
                }
                if self
                    .state
                    .read()
                    .expect("state lock poisoned")
                    .clip_marquee_start
                    .is_some()
                {
                    return Task::none();
                }
                if matches!(
                    self.state.read().expect("state lock poisoned").resizing,
                    Some(Resizing::Clip { .. })
                ) {
                    return Task::none();
                }
                match &mut self.drag.clip {
                    Some(active)
                        if active.kind == clip.kind
                            && active.index == clip.index
                            && active.track_index == clip.track_index =>
                    {
                        active.end = self.state.read().expect("state lock poisoned").cursor;
                    }
                    Some(_) => {}
                    None => {
                        let mut dragged = clip.clone();
                        let cursor = self.state.read().expect("state lock poisoned").cursor;
                        dragged.start = cursor;
                        dragged.end = cursor;
                        dragged.copy = self.state.read().expect("state lock poisoned").ctrl;
                        self.drag.clip = Some(dragged);
                        self.drag.clip_preview_snap_adjust_samples = 0.0;
                        self.drag.clip_snap_targets.clear();
                    }
                }
            }
            Message::HandleClipZones(ref zones) => {
                if let Some(clip) = &self.drag.clip {
                    let state = self.state.read().expect("state lock poisoned");
                    let from_track_name = &clip.track_index;
                    let to_track_zone = zones.iter().find(|(id, _)| {
                        state.tracks.iter().any(|t| Id::from(t.name.clone()) == *id)
                    });
                    let Some((to_track_id, to_track_rect)) = to_track_zone else {
                        self.drag.clip = None;
                        self.drag.clip_preview_target_valid = false;
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
                            self.drag.clip = None;
                            self.drag.clip_preview_target_track = None;
                            self.drag.clip_preview_target_valid = false;
                            self.drag.clip_preview_snap_adjust_samples = 0.0;
                            self.drag.clip_snap_targets.clear();
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
                                self.drag.clip_snap_targets = snap_targets;
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
                                    self.drag.clip = None;
                                    self.drag.clip_preview_target_track = None;
                                    self.drag.clip_preview_target_valid = false;
                                    self.drag.clip_preview_snap_adjust_samples = 0.0;
                                    self.drag.clip_snap_targets.clear();
                                    return Task::batch(tasks);
                                }
                                if clip_index >= from_track.audio.clips.len() {
                                    self.drag.clip = None;
                                    self.drag.clip_preview_target_valid = false;
                                    self.drag.clip_preview_snap_adjust_samples = 0.0;
                                    self.drag.clip_snap_targets.clear();
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
                                self.drag.clip = None;
                                self.drag.clip_preview_target_track = None;
                                self.drag.clip_preview_target_valid = false;
                                self.drag.clip_preview_snap_adjust_samples = 0.0;
                                self.drag.clip_snap_targets.clear();
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
                                self.drag.clip_snap_targets = snap_targets;
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
                                    self.drag.clip = None;
                                    self.drag.clip_preview_target_track = None;
                                    self.drag.clip_preview_target_valid = false;
                                    self.drag.clip_preview_snap_adjust_samples = 0.0;
                                    self.drag.clip_snap_targets.clear();
                                    return Task::batch(tasks);
                                }
                                if clip_index >= from_track.midi.clips.len() {
                                    self.drag.clip = None;
                                    self.drag.clip_preview_target_valid = false;
                                    self.drag.clip_preview_snap_adjust_samples = 0.0;
                                    self.drag.clip_snap_targets.clear();
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
                                self.drag.clip = None;
                                self.drag.clip_preview_target_track = None;
                                self.drag.clip_preview_target_valid = false;
                                self.drag.clip_preview_snap_adjust_samples = 0.0;
                                self.drag.clip_snap_targets.clear();
                                return task;
                            }
                        }
                    }
                }
                self.drag.clip = None;
                self.drag.clip_preview_target_track = None;
                self.drag.clip_preview_target_valid = false;
                self.drag.clip_preview_snap_adjust_samples = 0.0;
                self.drag.clip_snap_targets.clear();
                return Task::none();
            }
            Message::HandleClipPreviewZones(ref zones) => {
                if let Some(clip) = &self.drag.clip {
                    let state = self.state.read().expect("state lock poisoned");
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
                        self.drag.clip_preview_target_track = None;
                        self.drag.clip_preview_target_valid = false;
                        self.drag.clip_preview_snap_adjust_samples = 0.0;
                        self.drag.clip_snap_targets.clear();
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
                            self.drag.clip_preview_target_track = Some(to_track.name.clone());
                            self.drag.clip_preview_target_valid = true;
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
                            self.drag.clip_preview_snap_adjust_samples = snap_adjust;
                            self.drag.clip_snap_targets = snap_targets;
                        } else {
                            self.drag.clip_preview_target_track = Some(to_track.name.clone());
                            self.drag.clip_preview_target_valid = false;
                            self.drag.clip_preview_snap_adjust_samples = 0.0;
                            self.drag.clip_snap_targets.clear();
                        }
                    } else {
                        self.drag.clip_preview_target_track = None;
                        self.drag.clip_preview_target_valid = false;
                        self.drag.clip_preview_snap_adjust_samples = 0.0;
                        self.drag.clip_snap_targets.clear();
                    }
                } else {
                    self.drag.clip_preview_target_track = None;
                    self.drag.clip_preview_target_valid = false;
                    self.drag.clip_preview_snap_adjust_samples = 0.0;
                    self.drag.clip_snap_targets.clear();
                }
            }
            Message::PaneClipDragStart {
                ref source_track_name,
                ref clip_id,
                kind,
            } => {
                self.drag.dragging_pane_clip = Some(crate::state::DraggedSessionClip {
                    source_track_name: source_track_name.clone(),
                    clip_id: clip_id.clone(),
                    kind,
                });
            }
            Message::PaneClipDropped { point } if self.drag.dragging_pane_clip.is_some() => {
                return maolan_widgets::iced_drop::zones_on_point(
                    Message::HandlePaneClipDropZones,
                    point,
                    None,
                    None,
                );
            }
            Message::HandlePaneClipDropZones(ref zones) => {
                let Some(dragged) = self.drag.dragging_pane_clip.take() else {
                    return Task::none();
                };
                // Dropped over a live view session slot: hand off to the
                // session clip logic, which assigns the clip to the slot
                // (copying track clips to the target track or moving unused
                // pool clips, keeping their id).
                let over_slot = {
                    let state = self.state.read().expect("state lock poisoned");
                    let slot_map = live_session::build_slot_zone_map(&state.tracks, &state.session);
                    zones.iter().any(|(id, _)| slot_map.contains_key(id))
                };
                if over_slot {
                    self.drag.dragging_session_clip = Some(dragged);
                    return self.update(Message::SessionClipHandleZones(zones.clone()));
                }
                let kind = dragged.kind;
                let target_track = {
                    let state = self.state.read().expect("state lock poisoned");
                    zones
                        .iter()
                        .filter_map(|(id, _)| {
                            state
                                .tracks
                                .iter()
                                .find(|t| Id::from(t.name.clone()) == *id)
                        })
                        .find(|track| {
                            !track.is_master
                                && !track.is_folder
                                && track.name != METRONOME_TRACK_ID
                                && match kind {
                                    Kind::Audio => track.audio.ins > 0,
                                    Kind::MIDI => track.midi.ins > 0,
                                }
                        })
                        .map(|track| track.name.clone())
                };
                let Some(target_track) = target_track else {
                    return Task::none();
                };
                let position = self.active_workspace_cursor();
                let samples_per_beat = self.transport.samples_per_beat(&self.state);
                let samples_per_bar = self.transport.samples_per_bar(&self.state);
                let start = self.timing.snap_sample_to_bar(
                    position.x.max(0.0) / self.pixels_per_sample().max(1.0e-6),
                    samples_per_beat,
                    samples_per_bar,
                );
                let action = {
                    let state = self.state.read().expect("state lock poisoned");
                    match &dragged.source_track_name {
                        // From the unused pool: move the clip, keeping its id.
                        None => match kind {
                            Kind::Audio => state
                                .unused_audio_clips
                                .iter()
                                .find(|clip| clip.id == dragged.clip_id)
                                .map(|clip| {
                                    if clip.is_group() {
                                        let mut data = Self::audio_clip_to_data(clip);
                                        data.start = start;
                                        Action::AddGroupedClip {
                                            track_name: target_track.clone(),
                                            kind: Kind::Audio,
                                            audio_clip: Some(data),
                                            midi_clip: None,
                                        }
                                    } else {
                                        Self::audio_clip_add_action(
                                            &dragged.clip_id,
                                            &target_track,
                                            clip,
                                            start,
                                            clip.length,
                                        )
                                    }
                                }),
                            Kind::MIDI => state
                                .unused_midi_clips
                                .iter()
                                .find(|clip| clip.id == dragged.clip_id)
                                .map(|clip| {
                                    if clip.is_group() {
                                        let mut data = Self::midi_clip_to_data(clip);
                                        data.start = start;
                                        Action::AddGroupedClip {
                                            track_name: target_track.clone(),
                                            kind: Kind::MIDI,
                                            audio_clip: None,
                                            midi_clip: Some(data),
                                        }
                                    } else {
                                        Self::midi_clip_add_action(
                                            &dragged.clip_id,
                                            &target_track,
                                            clip,
                                            start,
                                            clip.length,
                                        )
                                    }
                                }),
                        },
                        // From a track: copy the clip to the drop position with fresh ids.
                        Some(source_track_name) => state
                            .tracks
                            .iter()
                            .find(|t| &t.name == source_track_name)
                            .and_then(|source| match kind {
                                Kind::Audio => source
                                    .audio
                                    .clips
                                    .iter()
                                    .find(|clip| clip.id == dragged.clip_id)
                                    .map(|clip| {
                                        if clip.is_group() {
                                            let mut data = Self::audio_clip_to_data(clip);
                                            data.start = start;
                                            Action::AddGroupedClip {
                                                track_name: target_track.clone(),
                                                kind: Kind::Audio,
                                                audio_clip: Some(data),
                                                midi_clip: None,
                                            }
                                        } else {
                                            Self::audio_clip_add_action(
                                                &clip.id,
                                                &target_track,
                                                clip,
                                                start,
                                                clip.length,
                                            )
                                        }
                                    }),
                                Kind::MIDI => source
                                    .midi
                                    .clips
                                    .iter()
                                    .find(|clip| clip.id == dragged.clip_id)
                                    .map(|clip| {
                                        if clip.is_group() {
                                            let mut data = Self::midi_clip_to_data(clip);
                                            data.start = start;
                                            Action::AddGroupedClip {
                                                track_name: target_track.clone(),
                                                kind: Kind::MIDI,
                                                audio_clip: None,
                                                midi_clip: Some(data),
                                            }
                                        } else {
                                            Self::midi_clip_add_action(
                                                &clip.id,
                                                &target_track,
                                                clip,
                                                start,
                                                clip.length,
                                            )
                                        }
                                    }),
                            }),
                    }
                };
                if let Some(action) = action {
                    self.try_send_engine(EngineMessage::Request(action));
                }
            }
            _ => {}
        }
        self.update_children(&message);
        Task::none()
    }
}
