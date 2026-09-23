use super::*;

impl Maolan {
    pub(super) fn handle_response_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Response(Ok(ref a)) => {
                match a {
                    Action::HistoryState { dirty } => {
                        self.session_ops.engine_dirty = *dirty;
                    }
                    Action::Log { source, message } => {
                        self.info(format!("[{source}] {message}"));
                    }
                    Action::TrackShowVst3Gui { .. }
                    | Action::ClipShowVst3Gui { .. }
                    | Action::TrackShowLv2Gui { .. }
                    | Action::ClipShowLv2Gui { .. } => {
                        self.pending.pending_native_ui_fallback = None;
                    }
                    _ if !self.session_ops.session_restore_in_progress
                        && history::should_record(a) =>
                    {
                        self.session_ops.engine_dirty = true;
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
                    Action::TrackClapResourceFiles {
                        track_name,
                        instance_id,
                        files,
                    } => {
                        let plugin_ref = crate::gui::PluginInstanceRef::Track {
                            track_name: track_name.clone(),
                            instance_id: *instance_id,
                        };
                        self.handle_clap_resource_files_response(&plugin_ref, files);
                    }
                    Action::ClipClapResourceFiles {
                        track_name,
                        clip_idx,
                        instance_id,
                        files,
                    } => {
                        let plugin_ref = crate::gui::PluginInstanceRef::Clip {
                            track_name: track_name.clone(),
                            clip_idx: *clip_idx,
                            instance_id: *instance_id,
                        };
                        self.handle_clap_resource_files_response(&plugin_ref, files);
                    }
                    Action::TrackClapStateDirty {
                        track_name,
                        instance_id,
                    } => {
                        tracing::info!(%track_name, instance_id, "DAW received CLAP state dirty");
                        self.session_ops.engine_dirty = true;
                        let mut tasks = vec![self.send(Action::TrackClapSnapshotState {
                            track_name: track_name.clone(),
                            instance_id: *instance_id,
                        })];
                        if let Some(task) = self.maybe_refresh_plugin_graph_for_track(track_name) {
                            tasks.push(task);
                        }
                        return Task::batch(tasks);
                    }
                    Action::ClipClapStateDirty {
                        track_name,
                        clip_idx,
                        instance_id,
                    } => {
                        tracing::info!(%track_name, instance_id, "DAW received CLAP state dirty");
                        self.session_ops.engine_dirty = true;
                        let mut tasks = vec![self.send(Action::ClipClapSnapshotState {
                            track_name: track_name.clone(),
                            clip_idx: *clip_idx,
                            instance_id: *instance_id,
                        })];
                        if let Some(task) = self.maybe_refresh_plugin_graph_for_track(track_name) {
                            tasks.push(task);
                        }
                        return Task::batch(tasks);
                    }
                    _ => {}
                }
                let handled_response_state = self.handle_response_engine_state_action(a);
                let handled_response_track = self.handle_response_track_action(a);
                let handled_response_timing = self.handle_response_timing_state_action(a);
                if handled_response_state {
                    match a {
                        Action::TrackConnectPluginAudio { track_name, .. }
                        | Action::TrackDisconnectPluginAudio { track_name, .. }
                        | Action::TrackConnectPluginMidi { track_name, .. }
                        | Action::TrackDisconnectPluginMidi { track_name, .. }
                        | Action::TrackConnectAudio { track_name, .. }
                        | Action::TrackDisconnectAudio { track_name, .. }
                        | Action::TrackConnectMidi { track_name, .. }
                        | Action::TrackDisconnectMidi { track_name, .. } => {
                            if !self.session_ops.session_restore_in_progress
                                && let Some(task) =
                                    self.maybe_refresh_plugin_graph_for_track(track_name)
                            {
                                return task;
                            }
                        }
                        _ => {}
                    }
                }
                if matches!(a, Action::EndSessionRestore) {
                    let sync_actions = self.cleanup_session_slot_references();
                    for action in sync_actions {
                        self.try_send_engine(EngineMessage::Request(action));
                    }
                    let open_track = {
                        let state = self.state.read().expect("state lock poisoned");
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
                            let mut state = self.state.write().expect("state lock poisoned");
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
                            let mut state = self.state.write().expect("state lock poisoned");
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
                            let mut state = self.state.write().expect("state lock poisoned");
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
                            let mut state = self.state.write().expect("state lock poisoned");
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
                            mixosc_addr,
                        } => {
                            let mut state = self.state.write().expect("state lock poisoned");
                            let mut track = Track::new(
                                name.clone(),
                                0.0,
                                *audio_ins,
                                *audio_outs,
                                *midi_ins,
                                *midi_outs,
                            );
                            track.is_folder = *folder;
                            track.mixosc_addr = mixosc_addr.clone();
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
                                self.pending.pending_track_freeze_restore.remove(name)
                                && let Some(track) =
                                    state.tracks.iter_mut().find(|t| &t.name == name)
                            {
                                track.frozen_audio_backup = audio_backup;
                                track.frozen_midi_backup = midi_backup;
                                track.frozen_render_clip = render_clip;
                            }
                            if let Some(mode) = self
                                .pending
                                .pending_track_midi_editor_view_mode
                                .remove(name)
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
                                let mut state = self.state.write().expect("state lock poisoned");
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
                                let mut state = self.state.write().expect("state lock poisoned");
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
                                let mut state = self.state.write().expect("state lock poisoned");
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
                            let mut state = self.state.write().expect("state lock poisoned");

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
                                state.session.slots.remove(name);
                                state.selected_slots.retain(|(n, _)| *n != *name);
                                state
                                    .session_midi_learn_slots
                                    .retain(|(n, _), _| *n != *name);
                                state
                                    .session_midi_learn_stop_track
                                    .retain(|n, _| *n != *name);
                                for track in &mut state.tracks {
                                    if track.parent_track.as_deref() == Some(name.as_str()) {
                                        track.parent_track = None;
                                    }
                                }
                            }
                            drop(state);
                            for (key, peaks) in undo_peaks {
                                self.pending.undo_peaks_cache.insert(key, peaks);
                            }
                            for (key, source_len) in undo_source_lengths {
                                self.pending
                                    .undo_source_lengths_cache
                                    .insert(key, source_len);
                            }
                        }
                        Action::ClipMove {
                            kind,
                            from,
                            to,
                            copy,
                        } => {
                            let mut state = self.state.write().expect("state lock poisoned");

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
                                        Self::apply_audio_cross_section_fades(
                                            &mut to_track.audio.clips,
                                            &mut clip_data,
                                        );
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
                            reversed,
                            gain_db,
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
                            pitch_correction_detector,
                            pitch_correction_mode,
                            plugin_graph_json,
                        } => {
                            if self.rec.recording_preview_start_sample.is_some() {
                                self.rec.stop_recording_preview();
                            }
                            let key =
                                Self::audio_clip_key(track_name, name, *start, *length, *offset);
                            let mut max_length_samples = offset.saturating_add(*length);
                            let mut source_length_samples = self
                                .pending
                                .pending_source_lengths
                                .remove(&key)
                                .or_else(|| self.pending.undo_source_lengths_cache.remove(&key))
                                .unwrap_or(0);
                            let mut wav_path_for_rebuild: Option<std::path::PathBuf> = None;
                            let mut peaks_path_for_load: Option<std::path::PathBuf> = None;
                            let precomputed_peaks = self
                                .pending
                                .pending_precomputed_peaks
                                .remove(&key)
                                .or_else(|| self.pending.undo_peaks_cache.remove(&key));
                            let loaded_bins = 0usize;
                            if *kind == Kind::Audio {
                                peaks_path_for_load = peaks_file.as_ref().and_then(|rel| {
                                    self.session_dir
                                        .as_ref()
                                        .map(|session_root| session_root.join(rel))
                                        .filter(|path| path.exists() && path.is_file())
                                });
                                if peaks_path_for_load.is_none() {
                                    peaks_path_for_load =
                                        self.pending.pending_peak_file_loads.remove(&key);
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
                            let mut state = self.state.write().expect("state lock poisoned");
                            state.unused_audio_clips.retain(|clip| clip.id != *clip_id);
                            state.unused_midi_clips.retain(|clip| clip.id != *clip_id);
                            if let Some(track) =
                                state.tracks.iter_mut().find(|t| &t.name == track_name)
                            {
                                match kind {
                                    Kind::Audio => {
                                        let mut clip = crate::state::AudioClip {
                                            id: clip_id.clone(),
                                            name: name.clone(),
                                            start: *start,
                                            length: *length,
                                            offset: *offset,
                                            input_channel: *input_channel,
                                            muted: *muted,
                                            reversed: *reversed,
                                            gain_db: *gain_db,
                                            edit_actions: Vec::new(),
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
                                            pitch_correction_detector: *pitch_correction_detector,
                                            pitch_correction_mode: *pitch_correction_mode,
                                            take_lane_override: None,
                                            take_lane_pinned: false,
                                            take_lane_locked: false,
                                            plugin_graph_json: plugin_graph_json.clone(),
                                            grouped_clips: vec![],
                                        };
                                        Self::apply_audio_cross_section_fades(
                                            &mut track.audio.clips,
                                            &mut clip,
                                        );
                                        track.audio.clips.push(clip);
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
                                            reversed: *reversed,
                                            max_length_samples,
                                            take_lane_override: None,
                                            take_lane_pinned: false,
                                            take_lane_locked: false,
                                            grouped_clips: vec![],
                                        });
                                    }
                                }
                            }
                            let session_record_target =
                                self.drag.session_slot_record_target.clone();
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
                                self.try_send_engine(EngineMessage::Request(
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
                                    && let Some(task) = self.pending.schedule_audio_peak_file_load(
                                        track_name, name, *start, *length, *offset, peaks_path,
                                    )
                                {
                                    self.update_children(&message);
                                    return task;
                                }
                                if let Some(wav_path) = wav_path_for_rebuild
                                    && let Some(task) = self.pending.schedule_audio_peak_rebuild(
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
                            let mut state = self.state.write().expect("state lock poisoned");
                            if let Some(clip) = audio_clip {
                                state
                                    .unused_audio_clips
                                    .retain(|unused| unused.id != clip.id);
                            }
                            if let Some(clip) = midi_clip {
                                state
                                    .unused_midi_clips
                                    .retain(|unused| unused.id != clip.id);
                            }
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
                                                .pending
                                                .pending_source_lengths
                                                .remove(&key)
                                                .or_else(|| {
                                                    self.pending
                                                        .undo_source_lengths_cache
                                                        .remove(&key)
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
                                            let mut clip = Self::audio_clip_from_data(
                                                clip,
                                                max_length_samples,
                                                source_length_samples,
                                            );
                                            Self::apply_audio_cross_section_fades(
                                                &mut track.audio.clips,
                                                &mut clip,
                                            );
                                            track.audio.clips.push(clip);
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
                            let mut state = self.state.write().expect("state lock poisoned");
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
                        Action::SetClipReversed {
                            track_name,
                            clip_index,
                            kind,
                            reversed,
                        } => {
                            let mut state = self.state.write().expect("state lock poisoned");
                            if let Some(track) =
                                state.tracks.iter_mut().find(|t| &t.name == track_name)
                            {
                                match kind {
                                    Kind::Audio => {
                                        if let Some(clip) = track.audio.clips.get_mut(*clip_index) {
                                            clip.reversed = *reversed;
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
                                            clip.reversed = *reversed;
                                        }
                                        refresh_midi_clip_previews = true;
                                    }
                                }
                            }
                        }
                        Action::SetClipFade {
                            track_name,
                            clip_index,
                            kind,
                            fade_enabled,
                            fade_in_samples,
                            fade_out_samples,
                        } => {
                            let mut state = self.state.write().expect("state lock poisoned");
                            if let Some(track) =
                                state.tracks.iter_mut().find(|t| &t.name == track_name)
                                && let Kind::Audio = kind
                                && let Some(clip) = track.audio.clips.get_mut(*clip_index)
                            {
                                clip.fade_enabled = *fade_enabled;
                                clip.fade_in_samples = *fade_in_samples;
                                clip.fade_out_samples = *fade_out_samples;
                            }
                        }
                        Action::SetClipSourceName {
                            track_name,
                            clip_index,
                            kind,
                            name,
                        } => {
                            let mut state = self.state.write().expect("state lock poisoned");
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
                        Action::SetClipIdentity {
                            track_name,
                            clip_index,
                            kind,
                            new_id,
                            new_name,
                        } => {
                            let mut state = self.state.write().expect("state lock poisoned");
                            if let Some(track) =
                                state.tracks.iter_mut().find(|t| &t.name == track_name)
                            {
                                match kind {
                                    Kind::Audio => {
                                        if let Some(clip) = track.audio.clips.get_mut(*clip_index) {
                                            clip.id = new_id.clone();
                                            clip.name = new_name.clone();
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
                                            clip.id = new_id.clone();
                                            clip.name = new_name.clone();
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
                            let mut state = self.state.write().expect("state lock poisoned");
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
                                    self.pending.pending_precomputed_peaks.remove(&peak_key)
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
                            let mut state = self.state.write().expect("state lock poisoned");
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
                            let mut state = self.state.write().expect("state lock poisoned");
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
                            let mut state = self.state.write().expect("state lock poisoned");
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
                                    self.try_send_engine(EngineMessage::Request(action));
                                }
                            }
                            for (key, peaks) in undo_peaks {
                                self.pending.undo_peaks_cache.insert(key, peaks);
                            }
                            for (key, source_len) in undo_source_lengths {
                                self.pending
                                    .undo_source_lengths_cache
                                    .insert(key, source_len);
                            }
                        }
                        Action::MoveClipToUnused {
                            track_name,
                            kind,
                            clip_indices,
                        } => {
                            let mut undo_peaks = Vec::new();
                            let mut undo_source_lengths = Vec::new();
                            let mut moved_audio = Vec::new();
                            let mut moved_midi = Vec::new();
                            let mut state = self.state.write().expect("state lock poisoned");
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
                                                moved_audio.push(clip.clone());
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
                                                moved_midi.push(clip.clone());
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
                            state.unused_audio_clips.extend(moved_audio);
                            state.unused_midi_clips.extend(moved_midi);
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
                            for (key, peaks) in undo_peaks {
                                self.pending.undo_peaks_cache.insert(key, peaks);
                            }
                            for (key, source_len) in undo_source_lengths {
                                self.pending
                                    .undo_source_lengths_cache
                                    .insert(key, source_len);
                            }
                        }
                        Action::SetUnusedClips { audio, midi } => {
                            let mut state = self.state.write().expect("state lock poisoned");
                            state.unused_audio_clips = audio
                                .iter()
                                .map(|clip| {
                                    Self::audio_clip_from_data(
                                        clip,
                                        clip.offset.saturating_add(clip.length).max(1),
                                        0,
                                    )
                                })
                                .collect();
                            state.unused_midi_clips = midi
                                .iter()
                                .map(|clip| {
                                    Self::midi_clip_from_data(
                                        clip,
                                        clip.offset.saturating_add(clip.length).max(1),
                                    )
                                })
                                .collect();
                        }
                        Action::DeleteUnusedClips { clip_ids } => {
                            let mut state = self.state.write().expect("state lock poisoned");
                            state
                                .unused_audio_clips
                                .retain(|clip| !clip_ids.contains(&clip.id));
                            state
                                .unused_midi_clips
                                .retain(|clip| !clip_ids.contains(&clip.id));
                        }
                        Action::ModifyMidiNotes {
                            track_name,
                            note_indices,
                            new_notes,
                            ..
                        } => {
                            let mut state = self.state.write().expect("state lock poisoned");
                            let mut updated_open_piano = false;
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
                                updated_open_piano = true;
                            }
                            if updated_open_piano {
                                Self::sort_open_piano_notes_like_engine(&mut state);
                            }
                        }
                        Action::ModifyMidiControllers {
                            track_name,
                            controller_indices,
                            new_controllers,
                            ..
                        } => {
                            let mut state = self.state.write().expect("state lock poisoned");
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
                            let mut state = self.state.write().expect("state lock poisoned");
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
                            let mut state = self.state.write().expect("state lock poisoned");
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
                            let mut state = self.state.write().expect("state lock poisoned");
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
                            let mut state = self.state.write().expect("state lock poisoned");
                            let mut updated_open_piano = false;
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
                                updated_open_piano = true;
                            }
                            if updated_open_piano {
                                Self::sort_open_piano_notes_like_engine(&mut state);
                            }
                        }
                        Action::InsertMidiNotes {
                            track_name, notes, ..
                        } => {
                            let mut state = self.state.write().expect("state lock poisoned");
                            let mut updated_open_piano = false;
                            if let Some(piano) = state.piano.as_mut()
                                && piano.track_idx == *track_name
                            {
                                let mut sorted_indices: Vec<usize> = (0..notes.len()).collect();
                                sorted_indices.sort_unstable_by_key(|&i| notes[i].0);
                                for i in sorted_indices {
                                    let (idx, note) = &notes[i];
                                    let insert_at = (*idx).min(piano.notes.len());
                                    piano.notes.insert(insert_at, engine_note_to_widgets(note));
                                }
                                state.piano_selected_notes.clear();
                                updated_open_piano = true;
                            }
                            if updated_open_piano {
                                Self::sort_open_piano_notes_like_engine(&mut state);
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
                                let mut state = self.state.write().expect("state lock poisoned");
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
                            self.state.write().expect("state lock poisoned").message = format!(
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
                                let mut state = self.state.write().expect("state lock poisoned");
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
                            self.state.write().expect("state lock poisoned").message = format!(
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
                            let mut state = self.state.write().expect("state lock poisoned");
                            state
                                .clap_states_by_track
                                .entry(track_name.clone())
                                .or_default()
                                .insert(plugin_id.clone(), clap_state.clone());
                            #[cfg(unix)]
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
                                let mut state = self.state.write().expect("state lock poisoned");
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
                            if self.pending.pending_save_path.is_some() {
                                self.pending.pending_save_clap_clips.remove(&(
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
                                .pending
                                .pending_add_clap_automation_instances
                                .remove(&(track_name.clone(), *instance_id));
                            {
                                let mut state = self.state.write().expect("state lock poisoned");
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
                                            default_value: p.default_value,
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
                                self.try_send_engine(EngineMessage::Request(action));
                            }
                        }
                        Action::ClipClapParameters {
                            track_name,
                            clip_idx,
                            instance_id,
                            parameters,
                        } => {
                            let mut state = self.state.write().expect("state lock poisoned");
                            let cached = state
                                .plugin_parameters_by_clip
                                .entry((track_name.clone(), *clip_idx))
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
                                        default_value: p.default_value,
                                    })
                                    .collect(),
                            );
                        }
                        Action::TrackVst3Parameters {
                            track_name,
                            instance_id,
                            parameters,
                        } => {
                            let pending = self
                                .pending
                                .pending_add_vst3_automation_instances
                                .remove(&(track_name.clone(), *instance_id));
                            {
                                let mut state = self.state.write().expect("state lock poisoned");
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
                                            default_value: p.default_value,
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
                                self.try_send_engine(EngineMessage::Request(action));
                            }
                        }
                        Action::ClipVst3Parameters {
                            track_name,
                            clip_idx,
                            instance_id,
                            parameters,
                        } => {
                            let mut state = self.state.write().expect("state lock poisoned");
                            let cached = state
                                .plugin_parameters_by_clip
                                .entry((track_name.clone(), *clip_idx))
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
                                        default_value: p.default_value,
                                    })
                                    .collect(),
                            );
                        }
                        #[cfg(unix)]
                        Action::ClipLv2PluginControls {
                            track_name,
                            clip_idx,
                            instance_id,
                            controls,
                            instance_access_handle: _,
                        } => {
                            let mut state = self.state.write().expect("state lock poisoned");
                            let cached = state
                                .plugin_parameters_by_clip
                                .entry((track_name.clone(), *clip_idx))
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
                                        default_value: p.value as f64,
                                    })
                                    .collect(),
                            );
                        }
                        #[cfg(unix)]
                        Action::TrackLv2PluginControls {
                            track_name,
                            instance_id,
                            controls,
                            instance_access_handle: _,
                        } => {
                            let pending = self
                                .pending
                                .pending_add_lv2_automation_instances
                                .remove(&(track_name.clone(), *instance_id));
                            {
                                let mut state = self.state.write().expect("state lock poisoned");
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
                                            default_value: p.value as f64,
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
                                self.try_send_engine(EngineMessage::Request(action));
                            }
                        }
                        Action::TrackSnapshotAllClapStates { track_name: _ } => {}
                        Action::TrackSnapshotAllClapStatesDone { track_name }
                            if self.pending.pending_save_path.is_some() =>
                        {
                            self.pending.pending_save_clap_tracks.remove(track_name);
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
                        #[cfg(unix)]
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
                            let mut gui_state = self.state.write().expect("state lock poisoned");
                            gui_state
                                .vst3_states_by_track
                                .entry(track_name.clone())
                                .or_default()
                                .insert(*instance_id, state.clone());
                        }
                        Action::ClipVst3StateSnapshot {
                            track_name,
                            clip_idx,
                            instance_id,
                            state,
                        } => {
                            let state_json =
                                serde_json::to_value(state).unwrap_or(serde_json::Value::Null);
                            let mut gui_state = self.state.write().expect("state lock poisoned");
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
                        #[cfg(unix)]
                        Action::TrackLv2Midnam {
                            track_name,
                            note_names,
                        } => {
                            let mut state = self.state.write().expect("state lock poisoned");
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
                            let mut state = self.state.write().expect("state lock poisoned");
                            if let Some(piano) = &mut state.piano
                                && piano.track_idx == *track_name
                            {
                                for (note, name) in note_names.iter() {
                                    piano.midnam_note_names.insert(*note, name.clone());
                                }
                            }
                            self.workspace.set_midi_edit_midnam_note_names(note_names);
                        }
                        #[cfg(unix)]
                        Action::ClipLv2StateSnapshot {
                            track_name,
                            clip_idx,
                            instance_id,
                            state: lv2_state,
                        } => {
                            let mut state = self.state.write().expect("state lock poisoned");
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
                                restore_in_progress = self.session_ops.session_restore_in_progress,
                                "received TrackPluginGraph"
                            );
                            let state = self.state.write().expect("state lock poisoned");
                            let keep_restore_cache = self.session_ops.session_restore_in_progress
                                && plugins.is_empty()
                                && state
                                    .plugin_graphs_by_track
                                    .get(track_name)
                                    .is_some_and(|(cached_plugins, _)| !cached_plugins.is_empty());
                            if keep_restore_cache {
                                return Task::none();
                            }
                            let mut plugins = plugins.clone();
                            drop(state);

                            let pending_queries =
                                self.queue_pending_graph_automation_queries(track_name, &plugins);

                            let mut state = self.state.write().expect("state lock poisoned");
                            Self::preserve_plugin_graph_states_from_cache(
                                track_name,
                                &state,
                                &mut plugins,
                            );
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
                            state
                                .plugin_graphs_by_track
                                .insert(track_name.clone(), (plugins, connections.clone()));
                            drop(state);

                            if !pending_queries.is_empty() {
                                return Task::batch(pending_queries);
                            }

                            if self.pending.pending_save_path.is_some() {
                                self.pending.pending_save_tracks.remove(track_name);
                                if let Some(task) = self.complete_pending_save(track_name) {
                                    return task;
                                }
                            }
                        }
                        Action::RenameTrack { old_name, new_name } => {
                            let mut state = self.state.write().expect("state lock poisoned");

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

                            #[cfg(unix)]
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
                            Self::rename_track_map_entry(
                                &mut state.session.slots,
                                old_name,
                                new_name,
                            );
                            Self::rename_track_map_entry(
                                &mut state.session_midi_learn_stop_track,
                                old_name,
                                new_name,
                            );
                            state.selected_slots = state
                                .selected_slots
                                .drain()
                                .map(|(n, s)| {
                                    if n == *old_name {
                                        (new_name.clone(), s)
                                    } else {
                                        (n, s)
                                    }
                                })
                                .collect();
                            state.session_midi_learn_slots = state
                                .session_midi_learn_slots
                                .drain()
                                .map(|((n, s), b)| {
                                    if n == *old_name {
                                        ((new_name.clone(), s), b)
                                    } else {
                                        ((n, s), b)
                                    }
                                })
                                .collect();
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
                            self.plugin_params.clap_param_values.insert(key, *value);
                            self.plugin_params.generic_plugin_param_values.insert(
                                (track_name.clone(), None, *instance_id, *param_id),
                                *value,
                            );
                        }
                        Action::TrackSetVst3Parameter {
                            track_name,
                            instance_id,
                            param_id,
                            value,
                        } => {
                            self.plugin_params.generic_plugin_param_values.insert(
                                (track_name.clone(), None, *instance_id, *param_id),
                                f64::from(*value),
                            );
                        }
                        Action::ClipSetClapParameter {
                            track_name,
                            clip_idx,
                            instance_id,
                            param_id,
                            value,
                        } => {
                            let key =
                                (track_name.clone(), Some(*clip_idx), *instance_id, *param_id);
                            self.plugin_params.clap_param_values.insert(key, *value);
                            self.plugin_params.generic_plugin_param_values.insert(
                                (track_name.clone(), Some(*clip_idx), *instance_id, *param_id),
                                *value,
                            );
                        }
                        Action::ClipSetVst3Parameter {
                            track_name,
                            clip_idx,
                            instance_id,
                            param_id,
                            value,
                        } => {
                            self.plugin_params.generic_plugin_param_values.insert(
                                (track_name.clone(), Some(*clip_idx), *instance_id, *param_id),
                                f64::from(*value),
                            );
                        }
                        #[cfg(unix)]
                        Action::ClipSetLv2ControlValue {
                            track_name,
                            clip_idx,
                            instance_id,
                            index,
                            value,
                        } => {
                            self.plugin_params.generic_plugin_param_values.insert(
                                (track_name.clone(), Some(*clip_idx), *instance_id, *index),
                                f64::from(*value),
                            );
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
                if let Some(fallback) = self.pending.pending_native_ui_fallback.take()
                    && native_ui_error_matches(&fallback.format, e)
                {
                    self.info(format!(
                        "{} native UI unavailable for instance {}; opening generic editor",
                        fallback.format, fallback.instance_id
                    ));
                    return self.open_generic_plugin_ui(
                        fallback.track_name,
                        fallback.clip_idx,
                        fallback.instance_id,
                        fallback.format,
                        fallback.plugin_id,
                    );
                }
                if !self.pending.pending_track_freeze_bounce.is_empty() {
                    self.pending.pending_track_freeze_bounce.clear();
                }
                self.pending.freeze_in_progress = false;
                self.pending.freeze_track_name = None;
                self.pending.freeze_cancel_requested = false;
                self.pending.pending_save_path = None;
                self.pending.pending_save_tracks.clear();
                self.pending.pending_save_clap_tracks.clear();
                self.pending.pending_save_clap_clips.clear();
                self.pending.pending_save_is_template = false;
                self.error(e.clone());
            }
            _ => {}
        }
        self.update_children(&message);
        Task::none()
    }
}
