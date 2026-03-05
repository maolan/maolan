use super::{CLIENT, MIN_CLIP_WIDTH_PX, Maolan, platform};
#[cfg(any(target_os = "windows", target_os = "macos"))]
use crate::message::PluginFormat;
use crate::{
    connections,
    message::{ExportNormalizeMode, Message, Show},
    state::{ConnectionViewSelection, HW, PianoData, PianoSysExPoint, Resizing, Track, View},
    ui_timing::DOUBLE_CLICK,
    widget::piano::{CTRL_SCROLL_ID, H_SCROLL_ID, KEYS_SCROLL_ID, NOTES_SCROLL_ID, V_SCROLL_ID},
    workspace::{
        EDITOR_H_SCROLL_ID, EDITOR_SCROLL_ID, PIANO_RULER_SCROLL_ID, PIANO_TEMPO_SCROLL_ID,
    },
};
use iced::widget::{Id, operation};
use iced::{Length, Point, Task, mouse};
#[cfg(any(target_os = "windows", all(unix, not(target_os = "macos"))))]
use maolan_engine::message::PluginGraphNode;
use maolan_engine::{
    kind::Kind,
    message::{Action, ClipMoveFrom, ClipMoveTo, Message as EngineMessage},
};
use rfd::AsyncFileDialog;
use std::{
    collections::{HashMap, HashSet},
    process::exit,
    time::Instant,
};
use tracing::error;

