use super::*;
use crate::consts::state_track::{TRACK_MIN_HEIGHT, TRACK_SUBTRACK_MIN_HEIGHT};
use crate::consts::widget_piano::{
    H_ZOOM_MAX, H_ZOOM_MIN, KEYBOARD_WIDTH, MAIN_SPLIT_SPACING, PITCH_MAX,
    RIGHT_SCROLL_GUTTER_WIDTH, TOOLS_STRIP_WIDTH,
};
#[cfg(test)]
use crate::gui::field_groups::{GenerateState, UiState};
use crate::message::{ModulatorChange, SnapMode, TrackAutomationTarget};
use maolan_engine::message::PluginGraphNode;
mod audio_editor;
mod automation;
mod clips;
mod core;
mod dialogs;
mod export;
mod generate;
mod live_session;
mod modulators;
mod piano;
mod plugins;
mod response;
mod response_events;
mod response_freeze_meter;
mod response_session_state;
mod response_state;
mod response_timing_state;
mod response_track;
mod routing;
mod session;
mod session_io;
mod show;
mod timing;
mod track_selection;
mod tracks;
mod transport;
mod ui;

const CLIP_EDGE_SNAP_THRESHOLD_PX: f32 = 12.0;
const TRACK_SETUP_MIN_TRACKS_WIDTH: f32 = 338.6557;

fn native_ui_error_matches(format: &str, error: &str) -> bool {
    if format.eq_ignore_ascii_case("VST3") {
        error.contains("VST3 GUI")
            || error.contains("editor view")
            || error.contains("No GUI view")
            || error.contains("Platform type")
    } else if format.eq_ignore_ascii_case("AU") {
        error.contains("AU GUI") || error.contains("AUv2") || error.contains("No GUI view")
    } else if format.eq_ignore_ascii_case("LV2") {
        error.contains("LV2 GUI")
            || error.contains("LV2 UI")
            || error.contains("No suitable LV2")
            || error.contains("supported LV2")
            || error.contains("only supported on X11")
    } else {
        false
    }
}

fn audio_editor_buffer_from_clip(
    path: &std::path::Path,
    offset: usize,
    length: usize,
) -> Result<maolan_editor::app::AudioBuffer, String> {
    let (samples, channels, sample_rate) =
        maolan_engine::audio_codec::decode_audio_to_f32_interleaved_sync(path)
            .map_err(|err| format!("Failed to open '{}': {err}", path.display()))?;
    let channels = channels.max(1);
    let frames = samples.len() / channels;
    let start = offset.min(frames);
    let end = start.saturating_add(length).min(frames);
    let clipped = if start < end {
        samples[start * channels..end * channels].to_vec()
    } else {
        Vec::new()
    };
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("audio clip")
        .to_string();

    Ok(maolan_editor::app::AudioBuffer::new(
        name,
        std::sync::Arc::new(clipped),
        channels,
        sample_rate,
    ))
}

struct MoveClipSnapArgs<'a> {
    kind: Kind,
    from_track_name: &'a str,
    clip_index: usize,
    offset: f32,
    group_drag_active: bool,
    selected_group: &'a [crate::state::ClipId],
    copy: bool,
}

/// Vertical track delta of a clip group drag: the drop target's position
/// minus the dragged clip's source track position.
pub(crate) fn group_drag_delta(
    state: &crate::state::StateData,
    from_track_name: &str,
    to_track_name: &str,
) -> Option<isize> {
    let from = state
        .tracks
        .iter()
        .position(|t| t.name == from_track_name)?;
    let to = state.tracks.iter().position(|t| t.name == to_track_name)?;
    Some(to as isize - from as isize)
}

/// Target track for one member of a clip group drag: the member's own
/// track shifted by the group delta. Returns `None` when the shifted
/// position is out of range or the track cannot accept the clip kind.
pub(crate) fn group_drag_target<'a>(
    state: &'a crate::state::StateData,
    kind: Kind,
    delta: isize,
    member_track_name: &str,
) -> Option<&'a str> {
    let member_pos = state
        .tracks
        .iter()
        .position(|t| t.name == member_track_name)?;
    let target_pos = member_pos as isize + delta;
    if target_pos < 0 || target_pos as usize >= state.tracks.len() {
        return None;
    }
    let source = &state.tracks[member_pos];
    let target = &state.tracks[target_pos as usize];
    let compatible = match kind {
        Kind::Audio => {
            !target.is_folder && target.audio.ins > 0 && target.audio.ins == source.audio.ins
        }
        Kind::MIDI => !target.is_folder && target.midi.ins > 0,
    };
    compatible.then_some(target.name.as_str())
}

/// Initial geometry of every selected clip of `kind` when the dragged clip
/// is part of the selection, so a resize/stretch gesture applies to the
/// whole selection. Empty when the drag is not a group resize.
pub(crate) fn clip_resize_group(
    state: &crate::state::StateData,
    kind: Kind,
    track_name: &str,
    clip_index: usize,
) -> Vec<crate::state::ClipResizeGroupMember> {
    let dragged = crate::state::ClipId {
        track_idx: track_name.to_string(),
        clip_idx: clip_index,
        kind,
    };
    if !state.selected_clips.contains(&dragged) {
        return Vec::new();
    }
    let members = state
        .selected_clips
        .iter()
        .filter(|id| id.kind == kind)
        .filter_map(|id| {
            let track = state.tracks.iter().find(|t| t.name == id.track_idx)?;
            let (locked, start, length, offset) = match kind {
                Kind::Audio => track
                    .audio
                    .clips
                    .get(id.clip_idx)
                    .map(|c| (c.take_lane_locked, c.start, c.length, c.offset)),
                Kind::MIDI => track
                    .midi
                    .clips
                    .get(id.clip_idx)
                    .map(|c| (c.take_lane_locked, c.start, c.length, c.offset)),
            }?;
            if locked {
                return None;
            }
            Some(crate::state::ClipResizeGroupMember {
                clip: id.clone(),
                initial_start: start,
                initial_length: length,
                initial_offset: offset,
            })
        })
        .collect::<Vec<_>>();
    if members.len() > 1 {
        members
    } else {
        Vec::new()
    }
}

impl Maolan {
    fn start_audio_editor_engine_preview(&self) -> Task<Message> {
        let Some(preview) = maolan_editor::app::host_preview(&self.audio_editor) else {
            return Task::none();
        };
        Task::perform(
            async move {
                CLIENT
                    .send(EngineMessage::StartAudioPreview {
                        samples: preview.samples,
                        channels: preview.channels,
                        start_sample: preview.start_sample,
                    })
                    .await
            },
            |_| Message::None,
        )
    }

    fn stop_audio_editor_engine_preview(&self) -> Task<Message> {
        Task::perform(
            async move { CLIENT.send(EngineMessage::StopAudioPreview).await },
            |_| Message::None,
        )
    }

    fn apply_audio_editor_action(
        &mut self,
        action: maolan_editor::app::AudioEditAction,
    ) -> Task<Message> {
        let Some(ctx) = self.audio_editor_clip.clone() else {
            self.info("No audio editor clip is active.");
            return Task::none();
        };

        let summary = {
            let mut state = self.state.write().expect("state lock poisoned");
            state
                .tracks
                .iter_mut()
                .find(|track| track.name == ctx.track_name)
                .and_then(|track| track.audio.clips.get_mut(ctx.clip_idx))
                .map(|clip| {
                    clip.edit_actions.push(action);
                    let summary = maolan_editor::app::summarize_audio_edit_actions(
                        clip.length,
                        &clip.edit_actions,
                    );
                    clip.fade_enabled = summary.fade_in_samples > 0 || summary.fade_out_samples > 0;
                    clip.fade_in_samples = summary.fade_in_samples;
                    clip.fade_out_samples = summary.fade_out_samples;
                    clip.gain_db = summary.gain_db;
                    clip.reversed = summary.reversed;
                    summary
                })
        };

        let Some(summary) = summary else {
            self.info("Audio editor clip no longer exists.");
            return Task::none();
        };

        self.session_ops.has_unsaved_changes = true;
        self.send(Action::ApplyGroupedActions(vec![
            Action::SetClipFade {
                track_name: ctx.track_name.clone(),
                clip_index: ctx.clip_idx,
                kind: Kind::Audio,
                fade_enabled: summary.fade_in_samples > 0 || summary.fade_out_samples > 0,
                fade_in_samples: summary.fade_in_samples,
                fade_out_samples: summary.fade_out_samples,
            },
            Action::SetClipGainDb {
                track_name: ctx.track_name.clone(),
                clip_index: ctx.clip_idx,
                kind: Kind::Audio,
                gain_db: summary.gain_db,
            },
            Action::SetClipReversed {
                track_name: ctx.track_name,
                clip_index: ctx.clip_idx,
                kind: Kind::Audio,
                reversed: summary.reversed,
            },
        ]))
    }

    fn preserve_plugin_graph_states_from_cache(
        track_name: &str,
        state: &crate::state::StateData,
        plugins: &mut [maolan_engine::message::PluginGraphPlugin],
    ) {
        let Some((cached_plugins, _)) = state.plugin_graphs_by_track.get(track_name) else {
            return;
        };

        for plugin in plugins.iter_mut().filter(|plugin| plugin.state.is_none()) {
            if let Some(cached_state) = cached_plugins
                .iter()
                .find(|cached| cached.instance_id == plugin.instance_id)
                .and_then(|cached| cached.state.clone())
            {
                plugin.state = Some(cached_state);
            }
        }
    }

    fn piano_timeline_viewport_width(&self) -> f32 {
        (self.size.width
            - TOOLS_STRIP_WIDTH
            - MAIN_SPLIT_SPACING
            - KEYBOARD_WIDTH
            - RIGHT_SCROLL_GUTTER_WIDTH)
            .max(1.0)
    }

    fn piano_fit_clip_zoom_x(&self, clip_length_samples: usize) -> f32 {
        let base_pps = self.pixels_per_sample().max(1.0e-6);
        let clip_width_at_zoom_one = clip_length_samples.max(1) as f32 * base_pps;
        (self.piano_timeline_viewport_width() / clip_width_at_zoom_one)
            .clamp(H_ZOOM_MIN, H_ZOOM_MAX)
    }

    fn sort_open_piano_notes_like_engine(state: &mut crate::state::StateData) {
        let Some(piano) = state.piano.as_mut() else {
            return;
        };
        if piano.notes.len() < 2 {
            return;
        }

        let mut indexed_notes = piano.notes.drain(..).enumerate().collect::<Vec<_>>();
        indexed_notes.sort_by_key(|(_, note)| (note.start_sample, note.pitch));

        let mut remap = vec![0usize; indexed_notes.len()];
        for (new_idx, (old_idx, _)) in indexed_notes.iter().enumerate() {
            remap[*old_idx] = new_idx;
        }
        piano.notes = indexed_notes.into_iter().map(|(_, note)| note).collect();

        if !state.piano_selected_notes.is_empty() {
            state.piano_selected_notes = state
                .piano_selected_notes
                .iter()
                .filter_map(|idx| remap.get(*idx).copied())
                .collect();
        }
    }

    fn midi_paint_interval_samples(
        snap_mode: SnapMode,
        samples_per_beat: f32,
        samples_per_bar: f32,
    ) -> f32 {
        match snap_mode {
            SnapMode::NoSnap | SnapMode::Clips => (samples_per_beat / 4.0).max(1.0),
            _ => snap_mode.interval_samples(samples_per_beat, samples_per_bar),
        }
    }

