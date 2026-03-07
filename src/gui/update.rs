use super::{
    AUDIO_PEAK_UPDATES, AutomationWriteKey, CLIENT, MIN_CLIP_WIDTH_PX, Maolan,
    TouchAutomationOverride, platform,
};
#[cfg(any(target_os = "windows", target_os = "macos"))]
use crate::message::PluginFormat;
use crate::{
    connections,
    message::{
        ExportNormalizeMode, ExportRenderMode, Message, Show, TrackAutomationMode,
        TrackAutomationTarget,
    },
    state::{
        ConnectionViewSelection, HW, PianoData, PianoSysExPoint, Resizing, TempoPoint,
        TimeSignaturePoint, Track, TrackAutomationLane, TrackAutomationPoint, View,
    },
    ui_timing::DOUBLE_CLICK,
    widget::piano::{CTRL_SCROLL_ID, H_SCROLL_ID, KEYS_SCROLL_ID, NOTES_SCROLL_ID, V_SCROLL_ID},
    workspace::{
        EDITOR_H_SCROLL_ID, EDITOR_SCROLL_ID, PIANO_RULER_SCROLL_ID, PIANO_TEMPO_SCROLL_ID,
        TRACKS_SCROLL_ID,
    },
};
use iced::widget::{Id, operation};
use iced::{Length, Point, Task, mouse};
#[cfg(any(target_os = "windows", all(unix, not(target_os = "macos"))))]
use maolan_engine::message::PluginGraphNode;
use maolan_engine::{
    history,
    kind::Kind,
    message::{
        Action, AudioWarpMarker, ClipMoveFrom, ClipMoveTo, Message as EngineMessage,
        OfflineAutomationLane, OfflineAutomationPoint, OfflineAutomationTarget,
    },
};
use rfd::AsyncFileDialog;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    process::exit,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tracing::error;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct MidiMappingsGlobalFile {
    play_pause: Option<maolan_engine::message::MidiLearnBinding>,
    stop: Option<maolan_engine::message::MidiLearnBinding>,
    record_toggle: Option<maolan_engine::message::MidiLearnBinding>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct MidiMappingsTrackFile {
    volume: Option<maolan_engine::message::MidiLearnBinding>,
    balance: Option<maolan_engine::message::MidiLearnBinding>,
    mute: Option<maolan_engine::message::MidiLearnBinding>,
    solo: Option<maolan_engine::message::MidiLearnBinding>,
    arm: Option<maolan_engine::message::MidiLearnBinding>,
    input_monitor: Option<maolan_engine::message::MidiLearnBinding>,
    disk_monitor: Option<maolan_engine::message::MidiLearnBinding>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct MidiMappingsFile {
    global: MidiMappingsGlobalFile,
    tracks: HashMap<String, MidiMappingsTrackFile>,
}

const AUTOSAVE_SNAPSHOT_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Clone)]
struct AutomationTrackView {
    name: String,
    automation_mode: TrackAutomationMode,
    automation_lanes: Vec<TrackAutomationLane>,
    frozen: bool,
}

impl Maolan {
    fn autosave_snapshot_root(&self) -> Option<std::path::PathBuf> {
        self.session_dir
            .as_ref()
            .map(|session_dir| session_dir.join(".maolan_autosave"))
    }

    fn autosave_snapshots_dir_for(path: &std::path::Path) -> std::path::PathBuf {
        path.join(".maolan_autosave/snapshots")
    }

    fn list_autosave_snapshots_for(path: &std::path::Path) -> Vec<std::path::PathBuf> {
        let snapshots_dir = Self::autosave_snapshots_dir_for(path);
        let mut snapshots = fs::read_dir(snapshots_dir)
            .ok()
            .into_iter()
            .flat_map(|entries| entries.filter_map(|entry| entry.ok()))
            .map(|entry| entry.path())
            .filter(|snapshot_dir| {
                snapshot_dir.is_dir() && snapshot_dir.join("session.json").exists()
            })
            .collect::<Vec<_>>();
        snapshots.sort_by(|a, b| b.cmp(a));
        snapshots
    }

    fn has_newer_autosave_snapshot(path: &std::path::Path) -> bool {
        let Some(autosave_session) = Self::list_autosave_snapshots_for(path).first().cloned()
        else {
            return false;
        };
        let autosave_mtime = fs::metadata(autosave_session.join("session.json"))
            .and_then(|m| m.modified())
            .ok();
        let session_mtime = fs::metadata(path.join("session.json"))
            .and_then(|m| m.modified())
            .ok();
        match (autosave_mtime, session_mtime) {
            (Some(a), Some(s)) => a > s,
            (Some(_), None) => true,
            _ => false,
        }
    }

    fn autosave_recovery_preview_summary(
        session_dir: &std::path::Path,
        snapshot_dir: &std::path::Path,
    ) -> String {
        fn read_counts(path: &std::path::Path) -> Option<(usize, usize, usize)> {
            let f = fs::File::open(path).ok()?;
            let json: serde_json::Value = serde_json::from_reader(f).ok()?;
            let tracks = json.get("tracks")?.as_array()?;
            let track_count = tracks.len();
            let mut audio_count = 0usize;
            let mut midi_count = 0usize;
            for track in tracks {
                audio_count = audio_count.saturating_add(
                    track
                        .get("audio")
                        .and_then(|a| a.get("clips"))
                        .and_then(serde_json::Value::as_array)
                        .map(|clips| clips.len())
                        .unwrap_or(0),
                );
                midi_count = midi_count.saturating_add(
                    track
                        .get("midi")
                        .and_then(|m| m.get("clips"))
                        .and_then(serde_json::Value::as_array)
                        .map(|clips| clips.len())
                        .unwrap_or(0),
                );
            }
            Some((track_count, audio_count, midi_count))
        }

        let live = read_counts(&session_dir.join("session.json"));
        let snap = read_counts(&snapshot_dir.join("session.json"));
        let label = snapshot_dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("snapshot");
        match (live, snap) {
            (Some((lt, la, lm)), Some((st, sa, sm))) => format!(
                "Autosave preview [{label}]: tracks {lt}->{st}, audio clips {la}->{sa}, midi clips {lm}->{sm}"
            ),
            _ => format!("Autosave preview [{label}]: unable to compute diff summary"),
        }
    }

    fn write_last_session_hint(path: &str) {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let config_dir = std::path::PathBuf::from(home).join(".config/maolan");
        let _ = fs::create_dir_all(&config_dir);
        let _ = fs::write(config_dir.join("last_session_path"), path);
    }

    fn export_diagnostics_bundle(&self) -> Result<std::path::PathBuf, String> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let root = self
            .session_dir
            .clone()
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
            .join(format!("maolan_diagnostics_{stamp}"));
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;

        let state = self.state.blocking_read();
        let diagnostics = state
            .diagnostics_report
            .clone()
            .unwrap_or_else(|| "No diagnostics report captured yet".to_string());
        fs::write(root.join("session_diagnostics.txt"), diagnostics).map_err(|e| e.to_string())?;
        fs::write(
            root.join("midi_mappings.txt"),
            if self.midi_mappings_report_lines.is_empty() {
                "No MIDI mappings captured yet".to_string()
            } else {
                self.midi_mappings_report_lines.join("\n")
            },
        )
        .map_err(|e| e.to_string())?;