impl Maolan {
    fn format_sysex_hex(data: &[u8]) -> String {
        data.iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn parse_sysex_hex(raw: &str) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        for token in raw
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|s| !s.is_empty())
        {
            let normalized = token
                .strip_prefix("0x")
                .or_else(|| token.strip_prefix("0X"))
                .unwrap_or(token);
            let byte = u8::from_str_radix(normalized, 16)
                .map_err(|_| format!("Invalid hex byte '{token}'"))?;
            out.push(byte);
        }
        if out.is_empty() {
            return Err("SysEx payload is empty".to_string());
        }
        if !matches!(out.first(), Some(0xF0) | Some(0xF7)) {
            out.insert(0, 0xF0);
        }
        if out.first() == Some(&0xF0) && out.last() != Some(&0xF7) {
            out.push(0xF7);
        }
        Ok(out)
    }

    fn sysex_to_engine(points: &[PianoSysExPoint]) -> Vec<maolan_engine::message::MidiRawEventData> {
        points
            .iter()
            .map(|p| maolan_engine::message::MidiRawEventData {
                sample: p.sample,
                data: p.data.clone(),
            })
            .collect()
    }

    fn sync_editor_scrollbars(&self) -> Task<Message> {
        let x = self.editor_scroll_x.clamp(0.0, 1.0);
        Task::batch(vec![
            operation::snap_to(
                Id::new(EDITOR_SCROLL_ID),
                operation::RelativeOffset {
                    x: Some(x),
                    y: None,
                },
            ),
            operation::snap_to(
                Id::new(EDITOR_H_SCROLL_ID),
                operation::RelativeOffset {
                    x: Some(x),
                    y: None,
                },
            ),
        ])
    }

    fn sync_piano_scrollbars(&self) -> Task<Message> {
        let (x, y) = {
            let state = self.state.blocking_read();
            (
                state.piano_scroll_x.clamp(0.0, 1.0),
                state.piano_scroll_y.clamp(0.0, 1.0),
            )
        };
        Task::batch(vec![
            operation::snap_to(
                Id::new(NOTES_SCROLL_ID),
                operation::RelativeOffset {
                    x: Some(x),
                    y: Some(y),
                },
            ),
            operation::snap_to(
                Id::new(KEYS_SCROLL_ID),
                operation::RelativeOffset {
                    x: None,
                    y: Some(y),
                },
            ),
            operation::snap_to(
                Id::new(CTRL_SCROLL_ID),
                operation::RelativeOffset {
                    x: Some(x),
                    y: None,
                },
            ),
            operation::snap_to(
                Id::new(PIANO_TEMPO_SCROLL_ID),
                operation::RelativeOffset {
                    x: Some(x),
                    y: None,
                },
            ),
            operation::snap_to(
                Id::new(PIANO_RULER_SCROLL_ID),
                operation::RelativeOffset {
                    x: Some(x),
                    y: None,
                },
            ),
            operation::snap_to(
                Id::new(H_SCROLL_ID),
                operation::RelativeOffset {
                    x: Some(x),
                    y: None,
                },
            ),
            operation::snap_to(
                Id::new(V_SCROLL_ID),
                operation::RelativeOffset {
                    x: None,
                    y: Some(y),
                },
            ),
        ])
    }

    fn normalize_period_frames(period_frames: usize) -> usize {
        let v = period_frames.clamp(64, 8192);
        if v.is_power_of_two() {
            v
        } else {
            v.next_power_of_two().min(8192)
        }
    }

    fn midi_lane_at_position(&self, position: Point) -> Option<(String, usize)> {
        let state = self.state.blocking_read();
        let mut y_offset = 0.0f32;
        for track in &state.tracks {
            let track_top = y_offset;
            let track_bottom = y_offset + track.height;
            if position.y < track_top || position.y > track_bottom {
                y_offset += track.height;
                continue;
            }
            if track.midi.ins == 0 {
                return None;
            }
            let local_y = (position.y - y_offset).max(0.0);
            let layout = track.lane_layout();
            let midi_top = track.lane_top(Kind::MIDI, 0);
            let midi_bottom =
                track.lane_top(Kind::MIDI, track.midi.ins.saturating_sub(1)) + layout.lane_height;
            if local_y < midi_top || local_y > midi_bottom {
                return None;
            }
            let lane = track
                .lane_index_at_y(Kind::MIDI, local_y)
                .min(track.midi.ins.saturating_sub(1));
            return Some((track.name.clone(), lane));
        }
        None
    }

    fn create_empty_midi_clip_from_drag(&mut self, start: Point, end: Point) -> Task<Message> {
        let Some((track_name, input_channel)) = self.midi_lane_at_position(start) else {
            return Task::none();
        };
        let Some(session_root) = self.session_dir.clone() else {
            self.state.blocking_write().message =
                "Creating MIDI clips requires an opened/saved session".to_string();
            return Task::none();
        };

        let pps = self.pixels_per_sample().max(1.0e-6);
        let x0 = start.x.min(end.x).max(0.0);
        let x1 = start.x.max(end.x).max(0.0);
        let start_sample = self.snap_sample_to_bar(x0 / pps);
        let mut end_sample = self.snap_sample_to_bar(x1 / pps);
        let min_len = self.snap_interval_samples().max(1);
        if end_sample <= start_sample {
            end_sample = start_sample.saturating_add(min_len);
        }
        let length = end_sample.saturating_sub(start_sample).max(min_len);

        let clip_name = match self.create_empty_midi_clip_file(&track_name, &session_root) {
            Ok(name) => name,
            Err(e) => {
                self.state.blocking_write().message = format!("Failed to create MIDI clip: {e}");
                return Task::none();
            }
        };

        self.send(Action::AddClip {
            name: clip_name,
            track_name,
            start: start_sample,
            length,
            offset: 0,
            input_channel,
            kind: Kind::MIDI,
            fade_enabled: true,
            fade_in_samples: 240,
            fade_out_samples: 240,
        })
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::None => {
                return Task::none();
            }
            Message::Undo => {
                return self.send(Action::Undo);
            }
            Message::Redo => {
                return self.send(Action::Redo);
            }
            Message::ToggleTransport => {
                if !self.state.blocking_read().hw_loaded {
                    return Task::none();
                }
                if self.playing && !self.paused {
                    self.toolbar.update(message.clone());
                    self.playing = false;
                    self.paused = false;
                    self.last_playback_tick = None;
                    self.stop_recording_preview();
                    return Task::batch(vec![
                        self.send(Action::SetClipPlaybackEnabled(true)),
                        self.send(Action::Stop),
                    ]);
                }
                let was_playing = self.playing;
                self.toolbar.update(message.clone());
                self.playing = true;
                self.paused = false;
                self.last_playback_tick = Some(Instant::now());
                if self.record_armed {
                    self.start_recording_preview();
                }
                let mut tasks = vec![self.send(Action::SetClipPlaybackEnabled(true))];
                if !was_playing {
                    tasks.push(self.send(Action::Play));
                }
                return Task::batch(tasks);
            }
            Message::ToggleLoop => {
                if self.loop_range_samples.is_none() {
                    return Task::none();
                }
                let enabled = !self.loop_enabled;
                self.loop_enabled = enabled;
                return self.send(Action::SetLoopEnabled(enabled));
            }
            Message::TogglePunch => {
                if self.punch_range_samples.is_none() {
                    return Task::none();
                }
                let enabled = !self.punch_enabled;
                self.punch_enabled = enabled;
                return self.send(Action::SetPunchEnabled(enabled));
            }
            Message::WindowResized(size) => {
                self.size = size;
                return self.sync_editor_scrollbars();
            }
            Message::WindowCloseRequested => {
                exit(0);
            }
            Message::Show(ref show) => {
                use crate::message::Show;
                if !self.state.blocking_read().hw_loaded
                    && matches!(
                        show,
                        Show::Save | Show::SaveAs | Show::SaveTemplateAs | Show::Open
                    )
                {
                    return Task::none();
                }
                {
                    let mut state = self.state.blocking_write();
                    state.ctrl = false;
                    state.shift = false;
                }
                match show {
                    Show::Save => {
                        if let Some(path) = &self.session_dir {
                            return self
                                .refresh_graphs_then_save(path.to_string_lossy().to_string());
                        }
                        return Task::perform(
                            async {
                                AsyncFileDialog::new()
                                    .set_title("Select folder to save session")
                                    .set_directory("/tmp")
                                    .pick_folder()
                                    .await
                                    .map(|handle| handle.path().to_path_buf())
                            },
                            Message::SaveFolderSelected,
                        );
                    }
                    Show::SaveAs => {
                        return Task::perform(
                            async {
                                AsyncFileDialog::new()
                                    .set_title("Select folder to save session")
                                    .set_directory("/tmp")
                                    .pick_folder()
                                    .await
                                    .map(|handle| handle.path().to_path_buf())
                            },
                            Message::SaveFolderSelected,
                        );
                    }
                    Show::SaveTemplateAs => {
                        self.state.blocking_write().template_save_dialog =
                            Some(crate::state::TemplateSaveDialog {
                                name: String::new(),
                            });
                        self.modal = Some(Show::SaveTemplateAs);
                    }
                    Show::Open => {
                        return Task::perform(
                            async {
                                AsyncFileDialog::new()
                                    .set_title("Select folder to open session")
                                    .set_directory("/tmp")
                                    .pick_folder()
                                    .await
                                    .map(|handle| handle.path().to_path_buf())
                            },
                            Message::OpenFolderSelected,
                        );
                    }
                    Show::AddTrack => {
                        self.modal = Some(Show::AddTrack);
                        // Scan and update track templates
                        let track_templates = crate::gui::scan_track_templates();
                        self.add_track.set_available_templates(track_templates);
                    }
                    Show::TrackPluginList => {
                        self.modal = Some(Show::TrackPluginList);
                        #[cfg(all(unix, not(target_os = "macos")))]
                        self.selected_lv2_plugins.clear();
                        self.selected_vst3_plugins.clear();
                        self.selected_clap_plugins.clear();
                    }
                    Show::ExportSettings => {
                        self.modal = Some(Show::ExportSettings);
                    }
                }
            }
            Message::AddTrackFromTemplate {
                ref name,
                ref template,
                audio_ins,
                midi_ins,
                audio_outs,
                midi_outs,
            } => {
                // First create the track
                let task = self.send(Action::AddTrack {
                    name: name.clone(),
                    audio_ins: audio_ins,
                    midi_ins: midi_ins,
                    audio_outs: audio_outs,
                    midi_outs: midi_outs,
                });

                // Store pending template load
                self.state.blocking_write().pending_track_template_load =
                    Some((name.clone(), template.clone()));

                self.modal = None;
                return task;
            }
            Message::NewFromTemplate(ref template_name) => {
                // Load template from ~/.config/maolan/session_templates/<template_name>
                let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
                let template_path = format!(
                    "{}/.config/maolan/session_templates/{}",
                    home, template_name
                );

                match self.load(template_path.clone()) {
                    Ok(task) => return task,
                    Err(e) => {
                        error!(
                            "Failed to load template '{}' from {}: {}",
                            template_name, template_path, e
                        );
                        self.state.blocking_write().message =
                            format!("Failed to load template: {}", e);
                    }
                }
            }
            Message::NewSession => {
                if !self.state.blocking_read().hw_loaded {
                    return Task::none();
                }
                self.playing = false;
                self.paused = false;
                self.transport_samples = 0.0;
                self.loop_enabled = false;
                self.loop_range_samples = None;
                self.punch_enabled = false;
                self.punch_range_samples = None;
                self.last_playback_tick = None;
                self.record_armed = false;
                self.pending_record_after_save = false;
                self.pending_save_path = None;
                self.pending_save_tracks.clear();
                self.pending_audio_peaks.clear();
                self.session_dir = None;
                self.stop_recording_preview();

                let existing_tracks: Vec<String> = self
                    .state
                    .blocking_read()
                    .tracks
                    .iter()
                    .map(|t| t.name.clone())
                    .collect();
                let mut tasks = vec![
                    self.send(Action::Stop),
                    self.send(Action::SetRecordEnabled(false)),
                    self.send(Action::SetLoopRange(None)),
                    self.send(Action::SetPunchRange(None)),
                ];
                for name in existing_tracks {
                    tasks.push(self.send(Action::RemoveTrack(name)));
                }
                {
                    let mut state = self.state.blocking_write();
                    state.connections.clear();
                    state.selected.clear();
                    state.selected_clips.clear();
                    state.connection_view_selection = ConnectionViewSelection::None;
                    state.plugin_graph_track = None;
                    #[cfg(all(unix, not(target_os = "macos")))]
                    {
                        state.plugin_graph_plugins.clear();
                        state.plugin_graph_connections.clear();
                        state.plugin_graphs_by_track.clear();
                    }
                    state.clap_plugins_by_track.clear();
                    state.clap_states_by_track.clear();
                    state.message = "New session".to_string();
                    state.piano = None;
                }
                return Task::batch(tasks);
            }
            Message::Cancel => self.modal = None,
            Message::Request(ref a) => return self.send(a.clone()),
            Message::TransportPlay => {
                self.toolbar.update(message.clone());
                let was_playing = self.playing;
                self.playing = true;
                self.paused = false;
                self.last_playback_tick = Some(Instant::now());
                if self.record_armed {
                    self.start_recording_preview();
                }
                let mut tasks = vec![self.send(Action::SetClipPlaybackEnabled(true))];
                if !was_playing {
                    tasks.push(self.send(Action::Play));
                }
                return Task::batch(tasks);
            }
            Message::TransportPause => {
                self.toolbar.update(message.clone());
                let was_playing = self.playing;
                self.playing = true;
                self.paused = true;
                self.last_playback_tick = None;
                self.stop_recording_preview();
                let mut tasks = vec![self.send(Action::SetClipPlaybackEnabled(false))];
                if !was_playing {
                    tasks.push(self.send(Action::Play));
                }
                return Task::batch(tasks);
            }
            Message::TransportStop => {
                self.toolbar.update(message.clone());
                self.playing = false;
                self.paused = false;
                self.last_playback_tick = None;
                self.stop_recording_preview();
                return Task::batch(vec![
                    self.send(Action::SetClipPlaybackEnabled(true)),
                    self.send(Action::Stop),
                ]);
            }
            Message::JumpToStart => {
                self.transport_samples = 0.0;
                return self.send(Action::TransportPosition(0));
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
                return self.send(Action::TransportPosition(end_sample));
            }
            Message::PlaybackTick => {
                if self.playing
                    && !self.paused
                    && let Some(last) = self.last_playback_tick
                {
                    let now = Instant::now();
                    let delta_s = now.duration_since(last).as_secs_f64();
                    self.last_playback_tick = Some(now);
                    self.transport_samples += delta_s * self.playback_rate_hz;
                }
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
                return self.send(Action::SetLoopRange(normalized));
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
                return self.send(Action::SetPunchRange(normalized));
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
                    let tracks = self.state.blocking_read().tracks.clone();
                    for track in tracks.iter().filter(|t| t.armed) {
                        let channels = track.audio.outs.max(1);
                        let entry = self
                            .recording_preview_peaks
                            .entry(track.name.clone())
                            .or_insert_with(|| vec![vec![]; channels]);
                        if entry.len() != channels {
                            entry.resize_with(channels, Vec::new);
                        }
                        for (channel_idx, channel_entry) in
                            entry.iter_mut().enumerate().take(channels)
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
                            channel_entry.push(amp);
                        }
                    }
                }
            }
            Message::ZoomVisibleBarsChanged(value) => {
                self.zoom_visible_bars = value.clamp(1.0, 256.0);
                return self.sync_editor_scrollbars();
            }
            Message::EditorScrollXChanged(value) => {
                let x = value.clamp(0.0, 1.0);
                if (self.editor_scroll_x - x).abs() > 0.0005 {
                    self.editor_scroll_x = x;
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
                if matches!(lane, crate::message::PianoControllerLane::SysEx) {
                    state.piano_sysex_panel_open = true;
                } else {
                    state.piano_sysex_panel_open = false;
                }
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
                if !state.piano_selected_notes.is_empty() {
                    if let Some(piano) = state.piano.as_ref() {
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
                    let row_h = ((14.0 * 7.0 / 12.0) * zoom_y).max(1.0);
                    let tracks_width = match state.tracks_width {
                        Length::Fixed(v) => v,
                        _ => 200.0,
                    };
                    let editor_width = (self.size.width - tracks_width - 3.0).max(1.0);
                    let total_samples =
                        (self.samples_per_bar() * self.zoom_visible_bars as f64).max(1.0);
                    let pps = ((editor_width as f64 / total_samples) as f32 * zoom_x).max(1.0e-6);

                    let delta_x = dragging.current_point.x - dragging.start_point.x;
                    let delta_y = dragging.current_point.y - dragging.start_point.y;

                    let delta_samples = (delta_x / pps) as i64;
                    let delta_pitch = -(delta_y / row_h).round() as i8;

                    if copy && let Some(piano) = state.piano.as_ref() {
                        let track_name = piano.track_idx.clone();
                        let clip_idx = 0; // TODO: Get actual clip index
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
                        let clip_idx = 0; // TODO: Get actual clip index

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
                    let tracks_width = match state.tracks_width {
                        Length::Fixed(v) => v,
                        _ => 200.0,
                    };
                    let editor_width = (self.size.width - tracks_width - 3.0).max(1.0);
                    let total_samples =
                        (self.samples_per_bar() * self.zoom_visible_bars as f64).max(1.0);
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
                        let clip_idx = 0; // TODO: Get actual clip index

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
                let selected_notes = state.piano_selected_notes.clone();
                let Some(piano) = state.piano.as_mut() else {
                    return Task::none();
                };
                if note_index >= piano.notes.len() {
                    return Task::none();
                }

                let mut target_indices: Vec<usize> =
                    if selected_notes.contains(&note_index) && selected_notes.len() > 1 {
                        selected_notes.iter().copied().collect()
                    } else {
                        vec![note_index]
                    };
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
                let clip_idx = 0; // TODO: Get actual clip index
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
                let clip_idx = 0; // TODO: Get actual clip index
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
                let clip_idx = 0; // TODO: Get actual clip index
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
                let clip_idx = 0; // TODO: Get actual clip index
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
                let clip_idx = 0; // TODO: Get actual clip index
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
                let old_sysexes = piano.sysexes.clone();
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
                let new_sysexes = piano.sysexes.clone();
                let new_hex = Self::format_sysex_hex(&piano.sysexes[new_index].data);
                state.piano_selected_sysex = Some(new_index);
                state.piano_sysex_hex_input = new_hex;
                drop(state);
                return self.send(Action::SetMidiSysExEvents {
                    track_name,
                    clip_index: 0,
                    new_sysex_events: Self::sysex_to_engine(&new_sysexes),
                    old_sysex_events: Self::sysex_to_engine(&old_sysexes),
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
                let old_sysexes = piano.sysexes.clone();
                piano.sysexes[selected_idx].data = payload;
                let new_hex = Self::format_sysex_hex(&piano.sysexes[selected_idx].data);
                let track_name = piano.track_idx.clone();
                let new_sysexes = piano.sysexes.clone();
                state.piano_sysex_hex_input = new_hex;
                drop(state);
                return self.send(Action::SetMidiSysExEvents {
                    track_name,
                    clip_index: 0,
                    new_sysex_events: Self::sysex_to_engine(&new_sysexes),
                    old_sysex_events: Self::sysex_to_engine(&old_sysexes),
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
                let old_sysexes = piano.sysexes.clone();
                piano.sysexes.remove(selected_idx);
                let (new_sel, new_hex) = if piano.sysexes.is_empty() {
                    (None, String::new())
                } else {
                    let idx = selected_idx.min(piano.sysexes.len().saturating_sub(1));
                    (Some(idx), Self::format_sysex_hex(&piano.sysexes[idx].data))
                };
                let track_name = piano.track_idx.clone();
                let new_sysexes = piano.sysexes.clone();
                state.piano_selected_sysex = new_sel;
                state.piano_sysex_hex_input = new_hex;
                drop(state);
                return self.send(Action::SetMidiSysExEvents {
                    track_name,
                    clip_index: 0,
                    new_sysex_events: Self::sysex_to_engine(&new_sysexes),
                    old_sysex_events: Self::sysex_to_engine(&old_sysexes),
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
                let old_sysexes = piano.sysexes.clone();
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
                let new_sysexes = piano.sysexes.clone();
                state.piano_selected_sysex = new_sel;
                state.piano_sysex_hex_input = new_hex;
                drop(state);
                return self.send(Action::SetMidiSysExEvents {
                    track_name,
                    clip_index: 0,
                    new_sysex_events: Self::sysex_to_engine(&new_sysexes),
                    old_sysex_events: Self::sysex_to_engine(&old_sysexes),
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
                    let (notes, zoom_x, zoom_y) = if let Some(piano) = state.piano.as_ref() {
                        (piano.notes.clone(), state.piano_zoom_x, state.piano_zoom_y)
                    } else {
                        return Task::none();
                    };

                    let row_h = ((14.0 * 7.0 / 12.0) * zoom_y).max(1.0);
                    let tracks_width = match state.tracks_width {
                        Length::Fixed(v) => v,
                        _ => 200.0,
                    };
                    let editor_width = (self.size.width - tracks_width - 3.0).max(1.0);
                    let total_samples =
                        (self.samples_per_bar() * self.zoom_visible_bars as f64).max(1.0);
                    let pps = ((editor_width as f64 / total_samples) as f32 * zoom_x).max(1.0e-6);

                    let min_x = start.x.min(position.x);
                    let max_x = start.x.max(position.x);
                    let min_y = start.y.min(position.y);
                    let max_y = start.y.max(position.y);

                    state.piano_selected_notes.clear();
                    for (idx, note) in notes.iter().enumerate() {
                        if note.pitch > 119 {
                            // PITCH_MAX
                            continue;
                        }
                        let y_idx = (119 - note.pitch) as usize;
                        let y = y_idx as f32 * row_h + 1.0;
                        let x = note.start_sample as f32 * pps;
                        let w = (note.length_samples as f32 * pps).max(2.0);
                        let h = (row_h - 2.0).max(2.0);

                        // Check if note intersects with selection rectangle
                        if x + w >= min_x && x <= max_x && y + h >= min_y && y <= max_y {
                            state.piano_selected_notes.insert(idx);
                        }
                    }
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
                let total_samples =
                    (self.samples_per_bar() * self.zoom_visible_bars as f64).max(1.0);
                let pps = ((editor_width as f64 / total_samples) as f32 * zoom_x).max(1.0e-6);

                let x0 = start.x.min(end.x).max(0.0);
                let x1 = start.x.max(end.x).max(0.0);
                let raw_start = (x0 / pps).floor().max(0.0) as usize;
                let raw_end = (x1 / pps).ceil().max(raw_start as f32 + 1.0) as usize;
                let start_sample = self.snap_sample_to_bar(raw_start as f32);
                let mut end_sample = self.snap_sample_to_bar(raw_end as f32);
                let min_len = self.snap_interval_samples().max(1);
                if end_sample <= start_sample {
                    end_sample = start_sample.saturating_add(min_len);
                }
                let length_samples = end_sample.saturating_sub(start_sample).max(min_len);

                let pitch_row = (start.y / row_h).floor();
                let pitch_row = pitch_row.clamp(0.0, 119.0) as usize;
                let pitch = 119_u8.saturating_sub(pitch_row as u8);

                if let Some(piano) = state.piano.as_ref() {
                    let track_name = piano.track_idx.clone();
                    let clip_idx = 0; // TODO: Get actual clip index
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

                if !selected_indices.is_empty() {
                    if let Some(piano) = state.piano.as_mut() {
                        let track_name = piano.track_idx.clone();
                        let clip_idx = 0; // TODO: Get actual clip index
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

                        let note_indices: Vec<usize> =
                            selected_indices.iter().rev().copied().collect();

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
            }
            Message::TracksResizeHover(hovered) => {
                self.tracks_resize_hovered = hovered;
            }
            Message::MixerResizeHover(hovered) => {
                self.mixer_resize_hovered = hovered;
            }
            Message::TransportRecordToggle => {
                self.toolbar.update(message.clone());
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
            #[cfg(all(unix, not(target_os = "macos")))]
            Message::RefreshLv2Plugins => return self.send(Action::ListLv2Plugins),
            Message::RefreshVst3Plugins => return self.send(Action::ListVst3Plugins),
            Message::RefreshClapPlugins => {
                if self.scan_clap_capabilities {
                    return self.send(Action::ListClapPluginsWithCapabilities);
                } else {
                    return self.send(Action::ListClapPlugins);
                }
            }
            Message::ToggleClapCapabilityScanning(enabled) => {
                self.scan_clap_capabilities = enabled;
                // Refresh plugins with new setting
                if self.scan_clap_capabilities {
                    return self.send(Action::ListClapPluginsWithCapabilities);
                } else {
                    return self.send(Action::ListClapPlugins);
                }
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            Message::FilterLv2Plugins(ref query) => {
                self.plugin_filter = query.clone();
            }
            Message::FilterVst3Plugins(ref query) => {
                self.vst3_plugin_filter = query.clone();
            }
            Message::FilterClapPlugin(ref query) => {
                self.clap_plugin_filter = query.clone();
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            Message::SelectLv2Plugin(ref plugin_uri) => {
                if self.selected_lv2_plugins.contains(plugin_uri) {
                    self.selected_lv2_plugins.remove(plugin_uri);
                } else {
                    self.selected_lv2_plugins.insert(plugin_uri.clone());
                }
            }
            Message::SelectVst3Plugin(ref plugin_path) => {
                if self.selected_vst3_plugins.contains(plugin_path) {
                    self.selected_vst3_plugins.remove(plugin_path);
                } else {
                    self.selected_vst3_plugins.insert(plugin_path.clone());
                }
            }
            Message::SelectClapPlugin(ref plugin_path) => {
                if self.selected_clap_plugins.contains(plugin_path) {
                    self.selected_clap_plugins.remove(plugin_path);
                } else {
                    self.selected_clap_plugins.insert(plugin_path.clone());
                }
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            Message::LoadSelectedLv2Plugins => {
                let track_name = {
                    let state = self.state.blocking_read();
                    state
                        .plugin_graph_track
                        .clone()
                        .or_else(|| state.selected.iter().next().cloned())
                };
                if let Some(track_name) = track_name {
                    let tasks: Vec<Task<Message>> = self
                        .selected_lv2_plugins
                        .iter()
                        .cloned()
                        .map(|plugin_uri| {
                            self.send(Action::TrackLoadLv2Plugin {
                                track_name: track_name.clone(),
                                plugin_uri,
                            })
                        })
                        .collect();
                    self.selected_lv2_plugins.clear();
                    self.modal = None;
                    return Task::batch(tasks);
                }
                self.state.blocking_write().message =
                    "Select a track before loading LV2 plugin".to_string();
            }
            Message::LoadSelectedVst3Plugins => {
                let track_name = {
                    let state = self.state.blocking_read();
                    state
                        .plugin_graph_track
                        .clone()
                        .or_else(|| state.selected.iter().next().cloned())
                };
                if let Some(track_name) = track_name {
                    let tasks: Vec<Task<Message>> = self
                        .selected_vst3_plugins
                        .iter()
                        .cloned()
                        .map(|plugin_path| {
                            self.send(Action::TrackLoadVst3Plugin {
                                track_name: track_name.clone(),
                                plugin_path,
                            })
                        })
                        .collect();
                    self.selected_vst3_plugins.clear();
                    self.modal = None;
                    return Task::batch(tasks);
                }
                self.state.blocking_write().message =
                    "Select a track before loading VST3 plugin".to_string();
            }
            Message::LoadSelectedClapPlugins => {
                let track_name = {
                    let state = self.state.blocking_read();
                    state
                        .plugin_graph_track
                        .clone()
                        .or_else(|| state.selected.iter().next().cloned())
                };
                if let Some(track_name) = track_name {
                    let tasks: Vec<Task<Message>> = self
                        .selected_clap_plugins
                        .iter()
                        .cloned()
                        .map(|plugin_path| {
                            self.send(Action::TrackLoadClapPlugin {
                                track_name: track_name.clone(),
                                plugin_path,
                            })
                        })
                        .collect();
                    self.selected_clap_plugins.clear();
                    self.modal = None;
                    return Task::batch(tasks);
                }
                self.state.blocking_write().message =
                    "Select a track before loading CLAP plugin".to_string();
            }
            Message::PluginFormatSelected(format) => {
                #[cfg(any(target_os = "windows", target_os = "macos"))]
                let format = if format == PluginFormat::Lv2 {
                    PluginFormat::Vst3
                } else {
                    format
                };
                self.plugin_format = format;
            }
            Message::UnloadClapPlugin(ref plugin_path) => {
                let track_name = {
                    let state = self.state.blocking_read();
                    state
                        .plugin_graph_track
                        .clone()
                        .or_else(|| state.selected.iter().next().cloned())
                };
                if let Some(track_name) = track_name {
                    return self.send(Action::TrackUnloadClapPlugin {
                        track_name,
                        plugin_path: plugin_path.clone(),
                    });
                }
                self.state.blocking_write().message =
                    "Select a track before unloading CLAP plugin".to_string();
            }
            Message::ShowClapPluginUi(ref plugin_path) => {
                if let Err(e) = self.clap_ui_host.open_editor(plugin_path) {
                    self.state.blocking_write().message = e;
                }
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            Message::OpenLv2PluginUi {
                ref track_name,
                instance_id,
            } => {
                return self.send(Action::TrackGetLv2PluginControls {
                    track_name: track_name.clone(),
                    instance_id,
                });
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            Message::PumpLv2Ui => {
                self.lv2_ui_host.pump();
            }
            Message::OpenVst3PluginUi {
                ref track_name,
                instance_id,
                ref plugin_path,
                ref plugin_name,
                ref plugin_id,
                audio_inputs,
                audio_outputs,
            } => {
                #[cfg(target_os = "windows")]
                {
                    let _ = (
                        plugin_path,
                        plugin_name,
                        plugin_id,
                        audio_inputs,
                        audio_outputs,
                    );
                    return self.send(Action::TrackOpenVst3Editor {
                        track_name: track_name.clone(),
                        instance_id,
                    });
                }

                #[cfg(not(target_os = "windows"))]
                {
                    let _ = (track_name, instance_id);
                    let (sample_rate_hz, block_size) = {
                        let st = self.state.blocking_read();
                        (self.playback_rate_hz.max(1.0), st.oss_period_frames.max(1))
                    };
                    if let Err(e) = self.vst3_ui_host.open_editor(
                        plugin_path,
                        plugin_name,
                        plugin_id,
                        sample_rate_hz,
                        block_size,
                        audio_inputs,
                        audio_outputs,
                        None,
                    ) {
                        self.state.blocking_write().message = e;
                    }
                }
            }
            Message::SendMessageFinished(Err(ref e)) => {
                error!("Error: {}", e);
            }
            Message::SendMessageFinished(Ok(())) => {}
            Message::Response(Ok(ref a)) => match a {
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
                        && let Some(track) = state.tracks.iter_mut().find(|t| &t.name == name)
                    {
                        track.position = position;
                    }
                    if let Some(height) = state.pending_track_heights.remove(name)
                        && let Some(track) = state.tracks.iter_mut().find(|t| &t.name == name)
                    {
                        track.height = height;
                    }

                    // Check if we need to load a template for this track
                    let pending_template = state.pending_track_template_load.clone();
                    drop(state);

                    if let Some((template_track_name, template_name)) = pending_template {
                        if template_track_name == *name {
                            self.state.blocking_write().pending_track_template_load = None;
                            return self.load_track_template(name.clone(), template_name);
                        }
                    }

                    self.modal = None;
                }
                Action::RemoveTrack(name) => {
                    let mut state = self.state.blocking_write();

                    if let Some(removed_idx) = state.tracks.iter().position(|t| t.name == *name) {
                        state
                            .connections
                            .retain(|conn| conn.from_track != *name && conn.to_track != *name);
                        state.tracks.remove(removed_idx);

                        state.selected.remove(name);
                        if let ConnectionViewSelection::Tracks(set) =
                            &mut state.connection_view_selection
                        {
                            set.remove(name);
                        }
                        state.clap_plugins_by_track.remove(name);
                        state.clap_states_by_track.remove(name);
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
                                        clip_to_move =
                                            Some(from_track.audio.clips.remove(from.clip_index));
                                    } else {
                                        clip_to_move =
                                            Some(from_track.audio.clips[from.clip_index].clone());
                                    }
                                }
                            }
                            Kind::MIDI => {
                                if from.clip_index < from_track.midi.clips.len() {
                                    if !copy {
                                        midi_clip_to_move =
                                            Some(from_track.midi.clips.remove(from.clip_index));
                                    } else {
                                        midi_clip_to_move =
                                            Some(from_track.midi.clips[from.clip_index].clone());
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
                }
                Action::AddClip {
                    name,
                    track_name,
                    start,
                    length,
                    offset,
                    input_channel,
                    kind,
                    fade_enabled,
                    fade_in_samples,
                    fade_out_samples,
                } => {
                    let mut audio_peaks = vec![];
                    let mut max_length_samples = offset.saturating_add(*length);
                    if *kind == Kind::Audio {
                        let key = Self::audio_clip_key(track_name, name, *start, *length, *offset);
                        audio_peaks = self.pending_audio_peaks.remove(&key).unwrap_or_default();
                        if name.to_ascii_lowercase().ends_with(".wav") {
                            let wav_path = if std::path::Path::new(name).is_absolute() {
                                Some(std::path::PathBuf::from(name))
                            } else {
                                self.session_dir
                                    .as_ref()
                                    .map(|session_root| session_root.join(name))
                            };
                            if let Some(wav_path) = wav_path {
                                if audio_peaks.is_empty()
                                    && wav_path.exists()
                                    && let Ok(computed) =
                                        Self::compute_audio_clip_peaks(&wav_path, 512)
                                {
                                    audio_peaks = computed;
                                }
                                if wav_path.exists()
                                    && let Ok(total_samples) =
                                        Self::audio_clip_source_length(&wav_path)
                                {
                                    max_length_samples =
                                        total_samples.saturating_sub(*offset).max(1);
                                }
                            }
                        }
                    }
                    let mut state = self.state.blocking_write();
                    if let Some(track) = state.tracks.iter_mut().find(|t| &t.name == track_name) {
                        match kind {
                            Kind::Audio => {
                                track.audio.clips.push(crate::state::AudioClip {
                                    name: name.clone(),
                                    start: *start,
                                    length: *length,
                                    offset: *offset,
                                    input_channel: *input_channel,
                                    max_length_samples,
                                    peaks_file: None,
                                    peaks: audio_peaks,
                                    fade_enabled: *fade_enabled,
                                    fade_in_samples: *fade_in_samples,
                                    fade_out_samples: *fade_out_samples,
                                });
                            }
                            Kind::MIDI => {
                                track.midi.clips.push(crate::state::MIDIClip {
                                    name: name.clone(),
                                    start: *start,
                                    length: *length,
                                    offset: *offset,
                                    input_channel: *input_channel,
                                    max_length_samples,
                                    fade_enabled: *fade_enabled,
                                    fade_in_samples: *fade_in_samples,
                                    fade_out_samples: *fade_out_samples,
                                });
                            }
                        }
                    }
                }
                Action::RemoveClip {
                    track_name,
                    kind,
                    clip_indices,
                } => {
                    let mut state = self.state.blocking_write();
                    if let Some(track) = state.tracks.iter_mut().find(|t| &t.name == track_name) {
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
                        for (note_idx, new_note) in note_indices.iter().zip(new_notes.iter()) {
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
                        let mut sorted = controllers.clone();
                        sorted.sort_unstable_by_key(|(idx, _)| *idx);
                        for (idx, ctrl) in sorted {
                            let insert_at = idx.min(piano.controllers.len());
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
                        let mut sorted_notes = notes.clone();
                        sorted_notes.sort_unstable_by_key(|(idx, _)| *idx);
                        for (idx, note) in sorted_notes {
                            let insert_at = idx.min(piano.notes.len());
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
                Action::Connect {
                    from_track,
                    from_port,
                    to_track,
                    to_port,
                    kind,
                } => {
                    let mut state = self.state.blocking_write();

                    state.connections.push(crate::state::Connection {
                        from_track: from_track.clone(),
                        from_port: *from_port,
                        to_track: to_track.clone(),
                        to_port: *to_port,
                        kind: *kind,
                    });
                }
                Action::Disconnect {
                    from_track,
                    from_port,
                    to_track,
                    to_port,
                    kind,
                } => {
                    let mut state = self.state.blocking_write();
                    let original_len = state.connections.len();

                    state.connections.retain(|conn| {
                        !(conn.from_track == from_track.as_str()
                            && conn.from_port == *from_port
                            && conn.to_track == to_track.as_str()
                            && conn.to_port == *to_port
                            && conn.kind == *kind)
                    });
                    if state.connections.len() < original_len {
                        state.message = format!("Disconnected {} from {}", from_track, to_track);
                    }
                }

                Action::OpenAudioDevice {
                    device,
                    bits,
                    exclusive,
                    period_frames,
                    nperiods,
                    sync_mode,
                } => {
                    let mut state = self.state.blocking_write();
                    state.message = format!(
                        "Opened device {} (bits={}, exclusive={}, period={}, nperiods={}, sync_mode={})",
                        device, bits, exclusive, period_frames, nperiods, sync_mode
                    );
                    state.hw_loaded = true;
                    state.oss_period_frames = (*period_frames).max(1);
                    state.oss_nperiods = (*nperiods).max(1);
                }
                Action::OpenMidiInputDevice(s) => {
                    let mut state = self.state.blocking_write();
                    if !state.opened_midi_in_hw.iter().any(|name| name == s) {
                        state.opened_midi_in_hw.push(s.clone());
                    }
                    state
                        .midi_hw_labels
                        .entry(s.clone())
                        .or_insert_with(|| platform::kernel_midi_label(s));
                    state.message = format!("Opened MIDI input {s}");
                }
                Action::OpenMidiOutputDevice(s) => {
                    let mut state = self.state.blocking_write();
                    if !state.opened_midi_out_hw.iter().any(|name| name == s) {
                        state.opened_midi_out_hw.push(s.clone());
                    }
                    state
                        .midi_hw_labels
                        .entry(s.clone())
                        .or_insert_with(|| platform::kernel_midi_label(s));
                    state.message = format!("Opened MIDI output {s}");
                }
                Action::HWInfo {
                    channels,
                    rate,
                    input,
                } => {
                    if *rate > 0 {
                        self.playback_rate_hz = *rate as f64;
                    }
                    let mut state = self.state.blocking_write();
                    if !state.hw_loaded {
                        state.hw_loaded = true;
                    }
                    let direction = if *input { "input" } else { "output" };
                    state.message = format!("HW {direction} channels: {channels} @ {rate} Hz");
                    if *input {
                        state.hw_in = Some(HW {
                            channels: *channels,
                        });
                    } else {
                        state.hw_out = Some(HW {
                            channels: *channels,
                        });
                        if state.hw_out_meter_db.len() != *channels {
                            state.hw_out_meter_db = vec![-90.0; *channels];
                        }
                    }
                }
                Action::TrackLevel(name, level) => {
                    if name == "hw:out" {
                        self.state.blocking_write().hw_out_level = *level;
                    }
                }
                Action::TrackBalance(name, balance) => {
                    if name == "hw:out" {
                        self.state.blocking_write().hw_out_balance = *balance;
                    }
                }
                Action::TrackToggleMute(name) => {
                    if name == "hw:out" {
                        let mut state = self.state.blocking_write();
                        state.hw_out_muted = !state.hw_out_muted;
                    }
                }
                Action::TrackMeters {
                    track_name,
                    output_db,
                } => {
                    if track_name == "hw:out" {
                        let mut state = self.state.blocking_write();
                        if state.hw_out_meter_db != *output_db {
                            state.hw_out_meter_db = output_db.clone();
                        }
                        return Task::none();
                    }
                    let mut state = self.state.blocking_write();
                    if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name)
                        && track.meter_out_db != *output_db
                    {
                        track.meter_out_db = output_db.clone();
                    }
                    return Task::none();
                }
                Action::SetSessionPath(_) => {
                    if self.pending_record_after_save {
                        self.pending_record_after_save = false;
                        return self.send(Action::SetRecordEnabled(true));
                    }
                }
                Action::TransportPosition(sample) => {
                    self.transport_samples = *sample as f64;
                    if self.playing && !self.paused {
                        self.last_playback_tick = Some(Instant::now());
                    }
                }
                Action::SetLoopEnabled(enabled) => {
                    self.loop_enabled = *enabled && self.loop_range_samples.is_some();
                }
                Action::SetLoopRange(range) => {
                    self.loop_range_samples = *range;
                    self.loop_enabled = range.is_some();
                }
                Action::SetPunchEnabled(enabled) => {
                    self.punch_enabled = *enabled && self.punch_range_samples.is_some();
                }
                Action::SetPunchRange(range) => {
                    self.punch_range_samples = *range;
                    self.punch_enabled = range.is_some();
                }
                #[cfg(all(unix, not(target_os = "macos")))]
                Action::Lv2Plugins(plugins) => {
                    let mut state = self.state.blocking_write();
                    state.lv2_plugins = plugins.clone();
                    state.lv2_plugins_loaded = true;
                    state.message = format!("Loaded {} LV2 plugins", state.lv2_plugins.len());
                }
                Action::Vst3Plugins(plugins) => {
                    let mut state = self.state.blocking_write();
                    state.vst3_plugins = plugins.clone();
                    state.vst3_plugins_loaded = true;
                    state.message = format!("Loaded {} VST3 plugins", state.vst3_plugins.len());
                }
                Action::ClapPlugins(plugins) => {
                    let mut state = self.state.blocking_write();
                    state.clap_plugins = plugins.clone();
                    state.clap_plugins_loaded = true;
                    state.message = format!("Loaded {} CLAP plugins", state.clap_plugins.len());
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
                    self.state.blocking_write().message =
                        format!("Loaded CLAP plugin '{plugin_name}' on track '{track_name}'");
                    #[cfg(any(target_os = "windows", all(unix, not(target_os = "macos"))))]
                    {
                        let plugin_track = self.state.blocking_read().plugin_graph_track.clone();
                        if plugin_track.as_deref() == Some(track_name.as_str()) {
                            return self.send(Action::TrackGetPluginGraph {
                                track_name: track_name.clone(),
                            });
                        }
                    }
                }
                Action::TrackUnloadClapPlugin {
                    track_name,
                    plugin_path,
                } => {
                    {
                        let mut state = self.state.blocking_write();
                        if let Some(entry) = state.clap_plugins_by_track.get_mut(track_name)
                            && let Some(pos) = entry
                                .iter()
                                .position(|existing| existing.eq_ignore_ascii_case(plugin_path))
                        {
                            entry.remove(pos);
                        }
                        if let Some(states) = state.clap_states_by_track.get_mut(track_name) {
                            states.remove(plugin_path);
                        }
                    }
                    let plugin_name = std::path::Path::new(plugin_path)
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| plugin_path.clone());
                    self.state.blocking_write().message =
                        format!("Unloaded CLAP plugin '{plugin_name}' from track '{track_name}'");
                    #[cfg(any(target_os = "windows", all(unix, not(target_os = "macos"))))]
                    {
                        let plugin_track = self.state.blocking_read().plugin_graph_track.clone();
                        if plugin_track.as_deref() == Some(track_name.as_str()) {
                            return self.send(Action::TrackGetPluginGraph {
                                track_name: track_name.clone(),
                            });
                        }
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
                #[cfg(any(target_os = "windows", target_os = "macos"))]
                Action::TrackSnapshotAllClapStates { track_name } => {
                    if self.pending_save_path.is_some() {
                        self.pending_save_tracks.remove(track_name);
                        if self.pending_save_tracks.is_empty() {
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
                                } else if let Err(e) = self.save(path.clone()) {
                                    error!("{}", e);
                                } else {
                                    return self.send(Action::SetSessionPath(path));
                                }
                            }
                        }
                    }
                }
                Action::TrackClearDefaultPassthrough { track_name } => {
                    let lv2_track = self.state.blocking_read().plugin_graph_track.clone();
                    #[cfg(any(target_os = "windows", all(unix, not(target_os = "macos"))))]
                    if lv2_track.as_deref() == Some(track_name.as_str()) {
                        return self.send(Action::TrackGetPluginGraph {
                            track_name: track_name.clone(),
                        });
                    }
                    let _ = (track_name, lv2_track);
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
                    let lv2_track = self.state.blocking_read().plugin_graph_track.clone();
                    if lv2_track.as_deref() == Some(track_name.as_str()) {
                        return self.send(Action::TrackGetPluginGraph {
                            track_name: track_name.clone(),
                        });
                    }
                }
                Action::TrackVst3StateSnapshot {
                    track_name,
                    instance_id,
                    state,
                } => {
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
                Action::TrackLoadVst3Plugin { track_name, .. }
                | Action::TrackUnloadVst3PluginInstance { track_name, .. }
                | Action::TrackConnectPluginAudio { track_name, .. }
                | Action::TrackDisconnectPluginAudio { track_name, .. }
                | Action::TrackConnectPluginMidi { track_name, .. }
                | Action::TrackDisconnectPluginMidi { track_name, .. } => {
                    let lv2_track = self.state.blocking_read().plugin_graph_track.clone();
                    if lv2_track.as_deref() == Some(track_name.as_str()) {
                        return self.send(Action::TrackGetPluginGraph {
                            track_name: track_name.clone(),
                        });
                    }
                }
                #[cfg(all(unix, not(target_os = "macos")))]
                Action::TrackLv2Midnam {
                    track_name,
                    note_names,
                } => {
                    let mut state = self.state.blocking_write();
                    if let Some(piano) = &mut state.piano {
                        if piano.track_idx == *track_name {
                            piano.midnam_note_names = note_names.clone();
                        }
                    }
                }
                #[cfg(all(unix, not(target_os = "macos")))]
                Action::TrackLv2PluginControls {
                    track_name,
                    instance_id,
                    controls,
                    instance_access_handle,
                } => {
                    let (plugin_name, plugin_uri) = {
                        let state = self.state.blocking_read();
                        state
                            .plugin_graph_plugins
                            .iter()
                            .find(|plugin| plugin.instance_id == *instance_id)
                            .map(|plugin| (plugin.name.clone(), plugin.uri.clone()))
                            .unwrap_or_else(|| (format!("LV2 #{instance_id}"), String::new()))
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
                    use tracing::info;
                    info!(
                        "Received plugin graph for track '{}' with {} plugins",
                        track_name,
                        plugins.len()
                    );
                    for (idx, plugin) in plugins.iter().enumerate() {
                        info!(
                            "  Plugin {}: uri={}, state properties count={}",
                            idx,
                            plugin.uri,
                            plugin
                                .state
                                .as_ref()
                                .map(|s| s.properties.len())
                                .unwrap_or(0)
                        );
                    }
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

                    if self.pending_save_path.is_some() {
                        self.pending_save_tracks.remove(track_name);
                        if self.pending_save_tracks.is_empty() {
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
                                        return self.save_track_as_template(track_name, path);
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
                    if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *old_name) {
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
                    if let Some(graph) = state.plugin_graphs_by_track.remove(old_name) {
                        state.plugin_graphs_by_track.insert(new_name.clone(), graph);
                    }
                    if let Some(clap) = state.clap_plugins_by_track.remove(old_name) {
                        state.clap_plugins_by_track.insert(new_name.clone(), clap);
                    }
                    if let Some(clap_states) = state.clap_states_by_track.remove(old_name) {
                        state
                            .clap_states_by_track
                            .insert(new_name.clone(), clap_states);
                    }
                    state.message = format!("Renamed track to '{}'", new_name);
                }
                _ => {}
            },
            Message::Response(Err(ref e)) => {
                self.state.blocking_write().message = e.clone();
                error!("Engine error: {e}");
            }
            Message::SaveFolderSelected(ref path_opt) => {
                {
                    let mut state = self.state.blocking_write();
                    state.ctrl = false;
                    state.shift = false;
                }
                if let Some(path) = path_opt {
                    self.session_dir = Some(path.clone());
                    return self.refresh_graphs_then_save(path.to_string_lossy().to_string());
                }
            }
            Message::RecordFolderSelected(ref path_opt) => {
                {
                    let mut state = self.state.blocking_write();
                    state.ctrl = false;
                    state.shift = false;
                }
                if let Some(path) = path_opt {
                    self.session_dir = Some(path.clone());
                    self.record_armed = true;
                    self.pending_record_after_save = true;
                    if self.playing {
                        self.start_recording_preview();
                    }
                    return self.refresh_graphs_then_save(path.to_string_lossy().to_string());
                } else {
                    self.pending_record_after_save = false;
                }
            }
            Message::OpenFolderSelected(Some(path)) => {
                {
                    let mut state = self.state.blocking_write();
                    state.ctrl = false;
                    state.shift = false;
                }
                self.session_dir = Some(path.clone());
                self.stop_recording_preview();
                match self.load(path.to_string_lossy().to_string()) {
                    Ok(task) => return task,
                    Err(e) => {
                        error!("{}", e);
                        return Task::none();
                    }
                }
            }
            Message::ShiftPressed => {
                if !self.state.blocking_read().hw_loaded {
                    return Task::none();
                }
                self.state.blocking_write().shift = true;
            }
            Message::ShiftReleased => {
                if !self.state.blocking_read().hw_loaded {
                    return Task::none();
                }
                self.state.blocking_write().shift = false;
            }
            Message::CtrlPressed => {
                if !self.state.blocking_read().hw_loaded {
                    return Task::none();
                }
                self.state.blocking_write().ctrl = true;
            }
            Message::CtrlReleased => {
                if !self.state.blocking_read().hw_loaded {
                    return Task::none();
                }
                self.state.blocking_write().ctrl = false;
            }
            Message::SelectTrack(ref name) => {
                let now = Instant::now();
                let track_name = name.clone();
                let ctrl = self.state.blocking_read().ctrl;
                let selected = self.state.blocking_read().selected.contains(name);
                let mut state = self.state.blocking_write();
                if ctrl {
                    state.connections_last_track_click = None;
                } else if let Some((last_track, last_time)) = &state.connections_last_track_click
                    && *last_track == track_name
                    && now.duration_since(*last_time) <= DOUBLE_CLICK.saturating_mul(2)
                {
                    state.connections_last_track_click = None;
                    return Task::perform(async {}, move |_| Message::OpenTrackPlugins(track_name));
                } else {
                    state.connections_last_track_click = Some((track_name.clone(), now));
                }

                if ctrl {
                    if selected {
                        state.selected.remove(name);
                        if let ConnectionViewSelection::Tracks(set) =
                            &mut state.connection_view_selection
                        {
                            set.remove(name);
                        }
                    } else {
                        state.selected.insert(name.clone());
                        if let ConnectionViewSelection::Tracks(set) =
                            &mut state.connection_view_selection
                        {
                            set.insert(name.clone());
                        }
                    }
                } else {
                    state.selected.clear();
                    state.selected.insert(name.clone());
                    let mut set = std::collections::HashSet::new();
                    set.insert(name.clone());
                    state.connection_view_selection = ConnectionViewSelection::Tracks(set);
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
                self.template_save.update(message.clone());
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
                self.track_template_save.update(message.clone());
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
                    let mut state = self.state.blocking_write();
                    match button {
                        mouse::Button::Left => {
                            state.mouse_left_down = true;
                            state.clip_marquee_start = None;
                            state.clip_marquee_end = None;
                        }
                        mouse::Button::Right => {
                            state.mouse_right_down = true;
                            state.midi_clip_create_start = None;
                            state.midi_clip_create_end = None;
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
                        Kind::Audio => track.audio.clips.get(clip_idx).map(|clip| {
                            if is_fade_out {
                                clip.fade_out_samples
                            } else {
                                clip.fade_in_samples
                            }
                        }),
                        Kind::MIDI => track.midi.clips.get(clip_idx).map(|clip| {
                            if is_fade_out {
                                clip.fade_out_samples
                            } else {
                                clip.fade_in_samples
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
                    && state.midi_clip_create_start.is_none()
                    && can_start_midi_drag
                {
                    state.midi_clip_create_start = Some(position);
                    state.midi_clip_create_end = Some(position);
                }
            }
            Message::MouseReleased => {
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
                    state.ctrl = false;
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
                if self.track.is_none() {
                    let state = self.state.blocking_read();
                    if index < state.tracks.len() {
                        self.track = Some(state.tracks[index].name.clone());
                    }
                }
            }
            Message::TrackDropped(point, _rect) => {
                if self.track.is_some() {
                    return iced_drop::zones_on_point(Message::HandleTrackZones, point, None, None);
                }
            }
            Message::HandleTrackZones(ref zones) => {
                if let Some(index_name) = &self.track
                    && let Some((track_id, _)) = zones.first()
                {
                    let mut state = self.state.blocking_write();
                    if let Some(index) = state.tracks.iter().position(|t| t.name == *index_name) {
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
                let playback_rate = self.playback_rate_hz;

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
                                        playback_rate as u32,
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
                                                    kind: Kind::Audio,
                                                    fade_enabled: true,
                                                    fade_in_samples: 240,
                                                    fade_out_samples: 240,
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
                                                    kind: Kind::MIDI,
                                                    fade_enabled: true,
                                                    fade_in_samples: 240,
                                                    fade_out_samples: 240,
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
            Message::ExportSampleRateSelected(rate) => {
                self.export_sample_rate_hz = rate;
            }
            Message::ExportBitDepthSelected(bit_depth) => {
                self.export_bit_depth = bit_depth;
            }
            Message::ExportNormalizeToggled(enabled) => {
                self.export_normalize = enabled;
            }
            Message::ExportNormalizeModeSelected(mode) => {
                self.export_normalize_mode = mode;
            }
            Message::ExportNormalizeDbfsInput(ref input) => {
                self.export_normalize_dbfs_input = input
                    .chars()
                    .filter(|c| c.is_ascii_digit() || *c == '-' || *c == '.')
                    .collect();
            }
            Message::ExportNormalizeLufsInput(ref input) => {
                self.export_normalize_lufs_input = input
                    .chars()
                    .filter(|c| c.is_ascii_digit() || *c == '-' || *c == '.')
                    .collect();
            }
            Message::ExportNormalizeDbtpInput(ref input) => {
                self.export_normalize_dbtp_input = input
                    .chars()
                    .filter(|c| c.is_ascii_digit() || *c == '-' || *c == '.')
                    .collect();
            }
            Message::ExportNormalizeLimiterToggled(enabled) => {
                self.export_normalize_tp_limiter = enabled;
            }
            Message::ExportSettingsConfirm => {
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
                self.modal = None;
                return Task::perform(
                    async {
                        AsyncFileDialog::new()
                            .set_title("Export to WAV")
                            .add_filter("WAV Audio", &["wav"])
                            .set_file_name("export.wav")
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
                let export_path = Self::ensure_wav_extension(path.clone());
                let state_clone = self.state.clone();

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

                            let result = Self::export_session(
                                &export_path,
                                sample_rate,
                                export_bit_depth,
                                export_normalize,
                                normalize_target_dbfs,
                                normalize_mode,
                                normalize_target_lufs,
                                normalize_true_peak_dbtp,
                                normalize_tp_limiter,
                                state_clone,
                                &session_root,
                                progress_fn,
                            )
                            .await;

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
                    self.state.blocking_write().message = "Export complete".to_string();
                } else if let Some(op) = operation {
                    let percent = (progress * 100.0) as usize;
                    self.state.blocking_write().message = format!("Exporting ({percent}%): {}", op);
                } else {
                    let percent = (progress * 100.0) as usize;
                    self.state.blocking_write().message = format!("Exporting ({percent}%)...");
                }
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
            Message::Workspace => {
                let mut state = self.state.blocking_write();
                state.view = View::Workspace;
            }
            Message::Connections => {
                let mut state = self.state.blocking_write();
                state.view = View::Connections;
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
                        {
                            let mut state = self.state.blocking_write();
                            state.piano = Some(PianoData {
                                track_idx: track_idx.clone(),
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
                    #[cfg(any(target_os = "windows", all(unix, not(target_os = "macos"))))]
                    {
                        state.plugin_graph_connecting = None;
                        state.plugin_graph_moving_plugin = None;
                    }
                    state.plugin_graph_last_plugin_click = None;
                    state.plugin_graph_selected_plugin = None;
                }
                #[cfg(any(target_os = "windows", all(unix, not(target_os = "macos"))))]
                return self.send(Action::TrackGetPluginGraph {
                    track_name: track_name.clone(),
                });
                #[cfg(target_os = "macos")]
                return Task::perform(async {}, |_| {
                    Message::Show(crate::message::Show::TrackPluginList)
                });
            }
            Message::HWSelected(ref hw) => {
                #[cfg(any(
                    target_os = "linux",
                    target_os = "freebsd",
                    target_os = "netbsd",
                    target_os = "openbsd"
                ))]
                {
                    let mut state = self.state.blocking_write();
                    #[cfg(target_os = "freebsd")]
                    {
                        let refreshed = crate::state::discover_freebsd_audio_devices();
                        let selected = refreshed
                            .iter()
                            .find(|candidate| candidate.id == hw.id)
                            .cloned()
                            .unwrap_or_else(|| hw.clone());
                        if !refreshed.is_empty() {
                            state.available_hw = refreshed;
                        }
                        if let Some(bits) = selected.preferred_bits() {
                            state.oss_bits = bits;
                        }
                        state.selected_hw = Some(selected);
                    }
                    #[cfg(target_os = "linux")]
                    {
                        if let Some(bits) = hw.preferred_bits() {
                            state.oss_bits = bits;
                        }
                        state.selected_hw = Some(hw.clone());
                    }
                    #[cfg(target_os = "openbsd")]
                    {
                        let refreshed = crate::state::discover_openbsd_audio_devices();
                        let selected = refreshed
                            .iter()
                            .find(|candidate| candidate.id == hw.id)
                            .cloned()
                            .unwrap_or_else(|| hw.clone());
                        if !refreshed.is_empty() {
                            state.available_hw = refreshed;
                        }
                        if let Some(bits) = selected.preferred_bits() {
                            state.oss_bits = bits;
                        }
                        state.selected_hw = Some(selected);
                    }
                    #[cfg(target_os = "netbsd")]
                    {
                        let refreshed = crate::state::discover_netbsd_audio_devices();
                        let selected = refreshed
                            .iter()
                            .find(|candidate| candidate.id == hw.id)
                            .cloned()
                            .unwrap_or_else(|| hw.clone());
                        if !refreshed.is_empty() {
                            state.available_hw = refreshed;
                        }
                        if let Some(bits) = selected.preferred_bits() {
                            state.oss_bits = bits;
                        }
                        state.selected_hw = Some(selected);
                    }
                }
                #[cfg(not(any(
                    target_os = "linux",
                    target_os = "freebsd",
                    target_os = "netbsd",
                    target_os = "openbsd"
                )))]
                {
                    self.state.blocking_write().selected_hw = Some(hw.to_string());
                }
            }
            Message::HWBackendSelected(ref backend) => {
                let mut state = self.state.blocking_write();
                state.selected_backend = backend.clone();
                state.selected_hw = None;
                #[cfg(any(
                    target_os = "linux",
                    target_os = "freebsd",
                    target_os = "netbsd",
                    target_os = "openbsd"
                ))]
                {
                    state.oss_bits = 32;
                    #[cfg(target_os = "freebsd")]
                    if matches!(backend, crate::state::AudioBackendOption::Oss) {
                        let refreshed = crate::state::discover_freebsd_audio_devices();
                        if !refreshed.is_empty() {
                            state.available_hw = refreshed.clone();
                        }
                        if let Some(selected) = refreshed.first().cloned() {
                            if let Some(bits) = selected.preferred_bits() {
                                state.oss_bits = bits;
                            }
                            state.selected_hw = Some(selected);
                        }
                    }
                    #[cfg(target_os = "netbsd")]
                    if matches!(backend, crate::state::AudioBackendOption::Audio4) {
                        let refreshed = crate::state::discover_netbsd_audio_devices();
                        if !refreshed.is_empty() {
                            state.available_hw = refreshed.clone();
                        }
                        if let Some(selected) = refreshed.first().cloned() {
                            if let Some(bits) = selected.preferred_bits() {
                                state.oss_bits = bits;
                            }
                            state.selected_hw = Some(selected);
                        }
                    }
                }
            }
            Message::HWExclusiveToggled(exclusive) => {
                self.state.blocking_write().oss_exclusive = exclusive;
            }
            #[cfg(unix)]
            Message::HWBitsChanged(bits) => {
                let mut state = self.state.blocking_write();
                state.oss_bits = bits;
            }
            Message::HWPeriodFramesChanged(period_frames) => {
                self.state.blocking_write().oss_period_frames =
                    Self::normalize_period_frames(period_frames);
            }
            Message::HWNPeriodsChanged(nperiods) => {
                self.state.blocking_write().oss_nperiods = nperiods.max(1);
            }
            Message::HWSyncModeToggled(sync_mode) => {
                self.state.blocking_write().oss_sync_mode = sync_mode;
            }
            Message::StartMovingTrackAndSelect(moving_track, track_name) => {
                let mut state = self.state.blocking_write();
                state.moving_track = Some(moving_track);
                return Task::perform(async {}, move |_| {
                    Message::ConnectionViewSelectTrack(track_name)
                });
            }
            _ => {}
        }
        self.update_children(&message);
        Task::none()
    }
}