    fn midi_drag_delta_samples(
        snap_mode: SnapMode,
        delta_samples: i64,
        samples_per_beat: f32,
        samples_per_bar: f32,
    ) -> i64 {
        if matches!(snap_mode, SnapMode::NoSnap | SnapMode::Clips) {
            return delta_samples;
        }
        let interval = snap_mode
            .interval_samples(samples_per_beat, samples_per_bar)
            .max(1.0);
        ((delta_samples as f32 / interval).round() * interval) as i64
    }

    fn insert_painted_midi_notes_between(
        &mut self,
        raw_start_sample: f32,
        raw_end_sample: f32,
        pitch: u8,
    ) -> Task<Message> {
        let mut state = self.state.write().expect("state lock poisoned");
        let Some(piano) = state.piano.as_ref() else {
            return Task::none();
        };

        let tempo = state.tempo.max(1.0);
        let tsig_num = state.time_signature_num.max(1) as f32;
        let tsig_denom = state.time_signature_denom.max(1) as f32;
        let samples_per_beat =
            (self.transport.playback_rate_hz as f32 * 60.0 / tempo) * (4.0 / tsig_denom);
        let samples_per_bar = samples_per_beat * tsig_num;
        let length_samples = Self::midi_paint_interval_samples(
            self.timing.midi_snap_mode,
            samples_per_beat,
            samples_per_bar,
        ) as usize;
        let interval = length_samples.max(1);
        let min_raw = raw_start_sample.min(raw_end_sample).max(0.0);
        let max_raw = raw_start_sample.max(raw_end_sample).max(0.0);
        let first = if matches!(
            self.timing.midi_snap_mode,
            SnapMode::NoSnap | SnapMode::Clips
        ) {
            ((min_raw / interval as f32).floor() as usize).saturating_mul(interval)
        } else {
            self.timing
                .midi_snap_mode
                .snap_sample(min_raw, samples_per_beat, samples_per_bar)
                .max(0.0) as usize
        };
        let last = if matches!(
            self.timing.midi_snap_mode,
            SnapMode::NoSnap | SnapMode::Clips
        ) {
            ((max_raw / interval as f32).floor() as usize).saturating_mul(interval)
        } else {
            self.timing
                .midi_snap_mode
                .snap_sample(max_raw, samples_per_beat, samples_per_bar)
                .max(0.0) as usize
        };

        let track_name = piano.track_idx.clone();
        let clip_idx = piano.clip_index;
        let insert_base = piano
            .notes
            .len()
            .saturating_add(state.piano_painted_notes.len());
        let existing_notes: std::collections::HashSet<(usize, u8)> = piano
            .notes
            .iter()
            .map(|note| (note.start_sample, note.pitch))
            .collect();
        let mut notes = Vec::new();
        let mut sample = first;
        while sample <= last {
            let key = crate::state::PaintedMidiNoteKey {
                start_sample: sample,
                pitch,
            };
            if !state.piano_painted_notes.contains(&key)
                && !existing_notes.contains(&(sample, pitch))
            {
                let insert_idx = insert_base.saturating_add(notes.len());
                state.piano_painted_notes.insert(key);
                notes.push((
                    insert_idx,
                    maolan_engine::message::MidiNoteData {
                        start_sample: sample,
                        length_samples: interval,
                        pitch,
                        velocity: 100,
                        channel: 0,
                        mpe: Default::default(),
                    },
                ));
            }
            let Some(next) = sample.checked_add(interval) else {
                break;
            };
            if next == sample {
                break;
            }
            sample = next;
        }

        if notes.is_empty() {
            return Task::none();
        }
        drop(state);

        self.send(Action::InsertMidiNotes {
            track_name,
            clip_index: clip_idx,
            notes,
        })
    }

    fn insert_stretched_midi_note_between(
        &mut self,
        raw_start_sample: f32,
        raw_end_sample: f32,
        pitch: u8,
    ) -> Task<Message> {
        let state = self.state.read().expect("state lock poisoned");
        let Some(piano) = state.piano.as_ref() else {
            return Task::none();
        };

        let tempo = state.tempo.max(1.0);
        let tsig_num = state.time_signature_num.max(1) as f32;
        let tsig_denom = state.time_signature_denom.max(1) as f32;
        let samples_per_beat =
            (self.transport.playback_rate_hz as f32 * 60.0 / tempo) * (4.0 / tsig_denom);
        let samples_per_bar = samples_per_beat * tsig_num;
        let snap_interval = self
            .timing
            .midi_snap_mode
            .interval_samples(samples_per_beat, samples_per_bar)
            .max(1.0);

        let start_raw = raw_start_sample.min(raw_end_sample).max(0.0);
        let end_raw = raw_start_sample.max(raw_end_sample).max(0.0);
        let start_sample = self
            .timing
            .midi_snap_mode
            .snap_sample(start_raw, samples_per_beat, samples_per_bar)
            .max(0.0) as usize;
        let mut end_sample = self
            .timing
            .midi_snap_mode
            .snap_sample(end_raw, samples_per_beat, samples_per_bar)
            .max(0.0) as usize;
        let min_len = snap_interval as usize;
        if end_sample <= start_sample {
            end_sample = start_sample.saturating_add(min_len);
        }
        let length_samples = end_sample.saturating_sub(start_sample).max(min_len);

        let track_name = piano.track_idx.clone();
        let clip_idx = piano.clip_index;
        let insert_idx = piano.notes.len();
        drop(state);

        self.send(Action::InsertMidiNotes {
            track_name,
            clip_index: clip_idx,
            notes: vec![(
                insert_idx,
                maolan_engine::message::MidiNoteData {
                    start_sample,
                    length_samples,
                    pitch,
                    velocity: 100,
                    channel: 0,
                    mpe: Default::default(),
                },
            )],
        })
    }

    fn piano_position_to_raw_sample_and_pitch(
        &self,
        state: &crate::state::StateData,
        position: Point,
    ) -> (f32, u8) {
        let zoom_x = state.piano_zoom_x;
        let zoom_y = state.piano_zoom_y;
        let row_h = ((14.0 * 7.0 / 12.0) * zoom_y).max(1.0);
        let tracks_width = match state.tracks_width {
            Length::Fixed(v) => v,
            _ => 200.0,
        };
        let editor_width = (self.size.width - tracks_width - 3.0).max(1.0);
        let tempo = state.tempo.max(1.0);
        let tsig_num = state.time_signature_num.max(1) as f32;
        let tsig_denom = state.time_signature_denom.max(1) as f32;
        let samples_per_beat =
            (self.transport.playback_rate_hz as f32 * 60.0 / tempo) * (4.0 / tsig_denom);
        let samples_per_bar = samples_per_beat * tsig_num;
        let total_samples = (samples_per_bar * self.ui.zoom_visible_bars).max(1.0);
        let pps = ((editor_width / total_samples) * zoom_x).max(1.0e-6);

        let raw_start_sample = (position.x.max(0.0) / pps).floor().max(0.0);
        let pitch_row = (position.y / row_h).floor();
        let pitch_row = pitch_row.clamp(0.0, f32::from(PITCH_MAX)) as usize;
        let pitch = PITCH_MAX.saturating_sub(pitch_row as u8);

        (raw_start_sample, pitch)
    }

    fn insert_painted_piano_notes_between(&mut self, start: Point, end: Point) -> Task<Message> {
        let state = self.state.read().expect("state lock poisoned");
        let (raw_start_sample, pitch) = self.piano_position_to_raw_sample_and_pitch(&state, start);
        let (raw_end_sample, _) = self.piano_position_to_raw_sample_and_pitch(&state, end);
        drop(state);
        self.insert_painted_midi_notes_between(raw_start_sample, raw_end_sample, pitch)
    }

    fn insert_stretched_piano_note_between(&mut self, start: Point, end: Point) -> Task<Message> {
        let state = self.state.read().expect("state lock poisoned");
        let (raw_start_sample, pitch) = self.piano_position_to_raw_sample_and_pitch(&state, start);
        let (raw_end_sample, _) = self.piano_position_to_raw_sample_and_pitch(&state, end);
        drop(state);
        self.insert_stretched_midi_note_between(raw_start_sample, raw_end_sample, pitch)
    }

    fn active_workspace_cursor(&self) -> Point {
        let state = self.state.read().expect("state lock poisoned");
        state.editor_cursor.unwrap_or(state.cursor)
    }

    /// Removes selected plugins/connections from the currently open track plugin graph,
    /// returning `Some(task)` if anything was selected.
    fn remove_selected_track_plugin_graph_items(&self) -> Option<Task<Message>> {
        let (
            track_name,
            selected_plugins,
            selected_indices,
            connections,
            selected_connectable,
            connectable_connections,
        ) = {
            let state = self.state.read().expect("state lock poisoned");
            (
                state.plugin_graph_track.clone(),
                state.plugin_graph_selected_plugins.clone(),
                state.plugin_graph_selected_connections.clone(),
                state.plugin_graph_connections.clone(),
                state.plugin_graph_selected_connectable_connections.clone(),
                state.connectable_connections.clone(),
            )
        };
        let track_name = track_name?;
        if !selected_plugins.is_empty() {
            let mut tasks = Vec::new();
            let mut state = self.state.write().expect("state lock poisoned");
            let plugins_to_remove: Vec<usize> = selected_plugins.iter().copied().collect();
            for instance_id in plugins_to_remove {
                let selected_node = state
                    .plugin_graph_plugins
                    .iter()
                    .find(|p| p.instance_id == instance_id)
                    .map(|p| p.node.clone());
                if let Some(node) = selected_node {
                    let task = match node {
                        #[cfg(unix)]
                        PluginGraphNode::Lv2PluginInstance(_) => {
                            self.send(Action::TrackUnloadLv2PluginInstance {
                                track_name: track_name.clone(),
                                instance_id,
                            })
                        }
                        PluginGraphNode::Vst3PluginInstance(_) => {
                            self.send(Action::TrackUnloadVst3PluginInstance {
                                track_name: track_name.clone(),
                                instance_id,
                            })
                        }
                        #[cfg(target_os = "macos")]
                        PluginGraphNode::AuPluginInstance(_) => {
                            self.send(Action::TrackUnloadAuPluginInstance {
                                track_name: track_name.clone(),
                                instance_id,
                            })
                        }
                        PluginGraphNode::ClapPluginInstance(_) => {
                            let plugin_id = state
                                .plugin_graph_plugins
                                .iter()
                                .find(|p| p.instance_id == instance_id)
                                .map(|p| p.plugin_id.clone())
                                .unwrap_or_default();
                            self.send(Action::TrackUnloadClapPlugin {
                                track_name: track_name.clone(),
                                plugin_id,
                            })
                        }
                        PluginGraphNode::TrackInput | PluginGraphNode::TrackOutput => Task::none(),
                    };
                    tasks.push(task);
                }
            }
            state.plugin_graph_selected_plugins.clear();
            state.plugin_graph_selected_connections.clear();
            state.plugin_graph_selected_connectable_connections.clear();
            return Some(Task::batch(tasks));
        }
        if !selected_indices.is_empty() {
            let actions = connections::selection::plugin_disconnect_actions(
                &track_name,
                &connections,
                &selected_indices,
            );
            let tasks = actions
                .into_iter()
                .map(|a| self.send(a))
                .collect::<Vec<_>>();
            let mut state = self.state.write().expect("state lock poisoned");
            state.plugin_graph_selected_connections.clear();
            state.plugin_graph_selected_plugins.clear();
            state.plugin_graph_selected_connectable_connections.clear();
            return Some(Task::batch(tasks));
        }
        if !selected_connectable.is_empty() {
            let actions = connections::selection::connectable_disconnect_actions(
                &track_name,
                &connectable_connections,
                &selected_connectable,
            );
            let tasks = actions
                .into_iter()
                .map(|a| self.send(a))
                .collect::<Vec<_>>();
            let mut state = self.state.write().expect("state lock poisoned");
            state.plugin_graph_selected_connectable_connections.clear();
            state.plugin_graph_selected_connections.clear();
            state.plugin_graph_selected_plugins.clear();
            return Some(Task::batch(tasks));
        }
        None
    }

