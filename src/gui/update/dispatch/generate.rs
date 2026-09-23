use super::*;

impl Maolan {
    pub(super) fn handle_generate_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::GenerateAudioModelSelected(model) => {
                self.generate.generate_audio_model = model;
            }
            Message::GenerateAudioAceStepLmSelected(lm) => {
                self.generate.generate_audio_acestep_lm = lm;
            }
            Message::GenerateAudioPromptAction(ref action) => {
                self.generate
                    .generate_audio_prompt_editor
                    .perform(action.clone());
            }

            Message::GenerateAudioTagsInput(ref value) => {
                self.generate.generate_audio_tags_input = value.clone();
            }

            Message::GenerateAudioBackendSelected(backend) => {
                self.generate.generate_audio_backend = backend;
            }
            Message::GenerateAudioKeyRootChanged(root) => {
                self.generate.generate_audio_key_root = root;
            }
            Message::GenerateAudioKeyModeChanged(mode) => {
                self.generate.generate_audio_key_mode = mode;
            }

            Message::GenerateAudioCfgScaleInput(ref value) => {
                self.generate.generate_audio_cfg_scale_input = value.clone();
            }
            Message::GenerateAudioStepsInput(value) => {
                self.generate.generate_audio_steps_input = value;
            }
            Message::GenerateAudioSecondsTotalInput(value) => {
                self.generate.generate_audio_seconds_total_input = value;
            }
            Message::GenerateAudioCancel => {
                #[cfg(unix)]
                if let Some(pid) = self.generate.generate_audio_process_id.take() {
                    use nix::sys::signal::{self, Signal};
                    use nix::unistd::Pid;
                    let _ = signal::kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
                }
                if let Some(handle) = self.generate.generate_audio_abort_handle.take() {
                    handle.abort();
                }
                self.generate.generate_audio_in_progress = false;
                self.generate.generate_audio_progress = 0.0;
                self.generate.generate_audio_operation = None;
                self.info("Generation cancelled");
            }
            Message::GenerateAudioSubmit => {
                if self.generate.generate_audio_in_progress {
                    return Task::none();
                }
                let Some(session_root) = self.session_dir.clone() else {
                    self.warning("Generated audio requires an opened/saved session");
                    return Task::none();
                };

                let prompt = self
                    .generate
                    .generate_audio_prompt_editor
                    .text()
                    .trim()
                    .to_string();
                if prompt.is_empty() {
                    self.warning("Prompt cannot be empty");
                    return Task::none();
                }
                let cfg_scale = match self
                    .generate
                    .generate_audio_cfg_scale_input
                    .trim()
                    .parse::<f32>()
                {
                    Ok(value) if value.is_finite() && (0.0..=20.0).contains(&value) => value,
                    _ => {
                        self.warning("CFG scale must be a number between 0 and 20");
                        return Task::none();
                    }
                };
                let ode_steps = self.generate.generate_audio_steps_input;
                if ode_steps == 0 || ode_steps > 50 {
                    self.warning("ODE steps must be between 1 and 50");
                    return Task::none();
                }
                let transport_sample = self.transport.transport_samples.max(0.0) as usize;
                let (bpm, time_signature_num, time_signature_denom) = {
                    let state = self.state.read().expect("state lock poisoned");
                    Self::timing_at_sample(&state, transport_sample)
                };
                let musical_key = crate::state::MusicalKey {
                    root: self.generate.generate_audio_key_root,
                    mode: self.generate.generate_audio_key_mode,
                };
                {
                    let mut state = self.state.write().expect("state lock poisoned");
                    if state.musical_key != musical_key {
                        state.musical_key = musical_key;
                        self.session_ops.has_unsaved_changes = true;
                    }
                }
                let output_stem =
                    super::super::super::Maolan::sanitize_generated_track_base_name(&prompt);
                let request = super::super::super::BurnGenerateRequest {
                    model: self.generate.generate_audio_model,
                    prompt,
                    output_path: {
                        let output_rel = match super::super::super::Maolan::unique_import_rel_path(
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
                        Some(
                            super::super::super::Maolan::generate_audio_tags_with_timing(
                                self.generate.generate_audio_tags_input.trim(),
                                bpm,
                                time_signature_num,
                                time_signature_denom,
                            ),
                        )
                    },
                    backend: self.generate.generate_audio_backend,
                    acestep_lm: self
                        .generate
                        .generate_audio_model
                        .uses_acestep_lm_selector()
                        .then_some(self.generate.generate_audio_acestep_lm),
                    cfg_scale,
                    ode_steps,
                    length: self
                        .generate
                        .generate_audio_seconds_total_input
                        .saturating_mul(1000),
                    bpm: Some(bpm),
                    key_scale: Some(musical_key.to_string()),
                    time_signature: Some(format!(
                        "{}/{}",
                        time_signature_num.max(1),
                        time_signature_denom.max(1)
                    )),
                };

                let _used_track_names: HashSet<String> = self
                    .state
                    .read()
                    .expect("state lock poisoned")
                    .tracks
                    .iter()
                    .map(|track| track.name.clone())
                    .collect();

                self.generate.generate_audio_in_progress = true;
                self.generate.generate_audio_progress = 0.0;
                self.generate.generate_audio_operation = Some("Launching generate".to_string());

                let (_pid, socket) =
                    match super::super::super::Maolan::spawn_generate_process(&request) {
                        Ok((_pid, socket)) => (_pid, socket),
                        Err(err) => {
                            self.generate.generate_audio_in_progress = false;
                            self.error(err);
                            return Task::none();
                        }
                    };
                #[cfg(unix)]
                {
                    self.generate.generate_audio_process_id = Some(_pid);
                }

                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                #[cfg(unix)]
                let _playback_rate = self.transport.playback_rate_hz;
                let task_handle = tokio::spawn(async move {
                    let _ = tx.send(Message::GenerateAudioProgress {
                        progress: 0.05,
                        operation: Some("Generating audio".to_string()),
                    });

                    let tx_progress = tx.clone();
                    match tokio::task::spawn_blocking(move || {
                        super::super::super::Maolan::communicate_with_generate_process(
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
                        let base_name =
                            super::super::super::Maolan::sanitize_generated_track_base_name(
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
                                super::super::super::Maolan::audio_clip_channel_count(&output_path)
                                    .map_err(|e| {
                                        format!(
                                            "Failed to inspect generated audio channels '{}': {e}",
                                            output_path.display()
                                        )
                                    })?;
                            let length =
                                super::super::super::Maolan::audio_clip_source_length(&output_path)
                                    .map_err(|e| {
                                        format!(
                                            "Failed to read generated audio length '{}': {e}",
                                            output_path.display()
                                        )
                                    })?;
                            let peaks =
                                super::super::super::Maolan::compute_audio_clip_peaks(&output_path)
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

                        let track_name = super::super::super::Maolan::unique_track_name(
                            &base_name,
                            &mut used_names,
                        );
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
                                mixosc_addr: None,
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
                                    super::super::super::Maolan::default_clip_plugin_graph_json(
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

                self.generate.generate_audio_abort_handle = Some(task_handle.abort_handle());

                return Task::run(
                    maolan_widgets::iced::futures::stream::unfold(rx, |mut rx| async move {
                        rx.recv().await.map(|msg| (msg, rx))
                    }),
                    |msg| msg,
                );
            }
            Message::GenerateAudioProgress {
                progress,
                ref operation,
            } => {
                self.generate.generate_audio_progress = progress.clamp(0.0, 1.0);
                self.generate.generate_audio_operation = operation.clone();
                if let Some(operation) = operation {
                    self.state.write().expect("state lock poisoned").message = operation.clone();
                }
            }
            Message::GenerateAudioFinished(ref result) => {
                self.generate.generate_audio_in_progress = false;
                self.generate.generate_audio_progress = if result.is_ok() { 1.0 } else { 0.0 };
                self.generate.generate_audio_operation = None;
                self.generate.generate_audio_abort_handle = None;
                #[cfg(unix)]
                {
                    self.generate.generate_audio_process_id = None;
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
            Message::GenerateMidiModelSelected(model) => {
                self.generate.generate_midi_model = model;
            }
            Message::GenerateMidiPromptAction(ref action) => {
                self.generate
                    .generate_midi_prompt_editor
                    .perform(action.clone());
            }
            Message::GenerateMidiBackendSelected(backend) => {
                self.generate.generate_midi_backend = backend;
            }
            Message::GenerateMidiKeyRootChanged(root) => {
                self.generate.generate_midi_key_root = root;
            }
            Message::GenerateMidiKeyModeChanged(mode) => {
                self.generate.generate_midi_key_mode = mode;
            }
            Message::GenerateMidiBpmInput(ref value) => {
                self.generate.generate_midi_bpm_input = value.clone();
            }
            Message::GenerateMidiTimeSignatureNumInput(ref value) => {
                self.generate.generate_midi_time_signature_num_input = value.clone();
            }
            Message::GenerateMidiTimeSignatureDenomInput(ref value) => {
                self.generate.generate_midi_time_signature_denom_input = value.clone();
            }
            Message::GenerateMidiLengthSecondsInput(ref value) => {
                self.generate.generate_midi_length_seconds_input = value.clone();
            }
            Message::GenerateMidiMaxTokensInput(ref value) => {
                self.generate.generate_midi_max_tokens_input = value.clone();
            }
            Message::GenerateMidiTopPInput(ref value) => {
                self.generate.generate_midi_top_p_input = value.clone();
            }
            Message::GenerateMidiSeedInput(ref value) => {
                self.generate.generate_midi_seed_input = value.clone();
            }
            Message::GenerateMidiCancel => {
                #[cfg(unix)]
                if let Some(pid) = self.generate.generate_midi_process_id.take() {
                    use nix::sys::signal::{self, Signal};
                    use nix::unistd::Pid;
                    let _ = signal::kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
                }
                if let Some(handle) = self.generate.generate_midi_abort_handle.take() {
                    handle.abort();
                }
                self.generate.generate_midi_in_progress = false;
                self.generate.generate_midi_progress = 0.0;
                self.generate.generate_midi_operation = None;
                self.info("MIDI generation cancelled");
            }
            Message::GenerateMidiSubmit => {
                if self.generate.generate_midi_in_progress {
                    return Task::none();
                }
                let Some(session_root) = self.session_dir.clone() else {
                    self.warning("Generated MIDI requires an opened/saved session");
                    return Task::none();
                };

                let prompt = self
                    .generate
                    .generate_midi_prompt_editor
                    .text()
                    .trim()
                    .to_string();
                if prompt.is_empty() {
                    self.warning("Prompt cannot be empty");
                    return Task::none();
                }

                let bpm = match self.generate.generate_midi_bpm_input.trim().parse::<f32>() {
                    Ok(value) if value.is_finite() && (20.0..=400.0).contains(&value) => value,
                    _ => {
                        self.warning("BPM must be a number between 20 and 400");
                        return Task::none();
                    }
                };

                let time_signature_num = match self
                    .generate
                    .generate_midi_time_signature_num_input
                    .trim()
                    .parse::<u8>()
                {
                    Ok(value) if (1..=64).contains(&value) => value,
                    _ => {
                        self.warning("Time signature numerator must be between 1 and 64");
                        return Task::none();
                    }
                };

                let time_signature_denom = match self
                    .generate
                    .generate_midi_time_signature_denom_input
                    .trim()
                    .parse::<u8>()
                {
                    Ok(value) if (1..=64).contains(&value) => value,
                    _ => {
                        self.warning("Time signature denominator must be between 1 and 64");
                        return Task::none();
                    }
                };

                let midi_seed = match self.generate.generate_midi_seed_input.trim().parse::<u64>() {
                    Ok(value) => value,
                    _ => {
                        self.warning("Seed must be a non-negative whole number");
                        return Task::none();
                    }
                };

                let (model, midi_length_seconds, midi_max_tokens, midi_top_p) = match self
                    .generate
                    .generate_midi_model
                {
                    crate::message::GenerateMidiModelOption::TextToMidi => {
                        let length = match self
                            .generate
                            .generate_midi_length_seconds_input
                            .trim()
                            .parse::<f32>()
                        {
                            Ok(value) if value.is_finite() && value > 0.0 => value,
                            _ => {
                                self.warning("Length must be a positive number");
                                return Task::none();
                            }
                        };
                        (
                            maolan_generate::ModelChoice::TextToMidi,
                            length,
                            1024_usize,
                            0.98_f32,
                        )
                    }
                    crate::message::GenerateMidiModelOption::MidiLlm => {
                        let max_tokens = match self
                            .generate
                            .generate_midi_max_tokens_input
                            .trim()
                            .parse::<usize>()
                        {
                            Ok(value) if value > 0 => value,
                            _ => {
                                self.warning("Max tokens must be a positive whole number");
                                return Task::none();
                            }
                        };
                        let top_p = match self
                            .generate
                            .generate_midi_top_p_input
                            .trim()
                            .parse::<f32>()
                        {
                            Ok(value) if value.is_finite() && (0.0..=1.0).contains(&value) => value,
                            _ => {
                                self.warning("Top-p must be a number between 0 and 1");
                                return Task::none();
                            }
                        };
                        (
                            maolan_generate::ModelChoice::MidiLlm,
                            10.0_f32,
                            max_tokens,
                            top_p,
                        )
                    }
                };

                let musical_key = crate::state::MusicalKey {
                    root: self.generate.generate_midi_key_root,
                    mode: self.generate.generate_midi_key_mode,
                };
                {
                    let mut state = self.state.write().expect("state lock poisoned");
                    if state.musical_key != musical_key {
                        state.musical_key = musical_key;
                        self.session_ops.has_unsaved_changes = true;
                    }
                }

                let backend = match self.generate.generate_midi_backend {
                    crate::message::BurnBackendOption::Cpu => maolan_generate::BackendChoice::Cpu,
                    crate::message::BurnBackendOption::Vulkan => {
                        maolan_generate::BackendChoice::Vulkan
                    }
                };

                let output_stem = Maolan::sanitize_generated_track_base_name(&prompt);
                let output_rel = match Maolan::unique_import_rel_path(
                    &session_root,
                    "midi",
                    &output_stem,
                    "mid",
                ) {
                    Ok(rel) => rel,
                    Err(err) => {
                        self.error(format!("Failed to prepare generated output path: {err}"));
                        return Task::none();
                    }
                };
                let output_path = session_root.join(output_rel);

                let request = maolan_generate::GenerateRequest {
                    model,
                    prompt: prompt.clone(),
                    output_path,
                    model_dir: None,
                    inspect_only: false,
                    backend,
                    cfg_scale: maolan_generate::DEFAULT_CFG_SCALE,
                    length: 6_000,
                    ode_steps: 10,
                    lyrics: None,
                    tags: None,
                    topk: 50,
                    temperature: 1.0,
                    decode_only: false,
                    frames_json: None,
                    decode_threads: None,
                    decoder_seed: 0,
                    bpm: Some(bpm),
                    key_scale: Some(musical_key.to_string()),
                    time_signature: Some(format!(
                        "{}/{}",
                        time_signature_num.max(1),
                        time_signature_denom.max(1)
                    )),
                    acestep_lm: maolan_generate::AceStepLmSize::default(),
                    midi_length_seconds,
                    midi_seed,
                    midi_max_tokens,
                    midi_top_p,
                    output_path_explicit: true,
                };

                self.generate.generate_midi_in_progress = true;
                self.generate.generate_midi_progress = 0.0;
                self.generate.generate_midi_operation = Some("Launching generate".to_string());

                let (_pid, socket) = match Maolan::spawn_generate_process(&request) {
                    Ok((_pid, socket)) => (_pid, socket),
                    Err(err) => {
                        self.generate.generate_midi_in_progress = false;
                        self.error(err);
                        return Task::none();
                    }
                };
                #[cfg(unix)]
                {
                    self.generate.generate_midi_process_id = Some(_pid);
                }

                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                #[cfg(unix)]
                let playback_rate = self.transport.playback_rate_hz;
                let task_handle = tokio::spawn(async move {
                    let _ = tx.send(Message::GenerateMidiProgress {
                        progress: 0.05,
                        operation: Some("Generating MIDI".to_string()),
                    });

                    let tx_progress = tx.clone();
                    match tokio::task::spawn_blocking(move || {
                        Maolan::communicate_with_generate_process(
                            socket,
                            |phase, progress, operation| {
                                let _ = tx_progress.send(Message::GenerateMidiProgress {
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
                            let _ = tx.send(Message::GenerateMidiFinished(Err(err)));
                            return;
                        }
                        Err(err) => {
                            let _ = tx.send(Message::GenerateMidiFinished(Err(format!(
                                "generate task failed: {err}"
                            ))));
                            return;
                        }
                    };

                    #[cfg(unix)]
                    {
                        let mut used_names: HashSet<String> = HashSet::new();
                        let base_name = Maolan::sanitize_generated_track_base_name(&prompt);
                        let output_path = request.output_path.clone();
                        let session_root = session_root.clone();
                        let tx_clone = tx.clone();

                        let import_result = tokio::task::spawn_blocking(move || {
                            Maolan::import_midi_to_session(
                                &output_path,
                                &session_root,
                                playback_rate,
                            )
                        })
                        .await;

                        let (clip_rel, length) = match import_result {
                            Ok(Ok(result)) => result,
                            Ok(Err(err)) => {
                                let _ = tx.send(Message::GenerateMidiFinished(Err(format!(
                                    "Failed to import generated MIDI: {err}"
                                ))));
                                return;
                            }
                            Err(err) => {
                                let _ = tx.send(Message::GenerateMidiFinished(Err(format!(
                                    "MIDI import task failed: {err}"
                                ))));
                                return;
                            }
                        };

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
                            let _ = tx_clone.send(Message::GenerateMidiProgress {
                                progress: adjusted,
                                operation,
                            });
                        };

                        progress_fn(0.95, Some("Importing MIDI".to_string()));

                        let track_name = Maolan::unique_track_name(&base_name, &mut used_names);

                        if let Err(err) = CLIENT
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
                            let _ = tx.send(Message::GenerateMidiFinished(Err(format!(
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
                            let _ = tx.send(Message::GenerateMidiFinished(Err(format!(
                                "Failed to add generated clip: {err}"
                            ))));
                            return;
                        }

                        progress_fn(1.0, Some("Complete".to_string()));
                        let _ = tx.send(Message::GenerateMidiFinished(Ok(track_name)));
                    }
                    #[cfg(not(unix))]
                    {
                        let _ = tx.send(Message::GenerateMidiFinished(Err(
                            "Generated MIDI is only available on Unix platforms".to_string(),
                        )));
                    }
                });

                self.generate.generate_midi_abort_handle = Some(task_handle.abort_handle());

                return Task::run(
                    maolan_widgets::iced::futures::stream::unfold(rx, |mut rx| async move {
                        rx.recv().await.map(|msg| (msg, rx))
                    }),
                    |msg| msg,
                );
            }
            Message::GenerateMidiProgress {
                progress,
                ref operation,
            } => {
                self.generate.generate_midi_progress = progress.clamp(0.0, 1.0);
                self.generate.generate_midi_operation = operation.clone();
                if let Some(operation) = operation {
                    self.state.write().expect("state lock poisoned").message = operation.clone();
                }
            }
            Message::GenerateMidiFinished(ref result) => {
                self.generate.generate_midi_in_progress = false;
                self.generate.generate_midi_progress = if result.is_ok() { 1.0 } else { 0.0 };
                self.generate.generate_midi_operation = None;
                self.generate.generate_midi_abort_handle = None;
                #[cfg(unix)]
                {
                    self.generate.generate_midi_process_id = None;
                }
                match result {
                    Ok(track_name) => {
                        self.modal = None;
                        self.info(format!("Generated MIDI imported to track '{track_name}'"));
                    }
                    Err(err) => {
                        self.error(err.to_string());
                    }
                }
            }
            _ => {}
        }
        self.update_children(&message);
        Task::none()
    }
}
