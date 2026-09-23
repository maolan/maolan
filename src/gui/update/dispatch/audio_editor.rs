use super::*;

impl Maolan {
    pub(super) fn handle_audio_editor_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::AudioEditor(maolan_editor::app::Message::Close) => {
                let task = maolan_editor::app::update(
                    &mut self.audio_editor,
                    maolan_editor::app::Message::Close,
                )
                .map(Message::AudioEditor);
                self.audio_editor_clip = None;
                self.state.write().expect("state lock poisoned").view = View::Workspace;
                return self.stop_audio_editor_engine_preview().chain(task);
            }
            Message::AudioEditor(
                maolan_editor::app::Message::Save | maolan_editor::app::Message::SaveAs,
            ) => {
                self.info("Audio editor changes are non-destructive in Maolan; save the session to keep them.");
                return Task::none();
            }
            Message::AudioEditor(maolan_editor::app::Message::Play) => {
                if (self.transport.playing && !self.transport.paused)
                    || self.transport.live_session_playing
                {
                    return Task::none();
                }
                let editor_task = maolan_editor::app::update(
                    &mut self.audio_editor,
                    maolan_editor::app::Message::Play,
                )
                .map(Message::AudioEditor);
                let preview_task = self.start_audio_editor_engine_preview();
                if self.transport.playing {
                    return preview_task.chain(editor_task);
                }
                return self
                    .handle_transport_message(Message::TransportPause)
                    .chain(preview_task)
                    .chain(editor_task);
            }
            Message::AudioEditor(maolan_editor::app::Message::TogglePlayback) => {
                if (self.transport.playing && !self.transport.paused)
                    || self.transport.live_session_playing
                {
                    return Task::none();
                }
                let was_editor_playing = maolan_editor::app::host_preview(&self.audio_editor)
                    .is_some_and(|_| maolan_editor::app::is_playing(&self.audio_editor));
                let editor_task = maolan_editor::app::update(
                    &mut self.audio_editor,
                    maolan_editor::app::Message::TogglePlayback,
                )
                .map(Message::AudioEditor);
                let preview_task = if was_editor_playing {
                    self.stop_audio_editor_engine_preview()
                } else {
                    self.start_audio_editor_engine_preview()
                };
                if was_editor_playing {
                    return preview_task.chain(editor_task);
                }
                if self.transport.playing {
                    return preview_task.chain(editor_task);
                }
                return self
                    .handle_transport_message(Message::TransportPause)
                    .chain(preview_task)
                    .chain(editor_task);
            }
            Message::AudioEditor(maolan_editor::app::Message::Stop) => {
                let editor_task = maolan_editor::app::update(
                    &mut self.audio_editor,
                    maolan_editor::app::Message::Stop,
                )
                .map(Message::AudioEditor);
                return self.stop_audio_editor_engine_preview().chain(editor_task);
            }
            Message::AudioEditor(msg) => {
                if let Some(action) =
                    maolan_editor::app::audio_edit_action_for_message(&self.audio_editor, &msg)
                {
                    return self.apply_audio_editor_action(action);
                }
                return maolan_editor::app::update(&mut self.audio_editor, msg)
                    .map(Message::AudioEditor);
            }
            Message::OpenAudioEditor {
                ref track_idx,
                clip_idx,
            } => {
                let session_dir = self.session_dir.clone();
                let clip_request = {
                    let mut state = self.state.write().expect("state lock poisoned");
                    state.view = View::AudioEditor;
                    state
                        .tracks
                        .iter()
                        .find(|track| track.name.as_str() == track_idx.as_str())
                        .and_then(|track| track.audio.clips.get(clip_idx))
                        .map(|clip| {
                            let path = std::path::PathBuf::from(&clip.name);
                            let path = if path.is_absolute() {
                                Some(path)
                            } else {
                                session_dir
                                    .as_ref()
                                    .map(|session_root| session_root.join(path))
                            };
                            (path, clip.offset, clip.length, clip.start)
                        })
                };

                if let Some((Some(path), offset, length, _timeline_start)) = clip_request {
                    let track_idx = track_idx.clone();
                    return Task::perform(
                        async move {
                            let result = tokio::task::spawn_blocking(move || {
                                audio_editor_buffer_from_clip(&path, offset, length)
                            })
                            .await
                            .unwrap_or_else(|e| Err(format!("Decode task failed: {e}")));
                            Message::AudioEditorBufferLoaded {
                                track_idx,
                                clip_idx,
                                result,
                            }
                        },
                        |message| message,
                    );
                }

                let mut state = self.state.write().expect("state lock poisoned");
                state.message = format!("Could not open audio clip {clip_idx} on {track_idx}.");
            }
            Message::AudioEditorBufferLoaded {
                ref track_idx,
                clip_idx,
                ref result,
            } => match result {
                Ok(audio) => {
                    self.audio_editor_clip = Some(crate::gui::AudioEditorClipContext {
                        track_name: track_idx.clone(),
                        clip_idx,
                    });
                    maolan_editor::app::set_embedded_transport(&mut self.audio_editor, false, 0);
                    return maolan_editor::app::open_audio(&mut self.audio_editor, audio.clone())
                        .map(Message::AudioEditor);
                }
                Err(err) => {
                    let mut state = self.state.write().expect("state lock poisoned");
                    state.message = err.clone();
                    return Task::none();
                }
            },
            _ => {}
        }
        self.update_children(&message);
        Task::none()
    }
}
