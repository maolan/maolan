use super::{MIN_CLIP_WIDTH_PX, Maolan, platform};
use crate::{
    connections,
    message::Message,
    state::{ConnectionViewSelection, HW, Resizing, Track, View},
};
use iced::widget::Id;
use iced::{Length, Point, Task, mouse};
use maolan_engine::{
    kind::Kind,
    message::{Action, ClipMoveFrom, ClipMoveTo},
};
use rfd::AsyncFileDialog;
use std::{process::exit, time::Instant};
use tracing::error;

impl Maolan {
    fn normalize_period_frames(period_frames: usize) -> usize {
        let v = period_frames.clamp(64, 8192);
        if v.is_power_of_two() {
            v
        } else {
            v.next_power_of_two().min(8192)
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::None => {
                return Task::none();
            }
            Message::ToggleTransport => {
                if !self.state.blocking_read().hw_loaded {
                    return Task::none();
                }
                if self.playing {
                    self.toolbar.update(message.clone());
                    self.playing = false;
                    self.last_playback_tick = None;
                    self.stop_recording_preview();
                    return self.send(Action::Stop);
                }
                self.toolbar.update(message.clone());
                self.playing = true;
                self.last_playback_tick = Some(Instant::now());
                if self.record_armed {
                    self.start_recording_preview();
                }
                return self.send(Action::Play);
            }
            Message::WindowResized(size) => {
                self.size = size;
            }
            Message::Show(ref show) => {
                use crate::message::Show;
                if !self.state.blocking_read().hw_loaded && matches!(show, Show::Save | Show::Open)
                {
                    return Task::none();
                }
                match show {
                    Show::Save => {
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
                    }
                    Show::TrackPluginList => {
                        self.modal = Some(Show::TrackPluginList);
                        self.selected_lv2_plugins.clear();
                    }
                }
            }
            Message::NewSession => {
                if !self.state.blocking_read().hw_loaded {
                    return Task::none();
                }
                self.playing = false;
                self.transport_samples = 0.0;
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
                    state.lv2_graph_track = None;
                    state.lv2_graph_plugins.clear();
                    state.lv2_graph_connections.clear();
                    state.lv2_graphs_by_track.clear();
                    state.message = "New session".to_string();
                }
                return Task::batch(tasks);
            }
            Message::Cancel => self.modal = None,
            Message::Request(ref a) => return self.send(a.clone()),
            Message::TransportPlay => {
                self.toolbar.update(message.clone());
                self.playing = true;
                self.last_playback_tick = Some(Instant::now());
                if self.record_armed {
                    self.start_recording_preview();
                }
                return self.send(Action::Play);
            }
            Message::TransportPause => {
                self.toolbar.update(message.clone());
                self.playing = false;
                self.last_playback_tick = None;
                self.stop_recording_preview();
                return self.send(Action::Stop);
            }
            Message::TransportStop => {
                self.toolbar.update(message.clone());
                self.playing = false;
                self.last_playback_tick = None;
                self.stop_recording_preview();
                return self.send(Action::Stop);
            }
            Message::PlaybackTick => {
                if self.playing
                    && let Some(last) = self.last_playback_tick
                {
                    let now = Instant::now();
                    let delta_s = now.duration_since(last).as_secs_f64();
                    self.last_playback_tick = Some(now);
                    self.transport_samples += delta_s * self.playback_rate_hz;
                }
            }
            Message::RecordingPreviewTick => {
                if self.playing
                    && self.record_armed
                    && self.recording_preview_start_sample.is_some()
                {
                    self.recording_preview_sample = Some(self.transport_samples.max(0.0) as usize);
                }
            }
            Message::RecordingPreviewPeaksTick => {
                if self.playing
                    && self.record_armed
                    && self.recording_preview_start_sample.is_some()
                {
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
                        for channel_idx in 0..channels {
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
                            entry[channel_idx].push(amp);
                        }
                    }
                }
            }
            Message::ZoomVisibleBarsChanged(value) => {
                self.zoom_visible_bars = value.clamp(1.0, 256.0);
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
            Message::RefreshLv2Plugins => return self.send(Action::ListLv2Plugins),
            Message::FilterLv2Plugins(ref query) => {
                self.plugin_filter = query.clone();
            }
            Message::SelectLv2Plugin(ref plugin_uri) => {
                if self.selected_lv2_plugins.contains(plugin_uri) {
                    self.selected_lv2_plugins.remove(plugin_uri);
                } else {
                    self.selected_lv2_plugins.insert(plugin_uri.clone());
                }
            }
            Message::LoadSelectedLv2Plugins => {
                let track_name = {
                    let state = self.state.blocking_read();
                    state
                        .lv2_graph_track
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
            Message::SendMessageFinished(ref result) => {
                if let Err(e) = result {
                    error!("Error: {}", e);
                }
            }
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
                    }
                }

                Action::ClipMove {
                    kind,
                    from,
                    to,
                    copy,
                } => {
                    let mut state = self.state.blocking_write();

                    // Find the track by name
                    let from_track_idx_option: Option<usize> = state
                        .tracks
                        .iter()
                        .position(|track| track.name == from.track_name);

                    if let Some(f_idx) = from_track_idx_option {
                        // Get mutable borrow of from_track outside the main loop
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

                        // Now find the to_track and add the clip
                        if let Some(to_track) = state
                            .tracks
                            .iter_mut()
                            .find(|track| track.name == to.track_name)
                        {
                            if let Some(mut clip_data) = clip_to_move {
                                clip_data.start = to.sample_offset;
                                to_track.audio.clips.push(clip_data);
                            } else if let Some(mut midi_clip_data) = midi_clip_to_move {
                                midi_clip_data.start = to.sample_offset;
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
                    kind,
                } => {
                    let mut audio_peaks = vec![];
                    if *kind == Kind::Audio {
                        let key = Self::audio_clip_key(track_name, name, *start, *length, *offset);
                        audio_peaks = self.pending_audio_peaks.remove(&key).unwrap_or_default();
                        if audio_peaks.is_empty()
                            && name.to_ascii_lowercase().ends_with(".wav")
                            && let Some(session_root) = &self.session_dir
                        {
                            let wav_path = session_root.join(name);
                            if wav_path.exists()
                                && let Ok(computed) = Self::compute_audio_clip_peaks(&wav_path, 512)
                            {
                                audio_peaks = computed;
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
                                    peaks_file: None,
                                    peaks: audio_peaks,
                                });
                            }
                            Kind::MIDI => {
                                track.midi.clips.push(crate::state::MIDIClip {
                                    name: name.clone(),
                                    start: *start,
                                    length: *length,
                                    offset: *offset,
                                });
                            }
                        }
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
                    exclusive,
                    period_frames,
                    nperiods,
                    sync_mode,
                } => {
                    self.state.blocking_write().message = format!(
                        "Opened device {} (exclusive={}, period={}, nperiods={}, sync_mode={})",
                        device, exclusive, period_frames, nperiods, sync_mode
                    );
                    self.state.blocking_write().hw_loaded = true;
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
                    state.message =
                        format!("HW {direction} channels: {channels} @ {rate} Hz");
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
                    if self.playing {
                        self.last_playback_tick = Some(Instant::now());
                    }
                }
                Action::Lv2Plugins(plugins) => {
                    let mut state = self.state.blocking_write();
                    state.lv2_plugins = plugins.clone();
                    state.message = format!("Loaded {} LV2 plugins", state.lv2_plugins.len());
                }
                Action::TrackLoadLv2Plugin { track_name, .. }
                | Action::TrackClearDefaultPassthrough { track_name, .. }
                | Action::TrackSetLv2PluginState { track_name, .. }
                | Action::TrackUnloadLv2PluginInstance { track_name, .. }
                | Action::TrackConnectLv2Audio { track_name, .. }
                | Action::TrackDisconnectLv2Audio { track_name, .. }
                | Action::TrackConnectLv2Midi { track_name, .. }
                | Action::TrackDisconnectLv2Midi { track_name, .. } => {
                    let lv2_track = self.state.blocking_read().lv2_graph_track.clone();
                    if lv2_track.as_deref() == Some(track_name.as_str()) {
                        return self.send(Action::TrackGetLv2Graph {
                            track_name: track_name.clone(),
                        });
                    }
                }
                Action::TrackLv2Graph {
                    track_name,
                    plugins,
                    connections,
                } => {
                    let mut state = self.state.blocking_write();
                    state
                        .lv2_graphs_by_track
                        .insert(track_name.clone(), (plugins.clone(), connections.clone()));
                    if state.lv2_graph_track.as_deref() == Some(track_name.as_str()) {
                        state.lv2_graph_track = Some(track_name.clone());
                        state.lv2_graph_plugins = plugins.clone();
                        state.lv2_graph_connections = connections.clone();
                        state.lv2_graph_selected_connections.clear();
                        state.lv2_graph_selected_plugin = state
                            .lv2_graph_selected_plugin
                            .filter(|id| plugins.iter().any(|p| p.instance_id == *id));
                        let mut new_positions = std::collections::HashMap::new();
                        for (idx, plugin) in plugins.iter().enumerate() {
                            let fallback = Point::new(200.0 + idx as f32 * 180.0, 220.0);
                            let pos = state
                                .lv2_graph_plugin_positions
                                .get(&plugin.instance_id)
                                .copied()
                                .unwrap_or(fallback);
                            new_positions.insert(plugin.instance_id, pos);
                        }
                        state.lv2_graph_plugin_positions = new_positions;
                    }
                    drop(state);

                    if self.pending_save_path.is_some() {
                        self.pending_save_tracks.remove(track_name);
                        if self.pending_save_tracks.is_empty() {
                            let path = self.pending_save_path.take().unwrap_or_default();
                            if !path.is_empty() {
                                if let Err(e) = self.save(path.clone()) {
                                    error!("{}", e);
                                } else {
                                    return self.send(Action::SetSessionPath(path));
                                }
                            }
                        }
                    }
                }
                _ => {
                    // Intentionally ignore responses that do not need explicit GUI handling.
                }
            },
            Message::Response(Err(ref e)) => {
                self.state.blocking_write().message = e.clone();
                error!("Engine error: {e}");
            }
            Message::SaveFolderSelected(ref path_opt) => {
                if let Some(path) = path_opt {
                    self.session_dir = Some(path.clone());
                    return self.refresh_graphs_then_save(path.to_string_lossy().to_string());
                }
            }
            Message::RecordFolderSelected(ref path_opt) => {
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
                let ctrl = self.state.blocking_read().ctrl;
                let selected = self.state.blocking_read().selected.contains(name);
                let mut state = self.state.blocking_write();

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
                let mut tasks = vec![];
                for name in &self.state.blocking_read().selected {
                    tasks.push(self.send(Action::RemoveTrack(name.clone())));
                }
                return Task::batch(tasks);
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
                        state.selected_clips.remove(&clip_id);
                    } else {
                        state.selected_clips.insert(clip_id);
                    }
                } else {
                    state.selected_clips.clear();
                    state.selected_clips.insert(clip_id);
                }
            }
            Message::DeselectAll => {
                let mut state = self.state.blocking_write();
                state.selected.clear();
                state.selected_clips.clear();
                state.connection_view_selection = ConnectionViewSelection::None;
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
                        let mut tasks = vec![];
                        for name in set {
                            tasks.push(self.send(Action::RemoveTrack(name.clone())));
                        }
                        drop(state);
                        self.state.blocking_write().connection_view_selection =
                            ConnectionViewSelection::None;
                        return Task::batch(tasks);
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
                let view = self.state.blocking_read().view.clone();
                match view {
                    crate::state::View::Connections => {
                        return self.update(Message::RemoveSelected);
                    }
                    crate::state::View::Workspace => {
                        return self.update(Message::RemoveSelectedTracks);
                    }
                    crate::state::View::TrackPlugins => {
                        let (track_name, selected_plugin, selected_indices, connections) = {
                            let state = self.state.blocking_read();
                            (
                                state.lv2_graph_track.clone(),
                                state.lv2_graph_selected_plugin,
                                state.lv2_graph_selected_connections.clone(),
                                state.lv2_graph_connections.clone(),
                            )
                        };
                        if let Some(track_name) = track_name {
                            if let Some(instance_id) = selected_plugin {
                                self.state.blocking_write().lv2_graph_selected_plugin = None;
                                self.state
                                    .blocking_write()
                                    .lv2_graph_selected_connections
                                    .clear();
                                return self.send(Action::TrackUnloadLv2PluginInstance {
                                    track_name,
                                    instance_id,
                                });
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
                                .lv2_graph_selected_connections
                                .clear();
                            self.state.blocking_write().lv2_graph_selected_plugin = None;
                            return Task::batch(tasks);
                        }
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
                            let clip = &track.audio.clips[clip_index];
                            let initial_value = if is_right_side {
                                clip.length
                            } else {
                                clip.start
                            };
                            state.resizing = Some(Resizing::Clip(
                                *kind,
                                track_name.clone(),
                                clip_index,
                                is_right_side,
                                initial_value as f32,
                                state.cursor.x,
                                clip.length as f32,
                            ));
                        }
                        Kind::MIDI => {
                            let clip = &track.midi.clips[clip_index];
                            let initial_value = if is_right_side {
                                clip.length
                            } else {
                                clip.start
                            };
                            state.resizing = Some(Resizing::Clip(
                                *kind,
                                track_name.clone(),
                                clip_index,
                                is_right_side,
                                initial_value as f32,
                                state.cursor.x,
                                clip.length as f32,
                            ));
                        }
                    }
                }
            }
            Message::MouseMoved(mouse::Event::CursorMoved { position }) => {
                let resizing = self.state.blocking_read().resizing.clone();
                self.state.blocking_write().cursor = position;
                match resizing {
                    Some(Resizing::Track(track_name, initial_height, initial_mouse_y)) => {
                        let mut state = self.state.blocking_write();
                        let delta = position.y - initial_mouse_y;
                        if let Some(track) = state.tracks.iter_mut().find(|t| t.name == track_name)
                        {
                            track.height = (initial_height + delta).clamp(60.0, 400.0);
                        }
                    }
                    Some(Resizing::Clip(
                        kind,
                        track_name,
                        index,
                        is_right_side,
                        initial_value,
                        initial_mouse_x,
                        initial_length,
                    )) => {
                        let pixels_per_sample = self.pixels_per_sample().max(1.0e-6);
                        let min_length_samples =
                            (MIN_CLIP_WIDTH_PX / pixels_per_sample).ceil().max(1.0);
                        let mut state = self.state.blocking_write();
                        if let Some(track) = state.tracks.iter_mut().find(|t| t.name == track_name)
                        {
                            let delta_samples = (position.x - initial_mouse_x) / pixels_per_sample;
                            match kind {
                                Kind::Audio => {
                                    let clip = &mut track.audio.clips[index];
                                    if is_right_side {
                                        let updated_length = (initial_value + delta_samples)
                                            .clamp(min_length_samples, usize::MAX as f32);
                                        clip.length = updated_length as usize;
                                    } else {
                                        let max_start = (initial_value + initial_length
                                            - min_length_samples)
                                            .max(0.0);
                                        let new_start =
                                            (initial_value + delta_samples).clamp(0.0, max_start);
                                        let updated_length = (initial_length
                                            - (new_start - initial_value))
                                            .clamp(min_length_samples, usize::MAX as f32);
                                        clip.start = new_start as usize;
                                        clip.length = updated_length as usize;
                                    }
                                }
                                Kind::MIDI => {
                                    let clip = &mut track.midi.clips[index];
                                    if is_right_side {
                                        let updated_length = (initial_value + delta_samples)
                                            .clamp(min_length_samples, usize::MAX as f32);
                                        clip.length = updated_length as usize;
                                    } else {
                                        let max_start = (initial_value + initial_length
                                            - min_length_samples)
                                            .max(0.0);
                                        let new_start =
                                            (initial_value + delta_samples).clamp(0.0, max_start);
                                        let updated_length = (initial_length
                                            - (new_start - initial_value))
                                            .clamp(min_length_samples, usize::MAX as f32);
                                        clip.start = new_start as usize;
                                        clip.length = updated_length as usize;
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
                    _ => {}
                }
            }
            Message::MouseReleased => {
                let mut state = self.state.blocking_write();
                state.resizing = None;
            }
            Message::ClipDrag(ref clip) => {
                if matches!(
                    self.state.blocking_read().resizing,
                    Some(Resizing::Clip(_, _, _, _, _, _, _))
                ) {
                    return Task::none();
                }
                {
                    let state = self.state.blocking_read();
                    if !state.selected_clips.is_empty() {
                        let clip_id = crate::state::ClipId {
                            track_idx: clip.track_index.clone(),
                            clip_idx: clip.index,
                            kind: clip.kind,
                        };
                        if !state.selected_clips.contains(&clip_id) {
                            return Task::none();
                        }
                    }
                }
                match &mut self.clip {
                    Some(active)
                        if active.kind == clip.kind
                            && active.index == clip.index
                            && active.track_index == clip.track_index =>
                    {
                        active.end = clip.start;
                    }
                    Some(_) => {
                        // Keep the original drag source locked until drop.
                    }
                    None => {
                        self.clip = Some(clip.clone());
                    }
                }
            }
            Message::ClipDropped(point, _rect) => {
                if matches!(
                    self.state.blocking_read().resizing,
                    Some(Resizing::Clip(_, _, _, _, _, _, _))
                ) {
                    self.clip = None;
                    return Task::none();
                }
                if let Some(clip) = &mut self.clip {
                    clip.end = point;
                    return iced_drop::zones_on_point(Message::HandleClipZones, point, None, None);
                }
            }
            Message::HandleClipZones(ref zones) => {
                if let Some(clip) = &self.clip {
                    let state = self.state.blocking_read();
                    let from_track_name = &clip.track_index;
                    let to_track_id = zones.iter().map(|(id, _)| id).find(|id| {
                        state
                            .tracks
                            .iter()
                            .any(|t| Id::from(t.name.clone()) == **id)
                    });
                    let Some(to_track_id) = to_track_id else {
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
                        let clip_index = clip.index;
                        match clip.kind {
                            Kind::Audio => {
                                if clip_index >= from_track.audio.clips.len() {
                                    self.clip = None;
                                    return Task::none();
                                }
                                let clip_index_in_from_track = clip_index;
                                let mut clip_copy =
                                    from_track.audio.clips[clip_index_in_from_track].clone();
                                let offset = (clip.end.x - clip.start.x)
                                    / self.pixels_per_sample().max(1.0e-6);
                                clip_copy.start =
                                    (clip_copy.start as f32 + offset).max(0.0) as usize;
                                let task = self.send(Action::ClipMove {
                                    kind: clip.kind,
                                    from: ClipMoveFrom {
                                        track_name: from_track.name.clone(),
                                        clip_index: clip.index,
                                    },
                                    to: ClipMoveTo {
                                        track_name: to_track.name.clone(),
                                        sample_offset: clip_copy.start,
                                    },
                                    copy: state.ctrl,
                                });
                                self.clip = None;
                                return task;
                            }
                            Kind::MIDI => {
                                if clip_index >= from_track.midi.clips.len() {
                                    self.clip = None;
                                    return Task::none();
                                }
                                let clip_index_in_from_track = clip_index;
                                let mut clip_copy =
                                    from_track.midi.clips[clip_index_in_from_track].clone();
                                let offset = (clip.end.x - clip.start.x)
                                    / self.pixels_per_sample().max(1.0e-6);
                                clip_copy.start =
                                    (clip_copy.start as f32 + offset).max(0.0) as usize;
                                let task = self.send(Action::ClipMove {
                                    kind: clip.kind,
                                    from: ClipMoveFrom {
                                        track_name: from_track.name.clone(),
                                        clip_index: clip.index,
                                    },
                                    to: ClipMoveTo {
                                        track_name: to_track.name.clone(),
                                        sample_offset: clip_copy.start,
                                    },
                                    copy: state.ctrl,
                                });
                                self.clip = None;
                                return task;
                            }
                        }
                    }
                }
                self.clip = None;
                return Task::none();
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
                            .position(|t| Id::from(t.name.clone()) == *track_id); // Compare Id with Id

                        if let Some(t_idx) = to_index {
                            state.tracks.insert(t_idx, moved_track);
                        } else {
                            // If target track not found, insert back to original position (or end)
                            // For simplicity, let's insert it at the end if target not found
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
                            .add_filter("wav", &["wav"])
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
                let count = paths.len();
                self.state.blocking_write().message = if count == 1 {
                    "Import is not implemented yet (1 file selected)".to_string()
                } else {
                    format!("Import is not implemented yet ({count} files selected)")
                };
            }
            Message::Workspace => {
                self.state.blocking_write().view = View::Workspace;
            }
            Message::Connections => {
                self.state.blocking_write().view = View::Connections;
            }
            Message::OpenTrackPlugins(track_name) => {
                {
                    let mut state = self.state.blocking_write();
                    state.view = View::TrackPlugins;
                    state.lv2_graph_track = Some(track_name.clone());
                    state.lv2_graph_connecting = None;
                    state.lv2_graph_moving_plugin = None;
                    state.lv2_graph_last_plugin_click = None;
                    state.lv2_graph_selected_plugin = None;
                }
                return self.send(Action::TrackGetLv2Graph { track_name });
            }
            Message::HWSelected(ref hw) => {
                #[cfg(target_os = "linux")]
                {
                    self.state.blocking_write().selected_hw = Some(hw.clone());
                }
                #[cfg(not(target_os = "linux"))]
                {
                    self.state.blocking_write().selected_hw = Some(hw.to_string());
                }
            }
            Message::HWExclusiveToggled(exclusive) => {
                self.state.blocking_write().oss_exclusive = exclusive;
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