    fn toggle_selected_plugin_bypass(&mut self) -> Task<Message> {
        let (
            track_name,
            clip_graph_open,
            selected_plugins,
            plugin_graph_plugins,
            plugin_graph_connections,
        ) = {
            let state = self.state.read().expect("state lock poisoned");
            (
                state.plugin_graph_track.clone(),
                state.plugin_graph_clip.is_some(),
                state.plugin_graph_selected_plugins.clone(),
                state.plugin_graph_plugins.clone(),
                state.plugin_graph_connections.clone(),
            )
        };
        let Some(track_name) = track_name else {
            return Task::none();
        };
        if selected_plugins.is_empty() {
            return Task::none();
        }

        if clip_graph_open {
            let mut state = self.state.write().expect("state lock poisoned");
            let mut changed = false;
            let selected_plugins = state.plugin_graph_selected_plugins.clone();
            for plugin in &mut state.plugin_graph_plugins {
                if selected_plugins.contains(&plugin.instance_id) {
                    plugin.bypassed = !plugin.bypassed;
                    changed = true;
                }
            }
            if !changed {
                return Task::none();
            }
            let sync = Self::save_open_clip_plugin_graph(&mut state);
            drop(state);
            self.session_ops.has_unsaved_changes = true;
            return sync.map_or_else(Task::none, |action| self.send(action));
        }

        let actions = selected_plugins
            .into_iter()
            .filter_map(|instance_id| {
                plugin_graph_plugins
                    .iter()
                    .find(|plugin| plugin.instance_id == instance_id)
                    .map(|plugin| Action::TrackSetPluginBypassed {
                        track_name: track_name.clone(),
                        instance_id,
                        format: plugin.format.clone(),
                        bypassed: !plugin.bypassed,
                    })
            })
            .collect::<Vec<_>>();
        if actions.is_empty() {
            return Task::none();
        }

        let mut state = self.state.write().expect("state lock poisoned");
        for action in &actions {
            if let Action::TrackSetPluginBypassed {
                instance_id,
                bypassed,
                ..
            } = action
                && let Some(plugin) = state
                    .plugin_graph_plugins
                    .iter_mut()
                    .find(|plugin| plugin.instance_id == *instance_id)
            {
                plugin.bypassed = *bypassed;
            }
        }
        let current_plugins = state.plugin_graph_plugins.clone();
        if let Some((plugins, _)) = state.plugin_graphs_by_track.get_mut(&track_name) {
            for action in &actions {
                if let Action::TrackSetPluginBypassed {
                    instance_id,
                    bypassed,
                    ..
                } = action
                    && let Some(plugin) = plugins
                        .iter_mut()
                        .find(|plugin| plugin.instance_id == *instance_id)
                {
                    plugin.bypassed = *bypassed;
                }
            }
        } else {
            state.plugin_graphs_by_track.insert(
                track_name.clone(),
                (current_plugins, plugin_graph_connections),
            );
        }
        drop(state);

        self.session_ops.has_unsaved_changes = true;
        Task::batch(actions.into_iter().map(|action| self.send(action)))
    }

    fn clip_edge_snap_threshold_samples(&self) -> f32 {
        (CLIP_EDGE_SNAP_THRESHOLD_PX / self.pixels_per_sample().max(1.0e-6)).max(1.0)
    }

    fn nearest_clip_edge_sample(
        raw_edge: f32,
        snapped_edge: f32,
        threshold_samples: f32,
        candidate_edges: impl IntoIterator<Item = (crate::state::ClipId, usize)>,
    ) -> (f32, Option<crate::state::ClipId>, Vec<crate::state::ClipId>) {
        let mut best: Option<(f32, f32, crate::state::ClipId)> = None;
        let mut matched_targets = Vec::new();

        for (clip_id, edge) in candidate_edges {
            let edge = edge as f32;
            let distance = (raw_edge - edge).abs();
            if distance > threshold_samples {
                continue;
            }
            if !matched_targets.contains(&clip_id) {
                matched_targets.push(clip_id.clone());
            }

            let replace = match best {
                None => true,
                Some((best_distance, best_edge, _)) => {
                    distance < best_distance
                        || (distance == best_distance
                            && (edge - snapped_edge).abs() < (best_edge - snapped_edge).abs())
                }
            };

            if replace {
                best = Some((distance, edge, clip_id));
            }
        }

        if let Some((_, edge, clip_id)) = best {
            (edge.max(0.0), Some(clip_id), matched_targets)
        } else {
            (snapped_edge.max(0.0), None, matched_targets)
        }
    }

    fn snapped_clip_move_start(
        raw_start: f32,
        clip_length: f32,
        snapped_start: f32,
        threshold_samples: f32,
        candidate_edges: impl IntoIterator<Item = (crate::state::ClipId, usize)>,
    ) -> (f32, Option<crate::state::ClipId>, Vec<crate::state::ClipId>) {
        let raw_end = raw_start + clip_length;
        let mut best: Option<(f32, f32, crate::state::ClipId)> = None;
        let mut matched_targets = Vec::new();

        for (clip_id, edge) in candidate_edges {
            let edge = edge as f32;
            let candidates = [(raw_start, edge), (raw_end, edge - clip_length)];

            for (raw_edge, snapped_start_candidate) in candidates {
                if snapped_start_candidate < 0.0 {
                    continue;
                }
                let distance = (raw_edge - edge).abs();
                if distance > threshold_samples {
                    continue;
                }
                if !matched_targets.contains(&clip_id) {
                    matched_targets.push(clip_id.clone());
                }

                let replace = match best {
                    None => true,
                    Some((best_distance, best_start, _)) => {
                        distance < best_distance
                            || (distance == best_distance
                                && (snapped_start_candidate - snapped_start).abs()
                                    < (best_start - snapped_start).abs())
                    }
                };

                if replace {
                    best = Some((distance, snapped_start_candidate, clip_id.clone()));
                }
            }
        }

        if let Some((_, start, clip_id)) = best {
            (start.max(0.0), Some(clip_id), matched_targets)
        } else {
            (snapped_start.max(0.0), None, matched_targets)
        }
    }

    fn clip_snap_edges(
        &self,
        excluded_clips: &[crate::state::ClipId],
    ) -> Vec<(crate::state::ClipId, usize)> {
        let state = self.state.read().expect("state lock poisoned");
        state
            .tracks
            .iter()
            .flat_map(|track| {
                let audio = track
                    .audio
                    .clips
                    .iter()
                    .enumerate()
                    .filter_map(|(clip_idx, clip)| {
                        let clip_id = crate::state::ClipId {
                            track_idx: track.name.clone(),
                            clip_idx,
                            kind: Kind::Audio,
                        };
                        (!excluded_clips.contains(&clip_id)).then_some([
                            (clip_id.clone(), clip.start),
                            (clip_id, clip.start.saturating_add(clip.length)),
                        ])
                    });
                let midi = track
                    .midi
                    .clips
                    .iter()
                    .enumerate()
                    .filter_map(|(clip_idx, clip)| {
                        let clip_id = crate::state::ClipId {
                            track_idx: track.name.clone(),
                            clip_idx,
                            kind: Kind::MIDI,
                        };
                        (!excluded_clips.contains(&clip_id)).then_some([
                            (clip_id.clone(), clip.start),
                            (clip_id, clip.start.saturating_add(clip.length)),
                        ])
                    });
                audio.chain(midi)
            })
            .flatten()
            .collect()
    }

    fn move_clip_snap_adjust_and_target(
        &self,
        args: MoveClipSnapArgs<'_>,
    ) -> (f32, Option<crate::state::ClipId>, Vec<crate::state::ClipId>) {
        let MoveClipSnapArgs {
            kind,
            from_track_name,
            clip_index,
            offset,
            group_drag_active,
            selected_group,
            copy,
        } = args;
        // NoSnap disables all snapping. Grid modes snap to the grid; Clips
        // mode snaps to clip edges instead of the grid (edge candidates are
        // only gathered in that mode).
        if matches!(self.timing.snap_mode, crate::message::SnapMode::NoSnap) {
            return (0.0, None, Vec::new());
        }

        let excluded_clips = if !copy {
            if group_drag_active {
                selected_group.to_vec()
            } else {
                vec![crate::state::ClipId {
                    track_idx: from_track_name.to_string(),
                    clip_idx: clip_index,
                    kind,
                }]
            }
        } else {
            Vec::new()
        };
        let candidate_edges = if self.timing.clip_edge_snap_enabled() {
            self.clip_snap_edges(&excluded_clips)
        } else {
            Vec::new()
        };
        let clip_edge_snap_threshold_samples = self.clip_edge_snap_threshold_samples();
        let state = self.state.read().expect("state lock poisoned");
        let source = state
            .tracks
            .iter()
            .find(|track| track.name == from_track_name)
            .and_then(|track| match kind {
                Kind::Audio => track
                    .audio
                    .clips
                    .get(clip_index)
                    .map(|clip| (clip.start as f32, clip.length as f32)),
                Kind::MIDI => track
                    .midi
                    .clips
                    .get(clip_index)
                    .map(|clip| (clip.start as f32, clip.length as f32)),
            });
        let Some((clip_start, clip_length)) = source else {
            return (0.0, None, Vec::new());
        };
        let raw_start = clip_start + offset;
        let samples_per_beat = self.transport.samples_per_beat(&self.state);
        let samples_per_bar = self.transport.samples_per_bar(&self.state);
        let snapped_start = self.timing.snap_sample_to_bar_drag(
            raw_start,
            offset,
            samples_per_beat,
            samples_per_bar,
        ) as f32;
        let (resolved_start, snap_target, snap_targets) = Self::snapped_clip_move_start(
            raw_start,
            clip_length,
            snapped_start,
            clip_edge_snap_threshold_samples,
            candidate_edges,
        );
        let resolved_start = if raw_start < 0.0 {
            resolved_start.min(snapped_start)
        } else {
            resolved_start
        };
        (resolved_start - raw_start, snap_target, snap_targets)
    }

