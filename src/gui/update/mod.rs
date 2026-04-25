use super::{
    AUDIO_PEAK_UPDATES, AutomationWriteKey, CLIENT, MIN_CLIP_WIDTH_PX, Maolan,
    RUBBERBAND_AVAILABLE, TouchAutomationOverride, platform,
};
mod autosave;
mod dispatch;
mod hw;
mod types;
use self::types::{
    AutomationTrackView, MidiMappingsFile, MidiMappingsGlobalFile, MidiMappingsTrackFile,
};
#[cfg(target_os = "macos")]
use crate::message::PluginFormat;
#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
use crate::state::AudioDeviceOption;
use crate::{
    connections,
    consts::gui::{AUTOSAVE_SNAPSHOT_INTERVAL, METER_QUANTIZE_STEP_DB},
    consts::gui_update_mod::{ATTACK_ALPHA, RELEASE_ALPHA},
    consts::state_ids::METRONOME_TRACK_ID,
    consts::widget_piano::PITCH_MAX,
    message::{
        ClipPitchCorrectionRequest, ClipStretchRequest, ExportNormalizeMode, ExportRenderMode,
        Message, Show, TrackAutomationMode, TrackAutomationTarget,
    },
    platform_caps,
    state::{
        ConnectionViewSelection, HW, PianoData, PianoSysExPoint, Resizing, TempoPoint,
        TimeSignaturePoint, Track, TrackAutomationLane, TrackAutomationPoint, View,
    },
    ui_timing::DOUBLE_CLICK,
    widget::midi_edit::{CTRL_SCROLL_ID, KEYS_SCROLL_ID, NOTES_SCROLL_ID, SYSEX_SCROLL_ID},
    workspace::{
        EDITOR_SCROLL_ID, EDITOR_TIMELINE_SCROLL_ID, PIANO_RULER_SCROLL_ID, PIANO_TEMPO_SCROLL_ID,
        TRACKS_SCROLL_ID, WORKSPACE_RULER_SCROLL_ID, WORKSPACE_TEMPO_SCROLL_ID,
        timeline_x_to_sample_f32,
    },
};
use iced::widget::{Id, operation};
use iced::{Length, Point, Task, mouse};
#[cfg(all(unix, not(target_os = "macos")))]
use maolan_engine::message::PluginGraphPlugin;
use maolan_engine::{
    history,
    kind::Kind,
    message::{
        Action, ClipMoveFrom, ClipMoveTo, Message as EngineMessage, OfflineAutomationLane,
        OfflineAutomationPoint, OfflineAutomationTarget,
    },
};
use rfd::AsyncFileDialog;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet, hash_map::DefaultHasher},
    fs,
    hash::{Hash, Hasher},
    io,
    process::exit,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tracing::error;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedPitchCorrectionPoint {
    start_sample: usize,
    length_samples: usize,
    detected_midi_pitch: f32,
    target_midi_pitch: f32,
    clarity: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedPitchCorrectionFile {
    source_name: String,
    source_offset: usize,
    source_length: usize,
    source_modified_unix_nanos: Option<u128>,
    raw_points: Vec<CachedPitchCorrectionPoint>,
    clip_length_samples: usize,
    frame_likeness: f32,
}

impl Maolan {
    const PITCH_CORRECTION_HISTORY_LIMIT: usize = 100;

    fn pitch_correction_cache_file_name(
        source_name: &str,
        source_offset: usize,
        source_length: usize,
    ) -> String {
        let mut hasher = DefaultHasher::new();
        source_name.hash(&mut hasher);
        source_offset.hash(&mut hasher);
        source_length.hash(&mut hasher);
        format!("{:016x}.json", hasher.finish())
    }

    fn pitch_correction_cache_file_path(
        session_root: &std::path::Path,
        source_name: &str,
        source_offset: usize,
        source_length: usize,
    ) -> std::path::PathBuf {
        session_root
            .join("pitch")
            .join(Self::pitch_correction_cache_file_name(
                source_name,
                source_offset,
                source_length,
            ))
    }

    fn source_modified_unix_nanos(path: &std::path::Path) -> io::Result<u128> {
        let modified = fs::metadata(path)?.modified()?;
        let duration = modified.duration_since(UNIX_EPOCH).map_err(|e| {
            io::Error::other(format!(
                "Failed to read modification time for '{}': {e}",
                path.display()
            ))
        })?;
        Ok(duration.as_nanos())
    }

    fn load_cached_pitch_correction(
        session_root: &std::path::Path,
        source_path: &std::path::Path,
        request: &ClipPitchCorrectionRequest,
    ) -> io::Result<Option<crate::state::PitchCorrectionData>> {
        let path = Self::pitch_correction_cache_file_path(
            session_root,
            &request.source_name,
            request.source_offset,
            request.source_length,
        );
        let cache_file = match fs::File::open(&path) {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err),
        };
        let cache: CachedPitchCorrectionFile =
            serde_json::from_reader(cache_file).map_err(|e| {
                io::Error::other(format!(
                    "Failed to read pitch correction cache '{}': {e}",
                    path.display()
                ))
            })?;
        if cache.source_name != request.source_name
            || cache.source_offset != request.source_offset
            || cache.source_length != request.source_length
        {
            return Ok(None);
        }

        if let Some(expected_modified_nanos) = cache.source_modified_unix_nanos {
            match Self::source_modified_unix_nanos(source_path) {
                Ok(current_modified_nanos) if current_modified_nanos == expected_modified_nanos => {
                }
                _ => return Ok(None),
            }
        }

        if cache.raw_points.is_empty() {
            return Ok(None);
        }

        let frame_likeness = request.frame_likeness.clamp(0.05, 2.0);
        let raw_points: Vec<crate::state::PitchCorrectionPoint> = cache
            .raw_points
            .into_iter()
            .map(|point| crate::state::PitchCorrectionPoint {
                start_sample: point.start_sample,
                length_samples: point.length_samples,
                detected_midi_pitch: point.detected_midi_pitch,
                target_midi_pitch: point.target_midi_pitch,
                clarity: point.clarity,
            })
            .collect();

        let max_gap_samples = raw_points
            .iter()
            .map(|point| point.length_samples)
            .min()
            .unwrap_or(1)
            .max(1);
        let points = Self::merge_adjacent_pitch_fragments(
            raw_points.clone(),
            frame_likeness,
            max_gap_samples,
        );

        Ok(Some(crate::state::PitchCorrectionData {
            track_idx: request.track_idx.clone(),
            clip_index: request.clip_idx,
            clip_name: request.clip_name.clone(),
            clip_length_samples: request.source_length.max(1),
            frame_likeness,
            raw_points,
            points,
        }))
    }

    fn save_cached_pitch_correction(
        session_root: &std::path::Path,
        source_path: &std::path::Path,
        request: &ClipPitchCorrectionRequest,
        pitch_correction: &crate::state::PitchCorrectionData,
    ) -> io::Result<()> {
        fs::create_dir_all(session_root.join("pitch"))?;
        let path = Self::pitch_correction_cache_file_path(
            session_root,
            &request.source_name,
            request.source_offset,
            request.source_length,
        );
        let source_modified_unix_nanos = Self::source_modified_unix_nanos(source_path).ok();
        let raw_points = pitch_correction
            .raw_points
            .iter()
            .map(|point| CachedPitchCorrectionPoint {
                start_sample: point.start_sample,
                length_samples: point.length_samples,
                detected_midi_pitch: point.detected_midi_pitch,
                target_midi_pitch: point.target_midi_pitch,
                clarity: point.clarity,
            })
            .collect();
        let cache = CachedPitchCorrectionFile {
            source_name: request.source_name.clone(),
            source_offset: request.source_offset,
            source_length: request.source_length,
            source_modified_unix_nanos,
            raw_points,
            clip_length_samples: pitch_correction.clip_length_samples,
            frame_likeness: pitch_correction.frame_likeness,
        };
        let file = fs::File::create(&path)?;
        serde_json::to_writer_pretty(file, &cache).map_err(|e| {
            io::Error::other(format!(
                "Failed to write pitch correction cache '{}': {e}",
                path.display()
            ))
        })
    }

    fn push_pitch_correction_history(
        &mut self,
        points: Vec<crate::state::PitchCorrectionPoint>,
        selected_points: HashSet<usize>,
    ) {
        self.pitch_correction_undo
            .push(super::PitchCorrectionHistoryEntry {
                points,
                selected_points,
            });
        if self.pitch_correction_undo.len() > Self::PITCH_CORRECTION_HISTORY_LIMIT {
            self.pitch_correction_undo.remove(0);
        }
        self.pitch_correction_redo.clear();
    }

    fn clear_pitch_correction_history(&mut self) {
        self.pitch_correction_undo.clear();
        self.pitch_correction_redo.clear();
    }

    fn undo_pitch_correction_edit(&mut self) -> Task<Message> {
        let Some(previous) = self.pitch_correction_undo.pop() else {
            return Task::none();
        };
        let mut state = self.state.blocking_write();
        let current_points = match state.pitch_correction.as_ref() {
            Some(pitch_correction) => pitch_correction.points.clone(),
            None => {
                self.pitch_correction_undo.push(previous);
                return Task::none();
            }
        };
        let current_selected = state.pitch_correction_selected_points.clone();
        self.pitch_correction_redo
            .push(super::PitchCorrectionHistoryEntry {
                points: current_points,
                selected_points: current_selected,
            });
        if let Some(pitch_correction) = state.pitch_correction.as_mut() {
            pitch_correction.points = previous.points;
        }
        state.pitch_correction_selected_points = previous.selected_points;
        state.pitch_correction_dragging_points = None;
        state.pitch_correction_selecting_rect = None;
        state.message = "Undid pitch correction edit".to_string();
        drop(state);
        self.sync_pitch_correction_realtime()
    }

    fn redo_pitch_correction_edit(&mut self) -> Task<Message> {
        let Some(next) = self.pitch_correction_redo.pop() else {
            return Task::none();
        };
        let mut state = self.state.blocking_write();
        let current_points = match state.pitch_correction.as_ref() {
            Some(pitch_correction) => pitch_correction.points.clone(),
            None => {
                self.pitch_correction_redo.push(next);
                return Task::none();
            }
        };
        let current_selected = state.pitch_correction_selected_points.clone();
        self.pitch_correction_undo
            .push(super::PitchCorrectionHistoryEntry {
                points: current_points,
                selected_points: current_selected,
            });
        if let Some(pitch_correction) = state.pitch_correction.as_mut() {
            pitch_correction.points = next.points;
        }
        state.pitch_correction_selected_points = next.selected_points;
        state.pitch_correction_dragging_points = None;
        state.pitch_correction_selecting_rect = None;
        state.message = "Redid pitch correction edit".to_string();
        drop(state);
        self.sync_pitch_correction_realtime()
    }

    fn quantize_meter_db(level_db: f32) -> f32 {
        let step = METER_QUANTIZE_STEP_DB;
        ((level_db / step).round() * step).clamp(-90.0, 20.0)
    }

    fn reset_track_plugin_view_state(state: &mut crate::state::StateData) {
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            state.plugin_graph_connecting = None;
            state.plugin_graph_moving_plugin = None;
        }
        state.plugin_graph_last_plugin_click = None;
        state.plugin_graph_selected_plugin = None;
    }

    fn open_track_plugins_followup(&self, track_name: String) -> Task<Message> {
        if platform_caps::SUPPORTS_PLUGIN_GRAPH {
            self.send(Action::TrackGetPluginGraph { track_name })
        } else {
            Task::perform(async {}, |_| {
                Message::Show(crate::message::Show::TrackPluginList)
            })
        }
    }

    fn maybe_refresh_plugin_graph_for_track(&self, track_name: &str) -> Option<Task<Message>> {
        if self.track_has_open_plugin_graph(track_name) {
            Some(self.send(Action::TrackGetPluginGraph {
                track_name: track_name.to_string(),
            }))
        } else {
            None
        }
    }

    fn track_has_open_plugin_graph(&self, track_name: &str) -> bool {
        platform_caps::SUPPORTS_PLUGIN_GRAPH && {
            let state = self.state.blocking_read();
            state.plugin_graph_clip.is_none()
                && state.plugin_graph_track.as_deref() == Some(track_name)
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn save_open_clip_plugin_graph(
        state: &mut crate::state::StateData,
    ) -> Option<maolan_engine::message::Action> {
        let target = state.plugin_graph_clip.clone()?;
        if let Some(track) = state
            .tracks
            .iter_mut()
            .find(|track| track.name == target.track_name)
            && let Some(clip) = track.audio.clips.get_mut(target.clip_idx)
        {
            let graph_json = Self::plugin_graph_snapshot_to_json(
                clip.plugin_graph_json.as_ref(),
                &state.plugin_graph_plugins,
                &state.plugin_graph_connections,
            );
            clip.plugin_graph_json = Some(graph_json);
            return Some(Action::SetClipPluginGraphJson {
                track_name: target.track_name,
                clip_index: target.clip_idx,
                plugin_graph_json: clip.plugin_graph_json.clone(),
            });
        }
        None
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    pub(super) fn default_clip_plugin_graph_json(
        audio_ins: usize,
        audio_outs: usize,
    ) -> serde_json::Value {
        let connections = (0..audio_ins.min(audio_outs))
            .map(|port| {
                serde_json::json!({
                    "from_node": "TrackInput",
                    "from_port": port,
                    "to_node": "TrackOutput",
                    "to_port": port,
                    "kind": "Audio",
                })
            })
            .collect::<Vec<_>>();
        serde_json::json!({
            "plugins": [],
            "connections": connections,
        })
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn clip_plugin_graph_json_or_default(
        graph_json: Option<serde_json::Value>,
        audio_ins: usize,
        audio_outs: usize,
    ) -> serde_json::Value {
        graph_json.unwrap_or_else(|| Self::default_clip_plugin_graph_json(audio_ins, audio_outs))
    }

    pub(crate) fn audio_clip_to_data(
        clip: &crate::state::AudioClip,
    ) -> maolan_engine::message::AudioClipData {
        let mut data = maolan_engine::message::AudioClipData {
            name: clip.name.clone(),
            start: clip.start,
            length: clip.length,
            offset: clip.offset,
            input_channel: clip.input_channel,
            muted: clip.muted,
            peaks_file: clip.peaks_file.clone(),
            fade_enabled: clip.fade_enabled,
            fade_in_samples: clip.fade_in_samples,
            fade_out_samples: clip.fade_out_samples,
            preview_name: clip.pitch_correction_preview_name.clone(),
            source_name: clip.pitch_correction_source_name.clone(),
            source_offset: clip.pitch_correction_source_offset,
            source_length: clip.pitch_correction_source_length,
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
            pitch_correction_formant_compensation: clip.pitch_correction_formant_compensation,
            plugin_graph_json: clip.plugin_graph_json.clone(),
            grouped_clips: clip
                .grouped_clips
                .iter()
                .map(Self::audio_clip_to_data)
                .collect(),
        };
        for child in &mut data.grouped_clips {
            child.fade_enabled = false;
            child.fade_in_samples = 0;
            child.fade_out_samples = 0;
        }
        data
    }

    pub(crate) fn audio_clip_from_data(
        data: &maolan_engine::message::AudioClipData,
        max_length_samples: usize,
    ) -> crate::state::AudioClip {
        let mut clip = crate::state::AudioClip {
            name: data.name.clone(),
            start: data.start,
            length: data.length,
            offset: data.offset,
            input_channel: data.input_channel,
            muted: data.muted,
            max_length_samples,
            peaks_file: data.peaks_file.clone(),
            peaks: Default::default(),
            fade_enabled: data.fade_enabled,
            fade_in_samples: data.fade_in_samples,
            fade_out_samples: data.fade_out_samples,
            pitch_correction_preview_name: data.preview_name.clone(),
            pitch_correction_source_name: data.source_name.clone(),
            pitch_correction_source_offset: data.source_offset,
            pitch_correction_source_length: data.source_length,
            pitch_correction_points: data
                .pitch_correction_points
                .iter()
                .map(|point| crate::state::PitchCorrectionPoint {
                    start_sample: point.start_sample,
                    length_samples: point.length_samples,
                    detected_midi_pitch: point.detected_midi_pitch,
                    target_midi_pitch: point.target_midi_pitch,
                    clarity: point.clarity,
                })
                .collect(),
            pitch_correction_frame_likeness: data.pitch_correction_frame_likeness,
            pitch_correction_inertia_ms: data.pitch_correction_inertia_ms,
            pitch_correction_formant_compensation: data.pitch_correction_formant_compensation,
            take_lane_override: None,
            take_lane_pinned: false,
            take_lane_locked: false,
            plugin_graph_json: data.plugin_graph_json.clone(),
            grouped_clips: data
                .grouped_clips
                .iter()
                .map(|child| Self::audio_clip_from_data(child, max_length_samples))
                .collect(),
        };
        clip.normalize_group_children();
        clip
    }

    pub(crate) fn midi_clip_to_data(
        clip: &crate::state::MIDIClip,
    ) -> maolan_engine::message::MidiClipData {
        maolan_engine::message::MidiClipData {
            name: clip.name.clone(),
            start: clip.start,
            length: clip.length,
            offset: clip.offset,
            input_channel: clip.input_channel,
            muted: clip.muted,
            grouped_clips: clip
                .grouped_clips
                .iter()
                .map(Self::midi_clip_to_data)
                .collect(),
        }
    }

    pub(crate) fn midi_clip_from_data(
        data: &maolan_engine::message::MidiClipData,
        max_length_samples: usize,
    ) -> crate::state::MIDIClip {
        let mut clip = crate::state::MIDIClip {
            name: data.name.clone(),
            start: data.start,
            length: data.length,
            offset: data.offset,
            input_channel: data.input_channel,
            muted: data.muted,
            max_length_samples,
            take_lane_override: None,
            take_lane_pinned: false,
            take_lane_locked: false,
            grouped_clips: data
                .grouped_clips
                .iter()
                .map(|child| Self::midi_clip_from_data(child, max_length_samples))
                .collect(),
        };
        clip.normalize_group_children();
        clip
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn open_clip_plugin_view(&mut self, track_name: String, clip_idx: usize) -> Task<Message> {
        let (
            graph_json,
            track_audio_ins,
            track_audio_outs,
            lv2_plugins,
            vst3_plugins,
            clap_plugins,
        ) = {
            let state = self.state.blocking_read();
            let track = state.tracks.iter().find(|track| track.name == track_name);
            let graph_json = track
                .and_then(|track| track.audio.clips.get(clip_idx))
                .and_then(|clip| clip.plugin_graph_json.clone());
            let (track_audio_ins, track_audio_outs) = track
                .map(|track| (track.audio.ins, track.audio.outs))
                .unwrap_or((0, 0));
            (
                graph_json,
                track_audio_ins,
                track_audio_outs,
                state.lv2_plugins.clone(),
                state.vst3_plugins.clone(),
                state.clap_plugins.clone(),
            )
        };
        let (plugins, connections) = Self::plugin_graph_snapshot_from_json(
            graph_json.as_ref(),
            &lv2_plugins,
            &vst3_plugins,
            &clap_plugins,
        );
        let target_track_name = track_name.clone();
        let mut state = self.state.blocking_write();
        state.view = crate::state::View::TrackPlugins;
        state.plugin_graph_track = Some(track_name.clone());
        state.plugin_graph_clip = Some(crate::state::PluginGraphClipTarget {
            track_name,
            clip_idx,
        });
        Self::reset_track_plugin_view_state(&mut state);
        state.plugin_graph_plugins = plugins.clone();
        state.plugin_graph_connections = connections;
        let mut new_positions = std::collections::HashMap::new();
        for (idx, plugin) in plugins.iter().enumerate() {
            let fallback = iced::Point::new(200.0 + idx as f32 * 180.0, 220.0);
            let pos = state
                .plugin_graph_plugin_positions
                .get(&plugin.instance_id)
                .copied()
                .unwrap_or(fallback);
            new_positions.insert(plugin.instance_id, pos);
        }
        state.plugin_graph_plugin_positions = new_positions;
        if graph_json.is_none()
            && let Some(track) = state
                .tracks
                .iter_mut()
                .find(|track| track.name == target_track_name)
            && let Some(clip) = track.audio.clips.get_mut(clip_idx)
        {
            let graph_json =
                Self::clip_plugin_graph_json_or_default(None, track_audio_ins, track_audio_outs);
            clip.plugin_graph_json = Some(graph_json.clone());
            return self.send(Action::SetClipPluginGraphJson {
                track_name: target_track_name,
                clip_index: clip_idx,
                plugin_graph_json: Some(graph_json),
            });
        }
        Task::none()
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn queue_pending_graph_automation_queries(
        &mut self,
        track_name: &str,
        plugins: &[PluginGraphPlugin],
    ) -> Vec<Task<Message>> {
        let mut pending_queries: Vec<Task<Message>> = vec![];
        self.queue_pending_lv2_automation_queries(track_name, plugins, &mut pending_queries);
        let pending_vst3_paths: Vec<(String, String)> = self
            .pending_add_vst3_automation_paths
            .iter()
            .filter(|(name, _)| name == track_name)
            .cloned()
            .collect();
        for (pending_track, pending_path) in pending_vst3_paths {
            if let Some(instance_id) = plugins
                .iter()
                .find(|plugin| {
                    plugin.format.eq_ignore_ascii_case("VST3")
                        && (plugin.uri == pending_path || plugin.plugin_id == pending_path)
                })
                .map(|plugin| plugin.instance_id)
            {
                self.pending_add_vst3_automation_paths
                    .remove(&(pending_track.clone(), pending_path));
                self.pending_add_vst3_automation_instances
                    .insert((pending_track.clone(), instance_id));
                pending_queries.push(self.send(Action::TrackGetVst3Parameters {
                    track_name: pending_track,
                    instance_id,
                }));
            }
        }
        let pending_paths: Vec<(String, String)> = self
            .pending_add_clap_automation_paths
            .iter()
            .filter(|(name, _)| name == track_name)
            .cloned()
            .collect();
        for (pending_track, pending_path) in pending_paths {
            if let Some(instance_id) = plugins
                .iter()
                .find(|plugin| {
                    plugin.format.eq_ignore_ascii_case("CLAP")
                        && (plugin.uri == pending_path || plugin.plugin_id == pending_path)
                })
                .map(|plugin| plugin.instance_id)
            {
                self.pending_add_clap_automation_paths
                    .remove(&(pending_track.clone(), pending_path));
                self.pending_add_clap_automation_instances
                    .insert((pending_track.clone(), instance_id));
                pending_queries.push(self.send(Action::TrackGetClapParameters {
                    track_name: pending_track,
                    instance_id,
                }));
            }
        }
        pending_queries
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn queue_pending_lv2_automation_queries(
        &mut self,
        track_name: &str,
        plugins: &[PluginGraphPlugin],
        pending_queries: &mut Vec<Task<Message>>,
    ) {
        let pending_lv2_uris: Vec<(String, String)> = self
            .pending_add_lv2_automation_uris
            .iter()
            .filter(|(name, _)| name == track_name)
            .cloned()
            .collect();
        for (pending_track, pending_uri) in pending_lv2_uris {
            if let Some(instance_id) = plugins
                .iter()
                .find(|plugin| {
                    plugin.format.eq_ignore_ascii_case("LV2")
                        && (plugin.uri == pending_uri || plugin.plugin_id == pending_uri)
                })
                .map(|plugin| plugin.instance_id)
            {
                self.pending_add_lv2_automation_uris
                    .remove(&(pending_track.clone(), pending_uri));
                self.pending_add_lv2_automation_instances
                    .insert((pending_track.clone(), instance_id));
                pending_queries.push(self.send(Action::TrackGetLv2PluginControls {
                    track_name: pending_track,
                    instance_id,
                }));
            }
        }
    }

    #[cfg(not(all(unix, not(target_os = "macos"))))]
    fn queue_pending_lv2_automation_queries(
        &mut self,
        _track_name: &str,
        _plugins: &[PluginGraphPlugin],
        _pending_queries: &mut Vec<Task<Message>>,
    ) {
    }

    fn pending_save_ready(&self) -> bool {
        self.pending_save_tracks.is_empty()
            && self.pending_save_clap_tracks.is_empty()
            && self.pending_vst3_save_ready()
    }

    #[cfg(target_os = "macos")]
    fn pending_vst3_save_ready(&self) -> bool {
        !platform_caps::REQUIRE_VST3_STATE_FOR_SAVE || self.pending_save_vst3_states.is_empty()
    }

    #[cfg(not(target_os = "macos"))]
    fn pending_vst3_save_ready(&self) -> bool {
        !platform_caps::REQUIRE_VST3_STATE_FOR_SAVE
    }

    fn rename_track_map_entry<T>(map: &mut HashMap<String, T>, old_name: &str, new_name: &str) {
        if let Some(value) = map.remove(old_name) {
            map.insert(new_name.to_string(), value);
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn open_lv2_plugin_ui_task(&self, track_name: &str, instance_id: usize) -> Task<Message> {
        self.send(Action::TrackGetLv2PluginControls {
            track_name: track_name.to_string(),
            instance_id,
        })
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn pump_lv2_ui(&mut self) {
        self.lv2_ui_host.pump();
    }

    fn rebuild_midi_mappings_report_lines_from_state(&mut self) {
        let state = self.state.blocking_read();
        let mut lines = Vec::<String>::new();
        let mut push_binding =
            |scope: String, label: &str, binding: &maolan_engine::message::MidiLearnBinding| {
                lines.push(format!(
                    "{scope} {label}: CH{} CC{}",
                    binding.channel + 1,
                    binding.cc
                ));
            };

        if let Some(binding) = state.global_midi_learn_play_pause.as_ref() {
            push_binding("Global".to_string(), "Play/Pause", binding);
        }
        if let Some(binding) = state.global_midi_learn_stop.as_ref() {
            push_binding("Global".to_string(), "Stop", binding);
        }
        if let Some(binding) = state.global_midi_learn_record_toggle.as_ref() {
            push_binding("Global".to_string(), "Record Toggle", binding);
        }

        for track in &state.tracks {
            if let Some(binding) = track.midi_learn_volume.as_ref() {
                push_binding(format!("Track '{}'", track.name), "Volume", binding);
            }
            if let Some(binding) = track.midi_learn_balance.as_ref() {
                push_binding(format!("Track '{}'", track.name), "Balance", binding);
            }
            if let Some(binding) = track.midi_learn_mute.as_ref() {
                push_binding(format!("Track '{}'", track.name), "Mute", binding);
            }
            if let Some(binding) = track.midi_learn_solo.as_ref() {
                push_binding(format!("Track '{}'", track.name), "Solo", binding);
            }
            if let Some(binding) = track.midi_learn_arm.as_ref() {
                push_binding(format!("Track '{}'", track.name), "Arm", binding);
            }
            if let Some(binding) = track.midi_learn_input_monitor.as_ref() {
                push_binding(format!("Track '{}'", track.name), "Input Monitor", binding);
            }
            if let Some(binding) = track.midi_learn_disk_monitor.as_ref() {
                push_binding(format!("Track '{}'", track.name), "Disk Monitor", binding);
            }
        }

        if lines.is_empty() {
            lines.push("No MIDI learn bindings".to_string());
        }
        self.midi_mappings_report_lines = lines;
    }

    fn assign_take_lanes<T, FBase, FStart, FLen, FPreferred>(
        clips: &[T],
        base_lane: FBase,
        start_sample: FStart,
        length_samples: FLen,
        preferred_take_lane: FPreferred,
    ) -> (Vec<usize>, Vec<usize>)
    where
        FBase: Fn(&T) -> usize,
        FStart: Fn(&T) -> usize,
        FLen: Fn(&T) -> usize,
        FPreferred: Fn(&T) -> Option<usize>,
    {
        let mut take_index_by_clip = vec![0_usize; clips.len()];
        let mut max_takes_by_lane: HashMap<usize, usize> = HashMap::new();
        let mut active_by_lane: HashMap<usize, Vec<(usize, usize)>> = HashMap::new();

        let mut order: Vec<usize> = (0..clips.len()).collect();
        order.sort_by_key(|idx| {
            let clip = &clips[*idx];
            (base_lane(clip), start_sample(clip), std::cmp::Reverse(*idx))
        });

        for idx in order {
            let clip = &clips[idx];
            let lane = base_lane(clip);
            let start = start_sample(clip);
            let end = start.saturating_add(length_samples(clip));
            let active = active_by_lane.entry(lane).or_default();
            active.retain(|(existing_end, _)| *existing_end > start);
            let preferred = preferred_take_lane(clip);
            let mut take_idx = preferred.unwrap_or(0);
            if preferred.is_none() {
                while active
                    .iter()
                    .any(|(_, existing_take)| *existing_take == take_idx)
                {
                    take_idx = take_idx.saturating_add(1);
                }
            }
            active.push((end, take_idx));
            take_index_by_clip[idx] = take_idx;
            max_takes_by_lane
                .entry(lane)
                .and_modify(|max_take| *max_take = (*max_take).max(take_idx.saturating_add(1)))
                .or_insert(take_idx.saturating_add(1));
        }

        let take_count_by_clip = clips
            .iter()
            .map(|clip| {
                let lane = base_lane(clip);
                max_takes_by_lane.get(&lane).copied().unwrap_or(1).max(1)
            })
            .collect::<Vec<_>>();

        (take_index_by_clip, take_count_by_clip)
    }

    fn timing_at_sample(state: &crate::state::StateData, sample: usize) -> (f32, u8, u8) {
        let bpm = state
            .tempo_points
            .iter()
            .filter(|p| p.sample <= sample)
            .max_by_key(|p| p.sample)
            .map(|p| p.bpm)
            .unwrap_or(state.tempo)
            .clamp(20.0, 300.0);
        let (num, den) = state
            .time_signature_points
            .iter()
            .filter(|p| p.sample <= sample)
            .max_by_key(|p| p.sample)
            .map(|p| (p.numerator.max(1), p.denominator.max(1)))
            .unwrap_or((
                state.time_signature_num.max(1),
                state.time_signature_denom.max(1),
            ));
        (bpm, num, den)
    }

    fn sync_timing_inputs_from_selection(&mut self) {
        let state = self.state.blocking_read();
        if let Some(sample) = self.selected_tempo_points.iter().next().copied()
            && let Some(point) = state.tempo_points.iter().find(|p| p.sample == sample)
        {
            self.tempo_input = format!("{:.2}", point.bpm);
        }
        if let Some(sample) = self.selected_time_signature_points.iter().next().copied()
            && let Some(point) = state
                .time_signature_points
                .iter()
                .find(|p| p.sample == sample)
        {
            self.time_signature_num_input = point.numerator.to_string();
            self.time_signature_denom_input = point.denominator.to_string();
        }
    }

    fn selected_piano_notes_edit<F>(&mut self, mut edit: F) -> Task<Message>
    where
        F: FnMut(
            usize,
            &maolan_engine::message::MidiNoteData,
        ) -> maolan_engine::message::MidiNoteData,
    {
        let mut state = self.state.blocking_write();
        if !matches!(state.view, View::Piano) {
            return Task::none();
        }
        let mut selected_indices: Vec<usize> = state.piano_selected_notes.iter().copied().collect();
        selected_indices.sort_unstable();
        selected_indices.dedup();
        if selected_indices.is_empty() {
            return Task::none();
        }
        let Some(piano) = state.piano.as_mut() else {
            return Task::none();
        };
        let track_name = piano.track_idx.clone();
        let clip_idx = piano.clip_index;

        let mut changed_indices = Vec::new();
        let mut new_notes = Vec::new();
        let mut old_notes = Vec::new();
        for idx in selected_indices {
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
            let mut new_note = edit(idx, &old_note);
            if new_note.length_samples == 0 {
                new_note.length_samples = 1;
            }
            if new_note.start_sample == old_note.start_sample
                && new_note.length_samples == old_note.length_samples
                && new_note.pitch == old_note.pitch
                && new_note.velocity == old_note.velocity
                && new_note.channel == old_note.channel
            {
                continue;
            }
            note.start_sample = new_note.start_sample;
            note.length_samples = new_note.length_samples;
            note.pitch = new_note.pitch;
            note.velocity = new_note.velocity;
            note.channel = new_note.channel;
            changed_indices.push(idx);
            new_notes.push(new_note);
            old_notes.push(old_note);
        }
        if changed_indices.is_empty() {
            return Task::none();
        }
        drop(state);
        self.send(Action::ModifyMidiNotes {
            track_name,
            clip_index: clip_idx,
            note_indices: changed_indices,
            new_notes,
            old_notes,
        })
    }

    fn queue_midi_clip_preview_loads(&mut self) -> Task<Message> {
        let Some(session_dir) = self.session_dir.clone() else {
            return Task::none();
        };
        let mut live = HashMap::<(String, usize), String>::new();
        {
            let state = self.state.blocking_read();
            for track in &state.tracks {
                for (clip_idx, clip) in track.midi.clips.iter().enumerate() {
                    live.insert((track.name.clone(), clip_idx), clip.name.clone());
                }
            }
        }

        self.midi_clip_previews
            .retain(|key, _| live.contains_key(key));
        self.pending_midi_clip_previews
            .retain(|(track_name, clip_idx, clip_name)| {
                live.get(&(track_name.clone(), *clip_idx))
                    .is_some_and(|name| name == clip_name)
            });

        let mut tasks = Vec::new();
        for ((track_name, clip_idx), clip_name) in live {
            self.midi_clip_previews
                .remove(&(track_name.clone(), clip_idx));
            let pending_key = (track_name.clone(), clip_idx, clip_name.clone());
            if !self.pending_midi_clip_previews.insert(pending_key) {
                continue;
            }
            let session_dir = session_dir.clone();
            let playback_rate_hz = self.playback_rate_hz;
            let task_clip = clip_name.clone();
            let result_track = track_name.clone();
            let result_clip = clip_name.clone();
            tasks.push(Task::perform(
                async move {
                    let clip_path = std::path::PathBuf::from(&task_clip);
                    let path = if clip_path.is_absolute() {
                        clip_path
                    } else {
                        session_dir.join(&task_clip)
                    };
                    match Self::parse_midi_clip_for_piano(&path, playback_rate_hz) {
                        Ok((notes, _, _, _)) => notes,
                        Err(_) => Vec::new(),
                    }
                },
                move |notes| Message::MidiClipPreviewLoaded {
                    track_idx: result_track.clone(),
                    clip_idx,
                    clip_name: result_clip.clone(),
                    notes,
                },
            ));
        }

        if tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(tasks)
        }
    }

    fn deterministic_note_jitter(seed_a: usize, seed_b: usize, amplitude: i64) -> i64 {
        if amplitude <= 0 {
            return 0;
        }
        let mut x = (seed_a as u64)
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add((seed_b as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9))
            .wrapping_add(0x94D0_49BB_1331_11EB);
        x ^= x >> 30;
        x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        x ^= x >> 27;
        x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
        x ^= x >> 31;
        let range = (amplitude as u64).saturating_mul(2).saturating_add(1);
        (x % range) as i64 - amplitude
    }

    fn nearest_scale_pitch(pitch: u8, root_semitone: u8, minor: bool) -> u8 {
        let pattern: &[u8] = if minor {
            &[0, 2, 3, 5, 7, 8, 10]
        } else {
            &[0, 2, 4, 5, 7, 9, 11]
        };
        let mut best = pitch;
        let mut best_dist = i16::MAX;
        for candidate in 0_u8..=127_u8 {
            let rel = (candidate as i16 - root_semitone as i16).rem_euclid(12) as u8;
            if !pattern.contains(&rel) {
                continue;
            }
            let dist = (candidate as i16 - pitch as i16).abs();
            if dist < best_dist || (dist == best_dist && candidate < best) {
                best = candidate;
                best_dist = dist;
            }
        }
        best
    }

    fn automation_key(target: TrackAutomationTarget) -> AutomationWriteKey {
        match target {
            TrackAutomationTarget::Volume => AutomationWriteKey::Volume,
            TrackAutomationTarget::Balance => AutomationWriteKey::Balance,
            TrackAutomationTarget::Mute => AutomationWriteKey::Mute,
            TrackAutomationTarget::Lv2Parameter {
                instance_id, index, ..
            } => AutomationWriteKey::Lv2 { instance_id, index },
            TrackAutomationTarget::Vst3Parameter {
                instance_id,
                param_id,
            } => AutomationWriteKey::Vst3 {
                instance_id,
                param_id,
            },
            TrackAutomationTarget::ClapParameter {
                instance_id,
                param_id,
                ..
            } => AutomationWriteKey::Clap {
                instance_id,
                param_id,
            },
        }
    }

    fn key_has_explicit_gesture_lifecycle(key: AutomationWriteKey) -> bool {
        matches!(key, AutomationWriteKey::Clap { .. })
    }

    fn record_automation_point(
        &mut self,
        track_name: &str,
        target: TrackAutomationTarget,
        value: f32,
    ) {
        if !self.playing || self.paused {
            return;
        }
        let sample = self.transport_samples.max(0.0) as usize;
        let mut state = self.state.blocking_write();
        let Some(track) = state.tracks.iter_mut().find(|t| t.name == track_name) else {
            return;
        };
        if track.automation_mode == TrackAutomationMode::Read {
            return;
        }
        if let Some(lane) = track
            .automation_lanes
            .iter_mut()
            .find(|lane| lane.target == target)
        {
            if let Some(existing) = lane.points.iter_mut().find(|p| p.sample == sample) {
                existing.value = value.clamp(0.0, 1.0);
            } else {
                lane.points.push(TrackAutomationPoint {
                    sample,
                    value: value.clamp(0.0, 1.0),
                });
                lane.points.sort_unstable_by_key(|p| p.sample);
            }
            lane.visible = true;
        } else {
            track
                .automation_lanes
                .push(crate::state::TrackAutomationLane {
                    target,
                    visible: true,
                    points: vec![TrackAutomationPoint {
                        sample,
                        value: value.clamp(0.0, 1.0),
                    }],
                });
        }
        track.height = track.min_height_for_layout().max(60.0);
    }

    fn record_manual_override(
        &mut self,
        track_name: &str,
        target: TrackAutomationTarget,
        value: f32,
    ) {
        let mode = {
            let state = self.state.blocking_read();
            state
                .tracks
                .iter()
                .find(|t| t.name == track_name)
                .map(|t| t.automation_mode)
        };
        let Some(mode) = mode else {
            return;
        };
        let key = Self::automation_key(target);
        let value = value.clamp(0.0, 1.0);
        match mode {
            TrackAutomationMode::Read | TrackAutomationMode::Write => {}
            TrackAutomationMode::Touch => {
                let key = Self::automation_key(target);
                self.touch_automation_overrides
                    .entry(track_name.to_string())
                    .or_default()
                    .insert(
                        key,
                        TouchAutomationOverride {
                            value,
                            updated_at: Instant::now(),
                        },
                    );
                self.touch_active_keys
                    .entry(track_name.to_string())
                    .or_default()
                    .insert(key);
            }
            TrackAutomationMode::Latch => {
                self.latch_automation_overrides
                    .entry(track_name.to_string())
                    .or_default()
                    .insert(key, value);
            }
        }
    }

    fn begin_touch_gesture(&mut self, track_name: &str, key: AutomationWriteKey) {
        let mode = {
            let state = self.state.blocking_read();
            state
                .tracks
                .iter()
                .find(|t| t.name == track_name)
                .map(|t| t.automation_mode)
        };
        if mode == Some(TrackAutomationMode::Touch) {
            self.touch_active_keys
                .entry(track_name.to_string())
                .or_default()
                .insert(key);
        }
    }

    fn end_touch_gesture(&mut self, track_name: &str, key: AutomationWriteKey) {
        if let Some(active) = self.touch_active_keys.get_mut(track_name) {
            active.remove(&key);
            if active.is_empty() {
                self.touch_active_keys.remove(track_name);
            }
        }
        if let Some(values) = self.touch_automation_overrides.get_mut(track_name) {
            values.remove(&key);
            if values.is_empty() {
                self.touch_automation_overrides.remove(track_name);
            }
        }
    }

    fn find_clap_target(
        &self,
        track_name: &str,
        instance_id: usize,
        param_id: u32,
    ) -> Option<TrackAutomationTarget> {
        let state = self.state.blocking_read();
        let track = state.tracks.iter().find(|t| t.name == track_name)?;
        track
            .automation_lanes
            .iter()
            .find_map(|lane| match lane.target {
                TrackAutomationTarget::ClapParameter {
                    instance_id: i,
                    param_id: p,
                    min,
                    max,
                } if i == instance_id && p == param_id => {
                    Some(TrackAutomationTarget::ClapParameter {
                        instance_id: i,
                        param_id: p,
                        min,
                        max,
                    })
                }
                _ => None,
            })
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn find_lv2_target(
        &self,
        track_name: &str,
        instance_id: usize,
        index: u32,
    ) -> Option<TrackAutomationTarget> {
        let state = self.state.blocking_read();
        let track = state.tracks.iter().find(|t| t.name == track_name)?;
        track
            .automation_lanes
            .iter()
            .find_map(|lane| match lane.target {
                TrackAutomationTarget::Lv2Parameter {
                    instance_id: i,
                    index: c,
                    min,
                    max,
                } if i == instance_id && c == index => Some(TrackAutomationTarget::Lv2Parameter {
                    instance_id: i,
                    index: c,
                    min,
                    max,
                }),
                _ => None,
            })
    }

    fn is_track_frozen(&self, track_name: &str) -> bool {
        let state = self.state.blocking_read();
        state
            .tracks
            .iter()
            .find(|t| t.name == track_name)
            .is_some_and(|t| t.frozen)
    }

    fn midi_mappings_path(&self) -> Option<std::path::PathBuf> {
        self.session_dir
            .as_ref()
            .map(|root| root.join("midi_mappings.json"))
    }

    fn export_midi_mappings_file(&self) -> Result<std::path::PathBuf, String> {
        let Some(path) = self.midi_mappings_path() else {
            return Err("Export MIDI mappings requires an opened/saved session".to_string());
        };
        let state = self.state.blocking_read();
        let mut tracks = HashMap::<String, MidiMappingsTrackFile>::new();
        for track in &state.tracks {
            tracks.insert(
                track.name.clone(),
                MidiMappingsTrackFile {
                    volume: track.midi_learn_volume.clone(),
                    balance: track.midi_learn_balance.clone(),
                    mute: track.midi_learn_mute.clone(),
                    solo: track.midi_learn_solo.clone(),
                    arm: track.midi_learn_arm.clone(),
                    input_monitor: track.midi_learn_input_monitor.clone(),
                    disk_monitor: track.midi_learn_disk_monitor.clone(),
                },
            );
        }
        let file = MidiMappingsFile {
            global: MidiMappingsGlobalFile {
                play_pause: state.global_midi_learn_play_pause.clone(),
                stop: state.global_midi_learn_stop.clone(),
                record_toggle: state.global_midi_learn_record_toggle.clone(),
            },
            tracks,
        };
        let f = std::fs::File::create(&path).map_err(|e| e.to_string())?;
        serde_json::to_writer_pretty(f, &file).map_err(|e| e.to_string())?;
        Ok(path)
    }

    fn import_midi_mappings_actions(&self) -> Result<Vec<Action>, String> {
        let Some(path) = self.midi_mappings_path() else {
            return Err("Import MIDI mappings requires an opened/saved session".to_string());
        };
        let f = std::fs::File::open(&path).map_err(|e| e.to_string())?;
        let file: MidiMappingsFile = serde_json::from_reader(f).map_err(|e| e.to_string())?;
        let state = self.state.blocking_read();
        let mut actions = Vec::<Action>::new();
        actions.push(Action::SetGlobalMidiLearnBinding {
            target: maolan_engine::message::GlobalMidiLearnTarget::PlayPause,
            binding: None,
        });
        actions.push(Action::SetGlobalMidiLearnBinding {
            target: maolan_engine::message::GlobalMidiLearnTarget::Stop,
            binding: None,
        });
        actions.push(Action::SetGlobalMidiLearnBinding {
            target: maolan_engine::message::GlobalMidiLearnTarget::RecordToggle,
            binding: None,
        });
        for track in &state.tracks {
            let name = track.name.clone();
            for target in [
                maolan_engine::message::TrackMidiLearnTarget::Volume,
                maolan_engine::message::TrackMidiLearnTarget::Balance,
                maolan_engine::message::TrackMidiLearnTarget::Mute,
                maolan_engine::message::TrackMidiLearnTarget::Solo,
                maolan_engine::message::TrackMidiLearnTarget::Arm,
                maolan_engine::message::TrackMidiLearnTarget::InputMonitor,
                maolan_engine::message::TrackMidiLearnTarget::DiskMonitor,
            ] {
                actions.push(Action::TrackSetMidiLearnBinding {
                    track_name: name.clone(),
                    target,
                    binding: None,
                });
            }
        }
        if file.global.play_pause.is_some() {
            actions.push(Action::SetGlobalMidiLearnBinding {
                target: maolan_engine::message::GlobalMidiLearnTarget::PlayPause,
                binding: file.global.play_pause,
            });
        }
        if file.global.stop.is_some() {
            actions.push(Action::SetGlobalMidiLearnBinding {
                target: maolan_engine::message::GlobalMidiLearnTarget::Stop,
                binding: file.global.stop,
            });
        }
        if file.global.record_toggle.is_some() {
            actions.push(Action::SetGlobalMidiLearnBinding {
                target: maolan_engine::message::GlobalMidiLearnTarget::RecordToggle,
                binding: file.global.record_toggle,
            });
        }
        for (track_name, mapping) in file.tracks {
            if !state.tracks.iter().any(|t| t.name == track_name) {
                continue;
            }
            let mut push_if_some =
                |target: maolan_engine::message::TrackMidiLearnTarget,
                 binding: Option<maolan_engine::message::MidiLearnBinding>| {
                    if binding.is_some() {
                        actions.push(Action::TrackSetMidiLearnBinding {
                            track_name: track_name.clone(),
                            target,
                            binding,
                        });
                    }
                };
            push_if_some(
                maolan_engine::message::TrackMidiLearnTarget::Volume,
                mapping.volume,
            );
            push_if_some(
                maolan_engine::message::TrackMidiLearnTarget::Balance,
                mapping.balance,
            );
            push_if_some(
                maolan_engine::message::TrackMidiLearnTarget::Mute,
                mapping.mute,
            );
            push_if_some(
                maolan_engine::message::TrackMidiLearnTarget::Solo,
                mapping.solo,
            );
            push_if_some(
                maolan_engine::message::TrackMidiLearnTarget::Arm,
                mapping.arm,
            );
            push_if_some(
                maolan_engine::message::TrackMidiLearnTarget::InputMonitor,
                mapping.input_monitor,
            );
            push_if_some(
                maolan_engine::message::TrackMidiLearnTarget::DiskMonitor,
                mapping.disk_monitor,
            );
        }
        Ok(actions)
    }

    fn maybe_record_automation_from_request(&mut self, action: &Action) {
        match action {
            Action::TrackLevel(track_name, level) if track_name != "hw:out" => {
                let normalized = ((*level + 90.0) / 110.0).clamp(0.0, 1.0);
                self.record_automation_point(track_name, TrackAutomationTarget::Volume, normalized);
                self.record_manual_override(track_name, TrackAutomationTarget::Volume, normalized);
            }
            Action::TrackBalance(track_name, balance) if track_name != "hw:out" => {
                let normalized = ((*balance + 1.0) * 0.5).clamp(0.0, 1.0);
                self.record_automation_point(
                    track_name,
                    TrackAutomationTarget::Balance,
                    normalized,
                );
                self.record_manual_override(track_name, TrackAutomationTarget::Balance, normalized);
            }
            Action::TrackToggleMute(track_name) if track_name != "hw:out" => {
                let next = {
                    let state = self.state.blocking_read();
                    state
                        .tracks
                        .iter()
                        .find(|t| t.name == *track_name)
                        .map(|t| !t.muted)
                };
                if let Some(next) = next {
                    self.record_automation_point(
                        track_name,
                        TrackAutomationTarget::Mute,
                        if next { 1.0 } else { 0.0 },
                    );
                    self.record_manual_override(
                        track_name,
                        TrackAutomationTarget::Mute,
                        if next { 1.0 } else { 0.0 },
                    );
                }
            }
            Action::TrackSetVst3Parameter {
                track_name,
                instance_id,
                param_id,
                value,
            } => {
                if self.is_track_frozen(track_name) {
                    return;
                }
                self.record_automation_point(
                    track_name,
                    TrackAutomationTarget::Vst3Parameter {
                        instance_id: *instance_id,
                        param_id: *param_id,
                    },
                    (*value).clamp(0.0, 1.0),
                );
                self.record_manual_override(
                    track_name,
                    TrackAutomationTarget::Vst3Parameter {
                        instance_id: *instance_id,
                        param_id: *param_id,
                    },
                    (*value).clamp(0.0, 1.0),
                );
            }
            Action::TrackSetClapParameter {
                track_name,
                instance_id,
                param_id,
                value,
            }
            | Action::TrackSetClapParameterAt {
                track_name,
                instance_id,
                param_id,
                value,
                ..
            } => {
                if self.is_track_frozen(track_name) {
                    return;
                }
                if let Some(TrackAutomationTarget::ClapParameter { min, max, .. }) =
                    self.find_clap_target(track_name, *instance_id, *param_id)
                {
                    let span = (max - min).abs();
                    let normalized = if span <= f64::EPSILON {
                        0.0
                    } else {
                        ((*value - min) / (max - min)).clamp(0.0, 1.0)
                    } as f32;
                    self.record_automation_point(
                        track_name,
                        TrackAutomationTarget::ClapParameter {
                            instance_id: *instance_id,
                            param_id: *param_id,
                            min,
                            max,
                        },
                        normalized,
                    );
                    self.record_manual_override(
                        track_name,
                        TrackAutomationTarget::ClapParameter {
                            instance_id: *instance_id,
                            param_id: *param_id,
                            min,
                            max,
                        },
                        normalized,
                    );
                }
            }
            Action::TrackBeginClapParameterEdit {
                track_name,
                instance_id,
                param_id,
                ..
            } => {
                if self.is_track_frozen(track_name) {
                    return;
                }
                self.begin_touch_gesture(
                    track_name,
                    AutomationWriteKey::Clap {
                        instance_id: *instance_id,
                        param_id: *param_id,
                    },
                );
            }
            Action::TrackEndClapParameterEdit {
                track_name,
                instance_id,
                param_id,
                ..
            } => {
                if self.is_track_frozen(track_name) {
                    return;
                }
                self.end_touch_gesture(
                    track_name,
                    AutomationWriteKey::Clap {
                        instance_id: *instance_id,
                        param_id: *param_id,
                    },
                );
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            Action::TrackSetLv2ControlValue {
                track_name,
                instance_id,
                index,
                value,
            } => {
                if self.is_track_frozen(track_name) {
                    return;
                }
                if let Some(TrackAutomationTarget::Lv2Parameter { min, max, .. }) =
                    self.find_lv2_target(track_name, *instance_id, *index)
                {
                    let span = (max - min).abs();
                    let normalized = if span <= f32::EPSILON {
                        0.0
                    } else {
                        ((*value - min) / (max - min)).clamp(0.0, 1.0)
                    };
                    self.record_automation_point(
                        track_name,
                        TrackAutomationTarget::Lv2Parameter {
                            instance_id: *instance_id,
                            index: *index,
                            min,
                            max,
                        },
                        normalized,
                    );
                    self.record_manual_override(
                        track_name,
                        TrackAutomationTarget::Lv2Parameter {
                            instance_id: *instance_id,
                            index: *index,
                            min,
                            max,
                        },
                        normalized,
                    );
                }
            }
            _ => {}
        }
    }

    fn vca_group_tracks_for(&self, track_name: &str) -> Vec<String> {
        let state = self.state.blocking_read();
        let Some(track) = state.tracks.iter().find(|t| t.name == track_name) else {
            return vec![track_name.to_string()];
        };
        let mut members = if let Some(group_name) = track.vca_master.as_deref() {
            state
                .tracks
                .iter()
                .filter(|t| t.vca_master.as_deref() == Some(group_name))
                .map(|t| t.name.clone())
                .collect::<Vec<_>>()
        } else {
            let mut members = vec![track.name.clone()];
            members.extend(
                state
                    .tracks
                    .iter()
                    .filter(|t| t.vca_master.as_deref() == Some(track.name.as_str()))
                    .map(|t| t.name.clone()),
            );
            members
        };
        members.sort();
        members.dedup();
        members
    }

    fn expand_request_to_vca_group(&self, action: &Action) -> Option<Vec<Action>> {
        let (source_track, builder): (&str, fn(String, &Action) -> Action) = match action {
            Action::TrackLevel(track_name, _) => (track_name.as_str(), |name, a| match a {
                Action::TrackLevel(_, level) => Action::TrackLevel(name, *level),
                _ => unreachable!(),
            }),
            Action::TrackBalance(track_name, _) => (track_name.as_str(), |name, a| match a {
                Action::TrackBalance(_, balance) => Action::TrackBalance(name, *balance),
                _ => unreachable!(),
            }),
            Action::TrackToggleArm(track_name) => {
                (track_name.as_str(), |name, _| Action::TrackToggleArm(name))
            }
            Action::TrackToggleMute(track_name) => {
                (track_name.as_str(), |name, _| Action::TrackToggleMute(name))
            }
            Action::TrackToggleSolo(track_name) => {
                (track_name.as_str(), |name, _| Action::TrackToggleSolo(name))
            }
            Action::TrackToggleInputMonitor(track_name) => (track_name.as_str(), |name, _| {
                Action::TrackToggleInputMonitor(name)
            }),
            Action::TrackToggleDiskMonitor(track_name) => (track_name.as_str(), |name, _| {
                Action::TrackToggleDiskMonitor(name)
            }),
            _ => return None,
        };
        if source_track == "hw:out" {
            return None;
        }
        let members = self.vca_group_tracks_for(source_track);
        if members.len() <= 1 {
            return None;
        }
        Some(
            members
                .into_iter()
                .map(|name| builder(name, action))
                .collect(),
        )
    }

    fn automation_lane_value_at(points: &[TrackAutomationPoint], sample: usize) -> Option<f32> {
        if points.is_empty() {
            return None;
        }
        let mut sorted: Vec<&TrackAutomationPoint> = points.iter().collect();
        sorted.sort_unstable_by_key(|p| p.sample);
        if sample <= sorted[0].sample {
            return Some(sorted[0].value.clamp(0.0, 1.0));
        }
        if sample >= sorted[sorted.len().saturating_sub(1)].sample {
            return Some(sorted[sorted.len().saturating_sub(1)].value.clamp(0.0, 1.0));
        }
        for segment in sorted.windows(2) {
            let left = segment[0];
            let right = segment[1];
            if sample < left.sample || sample > right.sample {
                continue;
            }
            let span = right.sample.saturating_sub(left.sample).max(1) as f32;
            let t = (sample.saturating_sub(left.sample) as f32 / span).clamp(0.0, 1.0);
            let value = left.value + (right.value - left.value) * t;
            return Some(value.clamp(0.0, 1.0));
        }
        None
    }

    fn collect_track_automation_actions(
        &mut self,
        sample: usize,
        tracks: &[AutomationTrackView],
    ) -> Vec<Action> {
        let now = Instant::now();
        for (track_name, active_keys) in self.touch_active_keys.iter_mut() {
            let values = self.touch_automation_overrides.get(track_name);
            active_keys.retain(|key| {
                if Self::key_has_explicit_gesture_lifecycle(*key) {
                    true
                } else {
                    values.and_then(|map| map.get(key)).is_some_and(|entry| {
                        now.duration_since(entry.updated_at) <= Duration::from_millis(220)
                    })
                }
            });
        }
        self.touch_active_keys.retain(|_, keys| !keys.is_empty());
        for (track_name, values) in self.touch_automation_overrides.iter_mut() {
            let active = self.touch_active_keys.get(track_name);
            values.retain(|key, entry| {
                active.is_some_and(|set| set.contains(key))
                    || now.duration_since(entry.updated_at) <= Duration::from_millis(220)
            });
        }
        self.touch_automation_overrides
            .retain(|_, values| !values.is_empty());

        let mut actions = Vec::new();
        for track in tracks {
            if track.automation_mode == TrackAutomationMode::Write {
                continue;
            }
            let mut vol = None;
            let mut bal = None;
            let mut muted = None;
            let runtime = self
                .track_automation_runtime
                .entry(track.name.clone())
                .or_default();
            for lane in &track.automation_lanes {
                let key = Self::automation_key(lane.target);
                let override_value = match track.automation_mode {
                    TrackAutomationMode::Touch => self
                        .touch_automation_overrides
                        .get(&track.name)
                        .and_then(|values| values.get(&key))
                        .and_then(|entry| {
                            let active = self
                                .touch_active_keys
                                .get(&track.name)
                                .is_some_and(|set| set.contains(&key));
                            let fresh =
                                now.duration_since(entry.updated_at) <= Duration::from_millis(220);
                            (active || fresh).then_some(entry.value)
                        }),
                    TrackAutomationMode::Latch => self
                        .latch_automation_overrides
                        .get(&track.name)
                        .and_then(|values| values.get(&key))
                        .copied(),
                    _ => None,
                };
                let value =
                    override_value.or_else(|| Self::automation_lane_value_at(&lane.points, sample));
                match lane.target {
                    TrackAutomationTarget::Volume => vol = value,
                    TrackAutomationTarget::Balance => bal = value,
                    TrackAutomationTarget::Mute => muted = value.map(|v| v >= 0.5),
                    #[cfg(all(unix, not(target_os = "macos")))]
                    TrackAutomationTarget::Lv2Parameter {
                        instance_id,
                        index,
                        min,
                        max,
                    } => {
                        if track.frozen {
                            continue;
                        }
                        #[cfg(all(unix, not(target_os = "macos")))]
                        if let Some(v) = value {
                            let lo = min.min(max);
                            let hi = max.max(min);
                            let param_value = (lo + v * (hi - lo)).clamp(lo, hi);
                            let key = (instance_id, index);
                            if runtime
                                .lv2_params
                                .get(&key)
                                .is_none_or(|current| (current - param_value).abs() >= 0.0005)
                            {
                                runtime.lv2_params.insert(key, param_value);
                                actions.push(Action::TrackSetLv2ControlValue {
                                    track_name: track.name.clone(),
                                    instance_id,
                                    index,
                                    value: param_value,
                                });
                            }
                        }
                    }
                    #[cfg(not(all(unix, not(target_os = "macos"))))]
                    TrackAutomationTarget::Lv2Parameter { .. } => {}
                    TrackAutomationTarget::Vst3Parameter {
                        instance_id,
                        param_id,
                    } => {
                        if track.frozen {
                            continue;
                        }
                        if let Some(v) = value {
                            let param_value = v.clamp(0.0, 1.0);
                            let key = (instance_id, param_id);
                            if runtime
                                .vst3_params
                                .get(&key)
                                .is_none_or(|current| (current - param_value).abs() >= 0.0005)
                            {
                                runtime.vst3_params.insert(key, param_value);
                                actions.push(Action::TrackSetVst3Parameter {
                                    track_name: track.name.clone(),
                                    instance_id,
                                    param_id,
                                    value: param_value,
                                });
                            }
                        }
                    }
                    TrackAutomationTarget::ClapParameter {
                        instance_id,
                        param_id,
                        min,
                        max,
                    } => {
                        if track.frozen {
                            continue;
                        }
                        if let Some(v) = value {
                            let lo = min.min(max);
                            let hi = max.max(min);
                            let param_value = (lo + v as f64 * (hi - lo)).clamp(lo, hi);
                            let key = (instance_id, param_id);
                            if runtime
                                .clap_params
                                .get(&key)
                                .is_none_or(|current| (current - param_value).abs() >= 0.0005)
                            {
                                runtime.clap_params.insert(key, param_value);
                                actions.push(Action::TrackSetClapParameterAt {
                                    track_name: track.name.clone(),
                                    instance_id,
                                    param_id,
                                    value: param_value,
                                    frame: 0,
                                });
                            }
                        }
                    }
                }
            }
            if let Some(v) = vol {
                let level_db = (-90.0 + v * 110.0).clamp(-90.0, 20.0);
                if runtime
                    .level_db
                    .is_none_or(|current| (current - level_db).abs() >= 0.1)
                {
                    runtime.level_db = Some(level_db);
                    actions.push(Action::TrackAutomationLevel(track.name.clone(), level_db));
                }
            }
            if let Some(v) = bal {
                let balance = (v * 2.0 - 1.0).clamp(-1.0, 1.0);
                if runtime
                    .balance
                    .is_none_or(|current| (current - balance).abs() >= 0.01)
                {
                    runtime.balance = Some(balance);
                    actions.push(Action::TrackAutomationBalance(track.name.clone(), balance));
                }
            }
            if let Some(v) = muted
                && runtime.muted != Some(v)
            {
                runtime.muted = Some(v);
                actions.push(Action::TrackAutomationMute(track.name.clone(), v));
            }
        }
        actions
    }

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

    fn sysex_to_engine(
        points: &[PianoSysExPoint],
    ) -> Vec<maolan_engine::message::MidiRawEventData> {
        points
            .iter()
            .map(|p| maolan_engine::message::MidiRawEventData {
                sample: p.sample,
                data: p.data.clone(),
            })
            .collect()
    }

    fn sync_editor_scrollbars(&self) -> Task<Message> {
        let x = self.editor_scroll_relative_x();
        let y = self.editor_scroll_y.clamp(0.0, 1.0);
        Task::batch(vec![
            operation::snap_to(
                Id::new(EDITOR_SCROLL_ID),
                operation::RelativeOffset {
                    x: None,
                    y: Some(y),
                },
            ),
            operation::snap_to(
                Id::new(EDITOR_TIMELINE_SCROLL_ID),
                operation::RelativeOffset {
                    x: Some(x),
                    y: None,
                },
            ),
            operation::snap_to(
                Id::new(WORKSPACE_TEMPO_SCROLL_ID),
                operation::RelativeOffset {
                    x: Some(x),
                    y: None,
                },
            ),
            operation::snap_to(
                Id::new(WORKSPACE_RULER_SCROLL_ID),
                operation::RelativeOffset {
                    x: Some(x),
                    y: None,
                },
            ),
            operation::snap_to(
                Id::new(TRACKS_SCROLL_ID),
                operation::RelativeOffset {
                    x: None,
                    y: Some(y),
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
        ])
    }

    fn normalize_period_frames(period_frames: usize) -> usize {
        let v = period_frames.clamp(16, 65536);
        if v.is_power_of_two() {
            v
        } else {
            v.next_power_of_two().min(65536)
        }
    }

    fn smooth_meter_db_levels(current: &mut Vec<f32>, target: &[f32]) {
        if current.len() != target.len() {
            *current = target
                .iter()
                .copied()
                .map(Self::quantize_meter_db)
                .collect();
            return;
        }

        for (cur, tgt) in current.iter_mut().zip(target.iter().copied()) {
            let alpha = if tgt > *cur {
                ATTACK_ALPHA
            } else {
                RELEASE_ALPHA
            };
            *cur = Self::quantize_meter_db((*cur + (tgt - *cur) * alpha).clamp(-90.0, 20.0));
        }
    }

    fn midi_lane_at_position(&self, position: Point) -> Option<(String, usize)> {
        let state = self.state.blocking_read();
        let mut y_offset = 0.0f32;
        for track in state
            .tracks
            .iter()
            .filter(|track| track.name != METRONOME_TRACK_ID)
        {
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

    fn clip_at_position(&self, position: Point) -> Option<(String, Kind, usize)> {
        let pps = self.pixels_per_sample().max(1.0e-6);
        let state = self.state.blocking_read();
        let mut y_offset = 0.0f32;
        for track in state
            .tracks
            .iter()
            .filter(|track| track.name != METRONOME_TRACK_ID)
        {
            let track_top = y_offset;
            let track_bottom = y_offset + track.height;
            if position.y < track_top || position.y > track_bottom {
                y_offset += track.height;
                continue;
            }
            let local_y = (position.y - y_offset).max(0.0);
            let local_x = position.x.max(0.0);
            let layout = track.lane_layout();
            let lane_clip_h = (layout.lane_height - 6.0).max(12.0);

            if track.audio.ins > 0 {
                let audio_top = track.lane_top(Kind::Audio, 0) + 3.0;
                let audio_bottom = audio_top + lane_clip_h;
                if local_y >= audio_top && local_y <= audio_bottom {
                    let (take_idx, take_count) = Self::assign_take_lanes(
                        &track.audio.clips,
                        |_| 0,
                        |clip| clip.start,
                        |clip| clip.length,
                        |clip| clip.take_lane_override,
                    );
                    let overlap = track
                        .audio
                        .clips
                        .iter()
                        .enumerate()
                        .filter(|(_, clip)| {
                            let cx = clip.start as f32 * pps;
                            let cw = (clip.length as f32 * pps).max(MIN_CLIP_WIDTH_PX);
                            local_x >= cx && local_x <= cx + cw
                        })
                        .map(|(idx, _)| idx)
                        .collect::<Vec<_>>();
                    if overlap.is_empty() {
                        return None;
                    }
                    let max_takes = overlap
                        .iter()
                        .filter_map(|idx| take_count.get(*idx).copied())
                        .max()
                        .unwrap_or(1)
                        .max(1);
                    let rel_y = (local_y - audio_top).max(0.0);
                    let slot_h = (lane_clip_h / max_takes as f32).max(1.0);
                    let desired_take = (rel_y / slot_h).floor() as usize;
                    if let Some(idx) = overlap
                        .iter()
                        .find(|idx| take_idx.get(**idx).copied().unwrap_or(0) == desired_take)
                        .copied()
                    {
                        return Some((track.name.clone(), Kind::Audio, idx));
                    }
                    return overlap
                        .iter()
                        .find_map(|idx| take_idx.get(*idx).is_some().then_some(*idx))
                        .map(|idx| (track.name.clone(), Kind::Audio, idx));
                }
            }

            if track.midi.ins > 0 {
                let midi_lane = track
                    .lane_index_at_y(Kind::MIDI, local_y)
                    .min(track.midi.ins.saturating_sub(1));
                let midi_top = track.lane_top(Kind::MIDI, midi_lane) + 3.0;
                let midi_bottom = midi_top + lane_clip_h;
                if local_y >= midi_top && local_y <= midi_bottom {
                    let (take_idx, take_count) = Self::assign_take_lanes(
                        &track.midi.clips,
                        |clip| clip.input_channel.min(track.midi.ins.saturating_sub(1)),
                        |clip| clip.start,
                        |clip| clip.length,
                        |clip| clip.take_lane_override,
                    );
                    let overlap = track
                        .midi
                        .clips
                        .iter()
                        .enumerate()
                        .filter(|(_, clip)| {
                            let lane = clip.input_channel.min(track.midi.ins.saturating_sub(1));
                            if lane != midi_lane {
                                return false;
                            }
                            let cx = clip.start as f32 * pps;
                            let cw = (clip.length as f32 * pps).max(MIN_CLIP_WIDTH_PX);
                            local_x >= cx && local_x <= cx + cw
                        })
                        .map(|(idx, _)| idx)
                        .collect::<Vec<_>>();
                    if overlap.is_empty() {
                        return None;
                    }
                    let max_takes = overlap
                        .iter()
                        .filter_map(|idx| take_count.get(*idx).copied())
                        .max()
                        .unwrap_or(1)
                        .max(1);
                    let rel_y = (local_y - midi_top).max(0.0);
                    let slot_h = (lane_clip_h / max_takes as f32).max(1.0);
                    let desired_take = (rel_y / slot_h).floor() as usize;
                    if let Some(idx) = overlap
                        .iter()
                        .find(|idx| take_idx.get(**idx).copied().unwrap_or(0) == desired_take)
                        .copied()
                    {
                        return Some((track.name.clone(), Kind::MIDI, idx));
                    }
                    return overlap
                        .iter()
                        .find_map(|idx| take_idx.get(*idx).is_some().then_some(*idx))
                        .map(|idx| (track.name.clone(), Kind::MIDI, idx));
                }
            }

            return None;
        }
        None
    }

    fn selected_group_candidate(&self) -> Option<(String, Kind, Vec<usize>)> {
        let state = self.state.blocking_read();
        let mut selected: Vec<_> = state.selected_clips.iter().cloned().collect();
        if selected.len() < 2 {
            return None;
        }
        selected.sort_by_key(|clip| clip.clip_idx);
        let first = selected.first()?.clone();
        if selected
            .iter()
            .any(|clip| clip.track_idx != first.track_idx || clip.kind != first.kind)
        {
            return None;
        }
        let track = state
            .tracks
            .iter()
            .find(|track| track.name == first.track_idx)?;
        let valid = match first.kind {
            Kind::Audio => selected.iter().all(|clip| {
                track
                    .audio
                    .clips
                    .get(clip.clip_idx)
                    .is_some_and(|clip| !clip.is_group())
            }),
            Kind::MIDI => selected.iter().all(|clip| {
                track
                    .midi
                    .clips
                    .get(clip.clip_idx)
                    .is_some_and(|clip| !clip.is_group())
            }),
        };
        if !valid {
            return None;
        }
        Some((
            first.track_idx,
            first.kind,
            selected.into_iter().map(|clip| clip.clip_idx).collect(),
        ))
    }

    fn group_selected_clips(&mut self) -> Task<Message> {
        let Some((track_name, kind, mut clip_indices)) = self.selected_group_candidate() else {
            self.state.blocking_write().message =
                "Select two or more clips of the same type on one track to group".to_string();
            return Task::none();
        };
        clip_indices.sort_unstable();
        let grouped_count = clip_indices.len();
        let mut tasks = vec![self.send(Action::BeginHistoryGroup)];
        match kind {
            Kind::Audio => {
                let Some(track) = self
                    .state
                    .blocking_read()
                    .tracks
                    .iter()
                    .find(|track| track.name == track_name)
                    .cloned()
                else {
                    return Task::none();
                };
                let mut clips: Vec<_> = clip_indices
                    .iter()
                    .filter_map(|idx| track.audio.clips.get(*idx).cloned())
                    .collect();
                if clips.len() < 2 {
                    return Task::none();
                }
                clips.sort_by_key(|clip| clip.start);
                let group_start = clips.iter().map(|clip| clip.start).min().unwrap_or(0);
                let group_end = clips
                    .iter()
                    .map(|clip| clip.start.saturating_add(clip.length))
                    .max()
                    .unwrap_or(group_start);
                let mut grouped_clips = clips;
                for child in &mut grouped_clips {
                    child.start = child.start.saturating_sub(group_start);
                    child.normalize_group_children();
                }
                tasks.push(self.send(Action::RemoveClip {
                    track_name: track_name.clone(),
                    kind,
                    clip_indices: clip_indices.clone(),
                }));
                tasks.push(
                    self.send(Action::AddGroupedClip {
                        track_name: track_name.clone(),
                        kind,
                        audio_clip: Some(Self::audio_clip_to_data(&crate::state::AudioClip {
                            name: "Group".to_string(),
                            start: group_start,
                            length: group_end.saturating_sub(group_start).max(1),
                            offset: 0,
                            input_channel: grouped_clips
                                .first()
                                .map(|clip| clip.input_channel)
                                .unwrap_or(0),
                            muted: grouped_clips.iter().all(|clip| clip.muted),
                            max_length_samples: group_end.saturating_sub(group_start).max(1),
                            peaks_file: None,
                            peaks: Default::default(),
                            fade_enabled: true,
                            fade_in_samples: 240,
                            fade_out_samples: 240,
                            pitch_correction_preview_name: None,
                            pitch_correction_source_name: None,
                            pitch_correction_source_offset: None,
                            pitch_correction_source_length: None,
                            pitch_correction_points: vec![],
                            pitch_correction_frame_likeness: None,
                            pitch_correction_inertia_ms: None,
                            pitch_correction_formant_compensation: None,
                            take_lane_override: None,
                            take_lane_pinned: false,
                            take_lane_locked: false,
                            plugin_graph_json: None,
                            grouped_clips,
                        })),
                        midi_clip: None,
                    }),
                );
            }
            Kind::MIDI => {
                let Some(track) = self
                    .state
                    .blocking_read()
                    .tracks
                    .iter()
                    .find(|track| track.name == track_name)
                    .cloned()
                else {
                    return Task::none();
                };
                let mut clips: Vec<_> = clip_indices
                    .iter()
                    .filter_map(|idx| track.midi.clips.get(*idx).cloned())
                    .collect();
                if clips.len() < 2 {
                    return Task::none();
                }
                clips.sort_by_key(|clip| clip.start);
                let group_start = clips.iter().map(|clip| clip.start).min().unwrap_or(0);
                let group_end = clips
                    .iter()
                    .map(|clip| clip.start.saturating_add(clip.length))
                    .max()
                    .unwrap_or(group_start);
                let mut grouped_clips = clips;
                for child in &mut grouped_clips {
                    child.start = child.start.saturating_sub(group_start);
                    child.normalize_group_children();
                }
                tasks.push(self.send(Action::RemoveClip {
                    track_name: track_name.clone(),
                    kind,
                    clip_indices: clip_indices.clone(),
                }));
                tasks.push(
                    self.send(Action::AddGroupedClip {
                        track_name: track_name.clone(),
                        kind,
                        audio_clip: None,
                        midi_clip: Some(Self::midi_clip_to_data(&crate::state::MIDIClip {
                            name: "Group".to_string(),
                            start: group_start,
                            length: group_end.saturating_sub(group_start).max(1),
                            offset: 0,
                            input_channel: grouped_clips
                                .first()
                                .map(|clip| clip.input_channel)
                                .unwrap_or(0),
                            muted: grouped_clips.iter().all(|clip| clip.muted),
                            max_length_samples: group_end.saturating_sub(group_start).max(1),
                            take_lane_override: None,
                            take_lane_pinned: false,
                            take_lane_locked: false,
                            grouped_clips,
                        })),
                    }),
                );
            }
        }
        tasks.push(self.send(Action::EndHistoryGroup));
        {
            let mut state = self.state.blocking_write();
            state.selected_clips.clear();
            state.clip_context_menu = None;
            state.message = format!("Grouped {} clips", grouped_count);
        }
        Task::batch(tasks)
    }

    fn ungroup_clip(&mut self, track_name: String, clip_idx: usize, kind: Kind) -> Task<Message> {
        let mut tasks = vec![self.send(Action::BeginHistoryGroup)];
        match kind {
            Kind::Audio => {
                let Some(group) = self
                    .state
                    .blocking_read()
                    .tracks
                    .iter()
                    .find(|track| track.name == track_name)
                    .and_then(|track| track.audio.clips.get(clip_idx))
                    .cloned()
                else {
                    return Task::none();
                };
                if !group.is_group() {
                    return Task::none();
                }
                tasks.push(self.send(Action::RemoveClip {
                    track_name: track_name.clone(),
                    kind,
                    clip_indices: vec![clip_idx],
                }));
                for mut child in group.grouped_clips {
                    child.start = child.start.saturating_add(group.start);
                    if child.is_group() {
                        tasks.push(self.send(Action::AddGroupedClip {
                            track_name: track_name.clone(),
                            kind,
                            audio_clip: Some(Self::audio_clip_to_data(&child)),
                            midi_clip: None,
                        }));
                    } else {
                        tasks.push(
                            self.send(Action::AddClip {
                                name: child.name,
                                track_name: track_name.clone(),
                                start: child.start,
                                length: child.length,
                                offset: child.offset,
                                input_channel: child.input_channel,
                                muted: child.muted,
                                peaks_file: child.peaks_file,
                                kind,
                                fade_enabled: child.fade_enabled,
                                fade_in_samples: child.fade_in_samples,
                                fade_out_samples: child.fade_out_samples,
                                source_name: child.pitch_correction_source_name,
                                source_offset: child.pitch_correction_source_offset,
                                source_length: child.pitch_correction_source_length,
                                preview_name: child.pitch_correction_preview_name,
                                pitch_correction_points: child
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
                                pitch_correction_frame_likeness: child
                                    .pitch_correction_frame_likeness,
                                pitch_correction_inertia_ms: child.pitch_correction_inertia_ms,
                                pitch_correction_formant_compensation: child
                                    .pitch_correction_formant_compensation,
                                plugin_graph_json: child.plugin_graph_json,
                            }),
                        );
                    }
                }
            }
            Kind::MIDI => {
                let Some(group) = self
                    .state
                    .blocking_read()
                    .tracks
                    .iter()
                    .find(|track| track.name == track_name)
                    .and_then(|track| track.midi.clips.get(clip_idx))
                    .cloned()
                else {
                    return Task::none();
                };
                if !group.is_group() {
                    return Task::none();
                }
                tasks.push(self.send(Action::RemoveClip {
                    track_name: track_name.clone(),
                    kind,
                    clip_indices: vec![clip_idx],
                }));
                for mut child in group.grouped_clips {
                    child.start = child.start.saturating_add(group.start);
                    if child.is_group() {
                        tasks.push(self.send(Action::AddGroupedClip {
                            track_name: track_name.clone(),
                            kind,
                            audio_clip: None,
                            midi_clip: Some(Self::midi_clip_to_data(&child)),
                        }));
                    } else {
                        tasks.push(self.send(Action::AddClip {
                            name: child.name,
                            track_name: track_name.clone(),
                            start: child.start,
                            length: child.length,
                            offset: child.offset,
                            input_channel: child.input_channel,
                            muted: child.muted,
                            peaks_file: None,
                            kind,
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
                }
            }
        }
        tasks.push(self.send(Action::EndHistoryGroup));
        {
            let mut state = self.state.blocking_write();
            state.selected_clips.clear();
            state.clip_context_menu = None;
            state.message = "Ungrouped clip".to_string();
        }
        Task::batch(tasks)
    }

    fn split_clip_at_position(&mut self, position: Point) -> Task<Message> {
        let Some((track_name, kind, clip_idx)) = self.clip_at_position(position) else {
            return Task::none();
        };
        let pps = self.pixels_per_sample().max(1.0e-6);
        let split_sample = self.snap_sample_to_bar(position.x.max(0.0) / pps);

        match kind {
            Kind::Audio => {
                let Some(clip) = self
                    .state
                    .blocking_read()
                    .tracks
                    .iter()
                    .find(|t| t.name == track_name)
                    .and_then(|t| t.audio.clips.get(clip_idx))
                    .cloned()
                else {
                    return Task::none();
                };
                let clip_end = clip.start.saturating_add(clip.length);
                if clip.take_lane_locked {
                    self.state.blocking_write().message =
                        "Cannot split a take-lane locked clip".to_string();
                    return Task::none();
                }
                if clip.is_group() {
                    self.state.blocking_write().message = "Cannot split a grouped clip".to_string();
                    return Task::none();
                }
                if split_sample <= clip.start || split_sample >= clip_end {
                    return Task::none();
                }
                let left_len = split_sample.saturating_sub(clip.start);
                let right_len = clip_end.saturating_sub(split_sample);
                if left_len == 0 || right_len == 0 {
                    return Task::none();
                }
                let left_fade_in = clip.fade_in_samples.min(left_len / 2);
                let left_fade_out = clip.fade_out_samples.min(left_len / 2);
                let right_fade_in = clip.fade_in_samples.min(right_len / 2);
                let right_fade_out = clip.fade_out_samples.min(right_len / 2);
                self.state.blocking_write().message = format!("Split audio clip '{}'", clip.name);
                let mut tasks = vec![self.send(Action::BeginHistoryGroup)];
                tasks.push(self.send(Action::RemoveClip {
                    track_name: track_name.clone(),
                    kind: Kind::Audio,
                    clip_indices: vec![clip_idx],
                }));
                tasks.push(
                    self.send(Action::AddClip {
                        name: clip.name.clone(),
                        track_name: track_name.clone(),
                        start: clip.start,
                        length: left_len,
                        offset: clip.offset,
                        input_channel: clip.input_channel,
                        muted: clip.muted,
                        peaks_file: clip.peaks_file.clone(),
                        kind: Kind::Audio,
                        fade_enabled: clip.fade_enabled,
                        fade_in_samples: left_fade_in,
                        fade_out_samples: left_fade_out,
                        source_name: clip.pitch_correction_source_name.clone(),
                        source_offset: clip.pitch_correction_source_offset,
                        source_length: clip
                            .pitch_correction_source_length
                            .map(|value| value.min(left_len)),
                        preview_name: None,
                        pitch_correction_points: vec![],
                        pitch_correction_frame_likeness: None,
                        pitch_correction_inertia_ms: None,
                        pitch_correction_formant_compensation: None,
                        plugin_graph_json: clip.plugin_graph_json.clone(),
                    }),
                );
                tasks.push(
                    self.send(Action::AddClip {
                        name: clip.name,
                        track_name,
                        start: split_sample,
                        length: right_len,
                        offset: clip.offset.saturating_add(left_len),
                        input_channel: clip.input_channel,
                        muted: clip.muted,
                        peaks_file: clip.peaks_file,
                        kind: Kind::Audio,
                        fade_enabled: clip.fade_enabled,
                        fade_in_samples: right_fade_in,
                        fade_out_samples: right_fade_out,
                        source_name: clip.pitch_correction_source_name,
                        source_offset: clip
                            .pitch_correction_source_offset
                            .map(|value| value.saturating_add(left_len)),
                        source_length: clip
                            .pitch_correction_source_length
                            .map(|value| value.saturating_sub(left_len)),
                        preview_name: None,
                        pitch_correction_points: vec![],
                        pitch_correction_frame_likeness: None,
                        pitch_correction_inertia_ms: None,
                        pitch_correction_formant_compensation: None,
                        plugin_graph_json: clip.plugin_graph_json,
                    }),
                );
                tasks.push(self.send(Action::EndHistoryGroup));
                Task::batch(tasks)
            }
            Kind::MIDI => {
                let Some(clip) = self
                    .state
                    .blocking_read()
                    .tracks
                    .iter()
                    .find(|t| t.name == track_name)
                    .and_then(|t| t.midi.clips.get(clip_idx))
                    .cloned()
                else {
                    return Task::none();
                };
                let clip_end = clip.start.saturating_add(clip.length);
                if clip.take_lane_locked {
                    self.state.blocking_write().message =
                        "Cannot split a take-lane locked clip".to_string();
                    return Task::none();
                }
                if clip.is_group() {
                    self.state.blocking_write().message = "Cannot split a grouped clip".to_string();
                    return Task::none();
                }
                if split_sample <= clip.start || split_sample >= clip_end {
                    return Task::none();
                }
                let left_len = split_sample.saturating_sub(clip.start);
                let right_len = clip_end.saturating_sub(split_sample);
                if left_len == 0 || right_len == 0 {
                    return Task::none();
                }
                self.state.blocking_write().message = format!("Split MIDI clip '{}'", clip.name);
                let mut tasks = vec![self.send(Action::BeginHistoryGroup)];
                tasks.push(self.send(Action::RemoveClip {
                    track_name: track_name.clone(),
                    kind: Kind::MIDI,
                    clip_indices: vec![clip_idx],
                }));
                tasks.push(self.send(Action::AddClip {
                    name: clip.name.clone(),
                    track_name: track_name.clone(),
                    start: clip.start,
                    length: left_len,
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
                tasks.push(self.send(Action::AddClip {
                    name: clip.name,
                    track_name,
                    start: split_sample,
                    length: right_len,
                    offset: clip.offset.saturating_add(left_len),
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
                tasks.push(self.send(Action::EndHistoryGroup));
                Task::batch(tasks)
            }
        }
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
            muted: false,
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
        })
    }

    fn open_clip_pitch_correction(&mut self, track_idx: String, clip_idx: usize) -> Task<Message> {
        if self.playing {
            self.state.blocking_write().message =
                "Pitch correction is unavailable while playing or paused".to_string();
            return Task::none();
        }
        let Some(session_root) = self.session_dir.clone() else {
            self.state.blocking_write().message =
                "Pitch correction requires an opened/saved session".to_string();
            return Task::none();
        };
        let clip = {
            let state = self.state.blocking_read();
            state
                .tracks
                .iter()
                .find(|t| t.name == track_idx)
                .and_then(|t| t.audio.clips.get(clip_idx))
                .cloned()
        };
        let Some(clip) = clip else {
            self.state.blocking_write().message = "Audio clip not found".to_string();
            return Task::none();
        };
        let request = {
            let state = self.state.blocking_read();
            ClipPitchCorrectionRequest {
                track_idx: track_idx.clone(),
                clip_idx,
                clip_name: clip.name.clone(),
                start: clip.start,
                source_name: clip
                    .pitch_correction_source_name
                    .clone()
                    .unwrap_or_else(|| clip.name.clone()),
                source_offset: clip.pitch_correction_source_offset.unwrap_or(clip.offset),
                source_length: clip.pitch_correction_source_length.unwrap_or(clip.length),
                frame_likeness: clip
                    .pitch_correction_frame_likeness
                    .unwrap_or(state.pitch_correction_frame_likeness),
            }
        };
        if !clip.pitch_correction_points.is_empty() {
            self.state.blocking_write().message =
                format!("Opened pitch correction for '{}'", request.clip_name);
            return Task::done(Message::ClipOpenPitchCorrectionFinished {
                request: request.clone(),
                result: Ok(crate::state::PitchCorrectionData {
                    track_idx: request.track_idx.clone(),
                    clip_index: request.clip_idx,
                    clip_name: request.clip_name.clone(),
                    clip_length_samples: request.source_length,
                    frame_likeness: request.frame_likeness,
                    raw_points: clip.pitch_correction_points.clone(),
                    points: clip.pitch_correction_points,
                }),
            });
        }
        let source_path = if std::path::Path::new(&request.source_name).is_absolute() {
            std::path::PathBuf::from(&request.source_name)
        } else {
            session_root.join(&request.source_name)
        };
        if !source_path.exists() {
            self.state.blocking_write().message =
                format!("Audio clip source is missing: {}", source_path.display());
            return Task::none();
        }

        if let Ok(Some(pitch_correction)) =
            Self::load_cached_pitch_correction(&session_root, &source_path, &request)
        {
            self.state.blocking_write().message =
                format!("Opened cached pitch correction for '{}'", request.clip_name);
            return Task::done(Message::ClipOpenPitchCorrectionFinished {
                request,
                result: Ok(pitch_correction),
            });
        }

        self.state.blocking_write().message =
            format!("Opening pitch correction for '{}'...", request.clip_name);
        self.clip_pitch_correction_in_progress = true;
        self.clip_pitch_correction_progress = 0.0;
        self.clip_pitch_correction_clip_name = request.clip_name.clone();
        self.clip_pitch_correction_operation = Some("Starting".to_string());
        let session_root_for_cache = session_root.clone();
        Task::run(
            {
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                tokio::spawn(async move {
                    let tx_clone = tx.clone();
                    let clip_name_for_progress = request.clip_name.clone();
                    let mut last_progress_bucket: Option<u16> = None;
                    let mut last_operation: Option<String> = None;
                    let progress_fn = move |progress: f32, operation: Option<String>| {
                        let clamped = progress.clamp(0.0, 1.0);
                        let bucket = (clamped * 100.0).round() as u16;
                        if last_progress_bucket == Some(bucket) && last_operation == operation {
                            return;
                        }
                        last_progress_bucket = Some(bucket);
                        last_operation = operation.clone();
                        if tx_clone
                            .send(Message::ClipOpenPitchCorrectionProgress {
                                clip_name: clip_name_for_progress.clone(),
                                progress: clamped,
                                operation,
                            })
                            .is_err()
                        {}
                    };

                    let result = Self::analyze_audio_clip_pitch_correction(
                        &source_path,
                        &request.clip_name,
                        request.source_offset,
                        request.source_length,
                        request.frame_likeness,
                        progress_fn,
                    )
                    .await
                    .map_err(|e| e.to_string());
                    if let Ok(ref pitch_correction) = result
                        && let Err(err) = Self::save_cached_pitch_correction(
                            &session_root_for_cache,
                            &source_path,
                            &request,
                            pitch_correction,
                        )
                    {
                        error!(
                            "Failed to cache pitch correction for '{}': {}",
                            request.clip_name, err
                        );
                    }
                    if tx
                        .send(Message::ClipOpenPitchCorrectionFinished { request, result })
                        .is_err()
                    {
                        return;
                    }
                    drop(tx);
                });

                iced::futures::stream::unfold(rx, |mut rx| async move {
                    rx.recv().await.map(|msg| (msg, rx))
                })
            },
            |msg| msg,
        )
    }

    fn start_clip_stretch_request(&mut self, request: ClipStretchRequest) -> Task<Message> {
        let Some(session_root) = self.session_dir.clone() else {
            self.state.blocking_write().message =
                "Stretching audio clips requires an opened/saved session".to_string();
            return Task::none();
        };
        let source_path = if std::path::Path::new(&request.clip_name).is_absolute() {
            std::path::PathBuf::from(&request.clip_name)
        } else {
            session_root.join(&request.clip_name)
        };
        self.start_clip_stretch_request_with_source(request, session_root, source_path)
    }

    fn start_clip_stretch_request_with_source(
        &mut self,
        request: ClipStretchRequest,
        session_root: std::path::PathBuf,
        source_path: std::path::PathBuf,
    ) -> Task<Message> {
        if !*RUBBERBAND_AVAILABLE {
            self.state.blocking_write().message =
                "Clip stretching is unavailable because 'rubberband' is not installed or not on PATH"
                    .to_string();
            return Task::none();
        }
        if !source_path.exists() {
            self.state.blocking_write().message =
                format!("Audio clip source is missing: {}", source_path.display());
            return Task::none();
        }
        Task::perform(
            async move {
                let result = Self::stretch_audio_clip_with_rubberband(
                    &source_path,
                    &session_root,
                    &request.clip_name,
                    request.offset,
                    request.length,
                    request.stretch_ratio,
                )
                .await
                .map_err(|e| e.to_string());
                (request, result)
            },
            |(request, result)| Message::ClipStretchFinished { request, result },
        )
    }

    fn current_pitch_correction_action(&self) -> Option<Action> {
        let (
            preview_name,
            track_name,
            clip_index,
            source_name,
            source_offset,
            source_length,
            points,
            frame_likeness,
            inertia_ms,
            formant_compensation,
        ) = ({
            let state = self.state.blocking_read();
            let pitch_correction = state.pitch_correction.as_ref()?;
            state
                .tracks
                .iter()
                .find(|t| t.name == pitch_correction.track_idx)
                .and_then(|t| t.audio.clips.get(pitch_correction.clip_index))
                .cloned()
                .map(|clip| {
                    (
                        clip.pitch_correction_preview_name.clone(),
                        pitch_correction.track_idx.clone(),
                        pitch_correction.clip_index,
                        clip.pitch_correction_source_name
                            .clone()
                            .unwrap_or_else(|| clip.name.clone()),
                        clip.pitch_correction_source_offset.unwrap_or(clip.offset),
                        clip.pitch_correction_source_length.unwrap_or(clip.length),
                        pitch_correction.points.clone(),
                        pitch_correction.frame_likeness,
                        state.pitch_correction_inertia_ms.min(1000),
                        state.pitch_correction_formant_compensation,
                    )
                })
        })?;

        Some(Action::SetClipPitchCorrection {
            track_name,
            clip_index,
            preview_name,
            source_name: Some(source_name),
            source_offset: Some(source_offset),
            source_length: Some(source_length),
            pitch_correction_points: points
                .into_iter()
                .map(|point| maolan_engine::message::PitchCorrectionPointData {
                    start_sample: point.start_sample,
                    length_samples: point.length_samples,
                    detected_midi_pitch: point.detected_midi_pitch,
                    target_midi_pitch: point.target_midi_pitch,
                    clarity: point.clarity,
                })
                .collect(),
            pitch_correction_frame_likeness: Some(frame_likeness),
            pitch_correction_inertia_ms: Some(inertia_ms),
            pitch_correction_formant_compensation: Some(formant_compensation),
        })
    }

    fn sync_pitch_correction_realtime(&mut self) -> Task<Message> {
        let Some(action) = self.current_pitch_correction_action() else {
            self.state.blocking_write().message = "Audio clip not found".to_string();
            return Task::none();
        };
        self.send(action)
    }

    fn snap_pitch_correction_points_to_nearest(&mut self, point_index: usize) -> Task<Message> {
        let mut state = self.state.blocking_write();
        let selection = if state
            .pitch_correction_selected_points
            .contains(&point_index)
        {
            let mut indices: Vec<usize> = state
                .pitch_correction_selected_points
                .iter()
                .copied()
                .collect();
            indices.sort_unstable();
            indices
        } else {
            state.pitch_correction_selected_points.clear();
            state.pitch_correction_selected_points.insert(point_index);
            vec![point_index]
        };
        state.pitch_correction_dragging_points = None;
        state.pitch_correction_selecting_rect = None;
        let before_selection = state.pitch_correction_selected_points.clone();

        let Some(pitch_correction) = state.pitch_correction.as_mut() else {
            return Task::none();
        };
        let before_points = pitch_correction.points.clone();
        let mut snapped = 0usize;
        for idx in selection.iter().copied() {
            if let Some(point) = pitch_correction.points.get_mut(idx) {
                let snapped_pitch = point
                    .target_midi_pitch
                    .round()
                    .clamp(0.0, f32::from(PITCH_MAX));
                if (point.target_midi_pitch - snapped_pitch).abs() > f32::EPSILON {
                    point.target_midi_pitch = snapped_pitch;
                    snapped = snapped.saturating_add(1);
                }
            }
        }
        state.message = if snapped == 0 {
            if selection.len() == 1 {
                "Pitch segment is already on the nearest note".to_string()
            } else {
                "Selected pitch segments are already on the nearest notes".to_string()
            }
        } else if selection.len() == 1 {
            "Snapped pitch segment to nearest note".to_string()
        } else {
            format!("Snapped {} pitch segments to nearest notes", snapped)
        };
        drop(state);
        if snapped > 0 {
            self.push_pitch_correction_history(before_points, before_selection);
            return self.sync_pitch_correction_realtime();
        }
        Task::none()
    }

    fn schedule_audio_peak_rebuild(
        &mut self,
        track_name: &str,
        clip_name: &str,
        start: usize,
        length: usize,
        offset: usize,
        wav_path: std::path::PathBuf,
    ) -> Option<Task<Message>> {
        let key = Self::audio_clip_key(track_name, clip_name, start, length, offset);
        if !self.pending_peak_rebuilds.insert(key) {
            return None;
        }

        let track_name = track_name.to_string();
        let clip_name = clip_name.to_string();
        std::thread::spawn(move || {
            if Self::stream_audio_clip_peaks_to_queue(
                &wav_path,
                track_name.clone(),
                clip_name.clone(),
                start,
                length,
                offset,
            )
            .is_err()
                && let Ok(mut queue) = AUDIO_PEAK_UPDATES.lock()
            {
                queue.push(super::AudioPeakChunkUpdate {
                    track_name,
                    clip_name,
                    start,
                    length,
                    offset,
                    channels: 0,
                    target_bins: 0,
                    bin_start: 0,
                    peaks: Vec::new(),
                    done: true,
                });
            }
        });
        Some(Task::none())
    }

    fn schedule_audio_peak_file_load(
        &mut self,
        track_name: &str,
        clip_name: &str,
        start: usize,
        length: usize,
        offset: usize,
        peaks_path: std::path::PathBuf,
    ) -> Option<Task<Message>> {
        let key = Self::audio_clip_key(track_name, clip_name, start, length, offset);
        if !self.pending_peak_rebuilds.insert(key) {
            return None;
        }
        let track_name = track_name.to_string();
        let clip_name = clip_name.to_string();
        std::thread::spawn(move || {
            if Self::stream_peak_file_to_queue(
                &peaks_path,
                track_name.clone(),
                clip_name.clone(),
                start,
                length,
                offset,
            )
            .is_err()
                && let Ok(mut queue) = AUDIO_PEAK_UPDATES.lock()
            {
                queue.push(super::AudioPeakChunkUpdate {
                    track_name,
                    clip_name,
                    start,
                    length,
                    offset,
                    channels: 0,
                    target_bins: 0,
                    bin_start: 0,
                    peaks: Vec::new(),
                    done: true,
                });
            }
        });
        Some(Task::none())
    }
}

#[cfg(all(test, unix, not(target_os = "macos")))]
mod tests {
    use super::Maolan;
    use crate::state::{AudioClip, PitchCorrectionData, PitchCorrectionPoint, Track};
    use maolan_engine::message::Action;

    #[test]
    fn clip_plugin_graph_json_or_default_uses_track_io_counts() {
        let graph = Maolan::clip_plugin_graph_json_or_default(None, 1, 1);
        let connections = graph["connections"].as_array().expect("connections array");

        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0]["from_node"], serde_json::json!("TrackInput"));
        assert_eq!(connections[0]["to_node"], serde_json::json!("TrackOutput"));
        assert_eq!(connections[0]["from_port"].as_u64(), Some(0));
        assert_eq!(connections[0]["to_port"].as_u64(), Some(0));
    }

    #[test]
    fn current_pitch_correction_action_uses_editor_state_immediately() {
        let app = Maolan::default();
        {
            let mut state = app.state.blocking_write();
            let mut track = Track::new("track".to_string(), 1.0, 1, 1, 0, 0);
            track.audio.clips.push(AudioClip {
                name: "audio/clip.wav".to_string(),
                start: 12,
                length: 4800,
                offset: 24,
                pitch_correction_source_name: Some("audio/source.wav".to_string()),
                pitch_correction_source_offset: Some(48),
                pitch_correction_source_length: Some(4096),
                pitch_correction_preview_name: Some("audio/preview.wav".to_string()),
                ..AudioClip::default()
            });
            state.tracks.push(track);
            state.pitch_correction = Some(PitchCorrectionData {
                track_idx: "track".to_string(),
                clip_index: 0,
                clip_name: "audio/clip.wav".to_string(),
                clip_length_samples: 4800,
                frame_likeness: 0.75,
                raw_points: vec![],
                points: vec![PitchCorrectionPoint {
                    start_sample: 64,
                    length_samples: 128,
                    detected_midi_pitch: 60.25,
                    target_midi_pitch: 61.5,
                    clarity: 0.9,
                }],
            });
            state.pitch_correction_inertia_ms = 333;
            state.pitch_correction_formant_compensation = false;
        }

        let action = app
            .current_pitch_correction_action()
            .expect("pitch correction action");

        match action {
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
                assert_eq!(track_name, "track");
                assert_eq!(clip_index, 0);
                assert_eq!(preview_name.as_deref(), Some("audio/preview.wav"));
                assert_eq!(source_name.as_deref(), Some("audio/source.wav"));
                assert_eq!(source_offset, Some(48));
                assert_eq!(source_length, Some(4096));
                assert_eq!(pitch_correction_points.len(), 1);
                assert_eq!(pitch_correction_points[0].target_midi_pitch, 61.5);
                assert_eq!(pitch_correction_frame_likeness, Some(0.75));
                assert_eq!(pitch_correction_inertia_ms, Some(333));
                assert_eq!(pitch_correction_formant_compensation, Some(false));
            }
            other => panic!("unexpected action: {other:?}"),
        }
    }

    #[test]
    fn normalize_period_frames_power_of_two() {
        assert_eq!(Maolan::normalize_period_frames(64), 64);
        assert_eq!(Maolan::normalize_period_frames(512), 512);
        assert_eq!(Maolan::normalize_period_frames(4096), 4096);
    }

    #[test]
    fn normalize_period_frames_non_power_of_two() {
        assert_eq!(Maolan::normalize_period_frames(100), 128);
        assert_eq!(Maolan::normalize_period_frames(500), 512);
        assert_eq!(Maolan::normalize_period_frames(3000), 4096);
    }

    #[test]
    fn normalize_period_frames_clamps_to_bounds() {
        assert_eq!(Maolan::normalize_period_frames(5), 16);
        assert_eq!(Maolan::normalize_period_frames(100000), 65536);
    }

    #[test]
    fn quantize_meter_db_clamps_and_steps() {
        assert_eq!(Maolan::quantize_meter_db(-100.0), -90.0);
        assert_eq!(Maolan::quantize_meter_db(25.0), 20.0);
        // Steps of 1.0 dB
        assert_eq!(Maolan::quantize_meter_db(5.3), 5.0);
        assert_eq!(Maolan::quantize_meter_db(5.7), 6.0);
    }

    #[test]
    fn deterministic_note_jitter_is_deterministic() {
        let j1 = Maolan::deterministic_note_jitter(100, 200, 10);
        let j2 = Maolan::deterministic_note_jitter(100, 200, 10);
        assert_eq!(j1, j2);
    }

    #[test]
    fn deterministic_note_jitter_respects_amplitude() {
        let j1 = Maolan::deterministic_note_jitter(100, 200, 10);
        assert!((-10..=10).contains(&j1));
    }

    #[test]
    fn nearest_scale_pitch_major() {
        // C major scale: C=0, D=2, E=4, F=5, G=7, A=9, B=11
        assert_eq!(Maolan::nearest_scale_pitch(0, 0, false), 0); // C
        assert_eq!(Maolan::nearest_scale_pitch(1, 0, false), 0); // C# -> C
        assert_eq!(Maolan::nearest_scale_pitch(6, 0, false), 5); // F# -> F
    }

    #[test]
    fn nearest_scale_pitch_minor() {
        // C minor scale: C=0, D=2, D#=3, F=5, G=7, G#=8, A#=10
        assert_eq!(Maolan::nearest_scale_pitch(0, 0, true), 0); // C
        assert_eq!(Maolan::nearest_scale_pitch(1, 0, true), 0); // C# -> C
        assert_eq!(Maolan::nearest_scale_pitch(4, 0, true), 3); // E -> D#
    }

    #[test]
    fn format_sysex_hex_formats_bytes() {
        assert_eq!(Maolan::format_sysex_hex(&[0xF0, 0x01, 0x02]), "F0 01 02");
        assert_eq!(Maolan::format_sysex_hex(&[]), "");
    }

    #[test]
    fn parse_sysex_hex_parses_valid_hex() {
        // Parser adds 0xF0 at start and 0xF7 at end if not present
        assert_eq!(
            Maolan::parse_sysex_hex("01 02").unwrap(),
            vec![0xF0, 0x01, 0x02, 0xF7]
        );
        assert_eq!(
            Maolan::parse_sysex_hex("f0,01,02").unwrap(),
            vec![0xF0, 0x01, 0x02, 0xF7]
        );
    }

    #[test]
    fn parse_sysex_hex_rejects_invalid() {
        assert!(Maolan::parse_sysex_hex("GG HH").is_err());
        assert!(Maolan::parse_sysex_hex("").is_err());
    }

    #[test]
    fn timing_at_sample_basic() {
        let state = crate::state::StateData {
            tempo: 120.0,
            time_signature_num: 4,
            time_signature_denom: 4,
            hw_sample_rate_hz: 48000,
            ..Default::default()
        };

        let (bpm, num, denom) = Maolan::timing_at_sample(&state, 0);
        assert_eq!(bpm, 120.0);
        assert_eq!(num, 4);
        assert_eq!(denom, 4);
    }

    #[test]
    fn automation_key_equality() {
        use crate::message::TrackAutomationTarget;

        let key1 = Maolan::automation_key(TrackAutomationTarget::Volume);
        let key2 = Maolan::automation_key(TrackAutomationTarget::Volume);
        let key3 = Maolan::automation_key(TrackAutomationTarget::Balance);

        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
    }

    #[test]
    fn key_has_explicit_gesture_lifecycle() {
        use super::AutomationWriteKey;

        // Only CLAP keys have explicit gesture lifecycle
        assert!(!Maolan::key_has_explicit_gesture_lifecycle(
            AutomationWriteKey::Mute
        ));
        assert!(!Maolan::key_has_explicit_gesture_lifecycle(
            AutomationWriteKey::Volume
        ));
        assert!(Maolan::key_has_explicit_gesture_lifecycle(
            AutomationWriteKey::Clap {
                instance_id: 0,
                param_id: 0
            }
        ));
    }
}