        let summary = serde_json::json!({
            "transport": {
                "playing": self.playing,
                "paused": self.paused,
                "sample": self.transport_samples.max(0.0) as usize,
            },
            "session_dir": self.session_dir.as_ref().map(|p| p.to_string_lossy().to_string()),
            "track_count": state.tracks.len(),
            "selected_tracks": state.selected.iter().cloned().collect::<Vec<String>>(),
            "export_in_progress": self.export_in_progress,
            "freeze_in_progress": self.freeze_in_progress,
            "record_armed": self.record_armed,
            "timestamp_unix": stamp,
        });
        let f = fs::File::create(root.join("ui_summary.json")).map_err(|e| e.to_string())?;
        serde_json::to_writer_pretty(f, &summary).map_err(|e| e.to_string())?;
        Ok(root)
    }

    fn prepare_pending_autosave_recovery(&mut self, select_older: bool) -> Result<(), String> {
        if let Some(pending) = self.pending_autosave_recovery.as_mut() {
            if select_older {
                if pending.selected_index + 1 < pending.snapshots.len() {
                    pending.selected_index += 1;
                } else {
                    return Err("No older autosave snapshot available".to_string());
                }
            }
            pending.confirm_armed = false;
            return Ok(());
        }

        let base_session_dir = self
            .pending_recovery_session_dir
            .clone()
            .or_else(|| self.session_dir.clone())
            .ok_or_else(|| "No session available for autosave recovery".to_string())?;
        let snapshots = Self::list_autosave_snapshots_for(&base_session_dir);
        if snapshots.is_empty() {
            return Err("No autosave snapshot found for this session".to_string());
        }
        let selected_index = if select_older {
            if snapshots.len() >= 2 {
                1
            } else {
                return Err("No older autosave snapshot available".to_string());
            }
        } else {
            0
        };
        self.pending_autosave_recovery = Some(super::PendingAutosaveRecovery {
            session_dir: base_session_dir,
            snapshots,
            selected_index,
            confirm_armed: false,
        });
        Ok(())
    }

    fn apply_pending_autosave_recovery(&mut self) -> Task<Message> {
        let Some(pending) = self.pending_autosave_recovery.clone() else {
            self.state.blocking_write().message = "No autosave recovery pending".to_string();
            return Task::none();
        };
        if let Some(snapshot) = pending.snapshots.get(pending.selected_index) {
            self.session_dir = Some(pending.session_dir.clone());
            self.stop_recording_preview();
            self.pending_recovery_session_dir = None;
            self.pending_autosave_recovery = None;
            self.pending_open_session_dir = None;
            self.has_unsaved_changes = true;
            self.state.blocking_write().message =
                format!("Recovering autosave snapshot '{}'...", snapshot.display());
            let snapshot = snapshot.clone();
            return Task::perform(async move { snapshot }, Message::LoadSessionPath);
        }
        self.pending_autosave_recovery = None;
        self.pending_open_session_dir = None;
        self.state.blocking_write().message =
            "Autosave recovery failed: no snapshot available".to_string();
        Task::none()
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
            (base_lane(clip), start_sample(clip), *idx)
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
            if self
                .midi_clip_previews
                .contains_key(&(track_name.clone(), clip_idx))
            {
                continue;
            }
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

    fn warp_markers_for_speed(clip_length: usize, speed: f32) -> Vec<AudioWarpMarker> {
        let speed = speed.max(0.01);
        let source_end = ((clip_length as f64) * speed as f64).round().max(1.0) as usize;
        vec![
            AudioWarpMarker {
                timeline_sample: 0,
                source_sample: 0,
            },
            AudioWarpMarker {
                timeline_sample: clip_length,
                source_sample: source_end,
            },
        ]
    }

    fn add_warp_marker_between(
        markers: &[AudioWarpMarker],
        clip_length: usize,
    ) -> Vec<AudioWarpMarker> {
        let mut out = markers.to_vec();
        let timeline_mid = clip_length / 2;
        if out.iter().any(|m| m.timeline_sample == timeline_mid) {
            return out;
        }
        let source_mid = if out.is_empty() {
            timeline_mid
        } else {
            let mut points = out
                .iter()
                .map(|m| (m.timeline_sample.min(clip_length), m.source_sample))
                .collect::<Vec<_>>();
            points.push((0, 0));
            points.push((clip_length, clip_length));
            points.sort_unstable_by_key(|(t, _)| *t);
            points.dedup_by_key(|(t, _)| *t);
            let mut mapped = timeline_mid;
            for window in points.windows(2) {
                let (x0, y0) = window[0];
                let (x1, y1) = window[1];
                if timeline_mid < x0 || timeline_mid > x1 {
                    continue;
                }
                if x1 == x0 {
                    mapped = y0;
                } else {
                    let t = (timeline_mid - x0) as f64 / (x1 - x0) as f64;
                    mapped = (y0 as f64 + (y1 as f64 - y0 as f64) * t).round() as usize;
                }
                break;
            }
            mapped
        };
        out.push(AudioWarpMarker {
            timeline_sample: timeline_mid,
            source_sample: source_mid,
        });
        out.sort_unstable_by_key(|m| m.timeline_sample);
        out.dedup_by_key(|m| m.timeline_sample);
        out
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
        let master = track
            .vca_master
            .clone()
            .unwrap_or_else(|| track.name.clone());
        let mut members = vec![master.clone()];
        members.extend(
            state
                .tracks
                .iter()
                .filter(|t| t.vca_master.as_deref() == Some(master.as_str()))
                .map(|t| t.name.clone()),
        );
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
        let x = self.editor_scroll_x.clamp(0.0, 1.0);
        let y = self.editor_scroll_y.clamp(0.0, 1.0);
        Task::batch(vec![
            operation::snap_to(
                Id::new(EDITOR_SCROLL_ID),
                operation::RelativeOffset {
                    x: Some(x),
                    y: Some(y),
                },
            ),
            operation::snap_to(
                Id::new(EDITOR_H_SCROLL_ID),
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

    fn clip_at_position(&self, position: Point) -> Option<(String, Kind, usize)> {
        let pps = self.pixels_per_sample().max(1.0e-6);
        let state = self.state.blocking_read();
        let mut y_offset = 0.0f32;
        for track in &state.tracks {
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
                if !clip.warp_markers.is_empty() {
                    self.state.blocking_write().message =
                        "Split for warped audio clips is not supported yet".to_string();
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
                tasks.push(self.send(Action::AddClip {
                    name: clip.name.clone(),
                    track_name: track_name.clone(),
                    start: clip.start,
                    length: left_len,
                    offset: clip.offset,
                    input_channel: clip.input_channel,
                    muted: clip.muted,
                    kind: Kind::Audio,
                    fade_enabled: clip.fade_enabled,
                    fade_in_samples: left_fade_in,
                    fade_out_samples: left_fade_out,
                    warp_markers: vec![],
                }));
                tasks.push(self.send(Action::AddClip {
                    name: clip.name,
                    track_name,
                    start: split_sample,
                    length: right_len,
                    offset: clip.offset.saturating_add(left_len),
                    input_channel: clip.input_channel,
                    muted: clip.muted,
                    kind: Kind::Audio,
                    fade_enabled: clip.fade_enabled,
                    fade_in_samples: right_fade_in,
                    fade_out_samples: right_fade_out,
                    warp_markers: vec![],
                }));
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
                    kind: Kind::MIDI,
                    fade_enabled: clip.fade_enabled,
                    fade_in_samples: left_fade_in,
                    fade_out_samples: left_fade_out,
                    warp_markers: vec![],
                }));
                tasks.push(self.send(Action::AddClip {
                    name: clip.name,
                    track_name,
                    start: split_sample,
                    length: right_len,
                    offset: clip.offset.saturating_add(left_len),
                    input_channel: clip.input_channel,
                    muted: clip.muted,
                    kind: Kind::MIDI,
                    fade_enabled: clip.fade_enabled,
                    fade_in_samples: right_fade_in,
                    fade_out_samples: right_fade_out,
                    warp_markers: vec![],
                }));
                tasks.push(self.send(Action::EndHistoryGroup));
                Task::batch(tasks)
            }
        }
    }

    fn comp_target_at_position(&self, position: Point) -> Option<(String, Kind, usize, usize)> {
        let pps = self.pixels_per_sample().max(1.0e-6);
        let sample = (position.x.max(0.0) / pps) as usize;
        let state = self.state.blocking_read();
        let mut y_offset = 0.0_f32;
        for track in &state.tracks {
            let track_top = y_offset;
            let track_bottom = y_offset + track.height;
            if position.y < track_top || position.y > track_bottom {
                y_offset += track.height;
                continue;
            }
            let local_y = (position.y - y_offset).max(0.0);
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
                            let end = clip.start.saturating_add(clip.length);
                            clip.start <= sample && sample < end
                        })
                        .map(|(idx, _)| idx)
                        .collect::<Vec<_>>();
                    let max_takes = overlap
                        .iter()
                        .filter_map(|idx| take_count.get(*idx).copied())
                        .max()
                        .unwrap_or(1)
                        .max(1);
                    let rel_y = (local_y - audio_top).max(0.0);
                    let slot_h = (lane_clip_h / max_takes as f32).max(1.0);
                    let desired_take = (rel_y / slot_h).floor() as usize;
                    if let Some(found) = overlap
                        .iter()
                        .filter_map(|idx| take_idx.get(*idx).copied())
                        .find(|idx| *idx == desired_take)
                    {
                        return Some((track.name.clone(), Kind::Audio, 0, found));
                    }
                    return Some((track.name.clone(), Kind::Audio, 0, desired_take));
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
                            let end = clip.start.saturating_add(clip.length);
                            let base = clip.input_channel.min(track.midi.ins.saturating_sub(1));
                            base == midi_lane && clip.start <= sample && sample < end
                        })
                        .map(|(idx, _)| idx)
                        .collect::<Vec<_>>();
                    let max_takes = overlap
                        .iter()
                        .filter_map(|idx| take_count.get(*idx).copied())
                        .max()
                        .unwrap_or(1)
                        .max(1);
                    let rel_y = (local_y - midi_top).max(0.0);
                    let slot_h = (lane_clip_h / max_takes as f32).max(1.0);
                    let desired_take = (rel_y / slot_h).floor() as usize;
                    if let Some(found) = overlap
                        .iter()
                        .filter_map(|idx| take_idx.get(*idx).copied())
                        .find(|idx| *idx == desired_take)
                    {
                        return Some((track.name.clone(), Kind::MIDI, midi_lane, found));
                    }
                    return Some((track.name.clone(), Kind::MIDI, midi_lane, desired_take));
                }
            }
            return None;
        }
        None
    }

    fn comp_swipe_updates(
        &self,
        track_name: &str,
        kind: Kind,
        base_lane: usize,
        take_lane: usize,
        start_sample: usize,
        end_sample: usize,
    ) -> Vec<(usize, bool)> {
        let state = self.state.blocking_read();
        let Some(track) = state.tracks.iter().find(|t| t.name == track_name) else {
            return Vec::new();
        };
        match kind {
            Kind::Audio => {
                let (take_idx, _) = Self::assign_take_lanes(
                    &track.audio.clips,
                    |_| 0,
                    |clip| clip.start,
                    |clip| clip.length,
                    |clip| clip.take_lane_override,
                );
                track
                    .audio
                    .clips
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, clip)| {
                        if clip.take_lane_locked {
                            return None;
                        }
                        let end = clip.start.saturating_add(clip.length);
                        if clip.start >= end_sample || end <= start_sample || base_lane != 0 {
                            return None;
                        }
                        let should_mute = take_idx.get(idx).copied().unwrap_or(0) != take_lane;
                        (clip.muted != should_mute).then_some((idx, should_mute))
                    })
                    .collect()
            }
            Kind::MIDI => {
                let (take_idx, _) = Self::assign_take_lanes(
                    &track.midi.clips,
                    |clip| clip.input_channel.min(track.midi.ins.saturating_sub(1)),
                    |clip| clip.start,
                    |clip| clip.length,
                    |clip| clip.take_lane_override,
                );
                track
                    .midi
                    .clips
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, clip)| {
                        if clip.take_lane_locked {
                            return None;
                        }
                        let end = clip.start.saturating_add(clip.length);
                        let lane = clip.input_channel.min(track.midi.ins.saturating_sub(1));
                        if clip.start >= end_sample || end <= start_sample || lane != base_lane {
                            return None;
                        }
                        let should_mute = take_idx.get(idx).copied().unwrap_or(0) != take_lane;
                        (clip.muted != should_mute).then_some((idx, should_mute))
                    })
                    .collect()
            }
        }
    }

    fn apply_comp_swipe(&mut self) -> Task<Message> {
        let (start, end) = {
            let mut state = self.state.blocking_write();
            let start = state.comp_swipe_start.take();
            let end = state.comp_swipe_end.take();
            (start, end)
        };
        let (Some(start), Some(end)) = (start, end) else {
            return Task::none();
        };
        if (start.x - end.x).abs() <= 1.0 || (start.y - end.y).abs() <= 1.0 {
            return Task::none();
        }
        let pps = self.pixels_per_sample().max(1.0e-6);
        let swipe_start_sample = (start.x.min(end.x).max(0.0) / pps) as usize;
        let swipe_end_sample = (start.x.max(end.x).max(0.0) / pps) as usize;
        if swipe_end_sample <= swipe_start_sample {
            return Task::none();
        }
        let target_pos = Point::new(start.x, (start.y + end.y) * 0.5);
        let Some((track_name, kind, base_lane, take_lane)) =
            self.comp_target_at_position(target_pos)
        else {
            return Task::none();
        };
        let updates = self.comp_swipe_updates(
            &track_name,
            kind,
            base_lane,
            take_lane,
            swipe_start_sample,
            swipe_end_sample,
        );
        if updates.is_empty() {
            return Task::none();
        }
        let mut tasks = vec![self.send(Action::BeginHistoryGroup)];
        for (idx, muted) in updates {
            tasks.push(self.send(Action::SetClipMuted {
                track_name: track_name.clone(),
                clip_index: idx,
                kind,
                muted,
            }));
        }
        tasks.push(self.send(Action::EndHistoryGroup));
        Task::batch(tasks)
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
            kind: Kind::MIDI,
            fade_enabled: true,
            fade_in_samples: 240,
            fade_out_samples: 240,
            warp_markers: vec![],
        })
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

    pub fn update(&mut self, message: Message) -> Task<Message> {
        if !matches!(message, Message::WindowCloseRequested) {
            self.close_confirm_pending = false;
        }
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
                    self.track_automation_runtime.clear();
                    self.touch_automation_overrides.clear();
                    self.touch_active_keys.clear();
                    self.latch_automation_overrides.clear();
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
                if self.has_unsaved_changes && !self.close_confirm_pending {
                    self.close_confirm_pending = true;
                    self.state.blocking_write().message =
                        "Unsaved changes detected. Close again to discard, or save the session."
                            .to_string();
                    return Task::none();
                }
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
                    Show::SessionMetadata => {
                        self.modal = Some(Show::SessionMetadata);
                    }
                    Show::Preferences => {
                        self.modal = Some(Show::Preferences);
                    }
                    Show::AutosaveRecovery => {
                        self.modal = Some(Show::AutosaveRecovery);
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
                    audio_ins,
                    midi_ins,
                    audio_outs,
                    midi_outs,
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
                self.state.blocking_write().message =
                    format!("Loading template '{}'...", template_name);
                return Task::perform(
                    async move { std::path::PathBuf::from(template_path) },
                    Message::LoadSessionPath,
                );
            }
            Message::NewSession => {
                if !self.state.blocking_read().hw_loaded {
                    return Task::none();
                }
                self.playing = false;
                self.paused = false;
                self.transport_samples = 0.0;
                self.track_automation_runtime.clear();
                self.touch_automation_overrides.clear();
                self.touch_active_keys.clear();
                self.latch_automation_overrides.clear();
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
                self.pending_peak_file_loads.clear();
                self.pending_peak_rebuilds.clear();
                self.midi_clip_previews.clear();
                self.pending_midi_clip_previews.clear();
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
                    self.send(Action::BeginSessionRestore),
                    self.send(Action::Stop),
                    self.send(Action::SetRecordEnabled(false)),
                    self.send(Action::SetLoopRange(None)),
                    self.send(Action::SetPunchRange(None)),
                    self.send(Action::SetGlobalMidiLearnBinding {
                        target: maolan_engine::message::GlobalMidiLearnTarget::PlayPause,
                        binding: None,
                    }),
                    self.send(Action::SetGlobalMidiLearnBinding {
                        target: maolan_engine::message::GlobalMidiLearnTarget::Stop,
                        binding: None,
                    }),
                    self.send(Action::SetGlobalMidiLearnBinding {
                        target: maolan_engine::message::GlobalMidiLearnTarget::RecordToggle,
                        binding: None,
                    }),
                ];
                for name in existing_tracks {
                    tasks.push(self.send(Action::RemoveTrack(name)));
                }
                tasks.push(self.send(Action::EndSessionRestore));
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
                    state.global_midi_learn_play_pause = None;
                    state.global_midi_learn_stop = None;
                    state.global_midi_learn_record_toggle = None;
                    state.session_author.clear();
                    state.session_album.clear();
                    state.session_year.clear();
                    state.session_track_number.clear();
                    state.session_genre.clear();
                    state.message = "New session".to_string();
                    state.piano = None;
                }
                self.pending_track_freeze_restore.clear();
                self.pending_track_freeze_bounce.clear();
                self.freeze_in_progress = false;
                self.freeze_progress = 0.0;
                self.freeze_track_name = None;
                self.freeze_cancel_requested = false;
                self.has_unsaved_changes = false;
                self.session_restore_in_progress = false;
                self.last_autosave_snapshot = None;
                self.pending_recovery_session_dir = None;
                self.pending_autosave_recovery = None;
                self.pending_open_session_dir = None;
                self.pending_diagnostics_bundle_export = false;
                self.diagnostics_bundle_wait_session_report = false;
                self.diagnostics_bundle_wait_midi_report = false;
                return Task::batch(tasks);
            }
            Message::Cancel => self.modal = None,
            Message::Request(ref a) => {
                if let Some(expanded) = self.expand_request_to_vca_group(a) {
                    let mut tasks = Vec::with_capacity(expanded.len());
                    for action in expanded {
                        self.maybe_record_automation_from_request(&action);
                        tasks.push(self.send(action));
                    }
                    return Task::batch(tasks);
                }
                self.maybe_record_automation_from_request(a);
                return self.send(a.clone());
            }
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
                self.track_automation_runtime.clear();
                self.touch_automation_overrides.clear();
                self.touch_active_keys.clear();
                self.latch_automation_overrides.clear();
                self.stop_recording_preview();
                return Task::batch(vec![
                    self.send(Action::SetClipPlaybackEnabled(true)),
                    self.send(Action::Stop),
                ]);
            }
            Message::JumpToStart => {
                self.transport_samples = 0.0;
                self.track_automation_runtime.clear();
                self.touch_automation_overrides.clear();
                self.touch_active_keys.clear();
                self.latch_automation_overrides.clear();
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
                let mut now_sample = self.transport_samples.max(0.0) as usize;
                if self.playing
                    && !self.paused
                    && let Some(last) = self.last_playback_tick
                {
                    let now = Instant::now();
                    let delta_s = now.duration_since(last).as_secs_f64();
                    self.last_playback_tick = Some(now);
                    self.transport_samples += delta_s * self.playback_rate_hz;
                    now_sample = self.transport_samples.max(0.0) as usize;
                }
                let mut tasks = Vec::new();
                {
                    let mut state = self.state.blocking_write();
                    let (bpm, num, den) = Self::timing_at_sample(&state, now_sample);
                    let tempo_changed = (state.tempo - bpm).abs() > 0.0001;
                    let ts_changed =
                        state.time_signature_num != num || state.time_signature_denom != den;
                    if tempo_changed || ts_changed {
                        state.tempo = bpm;
                        state.time_signature_num = num;
                        state.time_signature_denom = den;
                        self.tempo_input = format!("{:.2}", bpm);
                        self.time_signature_num_input = num.to_string();
                        self.time_signature_denom_input = den.to_string();
                    }
                    if self
                        .last_sent_tempo_bpm
                        .is_none_or(|prev| (prev - bpm as f64).abs() > 0.0001)
                    {
                        self.last_sent_tempo_bpm = Some(bpm as f64);
                        tasks.push(self.send(Action::SetTempo(bpm as f64)));
                    }
                    if self
                        .last_sent_time_signature
                        .is_none_or(|prev| prev != (num as u16, den as u16))
                    {
                        self.last_sent_time_signature = Some((num as u16, den as u16));
                        tasks.push(self.send(Action::SetTimeSignature {
                            numerator: num as u16,
                            denominator: den as u16,
                        }));
                    }
                }
                if self.playing && !self.paused {
                    let tracks = {
                        let state = self.state.blocking_read();
                        state
                            .tracks
                            .iter()
                            .map(|track| AutomationTrackView {
                                name: track.name.clone(),
                                automation_mode: track.automation_mode,
                                automation_lanes: track.automation_lanes.clone(),
                                frozen: track.frozen,
                            })
                            .collect::<Vec<_>>()
                    };
                    let actions = self.collect_track_automation_actions(now_sample, &tracks);
                    if !actions.is_empty() {
                        tasks.extend(actions.into_iter().map(|a| self.send(a)));
                    }
                }
                if !tasks.is_empty() {
                    return Task::batch(tasks);
                }
            }
            Message::AutosaveSnapshotTick => {
                if !self.has_unsaved_changes
                    || self.session_restore_in_progress
                    || self.pending_save_path.is_some()
                {
                    return Task::none();
                }
                let Some(autosave_root) = self.autosave_snapshot_root() else {
                    return Task::none();
                };
                let now = Instant::now();
                if self
                    .last_autosave_snapshot
                    .is_some_and(|last| now.duration_since(last) < AUTOSAVE_SNAPSHOT_INTERVAL)
                {
                    return Task::none();
                }
                let stamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let snapshot_dir = autosave_root.join("snapshots").join(format!("{stamp}"));
                match self.save(snapshot_dir.to_string_lossy().to_string()) {
                    Ok(()) => {
                        self.last_autosave_snapshot = Some(now);
                        let mut snapshots = self
                            .session_dir
                            .as_ref()
                            .map(|path| Self::list_autosave_snapshots_for(path))
                            .unwrap_or_default();
                        const AUTOSAVE_KEEP_COUNT: usize = 10;
                        if snapshots.len() > AUTOSAVE_KEEP_COUNT {
                            snapshots.sort();
                            let remove_count = snapshots.len().saturating_sub(AUTOSAVE_KEEP_COUNT);
                            for stale in snapshots.into_iter().take(remove_count) {
                                let _ = fs::remove_dir_all(stale);
                            }
                        }
                    }
                    Err(e) => {
                        self.state.blocking_write().message = format!("Autosave failed: {e}");
                    }
                }
                return Task::none();
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
            Message::TempoAdjust(delta) => {
                let sample = self.transport_samples.max(0.0) as usize;
                let mut state = self.state.blocking_write();
                let selected_samples: Vec<usize> =
                    self.selected_tempo_points.iter().copied().collect();
                let current_bpm = if let Some(sel) = selected_samples.first().copied() {
                    state
                        .tempo_points
                        .iter()
                        .find(|p| p.sample == sel)
                        .map(|p| p.bpm)
                        .unwrap_or(state.tempo)
                } else {
                    state
                        .tempo_points
                        .iter()
                        .filter(|p| p.sample <= sample)
                        .max_by_key(|p| p.sample)
                        .map(|p| p.bpm)
                        .unwrap_or(state.tempo)
                };
                let tempo = (current_bpm + delta).clamp(20.0, 300.0);
                if !selected_samples.is_empty() {
                    for point in state.tempo_points.iter_mut() {
                        if selected_samples.contains(&point.sample) {
                            point.bpm = tempo;
                        }
                    }
                } else if let Some(point) = state
                    .tempo_points
                    .iter_mut()
                    .filter(|p| p.sample <= sample)
                    .max_by_key(|p| p.sample)
                {
                    point.bpm = tempo;
                } else {
                    state.tempo_points.push(TempoPoint {
                        sample: 0,
                        bpm: tempo,
                    });
                }
                state.tempo_points.sort_unstable_by_key(|p| p.sample);
                state.tempo = tempo;
                self.tempo_input = format!("{:.2}", tempo);
                drop(state);
                self.last_sent_tempo_bpm = Some(tempo as f64);
                return self.send(Action::SetTempo(tempo as f64));
            }
            Message::TempoPointAdd(sample) => {
                let mut state = self.state.blocking_write();
                let (bpm, _, _) = Self::timing_at_sample(&state, sample);
                if let Some(existing) = state.tempo_points.iter_mut().find(|p| p.sample == sample) {
                    existing.bpm = bpm;
                } else {
                    state.tempo_points.push(TempoPoint { sample, bpm });
                    state.tempo_points.sort_unstable_by_key(|p| p.sample);
                }
                self.selected_tempo_points.clear();
                self.selected_tempo_points.insert(sample);
                self.selected_time_signature_points.clear();
                self.timing_selection_lane = Some(super::TimingSelectionLane::Tempo);
                drop(state);
                self.sync_timing_inputs_from_selection();
                return self.update(Message::PlaybackTick);
            }
            Message::TempoPointSelect { sample, additive } => {
                if additive {
                    if !self.selected_tempo_points.insert(sample) {
                        self.selected_tempo_points.remove(&sample);
                    }
                } else {
                    self.selected_tempo_points.clear();
                    self.selected_tempo_points.insert(sample);
                }
                self.selected_time_signature_points.clear();
                self.timing_selection_lane = if self.selected_tempo_points.is_empty() {
                    None
                } else {
                    Some(super::TimingSelectionLane::Tempo)
                };
                self.sync_timing_inputs_from_selection();
            }
            Message::TempoPointsMove {
                from_samples,
                to_samples,
            } => {
                if from_samples.is_empty() || from_samples.len() != to_samples.len() {
                    return Task::none();
                }
                let mut state = self.state.blocking_write();
                let mut moved_values: Vec<f32> = Vec::new();
                for sample in from_samples.iter().copied() {
                    if sample == 0 {
                        continue;
                    }
                    if let Some(idx) = state.tempo_points.iter().position(|p| p.sample == sample) {
                        moved_values.push(state.tempo_points[idx].bpm);
                        state.tempo_points.remove(idx);
                    }
                }
                for (to, bpm) in to_samples.iter().copied().zip(moved_values.into_iter()) {
                    if let Some(existing) = state.tempo_points.iter_mut().find(|p| p.sample == to) {
                        existing.bpm = bpm;
                    } else {
                        state.tempo_points.push(TempoPoint { sample: to, bpm });
                    }
                }
                state.tempo_points.sort_unstable_by_key(|p| p.sample);
                drop(state);
                self.selected_tempo_points.clear();
                self.selected_tempo_points.extend(to_samples);
                self.selected_time_signature_points.clear();
                self.timing_selection_lane = Some(super::TimingSelectionLane::Tempo);
                self.sync_timing_inputs_from_selection();
                return self.update(Message::PlaybackTick);
            }
            Message::TempoSelectionDuplicate => {
                if self.selected_tempo_points.is_empty() {
                    return Task::none();
                }
                let mut state = self.state.blocking_write();
                let mut inserted = Vec::new();
                for sample in self.selected_tempo_points.iter().copied() {
                    let Some(point) = state
                        .tempo_points
                        .iter()
                        .find(|p| p.sample == sample)
                        .cloned()
                    else {
                        continue;
                    };
                    let new_sample = sample
                        .saturating_add(self.samples_per_beat().round() as usize)
                        .max(1);
                    if let Some(existing) = state
                        .tempo_points
                        .iter_mut()
                        .find(|p| p.sample == new_sample)
                    {
                        existing.bpm = point.bpm;
                    } else {
                        state.tempo_points.push(TempoPoint {
                            sample: new_sample,
                            bpm: point.bpm,
                        });
                    }
                    inserted.push(new_sample);
                }
                state.tempo_points.sort_unstable_by_key(|p| p.sample);
                drop(state);
                self.selected_tempo_points.clear();
                self.selected_tempo_points.extend(inserted);
                self.selected_time_signature_points.clear();
                self.timing_selection_lane = Some(super::TimingSelectionLane::Tempo);
                self.sync_timing_inputs_from_selection();
                return self.update(Message::PlaybackTick);
            }
            Message::TempoSelectionResetToPrevious => {
                if self.selected_tempo_points.is_empty() {
                    return Task::none();
                }
                let mut state = self.state.blocking_write();
                let samples: Vec<usize> = self.selected_tempo_points.iter().copied().collect();
                for sample in samples {
                    let previous_bpm = state
                        .tempo_points
                        .iter()
                        .filter(|p| p.sample < sample)
                        .max_by_key(|p| p.sample)
                        .map(|p| p.bpm)
                        .unwrap_or(state.tempo);
                    if let Some(point) = state.tempo_points.iter_mut().find(|p| p.sample == sample)
                    {
                        point.bpm = previous_bpm;
                    }
                }
                drop(state);
                self.sync_timing_inputs_from_selection();
                return self.update(Message::PlaybackTick);
            }
            Message::TempoSelectionDelete => {
                if self.selected_tempo_points.is_empty() {
                    return Task::none();
                }
                let selected: Vec<usize> = self.selected_tempo_points.iter().copied().collect();
                let mut state = self.state.blocking_write();
                state
                    .tempo_points
                    .retain(|p| p.sample == 0 || selected.binary_search(&p.sample).is_err());
                drop(state);
                self.selected_tempo_points.clear();
                self.timing_selection_lane = None;
                return self.update(Message::PlaybackTick);
            }
            Message::TimeSignaturePointAdd(sample) => {
                let mut state = self.state.blocking_write();
                let (_, numerator, denominator) = Self::timing_at_sample(&state, sample);
                if let Some(existing) = state
                    .time_signature_points
                    .iter_mut()
                    .find(|p| p.sample == sample)
                {
                    existing.numerator = numerator;
                    existing.denominator = denominator;
                } else {
                    state.time_signature_points.push(TimeSignaturePoint {
                        sample,
                        numerator,
                        denominator,
                    });
                    state
                        .time_signature_points
                        .sort_unstable_by_key(|p| p.sample);
                }
                self.selected_time_signature_points.clear();
                self.selected_time_signature_points.insert(sample);
                self.selected_tempo_points.clear();
                self.timing_selection_lane = Some(super::TimingSelectionLane::TimeSignature);
                drop(state);
                self.sync_timing_inputs_from_selection();
                return self.update(Message::PlaybackTick);
            }
            Message::TimeSignaturePointSelect { sample, additive } => {
                if additive {
                    if !self.selected_time_signature_points.insert(sample) {
                        self.selected_time_signature_points.remove(&sample);
                    }
                } else {
                    self.selected_time_signature_points.clear();
                    self.selected_time_signature_points.insert(sample);
                }
                self.selected_tempo_points.clear();
                self.timing_selection_lane = if self.selected_time_signature_points.is_empty() {
                    None
                } else {
                    Some(super::TimingSelectionLane::TimeSignature)
                };
                self.sync_timing_inputs_from_selection();
            }
            Message::TimeSignaturePointsMove {
                from_samples,
                to_samples,
            } => {
                if from_samples.is_empty() || from_samples.len() != to_samples.len() {
                    return Task::none();
                }
                let mut state = self.state.blocking_write();
                let mut moved_values: Vec<(u8, u8)> = Vec::new();
                for sample in from_samples.iter().copied() {
                    if sample == 0 {
                        continue;
                    }
                    if let Some(idx) = state
                        .time_signature_points
                        .iter()
                        .position(|p| p.sample == sample)
                    {
                        moved_values.push((
                            state.time_signature_points[idx].numerator,
                            state.time_signature_points[idx].denominator,
                        ));
                        state.time_signature_points.remove(idx);
                    }
                }
                for (to, (numerator, denominator)) in
                    to_samples.iter().copied().zip(moved_values.into_iter())
                {
                    if let Some(existing) = state
                        .time_signature_points
                        .iter_mut()
                        .find(|p| p.sample == to)
                    {
                        existing.numerator = numerator;
                        existing.denominator = denominator;
                    } else {
                        state.time_signature_points.push(TimeSignaturePoint {
                            sample: to,
                            numerator,
                            denominator,
                        });
                    }
                }
                state
                    .time_signature_points
                    .sort_unstable_by_key(|p| p.sample);
                drop(state);
                self.selected_time_signature_points.clear();
                self.selected_time_signature_points.extend(to_samples);
                self.selected_tempo_points.clear();
                self.timing_selection_lane = Some(super::TimingSelectionLane::TimeSignature);
                self.sync_timing_inputs_from_selection();
                return self.update(Message::PlaybackTick);
            }
            Message::TimeSignatureSelectionDuplicate => {
                if self.selected_time_signature_points.is_empty() {
                    return Task::none();
                }
                let mut state = self.state.blocking_write();
                let mut inserted = Vec::new();
                for sample in self.selected_time_signature_points.iter().copied() {
                    let Some(point) = state
                        .time_signature_points
                        .iter()
                        .find(|p| p.sample == sample)
                        .cloned()
                    else {
                        continue;
                    };
                    let new_sample = sample
                        .saturating_add(self.samples_per_beat().round() as usize)
                        .max(1);
                    if let Some(existing) = state
                        .time_signature_points
                        .iter_mut()
                        .find(|p| p.sample == new_sample)
                    {
                        existing.numerator = point.numerator;
                        existing.denominator = point.denominator;
                    } else {
                        state.time_signature_points.push(TimeSignaturePoint {
                            sample: new_sample,
                            numerator: point.numerator,
                            denominator: point.denominator,
                        });
                    }
                    inserted.push(new_sample);
                }
                state
                    .time_signature_points
                    .sort_unstable_by_key(|p| p.sample);
                drop(state);
                self.selected_time_signature_points.clear();
                self.selected_time_signature_points.extend(inserted);
                self.selected_tempo_points.clear();
                self.timing_selection_lane = Some(super::TimingSelectionLane::TimeSignature);
                self.sync_timing_inputs_from_selection();
                return self.update(Message::PlaybackTick);
            }
            Message::TimeSignatureSelectionResetToPrevious => {
                if self.selected_time_signature_points.is_empty() {
                    return Task::none();
                }
                let mut state = self.state.blocking_write();
                let samples: Vec<usize> = self
                    .selected_time_signature_points
                    .iter()
                    .copied()
                    .collect();
                for sample in samples {
                    let (num, den) = state
                        .time_signature_points
                        .iter()
                        .filter(|p| p.sample < sample)
                        .max_by_key(|p| p.sample)
                        .map(|p| (p.numerator, p.denominator))
                        .unwrap_or((state.time_signature_num, state.time_signature_denom));
                    if let Some(point) = state
                        .time_signature_points
                        .iter_mut()
                        .find(|p| p.sample == sample)
                    {
                        point.numerator = num.max(1);
                        point.denominator = den.max(1);
                    }
                }
                drop(state);
                self.sync_timing_inputs_from_selection();
                return self.update(Message::PlaybackTick);
            }
            Message::TimeSignatureSelectionDelete => {
                if self.selected_time_signature_points.is_empty() {
                    return Task::none();
                }
                let selected: Vec<usize> = self
                    .selected_time_signature_points
                    .iter()
                    .copied()
                    .collect();
                let mut state = self.state.blocking_write();
                state
                    .time_signature_points
                    .retain(|p| p.sample == 0 || selected.binary_search(&p.sample).is_err());
                drop(state);
                self.selected_time_signature_points.clear();
                self.timing_selection_lane = None;
                return self.update(Message::PlaybackTick);
            }
            Message::ClearTimingPointSelection => {
                self.selected_tempo_points.clear();
                self.selected_time_signature_points.clear();
                self.timing_selection_lane = None;
            }
            Message::TimeSignatureNumeratorAdjust(delta) => {
                let sample = self.transport_samples.max(0.0) as usize;
                let mut state = self.state.blocking_write();
                let selected_samples: Vec<usize> = self
                    .selected_time_signature_points
                    .iter()
                    .copied()
                    .collect();
                let current = if let Some(sel) = selected_samples.first().copied() {
                    state
                        .time_signature_points
                        .iter()
                        .find(|p| p.sample == sel)
                        .map(|p| i16::from(p.numerator))
                        .unwrap_or(i16::from(state.time_signature_num))
                } else {
                    state
                        .time_signature_points
                        .iter()
                        .filter(|p| p.sample <= sample)
                        .max_by_key(|p| p.sample)
                        .map(|p| i16::from(p.numerator))
                        .unwrap_or(i16::from(state.time_signature_num))
                };
                let next = (current + i16::from(delta)).clamp(1, 16) as u8;
                if !selected_samples.is_empty() {
                    for point in state.time_signature_points.iter_mut() {
                        if selected_samples.contains(&point.sample) {
                            point.numerator = next;
                        }
                    }
                } else if let Some(point) = state
                    .time_signature_points
                    .iter_mut()
                    .filter(|p| p.sample <= sample)
                    .max_by_key(|p| p.sample)
                {
                    point.numerator = next;
                } else {
                    let denominator = state.time_signature_denom.max(1);
                    state.time_signature_points.push(TimeSignaturePoint {
                        sample: 0,
                        numerator: next,
                        denominator,
                    });
                }
                state
                    .time_signature_points
                    .sort_unstable_by_key(|p| p.sample);
                state.time_signature_num = next;
                let numerator = state.time_signature_num as u16;
                let denominator = state.time_signature_denom as u16;
                self.time_signature_num_input = numerator.to_string();
                drop(state);
                self.last_sent_time_signature = Some((numerator, denominator));
                return self.send(Action::SetTimeSignature {
                    numerator,
                    denominator,
                });
            }
            Message::TimeSignatureDenominatorAdjust(delta) => {
                let sample = self.transport_samples.max(0.0) as usize;
                let mut state = self.state.blocking_write();
                let values = [2_u8, 4, 8, 16];
                let selected_samples: Vec<usize> = self
                    .selected_time_signature_points
                    .iter()
                    .copied()
                    .collect();
                let current = if let Some(sel) = selected_samples.first().copied() {
                    state
                        .time_signature_points
                        .iter()
                        .find(|p| p.sample == sel)
                        .map(|p| p.denominator)
                        .unwrap_or(state.time_signature_denom)
                } else {
                    state
                        .time_signature_points
                        .iter()
                        .filter(|p| p.sample <= sample)
                        .max_by_key(|p| p.sample)
                        .map(|p| p.denominator)
                        .unwrap_or(state.time_signature_denom)
                };
                let current_idx = values.iter().position(|v| *v == current).unwrap_or(1) as i16;
                let next_idx = (current_idx + i16::from(delta)).clamp(0, 3) as usize;
                let next = values[next_idx];
                if !selected_samples.is_empty() {
                    for point in state.time_signature_points.iter_mut() {
                        if selected_samples.contains(&point.sample) {
                            point.denominator = next;
                        }
                    }
                } else if let Some(point) = state
                    .time_signature_points
                    .iter_mut()
                    .filter(|p| p.sample <= sample)
                    .max_by_key(|p| p.sample)
                {
                    point.denominator = next;
                } else {
                    let numerator = state.time_signature_num.max(1);
                    state.time_signature_points.push(TimeSignaturePoint {
                        sample: 0,
                        numerator,
                        denominator: next,
                    });
                }
                state
                    .time_signature_points
                    .sort_unstable_by_key(|p| p.sample);
                state.time_signature_denom = next;
                let numerator = state.time_signature_num as u16;
                let denominator = state.time_signature_denom as u16;
                self.time_signature_denom_input = denominator.to_string();
                drop(state);
                self.last_sent_time_signature = Some((numerator, denominator));
                return self.send(Action::SetTimeSignature {
                    numerator,
                    denominator,
                });
            }
            Message::TempoInputChanged(ref value) => {
                self.tempo_input = value.clone();
            }
            Message::TempoInputCommit => {
                let Ok(parsed) = self.tempo_input.trim().parse::<f32>() else {
                    self.state.blocking_write().message = "Invalid BPM value".to_string();
                    return Task::none();
                };
                let bpm = parsed.clamp(20.0, 300.0);
                let sample = self.transport_samples.max(0.0) as usize;
                let mut state = self.state.blocking_write();
                let selected_samples: Vec<usize> =
                    self.selected_tempo_points.iter().copied().collect();
                if !selected_samples.is_empty() {
                    for point in state.tempo_points.iter_mut() {
                        if selected_samples.contains(&point.sample) {
                            point.bpm = bpm;
                        }
                    }
                } else if let Some(point) = state
                    .tempo_points
                    .iter_mut()
                    .filter(|p| p.sample <= sample)
                    .max_by_key(|p| p.sample)
                {
                    point.bpm = bpm;
                } else {
                    state.tempo_points.push(TempoPoint { sample: 0, bpm });
                }
                state.tempo_points.sort_unstable_by_key(|p| p.sample);
                state.tempo = bpm;
                self.tempo_input = format!("{:.2}", bpm);
                drop(state);
                self.last_sent_tempo_bpm = Some(bpm as f64);
                return self.send(Action::SetTempo(bpm as f64));
            }
            Message::TimeSignatureNumeratorInputChanged(ref value) => {
                self.time_signature_num_input = value.clone();
            }
            Message::TimeSignatureDenominatorInputChanged(ref value) => {
                self.time_signature_denom_input = value.clone();
            }
            Message::TimeSignatureInputCommit => {
                let Ok(num) = self.time_signature_num_input.trim().parse::<u16>() else {
                    self.state.blocking_write().message =
                        "Invalid time signature numerator".to_string();
                    return Task::none();
                };
                let Ok(den) = self.time_signature_denom_input.trim().parse::<u16>() else {
                    self.state.blocking_write().message =
                        "Invalid time signature denominator".to_string();
                    return Task::none();
                };
                let numerator = num.clamp(1, 16) as u8;
                let denominator = match den {
                    2 | 4 | 8 | 16 => den as u8,
                    _ => {
                        self.state.blocking_write().message =
                            "Time signature denominator must be 2, 4, 8, or 16".to_string();
                        return Task::none();
                    }
                };
                let sample = self.transport_samples.max(0.0) as usize;
                let mut state = self.state.blocking_write();
                let selected_samples: Vec<usize> = self
                    .selected_time_signature_points
                    .iter()
                    .copied()
                    .collect();
                if !selected_samples.is_empty() {
                    for point in state.time_signature_points.iter_mut() {
                        if selected_samples.contains(&point.sample) {
                            point.numerator = numerator;
                            point.denominator = denominator;
                        }
                    }
                } else if let Some(point) = state
                    .time_signature_points
                    .iter_mut()
                    .filter(|p| p.sample <= sample)
                    .max_by_key(|p| p.sample)
                {
                    point.numerator = numerator;
                    point.denominator = denominator;
                } else {
                    state.time_signature_points.push(TimeSignaturePoint {
                        sample: 0,
                        numerator,
                        denominator,
                    });
                }
                state
                    .time_signature_points
                    .sort_unstable_by_key(|p| p.sample);
                state.time_signature_num = numerator;
                state.time_signature_denom = denominator;
                self.time_signature_num_input = numerator.to_string();
                self.time_signature_denom_input = denominator.to_string();
                drop(state);
                self.last_sent_time_signature = Some((numerator as u16, denominator as u16));
                return self.send(Action::SetTimeSignature {
                    numerator: numerator as u16,
                    denominator: denominator as u16,
                });
            }
            Message::SetSnapMode(mode) => {
                self.snap_mode = mode;
            }
            Message::ToggleCompTool => {
                self.edit_tool = match self.edit_tool {
                    crate::message::EditTool::Select => crate::message::EditTool::Comp,
                    crate::message::EditTool::Comp => crate::message::EditTool::Select,
                };
                if !matches!(self.edit_tool, crate::message::EditTool::Comp) {
                    let mut state = self.state.blocking_write();
                    state.comp_swipe_start = None;
                    state.comp_swipe_end = None;
                }
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
                    let peaks = &mut self.recording_preview_peaks;
                    let state = self.state.blocking_read();
                    for track in state.tracks.iter().filter(|track| track.armed) {
                        let channels = track.audio.outs.max(1);
                        let entry = peaks
                            .entry(track.name.clone())
                            .or_insert_with(|| std::sync::Arc::new(vec![vec![]; channels]));
                        if entry.len() != channels {
                            *entry = std::sync::Arc::new(vec![vec![]; channels]);
                        }
                        let entry_mut = std::sync::Arc::make_mut(entry);
                        for (channel_idx, channel_entry) in
                            entry_mut.iter_mut().enumerate().take(channels)
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
                            channel_entry.push([-amp, amp]);
                        }
                    }
                }
            }
            Message::ZoomVisibleBarsChanged(value) => {
                self.zoom_visible_bars = value.clamp(1.0, 256.0);
                return self.sync_editor_scrollbars();
            }
            Message::EditorScrollChanged { x, y } => {
                let x = x.clamp(0.0, 1.0);
                let y = y.clamp(0.0, 1.0);
                let x_changed = (self.editor_scroll_x - x).abs() > 0.0005;
                let y_changed = (self.editor_scroll_y - y).abs() > 0.0005;
                if x_changed || y_changed {
                    self.editor_scroll_x = x;
                    self.editor_scroll_y = y;
                    return self.sync_editor_scrollbars();
                }
            }
            Message::EditorScrollXChanged(value) => {
                let x = value.clamp(0.0, 1.0);
                if (self.editor_scroll_x - x).abs() > 0.0005 {
                    self.editor_scroll_x = x;
                    return self.sync_editor_scrollbars();
                }
            }
            Message::EditorScrollYChanged(value) => {
                let y = value.clamp(0.0, 1.0);
                if (self.editor_scroll_y - y).abs() > 0.0005 {
                    self.editor_scroll_y = y;
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
                state.piano_sysex_panel_open =
                    matches!(lane, crate::message::PianoControllerLane::SysEx);
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
                if !state.piano_selected_notes.is_empty()
                    && let Some(piano) = state.piano.as_ref()
                {
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
                    let tempo = state.tempo.max(1.0) as f64;
                    let tsig_num = state.time_signature_num.max(1) as f64;
                    let tsig_denom = state.time_signature_denom.max(1) as f64;
                    let row_h = ((14.0 * 7.0 / 12.0) * zoom_y).max(1.0);
                    let tracks_width = match state.tracks_width {
                        Length::Fixed(v) => v,
                        _ => 200.0,
                    };
                    let editor_width = (self.size.width - tracks_width - 3.0).max(1.0);
                    let samples_per_beat =
                        (self.playback_rate_hz * 60.0 / tempo) * (4.0 / tsig_denom);
                    let samples_per_bar = samples_per_beat * tsig_num;
                    let total_samples = (samples_per_bar * self.zoom_visible_bars as f64).max(1.0);
                    let pps = ((editor_width as f64 / total_samples) as f32 * zoom_x).max(1.0e-6);

                    let delta_x = dragging.current_point.x - dragging.start_point.x;
                    let delta_y = dragging.current_point.y - dragging.start_point.y;

                    let delta_samples = (delta_x / pps) as i64;
                    let delta_pitch = -(delta_y / row_h).round() as i8;

                    if copy && let Some(piano) = state.piano.as_ref() {
                        let track_name = piano.track_idx.clone();
                        let clip_idx = piano.clip_index;
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
                        let clip_idx = piano.clip_index;

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
                    let tempo = state.tempo.max(1.0) as f64;
                    let tsig_num = state.time_signature_num.max(1) as f64;
                    let tsig_denom = state.time_signature_denom.max(1) as f64;
                    let tracks_width = match state.tracks_width {
                        Length::Fixed(v) => v,
                        _ => 200.0,
                    };
                    let editor_width = (self.size.width - tracks_width - 3.0).max(1.0);
                    let samples_per_beat =
                        (self.playback_rate_hz * 60.0 / tempo) * (4.0 / tsig_denom);
                    let samples_per_bar = samples_per_beat * tsig_num;
                    let total_samples = (samples_per_bar * self.zoom_visible_bars as f64).max(1.0);
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
                        let clip_idx = piano.clip_index;

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
                let selected_contains = state.piano_selected_notes.contains(&note_index);
                let selected_len = state.piano_selected_notes.len();
                let mut target_indices: Vec<usize> = if selected_contains && selected_len > 1 {
                    state.piano_selected_notes.iter().copied().collect()
                } else {
                    vec![note_index]
                };
                let Some(piano) = state.piano.as_mut() else {
                    return Task::none();
                };
                if note_index >= piano.notes.len() {
                    return Task::none();
                }
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
                let clip_idx = piano.clip_index;
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
                let clip_idx = piano.clip_index;
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
                let clip_idx = piano.clip_index;
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
                let clip_idx = piano.clip_index;
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
                let clip_idx = piano.clip_index;
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
                let old_sysex_events = Self::sysex_to_engine(&piano.sysexes);
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
                let clip_index = piano.clip_index;
                let new_sysex_events = Self::sysex_to_engine(&piano.sysexes);
                let new_hex = Self::format_sysex_hex(&piano.sysexes[new_index].data);
                state.piano_selected_sysex = Some(new_index);
                state.piano_sysex_hex_input = new_hex;
                drop(state);
                return self.send(Action::SetMidiSysExEvents {
                    track_name,
                    clip_index,
                    new_sysex_events,
                    old_sysex_events,
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
                let old_sysex_events = Self::sysex_to_engine(&piano.sysexes);
                piano.sysexes[selected_idx].data = payload;
                let new_hex = Self::format_sysex_hex(&piano.sysexes[selected_idx].data);
                let track_name = piano.track_idx.clone();
                let clip_index = piano.clip_index;
                let new_sysex_events = Self::sysex_to_engine(&piano.sysexes);
                state.piano_sysex_hex_input = new_hex;
                drop(state);
                return self.send(Action::SetMidiSysExEvents {
                    track_name,
                    clip_index,
                    new_sysex_events,
                    old_sysex_events,
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
                let old_sysex_events = Self::sysex_to_engine(&piano.sysexes);
                piano.sysexes.remove(selected_idx);
                let (new_sel, new_hex) = if piano.sysexes.is_empty() {
                    (None, String::new())
                } else {
                    let idx = selected_idx.min(piano.sysexes.len().saturating_sub(1));
                    (Some(idx), Self::format_sysex_hex(&piano.sysexes[idx].data))
                };
                let track_name = piano.track_idx.clone();
                let clip_index = piano.clip_index;
                let new_sysex_events = Self::sysex_to_engine(&piano.sysexes);
                state.piano_selected_sysex = new_sel;
                state.piano_sysex_hex_input = new_hex;
                drop(state);
                return self.send(Action::SetMidiSysExEvents {
                    track_name,
                    clip_index,
                    new_sysex_events,
                    old_sysex_events,
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
                let old_sysex_events = Self::sysex_to_engine(&piano.sysexes);
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
                let clip_index = piano.clip_index;
                let new_sysex_events = Self::sysex_to_engine(&piano.sysexes);
                state.piano_selected_sysex = new_sel;
                state.piano_sysex_hex_input = new_hex;
                drop(state);
                return self.send(Action::SetMidiSysExEvents {
                    track_name,
                    clip_index,
                    new_sysex_events,
                    old_sysex_events,
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
                    let (zoom_x, zoom_y) = if state.piano.is_some() {
                        (state.piano_zoom_x, state.piano_zoom_y)
                    } else {
                        return Task::none();
                    };

                    let tempo = state.tempo.max(1.0) as f64;
                    let tsig_num = state.time_signature_num.max(1) as f64;
                    let tsig_denom = state.time_signature_denom.max(1) as f64;
                    let row_h = ((14.0 * 7.0 / 12.0) * zoom_y).max(1.0);
                    let tracks_width = match state.tracks_width {
                        Length::Fixed(v) => v,
                        _ => 200.0,
                    };
                    let editor_width = (self.size.width - tracks_width - 3.0).max(1.0);
                    let samples_per_beat =
                        (self.playback_rate_hz * 60.0 / tempo) * (4.0 / tsig_denom);
                    let samples_per_bar = samples_per_beat * tsig_num;
                    let total_samples = (samples_per_bar * self.zoom_visible_bars as f64).max(1.0);
                    let pps = ((editor_width as f64 / total_samples) as f32 * zoom_x).max(1.0e-6);

                    let min_x = start.x.min(position.x);
                    let max_x = start.x.max(position.x);
                    let min_y = start.y.min(position.y);
                    let max_y = start.y.max(position.y);

                    let mut selected = std::collections::HashSet::new();
                    if let Some(piano) = state.piano.as_ref() {
                        for (idx, note) in piano.notes.iter().enumerate() {
                            if note.pitch > 119 {
                                continue;
                            }
                            let y_idx = (119 - note.pitch) as usize;
                            let y = y_idx as f32 * row_h + 1.0;
                            let x = note.start_sample as f32 * pps;
                            let w = (note.length_samples as f32 * pps).max(2.0);
                            let h = (row_h - 2.0).max(2.0);

                            if x + w >= min_x && x <= max_x && y + h >= min_y && y <= max_y {
                                selected.insert(idx);
                            }
                        }
                    }
                    state.piano_selected_notes = selected;
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
                let tempo = state.tempo.max(1.0) as f64;
                let tsig_num = state.time_signature_num.max(1) as f64;
                let tsig_denom = state.time_signature_denom.max(1) as f64;
                let samples_per_beat = (self.playback_rate_hz * 60.0 / tempo) * (4.0 / tsig_denom);
                let samples_per_bar = samples_per_beat * tsig_num;
                let total_samples = (samples_per_bar * self.zoom_visible_bars as f64).max(1.0);
                let pps = ((editor_width as f64 / total_samples) as f32 * zoom_x).max(1.0e-6);

                let x0 = start.x.min(end.x).max(0.0);
                let x1 = start.x.max(end.x).max(0.0);
                let raw_start = (x0 / pps).floor().max(0.0) as usize;
                let raw_end = (x1 / pps).ceil().max(raw_start as f32 + 1.0) as usize;
                let snap_interval = match self.snap_mode {
                    crate::message::SnapMode::NoSnap => 1.0,
                    crate::message::SnapMode::Bar => samples_per_bar.max(1.0),
                    crate::message::SnapMode::Beat => samples_per_beat.max(1.0),
                    crate::message::SnapMode::Eighth => (samples_per_beat / 2.0).max(1.0),
                    crate::message::SnapMode::Sixteenth => (samples_per_beat / 4.0).max(1.0),
                    crate::message::SnapMode::ThirtySecond => (samples_per_beat / 8.0).max(1.0),
                    crate::message::SnapMode::SixtyFourth => (samples_per_beat / 16.0).max(1.0),
                };
                let snap_sample = |sample: f32| -> usize {
                    if matches!(self.snap_mode, crate::message::SnapMode::NoSnap) {
                        return sample.max(0.0) as usize;
                    }
                    ((sample.max(0.0) as f64 / snap_interval).round() * snap_interval) as usize
                };
                let start_sample = snap_sample(raw_start as f32);
                let mut end_sample = snap_sample(raw_end as f32);
                let min_len = snap_interval.max(1.0) as usize;
                if end_sample <= start_sample {
                    end_sample = start_sample.saturating_add(min_len);
                }
                let length_samples = end_sample.saturating_sub(start_sample).max(min_len);

                let pitch_row = (start.y / row_h).floor();
                let pitch_row = pitch_row.clamp(0.0, 119.0) as usize;
                let pitch = 119_u8.saturating_sub(pitch_row as u8);

                if let Some(piano) = state.piano.as_ref() {
                    let track_name = piano.track_idx.clone();
                    let clip_idx = piano.clip_index;
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

                if !selected_indices.is_empty()
                    && let Some(piano) = state.piano.as_mut()
                {
                    let track_name = piano.track_idx.clone();
                    let clip_idx = piano.clip_index;
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

                    let note_indices: Vec<usize> = selected_indices.iter().rev().copied().collect();

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
            Message::PianoQuantizeSelectedNotes => {
                let interval = self.snap_interval_samples().max(1);
                let strength = self
                    .state
                    .blocking_read()
                    .piano_quantize_strength
                    .clamp(0.0, 1.0);
                return self.selected_piano_notes_edit(move |_idx, note| {
                    let snapped =
                        ((note.start_sample.saturating_add(interval / 2)) / interval) * interval;
                    let mut out = note.clone();
                    if strength >= 0.999 {
                        out.start_sample = snapped;
                    } else {
                        let cur = note.start_sample as f32;
                        let dst = snapped as f32;
                        out.start_sample = (cur + (dst - cur) * strength).round().max(0.0) as usize;
                    }
                    out
                });
            }
            Message::PianoScaleSelectedNotes => {
                let (root, minor) = {
                    let state = self.state.blocking_read();
                    (state.piano_scale_root.semitone(), state.piano_scale_minor)
                };
                return self.selected_piano_notes_edit(move |_idx, note| {
                    let mut out = note.clone();
                    out.pitch = Self::nearest_scale_pitch(note.pitch, root, minor);
                    out
                });
            }
            Message::PianoChordSelectedNotes => {
                let chord_kind = self.state.blocking_read().piano_chord_kind;
                let state = self.state.blocking_write();
                let selected: Vec<usize> = {
                    let mut v: Vec<usize> = state.piano_selected_notes.iter().copied().collect();
                    v.sort_unstable();
                    v
                };
                if selected.is_empty() {
                    return Task::none();
                }
                let Some(piano) = state.piano.as_ref() else {
                    return Task::none();
                };
                let track_name = piano.track_idx.clone();
                let clip_index = piano.clip_index;
                let mut existing = std::collections::HashSet::<(usize, usize, u8, u8)>::new();
                for note in &piano.notes {
                    existing.insert((
                        note.start_sample,
                        note.length_samples,
                        note.pitch,
                        note.channel,
                    ));
                }
                let mut to_insert: Vec<(usize, maolan_engine::message::MidiNoteData)> = Vec::new();
                let mut next_index = piano.notes.len();
                for idx in selected {
                    let Some(note) = piano.notes.get(idx) else {
                        continue;
                    };
                    for interval in chord_kind.intervals() {
                        let pitch = note.pitch.saturating_add(*interval).min(127);
                        let key = (note.start_sample, note.length_samples, pitch, note.channel);
                        if existing.contains(&key) {
                            continue;
                        }
                        existing.insert(key);
                        to_insert.push((
                            next_index,
                            maolan_engine::message::MidiNoteData {
                                start_sample: note.start_sample,
                                length_samples: note.length_samples,
                                pitch,
                                velocity: note.velocity,
                                channel: note.channel,
                            },
                        ));
                        next_index = next_index.saturating_add(1);
                    }
                }
                drop(state);
                if to_insert.is_empty() {
                    return Task::none();
                }
                return self.send(Action::InsertMidiNotes {
                    track_name,
                    clip_index,
                    notes: to_insert,
                });
            }
            Message::PianoLegatoSelectedNotes => {
                let state = self.state.blocking_read();
                let Some(piano) = state.piano.as_ref() else {
                    return Task::none();
                };
                let mut selected: Vec<usize> = state.piano_selected_notes.iter().copied().collect();
                selected.sort_unstable();
                if selected.is_empty() {
                    return Task::none();
                }
                let mut next_start_by_idx = vec![None; piano.notes.len()];
                for (idx, note) in piano.notes.iter().enumerate() {
                    let next_start = piano
                        .notes
                        .iter()
                        .enumerate()
                        .filter(|(i, n)| {
                            *i != idx
                                && n.channel == note.channel
                                && n.pitch == note.pitch
                                && n.start_sample > note.start_sample
                        })
                        .map(|(_, n)| n.start_sample)
                        .min();
                    next_start_by_idx[idx] = next_start;
                }
                drop(state);
                return self.selected_piano_notes_edit(move |idx, note| {
                    let mut out = note.clone();
                    let next_start = next_start_by_idx.get(idx).and_then(|next| *next);
                    if let Some(next) = next_start {
                        out.length_samples = next.saturating_sub(note.start_sample).max(1);
                    }
                    out
                });
            }
            Message::PianoVelocityShapeSelectedNotes => {
                let amount = self
                    .state
                    .blocking_read()
                    .piano_velocity_shape_amount
                    .clamp(0.0, 1.0);
                let state = self.state.blocking_read();
                let Some(piano) = state.piano.as_ref() else {
                    return Task::none();
                };
                let mut selected: Vec<(usize, usize)> = state
                    .piano_selected_notes
                    .iter()
                    .copied()
                    .filter_map(|idx| piano.notes.get(idx).map(|n| (idx, n.start_sample)))
                    .collect();
                selected.sort_unstable_by_key(|(_, start)| *start);
                let rank: std::collections::HashMap<usize, usize> = selected
                    .iter()
                    .enumerate()
                    .map(|(i, (idx, _))| (*idx, i))
                    .collect();
                let total = selected.len().max(1);
                drop(state);
                return self.selected_piano_notes_edit(move |idx, note| {
                    let mut out = note.clone();
                    let pos = *rank.get(&idx).unwrap_or(&0);
                    let t = if total <= 1 {
                        0.5
                    } else {
                        pos as f32 / (total.saturating_sub(1)) as f32
                    };
                    let shaped = (35.0 + t * (120.0 - 35.0)).round().clamp(1.0, 127.0) as u8;
                    let blended = (note.velocity as f32
                        + (shaped as f32 - note.velocity as f32) * amount)
                        .round()
                        .clamp(1.0, 127.0) as u8;
                    out.velocity = blended;
                    out
                });
            }
            Message::PianoHumanizeSelectedNotes => {
                let interval = self.snap_interval_samples().max(1) as i64;
                let (time_amount, vel_amount) = {
                    let state = self.state.blocking_read();
                    (
                        state.piano_humanize_time_amount.clamp(0.0, 1.0),
                        state.piano_humanize_velocity_amount.clamp(0.0, 1.0),
                    )
                };
                let max_time_jitter = (((interval / 8).max(1)) as f32 * time_amount).round() as i64;
                let max_vel_jitter = (6.0_f32 * vel_amount).round() as i64;
                return self.selected_piano_notes_edit(move |idx, note| {
                    let mut out = note.clone();
                    let dt =
                        Self::deterministic_note_jitter(idx, note.start_sample, max_time_jitter);
                    let new_start = (note.start_sample as i64 + dt).max(0) as usize;
                    let dv = Self::deterministic_note_jitter(
                        idx ^ 0xA5A5,
                        note.length_samples,
                        max_vel_jitter,
                    ) as i16;
                    let new_vel = (i16::from(note.velocity) + dv).clamp(1, 127) as u8;
                    out.start_sample = new_start;
                    out.velocity = new_vel;
                    out
                });
            }
            Message::PianoGrooveSelectedNotes => {
                let interval = self.snap_interval_samples().max(1);
                let amount = self
                    .state
                    .blocking_read()
                    .piano_groove_amount
                    .clamp(0.0, 1.0);
                let swing = (((interval as f32) * 0.22) * amount).round().max(0.0) as usize;
                return self.selected_piano_notes_edit(move |_idx, note| {
                    let straight =
                        ((note.start_sample.saturating_add(interval / 2)) / interval) * interval;
                    let grid = straight / interval;
                    let mut out = note.clone();
                    out.start_sample = if grid % 2 == 1 {
                        straight.saturating_add(swing)
                    } else {
                        straight
                    };
                    out
                });
            }
            Message::PianoQuantizeStrengthChanged(value) => {
                self.state.blocking_write().piano_quantize_strength = value.clamp(0.0, 1.0);
            }
            Message::PianoHumanizeTimeAmountChanged(value) => {
                self.state.blocking_write().piano_humanize_time_amount = value.clamp(0.0, 1.0);
            }
            Message::PianoHumanizeVelocityAmountChanged(value) => {
                self.state.blocking_write().piano_humanize_velocity_amount = value.clamp(0.0, 1.0);
            }
            Message::PianoGrooveAmountChanged(value) => {
                self.state.blocking_write().piano_groove_amount = value.clamp(0.0, 1.0);
            }
            Message::PianoScaleRootSelected(root) => {
                self.state.blocking_write().piano_scale_root = root;
            }
            Message::PianoScaleMinorToggled(minor) => {
                self.state.blocking_write().piano_scale_minor = minor;
            }
            Message::PianoChordKindSelected(kind) => {
                self.state.blocking_write().piano_chord_kind = kind;
            }
            Message::PianoVelocityShapeAmountChanged(value) => {
                self.state.blocking_write().piano_velocity_shape_amount = value.clamp(0.0, 1.0);
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
            Message::Response(Ok(ref a)) => {
                if !self.session_restore_in_progress && history::should_record(a) {
                    self.has_unsaved_changes = true;
                }
                let mut refresh_midi_clip_previews = false;
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
                            let min_h = track.min_height_for_layout().max(60.0);
                            track.height = height.max(min_h);
                        }
                        if let Some((audio_backup, midi_backup, render_clip)) =
                            self.pending_track_freeze_restore.remove(name)
                            && let Some(track) = state.tracks.iter_mut().find(|t| &t.name == name)
                        {
                            track.frozen_audio_backup = audio_backup;
                            track.frozen_midi_backup = midi_backup;
                            track.frozen_render_clip = render_clip;
                        }

                        // Check if we need to load a template for this track
                        let pending_template = state.pending_track_template_load.clone();
                        drop(state);

                        if let Some((template_track_name, template_name)) = pending_template
                            && template_track_name == *name
                        {
                            self.state.blocking_write().pending_track_template_load = None;
                            return self.load_track_template(name.clone(), template_name);
                        }

                        if !matches!(self.modal, Some(Show::AutosaveRecovery)) {
                            self.modal = None;
                        }
                    }
                    Action::RemoveTrack(name) => {
                        let mut state = self.state.blocking_write();

                        if let Some(removed_idx) = state.tracks.iter().position(|t| t.name == *name)
                        {
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
                            for track in &mut state.tracks {
                                if track.vca_master.as_deref() == Some(name.as_str()) {
                                    track.vca_master = None;
                                }
                                track.aux_sends.retain(|send| send.aux_track != *name);
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
                                            midi_clip_to_move =
                                                Some(from_track.midi.clips.remove(from.clip_index));
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
                        name,
                        track_name,
                        start,
                        length,
                        offset,
                        input_channel,
                        muted,
                        kind,
                        fade_enabled,
                        fade_in_samples,
                        fade_out_samples,
                        warp_markers,
                    } => {
                        let mut audio_peaks = crate::state::ClipPeaks::default();
                        let mut max_length_samples = offset.saturating_add(*length);
                        let mut wav_path_for_rebuild: Option<std::path::PathBuf> = None;
                        let mut peaks_path_for_load: Option<std::path::PathBuf> = None;
                        let mut loaded_bins = 0usize;
                        if *kind == Kind::Audio {
                            let key =
                                Self::audio_clip_key(track_name, name, *start, *length, *offset);
                            audio_peaks = self.pending_audio_peaks.remove(&key).unwrap_or_default();
                            peaks_path_for_load = self.pending_peak_file_loads.remove(&key);
                            loaded_bins = audio_peaks.iter().map(Vec::len).max().unwrap_or(0);
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
                                    }
                                    wav_path_for_rebuild = Some(wav_path);
                                }
                            }
                        }
                        let mut state = self.state.blocking_write();
                        if let Some(track) = state.tracks.iter_mut().find(|t| &t.name == track_name)
                        {
                            match kind {
                                Kind::Audio => {
                                    track.audio.clips.push(crate::state::AudioClip {
                                        name: name.clone(),
                                        start: *start,
                                        length: *length,
                                        offset: *offset,
                                        input_channel: *input_channel,
                                        muted: *muted,
                                        max_length_samples,
                                        peaks_file: None,
                                        peaks: audio_peaks,
                                        fade_enabled: *fade_enabled,
                                        fade_in_samples: *fade_in_samples,
                                        fade_out_samples: *fade_out_samples,
                                        warp_markers: warp_markers.clone(),
                                        take_lane_override: None,
                                        take_lane_pinned: false,
                                        take_lane_locked: false,
                                    });
                                }
                                Kind::MIDI => {
                                    track.midi.clips.push(crate::state::MIDIClip {
                                        name: name.clone(),
                                        start: *start,
                                        length: *length,
                                        offset: *offset,
                                        input_channel: *input_channel,
                                        muted: *muted,
                                        max_length_samples,
                                        fade_enabled: *fade_enabled,
                                        fade_in_samples: *fade_in_samples,
                                        fade_out_samples: *fade_out_samples,
                                        take_lane_override: None,
                                        take_lane_pinned: false,
                                        take_lane_locked: false,
                                    });
                                }
                            }
                        }
                        drop(state);
                        if *kind == Kind::Audio && loaded_bins < 32_768 {
                            if let Some(peaks_path) = peaks_path_for_load
                                && let Some(task) = self.schedule_audio_peak_file_load(
                                    track_name, name, *start, *length, *offset, peaks_path,
                                )
                            {
                                self.update_children(&message);
                                return task;
                            }
                            if let Some(wav_path) = wav_path_for_rebuild
                                && let Some(task) = self.schedule_audio_peak_rebuild(
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
                    Action::SetClipMuted {
                        track_name,
                        clip_index,
                        kind,
                        muted,
                    } => {
                        let mut state = self.state.blocking_write();
                        if let Some(track) = state.tracks.iter_mut().find(|t| &t.name == track_name)
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
                    Action::SetAudioClipWarpMarkers {
                        track_name,
                        clip_index,
                        warp_markers,
                    } => {
                        let mut state = self.state.blocking_write();
                        if let Some(track) = state.tracks.iter_mut().find(|t| &t.name == track_name)
                            && let Some(clip) = track.audio.clips.get_mut(*clip_index)
                        {
                            clip.warp_markers = warp_markers.clone();
                        }
                    }
                    Action::RemoveClip {
                        track_name,
                        kind,
                        clip_indices,
                    } => {
                        let mut state = self.state.blocking_write();
                        if let Some(track) = state.tracks.iter_mut().find(|t| &t.name == track_name)
                        {
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
                        if *kind == Kind::MIDI {
                            refresh_midi_clip_previews = true;
                        }
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
                            let mut sorted_indices: Vec<usize> = (0..controllers.len()).collect();
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
                            let mut sorted_indices: Vec<usize> = (0..notes.len()).collect();
                            sorted_indices.sort_unstable_by_key(|&i| notes[i].0);
                            for i in sorted_indices {
                                let (idx, note) = &notes[i];
                                let insert_at = (*idx).min(piano.notes.len());
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
                            state.message =
                                format!("Disconnected {} from {}", from_track, to_track);
                        }
                    }

                    Action::OpenAudioDevice {
                        device,
                        #[cfg(any(
                            target_os = "windows",
                            target_os = "freebsd",
                            target_os = "linux"
                        ))]
                            input_device: _,
                        sample_rate_hz: _,
                        bits,
                        exclusive,
                        period_frames,
                        nperiods,
                        sync_mode,
                    } => {
                        let mut state = self.state.blocking_write();
                        state.message = format!(
                            "Opened device {} (rate={} Hz, bits={}, exclusive={}, period={}, nperiods={}, sync_mode={})",
                            device,
                            state.hw_sample_rate_hz.max(1),
                            bits,
                            exclusive,
                            period_frames,
                            nperiods,
                            sync_mode
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
                        if *rate > 0 {
                            state.hw_sample_rate_hz = *rate as i32;
                        }
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
                    Action::SessionDiagnosticsReport {
                        track_count,
                        frozen_track_count,
                        audio_clip_count,
                        midi_clip_count,
                        #[cfg(all(unix, not(target_os = "macos")))]
                        lv2_instance_count,
                        vst3_instance_count,
                        clap_instance_count,
                        pending_requests,
                        workers_total,
                        workers_ready,
                        pending_hw_midi_events,
                        playing,
                        transport_sample,
                        tempo_bpm,
                        sample_rate_hz,
                        cycle_samples,
                    } => {
                        let plugin_summary = format!(
                            "VST3={} CLAP={}{}",
                            vst3_instance_count,
                            clap_instance_count,
                            {
                                #[cfg(all(unix, not(target_os = "macos")))]
                                {
                                    format!(" LV2={}", lv2_instance_count)
                                }
                                #[cfg(not(all(unix, not(target_os = "macos"))))]
                                {
                                    String::new()
                                }
                            }
                        );
                        let report = format!(
                            "Session Diagnostics: tracks={} frozen={} audio_clips={} midi_clips={} | plugins: {} | engine: playing={} transport={} tempo={:.2} BPM | audio: rate={}Hz cycle={} | workers: ready={}/{} pending_req={} pending_midi_ev={}",
                            track_count,
                            frozen_track_count,
                            audio_clip_count,
                            midi_clip_count,
                            plugin_summary,
                            playing,
                            transport_sample,
                            tempo_bpm,
                            sample_rate_hz,
                            cycle_samples,
                            workers_ready,
                            workers_total,
                            pending_requests,
                            pending_hw_midi_events
                        );
                        let mut state = self.state.blocking_write();
                        state.message = report.clone();
                        state.diagnostics_report = Some(report);
                        if self.pending_diagnostics_bundle_export {
                            self.diagnostics_bundle_wait_session_report = false;
                            if !self.diagnostics_bundle_wait_session_report
                                && !self.diagnostics_bundle_wait_midi_report
                            {
                                self.pending_diagnostics_bundle_export = false;
                                match self.export_diagnostics_bundle() {
                                    Ok(path) => {
                                        state.message = format!(
                                            "Diagnostics bundle exported: {}",
                                            path.display()
                                        );
                                    }
                                    Err(e) => {
                                        state.message =
                                            format!("Diagnostics bundle export failed: {e}");
                                    }
                                }
                            }
                        }
                    }
                    Action::MidiLearnMappingsReport { lines } => {
                        let report = lines.join(" | ");
                        self.midi_mappings_report_lines = lines.clone();
                        let mut state = self.state.blocking_write();
                        state.message = format!("MIDI mappings: {}", report);
                        state.diagnostics_report = Some(format!("MIDI mappings: {}", report));
                        if self.pending_diagnostics_bundle_export {
                            self.diagnostics_bundle_wait_midi_report = false;
                            if !self.diagnostics_bundle_wait_session_report
                                && !self.diagnostics_bundle_wait_midi_report
                            {
                                self.pending_diagnostics_bundle_export = false;
                                match self.export_diagnostics_bundle() {
                                    Ok(path) => {
                                        state.message = format!(
                                            "Diagnostics bundle exported: {}",
                                            path.display()
                                        );
                                    }
                                    Err(e) => {
                                        state.message =
                                            format!("Diagnostics bundle export failed: {e}");
                                    }
                                }
                            }
                        }
                    }
                    Action::ClearAllMidiLearnBindings => {
                        self.midi_mappings_report_lines.clear();
                        let mut state = self.state.blocking_write();
                        state.global_midi_learn_play_pause = None;
                        state.global_midi_learn_stop = None;
                        state.global_midi_learn_record_toggle = None;
                        for track in &mut state.tracks {
                            track.midi_learn_volume = None;
                            track.midi_learn_balance = None;
                            track.midi_learn_mute = None;
                            track.midi_learn_solo = None;
                            track.midi_learn_arm = None;
                            track.midi_learn_input_monitor = None;
                            track.midi_learn_disk_monitor = None;
                        }
                        state.message = "Cleared all MIDI mappings".to_string();
                    }
                    Action::TrackLevel(name, level) => {
                        let mut state = self.state.blocking_write();
                        if name == "hw:out" {
                            state.hw_out_level = *level;
                        } else if let Some(track) =
                            state.tracks.iter_mut().find(|t| t.name == *name)
                        {
                            track.level = *level;
                        }
                    }
                    Action::TrackBalance(name, balance) => {
                        let mut state = self.state.blocking_write();
                        if name == "hw:out" {
                            state.hw_out_balance = *balance;
                        } else if let Some(track) =
                            state.tracks.iter_mut().find(|t| t.name == *name)
                        {
                            track.balance = *balance;
                        }
                    }
                    Action::TrackAutomationLevel(name, level) => {
                        if let Some(track) = self
                            .state
                            .blocking_write()
                            .tracks
                            .iter_mut()
                            .find(|t| t.name == *name)
                        {
                            track.level = *level;
                        }
                    }
                    Action::TrackAutomationBalance(name, balance) => {
                        if let Some(track) = self
                            .state
                            .blocking_write()
                            .tracks
                            .iter_mut()
                            .find(|t| t.name == *name)
                        {
                            track.balance = *balance;
                        }
                    }
                    Action::TrackAutomationMute(name, muted) => {
                        if let Some(track) = self
                            .state
                            .blocking_write()
                            .tracks
                            .iter_mut()
                            .find(|t| t.name == *name)
                        {
                            track.muted = *muted;
                        }
                    }
                    Action::TrackToggleMute(name) => {
                        let mut state = self.state.blocking_write();
                        if name == "hw:out" {
                            state.hw_out_muted = !state.hw_out_muted;
                        } else if let Some(track) =
                            state.tracks.iter_mut().find(|t| t.name == *name)
                        {
                            track.muted = !track.muted;
                        }
                    }
                    Action::TrackToggleSolo(name) => {
                        if let Some(track) = self
                            .state
                            .blocking_write()
                            .tracks
                            .iter_mut()
                            .find(|t| t.name == *name)
                        {
                            track.soloed = !track.soloed;
                        }
                    }
                    Action::TrackToggleArm(name) => {
                        if let Some(track) = self
                            .state
                            .blocking_write()
                            .tracks
                            .iter_mut()
                            .find(|t| t.name == *name)
                        {
                            track.armed = !track.armed;
                        }
                    }
                    Action::TrackToggleInputMonitor(name) => {
                        if let Some(track) = self
                            .state
                            .blocking_write()
                            .tracks
                            .iter_mut()
                            .find(|t| t.name == *name)
                        {
                            track.input_monitor = !track.input_monitor;
                        }
                    }
                    Action::TrackToggleDiskMonitor(name) => {
                        if let Some(track) = self
                            .state
                            .blocking_write()
                            .tracks
                            .iter_mut()
                            .find(|t| t.name == *name)
                        {
                            track.disk_monitor = !track.disk_monitor;
                        }
                    }
                    Action::TrackSetVcaMaster {
                        track_name,
                        master_track,
                    } => {
                        if let Some(track) = self
                            .state
                            .blocking_write()
                            .tracks
                            .iter_mut()
                            .find(|t| t.name == *track_name)
                        {
                            track.vca_master = master_track.clone();
                        }
                    }
                    Action::TrackArmMidiLearn { track_name, target } => {
                        self.state.blocking_write().message = format!(
                            "MIDI learn armed for '{}' ({:?}). Move a hardware MIDI CC control.",
                            track_name, target
                        );
                    }
                    Action::TrackSetMidiLearnBinding {
                        track_name,
                        target,
                        binding,
                    } => {
                        if let Some(track) = self
                            .state
                            .blocking_write()
                            .tracks
                            .iter_mut()
                            .find(|t| t.name == *track_name)
                        {
                            match target {
                                maolan_engine::message::TrackMidiLearnTarget::Volume => {
                                    track.midi_learn_volume = binding.clone();
                                }
                                maolan_engine::message::TrackMidiLearnTarget::Balance => {
                                    track.midi_learn_balance = binding.clone();
                                }
                                maolan_engine::message::TrackMidiLearnTarget::Mute => {
                                    track.midi_learn_mute = binding.clone();
                                }
                                maolan_engine::message::TrackMidiLearnTarget::Solo => {
                                    track.midi_learn_solo = binding.clone();
                                }
                                maolan_engine::message::TrackMidiLearnTarget::Arm => {
                                    track.midi_learn_arm = binding.clone();
                                }
                                maolan_engine::message::TrackMidiLearnTarget::InputMonitor => {
                                    track.midi_learn_input_monitor = binding.clone();
                                }
                                maolan_engine::message::TrackMidiLearnTarget::DiskMonitor => {
                                    track.midi_learn_disk_monitor = binding.clone();
                                }
                            }
                        }
                        let message = if let Some(binding) = binding {
                            format!(
                                "MIDI learn mapped '{}' {:?} to CH{} CC{}",
                                track_name,
                                target,
                                binding.channel + 1,
                                binding.cc
                            )
                        } else {
                            format!("MIDI learn cleared for '{}' {:?}", track_name, target)
                        };
                        self.state.blocking_write().message = message;
                        if self.midi_mappings_panel_open {
                            self.rebuild_midi_mappings_report_lines_from_state();
                        }
                    }
                    Action::SetGlobalMidiLearnBinding { target, binding } => {
                        {
                            let mut state = self.state.blocking_write();
                            match target {
                                maolan_engine::message::GlobalMidiLearnTarget::PlayPause => {
                                    state.global_midi_learn_play_pause = binding.clone();
                                }
                                maolan_engine::message::GlobalMidiLearnTarget::Stop => {
                                    state.global_midi_learn_stop = binding.clone();
                                }
                                maolan_engine::message::GlobalMidiLearnTarget::RecordToggle => {
                                    state.global_midi_learn_record_toggle = binding.clone();
                                }
                            }
                        }
                        self.state.blocking_write().message = if let Some(binding) = binding {
                            format!(
                                "Global MIDI learn mapped {:?} to CH{} CC{}",
                                target,
                                binding.channel + 1,
                                binding.cc
                            )
                        } else {
                            format!("Global MIDI learn cleared for {:?}", target)
                        };
                        if self.midi_mappings_panel_open {
                            self.rebuild_midi_mappings_report_lines_from_state();
                        }
                    }
                    Action::TrackSetFrozen { track_name, frozen } => {
                        self.state.blocking_write().message = if *frozen {
                            format!("Track '{track_name}' frozen")
                        } else {
                            format!("Track '{track_name}' unfrozen")
                        };
                    }
                    Action::TrackOfflineBounce {
                        track_name,
                        output_path,
                        ..
                    } => {
                        self.freeze_in_progress = false;
                        self.freeze_track_name = None;
                        if let Some(pending) = self.pending_track_freeze_bounce.remove(track_name) {
                            if self.freeze_cancel_requested {
                                self.freeze_cancel_requested = false;
                                let _ = std::fs::remove_file(output_path);
                                self.state.blocking_write().message =
                                    format!("Freeze canceled for '{}'", track_name);
                                return Task::none();
                            }
                            let render_path = std::path::PathBuf::from(output_path);
                            let render_peaks =
                                Self::compute_audio_clip_peaks(&render_path).unwrap_or_default();
                            {
                                let mut state = self.state.blocking_write();
                                if let Some(track_mut) =
                                    state.tracks.iter_mut().find(|t| t.name == *track_name)
                                {
                                    track_mut.frozen_audio_backup = pending.backup_audio.clone();
                                    track_mut.frozen_midi_backup = pending.backup_midi.clone();
                                    track_mut.frozen_render_clip =
                                        Some(pending.rendered_clip_rel.clone());
                                    state.message = format!("Frozen track '{}'", track_name);
                                }
                            }
                            let key = Self::audio_clip_key(
                                track_name,
                                &pending.rendered_clip_rel,
                                0,
                                pending.rendered_length,
                                0,
                            );
                            self.pending_audio_peaks.insert(key, render_peaks);
                            let mut tasks = vec![self.send(Action::BeginHistoryGroup)];
                            if !pending.backup_audio.is_empty() {
                                tasks.push(self.send(Action::RemoveClip {
                                    track_name: track_name.clone(),
                                    kind: Kind::Audio,
                                    clip_indices: (0..pending.backup_audio.len()).collect(),
                                }));
                            }
                            if !pending.backup_midi.is_empty() {
                                tasks.push(self.send(Action::RemoveClip {
                                    track_name: track_name.clone(),
                                    kind: Kind::MIDI,
                                    clip_indices: (0..pending.backup_midi.len()).collect(),
                                }));
                            }
                            tasks.push(self.send(Action::AddClip {
                                name: pending.rendered_clip_rel,
                                track_name: track_name.clone(),
                                start: 0,
                                length: pending.rendered_length.max(1),
                                offset: 0,
                                input_channel: 0,
                                muted: false,
                                kind: Kind::Audio,
                                fade_enabled: true,
                                fade_in_samples: 240,
                                fade_out_samples: 240,
                                warp_markers: vec![],
                            }));
                            tasks.push(self.send(Action::TrackSetFrozen {
                                track_name: track_name.clone(),
                                frozen: true,
                            }));
                            tasks.push(self.send(Action::EndHistoryGroup));
                            return Task::batch(tasks);
                        }
                    }
                    Action::TrackOfflineBounceProgress {
                        track_name,
                        progress,
                        operation,
                    } => {
                        self.freeze_in_progress = true;
                        self.freeze_track_name = Some(track_name.clone());
                        self.freeze_progress = *progress;
                        let percent = (progress * 100.0).round().clamp(0.0, 100.0) as u32;
                        self.state.blocking_write().message = if self.freeze_cancel_requested {
                            format!("Canceling freeze ({percent}%)...")
                        } else if let Some(op) = operation {
                            format!("{} ({percent}%)", op)
                        } else {
                            format!("Rendering freeze ({percent}%)")
                        };
                        return Task::none();
                    }
                    Action::TrackOfflineBounceCanceled { track_name } => {
                        self.freeze_in_progress = false;
                        self.freeze_track_name = None;
                        self.freeze_progress = 0.0;
                        self.freeze_cancel_requested = false;
                        self.pending_track_freeze_bounce.remove(track_name);
                        self.state.blocking_write().message =
                            format!("Freeze canceled for '{}'", track_name);
                        return Task::none();
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
                        self.has_unsaved_changes = false;
                        self.last_autosave_snapshot = None;
                        self.pending_autosave_recovery = None;
                        self.pending_open_session_dir = None;
                        if let Some(path) = &self.session_dir {
                            Self::write_last_session_hint(&path.to_string_lossy());
                        }
                        if let Some(autosave_root) = self.autosave_snapshot_root()
                            && autosave_root.exists()
                        {
                            let _ = fs::remove_dir_all(&autosave_root);
                        }
                        if self.pending_record_after_save {
                            self.pending_record_after_save = false;
                            return self.send(Action::SetRecordEnabled(true));
                        }
                    }
                    Action::BeginSessionRestore => {
                        self.session_restore_in_progress = true;
                        self.has_unsaved_changes = false;
                        self.last_autosave_snapshot = None;
                        self.pending_autosave_recovery = None;
                        self.pending_open_session_dir = None;
                    }
                    Action::EndSessionRestore => {
                        self.session_restore_in_progress = false;
                        self.has_unsaved_changes = false;
                        self.last_autosave_snapshot = None;
                        self.pending_autosave_recovery = None;
                        self.pending_open_session_dir = None;
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
                    Action::SetTempo(bpm) => {
                        let bpm = (*bpm as f32).clamp(20.0, 300.0);
                        self.state.blocking_write().tempo = bpm;
                        self.tempo_input = format!("{:.2}", bpm);
                        self.last_sent_tempo_bpm = Some(bpm as f64);
                    }
                    Action::SetTimeSignature {
                        numerator,
                        denominator,
                    } => {
                        let mut state = self.state.blocking_write();
                        state.time_signature_num = (*numerator).clamp(1, 16) as u8;
                        state.time_signature_denom = match *denominator {
                            2 => 2,
                            4 => 4,
                            8 => 8,
                            16 => 16,
                            _ => 4,
                        };
                        self.time_signature_num_input = state.time_signature_num.to_string();
                        self.time_signature_denom_input = state.time_signature_denom.to_string();
                        self.last_sent_time_signature = Some((
                            state.time_signature_num as u16,
                            state.time_signature_denom as u16,
                        ));
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
                            let plugin_track =
                                self.state.blocking_read().plugin_graph_track.clone();
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
                        self.state.blocking_write().message = format!(
                            "Unloaded CLAP plugin '{plugin_name}' from track '{track_name}'"
                        );
                        #[cfg(any(target_os = "windows", all(unix, not(target_os = "macos"))))]
                        {
                            let plugin_track =
                                self.state.blocking_read().plugin_graph_track.clone();
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
                    Action::TrackClapParameters {
                        track_name,
                        instance_id,
                        parameters,
                    } => {
                        if self
                            .pending_add_clap_automation_instances
                            .remove(&(track_name.clone(), *instance_id))
                        {
                            let mut state = self.state.blocking_write();
                            if let Some(track) =
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
                                track.height = track.min_height_for_layout().max(60.0);
                                state.message = format!(
                                    "Added {} CLAP automation lanes on '{}'",
                                    parameters.len(),
                                    track_name
                                );
                            }
                        }
                    }
                    Action::TrackVst3Parameters {
                        track_name,
                        instance_id,
                        parameters,
                    } => {
                        if self
                            .pending_add_vst3_automation_instances
                            .remove(&(track_name.clone(), *instance_id))
                        {
                            let mut state = self.state.blocking_write();
                            if let Some(track) =
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
                                track.height = track.min_height_for_layout().max(60.0);
                                state.message = format!(
                                    "Added {} VST3 automation lanes on '{}'",
                                    parameters.len(),
                                    track_name
                                );
                            }
                        }
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
                        if let Some(piano) = &mut state.piano
                            && piano.track_idx == *track_name
                        {
                            piano.midnam_note_names = note_names.clone();
                        }
                    }
                    #[cfg(all(unix, not(target_os = "macos")))]
                    Action::TrackLv2PluginControls {
                        track_name,
                        instance_id,
                        controls,
                        instance_access_handle,
                    } => {
                        if self
                            .pending_add_lv2_automation_instances
                            .remove(&(track_name.clone(), *instance_id))
                        {
                            let mut state = self.state.blocking_write();
                            if let Some(track) =
                                state.tracks.iter_mut().find(|t| t.name == *track_name)
                            {
                                for control in controls {
                                    let target = TrackAutomationTarget::Lv2Parameter {
                                        instance_id: *instance_id,
                                        index: control.index,
                                        min: control.min,
                                        max: control.max,
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
                                track.height = track.min_height_for_layout().max(60.0);
                                state.message = format!(
                                    "Added {} LV2 automation lanes on '{}'",
                                    controls.len(),
                                    track_name
                                );
                            }
                            return Task::none();
                        }
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

                        let mut pending_queries: Vec<Task<Message>> = vec![];
                        #[cfg(all(unix, not(target_os = "macos")))]
                        let pending_lv2_uris: Vec<(String, String)> = self
                            .pending_add_lv2_automation_uris
                            .iter()
                            .filter(|(name, _)| name == track_name)
                            .cloned()
                            .collect();
                        #[cfg(all(unix, not(target_os = "macos")))]
                        for (pending_track, pending_uri) in pending_lv2_uris {
                            if let Some(instance_id) = plugins
                                .iter()
                                .find(|plugin| {
                                    plugin.format.eq_ignore_ascii_case("LV2")
                                        && (plugin.uri == pending_uri
                                            || plugin.plugin_id == pending_uri)
                                })
                                .map(|plugin| plugin.instance_id)
                            {
                                self.pending_add_lv2_automation_uris
                                    .remove(&(pending_track.clone(), pending_uri));
                                self.pending_add_lv2_automation_instances
                                    .insert((pending_track.clone(), instance_id));
                                pending_queries.push(self.send(
                                    Action::TrackGetLv2PluginControls {
                                        track_name: pending_track,
                                        instance_id,
                                    },
                                ));
                            }
                        }
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
                                        && (plugin.uri == pending_path
                                            || plugin.plugin_id == pending_path)
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
                                        && (plugin.uri == pending_path
                                            || plugin.plugin_id == pending_path)
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
                        if !pending_queries.is_empty() {
                            return Task::batch(pending_queries);
                        }

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
                        for track in &mut state.tracks {
                            if track.vca_master.as_deref() == Some(old_name.as_str()) {
                                track.vca_master = Some(new_name.clone());
                            }
                        }
                        state.message = format!("Renamed track to '{}'", new_name);
                        refresh_midi_clip_previews = true;
                    }
                    _ => {}
                }
                if refresh_midi_clip_previews {
                    self.update_children(&message);
                    return self.queue_midi_clip_preview_loads();
                }
            }
            Message::Response(Err(ref e)) => {
                if !self.pending_track_freeze_bounce.is_empty() {
                    self.pending_track_freeze_bounce.clear();
                }
                self.freeze_in_progress = false;
                self.freeze_track_name = None;
                self.freeze_cancel_requested = false;
                self.pending_diagnostics_bundle_export = false;
                self.diagnostics_bundle_wait_session_report = false;
                self.diagnostics_bundle_wait_midi_report = false;
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
                if Self::has_newer_autosave_snapshot(&path) {
                    self.pending_recovery_session_dir = Some(path.clone());
                    self.pending_autosave_recovery = None;
                    self.pending_open_session_dir = Some(path.clone());
                    self.modal = Some(Show::AutosaveRecovery);
                    self.state.blocking_write().message =
                        "Found newer autosave snapshot for opened session.".to_string();
                    return Task::none();
                } else if self
                    .pending_recovery_session_dir
                    .as_ref()
                    .is_some_and(|pending| pending == &path)
                {
                    self.pending_recovery_session_dir = None;
                }
                self.pending_open_session_dir = None;
                self.session_dir = Some(path.clone());
                self.pending_autosave_recovery = None;
                self.stop_recording_preview();
                self.state.blocking_write().message = "Loading session...".to_string();
                return Task::perform(async move { path }, Message::LoadSessionPath);
            }
            Message::LoadSessionPath(path) => {
                self.session_dir = Some(path.clone());
                self.stop_recording_preview();
                match self.load(path.to_string_lossy().to_string()) {
                    Ok(task) => {
                        return Task::batch(vec![task, self.queue_midi_clip_preview_loads()]);
                    }
                    Err(e) => {
                        error!("{}", e);
                        self.state.blocking_write().message =
                            format!("Failed to load session: {}", e);
                        return Task::none();
                    }
                }
            }
            Message::RecoverAutosaveSnapshot => {
                let startup_modal_flow = matches!(self.modal, Some(Show::AutosaveRecovery));
                if let Err(e) = self.prepare_pending_autosave_recovery(false) {
                    self.state.blocking_write().message = e;
                    return Task::none();
                }
                if startup_modal_flow {
                    self.modal = None;
                    return self.apply_pending_autosave_recovery();
                }
                if let Some(pending) = self.pending_autosave_recovery.as_mut() {
                    let selected_snapshot = pending
                        .snapshots
                        .get(pending.selected_index)
                        .cloned()
                        .unwrap_or_else(|| pending.snapshots[0].clone());
                    let preview = Self::autosave_recovery_preview_summary(
                        &pending.session_dir,
                        &selected_snapshot,
                    );
                    if !pending.confirm_armed {
                        pending.confirm_armed = true;
                        self.state.blocking_write().message = format!(
                            "{preview}. Run Recover Autosave Snapshot again to apply. Use Recover Older Autosave Snapshot to pick an older one."
                        );
                        return Task::none();
                    }
                }
                return self.apply_pending_autosave_recovery();
            }
            Message::RecoverOlderAutosaveSnapshot => {
                let startup_modal_flow = matches!(self.modal, Some(Show::AutosaveRecovery));
                if let Err(e) = self.prepare_pending_autosave_recovery(true) {
                    self.state.blocking_write().message = e;
                    return Task::none();
                }
                if startup_modal_flow {
                    self.modal = None;
                    return self.apply_pending_autosave_recovery();
                }
                if let Some(pending) = self.pending_autosave_recovery.as_mut() {
                    let selected_snapshot = pending
                        .snapshots
                        .get(pending.selected_index)
                        .cloned()
                        .unwrap_or_else(|| pending.snapshots[0].clone());
                    let preview = Self::autosave_recovery_preview_summary(
                        &pending.session_dir,
                        &selected_snapshot,
                    );
                    self.state.blocking_write().message = format!(
                        "{preview}. Run Recover Autosave Snapshot to apply this selection."
                    );
                }
                return Task::none();
            }
            Message::RecoverAutosaveIgnore => {
                let deferred_open = self.pending_open_session_dir.clone();
                self.pending_recovery_session_dir = None;
                self.pending_autosave_recovery = None;
                self.pending_open_session_dir = None;
                self.modal = None;
                if let Some(path) = deferred_open {
                    self.state.blocking_write().message = "Loading session...".to_string();
                    return Task::perform(async move { path }, Message::LoadSessionPath);
                } else {
                    self.state.blocking_write().message = "Autosave recovery ignored".to_string();
                    return Task::none();
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
                    state.selected.insert(name.clone());
                    if let ConnectionViewSelection::Tracks(set) =
                        &mut state.connection_view_selection
                    {
                        set.insert(name.clone());
                    } else {
                        let mut set = std::collections::HashSet::new();
                        set.insert(name.clone());
                        state.connection_view_selection = ConnectionViewSelection::Tracks(set);
                    }
                } else {
                    state.selected.clear();
                    state.selected.insert(name.clone());
                    let mut set = std::collections::HashSet::new();
                    set.insert(name.clone());
                    state.connection_view_selection = ConnectionViewSelection::Tracks(set);
                }
            }
            Message::SelectTrackFromMixer(ref name) => {
                let ctrl = self.state.blocking_read().ctrl;
                let mut state = self.state.blocking_write();
                state.connections_last_track_click = None;

                if ctrl {
                    state.selected.insert(name.clone());
                    if let ConnectionViewSelection::Tracks(set) =
                        &mut state.connection_view_selection
                    {
                        set.insert(name.clone());
                    } else {
                        let mut set = std::collections::HashSet::new();
                        set.insert(name.clone());
                        state.connection_view_selection = ConnectionViewSelection::Tracks(set);
                    }
                } else {
                    state.selected.clear();
                    state.selected.insert(name.clone());
                    let mut set = std::collections::HashSet::new();
                    set.insert(name.clone());
                    state.connection_view_selection = ConnectionViewSelection::Tracks(set);
                }
            }
            Message::TrackSetVcaMaster {
                ref track_name,
                ref master_track,
            } => {
                if master_track.as_deref() == Some(track_name.as_str()) {
                    self.state.blocking_write().message =
                        "Track cannot be its own VCA master".to_string();
                    return Task::none();
                }
                return self.send(Action::TrackSetVcaMaster {
                    track_name: track_name.clone(),
                    master_track: master_track.clone(),
                });
            }
            Message::TrackCreateAuxReturnFromSelection => {
                let (selected_tracks, max_outs, max_midi_outs, existing_names) = {
                    let state = self.state.blocking_read();
                    let selected = state.selected.iter().cloned().collect::<Vec<_>>();
                    let mut audio_outs = 2usize;
                    let mut midi_outs = 0usize;
                    for track in &state.tracks {
                        if state.selected.contains(&track.name) {
                            audio_outs = audio_outs.max(track.audio.outs.max(1));
                            midi_outs = midi_outs.max(track.midi.outs);
                        }
                    }
                    (
                        selected,
                        audio_outs.max(1),
                        midi_outs,
                        state
                            .tracks
                            .iter()
                            .map(|t| t.name.clone())
                            .collect::<std::collections::HashSet<_>>(),
                    )
                };
                if selected_tracks.is_empty() {
                    self.state.blocking_write().message =
                        "Select one or more tracks first".to_string();
                    return Task::none();
                }
                let mut idx = 1usize;
                let aux_name = loop {
                    let candidate = format!("Aux Return {idx}");
                    if !existing_names.contains(&candidate) {
                        break candidate;
                    }
                    idx = idx.saturating_add(1);
                };
                let mut tasks = vec![self.send(Action::BeginHistoryGroup)];
                tasks.push(self.send(Action::AddTrack {
                    name: aux_name.clone(),
                    audio_ins: max_outs,
                    midi_ins: 0,
                    audio_outs: max_outs,
                    midi_outs: max_midi_outs,
                }));
                for track_name in &selected_tracks {
                    tasks.push(self.send(Action::Connect {
                        from_track: track_name.clone(),
                        from_port: 0,
                        to_track: aux_name.clone(),
                        to_port: 0,
                        kind: Kind::Audio,
                    }));
                }
                tasks.push(self.send(Action::Connect {
                    from_track: aux_name.clone(),
                    from_port: 0,
                    to_track: "hw:out".to_string(),
                    to_port: 0,
                    kind: Kind::Audio,
                }));
                tasks.push(self.send(Action::EndHistoryGroup));
                {
                    let mut state = self.state.blocking_write();
                    for track in &mut state.tracks {
                        if selected_tracks.iter().any(|name| name == &track.name)
                            && !track.aux_sends.iter().any(|s| s.aux_track == aux_name)
                        {
                            track.aux_sends.push(crate::state::AuxSend {
                                aux_track: aux_name.clone(),
                                level_db: 0.0,
                                pan: 0.0,
                                pre_fader: false,
                            });
                        }
                    }
                }
                self.state.blocking_write().message = format!(
                    "Created '{}' and connected selected tracks as sends",
                    aux_name
                );
                return Task::batch(tasks);
            }
            Message::TrackAuxSendLevelAdjust {
                ref track_name,
                ref aux_track,
                delta_db,
            } => {
                let mut state = self.state.blocking_write();
                if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name)
                    && let Some(send) = track
                        .aux_sends
                        .iter_mut()
                        .find(|s| s.aux_track == *aux_track)
                {
                    send.level_db = (send.level_db + delta_db).clamp(-60.0, 12.0);
                    state.message = format!(
                        "Aux send {} -> {} level {:.1} dB",
                        track_name, aux_track, send.level_db
                    );
                }
                return Task::none();
            }
            Message::TrackAuxSendPanAdjust {
                ref track_name,
                ref aux_track,
                delta,
            } => {
                let mut state = self.state.blocking_write();
                if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name)
                    && let Some(send) = track
                        .aux_sends
                        .iter_mut()
                        .find(|s| s.aux_track == *aux_track)
                {
                    send.pan = (send.pan + delta).clamp(-1.0, 1.0);
                    state.message = format!(
                        "Aux send {} -> {} pan {:.2}",
                        track_name, aux_track, send.pan
                    );
                }
                return Task::none();
            }
            Message::TrackAuxSendTogglePrePost {
                ref track_name,
                ref aux_track,
            } => {
                let mut state = self.state.blocking_write();
                if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name)
                    && let Some(send) = track
                        .aux_sends
                        .iter_mut()
                        .find(|s| s.aux_track == *aux_track)
                {
                    send.pre_fader = !send.pre_fader;
                    state.message = format!(
                        "Aux send {} -> {} mode {}",
                        track_name,
                        aux_track,
                        if send.pre_fader {
                            "Pre-Fader"
                        } else {
                            "Post-Fader"
                        }
                    );
                }
                return Task::none();
            }
            Message::TrackMidiLearnArm {
                ref track_name,
                target,
            } => {
                self.state.blocking_write().message = format!(
                    "MIDI learn armed for '{}' ({:?}). Move a hardware MIDI CC control.",
                    track_name, target
                );
                return self.send(Action::TrackArmMidiLearn {
                    track_name: track_name.clone(),
                    target,
                });
            }
            Message::TrackMidiLearnClear {
                ref track_name,
                target,
            } => {
                return self.send(Action::TrackSetMidiLearnBinding {
                    track_name: track_name.clone(),
                    target,
                    binding: None,
                });
            }
            Message::GlobalMidiLearnArm { target } => {
                self.state.blocking_write().message = format!(
                    "Global MIDI learn armed for {:?}. Move a hardware MIDI CC control.",
                    target
                );
                return self.send(Action::GlobalArmMidiLearn { target });
            }
            Message::GlobalMidiLearnClear { target } => {
                return self.send(Action::SetGlobalMidiLearnBinding {
                    target,
                    binding: None,
                });
            }
            Message::TrackFreezeToggle { ref track_name } => {
                if self.freeze_in_progress {
                    if self.freeze_track_name.as_deref() == Some(track_name.as_str()) {
                        self.freeze_cancel_requested = true;
                        self.state.blocking_write().message =
                            format!("Cancel requested for freezing '{}'", track_name);
                        return self.send(Action::TrackOfflineBounceCancel {
                            track_name: track_name.clone(),
                        });
                    } else {
                        self.state.blocking_write().message = format!(
                            "Freeze in progress for '{}'",
                            self.freeze_track_name.clone().unwrap_or_default()
                        );
                    }
                    return Task::none();
                }
                let Some(session_root) = self.session_dir.clone() else {
                    self.state.blocking_write().message =
                        "Freeze requires an opened/saved session".to_string();
                    return Task::none();
                };
                let track_snapshot = {
                    let state = self.state.blocking_read();
                    state.tracks.iter().find(|t| t.name == *track_name).cloned()
                };
                let Some(track) = track_snapshot else {
                    self.state.blocking_write().message =
                        format!("Track '{}' not found", track_name);
                    return Task::none();
                };

                if track.frozen {
                    let current_audio_len = track.audio.clips.len();
                    let current_midi_len = track.midi.clips.len();
                    let restore_audio = track.frozen_audio_backup.clone();
                    let restore_midi = track.frozen_midi_backup.clone();
                    {
                        let mut state = self.state.blocking_write();
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
                        tasks.push(self.send(Action::AddClip {
                            name: clip.name,
                            track_name: track_name.clone(),
                            start: clip.start,
                            length: clip.length,
                            offset: clip.offset,
                            input_channel: clip.input_channel,
                            muted: clip.muted,
                            kind: Kind::Audio,
                            fade_enabled: clip.fade_enabled,
                            fade_in_samples: clip.fade_in_samples,
                            fade_out_samples: clip.fade_out_samples,
                            warp_markers: clip.warp_markers,
                        }));
                    }
                    for clip in restore_midi {
                        tasks.push(self.send(Action::AddClip {
                            name: clip.name,
                            track_name: track_name.clone(),
                            start: clip.start,
                            length: clip.length,
                            offset: clip.offset,
                            input_channel: clip.input_channel,
                            muted: clip.muted,
                            kind: Kind::MIDI,
                            fade_enabled: clip.fade_enabled,
                            fade_in_samples: clip.fade_in_samples,
                            fade_out_samples: clip.fade_out_samples,
                            warp_markers: vec![],
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
                    self.state.blocking_write().message =
                        format!("Track '{}' has no clips to freeze", track_name);
                    return Task::none();
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
                            self.state.blocking_write().message =
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
                    let target = match lane.target {
                        crate::message::TrackAutomationTarget::Volume => {
                            OfflineAutomationTarget::Volume
                        }
                        crate::message::TrackAutomationTarget::Balance => {
                            OfflineAutomationTarget::Balance
                        }
                        crate::message::TrackAutomationTarget::Mute => {
                            OfflineAutomationTarget::Mute
                        }
                        #[cfg(all(unix, not(target_os = "macos")))]
                        crate::message::TrackAutomationTarget::Lv2Parameter {
                            instance_id,
                            index,
                            min,
                            max,
                        } => OfflineAutomationTarget::Lv2Parameter {
                            instance_id,
                            index,
                            min,
                            max,
                        },
                        #[cfg(not(all(unix, not(target_os = "macos"))))]
                        crate::message::TrackAutomationTarget::Lv2Parameter { .. } => continue,
                        crate::message::TrackAutomationTarget::Vst3Parameter {
                            instance_id,
                            param_id,
                        } => OfflineAutomationTarget::Vst3Parameter {
                            instance_id,
                            param_id,
                        },
                        crate::message::TrackAutomationTarget::ClapParameter {
                            instance_id,
                            param_id,
                            min,
                            max,
                        } => OfflineAutomationTarget::ClapParameter {
                            instance_id,
                            param_id,
                            min,
                            max,
                        },
                    };
                    let points = lane
                        .points
                        .iter()
                        .map(|p| OfflineAutomationPoint {
                            sample: p.sample,
                            value: p.value,
                        })
                        .collect::<Vec<_>>();
                    automation_lanes.push(OfflineAutomationLane { target, points });
                }
                self.pending_track_freeze_bounce.insert(
                    track_name.clone(),
                    super::PendingTrackFreezeBounce {
                        rendered_clip_rel: render_rel,
                        rendered_length: render_length.max(1),
                        backup_audio: track.audio.clips.clone(),
                        backup_midi: track.midi.clips.clone(),
                    },
                );
                self.freeze_in_progress = true;
                self.freeze_progress = 0.0;
                self.freeze_track_name = Some(track_name.clone());
                self.freeze_cancel_requested = false;
                self.state.blocking_write().message =
                    format!("Rendering freeze for '{}'", track_name);
                return self.send(Action::TrackOfflineBounce {
                    track_name: track_name.clone(),
                    output_path: render_abs,
                    start_sample: 0,
                    length_samples: render_length.max(1),
                    automation_lanes,
                });
            }
            Message::TrackFreezeFlatten { ref track_name } => {
                let is_frozen = {
                    let state = self.state.blocking_read();
                    state
                        .tracks
                        .iter()
                        .find(|t| t.name == *track_name)
                        .is_some_and(|t| t.frozen)
                };
                if !is_frozen {
                    self.state.blocking_write().message =
                        format!("Track '{}' is not frozen", track_name);
                    return Task::none();
                }
                {
                    let mut state = self.state.blocking_write();
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
            Message::TrackAutomationToggle { ref track_name } => {
                let mut state = self.state.blocking_write();
                if let Some(track) = state
                    .tracks
                    .iter_mut()
                    .find(|track| track.name == track_name.as_str())
                {
                    let any_visible = track.automation_lanes.iter().any(|lane| lane.visible);
                    if any_visible {
                        for lane in &mut track.automation_lanes {
                            lane.visible = false;
                        }
                    } else if track.automation_lanes.is_empty() {
                        track
                            .automation_lanes
                            .push(crate::state::TrackAutomationLane {
                                target: crate::message::TrackAutomationTarget::Volume,
                                visible: true,
                                points: vec![],
                            });
                    } else {
                        for lane in &mut track.automation_lanes {
                            lane.visible = true;
                        }
                    }
                    track.height = track.min_height_for_layout().max(60.0);
                }
            }
            Message::TrackAutomationCycleMode { ref track_name } => {
                let mut state = self.state.blocking_write();
                if let Some(track) = state
                    .tracks
                    .iter_mut()
                    .find(|track| track.name == track_name.as_str())
                {
                    let next_mode = match track.automation_mode {
                        TrackAutomationMode::Read => TrackAutomationMode::Touch,
                        TrackAutomationMode::Touch => TrackAutomationMode::Latch,
                        TrackAutomationMode::Latch => TrackAutomationMode::Write,
                        TrackAutomationMode::Write => TrackAutomationMode::Read,
                    };
                    track.automation_mode = next_mode;
                    state.message = format!(
                        "Track '{}' automation mode: {}",
                        track.name, track.automation_mode
                    );
                }
                drop(state);
                let key = track_name.clone();
                let mode = self
                    .state
                    .blocking_read()
                    .tracks
                    .iter()
                    .find(|track| track.name == key)
                    .map(|track| track.automation_mode);
                match mode {
                    Some(TrackAutomationMode::Read) => {
                        self.touch_active_keys.remove(&key);
                        self.touch_automation_overrides.remove(&key);
                        self.latch_automation_overrides.remove(&key);
                    }
                    Some(TrackAutomationMode::Touch) => {
                        self.latch_automation_overrides.remove(&key);
                    }
                    Some(TrackAutomationMode::Write) => {
                        self.touch_active_keys.remove(&key);
                        self.touch_automation_overrides.remove(&key);
                    }
                    _ => {}
                }
            }
            Message::TrackAutomationAddLane {
                ref track_name,
                target,
            } => {
                let mut state = self.state.blocking_write();
                if let Some(track) = state
                    .tracks
                    .iter_mut()
                    .find(|track| track.name == track_name.as_str())
                {
                    if let Some(lane) = track
                        .automation_lanes
                        .iter_mut()
                        .find(|lane| lane.target == target)
                    {
                        lane.visible = true;
                    } else {
                        track
                            .automation_lanes
                            .push(crate::state::TrackAutomationLane {
                                target,
                                visible: true,
                                points: vec![],
                            });
                    }
                    track.height = track.min_height_for_layout().max(60.0);
                }
            }
            Message::TrackAutomationAddClapLanes {
                ref track_name,
                ref plugin_path,
            } => {
                #[cfg(any(target_os = "windows", all(unix, not(target_os = "macos"))))]
                {
                    let instance_id = {
                        let state = self.state.blocking_read();
                        state
                            .plugin_graphs_by_track
                            .get(track_name)
                            .and_then(|(plugins, _)| {
                                plugins
                                    .iter()
                                    .find(|plugin| {
                                        plugin.format.eq_ignore_ascii_case("CLAP")
                                            && (plugin.uri == *plugin_path
                                                || plugin.plugin_id == *plugin_path)
                                    })
                                    .map(|plugin| plugin.instance_id)
                            })
                    };
                    if let Some(instance_id) = instance_id {
                        self.pending_add_clap_automation_instances
                            .insert((track_name.clone(), instance_id));
                        return self.send(Action::TrackGetClapParameters {
                            track_name: track_name.clone(),
                            instance_id,
                        });
                    }
                    self.pending_add_clap_automation_paths
                        .insert((track_name.clone(), plugin_path.clone()));
                    return self.send(Action::TrackGetPluginGraph {
                        track_name: track_name.clone(),
                    });
                }
                #[cfg(not(any(target_os = "windows", all(unix, not(target_os = "macos")))))]
                {
                    self.state.blocking_write().message =
                        "CLAP automation lanes are unavailable on this platform".to_string();
                }
            }
            Message::TrackAutomationAddVst3Lanes {
                ref track_name,
                ref plugin_path,
            } => {
                #[cfg(any(target_os = "windows", all(unix, not(target_os = "macos"))))]
                {
                    let instance_id = {
                        let state = self.state.blocking_read();
                        state
                            .plugin_graphs_by_track
                            .get(track_name)
                            .and_then(|(plugins, _)| {
                                plugins
                                    .iter()
                                    .find(|plugin| {
                                        plugin.format.eq_ignore_ascii_case("VST3")
                                            && (plugin.uri == *plugin_path
                                                || plugin.plugin_id == *plugin_path)
                                    })
                                    .map(|plugin| plugin.instance_id)
                            })
                    };
                    if let Some(instance_id) = instance_id {
                        self.pending_add_vst3_automation_instances
                            .insert((track_name.clone(), instance_id));
                        return self.send(Action::TrackGetVst3Parameters {
                            track_name: track_name.clone(),
                            instance_id,
                        });
                    }
                    self.pending_add_vst3_automation_paths
                        .insert((track_name.clone(), plugin_path.clone()));
                    return self.send(Action::TrackGetPluginGraph {
                        track_name: track_name.clone(),
                    });
                }
                #[cfg(not(any(target_os = "windows", all(unix, not(target_os = "macos")))))]
                {
                    self.state.blocking_write().message =
                        "VST3 automation lanes are unavailable on this platform".to_string();
                }
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            Message::TrackAutomationAddLv2Lanes {
                ref track_name,
                ref plugin_uri,
            } => {
                let instance_id = {
                    let state = self.state.blocking_read();
                    state
                        .plugin_graphs_by_track
                        .get(track_name)
                        .and_then(|(plugins, _)| {
                            plugins
                                .iter()
                                .find(|plugin| {
                                    plugin.format.eq_ignore_ascii_case("LV2")
                                        && (plugin.uri == *plugin_uri
                                            || plugin.plugin_id == *plugin_uri)
                                })
                                .map(|plugin| plugin.instance_id)
                        })
                };
                if let Some(instance_id) = instance_id {
                    self.pending_add_lv2_automation_instances
                        .insert((track_name.clone(), instance_id));
                    return self.send(Action::TrackGetLv2PluginControls {
                        track_name: track_name.clone(),
                        instance_id,
                    });
                }
                self.pending_add_lv2_automation_uris
                    .insert((track_name.clone(), plugin_uri.clone()));
                return self.send(Action::TrackGetPluginGraph {
                    track_name: track_name.clone(),
                });
            }
            Message::TrackAutomationLaneHover {
                ref track_name,
                target,
                position,
            } => {
                let mut state = self.state.blocking_write();
                state.automation_lane_hover = Some((track_name.clone(), target, position));
            }
            Message::TrackAutomationLaneInsertPoint {
                ref track_name,
                target,
            } => {
                let pixels_per_sample = self.pixels_per_sample().max(1.0e-6);
                let mut state = self.state.blocking_write();
                let Some((hover_track, hover_target, hover_position)) = state
                    .automation_lane_hover
                    .as_ref()
                    .map(|(name, target, position)| (name.as_str(), *target, *position))
                else {
                    return Task::none();
                };
                if hover_track != track_name.as_str() || hover_target != target {
                    return Task::none();
                }
                if let Some(track) = state
                    .tracks
                    .iter_mut()
                    .find(|track| track.name == track_name.as_str())
                {
                    let lane_height = track.lane_layout().lane_height.max(12.0);
                    let lane_value_h = (lane_height - 6.0).max(1.0);
                    let value = (1.0 - ((hover_position.y - 3.0) / lane_value_h)).clamp(0.0, 1.0);
                    let sample = ((hover_position.x / pixels_per_sample).round().max(0.0)) as usize;

                    if let Some(lane) = track
                        .automation_lanes
                        .iter_mut()
                        .find(|lane| lane.target == target)
                    {
                        if let Some(existing) = lane.points.iter_mut().find(|p| p.sample == sample)
                        {
                            existing.value = value;
                        } else {
                            lane.points
                                .push(crate::state::TrackAutomationPoint { sample, value });
                            lane.points.sort_unstable_by_key(|p| p.sample);
                        }
                        lane.visible = true;
                    } else {
                        track
                            .automation_lanes
                            .push(crate::state::TrackAutomationLane {
                                target,
                                visible: true,
                                points: vec![crate::state::TrackAutomationPoint { sample, value }],
                            });
                    }
                }
            }
            Message::TrackAutomationLaneDeletePoint {
                ref track_name,
                target,
                sample,
            } => {
                let mut state = self.state.blocking_write();
                if let Some(track) = state
                    .tracks
                    .iter_mut()
                    .find(|track| track.name == track_name.as_str())
                    && let Some(lane) = track
                        .automation_lanes
                        .iter_mut()
                        .find(|lane| lane.target == target)
                {
                    lane.points.retain(|point| point.sample != sample);
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
            Message::ClipWarpReset {
                ref track_idx,
                clip_idx,
            } => {
                return self.send(Action::SetAudioClipWarpMarkers {
                    track_name: track_idx.clone(),
                    clip_index: clip_idx,
                    warp_markers: vec![],
                });
            }
            Message::ClipWarpHalfSpeed {
                ref track_idx,
                clip_idx,
            } => {
                let clip_len = {
                    let state = self.state.blocking_read();
                    state
                        .tracks
                        .iter()
                        .find(|t| t.name == *track_idx)
                        .and_then(|t| t.audio.clips.get(clip_idx))
                        .map(|c| c.length)
                };
                if let Some(clip_len) = clip_len {
                    return self.send(Action::SetAudioClipWarpMarkers {
                        track_name: track_idx.clone(),
                        clip_index: clip_idx,
                        warp_markers: Self::warp_markers_for_speed(clip_len, 0.5),
                    });
                }
            }
            Message::ClipWarpDoubleSpeed {
                ref track_idx,
                clip_idx,
            } => {
                let clip_len = {
                    let state = self.state.blocking_read();
                    state
                        .tracks
                        .iter()
                        .find(|t| t.name == *track_idx)
                        .and_then(|t| t.audio.clips.get(clip_idx))
                        .map(|c| c.length)
                };
                if let Some(clip_len) = clip_len {
                    return self.send(Action::SetAudioClipWarpMarkers {
                        track_name: track_idx.clone(),
                        clip_index: clip_idx,
                        warp_markers: Self::warp_markers_for_speed(clip_len, 2.0),
                    });
                }
            }
            Message::ClipWarpAddMarker {
                ref track_idx,
                clip_idx,
            } => {
                let marker_state = {
                    let state = self.state.blocking_read();
                    state
                        .tracks
                        .iter()
                        .find(|t| t.name == *track_idx)
                        .and_then(|t| t.audio.clips.get(clip_idx))
                        .map(|c| (c.length, c.warp_markers.clone()))
                };
                if let Some((clip_len, markers)) = marker_state {
                    return self.send(Action::SetAudioClipWarpMarkers {
                        track_name: track_idx.clone(),
                        clip_index: clip_idx,
                        warp_markers: Self::add_warp_marker_between(&markers, clip_len),
                    });
                }
            }
            Message::ClipSetActiveTake {
                ref track_idx,
                clip_idx,
                kind,
            } => {
                let updates = {
                    let state = self.state.blocking_read();
                    let Some(track) = state.tracks.iter().find(|t| t.name == *track_idx) else {
                        return Task::none();
                    };
                    match kind {
                        Kind::Audio => {
                            let Some(selected) = track.audio.clips.get(clip_idx) else {
                                return Task::none();
                            };
                            let selected_end = selected.start.saturating_add(selected.length);
                            track
                                .audio
                                .clips
                                .iter()
                                .enumerate()
                                .filter_map(|(idx, clip)| {
                                    let end = clip.start.saturating_add(clip.length);
                                    (!clip.take_lane_locked
                                        && selected.start < end
                                        && clip.start < selected_end)
                                        .then_some((idx, idx != clip_idx))
                                })
                                .collect::<Vec<_>>()
                        }
                        Kind::MIDI => {
                            let Some(selected) = track.midi.clips.get(clip_idx) else {
                                return Task::none();
                            };
                            let selected_end = selected.start.saturating_add(selected.length);
                            track
                                .midi
                                .clips
                                .iter()
                                .enumerate()
                                .filter_map(|(idx, clip)| {
                                    let end = clip.start.saturating_add(clip.length);
                                    (!clip.take_lane_locked
                                        && selected.start < end
                                        && clip.start < selected_end)
                                        .then_some((idx, idx != clip_idx))
                                })
                                .collect::<Vec<_>>()
                        }
                    }
                };
                if updates.is_empty() {
                    return Task::none();
                }
                let mut tasks = vec![self.send(Action::BeginHistoryGroup)];
                for (idx, should_mute) in updates {
                    tasks.push(self.send(Action::SetClipMuted {
                        track_name: track_idx.clone(),
                        clip_index: idx,
                        kind,
                        muted: should_mute,
                    }));
                }
                tasks.push(self.send(Action::EndHistoryGroup));
                return Task::batch(tasks);
            }
            Message::ClipCycleActiveTake {
                ref track_idx,
                clip_idx,
                kind,
            } => {
                let updates = {
                    let state = self.state.blocking_read();
                    let Some(track) = state.tracks.iter().find(|t| t.name == *track_idx) else {
                        return Task::none();
                    };
                    let mut group: Vec<(usize, usize, bool)> = match kind {
                        Kind::Audio => {
                            let Some(selected) = track.audio.clips.get(clip_idx) else {
                                return Task::none();
                            };
                            let selected_end = selected.start.saturating_add(selected.length);
                            track
                                .audio
                                .clips
                                .iter()
                                .enumerate()
                                .filter_map(|(idx, clip)| {
                                    let end = clip.start.saturating_add(clip.length);
                                    (!clip.take_lane_locked
                                        && selected.start < end
                                        && clip.start < selected_end)
                                        .then_some((idx, clip.start, clip.muted))
                                })
                                .collect()
                        }
                        Kind::MIDI => {
                            let Some(selected) = track.midi.clips.get(clip_idx) else {
                                return Task::none();
                            };
                            let selected_end = selected.start.saturating_add(selected.length);
                            track
                                .midi
                                .clips
                                .iter()
                                .enumerate()
                                .filter_map(|(idx, clip)| {
                                    let end = clip.start.saturating_add(clip.length);
                                    (!clip.take_lane_locked
                                        && selected.start < end
                                        && clip.start < selected_end)
                                        .then_some((idx, clip.start, clip.muted))
                                })
                                .collect()
                        }
                    };
                    if group.is_empty() {
                        return Task::none();
                    }
                    group.sort_by_key(|(idx, start, _)| (*start, *idx));
                    let current_pos = group
                        .iter()
                        .position(|(idx, _, _)| *idx == clip_idx)
                        .or_else(|| group.iter().position(|(_, _, muted)| !*muted))
                        .unwrap_or(0);
                    let next_pos = (current_pos + 1) % group.len();
                    let next_idx = group[next_pos].0;
                    group
                        .iter()
                        .map(|(idx, _, _)| (*idx, *idx != next_idx))
                        .collect::<Vec<_>>()
                };
                if updates.is_empty() {
                    return Task::none();
                }
                let mut tasks = vec![self.send(Action::BeginHistoryGroup)];
                for (idx, should_mute) in updates {
                    tasks.push(self.send(Action::SetClipMuted {
                        track_name: track_idx.clone(),
                        clip_index: idx,
                        kind,
                        muted: should_mute,
                    }));
                }
                tasks.push(self.send(Action::EndHistoryGroup));
                return Task::batch(tasks);
            }
            Message::ClipUnmuteTakesInRange {
                ref track_idx,
                clip_idx,
                kind,
            } => {
                let updates = {
                    let state = self.state.blocking_read();
                    let Some(track) = state.tracks.iter().find(|t| t.name == *track_idx) else {
                        return Task::none();
                    };
                    match kind {
                        Kind::Audio => {
                            let Some(selected) = track.audio.clips.get(clip_idx) else {
                                return Task::none();
                            };
                            let selected_end = selected.start.saturating_add(selected.length);
                            track
                                .audio
                                .clips
                                .iter()
                                .enumerate()
                                .filter_map(|(idx, clip)| {
                                    let end = clip.start.saturating_add(clip.length);
                                    (!clip.take_lane_locked
                                        && selected.start < end
                                        && clip.start < selected_end)
                                        .then_some((idx, false))
                                })
                                .collect::<Vec<_>>()
                        }
                        Kind::MIDI => {
                            let Some(selected) = track.midi.clips.get(clip_idx) else {
                                return Task::none();
                            };
                            let selected_end = selected.start.saturating_add(selected.length);
                            track
                                .midi
                                .clips
                                .iter()
                                .enumerate()
                                .filter_map(|(idx, clip)| {
                                    let end = clip.start.saturating_add(clip.length);
                                    (!clip.take_lane_locked
                                        && selected.start < end
                                        && clip.start < selected_end)
                                        .then_some((idx, false))
                                })
                                .collect::<Vec<_>>()
                        }
                    }
                };
                if updates.is_empty() {
                    return Task::none();
                }
                let mut tasks = vec![self.send(Action::BeginHistoryGroup)];
                for (idx, should_mute) in updates {
                    tasks.push(self.send(Action::SetClipMuted {
                        track_name: track_idx.clone(),
                        clip_index: idx,
                        kind,
                        muted: should_mute,
                    }));
                }
                tasks.push(self.send(Action::EndHistoryGroup));
                return Task::batch(tasks);
            }
            Message::ClipTakeLanePinToggle {
                ref track_idx,
                clip_idx,
                kind,
            } => {
                let mut state = self.state.blocking_write();
                let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_idx) else {
                    return Task::none();
                };
                match kind {
                    Kind::Audio => {
                        if clip_idx >= track.audio.clips.len() {
                            return Task::none();
                        }
                        let current_take = {
                            let (take_idx, _) = Self::assign_take_lanes(
                                &track.audio.clips,
                                |_| 0,
                                |clip| clip.start,
                                |clip| clip.length,
                                |clip| clip.take_lane_override,
                            );
                            take_idx.get(clip_idx).copied().unwrap_or(0)
                        };
                        let clip = &mut track.audio.clips[clip_idx];
                        if clip.take_lane_pinned {
                            clip.take_lane_pinned = false;
                            if !clip.take_lane_locked {
                                clip.take_lane_override = None;
                            }
                        } else {
                            clip.take_lane_pinned = true;
                            clip.take_lane_override = Some(current_take);
                        }
                    }
                    Kind::MIDI => {
                        if clip_idx >= track.midi.clips.len() {
                            return Task::none();
                        }
                        let lane_count = track.midi.ins.max(1);
                        let (take_idx, _) = Self::assign_take_lanes(
                            &track.midi.clips,
                            |clip| clip.input_channel.min(lane_count.saturating_sub(1)),
                            |clip| clip.start,
                            |clip| clip.length,
                            |clip| clip.take_lane_override,
                        );
                        let current_take = take_idx.get(clip_idx).copied().unwrap_or(0);
                        let clip = &mut track.midi.clips[clip_idx];
                        if clip.take_lane_pinned {
                            clip.take_lane_pinned = false;
                            if !clip.take_lane_locked {
                                clip.take_lane_override = None;
                            }
                        } else {
                            clip.take_lane_pinned = true;
                            clip.take_lane_override = Some(current_take);
                        }
                    }
                }
            }
            Message::ClipTakeLaneLockToggle {
                ref track_idx,
                clip_idx,
                kind,
            } => {
                let mut state = self.state.blocking_write();
                let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_idx) else {
                    return Task::none();
                };
                match kind {
                    Kind::Audio => {
                        let Some(clip) = track.audio.clips.get_mut(clip_idx) else {
                            return Task::none();
                        };
                        clip.take_lane_locked = !clip.take_lane_locked;
                    }
                    Kind::MIDI => {
                        let Some(clip) = track.midi.clips.get_mut(clip_idx) else {
                            return Task::none();
                        };
                        clip.take_lane_locked = !clip.take_lane_locked;
                    }
                }
            }
            Message::ClipTakeLaneMove {
                ref track_idx,
                clip_idx,
                kind,
                delta,
            } => {
                let mut state = self.state.blocking_write();
                let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_idx) else {
                    return Task::none();
                };
                match kind {
                    Kind::Audio => {
                        if clip_idx >= track.audio.clips.len() {
                            return Task::none();
                        }
                        let (take_idx, _) = Self::assign_take_lanes(
                            &track.audio.clips,
                            |_| 0,
                            |clip| clip.start,
                            |clip| clip.length,
                            |clip| clip.take_lane_override,
                        );
                        let current_take = take_idx.get(clip_idx).copied().unwrap_or(0);
                        let clip = &mut track.audio.clips[clip_idx];
                        if clip.take_lane_locked {
                            return Task::none();
                        }
                        let next_take = if delta.is_negative() {
                            current_take.saturating_sub(delta.unsigned_abs() as usize)
                        } else {
                            current_take.saturating_add(delta as usize)
                        };
                        clip.take_lane_override = Some(next_take);
                        clip.take_lane_pinned = true;
                    }
                    Kind::MIDI => {
                        if clip_idx >= track.midi.clips.len() {
                            return Task::none();
                        }
                        let lane_count = track.midi.ins.max(1);
                        let (take_idx, _) = Self::assign_take_lanes(
                            &track.midi.clips,
                            |clip| clip.input_channel.min(lane_count.saturating_sub(1)),
                            |clip| clip.start,
                            |clip| clip.length,
                            |clip| clip.take_lane_override,
                        );
                        let current_take = take_idx.get(clip_idx).copied().unwrap_or(0);
                        let clip = &mut track.midi.clips[clip_idx];
                        if clip.take_lane_locked {
                            return Task::none();
                        }
                        let next_take = if delta.is_negative() {
                            current_take.saturating_sub(delta.unsigned_abs() as usize)
                        } else {
                            current_take.saturating_add(delta as usize)
                        };
                        clip.take_lane_override = Some(next_take);
                        clip.take_lane_pinned = true;
                    }
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
                state.comp_swipe_start = None;
                state.comp_swipe_end = None;
                state.midi_clip_create_start = None;
                state.midi_clip_create_end = None;
                state.selected_clips.clear();
            }
            Message::MousePressed(button) => {
                if self.modal.is_none()
                    && matches!(self.state.blocking_read().view, View::Workspace)
                {
                    if button == mouse::Button::Middle {
                        let cursor = self.state.blocking_read().cursor;
                        return self.split_clip_at_position(cursor);
                    }
                    let mut state = self.state.blocking_write();
                    match button {
                        mouse::Button::Left => {
                            state.mouse_left_down = true;
                            state.clip_marquee_start = None;
                            state.clip_marquee_end = None;
                            if matches!(self.edit_tool, crate::message::EditTool::Comp) {
                                state.comp_swipe_start = Some(state.cursor);
                                state.comp_swipe_end = Some(state.cursor);
                            }
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
                if !self.selected_tempo_points.is_empty() {
                    return self.update(Message::TempoSelectionDelete);
                }
                if !self.selected_time_signature_points.is_empty() {
                    return self.update(Message::TimeSignatureSelectionDelete);
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
                                initial_value: initial_value as f32,
                                initial_mouse_x: state.cursor.x,
                                initial_length: clip.length as f32,
                            });
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
                        Kind::Audio => track.audio.clips.get(clip_idx).and_then(|clip| {
                            if clip.take_lane_locked {
                                return None;
                            }
                            if is_fade_out {
                                Some(clip.fade_out_samples)
                            } else {
                                Some(clip.fade_in_samples)
                            }
                        }),
                        Kind::MIDI => track.midi.clips.get(clip_idx).and_then(|clip| {
                            if clip.take_lane_locked {
                                return None;
                            }
                            if is_fade_out {
                                Some(clip.fade_out_samples)
                            } else {
                                Some(clip.fade_in_samples)
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
                    if matches!(self.edit_tool, crate::message::EditTool::Comp)
                        && matches!(self.state.blocking_read().view, View::Workspace)
                    {
                        let mut state = self.state.blocking_write();
                        if state.comp_swipe_start.is_some() {
                            state.comp_swipe_end = Some(position);
                        }
                        return Task::none();
                    }
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
                    && matches!(self.edit_tool, crate::message::EditTool::Select)
                {
                    state.clip_marquee_start = Some(position);
                    state.clip_marquee_end = Some(position);
                }
                if state.mouse_right_down
                    && !matches!(resizing, Some(Resizing::Clip { .. }))
                    && self.clip.is_none()
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
            }
            Message::MouseReleased => {
                if matches!(self.edit_tool, crate::message::EditTool::Comp) {
                    let had_swipe = {
                        let mut state = self.state.blocking_write();
                        state.mouse_left_down = false;
                        state.mouse_right_down = false;
                        state.clip_click_consumed = false;
                        state.resizing = None;
                        self.clip = None;
                        state.comp_swipe_start.is_some() && state.comp_swipe_end.is_some()
                    };
                    if had_swipe {
                        return self.apply_comp_swipe();
                    }
                    return Task::none();
                }
                let active = std::mem::take(&mut self.touch_active_keys);
                for (track_name, keys) in active {
                    if let Some(values) = self.touch_automation_overrides.get_mut(&track_name) {
                        for key in keys {
                            values.remove(&key);
                        }
                        if values.is_empty() {
                            self.touch_automation_overrides.remove(&track_name);
                        }
                    }
                }
                if self.modal.is_some() {
                    let mut state = self.state.blocking_write();
                    state.mouse_left_down = false;
                    state.mouse_right_down = false;
                    state.clip_click_consumed = false;
                    state.clip_marquee_start = None;
                    state.clip_marquee_end = None;
                    state.comp_swipe_start = None;
                    state.comp_swipe_end = None;
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
                if matches!(self.edit_tool, crate::message::EditTool::Comp) {
                    return Task::none();
                }
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
                let state = self.state.blocking_read();
                if index < state.tracks.len() {
                    self.track = Some(state.tracks[index].name.clone());
                }
            }
            Message::TrackDropped(point, _rect) => {
                if self.track.is_some() {
                    return iced_drop::zones_on_point(Message::HandleTrackZones, point, None, None);
                }
                self.track = None;
            }
            Message::HandleTrackZones(ref zones) => {
                if let Some(index_name) = &self.track {
                    let dragged_id = Id::from(index_name.clone());
                    let target_zone = zones
                        .iter()
                        .find(|(zone_id, _)| *zone_id != dragged_id)
                        .or_else(|| zones.first());
                    if let Some((track_id, _)) = target_zone {
                        let mut state = self.state.blocking_write();
                        if let Some(index) = state.tracks.iter().position(|t| t.name == *index_name)
                        {
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
                self.track = None;
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
                let playback_rate = self.playback_rate_hz.max(1.0);

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
                                        playback_rate.round().max(1.0) as u32,
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
                                                    muted: false,
                                                    kind: Kind::Audio,
                                                    fade_enabled: true,
                                                    fade_in_samples: 240,
                                                    fade_out_samples: 240,
                                                    warp_markers: vec![],
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
                                                    muted: false,
                                                    kind: Kind::MIDI,
                                                    fade_enabled: true,
                                                    fade_in_samples: 240,
                                                    fade_out_samples: 240,
                                                    warp_markers: vec![],
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
            Message::ExportDiagnosticsBundleRequest => {
                self.pending_diagnostics_bundle_export = true;
                self.diagnostics_bundle_wait_session_report = true;
                self.diagnostics_bundle_wait_midi_report = true;
                return Task::batch(vec![
                    self.send(Action::RequestSessionDiagnostics),
                    self.send(Action::RequestMidiLearnMappingsReport),
                ]);
            }
            Message::SessionDiagnosticsRequest => {
                return self.send(Action::RequestSessionDiagnostics);
            }
            Message::MidiLearnMappingsPanelToggle => {
                self.midi_mappings_panel_open = !self.midi_mappings_panel_open;
                if self.midi_mappings_panel_open {
                    return self.send(Action::RequestMidiLearnMappingsReport);
                }
            }
            Message::MidiLearnMappingsReportRequest => {
                return self.send(Action::RequestMidiLearnMappingsReport);
            }
            Message::MidiLearnMappingsExportRequest => match self.export_midi_mappings_file() {
                Ok(path) => {
                    self.state.blocking_write().message =
                        format!("Exported MIDI mappings: {}", path.display());
                }
                Err(e) => {
                    self.state.blocking_write().message = e;
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
                    self.state.blocking_write().message = "Imported MIDI mappings".to_string();
                    return Task::batch(tasks);
                }
                Err(e) => {
                    self.state.blocking_write().message = e;
                }
            },
            Message::MidiLearnMappingsClearAllRequest => {
                return self.send(Action::ClearAllMidiLearnBindings);
            }
            Message::ExportSampleRateSelected(rate) => {
                self.export_sample_rate_hz = rate;
            }
            Message::ExportFormatWavToggled(enabled) => {
                self.export_format_wav = enabled;
                let selected = self.selected_export_formats();
                let valid_bit_depths = super::Maolan::export_bit_depth_options(&selected);
                if !valid_bit_depths.contains(&self.export_bit_depth)
                    && let Some(first) = valid_bit_depths.first().copied()
                {
                    self.export_bit_depth = first;
                }
            }
            Message::ExportFormatMp3Toggled(enabled) => {
                self.export_format_mp3 = enabled;
                let selected = self.selected_export_formats();
                let valid_bit_depths = super::Maolan::export_bit_depth_options(&selected);
                if !valid_bit_depths.contains(&self.export_bit_depth)
                    && let Some(first) = valid_bit_depths.first().copied()
                {
                    self.export_bit_depth = first;
                }
            }
            Message::ExportFormatOggToggled(enabled) => {
                self.export_format_ogg = enabled;
                let selected = self.selected_export_formats();
                let valid_bit_depths = super::Maolan::export_bit_depth_options(&selected);
                if !valid_bit_depths.contains(&self.export_bit_depth)
                    && let Some(first) = valid_bit_depths.first().copied()
                {
                    self.export_bit_depth = first;
                }
            }
            Message::ExportFormatFlacToggled(enabled) => {
                self.export_format_flac = enabled;
                let selected = self.selected_export_formats();
                let valid_bit_depths = super::Maolan::export_bit_depth_options(&selected);
                if !valid_bit_depths.contains(&self.export_bit_depth)
                    && let Some(first) = valid_bit_depths.first().copied()
                {
                    self.export_bit_depth = first;
                }
            }
            Message::ExportMp3ModeSelected(mode) => {
                self.export_mp3_mode = mode;
            }
            Message::ExportMp3BitrateSelected(kbps) => {
                self.export_mp3_bitrate_kbps = kbps;
            }
            Message::ExportOggQualityInput(ref input) => {
                self.export_ogg_quality_input = input
                    .chars()
                    .filter(|c| c.is_ascii_digit() || *c == '-' || *c == '.')
                    .collect();
            }
            Message::ExportBitDepthSelected(bit_depth) => {
                self.export_bit_depth = bit_depth;
            }
            Message::ExportRenderModeSelected(mode) => {
                self.export_render_mode = mode;
                if !matches!(mode, ExportRenderMode::Mixdown) {
                    self.export_normalize = false;
                }
            }
            Message::ExportRealtimeFallbackToggled(enabled) => {
                self.export_realtime_fallback = enabled;
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
            Message::ExportMasterLimiterToggled(enabled) => {
                self.export_master_limiter = enabled;
            }
            Message::ExportMasterLimiterCeilingInput(ref input) => {
                self.export_master_limiter_ceiling_input = input
                    .chars()
                    .filter(|c| c.is_ascii_digit() || *c == '-' || *c == '.')
                    .collect();
            }
            Message::ExportSettingsConfirm => {
                let master_ceiling = self.export_master_limiter_ceiling_input.parse::<f32>().ok();
                let Some(master_ceiling) = master_ceiling else {
                    self.state.blocking_write().message =
                        "Master limiter ceiling must be a number in dBTP".to_string();
                    return Task::none();
                };
                if !(-20.0..=0.0).contains(&master_ceiling) {
                    self.state.blocking_write().message =
                        "Master limiter ceiling must be between -20.0 and 0.0 dBTP".to_string();
                    return Task::none();
                }
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
                let selected_formats = self.selected_export_formats();
                if selected_formats.is_empty() {
                    self.state.blocking_write().message =
                        "Select at least one export format".to_string();
                    return Task::none();
                }
                if self.export_format_ogg {
                    let ogg_quality = self.export_ogg_quality_input.parse::<f32>().ok();
                    let Some(ogg_quality) = ogg_quality else {
                        self.state.blocking_write().message =
                            "OGG quality must be a number between -0.1 and 1.0".to_string();
                        return Task::none();
                    };
                    if !(-0.1..=1.0).contains(&ogg_quality) {
                        self.state.blocking_write().message =
                            "OGG quality must be between -0.1 and 1.0".to_string();
                        return Task::none();
                    }
                }
                self.modal = None;
                return Task::perform(
                    async move {
                        AsyncFileDialog::new()
                            .set_title("Export Audio")
                            .add_filter("Audio", &["wav", "mp3", "ogg", "flac"])
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
                let export_master_limiter = self.export_master_limiter;
                let export_master_limiter_ceiling_dbtp = self
                    .export_master_limiter_ceiling_input
                    .parse::<f32>()
                    .ok()
                    .unwrap_or(-1.0);
                let export_realtime_fallback = self.export_realtime_fallback;
                let export_formats = self.selected_export_formats();
                if export_formats.is_empty() {
                    self.state.blocking_write().message =
                        "Select at least one export format".to_string();
                    return Task::none();
                }
                let export_path = Self::export_base_path(path.clone());
                let export_mp3_mode = self.export_mp3_mode;
                let export_mp3_bitrate_kbps = self.export_mp3_bitrate_kbps;
                let export_ogg_quality = self
                    .export_ogg_quality_input
                    .parse::<f32>()
                    .ok()
                    .unwrap_or(0.6);
                let state_clone = self.state.clone();
                let render_mode = self.export_render_mode;
                let (
                    metadata_author,
                    metadata_album,
                    metadata_year,
                    metadata_track_number,
                    metadata_genre,
                ) = {
                    let state = self.state.blocking_read();
                    (
                        state.session_author.trim().to_string(),
                        state.session_album.trim().to_string(),
                        state.session_year.trim().parse::<u32>().ok(),
                        state.session_track_number.trim().parse::<u32>().ok(),
                        state.session_genre.trim().to_string(),
                    )
                };

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

                            let options = super::ExportSessionOptions {
                                export_path: export_path.clone(),
                                sample_rate,
                                formats: export_formats,
                                render_mode,
                                realtime_fallback: export_realtime_fallback,
                                bit_depth: export_bit_depth,
                                mp3_mode: export_mp3_mode,
                                mp3_bitrate_kbps: export_mp3_bitrate_kbps,
                                ogg_quality: export_ogg_quality,
                                normalize: export_normalize,
                                normalize_target_dbfs,
                                normalize_mode,
                                normalize_target_lufs,
                                normalize_true_peak_dbtp,
                                normalize_tp_limiter,
                                master_limiter: export_master_limiter,
                                master_limiter_ceiling_dbtp: export_master_limiter_ceiling_dbtp,
                                metadata_author,
                                metadata_album,
                                metadata_year,
                                metadata_track_number,
                                metadata_genre,
                                state: state_clone,
                                session_root: session_root.clone(),
                            };
                            let result = Self::export_session(&options, progress_fn).await;

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
                    self.state.blocking_write().message = operation
                        .clone()
                        .unwrap_or_else(|| "Export complete".to_string());
                } else if let Some(op) = operation {
                    let percent = (progress * 100.0) as usize;
                    self.state.blocking_write().message = format!("Exporting ({percent}%): {}", op);
                } else {
                    let percent = (progress * 100.0) as usize;
                    self.state.blocking_write().message = format!("Exporting ({percent}%)...");
                }
            }
            Message::PreferencesSampleRateSelected(rate) => {
                self.prefs_export_sample_rate_hz = rate;
            }
            Message::PreferencesSnapModeSelected(mode) => {
                self.prefs_snap_mode = mode;
            }
            Message::PreferencesSave => {
                let prefs = super::AppPreferences {
                    default_export_sample_rate_hz: self.prefs_export_sample_rate_hz,
                    default_snap_mode: self.prefs_snap_mode,
                };
                let path = super::preferences_path();
                if let Some(parent) = path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                match fs::File::create(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|f| {
                        serde_json::to_writer_pretty(f, &prefs).map_err(|e| e.to_string())
                    }) {
                    Ok(()) => {
                        self.export_sample_rate_hz = self.prefs_export_sample_rate_hz;
                        self.snap_mode = self.prefs_snap_mode;
                        self.modal = None;
                        self.state.blocking_write().message =
                            format!("Preferences saved: {}", path.display());
                    }
                    Err(e) => {
                        self.state.blocking_write().message =
                            format!("Failed to save preferences: {e}");
                    }
                }
            }
            Message::SessionMetadataAuthorInput(ref value) => {
                self.state.blocking_write().session_author = value.clone();
            }
            Message::SessionMetadataAlbumInput(ref value) => {
                self.state.blocking_write().session_album = value.clone();
            }
            Message::SessionMetadataYearInput(ref value) => {
                self.state.blocking_write().session_year = value
                    .chars()
                    .filter(|c| c.is_ascii_digit())
                    .collect::<String>();
            }
            Message::SessionMetadataTrackNumberInput(ref value) => {
                self.state.blocking_write().session_track_number = value
                    .chars()
                    .filter(|c| c.is_ascii_digit())
                    .collect::<String>();
            }
            Message::SessionMetadataGenreInput(ref value) => {
                self.state.blocking_write().session_genre = value.clone();
            }
            Message::SessionMetadataSave => {
                {
                    let mut state = self.state.blocking_write();
                    state.session_author = state.session_author.trim().to_string();
                    state.session_album = state.session_album.trim().to_string();
                    state.session_year = state.session_year.trim().to_string();
                    state.session_track_number = state.session_track_number.trim().to_string();
                    state.session_genre = state.session_genre.trim().to_string();
                    state.message = "Session metadata updated".to_string();
                }
                self.has_unsaved_changes = true;
                self.modal = None;
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
            Message::DrainAudioPeakUpdates => {
                let updates = if let Ok(mut queue) = AUDIO_PEAK_UPDATES.lock() {
                    std::mem::take(&mut *queue)
                } else {
                    Vec::new()
                };
                if updates.is_empty() {
                    return Task::none();
                }

                let mut state = self.state.blocking_write();
                for update in updates {
                    let key = Self::audio_clip_key(
                        &update.track_name,
                        &update.clip_name,
                        update.start,
                        update.length,
                        update.offset,
                    );
                    if update.done {
                        self.pending_peak_rebuilds.remove(&key);
                        continue;
                    }
                    if update.target_bins == 0 {
                        continue;
                    }
                    if let Some(track) = state
                        .tracks
                        .iter_mut()
                        .find(|t| t.name == update.track_name)
                        && let Some(clip) = track.audio.clips.iter_mut().find(|clip| {
                            clip.name == update.clip_name
                                && clip.start == update.start
                                && clip.length == update.length
                                && clip.offset == update.offset
                        })
                    {
                        if clip.peaks.len() != update.channels
                            || clip.peaks.first().map(Vec::len).unwrap_or(0) != update.target_bins
                        {
                            clip.peaks = std::sync::Arc::new(vec![
                                vec![
                                    [0.0_f32, 0.0_f32];
                                    update.target_bins
                                ];
                                update.channels
                            ]);
                        }
                        let chunk_bins = update.peaks.first().map(Vec::len).unwrap_or(0);
                        let end = (update.bin_start + chunk_bins).min(update.target_bins);
                        if end > update.bin_start {
                            let peaks_mut = std::sync::Arc::make_mut(&mut clip.peaks);
                            for channel_idx in 0..update.channels.min(peaks_mut.len()) {
                                if let Some(src) = update.peaks.get(channel_idx) {
                                    let dst = &mut peaks_mut[channel_idx][update.bin_start..end];
                                    let n = dst.len().min(src.len());
                                    dst[..n].copy_from_slice(&src[..n]);
                                }
                            }
                        }
                    }
                }
            }
            Message::Workspace => {
                let mut state = self.state.blocking_write();
                state.view = View::Workspace;
                drop(state);
                return self.queue_midi_clip_preview_loads();
            }
            Message::Connections => {
                let mut state = self.state.blocking_write();
                state.view = View::Connections;
            }
            Message::MidiClipPreviewLoaded {
                ref track_idx,
                clip_idx,
                ref clip_name,
                ref notes,
            } => {
                self.pending_midi_clip_previews.remove(&(
                    track_idx.clone(),
                    clip_idx,
                    clip_name.clone(),
                ));
                let valid = {
                    let state = self.state.blocking_read();
                    state
                        .tracks
                        .iter()
                        .find(|track| track.name == *track_idx)
                        .and_then(|track| track.midi.clips.get(clip_idx))
                        .is_some_and(|clip| clip.name == *clip_name)
                };
                if valid {
                    self.midi_clip_previews
                        .insert((track_idx.clone(), clip_idx), notes.clone());
                }
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
                        self.midi_clip_previews
                            .insert((track_idx.clone(), clip_idx), notes.clone());
                        self.pending_midi_clip_previews.remove(&(
                            track_idx.clone(),
                            clip_idx,
                            clip_name.clone(),
                        ));
                        {
                            let mut state = self.state.blocking_write();
                            state.piano = Some(PianoData {
                                track_idx: track_idx.clone(),
                                clip_index: clip_idx,
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
            #[cfg(target_os = "windows")]
            Message::HWInputSelected(ref hw) => {
                self.state.blocking_write().selected_input_hw = Some(hw.to_string());
            }
            #[cfg(target_os = "freebsd")]
            Message::HWInputSelected(ref hw) => {
                let mut state = self.state.blocking_write();
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
                state.selected_input_hw = Some(selected);
            }
            #[cfg(target_os = "linux")]
            Message::HWInputSelected(ref hw) => {
                let mut state = self.state.blocking_write();
                let refreshed = crate::state::discover_alsa_input_devices();
                let selected = refreshed
                    .iter()
                    .find(|candidate| candidate.id == hw.id)
                    .cloned()
                    .unwrap_or_else(|| hw.clone());
                if !refreshed.is_empty() {
                    state.available_input_hw = refreshed;
                }
                if let Some(bits) = selected.preferred_bits() {
                    state.oss_bits = bits;
                }
                state.selected_input_hw = Some(selected);
            }
            Message::HWBackendSelected(ref backend) => {
                let mut state = self.state.blocking_write();
                state.selected_backend = backend.clone();
                state.selected_hw = None;
                #[cfg(any(target_os = "freebsd", target_os = "linux"))]
                {
                    state.selected_input_hw = None;
                }
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
                            state.selected_input_hw = state.selected_hw.clone();
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
                    #[cfg(target_os = "linux")]
                    if matches!(backend, crate::state::AudioBackendOption::Alsa) {
                        let refreshed_out = crate::state::discover_alsa_output_devices();
                        let refreshed_in = crate::state::discover_alsa_input_devices();
                        if !refreshed_out.is_empty() {
                            state.available_hw = refreshed_out.clone();
                        }
                        if !refreshed_in.is_empty() {
                            state.available_input_hw = refreshed_in.clone();
                        }
                        if let Some(selected_out) = refreshed_out.first().cloned() {
                            if let Some(bits) = selected_out.preferred_bits() {
                                state.oss_bits = bits;
                            }
                            state.selected_hw = Some(selected_out);
                        }
                        if let Some(selected_in) = refreshed_in.first().cloned() {
                            if let Some(bits) = selected_in.preferred_bits() {
                                state.oss_bits = bits;
                            }
                            state.selected_input_hw = Some(selected_in);
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
            Message::HWSampleRateChanged(rate_hz) => {
                self.state.blocking_write().hw_sample_rate_hz = rate_hz.max(1);
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