    /// Compute the `ClipMove` actions for dropping a drag of the selected
    /// clips. Returns `None` when the drag is not a group drag (fewer than
    /// two selected clips of this kind, or the dragged clip is not part of
    /// the selection); the caller then moves only the dragged clip.
    ///
    /// Every selected clip of the dragged kind moves by the same sample
    /// offset and the same vertical track delta as the dragged clip; clips
    /// whose computed target track is missing or incompatible stay in place.
    pub(super) fn selected_group_clip_moves(
        state: &crate::state::StateData,
        kind: Kind,
        from_track_name: &str,
        dragged_clip_index: usize,
        to_track_name: &str,
        offset_samples: f32,
        snap_adjust_samples: f32,
        input_channel: usize,
        copy: bool,
    ) -> Option<Vec<Action>> {
        let mut selected: Vec<crate::state::ClipId> = state
            .selected_clips
            .iter()
            .filter(|id| id.kind == kind)
            .cloned()
            .collect();
        let dragged_selected = selected
            .iter()
            .any(|id| id.track_idx == from_track_name && id.clip_idx == dragged_clip_index);
        if selected.len() < 2 || !dragged_selected {
            return None;
        }
        let delta = group_drag_delta(state, from_track_name, to_track_name)?;
        // Removing clips from a track shifts the indices of the clips that
        // follow them, so move each track's clips from the back; copying
        // leaves the source indices intact, so keep the ascending order.
        selected.sort_unstable_by(|a, b| match a.track_idx.cmp(&b.track_idx) {
            std::cmp::Ordering::Equal if copy => a.clip_idx.cmp(&b.clip_idx),
            std::cmp::Ordering::Equal => b.clip_idx.cmp(&a.clip_idx),
            other => other,
        });
        let mut moves = Vec::with_capacity(selected.len());
        for id in selected {
            let Some(target_name) = group_drag_target(state, kind, delta, &id.track_idx) else {
                continue;
            };
            let Some(source) = state.tracks.iter().find(|t| t.name == id.track_idx) else {
                continue;
            };
            let clip_start = match kind {
                Kind::Audio => source.audio.clips.get(id.clip_idx).map(|clip| clip.start),
                Kind::MIDI => source.midi.clips.get(id.clip_idx).map(|clip| clip.start),
            };
            let Some(start) = clip_start else {
                continue;
            };
            let sample_offset = (start as f32 + offset_samples + snap_adjust_samples)
                .max(0.0)
                .round() as usize;
            moves.push(Action::ClipMove {
                kind,
                from: ClipMoveFrom {
                    track_name: id.track_idx.clone(),
                    clip_index: id.clip_idx,
                },
                to: ClipMoveTo {
                    track_name: target_name.to_string(),
                    sample_offset,
                    input_channel,
                },
                copy,
            });
        }
        Some(moves)
    }

    pub(super) fn request_quit(&self) -> Task<Message> {
        self.send(Action::Quit)
    }

    pub(super) fn request_window_close(&mut self) -> Task<Message> {
        if self.session_ops.is_dirty() {
            self.modal = Some(Show::UnsavedChanges);
            self.state.write().expect("state lock poisoned").message =
                "Unsaved changes detected. Save, discard, or cancel.".to_string();
            Task::none()
        } else {
            self.request_quit()
        }
    }

