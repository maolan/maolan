use super::*;
use crate::state::SlotPlayState;
use iced::widget::Id;
use iced_drop::zones_on_point;

impl Maolan {
    pub(super) fn handle_live_session_message(
        &mut self,
        message: Message,
    ) -> Option<Task<Message>> {
        match message {
            Message::SessionSlotSelect {
                track_name,
                scene_index,
                additive,
            } => {
                let mut state = self.state.blocking_write();
                if additive {
                    state.selected_slots.insert((track_name, scene_index));
                } else {
                    state.selected_slots.clear();
                    state.selected_slots.insert((track_name, scene_index));
                }
                None
            }
            Message::SessionSceneSelect(scene_index) => {
                let mut state = self.state.blocking_write();
                state.selected_scene = Some(scene_index);
                None
            }
            Message::SessionSlotPressed {
                track_name,
                scene_index,
            } => {
                {
                    let mut state = self.state.blocking_write();
                    state.selected_slots.clear();
                    state
                        .selected_slots
                        .insert((track_name.clone(), scene_index));
                }
                self.toggle_session_slot(&track_name, scene_index);
                None
            }
            Message::SessionSlotReleased { .. } => None,
            Message::SessionSlotSetPlayStopIcon {
                track_name,
                scene_index,
                icon,
            } => {
                {
                    let mut state = self.state.blocking_write();
                    state.session.ensure_track_slots(&track_name);
                    if let Some(slots) = state.session.slots.get_mut(&track_name)
                        && let Some(slot) = slots.get_mut(scene_index)
                    {
                        slot.play_stop_icon = icon;
                    }
                }
                let _ = CLIENT.sender.try_send(EngineMessage::Request(
                    Action::TrackSetSessionSlotPlayEnabled {
                        track_name: track_name.clone(),
                        scene_index,
                        enabled: icon == Some(true),
                    },
                ));
                let _ = CLIENT.sender.try_send(EngineMessage::Request(
                    Action::TrackSetSessionSlotStopEnabled {
                        track_name: track_name.clone(),
                        scene_index,
                        enabled: icon == Some(false),
                    },
                ));
                if self.live_session_playing {
                    if icon == Some(true) {
                        self.launch_session_slot(&track_name, scene_index);
                    } else {
                        self.stop_session_slot(&track_name, scene_index);
                    }
                }
                None
            }
            Message::SessionSlotRightClick {
                track_name,
                scene_index,
                ..
            } => {
                let anchor = {
                    let state = self.state.blocking_read();
                    state
                        .session_slot_context_hover
                        .as_ref()
                        .filter(|((hover_track, hover_scene), _)| {
                            hover_track == &track_name && *hover_scene == scene_index
                        })
                        .map(|(_, point)| *point)
                        .unwrap_or(Point::new(8.0, 24.0))
                };
                let mut state = self.state.blocking_write();
                state.selected_slots.clear();
                state
                    .selected_slots
                    .insert((track_name.clone(), scene_index));
                state.session_slot_context_menu = Some(crate::state::SessionSlotContextMenuState {
                    track_name,
                    scene_index,
                    anchor,
                });
                None
            }
            Message::SessionSlotContextMenuHover {
                track_name,
                scene_index,
                position,
            } => {
                self.state.blocking_write().session_slot_context_hover =
                    Some(((track_name, scene_index), position));
                None
            }
            Message::SessionScenePressed(scene_index) => {
                // Clicking a Master slot selects the scene. While the live
                // session is playing it also becomes the next scene to
                // launch; while stopped it is the scene Play will start.
                {
                    let mut state = self.state.blocking_write();
                    if scene_index < state.session.scenes.len() {
                        state.selected_scene = Some(scene_index);
                    } else {
                        return None;
                    }
                }
                if self.live_session_playing {
                    return Some(
                        self.send(Action::Session(SessionAction::QueueScene { scene_index })),
                    );
                }
                None
            }
            Message::SessionSceneReleased(_) => None,
            Message::SessionSceneRightClick { scene_index, .. } => {
                let anchor = {
                    let state = self.state.blocking_read();
                    state
                        .session_scene_context_hover
                        .as_ref()
                        .filter(|(hover_scene, _)| *hover_scene == scene_index)
                        .map(|(_, point)| *point)
                        .unwrap_or(Point::new(8.0, 24.0))
                };
                let mut state = self.state.blocking_write();
                state.selected_scene = Some(scene_index);
                state.session_scene_context_menu =
                    Some(crate::state::SessionSceneContextMenuState {
                        scene_index,
                        anchor,
                    });
                None
            }
            Message::SessionSceneContextMenuHover {
                scene_index,
                position,
            } => {
                self.state.blocking_write().session_scene_context_hover =
                    Some((scene_index, position));
                None
            }
            Message::SessionSceneContextMenuHide => {
                self.state.blocking_write().session_scene_context_menu = None;
                None
            }
            Message::SessionSceneRenameShow(scene_index) => {
                let name = {
                    let state = self.state.blocking_read();
                    state
                        .session
                        .scenes
                        .get(scene_index)
                        .map(|scene| scene.name.clone())
                        .unwrap_or_default()
                };
                self.state.blocking_write().scene_rename_dialog =
                    Some(crate::state::SceneRenameDialog { scene_index, name });
                None
            }
            Message::SessionSceneRenameInput(value) => {
                if let Some(dialog) = self.state.blocking_write().scene_rename_dialog.as_mut() {
                    dialog.name = value;
                }
                None
            }
            Message::SessionSceneRenameConfirm => {
                let mut state = self.state.blocking_write();
                if let Some(dialog) = state.scene_rename_dialog.take()
                    && let Some(scene) = state.session.scenes.get_mut(dialog.scene_index)
                {
                    scene.name = dialog.name;
                }
                None
            }
            Message::SessionSceneRenameCancel => {
                self.state.blocking_write().scene_rename_dialog = None;
                None
            }
            Message::SessionSceneRemove(scene_index) => {
                let mut state = self.state.blocking_write();
                if scene_index < state.session.scenes.len() {
                    state.session.scenes.remove(scene_index);
                    for slots in state.session.slots.values_mut() {
                        if scene_index < slots.len() {
                            slots.remove(scene_index);
                        }
                    }
                    if state.selected_scene == Some(scene_index) {
                        state.selected_scene = None;
                    }
                }
                state.session_scene_context_menu = None;
                None
            }
            Message::SessionSceneSetColor { scene_index, color } => {
                if let Some(scene) = self
                    .state
                    .blocking_write()
                    .session
                    .scenes
                    .get_mut(scene_index)
                {
                    scene.color = Some([color.r, color.g, color.b, color.a]);
                }
                None
            }
            Message::SessionSceneClearColor(scene_index) => {
                if let Some(scene) = self
                    .state
                    .blocking_write()
                    .session
                    .scenes
                    .get_mut(scene_index)
                {
                    scene.color = None;
                }
                None
            }
            Message::SessionSceneSetTempo { scene_index, bpm } => {
                if let Some(scene) = self
                    .state
                    .blocking_write()
                    .session
                    .scenes
                    .get_mut(scene_index)
                {
                    scene.tempo = bpm;
                }
                None
            }
            Message::SessionSceneSetLaunchQuantization {
                scene_index,
                quantization,
            } => {
                if let Some(scene) = self
                    .state
                    .blocking_write()
                    .session
                    .scenes
                    .get_mut(scene_index)
                {
                    scene.launch_quantization = quantization;
                }
                None
            }
            Message::SessionSceneAdd => {
                let mut state = self.state.blocking_write();
                state.session.add_scene();
                let track_names: Vec<String> = state
                    .tracks
                    .iter()
                    .map(|track| track.name.clone())
                    .collect();
                for track_name in track_names {
                    state.session.ensure_track_slots(&track_name);
                }
                None
            }
            Message::SessionStopTrackPressed(track_name) => {
                self.stop_session_track(&track_name);
                None
            }
            Message::SessionStopAllPressed => {
                self.stop_all_session_clips();
                None
            }
            Message::SessionSlotDrag { from, to } => {
                self.move_session_slot(from, to);
                None
            }
            Message::SessionSlotDragStart {
                track_name,
                scene_index,
                ..
            } => {
                self.dragging_session_slot = Some((track_name, scene_index));
                None
            }
            Message::SessionClipDragStart {
                source_track_name,
                clip_id,
                kind,
            } => {
                self.dragging_session_clip = Some(crate::state::DraggedSessionClip {
                    source_track_name,
                    clip_id,
                    kind,
                });
                None
            }
            Message::SessionClipDropped { point } => {
                if self.dragging_session_clip.is_some() {
                    return Some(zones_on_point(
                        Message::SessionClipHandleZones,
                        point,
                        None,
                        None,
                    ));
                }
                None
            }
            Message::SessionClipHandleZones(ref zones) => {
                if let Some(dragged) = self.dragging_session_clip.clone() {
                    let slot_map = {
                        let state = self.state.blocking_read();
                        build_slot_zone_map(&state.tracks, &state.session)
                    };
                    let target = zones
                        .iter()
                        .find_map(|(zone_id, _rect)| slot_map.get(zone_id).cloned());
                    if let Some((target_track_name, scene_index)) = target {
                        let (valid, same_track) = {
                            let state = self.state.blocking_read();
                            let source_track = dragged
                                .source_track_name
                                .as_ref()
                                .and_then(|name| state.tracks.iter().find(|t| &t.name == name));
                            let target_track =
                                state.tracks.iter().find(|t| t.name == target_track_name);
                            let valid = if let Some(target) = target_track {
                                !target.is_master
                                    && !target.is_folder
                                    && match dragged.kind {
                                        maolan_engine::kind::Kind::Audio => match source_track {
                                            Some(source) => {
                                                source.audio.outs > 0
                                                    && target.audio.outs == source.audio.outs
                                            }
                                            None => target.audio.ins > 0,
                                        },
                                        maolan_engine::kind::Kind::MIDI => target.midi.ins > 0,
                                    }
                            } else {
                                false
                            };
                            let same_track =
                                dragged.source_track_name.as_ref() == Some(&target_track_name);
                            (valid, same_track)
                        };
                        if valid {
                            let source_clip_name = {
                                let state = self.state.blocking_read();
                                match &dragged.source_track_name {
                                    Some(source_track_name) => state
                                        .tracks
                                        .iter()
                                        .find(|t| &t.name == source_track_name)
                                        .and_then(|source| match dragged.kind {
                                            maolan_engine::kind::Kind::Audio => source
                                                .audio
                                                .clips
                                                .iter()
                                                .find(|c| c.id == dragged.clip_id)
                                                .map(|clip| clip.name.clone()),
                                            maolan_engine::kind::Kind::MIDI => source
                                                .midi
                                                .clips
                                                .iter()
                                                .find(|c| c.id == dragged.clip_id)
                                                .map(|clip| clip.name.clone()),
                                        }),
                                    None => match dragged.kind {
                                        maolan_engine::kind::Kind::Audio => state
                                            .unused_audio_clips
                                            .iter()
                                            .find(|c| c.id == dragged.clip_id)
                                            .map(|clip| clip.name.clone()),
                                        maolan_engine::kind::Kind::MIDI => state
                                            .unused_midi_clips
                                            .iter()
                                            .find(|c| c.id == dragged.clip_id)
                                            .map(|clip| clip.name.clone()),
                                    },
                                }
                            };
                            let clip_id = if same_track {
                                dragged.clip_id.clone()
                            } else if dragged.source_track_name.is_some() {
                                let clip_id = dragged.clip_id.clone();
                                let state = self.state.blocking_read();
                                let source_track = dragged
                                    .source_track_name
                                    .as_ref()
                                    .and_then(|name| state.tracks.iter().find(|t| &t.name == name));
                                if let Some(source) = source_track {
                                    let add_clip = match dragged.kind {
                                        maolan_engine::kind::Kind::Audio => source
                                            .audio
                                            .clips
                                            .iter()
                                            .find(|c| c.id == dragged.clip_id)
                                            .map(|clip| {
                                                Self::audio_clip_add_action(
                                                    &clip_id,
                                                    &target_track_name,
                                                    clip,
                                                    clip.start,
                                                )
                                            }),
                                        maolan_engine::kind::Kind::MIDI => source
                                            .midi
                                            .clips
                                            .iter()
                                            .find(|c| c.id == dragged.clip_id)
                                            .map(|clip| {
                                                Self::midi_clip_add_action(
                                                    &clip_id,
                                                    &target_track_name,
                                                    clip,
                                                    clip.start,
                                                )
                                            }),
                                    };
                                    drop(state);
                                    if let Some(action) = add_clip {
                                        let _ =
                                            CLIENT.sender.try_send(EngineMessage::Request(action));
                                    }
                                }
                                clip_id
                            } else {
                                // Dragged from the unused pool: move the clip onto the
                                // target track keeping its id, which also removes it
                                // from the pool.
                                let new_id = dragged.clip_id.clone();
                                let state = self.state.blocking_read();
                                let add_clip = match dragged.kind {
                                    maolan_engine::kind::Kind::Audio => state
                                        .unused_audio_clips
                                        .iter()
                                        .find(|c| c.id == dragged.clip_id)
                                        .map(|clip| {
                                            if clip.is_group() {
                                                Action::AddGroupedClip {
                                                    track_name: target_track_name.clone(),
                                                    kind: maolan_engine::kind::Kind::Audio,
                                                    audio_clip: Some(Self::audio_clip_to_data(
                                                        clip,
                                                    )),
                                                    midi_clip: None,
                                                }
                                            } else {
                                                Self::audio_clip_add_action(
                                                    &new_id,
                                                    &target_track_name,
                                                    clip,
                                                    clip.start,
                                                )
                                            }
                                        }),
                                    maolan_engine::kind::Kind::MIDI => state
                                        .unused_midi_clips
                                        .iter()
                                        .find(|c| c.id == dragged.clip_id)
                                        .map(|clip| {
                                            if clip.is_group() {
                                                Action::AddGroupedClip {
                                                    track_name: target_track_name.clone(),
                                                    kind: maolan_engine::kind::Kind::MIDI,
                                                    audio_clip: None,
                                                    midi_clip: Some(Self::midi_clip_to_data(clip)),
                                                }
                                            } else {
                                                Self::midi_clip_add_action(
                                                    &new_id,
                                                    &target_track_name,
                                                    clip,
                                                    clip.start,
                                                )
                                            }
                                        }),
                                };
                                drop(state);
                                if let Some(action) = add_clip {
                                    let _ = CLIENT.sender.try_send(EngineMessage::Request(action));
                                }
                                new_id
                            };
                            {
                                let mut state = self.state.blocking_write();
                                state.session.ensure_track_slots(&target_track_name);
                                if let Some(slot) =
                                    state.session.slot_mut(&target_track_name, scene_index)
                                {
                                    slot.clip = Some(crate::state::SlotClipRef {
                                        clip_id: clip_id.clone(),
                                        launch_mode: crate::state::LaunchMode::Toggle,
                                        launch_quantization: crate::state::LaunchQuantization::Bar,
                                        loop_enabled: true,
                                        loop_start_samples: 0,
                                        loop_end_samples: 0,
                                    });
                                    slot.clip_name = source_clip_name;
                                }
                            }
                            return Some(self.send(
                                maolan_engine::message::Action::TrackSetSessionSlot {
                                    track_name: target_track_name,
                                    scene_index,
                                    clip_id: Some(clip_id),
                                },
                            ));
                        }
                    }
                }
                self.dragging_session_clip = None;
                None
            }
            Message::SessionSlotDropped { point, .. } => {
                if self.dragging_session_slot.is_some() {
                    return Some(zones_on_point(
                        Message::SessionSlotHandleZones,
                        point,
                        None,
                        None,
                    ));
                }
                None
            }
            Message::SessionSlotHandleZones(ref zones) => {
                if let Some((from_track, from_scene)) = self.dragging_session_slot.clone() {
                    let from_id = Id::from(slot_zone_id(&from_track, from_scene));
                    let slot_map = {
                        let state = self.state.blocking_read();
                        build_slot_zone_map(&state.tracks, &state.session)
                    };
                    let workspace_zone = Id::from("workspace-drop-zone");
                    let target_zone = zones
                        .iter()
                        .find(|(zone_id, _)| *zone_id != from_id)
                        .or_else(|| zones.first());
                    if let Some((zone_id, _)) = target_zone {
                        if *zone_id == workspace_zone {
                            let _ = self.update(Message::SessionSlotCopyToArrangement {
                                track_name: from_track,
                                scene_index: from_scene,
                            });
                        } else if let Some((to_track, to_scene)) = slot_map.get(zone_id).cloned() {
                            let _ = self.update(Message::SessionSlotDrag {
                                from: (from_track, from_scene),
                                to: (to_track, to_scene),
                            });
                        }
                    }
                }
                self.dragging_session_slot = None;
                None
            }
            Message::WorkspaceSessionSlotDropped => {
                self.dragging_session_slot = None;
                None
            }
            Message::SessionSlotClearRef {
                track_name,
                scene_index,
            } => {
                {
                    let mut state = self.state.blocking_write();
                    if let Some(slot) = state.session.slot_mut(&track_name, scene_index) {
                        slot.clip = None;
                        slot.clip_name = None;
                    }
                }
                let _ =
                    CLIENT
                        .sender
                        .try_send(EngineMessage::Request(Action::TrackSetSessionSlot {
                            track_name: track_name.clone(),
                            scene_index,
                            clip_id: None,
                        }));
                None
            }
            Message::SessionSlotDuplicate {
                track_name,
                scene_index,
            } => {
                self.duplicate_session_slot(&track_name, scene_index);
                None
            }
            Message::SessionSlotCopyToArrangement {
                track_name,
                scene_index,
            } => {
                let state = self.state.blocking_read();
                let clip_ref = state
                    .session
                    .slot(&track_name, scene_index)
                    .and_then(|slot| slot.clip.as_ref())?;
                let track = state.tracks.iter().find(|track| track.name == track_name)?;
                let start = self.transport_samples.max(0.0) as usize;
                if let Some(clip) = track
                    .audio
                    .clips
                    .iter()
                    .find(|clip| clip.id == clip_ref.clip_id)
                {
                    let _ = CLIENT
                        .sender
                        .try_send(EngineMessage::Request(Action::AddClip {
                            clip_id: crate::state::generate_clip_id(),
                            name: clip.name.clone(),
                            track_name: track_name.clone(),
                            start,
                            length: clip.length,
                            offset: clip.offset,
                            input_channel: clip.input_channel,
                            muted: clip.muted,
                            peaks_file: clip.peaks_file.clone(),
                            kind: Kind::Audio,
                            fade_enabled: clip.fade_enabled,
                            fade_in_samples: clip.fade_in_samples,
                            fade_out_samples: clip.fade_out_samples,
                            source_name: clip.pitch_correction_source_name.clone(),
                            source_offset: clip.pitch_correction_source_offset,
                            source_length: clip.pitch_correction_source_length,
                            preview_name: clip.pitch_correction_preview_name.clone(),
                            pitch_correction_points: clip
                                .pitch_correction_points
                                .iter()
                                .map(|point| maolan_engine::message::PitchCorrectionPointData {
                                    start_sample: point.start_sample,
                                    length_samples: point.length_samples,
                                    detected_midi_pitch: point.detected_midi_pitch,
                                    target_midi_pitch: point.target_midi_pitch,
                                    clarity: point.clarity,
                                })
                                .collect(),
                            pitch_correction_frame_likeness: clip.pitch_correction_frame_likeness,
                            pitch_correction_inertia_ms: clip.pitch_correction_inertia_ms,
                            pitch_correction_formant_compensation: clip
                                .pitch_correction_formant_compensation,
                            plugin_graph_json: clip.plugin_graph_json.clone(),
                        }));
                } else if let Some(clip) = track
                    .midi
                    .clips
                    .iter()
                    .find(|clip| clip.id == clip_ref.clip_id)
                {
                    let _ = CLIENT
                        .sender
                        .try_send(EngineMessage::Request(Action::AddClip {
                            clip_id: crate::state::generate_clip_id(),
                            name: clip.name.clone(),
                            track_name: track_name.clone(),
                            start,
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
                None
            }
            Message::SessionSlotDoubleClick {
                track_name,
                scene_index,
            } => {
                self.open_session_clip(&track_name, scene_index);
                None
            }
            Message::SessionViewScrollChanged { x, y } => {
                let mut state = self.state.blocking_write();
                state.session_view_scroll_x = x.clamp(0.0, 1.0);
                state.session_view_scroll_y = y.clamp(0.0, 1.0);
                None
            }
            Message::SessionNavMove { delta_x, delta_y } => {
                self.navigate_session_selection(delta_x, delta_y);
                None
            }
            Message::SessionNavLaunch => {
                self.launch_selected_session_slot();
                None
            }
            Message::SessionNavStopAll => {
                self.stop_all_session_clips();
                None
            }
            Message::SessionImportArrangement => {
                self.import_arrangement_to_session();
                None
            }
            Message::SessionRecordToArrangement => {
                self.record_session_to_arrangement();
                None
            }
            Message::SessionSlotRecord {
                track_name,
                scene_index,
            } => {
                self.record_into_session_slot(&track_name, scene_index);
                None
            }
            _ => None,
        }
    }

    fn navigate_session_selection(&mut self, delta_x: i32, delta_y: i32) {
        let state = self.state.blocking_read();
        let track_names: Vec<String> = state
            .tracks
            .iter()
            .filter(|track| track.name != crate::consts::state_ids::METRONOME_TRACK_ID)
            .map(|track| track.name.clone())
            .collect();
        let scene_count = state.session.scene_count();
        let selected = state.selected_slots.iter().next().cloned();
        drop(state);

        let (track_idx, scene_idx) = match selected {
            Some((track_name, scene_index)) => {
                let track_idx = track_names
                    .iter()
                    .position(|name| name == &track_name)
                    .unwrap_or(0);
                (track_idx as i32, scene_index as i32)
            }
            None => (0, 0),
        };

        let new_track_idx =
            (track_idx + delta_y).clamp(0, track_names.len().saturating_sub(1) as i32) as usize;
        let new_scene_idx =
            (scene_idx + delta_x).clamp(0, scene_count.saturating_sub(1) as i32) as usize;

        if let Some(track_name) = track_names.get(new_track_idx) {
            let mut state = self.state.blocking_write();
            state.selected_slots.clear();
            state
                .selected_slots
                .insert((track_name.clone(), new_scene_idx));
        }
    }

    fn launch_selected_session_slot(&mut self) {
        let maybe_selection = {
            let state = self.state.blocking_read();
            state.selected_slots.iter().next().cloned()
        };
        if let Some((track_name, scene_index)) = maybe_selection {
            self.toggle_session_slot(&track_name, scene_index);
        }
    }

    pub(super) fn toggle_session_slot(&mut self, track_name: &str, scene_index: usize) {
        let live_session_playing = self.live_session_playing;
        let engine_action = {
            let mut state = self.state.blocking_write();
            let (is_play_enabled, clip_ref) = {
                let Some(slot) = state.session.slot(track_name, scene_index) else {
                    return;
                };
                (slot.is_play_enabled(), slot.clip.clone())
            };
            let Some(clip_ref) = clip_ref else {
                return;
            };
            let launch_quantization = if live_session_playing {
                self.snap_mode.launch_quantization()
            } else {
                clip_ref.launch_quantization.into()
            };
            let runtime = state
                .slot_runtimes
                .entry((track_name.to_string(), scene_index))
                .or_default();
            match runtime.state {
                SlotPlayState::Stopped if is_play_enabled => {
                    runtime.state = SlotPlayState::Queued;
                    runtime.next_state = Some(SlotPlayState::Playing);
                    Some(Action::Session(SessionAction::LaunchClip {
                        track_name: track_name.to_string(),
                        scene_index,
                        clip_id: clip_ref.clip_id,
                        launch_quantization,
                        loop_enabled: clip_ref.loop_enabled,
                        loop_start_samples: clip_ref.loop_start_samples,
                        loop_end_samples: clip_ref.loop_end_samples,
                    }))
                }
                SlotPlayState::Stopped => None,
                SlotPlayState::Playing | SlotPlayState::Queued => {
                    runtime.state = SlotPlayState::Stopping;
                    runtime.next_state = Some(SlotPlayState::Stopped);
                    Some(Action::Session(SessionAction::StopClip {
                        track_name: track_name.to_string(),
                        scene_index,
                        launch_quantization,
                    }))
                }
                SlotPlayState::Stopping => None,
            }
        };
        if let Some(action) = engine_action {
            let _ = CLIENT.sender.try_send(EngineMessage::Request(action));
        }
    }

    fn launch_session_slot(&mut self, track_name: &str, scene_index: usize) {
        let engine_action = {
            let mut state = self.state.blocking_write();
            let Some(slot) = state.session.slot(track_name, scene_index) else {
                return;
            };
            if !slot.is_play_enabled() {
                return;
            }
            let Some(clip_ref) = slot.clip.clone() else {
                return;
            };
            let runtime = state
                .slot_runtimes
                .entry((track_name.to_string(), scene_index))
                .or_default();
            if runtime.state != SlotPlayState::Stopped {
                return;
            }
            runtime.state = SlotPlayState::Queued;
            runtime.next_state = Some(SlotPlayState::Playing);
            Some(Action::Session(SessionAction::LaunchClip {
                track_name: track_name.to_string(),
                scene_index,
                clip_id: clip_ref.clip_id,
                launch_quantization: self.snap_mode.launch_quantization(),
                loop_enabled: clip_ref.loop_enabled,
                loop_start_samples: clip_ref.loop_start_samples,
                loop_end_samples: clip_ref.loop_end_samples,
            }))
        };
        if let Some(action) = engine_action {
            let _ = CLIENT.sender.try_send(EngineMessage::Request(action));
        }
    }

    fn stop_session_slot(&mut self, track_name: &str, scene_index: usize) {
        let engine_action = {
            let mut state = self.state.blocking_write();
            if state.session.slot(track_name, scene_index).is_none() {
                return;
            }
            let runtime = state
                .slot_runtimes
                .entry((track_name.to_string(), scene_index))
                .or_default();
            if !matches!(
                runtime.state,
                SlotPlayState::Playing | SlotPlayState::Queued
            ) {
                return;
            }
            runtime.state = SlotPlayState::Stopping;
            runtime.next_state = Some(SlotPlayState::Stopped);
            Some(Action::Session(SessionAction::StopClip {
                track_name: track_name.to_string(),
                scene_index,
                launch_quantization: self.snap_mode.launch_quantization(),
            }))
        };
        if let Some(action) = engine_action {
            let _ = CLIENT.sender.try_send(EngineMessage::Request(action));
        }
    }

    pub(super) fn launch_session_scene(
        &mut self,
        scene_index: usize,
        force_loop: bool,
    ) -> Task<Message> {
        let (track_names, scene_tempo, current_tempo) = {
            let state = self.state.blocking_read();
            let track_names: Vec<String> = state.tracks.iter().map(|t| t.name.clone()).collect();
            let scene_tempo = state.session.scenes.get(scene_index).and_then(|s| s.tempo);
            (track_names, scene_tempo, state.tempo)
        };
        if force_loop {
            let launches: Vec<(String, crate::state::SlotClipRef)> = {
                let state = self.state.blocking_read();
                track_names
                    .iter()
                    .filter_map(|track_name| {
                        let slot = state.session.slot(track_name, scene_index)?;
                        if !slot.is_play_enabled() {
                            return None;
                        }
                        let clip_ref = slot.clip.clone()?;
                        Some((track_name.clone(), clip_ref))
                    })
                    .collect()
            };
            let actions: Vec<Action> = {
                let mut state = self.state.blocking_write();
                launches
                    .into_iter()
                    .map(|(track_name, clip_ref)| {
                        let runtime = state
                            .slot_runtimes
                            .entry((track_name.clone(), scene_index))
                            .or_default();
                        runtime.state = SlotPlayState::Queued;
                        runtime.next_state = Some(SlotPlayState::Playing);
                        Action::Session(SessionAction::LaunchClip {
                            track_name: track_name.clone(),
                            scene_index,
                            clip_id: clip_ref.clip_id,
                            launch_quantization: clip_ref.launch_quantization.into(),
                            loop_enabled: true,
                            loop_start_samples: 0,
                            loop_end_samples: 0,
                        })
                    })
                    .collect()
            };
            let mut tasks: Vec<Task<Message>> = actions
                .into_iter()
                .map(|action| self.send(action))
                .collect();
            if let Some(bpm) = scene_tempo {
                let bpm = bpm.clamp(20.0, 300.0);
                if (bpm - current_tempo).abs() > 0.01 {
                    tasks.push(Task::done(Message::TempoInputChanged(format!(
                        "{:.2}",
                        bpm
                    ))));
                    tasks.push(Task::done(Message::TempoInputCommit));
                }
            }
            Task::batch(tasks)
        } else {
            for track_name in track_names {
                self.toggle_session_slot(&track_name, scene_index);
            }
            if let Some(bpm) = scene_tempo {
                let bpm = bpm.clamp(20.0, 300.0);
                if (bpm - current_tempo).abs() > 0.01 {
                    return Task::batch(vec![
                        Task::done(Message::TempoInputChanged(format!("{:.2}", bpm))),
                        Task::done(Message::TempoInputCommit),
                    ]);
                }
            }
            Task::none()
        }
    }

    pub(super) fn start_live_session_play(&mut self) -> Task<Message> {
        tracing::info!(
            "start_live_session_play live_session_playing={}",
            self.live_session_playing
        );
        if self.live_session_playing {
            return Task::none();
        }
        self.live_session_playing = true;
        let scene = {
            let state = self.state.blocking_read();
            state
                .selected_scene
                .filter(|scene| *scene < state.session.scenes.len())
                .unwrap_or(0)
        };
        self.stop_workspace_playback(true)
            .chain(self.launch_session_scene(scene, true))
    }

    pub(super) fn stop_live_session_play(&mut self) -> Task<Message> {
        self.live_session_playing = false;
        self.stop_all_session_clips();
        self.stop_workspace_playback(false)
    }

    pub(super) fn stop_session_track(&mut self, track_name: &str) {
        let scene_count = {
            let state = self.state.blocking_read();
            state.session.scene_count()
        };
        let mut state = self.state.blocking_write();
        for scene_index in 0..scene_count {
            let runtime = state
                .slot_runtimes
                .entry((track_name.to_string(), scene_index))
                .or_default();
            if matches!(
                runtime.state,
                SlotPlayState::Playing | SlotPlayState::Queued
            ) {
                runtime.state = SlotPlayState::Stopping;
                runtime.next_state = Some(SlotPlayState::Stopped);
            }
        }
    }

    pub(super) fn stop_all_session_clips(&mut self) {
        self.live_session_playing = false;
        let _ = CLIENT
            .sender
            .try_send(EngineMessage::Request(Action::Session(
                SessionAction::StopAllClips,
            )));
        let mut state = self.state.blocking_write();
        for runtime in state.slot_runtimes.values_mut() {
            if matches!(
                runtime.state,
                SlotPlayState::Playing | SlotPlayState::Queued
            ) {
                runtime.state = SlotPlayState::Stopping;
                runtime.next_state = Some(SlotPlayState::Stopped);
            }
        }
    }

    fn move_session_slot(&mut self, from: (String, usize), to: (String, usize)) {
        let moved_clip = {
            let mut state = self.state.blocking_write();
            state.session.move_slot(&from.0, from.1, &to.0, to.1)
        };
        if let Some(clip_ref) = moved_clip {
            let _ = CLIENT
                .sender
                .try_send(EngineMessage::Request(Action::TrackSetSessionSlot {
                    track_name: from.0.clone(),
                    scene_index: from.1,
                    clip_id: None,
                }));
            let _ = CLIENT
                .sender
                .try_send(EngineMessage::Request(Action::TrackSetSessionSlot {
                    track_name: to.0,
                    scene_index: to.1,
                    clip_id: Some(clip_ref.clip_id),
                }));
        }
    }

    fn duplicate_session_slot(&mut self, track_name: &str, scene_index: usize) {
        let (scene_count, destination) = {
            let state = self.state.blocking_read();
            let scene_count = state.session.scene_count();
            let destination = (scene_index + 1..scene_count)
                .find(|&candidate| {
                    state
                        .session
                        .slot(track_name, candidate)
                        .is_none_or(|slot| slot.clip.is_none())
                })
                .map(|candidate| (track_name.to_string(), candidate));
            (scene_count, destination)
        };
        let Some((to_track, to_scene)) = destination else {
            self.state.blocking_write().message =
                "No empty slot to duplicate the clip reference into".to_string();
            return;
        };
        let clip_id = {
            let mut state = self.state.blocking_write();
            if !state
                .session
                .copy_slot(track_name, scene_index, &to_track, to_scene)
            {
                None
            } else {
                state
                    .session
                    .slot(&to_track, to_scene)
                    .and_then(|slot| slot.clip.as_ref())
                    .map(|clip_ref| clip_ref.clip_id.clone())
            }
        };
        if let Some(clip_id) = clip_id {
            let _ = CLIENT
                .sender
                .try_send(EngineMessage::Request(Action::TrackSetSessionSlot {
                    track_name: to_track,
                    scene_index: to_scene,
                    clip_id: Some(clip_id),
                }));
        }
        let _ = scene_count;
    }

    fn open_session_clip(&mut self, track_name: &str, scene_index: usize) {
        let maybe_clip_id = {
            let state = self.state.blocking_read();
            state
                .session
                .slot(track_name, scene_index)
                .and_then(|slot| slot.clip.as_ref())
                .map(|clip_ref| clip_ref.clip_id.clone())
        };
        let mut state = self.state.blocking_write();
        if let Some(clip_id) = maybe_clip_id {
            for track in &state.tracks {
                if track.name != *track_name {
                    continue;
                }
                if let Some((clip_idx, _)) = track
                    .audio
                    .clips
                    .iter()
                    .enumerate()
                    .find(|(_, c)| c.id == clip_id)
                {
                    state.plugin_graph_clip = Some(crate::state::PluginGraphClipTarget {
                        track_name: track_name.to_string(),
                        clip_idx,
                    });
                    state.view = crate::state::View::Workspace;
                    return;
                }
                if track.midi.clips.iter().any(|c| c.id == clip_id) {
                    state.view = crate::state::View::Piano;
                    return;
                }
            }
        }
    }

    fn import_arrangement_to_session(&mut self) {
        let track_clips: Vec<(String, Vec<String>)> = {
            let state = self.state.blocking_read();
            state
                .tracks
                .iter()
                .map(|track| {
                    let clip_ids: Vec<String> = track
                        .audio
                        .clips
                        .iter()
                        .map(|clip| clip.id.clone())
                        .chain(track.midi.clips.iter().map(|clip| clip.id.clone()))
                        .collect();
                    (track.name.clone(), clip_ids)
                })
                .collect()
        };

        let mut sync_actions = Vec::new();
        {
            let mut state = self.state.blocking_write();
            if state.session.scenes.is_empty() {
                state.session.scenes.push(crate::state::Scene {
                    name: "Scene 1".to_string(),
                    color: None,
                    launch_quantization: crate::state::LaunchQuantization::Bar,
                    tempo: None,
                });
            }

            let scene_count = state.session.scenes.len();
            for (track_name, clip_ids) in track_clips {
                state.session.ensure_track_slots(&track_name);
                let slots = state
                    .session
                    .slots
                    .get_mut(&track_name)
                    .expect("ensure_track_slots created slots");
                for (slot_index, clip_id) in clip_ids.iter().enumerate() {
                    let scene_index = slot_index.min(scene_count.saturating_sub(1));
                    if let Some(slot) = slots.get_mut(scene_index) {
                        slot.clip = Some(crate::state::SlotClipRef {
                            clip_id: clip_id.clone(),
                            launch_mode: crate::state::LaunchMode::Toggle,
                            launch_quantization: crate::state::LaunchQuantization::Bar,
                            loop_enabled: true,
                            loop_start_samples: 0,
                            loop_end_samples: 0,
                        });
                        sync_actions.push(Action::TrackSetSessionSlot {
                            track_name: track_name.clone(),
                            scene_index,
                            clip_id: Some(clip_id.clone()),
                        });
                    }
                }
            }
            state.message = "Imported arrangement clips to session".to_string();
        }
        for action in sync_actions {
            let _ = CLIENT.sender.try_send(EngineMessage::Request(action));
        }
    }

    fn record_session_to_arrangement(&mut self) {
        if self.session_dir.is_none() {
            self.state.blocking_write().message =
                "Save the session before recording to arrangement".to_string();
            return;
        }

        let tracks_to_arm: Vec<String> = {
            let state = self.state.blocking_read();
            let armed: std::collections::HashSet<String> = state
                .tracks
                .iter()
                .filter(|t| t.armed)
                .map(|t| t.name.clone())
                .collect();
            state
                .tracks
                .iter()
                .filter(|t| t.name != crate::consts::state_ids::METRONOME_TRACK_ID)
                .filter(|t| {
                    if armed.contains(&t.name) {
                        return false;
                    }
                    state
                        .session
                        .slots
                        .get(&t.name)
                        .is_some_and(|slots| slots.iter().any(|slot| slot.clip.is_some()))
                })
                .map(|t| t.name.clone())
                .collect()
        };

        for track_name in tracks_to_arm {
            let _ = CLIENT
                .sender
                .try_send(EngineMessage::Request(Action::TrackToggleArm(track_name)));
        }

        if !self.playing {
            let _ = CLIENT.sender.try_send(EngineMessage::Request(Action::Play));
            self.playing = true;
            self.paused = false;
        }

        if !self.record_armed {
            self.record_armed = true;
            if self.playing {
                self.start_recording_preview();
            }
            let _ = CLIENT
                .sender
                .try_send(EngineMessage::Request(Action::SetRecordEnabled(true)));
        }

        self.state.blocking_write().message = "Recording session to arrangement".to_string();
    }

    fn record_into_session_slot(&mut self, track_name: &str, scene_index: usize) {
        let already_armed = {
            let state = self.state.blocking_read();
            state
                .tracks
                .iter()
                .find(|t| t.name == track_name)
                .is_some_and(|t| t.armed)
        };
        if !already_armed {
            let _ = CLIENT
                .sender
                .try_send(EngineMessage::Request(Action::TrackToggleArm(
                    track_name.to_string(),
                )));
        }

        self.session_slot_record_target = Some((track_name.to_string(), scene_index));

        if !self.playing {
            let _ = CLIENT.sender.try_send(EngineMessage::Request(Action::Play));
            self.playing = true;
            self.paused = false;
        }

        if !self.record_armed {
            self.record_armed = true;
            if self.playing {
                self.start_recording_preview();
            }
            let _ = CLIENT
                .sender
                .try_send(EngineMessage::Request(Action::SetRecordEnabled(true)));
        }

        self.state.blocking_write().message = format!(
            "Recording into session slot {}:{}",
            track_name,
            scene_index + 1
        );
    }

    pub(super) fn collect_valid_clip_ids(&self) -> std::collections::HashSet<String> {
        let state = self.state.blocking_read();
        state
            .tracks
            .iter()
            .flat_map(|track| {
                track
                    .audio
                    .clips
                    .iter()
                    .map(|clip| clip.id.clone())
                    .chain(track.midi.clips.iter().map(|clip| clip.id.clone()))
            })
            .collect()
    }

    pub(super) fn clear_session_slots_for_clip_ids(
        &mut self,
        clip_ids: &std::collections::HashSet<String>,
    ) -> Vec<Action> {
        let mut sync_actions = Vec::new();
        {
            let mut state = self.state.blocking_write();
            for (track_name, slots) in &mut state.session.slots {
                for (scene_index, slot) in slots.iter_mut().enumerate() {
                    if slot
                        .clip
                        .as_ref()
                        .is_some_and(|clip_ref| clip_ids.contains(&clip_ref.clip_id))
                    {
                        slot.clip = None;
                        sync_actions.push(Action::TrackSetSessionSlot {
                            track_name: track_name.clone(),
                            scene_index,
                            clip_id: None,
                        });
                    }
                }
            }
        }
        sync_actions
    }

    pub(super) fn cleanup_session_slot_references(&mut self) -> Vec<Action> {
        let valid_ids = self.collect_valid_clip_ids();
        let mut sync_actions = Vec::new();
        {
            let mut state = self.state.blocking_write();
            let mut cleared = 0;
            for (track_name, slots) in &mut state.session.slots {
                for (scene_index, slot) in slots.iter_mut().enumerate() {
                    if slot
                        .clip
                        .as_ref()
                        .is_some_and(|clip_ref| !valid_ids.contains(&clip_ref.clip_id))
                    {
                        slot.clip = None;
                        cleared += 1;
                        sync_actions.push(Action::TrackSetSessionSlot {
                            track_name: track_name.clone(),
                            scene_index,
                            clip_id: None,
                        });
                    }
                }
            }
            if cleared > 0 {
                state.message = format!("Cleared {} dangling session slot reference(s)", cleared);
            }
        }
        sync_actions
    }
}

fn slot_zone_id(track_name: &str, scene_index: usize) -> String {
    format!("session-slot:{}:{}", track_name, scene_index)
}

pub(super) fn build_slot_zone_map(
    tracks: &[crate::state::Track],
    session: &crate::state::SessionMatrix,
) -> std::collections::HashMap<Id, (String, usize)> {
    let mut map = std::collections::HashMap::new();
    let scene_count = session.scene_count();
    for track in tracks {
        if track.name == crate::consts::state_ids::METRONOME_TRACK_ID {
            continue;
        }
        for scene_index in 0..scene_count {
            let id = Id::from(slot_zone_id(&track.name, scene_index));
            map.insert(id, (track.name.clone(), scene_index));
        }
    }
    map
}
