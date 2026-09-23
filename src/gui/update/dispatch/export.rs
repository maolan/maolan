use super::*;

impl Maolan {
    pub(super) fn handle_export_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::TrackFreezeToggle { ref track_name } => {
                if self.pending.freeze_in_progress {
                    if self.pending.freeze_track_name.as_deref() == Some(track_name.as_str()) {
                        self.pending.freeze_cancel_requested = true;
                        self.state.write().expect("state lock poisoned").message =
                            format!("Cancel requested for freezing '{}'", track_name);
                        return self.send(Action::TrackOfflineBounceCancel {
                            track_name: track_name.clone(),
                        });
                    } else {
                        self.state.write().expect("state lock poisoned").message = format!(
                            "Freeze in progress for '{}'",
                            self.pending.freeze_track_name.clone().unwrap_or_default()
                        );
                    }
                    return Task::none();
                }
                let Some(session_root) = self.session_dir.clone() else {
                    self.state.write().expect("state lock poisoned").message =
                        "Freeze requires an opened/saved session".to_string();
                    return Task::none();
                };
                let track_snapshot = {
                    let state = self.state.read().expect("state lock poisoned");
                    state.tracks.iter().find(|t| t.name == *track_name).cloned()
                };
                let Some(track) = track_snapshot else {
                    self.state.write().expect("state lock poisoned").message =
                        format!("Track '{}' not found", track_name);
                    return Task::none();
                };

                if track.frozen {
                    let current_audio_len = track.audio.clips.len();
                    let current_midi_len = track.midi.clips.len();
                    let restore_audio = track.frozen_audio_backup.clone();
                    let restore_midi = track.frozen_midi_backup.clone();
                    {
                        let mut state = self.state.write().expect("state lock poisoned");
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
                                reversed: clip.reversed,
                                gain_db: clip.gain_db,
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
                                pitch_correction_detector: clip.pitch_correction_detector,
                                pitch_correction_mode: clip.pitch_correction_mode,
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
                            reversed: clip.reversed,
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
                    self.state.write().expect("state lock poisoned").message =
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
                    self.state.write().expect("state lock poisoned").message =
                        format!("Preparing pitch-corrected freeze for '{}'", track_name);
                    return Task::perform(
                        {
                            let session_root = session_root.clone();
                            let track_name = track_name.clone();
                            async move {
                                let mut prepared_clips = Vec::new();
                                for (clip_index, clip) in corrected_clips {
                                    // Resynthesized clips reuse their rendered
                                    // preview instead of a timestretch render.
                                    if clip.pitch_correction_mode
                                        == maolan_engine::message::PitchCorrectionMode::Resynth
                                        && let Some(preview_name) =
                                            clip.pitch_correction_preview_name.clone()
                                    {
                                        let preview_path =
                                            if std::path::Path::new(&preview_name).is_absolute() {
                                                std::path::PathBuf::from(&preview_name)
                                            } else {
                                                session_root.join(&preview_name)
                                            };
                                        if preview_path.exists() {
                                            prepared_clips.push(
                                                crate::message::PreparedFreezeClip {
                                                    clip_index,
                                                    preview_name,
                                                },
                                            );
                                            continue;
                                        }
                                    }
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
                                        Self::render_audio_clip_pitch_correction_with_timestretch(
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
                            self.state.write().expect("state lock poisoned").message =
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
                    let target = match &lane.target {
                        crate::message::TrackAutomationTarget::Volume => {
                            OfflineAutomationTarget::Volume
                        }
                        crate::message::TrackAutomationTarget::Balance => {
                            OfflineAutomationTarget::Balance
                        }
                        crate::message::TrackAutomationTarget::MidiCc { channel, cc } => {
                            OfflineAutomationTarget::MidiCc {
                                channel: *channel,
                                cc: *cc,
                            }
                        }
                        #[cfg(unix)]
                        crate::message::TrackAutomationTarget::Lv2Parameter {
                            instance_id,
                            index,
                            min,
                            max,
                        } => OfflineAutomationTarget::Lv2Parameter {
                            instance_id: *instance_id,
                            index: *index,
                            min: *min,
                            max: *max,
                        },
                        #[cfg(not(unix))]
                        crate::message::TrackAutomationTarget::Lv2Parameter { .. } => continue,
                        crate::message::TrackAutomationTarget::Vst3Parameter {
                            instance_id,
                            param_id,
                        } => OfflineAutomationTarget::Vst3Parameter {
                            instance_id: *instance_id,
                            param_id: *param_id,
                        },
                        crate::message::TrackAutomationTarget::ClapParameter {
                            instance_id,
                            param_id,
                            min,
                            max,
                        } => OfflineAutomationTarget::ClapParameter {
                            instance_id: *instance_id,
                            param_id: *param_id,
                            min: *min,
                            max: *max,
                        },
                        crate::message::TrackAutomationTarget::MixOsc { addr, path } => {
                            OfflineAutomationTarget::MixOsc {
                                addr: addr.clone(),
                                path: path.clone(),
                            }
                        }
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
                self.pending.pending_track_freeze_bounce.insert(
                    track_name.clone(),
                    super::super::super::PendingTrackFreezeBounce {
                        rendered_clip_rel: render_rel,
                        rendered_length: render_length.max(1),
                        backup_audio: track.audio.clips.clone(),
                        backup_midi: track.midi.clips.clone(),
                    },
                );
                self.pending.freeze_in_progress = true;
                self.pending.freeze_progress = 0.0;
                self.pending.freeze_track_name = Some(track_name.clone());
                self.pending.freeze_cancel_requested = false;
                self.state.write().expect("state lock poisoned").message =
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
                    self.state.write().expect("state lock poisoned").message =
                        format!("Failed to prepare freeze for '{}': {e}", track_name);
                    return Task::none();
                }
                let Some(session_root) = self.session_dir.clone() else {
                    self.state.write().expect("state lock poisoned").message =
                        "Freeze requires an opened/saved session".to_string();
                    return Task::none();
                };
                let original_track_snapshot = {
                    let state = self.state.read().expect("state lock poisoned");
                    state.tracks.iter().find(|t| t.name == *track_name).cloned()
                };
                let Some(track) = original_track_snapshot.clone() else {
                    self.state.write().expect("state lock poisoned").message =
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
                            self.state.write().expect("state lock poisoned").message =
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
                    let target = match &lane.target {
                        crate::message::TrackAutomationTarget::Volume => {
                            OfflineAutomationTarget::Volume
                        }
                        crate::message::TrackAutomationTarget::Balance => {
                            OfflineAutomationTarget::Balance
                        }
                        crate::message::TrackAutomationTarget::MidiCc { channel, cc } => {
                            OfflineAutomationTarget::MidiCc {
                                channel: *channel,
                                cc: *cc,
                            }
                        }
                        #[cfg(unix)]
                        crate::message::TrackAutomationTarget::Lv2Parameter {
                            instance_id,
                            index,
                            min,
                            max,
                        } => OfflineAutomationTarget::Lv2Parameter {
                            instance_id: *instance_id,
                            index: *index,
                            min: *min,
                            max: *max,
                        },
                        #[cfg(not(unix))]
                        crate::message::TrackAutomationTarget::Lv2Parameter { .. } => continue,
                        crate::message::TrackAutomationTarget::Vst3Parameter {
                            instance_id,
                            param_id,
                        } => OfflineAutomationTarget::Vst3Parameter {
                            instance_id: *instance_id,
                            param_id: *param_id,
                        },
                        crate::message::TrackAutomationTarget::ClapParameter {
                            instance_id,
                            param_id,
                            min,
                            max,
                        } => OfflineAutomationTarget::ClapParameter {
                            instance_id: *instance_id,
                            param_id: *param_id,
                            min: *min,
                            max: *max,
                        },
                        crate::message::TrackAutomationTarget::MixOsc { addr, path } => {
                            OfflineAutomationTarget::MixOsc {
                                addr: addr.clone(),
                                path: path.clone(),
                            }
                        }
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
                self.pending.pending_track_freeze_bounce.insert(
                    track_name.clone(),
                    super::super::super::PendingTrackFreezeBounce {
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
                self.pending.freeze_in_progress = true;
                self.pending.freeze_progress = 0.0;
                self.pending.freeze_track_name = Some(track_name.clone());
                self.pending.freeze_cancel_requested = false;
                self.state.write().expect("state lock poisoned").message =
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
                    let state = self.state.read().expect("state lock poisoned");
                    state
                        .tracks
                        .iter()
                        .find(|t| t.name == *track_name)
                        .is_some_and(|t| t.frozen)
                };
                if !is_frozen {
                    self.state.write().expect("state lock poisoned").message =
                        format!("Track '{}' is not frozen", track_name);
                    return Task::none();
                }
                {
                    let mut state = self.state.write().expect("state lock poisoned");
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
            Message::OpenExporter => {
                if self.session_dir.is_none() {
                    self.state.write().expect("state lock poisoned").message =
                        "Export requires an opened/saved session".to_string();
                    return Task::none();
                }
                let nearest_rate = crate::consts::gui_mod::STANDARD_EXPORT_SAMPLE_RATES
                    .iter()
                    .min_by_key(|rate| {
                        (i64::from(**rate) - self.transport.playback_rate_hz.round() as i64).abs()
                    })
                    .copied()
                    .unwrap_or(48_000);
                self.transfer.export_sample_rate_hz = nearest_rate;
                self.transfer.normalize_hw_out_ports(&self.state);
                self.modal = Some(crate::message::Show::ExportSettings);
            }
            Message::MidiLearnMappingsPanelToggle => {
                self.ui.midi_mappings_panel_open = !self.ui.midi_mappings_panel_open;
                if self.ui.midi_mappings_panel_open {
                    return self.send(Action::RequestMidiLearnMappingsReport);
                }
            }
            Message::MidiLearnMappingsReportRequest => {
                return self.send(Action::RequestMidiLearnMappingsReport);
            }
            Message::MidiLearnMappingsExportRequest => match self.export_midi_mappings_file() {
                Ok(path) => {
                    self.state.write().expect("state lock poisoned").message =
                        format!("Exported MIDI mappings: {}", path.display());
                }
                Err(e) => {
                    self.state.write().expect("state lock poisoned").message = e;
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
                    self.state.write().expect("state lock poisoned").message =
                        "Imported MIDI mappings".to_string();
                    return Task::batch(tasks);
                }
                Err(e) => {
                    self.state.write().expect("state lock poisoned").message = e;
                }
            },
            Message::MidiLearnMappingsClearAllRequest => {
                return self.send(Action::ClearAllMidiLearnBindings);
            }
            Message::ExportSettingsConfirm => {
                let master_ceiling = self
                    .transfer
                    .export_master_limiter_ceiling_input
                    .parse::<f32>()
                    .ok();
                let Some(master_ceiling) = master_ceiling else {
                    self.state.write().expect("state lock poisoned").message =
                        "Master limiter ceiling must be a number in dBTP".to_string();
                    return Task::none();
                };
                if !(-20.0..=0.0).contains(&master_ceiling) {
                    self.state.write().expect("state lock poisoned").message =
                        "Master limiter ceiling must be between -20.0 and 0.0 dBTP".to_string();
                    return Task::none();
                }
                if self.transfer.export_normalize {
                    match self.transfer.export_normalize_mode {
                        ExportNormalizeMode::Peak => {
                            let target = self
                                .transfer
                                .export_normalize_dbfs_input
                                .parse::<f32>()
                                .ok();
                            let Some(target) = target else {
                                self.state.write().expect("state lock poisoned").message =
                                    "Normalize target must be a number in dBFS".to_string();
                                return Task::none();
                            };
                            if !(-60.0..=0.0).contains(&target) {
                                self.state.write().expect("state lock poisoned").message =
                                    "Normalize target must be between -60.0 and 0.0 dBFS"
                                        .to_string();
                                return Task::none();
                            }
                        }
                        ExportNormalizeMode::Loudness => {
                            let lufs = self
                                .transfer
                                .export_normalize_lufs_input
                                .parse::<f32>()
                                .ok();
                            let dbtp = self
                                .transfer
                                .export_normalize_dbtp_input
                                .parse::<f32>()
                                .ok();
                            let (Some(lufs), Some(dbtp)) = (lufs, dbtp) else {
                                self.state.write().expect("state lock poisoned").message =
                                    "Loudness mode requires numeric LUFS and dBTP values"
                                        .to_string();
                                return Task::none();
                            };
                            if !(-70.0..=-5.0).contains(&lufs) {
                                self.state.write().expect("state lock poisoned").message =
                                    "LUFS target must be between -70.0 and -5.0".to_string();
                                return Task::none();
                            }
                            if !(-20.0..=0.0).contains(&dbtp) {
                                self.state.write().expect("state lock poisoned").message =
                                    "dBTP ceiling must be between -20.0 and 0.0".to_string();
                                return Task::none();
                            }
                        }
                    }
                }
                let selected_formats = self.transfer.selected_formats();
                if selected_formats.is_empty() {
                    self.state.write().expect("state lock poisoned").message =
                        "Select at least one export format".to_string();
                    return Task::none();
                }
                if matches!(self.transfer.export_render_mode, ExportRenderMode::Mixdown)
                    && self.transfer.export_hw_out_ports.is_empty()
                {
                    self.state.write().expect("state lock poisoned").message =
                        "Select at least one hw:out port for mixdown export".to_string();
                    return Task::none();
                }
                self.modal = None;
                return Task::perform(
                    async move {
                        AsyncFileDialog::new()
                            .set_title("Export Audio")
                            .add_filter("Audio", &["wav", "flac"])
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
                    self.state.write().expect("state lock poisoned").message =
                        "Export requires an opened/saved session".to_string();
                    return Task::none();
                };

                let sample_rate = self.transfer.export_sample_rate_hz as i32;
                let export_bit_depth = self.transfer.export_bit_depth;
                let export_dither = self.transfer.export_dither;
                let export_normalize = self.transfer.export_normalize;
                let normalize_mode = self.transfer.export_normalize_mode;
                let normalize_target_dbfs = self
                    .transfer
                    .export_normalize_dbfs_input
                    .parse::<f32>()
                    .ok()
                    .unwrap_or(0.0);
                let normalize_target_lufs = self
                    .transfer
                    .export_normalize_lufs_input
                    .parse::<f32>()
                    .ok()
                    .unwrap_or(-23.0);
                let normalize_true_peak_dbtp = self
                    .transfer
                    .export_normalize_dbtp_input
                    .parse::<f32>()
                    .ok()
                    .unwrap_or(-1.0);
                let normalize_tp_limiter = self.transfer.export_normalize_tp_limiter;
                let export_master_limiter = self.transfer.export_master_limiter;
                let export_master_limiter_ceiling_dbtp = self
                    .transfer
                    .export_master_limiter_ceiling_input
                    .parse::<f32>()
                    .ok()
                    .unwrap_or(-1.0);
                let export_realtime_fallback = self.transfer.export_realtime_fallback;
                let export_formats = self.transfer.selected_formats();
                if export_formats.is_empty() {
                    self.state.write().expect("state lock poisoned").message =
                        "Select at least one export format".to_string();
                    return Task::none();
                }
                let export_path = Self::export_base_path(path.clone());
                let selected_hw_out_ports =
                    self.transfer.export_hw_out_ports.iter().copied().collect();
                let state_clone = self.state.clone();
                let render_mode = self.transfer.export_render_mode;

                self.transfer.export_in_progress = true;
                self.transfer.export_progress = 0.0;
                self.transfer.export_operation = Some("Preparing".to_string());
                self.transfer.export_cancel =
                    std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                let export_cancel = self.transfer.export_cancel.clone();

                self.transfer.export_pending_bounces.clear();
                if matches!(
                    render_mode,
                    crate::message::ExportRenderMode::StemsPostFader
                ) {
                    let state_guard = self.state.read().expect("state lock poisoned");
                    let has_solo = state_guard.tracks.iter().any(|t| t.soloed);
                    let selected_set: std::collections::HashSet<String> =
                        state_guard.selected.iter().cloned().collect();
                    for track in &state_guard.tracks {
                        if selected_set.contains(&track.name)
                            && !track.muted
                            && (!has_solo || track.soloed)
                        {
                            self.transfer
                                .export_pending_bounces
                                .insert(track.name.clone());
                        }
                    }
                }
                let bounce_notify = std::sync::Arc::new(tokio::sync::Notify::new());
                self.transfer.export_bounce_notify = Some(bounce_notify.clone());

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

                            let options = super::super::super::ExportSessionOptions {
                                export_path: export_path.clone(),
                                sample_rate,
                                formats: export_formats,
                                render_mode,
                                selected_hw_out_ports,
                                realtime_fallback: export_realtime_fallback,
                                bit_depth: export_bit_depth,
                                dither: export_dither,
                                normalize: export_normalize,
                                normalize_target_dbfs,
                                normalize_mode,
                                normalize_target_lufs,
                                normalize_true_peak_dbtp,
                                normalize_tp_limiter,
                                master_limiter: export_master_limiter,
                                master_limiter_ceiling_dbtp: export_master_limiter_ceiling_dbtp,
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

                        maolan_widgets::iced::futures::stream::unfold(rx, |mut rx| async move {
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
                if (self.transfer.export_progress - progress).abs() < f32::EPSILON
                    && self.transfer.export_operation == *operation
                {
                    return Task::none();
                }
                self.transfer.export_progress = progress;
                self.transfer.export_operation = operation.clone();

                if let Some(op) = operation
                    && op.starts_with("Error:")
                {
                    self.transfer.export_in_progress = false;
                    self.state.write().expect("state lock poisoned").message = op.clone();
                } else if progress >= 1.0 {
                    self.transfer.export_in_progress = false;
                    self.state.write().expect("state lock poisoned").message = operation
                        .clone()
                        .unwrap_or_else(|| "Export complete".to_string());
                } else if let Some(op) = operation {
                    let percent = (progress * 100.0) as usize;
                    self.state.write().expect("state lock poisoned").message =
                        format!("Exporting ({percent}%): {}", op);
                } else {
                    let percent = (progress * 100.0) as usize;
                    self.state.write().expect("state lock poisoned").message =
                        format!("Exporting ({percent}%)...");
                }
            }
            _ => {}
        }
        self.update_children(&message);
        Task::none()
    }
}