    pub(crate) fn try_send_engine(&self, message: EngineMessage) {
        if let Err(err) = CLIENT.sender.try_send(message) {
            self.state.write().expect("state lock poisoned").message =
                format!("Engine busy; command dropped ({err})");
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        struct SyncLogOnDrop(*mut Maolan);

        impl Drop for SyncLogOnDrop {
            fn drop(&mut self) {
                unsafe {
                    (*self.0).sync_message_log_from_state();
                }
            }
        }

        let _sync_log_on_drop = SyncLogOnDrop(self as *mut _);
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
        if let Some(task) = self.handle_live_session_message(message.clone()) {
            return task;
        }
        if let Some(task) = self.handle_track_selection_message(message.clone()) {
            return task;
        }
        if let Some(task) = self.handle_plugin_message(message.clone()) {
            return task;
        }
        if let Message::AudioEditor(editor_message) = &message
            && maolan_editor::app::message_edits_document(editor_message)
        {
            self.session_ops.has_unsaved_changes = true;
        }
        if matches!(
            message,
            Message::ClipRenameShow { .. }
                | Message::ClipToggleFade { .. }
                | Message::ClipSetMuted { .. }
                | Message::ClipAssignToSessionSlot { .. }
                | Message::ClipOpenPitchCorrection { .. }
                | Message::UngroupClip { .. }
                | Message::GroupSelectedClips
        ) {
            self.state
                .write()
                .expect("state lock poisoned")
                .clip_context_menu = None;
        }
        if matches!(
            message,
            Message::TrackAutomationToggleLane { .. }
                | Message::TrackRenameShow(_)
                | Message::TrackAutomationCycleMode { .. }
                | Message::TrackTemplateSaveShow(_)
                | Message::TrackFreezeToggle { .. }
                | Message::TrackFreezeFlatten { .. }
                | Message::TrackToggleFolder { .. }
                | Message::TrackSetFolder { .. }
                | Message::TrackSetParent { .. }
                | Message::TrackMidiLearnArm { .. }
                | Message::TrackMidiLearnClear { .. }
                | Message::TrackAddReturn(_)
                | Message::TrackAddSend(_)
                | Message::Request(_)
                | Message::Show(_)
        ) {
            self.state
                .write()
                .expect("state lock poisoned")
                .track_context_menu = None;
        }
        if matches!(
            message,
            Message::SessionSlotPressed { .. }
                | Message::SessionSlotSetPlayStopIcon { .. }
                | Message::SessionScenePressed(_)
                | Message::SessionStopTrackPressed(_)
                | Message::SessionStopAllPressed
                | Message::SessionSlotDoubleClick { .. }
                | Message::SessionSlotClearRef { .. }
                | Message::SessionSlotDuplicate { .. }
                | Message::SessionSlotDragStart { .. }
                | Message::SessionSlotDropped { .. }
                | Message::SessionSlotRecord { .. }
                | Message::SessionMidiLearnArm { .. }
                | Message::SessionMidiLearnClear { .. }
                | Message::SessionSceneRenameShow(_)
                | Message::SessionSceneRemove(_)
                | Message::SessionSceneSetColor { .. }
                | Message::SessionSceneClearColor(_)
                | Message::SessionSceneSetTempo { .. }
                | Message::SessionSceneSetLaunchQuantization { .. }
        ) {
            let mut state = self.state.write().expect("state lock poisoned");
            state.session_slot_context_menu = None;
            state.session_scene_context_menu = None;
        }
        match message {
            Message::Show(ref show) => return self.handle_show_message(show),
            Message::BranchInput(_)
            | Message::BranchCreate(_)
            | Message::BranchSwitch(_)
            | Message::BranchMerge(_)
            | Message::BranchResetHard(_)
            | Message::BranchCopyTrack { .. } => return self.handle_branch_message(message),
            Message::AddTrack(crate::message::AddTrack::TrackType(_))
            | Message::AddTrack(crate::message::AddTrack::MixerSelected(_))
            | Message::AddTrack(crate::message::AddTrack::MixersDiscovered(_))
            | Message::TrackToggleFolder { .. }
            | Message::TrackSetFolder { .. }
            | Message::TrackSetParent { .. }
            | Message::TrackMidiLaneChannelSelected { .. }
            | Message::TrackSetupToggle(_)
            | Message::TrackMidiSetupChannelSelected { .. }
            | Message::MpeConfigShow { .. }
            | Message::MpeConfigSetZone { .. }
            | Message::MpeConfigSetPitchBendSensitivity { .. }
            | Message::TrackAddReturn(_)
            | Message::TrackAddSend(_)
            | Message::TrackMidiLearnArm { .. }
            | Message::TrackMidiLearnClear { .. }
            | Message::GlobalMidiLearnArm { .. }
            | Message::GlobalMidiLearnClear { .. }
            | Message::SessionMidiLearnArm { .. }
            | Message::SessionMidiLearnClear { .. }
            | Message::TrackColorChanged { .. }
            | Message::TrackColorClear(_)
            | Message::RemoveSelectedTracks
            | Message::TrackRenameShow(_)
            | Message::TrackRenameInput(_)
            | Message::TemplateSaveInput(_)
            | Message::TrackRenameConfirm
            | Message::TrackRenameCancel
            | Message::TrackTemplateSaveShow(_)
            | Message::TrackTemplateSaveInput(_)
            | Message::TrackTemplateSaveConfirm
            | Message::TrackTemplateSaveCancel
            | Message::TrackContextMenuHover { .. }
            | Message::TrackContextMenuSubmenuOpen(_)
            | Message::TrackContextMenuSubmenuClose
            | Message::TrackContextMenuToggle(_)
            | Message::TemplateSaveConfirm
            | Message::TemplateSaveCancel
            | Message::RemoveSelected
            | Message::Remove
            | Message::TrackResizeStart(_)
            | Message::TrackResizeHover(_, _)
            | Message::TrackLaneResizeStart { .. }
            | Message::TrackLaneDividerReset { .. }
            | Message::TracksResizeStart
            | Message::MixerResizeStart
            | Message::TrackDrag { .. }
            | Message::TrackDropped(_, _)
            | Message::HandleTrackZones(_)
            | Message::TrackToggleDiskMonitor { .. }
            | Message::TrackToggleInputMonitor { .. }
            | Message::TrackToggleMidiDiskMonitor { .. }
            | Message::TrackToggleMidiInputMonitor { .. } => {
                return self.handle_tracks_message(message);
            }
            Message::AddTrack(crate::message::AddTrack::Submit)
            | Message::AddTrackFromTemplate { .. }
            | Message::ApplyTemplate(crate::message::ApplyTemplate::Submit)
            | Message::ApplyTrackTemplate { .. }
            | Message::NewFromTemplate(_)
            | Message::NewSession
            | Message::Request(_)
            | Message::RequestBatch(_)
            | Message::MeterPollTick => return self.handle_session_message(message),
            Message::EscapePressed
            | Message::Cancel
            | Message::OpenUrl(_)
            | Message::ConfirmCloseSave
            | Message::ConfirmCloseDiscard
            | Message::ConfirmCloseCancel
            | Message::PreferencesSampleRateSelected(_)
            | Message::PreferencesSnapModeSelected(_)
            | Message::PreferencesMidiSnapModeSelected(_)
            | Message::PreferencesBitDepthSelected(_)
            | Message::PreferencesOscEnabledToggled(_)
            | Message::PreferencesOutputDeviceSelected(_)
            | Message::PreferencesInputDeviceSelected(_)
            | Message::PreferencesSave
            | Message::SessionMetadataAuthorInput(_)
            | Message::SessionMetadataAlbumInput(_)
            | Message::SessionMetadataYearInput(_)
            | Message::SessionMetadataTrackNumberInput(_)
            | Message::SessionMetadataGenreInput(_)
            | Message::SessionMetadataSave => return self.handle_dialog_message(message),
            Message::TransportPlay
            | Message::TransportPause
            | Message::TransportStop
            | Message::TransportPanic
            | Message::JumpToStart
            | Message::JumpToEnd
            | Message::PlaybackTick
            | Message::AutosaveSnapshotTick
            | Message::SetLoopRange(_)
            | Message::SetSessionRange(_)
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
            | Message::TapTempo
            | Message::TimeSignatureNumeratorInputChanged(_)
            | Message::TimeSignatureDenominatorInputChanged(_)
            | Message::TimeSignatureInputCommit => {
                return self.handle_timing_message(message);
            }
            Message::SetSnapMode(_)
            | Message::SetMidiSnapMode(_)
            | Message::ToggleStepRecording
            | Message::StepRecordNote { .. }
            | Message::SetClipSnapTargets(_)
            | Message::RecordingPreviewTick
            | Message::RecordingPreviewPeaksTick
            | Message::TransportRecordToggle => {
                return self.handle_transport_control_message(message);
            }
            Message::ZoomSliderChanged(_)
            | Message::TimelineZoomByScroll(_)
            | Message::EditorScrollXChanged(_)
            | Message::EditorScrollXPixels(_)
            | Message::EditorScrollYChanged(_)
            | Message::MixerScrollXChanged(_)
            | Message::TracksResizeHover(_)
            | Message::TracksFilterInput(_)
            | Message::MixerResizeHover(_)
            | Message::ShortcutsHint(_)
            | Message::ShortcutEditStart(_)
            | Message::ShortcutCaptured(_)
            | Message::ClipResizeHandleHover { hovered: true, .. }
            | Message::ClipResizeHandleHover { hovered: false, .. }
            | Message::MixerLevelEditStart(_)
            | Message::MixerLevelEditInput(_)
            | Message::MixerLevelEditCommit
            | Message::Workspace
            | Message::ToggleMixerVisibility
            | Message::ToggleTracksVisibility
            | Message::ToggleEditorVisibility
            | Message::ToggleToolbarVisibility
            | Message::X32
            | Message::Session
            | Message::ToggleLogVisibility
            | Message::ToggleShortcutsPane
            | Message::ToggleClipsPane
            | Message::ToggleModulatorsPane
            | Message::ToggleCutIndicator
            | Message::ToggleIoDelay
            | Message::AddIoDelayMeasurement
            | Message::LogViewAction(_) => return self.handle_ui_message(message),
            Message::PianoZoomXChanged(_)
            | Message::PianoTimelineZoomByScroll(_)
            | Message::PianoZoomYChanged(_)
            | Message::PianoScrollChanged { .. }
            | Message::PianoScrollXChanged(_)
            | Message::PianoScrollYChanged(_)
            | Message::PianoSysExScrollYChanged(_)
            | Message::PianoControllerLaneSelected(_)
            | Message::MidiEditorViewModeSelected(_)
            | Message::PianoControllerKindSelected(_)
            | Message::PianoVelocityKindSelected(_)
            | Message::PianoRpnKindSelected(_)
            | Message::PianoNrpnKindSelected(_)
            | Message::PianoKeyPressed(_, _)
            | Message::PianoKeyReleased(_)
            | Message::DrumKeyPressed(_, _)
            | Message::DrumKeyReleased(_)
            | Message::PianoNoteClick { .. }
            | Message::PianoNotesDrag { .. }
            | Message::PianoNotesEndDrag
            | Message::PitchCorrectionPointClick { .. }
            | Message::PitchCorrectionSnapToNearest { .. }
            | Message::PitchCorrectionPointsDrag { .. }
            | Message::PitchCorrectionPointsEndDrag
            | Message::PitchCorrectionSelectRectStart { .. }
            | Message::PitchCorrectionSelectRectDrag { .. }
            | Message::PitchCorrectionSelectRectEnd
            | Message::PitchCorrectionClearSelection
            | Message::SelectAll
            | Message::PitchCorrectionFrameLikenessChanged(_)
            | Message::PitchCorrectionInertiaChanged(_)
            | Message::PitchCorrectionFormantCompensationChanged(_)
            | Message::PitchCorrectionDetectorChanged(_)
            | Message::PitchCorrectionModeChanged(_)
            | Message::ClipPitchCorrectionResynthFinished { .. }
            | Message::PianoNoteResizeStart { .. }
            | Message::PianoNoteResizeDrag { .. }
            | Message::PianoNoteResizeEnd
            | Message::PianoAdjustVelocity { .. }
            | Message::PianoSetVelocity { .. }
            | Message::PianoAdjustController { .. }
            | Message::PianoSetControllerValue { .. }
            | Message::PianoInsertControllers { .. }
            | Message::PianoSysExSelect(_)
            | Message::PianoSysExOpenEditor(_)
            | Message::PianoSysExCloseEditor
            | Message::PianoSysExHexInput(_)
            | Message::PianoSysExAdd
            | Message::PianoSysExUpdate
            | Message::PianoSysExDelete
            | Message::PianoSysExMove { .. }
            | Message::PianoSelectRectStart { .. }
            | Message::PianoSelectRectDrag { .. }
            | Message::PianoSelectRectEnd
            | Message::PianoCreateNoteStart { .. }
            | Message::PianoCreateNoteDrag { .. }
            | Message::PianoCreateNoteEnd { .. }
            | Message::PianoDeleteSelectedNotes
            | Message::PianoDeleteNotes { .. }
            | Message::DrumNoteSelected(_)
            | Message::DrumNoteCreate { .. }
            | Message::DrumNoteDelete(_)
            | Message::DrumNoteMove { .. }
            | Message::DrumSelectRectStart { .. }
            | Message::DrumSelectRectDrag { .. }
            | Message::DrumSelectRectEnd
            | Message::PianoDeleteControllers { .. }
            | Message::PianoSetMpeValue { .. }
            | Message::PianoInsertMpePoints { .. }
            | Message::PianoDeleteMpePoints { .. }
            | Message::PianoQuantizeSelectedNotes
            | Message::PianoScaleSelectedNotes
            | Message::PianoChordSelectedNotes
            | Message::PianoLegatoSelectedNotes
            | Message::PianoVelocityShapeSelectedNotes
            | Message::PianoHumanizeSelectedNotes
            | Message::PianoGrooveSelectedNotes
            | Message::PianoHumanizeTimeAmountChanged(_)
            | Message::PianoHumanizeVelocityAmountChanged(_)
            | Message::PianoGrooveAmountChanged(_)
            | Message::PianoScaleRootSelected(_)
            | Message::PianoScaleMinorToggled(_)
            | Message::PianoShowNoteNames(_)
            | Message::PianoChordKindSelected(_)
            | Message::PianoVelocityShapeAmountChanged(_)
            | Message::MidiClipPreviewLoaded { .. }
            | Message::OpenMidiPiano { .. } => return self.handle_piano_message(message),
            Message::Response(Ok(_))
            | Message::Response(Err(_))
            | Message::EngineEvent(_)
            | Message::EngineQueryReply(_) => {
                return self.handle_response_message(message);
            }
            Message::TrackFreezeToggle { .. }
            | Message::TrackFreezePrepared { .. }
            | Message::TrackFreezeFlatten { .. }
            | Message::OpenExporter
            | Message::MidiLearnMappingsPanelToggle
            | Message::MidiLearnMappingsReportRequest
            | Message::MidiLearnMappingsExportRequest
            | Message::MidiLearnMappingsImportRequest
            | Message::MidiLearnMappingsClearAllRequest
            | Message::ExportSettingsConfirm
            | Message::ExportFileSelected(Some(_))
            | Message::ExportFileSelected(None)
            | Message::ExportProgress { .. } => return self.handle_export_message(message),
            Message::TrackAutomationToggleLane { .. }
            | Message::TrackAutomationCycleMode { .. }
            | Message::TrackAutomationAddPluginLanes { .. }
            | Message::TrackAutomationLaneInsertPoints { .. }
            | Message::TrackAutomationLaneDeletePoint { .. } => {
                return self.handle_automation_message(message);
            }
            Message::ConnectionViewSelectTrack(_)
            | Message::ConnectionViewDeselectAll
            | Message::ConnectionPositionsChanged
            | Message::ConnectionViewSelectConnection(_)
            | Message::Connections
            | Message::OpenTrackPlugins(_)
            | Message::OpenFolderConnections(_)
            | Message::SessionViewConnectionsOpen(_)
            | Message::SessionViewConnectionsClose
            | Message::EditorConnectionsOpen(_)
            | Message::EditorConnectionsClose
            | Message::OpenJackConnections
            | Message::CloseJackConnections
            | Message::JackPortClick { .. }
            | Message::JackDisconnect { .. }
            | Message::OpenHwPorts { .. } => return self.handle_connections_message(message),
            Message::PluginGraphControllerMenuOpen { .. }
            | Message::PluginGraphControllerMenuClose
            | Message::PluginGraphControllerMenuHover(_)
            | Message::PluginGraphShowController { .. }
            | Message::PluginGraphHideController { .. }
            | Message::ToggleSelectedPluginBypass
            | Message::OpenClipPlugins {
                track_idx: _,
                clip_idx: _,
            } => return self.handle_plugin_message_graph(message),
            Message::MarkerLaneCreate { .. }
            | Message::MarkerLaneCreateAtPlayhead
            | Message::MarkerLaneEdit { .. }
            | Message::MarkerNameInput(_)
            | Message::MarkerNameConfirm
            | Message::MarkerNameCancel
            | Message::SelectClip { .. }
            | Message::ClipRenameShow { .. }
            | Message::ClipRenameInput(_)
            | Message::ClipRenameConfirm
            | Message::ClipRenameCancel
            | Message::ClipToggleFade { .. }
            | Message::ClipSetMuted { .. }
            | Message::ClipReverse { .. }
            | Message::ClipAssignToSessionSlot { .. }
            | Message::GroupSelectedClips
            | Message::UngroupClip { .. }
            | Message::ClipOpenPitchCorrection { .. }
            | Message::ClipExportPitchCorrectionMidi { .. }
            | Message::ClipPitchCorrectionMidiFileSelected { path: Some(_), .. }
            | Message::ClipPitchCorrectionMidiFileSelected { path: None, .. }
            | Message::ClipOpenPitchCorrectionProgress { .. }
            | Message::ClipStretchFinished { .. }
            | Message::ClipOpenPitchCorrectionFinished { .. }
            | Message::MousePressed(_)
            | Message::ClipResizeStart(_, _, _, _)
            | Message::FadeResizeStart { .. }
            | Message::MouseMoved(mouse::Event::CursorMoved { .. })
            | Message::EditorMouseMoved(_)
            | Message::MouseReleased
            | Message::ClipDrag(_)
            | Message::HandleClipZones(_)
            | Message::HandleClipPreviewZones(_)
            | Message::PaneClipDragStart { .. }
            | Message::PaneClipDropped { .. }
            | Message::HandlePaneClipDropZones(_) => return self.handle_clips_message(message),
            Message::DeselectAll | Message::DeselectClips => {
                return self.handle_selection_message(message);
            }
            Message::OpenFileImporter
            | Message::DeleteUnusedSessionMediaFiles
            | Message::CollectToSession
            | Message::ImportFilesSelected(Some(_))
            | Message::ImportFilesSelected(None)
            | Message::ImportProgress { .. }
            | Message::ImportFinished { .. }
            | Message::ImportPreparedAudioPeaks { .. }
            | Message::TrackTemplatesLoaded(_, _)
            | Message::PreferencesDevicesLoaded { .. }
            | Message::DrainAudioPeakUpdates => {
                return self.handle_session_io_extended_message(message);
            }
            Message::GenerateAudioModelSelected(_)
            | Message::GenerateAudioAceStepLmSelected(_)
            | Message::GenerateAudioPromptAction(_)
            | Message::GenerateAudioTagsInput(_)
            | Message::GenerateAudioBackendSelected(_)
            | Message::GenerateAudioKeyRootChanged(_)
            | Message::GenerateAudioKeyModeChanged(_)
            | Message::GenerateAudioCfgScaleInput(_)
            | Message::GenerateAudioStepsInput(_)
            | Message::GenerateAudioSecondsTotalInput(_)
            | Message::GenerateAudioCancel
            | Message::GenerateAudioSubmit
            | Message::GenerateAudioProgress { .. }
            | Message::GenerateAudioFinished(_)
            | Message::GenerateMidiModelSelected(_)
            | Message::GenerateMidiPromptAction(_)
            | Message::GenerateMidiBackendSelected(_)
            | Message::GenerateMidiKeyRootChanged(_)
            | Message::GenerateMidiKeyModeChanged(_)
            | Message::GenerateMidiBpmInput(_)
            | Message::GenerateMidiTimeSignatureNumInput(_)
            | Message::GenerateMidiTimeSignatureDenomInput(_)
            | Message::GenerateMidiLengthSecondsInput(_)
            | Message::GenerateMidiMaxTokensInput(_)
            | Message::GenerateMidiTopPInput(_)
            | Message::GenerateMidiSeedInput(_)
            | Message::GenerateMidiCancel
            | Message::GenerateMidiSubmit
            | Message::GenerateMidiProgress { .. }
            | Message::GenerateMidiFinished(_) => return self.handle_generate_message(message),
            Message::HwMixer(msg) => {
                return mixosc::app::update(&mut self.hw_mixer, msg).map(Message::HwMixer);
            }
            Message::AudioEditor(maolan_editor::app::Message::Close)
            | Message::AudioEditor(
                maolan_editor::app::Message::Save | maolan_editor::app::Message::SaveAs,
            )
            | Message::AudioEditor(maolan_editor::app::Message::Play)
            | Message::AudioEditor(maolan_editor::app::Message::TogglePlayback)
            | Message::AudioEditor(maolan_editor::app::Message::Stop)
            | Message::AudioEditor(_)
            | Message::OpenAudioEditor { .. }
            | Message::AudioEditorBufferLoaded { .. } => {
                return self.handle_audio_editor_message(message);
            }
            Message::ModulatorAdd
            | Message::ModulatorRemove(_)
            | Message::ModulatorSelect(_)
            | Message::ModulatorToggleTarget { .. }
            | Message::ModulatorToggleSelectedTarget { .. }
            | Message::ModulatorUpdate { .. }
            | Message::ModulatorTargetShow { .. }
            | Message::ModulatorTargetMinInput(_)
            | Message::ModulatorTargetMaxInput(_)
            | Message::ModulatorTargetConfirm
            | Message::ModulatorTargetCancel
            | Message::ModulatorTargetRemove { .. } => {
                return self.handle_modulators_message(message);
            }
            _ => {}
        }
        self.update_children(&message);
        Task::none()
    }

    fn handle_step_record_note(&mut self, channel: u8, pitch: u8, velocity: u8) -> Task<Message> {
        let piano = match self
            .state
            .read()
            .expect("state lock poisoned")
            .piano
            .as_ref()
        {
            Some(p) => p.clone(),
            None => return Task::none(),
        };
        let track_name = piano.track_idx.clone();
        let clip_idx = piano.clip_index;
        let clip_length = piano.clip_length_samples;
        let insert_idx = piano.notes.len();

        let samples_per_beat = self.transport.samples_per_beat(&self.state);
        let samples_per_bar = self.transport.samples_per_bar(&self.state);
        let interval = match self.timing.midi_snap_mode {
            SnapMode::NoSnap | SnapMode::Clips => (samples_per_beat / 4.0).max(1.0) as usize,
            mode => mode
                .interval_samples(samples_per_beat, samples_per_bar)
                .max(1.0) as usize,
        };

        let start_sample = self
            .transport
            .step_recording_cursor_samples
            .clamp(0, clip_length);
        let end_sample = start_sample.saturating_add(interval).min(clip_length);
        if end_sample <= start_sample {
            return Task::none();
        }

        let note = maolan_engine::message::MidiNoteData {
            start_sample,
            length_samples: end_sample.saturating_sub(start_sample),
            pitch,
            velocity,
            channel,
            mpe: Default::default(),
        };

        self.transport.step_recording_cursor_samples = end_sample;

        self.send(Action::InsertMidiNotes {
            track_name,
            clip_index: clip_idx,
            notes: vec![(insert_idx, note)],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use maolan_widgets::iced::Point;
    use serde_json::json;

    fn audio_track(name: &str, clip_starts: &[usize]) -> crate::state::Track {
        let mut track = crate::state::Track::new(name.to_string(), 0.0, 2, 2, 0, 0);
        for &start in clip_starts {
            track.audio.clips.push(crate::state::AudioClip {
                name: format!("{name}.wav"),
                start,
                length: 100,
                offset: 0,
                ..Default::default()
            });
        }
        track
    }

    fn select(state: &mut crate::state::StateData, track: &str, clip: usize, kind: Kind) {
        state.selected_clips.insert(crate::state::ClipId {
            track_idx: track.to_string(),
            clip_idx: clip,
            kind,
        });
    }

    fn move_target(m: &Action) -> (&str, &str, usize) {
        let Action::ClipMove { from, to, .. } = m else {
            panic!("expected ClipMove");
        };
        (&from.track_name, &to.track_name, to.sample_offset)
    }

    #[test]
    fn group_drag_horizontal_moves_all_selected_tracks() {
        let mut state = crate::state::StateData::default();
        state.tracks.push(audio_track("a", &[1000]));
        state.tracks.push(audio_track("b", &[2000]));
        state.tracks.push(audio_track("c", &[3000]));
        select(&mut state, "a", 0, Kind::Audio);
        select(&mut state, "c", 0, Kind::Audio);

        let moves = Maolan::selected_group_clip_moves(
            &state,
            Kind::Audio,
            "a",
            0,
            "a",
            480.0,
            0.0,
            0,
            false,
        )
        .expect("multi-track selection is a group drag");

        assert_eq!(moves.len(), 2);
        let mut targets = moves.iter().map(move_target).collect::<Vec<_>>();
        targets.sort();
        assert_eq!(
            targets,
            vec![("a", "a", 1480), ("c", "c", 3480)],
            "each clip moves by the same offset within its own track"
        );
    }

    #[test]
    fn group_drag_shifts_members_by_track_delta() {
        let mut state = crate::state::StateData::default();
        state.tracks.push(audio_track("a", &[1000]));
        state.tracks.push(audio_track("b", &[2000]));
        state.tracks.push(audio_track("c", &[3000]));
        select(&mut state, "a", 0, Kind::Audio);
        select(&mut state, "c", 0, Kind::Audio);

        // Drop the dragged clip one track down; the member on "c" would land
        // past the last track and must stay in place.
        let moves =
            Maolan::selected_group_clip_moves(&state, Kind::Audio, "a", 0, "b", 0.0, 0.0, 0, false)
                .expect("multi-track selection is a group drag");

        assert_eq!(moves.len(), 1);
        assert_eq!(move_target(&moves[0]), ("a", "b", 1000));
    }

    #[test]
    fn group_drag_skips_incompatible_target_tracks() {
        let mut state = crate::state::StateData::default();
        state.tracks.push(audio_track("a", &[1000]));
        // "b" has a different audio channel count and cannot accept the clips.
        let mut mismatched = crate::state::Track::new("b".to_string(), 0.0, 1, 1, 0, 0);
        mismatched.audio.clips.push(crate::state::AudioClip {
            name: "b.wav".to_string(),
            start: 2000,
            length: 100,
            offset: 0,
            ..Default::default()
        });
        state.tracks.push(mismatched);
        state.tracks.push(audio_track("c", &[3000]));
        select(&mut state, "a", 0, Kind::Audio);
        select(&mut state, "c", 0, Kind::Audio);

        // Drag down one track: "a" -> "b" is incompatible (skipped), while
        // "c" -> "d" does not exist (out of range, skipped).
        let moves =
            Maolan::selected_group_clip_moves(&state, Kind::Audio, "a", 0, "b", 0.0, 0.0, 0, false)
                .expect("multi-track selection is a group drag");
        assert!(
            moves.is_empty(),
            "no clip has a compatible target, all stay in place"
        );
    }

    #[test]
    fn group_drag_removes_clips_back_to_front_per_track() {
        let mut state = crate::state::StateData::default();
        state.tracks.push(audio_track("a", &[1000, 5000]));
        state.tracks.push(audio_track("b", &[2000]));
        select(&mut state, "a", 0, Kind::Audio);
        select(&mut state, "a", 1, Kind::Audio);
        select(&mut state, "b", 0, Kind::Audio);

        let moves = Maolan::selected_group_clip_moves(
            &state,
            Kind::Audio,
            "a",
            0,
            "a",
            480.0,
            0.0,
            0,
            false,
        )
        .expect("same-track multi-selection is a group drag");

        // Without copying, each track's clips must move in descending index
        // order so the removals do not shift the later indices.
        let order = moves
            .iter()
            .map(|m| match m {
                Action::ClipMove { from, .. } => (from.track_name.as_str(), from.clip_index),
                _ => unreachable!(),
            })
            .collect::<Vec<_>>();
        assert_eq!(order, vec![("a", 1), ("a", 0), ("b", 0)]);
        assert!(
            moves
                .iter()
                .all(|m| !matches!(m, Action::ClipMove { copy: true, .. }))
        );
    }

    #[test]
    fn group_drag_copy_keeps_ascending_order() {
        let mut state = crate::state::StateData::default();
        state.tracks.push(audio_track("a", &[1000, 5000]));
        select(&mut state, "a", 0, Kind::Audio);
        select(&mut state, "a", 1, Kind::Audio);

        let moves = Maolan::selected_group_clip_moves(
            &state,
            Kind::Audio,
            "a",
            0,
            "a",
            480.0,
            0.0,
            0,
            true,
        )
        .expect("same-track multi-selection is a group drag");

        let order = moves
            .iter()
            .map(|m| match m {
                Action::ClipMove { from, .. } => from.clip_index,
                _ => unreachable!(),
            })
            .collect::<Vec<_>>();
        assert_eq!(order, vec![0, 1]);
        assert!(
            moves
                .iter()
                .all(|m| matches!(m, Action::ClipMove { copy: true, .. }))
        );
    }

    #[test]
    fn single_selection_and_unselected_drag_are_not_group_drags() {
        let mut state = crate::state::StateData::default();
        state.tracks.push(audio_track("a", &[1000]));
        state.tracks.push(audio_track("b", &[2000]));
        select(&mut state, "a", 0, Kind::Audio);
        select(&mut state, "b", 0, Kind::Audio);

        // Only one selected clip of this kind.
        let mut single = crate::state::StateData::default();
        single.tracks.push(audio_track("a", &[1000]));
        single.tracks.push(audio_track("b", &[2000]));
        select(&mut single, "a", 0, Kind::Audio);
        assert!(
            Maolan::selected_group_clip_moves(
                &single,
                Kind::Audio,
                "a",
                0,
                "b",
                0.0,
                0.0,
                0,
                false
            )
            .is_none()
        );

        // Dragging a clip that is not part of the selection.
        assert!(
            Maolan::selected_group_clip_moves(&state, Kind::Audio, "a", 0, "b", 0.0, 0.0, 0, false)
                .is_some()
        );
        let mut unselected = crate::state::StateData::default();
        unselected.tracks.push(audio_track("a", &[1000]));
        unselected.tracks.push(audio_track("b", &[2000]));
        select(&mut unselected, "b", 0, Kind::Audio);
        assert!(
            Maolan::selected_group_clip_moves(
                &unselected,
                Kind::Audio,
                "a",
                0,
                "b",
                0.0,
                0.0,
                0,
                false
            )
            .is_none(),
            "dragging an unselected clip is not a group drag"
        );
    }

    #[test]
    fn clip_resize_group_requires_multi_selection_of_dragged_clip() {
        // Dragging an unselected clip is a single-clip resize even when
        // other clips are selected.
        let mut unselected = crate::state::StateData::default();
        unselected.tracks.push(audio_track("a", &[1000]));
        unselected.tracks.push(audio_track("b", &[2000, 3000]));
        select(&mut unselected, "b", 0, Kind::Audio);
        select(&mut unselected, "b", 1, Kind::Audio);
        assert!(clip_resize_group(&unselected, Kind::Audio, "a", 0).is_empty());

        // A lone selected clip is a single-clip resize.
        let mut single = crate::state::StateData::default();
        single.tracks.push(audio_track("a", &[1000]));
        single.tracks.push(audio_track("b", &[2000]));
        select(&mut single, "a", 0, Kind::Audio);
        assert!(clip_resize_group(&single, Kind::Audio, "a", 0).is_empty());

        // Multi-selection across tracks resizes together.
        let mut multi = crate::state::StateData::default();
        multi.tracks.push(audio_track("a", &[1000]));
        multi.tracks.push(audio_track("b", &[2000]));
        select(&mut multi, "a", 0, Kind::Audio);
        select(&mut multi, "b", 0, Kind::Audio);
        let mut group = clip_resize_group(&multi, Kind::Audio, "a", 0);
        group.sort_by(|x, y| x.clip.track_idx.cmp(&y.clip.track_idx));
        assert_eq!(group.len(), 2);
        assert_eq!(group[0].clip.track_idx, "a");
        assert_eq!(group[0].initial_start, 1000);
        assert_eq!(group[1].clip.track_idx, "b");
        assert_eq!(group[1].initial_start, 2000);

        // Clips of the other kind and locked clips stay out of the group.
        let mut mixed = crate::state::StateData::default();
        mixed.tracks.push(audio_track("a", &[1000]));
        let mut locked_track = audio_track("b", &[2000]);
        locked_track.audio.clips[0].take_lane_locked = true;
        mixed.tracks.push(locked_track);
        select(&mut mixed, "a", 0, Kind::Audio);
        select(&mut mixed, "b", 0, Kind::Audio);
        let group = clip_resize_group(&mixed, Kind::Audio, "a", 0);
        assert!(
            group.is_empty(),
            "the only other selected clip is locked, so this is a single resize"
        );
    }

    #[test]
    fn midi_group_drag_uses_midi_clips() {
        let mut state = crate::state::StateData::default();
        let midi_track = |name: &str, start: usize| {
            let mut track = crate::state::Track::new(name.to_string(), 0.0, 0, 0, 1, 1);
            track.midi.clips.push(crate::state::MIDIClip {
                name: format!("{name}.midi"),
                start,
                length: 100,
                offset: 0,
                ..Default::default()
            });
            track
        };
        state.tracks.push(midi_track("m1", 1000));
        state.tracks.push(midi_track("m2", 2000));
        select(&mut state, "m1", 0, Kind::MIDI);
        select(&mut state, "m2", 0, Kind::MIDI);

        let moves = Maolan::selected_group_clip_moves(
            &state,
            Kind::MIDI,
            "m1",
            0,
            "m1",
            480.0,
            0.0,
            0,
            false,
        )
        .expect("multi-track midi selection is a group drag");

        let mut targets = moves.iter().map(move_target).collect::<Vec<_>>();
        targets.sort();
        assert_eq!(targets, vec![("m1", "m1", 1480), ("m2", "m2", 2480)]);
    }

    #[test]
    fn plugin_graph_refresh_preserves_cached_plugin_state() {
        use maolan_engine::message::{PluginGraphNode, PluginGraphPlugin};

        fn plugin(instance_id: usize, state: Option<serde_json::Value>) -> PluginGraphPlugin {
            PluginGraphPlugin {
                node: PluginGraphNode::ClapPluginInstance(instance_id),
                instance_id,
                format: "CLAP".to_string(),
                uri: "/tmp/maolan_plugins.clap".to_string(),
                plugin_id: "rs.maolan.kick".to_string(),
                name: "Maolan Kick".to_string(),
                main_audio_inputs: 0,
                main_audio_outputs: 2,
                audio_inputs: 0,
                audio_outputs: 2,
                midi_inputs: 1,
                midi_outputs: 0,
                state,
                bypassed: false,
            }
        }

        let mut state = crate::state::StateData::default();
        state.plugin_graphs_by_track.insert(
            "Kick".to_string(),
            (vec![plugin(7, Some(json!({"bytes": [1, 2, 3]})))], vec![]),
        );
        let mut refreshed = vec![plugin(7, None)];

        Maolan::preserve_plugin_graph_states_from_cache("Kick", &state, &mut refreshed);

        assert_eq!(refreshed[0].state, Some(json!({"bytes": [1, 2, 3]})));
    }

    #[test]
    fn active_workspace_cursor_prefers_editor_cursor() {
        let app = Maolan {
            state: {
                let state = crate::state::State::default();
                {
                    let mut guard = state.write().expect("state lock poisoned");
                    guard.cursor = Point::new(10.0, 20.0);
                    guard.editor_cursor = Some(Point::new(30.0, 40.0));
                }
                state
            },
            ..Maolan::default()
        };

        assert_eq!(app.active_workspace_cursor(), Point::new(30.0, 40.0));
    }

    #[test]
    fn active_workspace_cursor_falls_back_to_global_cursor() {
        let app = Maolan {
            state: {
                let state = crate::state::State::default();
                state.write().expect("state lock poisoned").cursor = Point::new(10.0, 20.0);
                state
            },
            ..Maolan::default()
        };

        assert_eq!(app.active_workspace_cursor(), Point::new(10.0, 20.0));
    }

    #[test]
    fn nearest_clip_edge_sample_prefers_candidate_within_threshold() {
        let (snapped, target, targets) = Maolan::nearest_clip_edge_sample(
            98.0,
            96.0,
            4.0,
            [("a", 70usize), ("b", 100usize), ("c", 130usize)]
                .into_iter()
                .map(|(name, edge)| {
                    (
                        crate::state::ClipId {
                            track_idx: name.to_string(),
                            clip_idx: 0,
                            kind: Kind::Audio,
                        },
                        edge,
                    )
                }),
        );
        assert_eq!(snapped, 100.0);
        assert_eq!(target.unwrap().track_idx, "b");
        assert_eq!(targets.len(), 1);
    }

    #[test]
    fn nearest_clip_edge_sample_falls_back_to_grid_when_no_edge_is_close() {
        let (snapped, target, targets) = Maolan::nearest_clip_edge_sample(
            98.0,
            96.0,
            1.0,
            [("a", 70usize), ("b", 100usize), ("c", 130usize)]
                .into_iter()
                .map(|(name, edge)| {
                    (
                        crate::state::ClipId {
                            track_idx: name.to_string(),
                            clip_idx: 0,
                            kind: Kind::Audio,
                        },
                        edge,
                    )
                }),
        );
        assert_eq!(snapped, 96.0);
        assert!(target.is_none());
        assert!(targets.is_empty());
    }

    #[test]
    fn snapped_clip_move_start_can_snap_using_right_edge() {
        let (snapped, target, targets) = Maolan::snapped_clip_move_start(
            70.0,
            20.0,
            64.0,
            2.0,
            std::iter::once((
                crate::state::ClipId {
                    track_idx: "target".to_string(),
                    clip_idx: 0,
                    kind: Kind::Audio,
                },
                90usize,
            )),
        );
        assert_eq!(snapped, 70.0);
        assert_eq!(target.unwrap().track_idx, "target");
        assert_eq!(targets.len(), 1);
    }

    #[test]
    fn snapped_clip_move_start_uses_clip_edge_over_grid_when_close() {
        let (snapped, target, targets) = Maolan::snapped_clip_move_start(
            91.0,
            12.0,
            96.0,
            2.0,
            [("left", 90usize), ("right", 140usize)]
                .into_iter()
                .map(|(name, edge)| {
                    (
                        crate::state::ClipId {
                            track_idx: name.to_string(),
                            clip_idx: 0,
                            kind: Kind::Audio,
                        },
                        edge,
                    )
                }),
        );
        assert_eq!(snapped, 90.0);
        assert_eq!(target.unwrap().track_idx, "left");
        assert_eq!(targets.len(), 1);
    }

    #[test]
    fn snapped_clip_move_start_does_not_snap_right_when_dragging_past_session_start() {
        let raw_start = -8.0;
        let snapped_start = 0.0;
        let (resolved_start, _target, _targets) = Maolan::snapped_clip_move_start(
            raw_start,
            100.0,
            snapped_start,
            20.0,
            std::iter::once((
                crate::state::ClipId {
                    track_idx: "other".to_string(),
                    clip_idx: 0,
                    kind: Kind::Audio,
                },
                105usize,
            )),
        );

        assert!(resolved_start > 0.0);
        let clamped_start = if raw_start < 0.0 {
            resolved_start.min(snapped_start)
        } else {
            resolved_start
        };
        assert_eq!(clamped_start, 0.0);
    }

    #[test]
    fn generate_audio_progress_message_uses_normalized_scale() {
        let mut app = Maolan {
            generate: GenerateState {
                generate_audio_in_progress: true,
                ..Default::default()
            },
            ..Maolan::default()
        };

        let _ = app.update(Message::GenerateAudioProgress {
            progress: 0.42,
            operation: Some("Generating".to_string()),
        });

        assert!((app.generate.generate_audio_progress - 0.42).abs() < f32::EPSILON);
        assert_eq!(
            app.generate.generate_audio_operation.as_deref(),
            Some("Generating")
        );
    }

    #[test]
    fn modulator_target_confirm_adds_volume_target() {
        let mut app = Maolan {
            modulators: vec![crate::state::Modulator::new(1)],
            ..Maolan::default()
        };
        app.modulator_target_dialog
            .open(crate::state::ModulatorTargetDialog {
                modulator_id: 1,
                track_name: "Drums".to_string(),
                target: crate::message::TrackAutomationTarget::Volume,
                min_input: "-90".to_string(),
                max_input: "20".to_string(),
                existing: false,
            });

        let _ = app.update(Message::ModulatorTargetConfirm);

        let m = app.modulators.iter().find(|m| m.id == 1).unwrap();
        assert_eq!(m.targets.len(), 1);
        assert_eq!(m.targets[0].track_name, "Drums");
        assert_eq!(
            m.targets[0].target,
            crate::message::TrackAutomationTarget::Volume
        );
        assert!((m.targets[0].min - -90.0).abs() < f32::EPSILON);
        assert!((m.targets[0].max - 20.0).abs() < f32::EPSILON);
        assert!(!app.modulator_target_dialog.is_open());
    }

    fn app_with_drum_notes(notes: Vec<crate::state::PianoNote>) -> Maolan {
        let state = crate::state::State::default();
        {
            let mut guard = state.write().expect("state lock poisoned");
            guard.view = crate::state::View::Piano;
            guard.piano = Some(crate::state::PianoData {
                track_idx: "Drums".to_string(),
                clip_index: 0,
                clip_start_samples: 0,
                clip_length_samples: 1000,
                notes,
                controllers: Vec::new(),
                sysexes: Vec::new(),
                midnam_note_names: std::collections::HashMap::new(),
            });
            guard.piano_selected_notes = std::collections::HashSet::new();
        }
        Maolan {
            state,
            ..Maolan::default()
        }
    }

    #[test]
    fn piano_fit_clip_zoom_x_fits_clip_to_midi_viewport() {
        let app = Maolan {
            size: maolan_widgets::iced::Size::new(1200.0, 800.0),
            ui: UiState {
                zoom_visible_bars: 127.0,
                ..Default::default()
            },
            ..Maolan::default()
        };
        let clip_length_samples = 960_000;
        let zoom_x = app.piano_fit_clip_zoom_x(clip_length_samples);
        let rendered_width = clip_length_samples as f32 * app.pixels_per_sample() * zoom_x;

        assert!(zoom_x >= H_ZOOM_MIN);
        assert!(zoom_x <= H_ZOOM_MAX);
        assert!(rendered_width <= app.piano_timeline_viewport_width() + 0.5);
    }

    #[test]
    fn select_all_in_piano_view_selects_all_edited_clip_notes() {
        let mut app = app_with_drum_notes(vec![
            crate::state::PianoNote {
                start_sample: 100,
                length_samples: 10,
                pitch: 36,
                velocity: 100,
                channel: 0,
                mpe: Default::default(),
            },
            crate::state::PianoNote {
                start_sample: 200,
                length_samples: 10,
                pitch: 38,
                velocity: 100,
                channel: 0,
                mpe: Default::default(),
            },
            crate::state::PianoNote {
                start_sample: 300,
                length_samples: 10,
                pitch: 42,
                velocity: 100,
                channel: 0,
                mpe: Default::default(),
            },
        ]);
        app.state
            .write()
            .expect("state lock poisoned")
            .piano_selected_notes = [1].iter().copied().collect();

        let _ = app.update(Message::SelectAll);

        let state = app.state.read().expect("state lock poisoned");
        assert_eq!(state.piano_selected_notes.len(), 3);
        assert!(state.piano_selected_notes.contains(&0));
        assert!(state.piano_selected_notes.contains(&1));
        assert!(state.piano_selected_notes.contains(&2));
    }

    #[test]
    fn midi_insert_response_sorts_open_editor_notes_like_engine() {
        let mut app = app_with_drum_notes(vec![
            crate::state::PianoNote {
                start_sample: 200,
                length_samples: 10,
                pitch: 64,
                velocity: 100,
                channel: 0,
                mpe: Default::default(),
            },
            crate::state::PianoNote {
                start_sample: 400,
                length_samples: 10,
                pitch: 67,
                velocity: 100,
                channel: 0,
                mpe: Default::default(),
            },
        ]);

        let _ = app.update(Message::Response(Ok(
            maolan_engine::message::Action::InsertMidiNotes {
                track_name: "Drums".to_string(),
                clip_index: 0,
                notes: vec![(
                    2,
                    maolan_engine::message::MidiNoteData {
                        start_sample: 100,
                        length_samples: 10,
                        pitch: 60,
                        velocity: 100,
                        channel: 0,
                        mpe: Default::default(),
                    },
                )],
            },
        )));

        let state = app.state.read().expect("state lock poisoned");
        let piano = state.piano.as_ref().unwrap();
        let starts = piano
            .notes
            .iter()
            .map(|note| note.start_sample)
            .collect::<Vec<_>>();
        assert_eq!(starts, vec![100, 200, 400]);
    }

    #[test]
    fn midi_modify_response_remaps_selection_after_sorting_open_editor_notes() {
        let mut app = app_with_drum_notes(vec![
            crate::state::PianoNote {
                start_sample: 100,
                length_samples: 10,
                pitch: 60,
                velocity: 100,
                channel: 0,
                mpe: Default::default(),
            },
            crate::state::PianoNote {
                start_sample: 200,
                length_samples: 10,
                pitch: 64,
                velocity: 100,
                channel: 0,
                mpe: Default::default(),
            },
        ]);
        app.state
            .write()
            .expect("state lock poisoned")
            .piano_selected_notes = [1].iter().copied().collect();

        let moved_note = maolan_engine::message::MidiNoteData {
            start_sample: 50,
            length_samples: 10,
            pitch: 64,
            velocity: 100,
            channel: 0,
            mpe: Default::default(),
        };
        let _ = app.update(Message::Response(Ok(
            maolan_engine::message::Action::ModifyMidiNotes {
                track_name: "Drums".to_string(),
                clip_index: 0,
                note_indices: vec![1],
                new_notes: vec![moved_note],
                old_notes: Vec::new(),
            },
        )));

        let state = app.state.read().expect("state lock poisoned");
        let piano = state.piano.as_ref().unwrap();
        let starts = piano
            .notes
            .iter()
            .map(|note| note.start_sample)
            .collect::<Vec<_>>();
        assert_eq!(starts, vec![50, 100]);
        assert_eq!(state.piano_selected_notes.len(), 1);
        assert!(state.piano_selected_notes.contains(&0));
    }

    #[test]
    fn drum_click_on_selected_note_preserves_multi_selection() {
        let mut app = app_with_drum_notes(vec![
            crate::state::PianoNote {
                start_sample: 100,
                length_samples: 10,
                pitch: 36,
                velocity: 100,
                channel: 0,
                mpe: Default::default(),
            },
            crate::state::PianoNote {
                start_sample: 200,
                length_samples: 10,
                pitch: 38,
                velocity: 100,
                channel: 0,
                mpe: Default::default(),
            },
        ]);
        app.state
            .write()
            .expect("state lock poisoned")
            .piano_selected_notes = [0, 1].iter().copied().collect();

        let _ = app.update(Message::DrumNoteSelected(0));

        let state = app.state.read().expect("state lock poisoned");
        assert_eq!(state.piano_selected_notes.len(), 2);
        assert!(state.piano_selected_notes.contains(&0));
        assert!(state.piano_selected_notes.contains(&1));
    }

    #[test]
    fn drum_note_move_moves_all_selected_notes() {
        let mut app = app_with_drum_notes(vec![
            crate::state::PianoNote {
                start_sample: 100,
                length_samples: 10,
                pitch: 36,
                velocity: 100,
                channel: 0,
                mpe: Default::default(),
            },
            crate::state::PianoNote {
                start_sample: 200,
                length_samples: 10,
                pitch: 38,
                velocity: 100,
                channel: 0,
                mpe: Default::default(),
            },
        ]);
        app.state
            .write()
            .expect("state lock poisoned")
            .piano_selected_notes = [0, 1].iter().copied().collect();
        app.timing.midi_snap_mode = crate::message::SnapMode::NoSnap;

        let _ = app.update(Message::DrumNoteMove {
            note_index: 0,
            delta_samples: 50,
            target_pitch: 36,
        });

        let state = app.state.read().expect("state lock poisoned");
        let piano = state.piano.as_ref().unwrap();
        assert_eq!(piano.notes[0].start_sample, 150);
        assert_eq!(piano.notes[1].start_sample, 250);
        assert_eq!(piano.notes[0].pitch, 36);
        assert_eq!(piano.notes[1].pitch, 38);
    }

    #[test]
    fn drum_note_move_right_near_snap_step_moves_to_next_step() {
        let mut app = app_with_drum_notes(vec![crate::state::PianoNote {
            start_sample: 24_000,
            length_samples: 10,
            pitch: 38,
            velocity: 100,
            channel: 0,
            mpe: Default::default(),
        }]);
        app.timing.midi_snap_mode = crate::message::SnapMode::Beat;

        let _ = app.update(Message::DrumNoteMove {
            note_index: 0,
            delta_samples: 23_900,
            target_pitch: 38,
        });

        let state = app.state.read().expect("state lock poisoned");
        let piano = state.piano.as_ref().unwrap();
        assert_eq!(piano.notes[0].start_sample, 48_000);
    }

    #[test]
    fn drum_note_move_can_move_selected_notes_between_rows() {
        let mut app = app_with_drum_notes(vec![
            crate::state::PianoNote {
                start_sample: 100,
                length_samples: 10,
                pitch: 36,
                velocity: 100,
                channel: 0,
                mpe: Default::default(),
            },
            crate::state::PianoNote {
                start_sample: 200,
                length_samples: 10,
                pitch: 38,
                velocity: 100,
                channel: 0,
                mpe: Default::default(),
            },
        ]);
        app.state
            .write()
            .expect("state lock poisoned")
            .piano_selected_notes = [0, 1].iter().copied().collect();
        app.timing.midi_snap_mode = crate::message::SnapMode::NoSnap;

        let _ = app.update(Message::DrumNoteMove {
            note_index: 0,
            delta_samples: 0,
            target_pitch: 37,
        });

        let state = app.state.read().expect("state lock poisoned");
        let piano = state.piano.as_ref().unwrap();
        assert_eq!(piano.notes[0].pitch, 37);
        assert_eq!(piano.notes[1].pitch, 39);
    }

    #[test]
    fn ctrl_drum_note_move_leaves_original_notes_in_place() {
        let mut app = app_with_drum_notes(vec![
            crate::state::PianoNote {
                start_sample: 100,
                length_samples: 10,
                pitch: 36,
                velocity: 100,
                channel: 0,
                mpe: Default::default(),
            },
            crate::state::PianoNote {
                start_sample: 200,
                length_samples: 10,
                pitch: 38,
                velocity: 100,
                channel: 0,
                mpe: Default::default(),
            },
        ]);
        {
            let mut state = app.state.write().expect("state lock poisoned");
            state.piano_selected_notes = [0, 1].iter().copied().collect();
            state.ctrl = true;
        }
        app.timing.midi_snap_mode = crate::message::SnapMode::NoSnap;

        let _ = app.update(Message::DrumNoteMove {
            note_index: 0,
            delta_samples: 50,
            target_pitch: 37,
        });

        let state = app.state.read().expect("state lock poisoned");
        let piano = state.piano.as_ref().unwrap();
        assert_eq!(piano.notes.len(), 2);
        assert_eq!(piano.notes[0].start_sample, 100);
        assert_eq!(piano.notes[0].pitch, 36);
        assert_eq!(piano.notes[1].start_sample, 200);
        assert_eq!(piano.notes[1].pitch, 38);
        assert!(state.piano_selected_notes.is_empty());
    }

    #[test]
    fn piano_click_on_selected_note_preserves_multi_selection() {
        let mut app = app_with_drum_notes(vec![
            crate::state::PianoNote {
                start_sample: 100,
                length_samples: 10,
                pitch: 60,
                velocity: 100,
                channel: 0,
                mpe: Default::default(),
            },
            crate::state::PianoNote {
                start_sample: 200,
                length_samples: 10,
                pitch: 64,
                velocity: 100,
                channel: 0,
                mpe: Default::default(),
            },
        ]);
        app.state
            .write()
            .expect("state lock poisoned")
            .piano_selected_notes = [0, 1].iter().copied().collect();

        let _ = app.update(Message::PianoNoteClick {
            note_index: 0,
            position: Point::new(10.0, 20.0),
        });

        let state = app.state.read().expect("state lock poisoned");
        assert_eq!(state.piano_selected_notes.len(), 2);
        let dragging = state.piano_dragging_notes.as_ref().unwrap();
        assert_eq!(dragging.note_indices.len(), 2);
        assert!(dragging.note_indices.contains(&0));
        assert!(dragging.note_indices.contains(&1));
    }
}
