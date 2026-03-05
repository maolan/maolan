use super::{
    AutomationWriteKey, CLIENT, MIN_CLIP_WIDTH_PX, Maolan, TouchAutomationOverride, platform,
};
#[cfg(any(target_os = "windows", target_os = "macos"))]
use crate::message::PluginFormat;
use crate::{
    connections,
    message::{ExportNormalizeMode, Message, Show, TrackAutomationMode, TrackAutomationTarget},
    state::{
        ConnectionViewSelection, HW, PianoData, PianoSysExPoint, Resizing, TempoPoint,
        TimeSignaturePoint, Track, TrackAutomationPoint, View,
    },
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
    message::{
        Action, ClipMoveFrom, ClipMoveTo, Message as EngineMessage, OfflineAutomationLane,
        OfflineAutomationPoint, OfflineAutomationTarget,
    },
};
use rfd::AsyncFileDialog;
use std::{
    collections::{HashMap, HashSet},
    process::exit,
    time::{Duration, Instant},
};
use tracing::error;

impl Maolan {
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
        let clip_idx = 0;

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

    fn collect_track_automation_actions(&mut self, sample: usize, tracks: &[Track]) -> Vec<Action> {
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
                self.pending_track_freeze_restore.clear();
                self.pending_track_freeze_bounce.clear();
                self.freeze_in_progress = false;
                self.freeze_progress = 0.0;
                self.freeze_track_name = None;
                self.freeze_cancel_requested = false;
                return Task::batch(tasks);
            }
            Message::Cancel => self.modal = None,
            Message::Request(ref a) => {
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
                    let tracks = self.state.blocking_read().tracks.clone();
                    let actions = self.collect_track_automation_actions(now_sample, &tracks);
                    if !actions.is_empty() {
                        tasks.extend(actions.into_iter().map(|a| self.send(a)));
                    }
                }
                if !tasks.is_empty() {
                    return Task::batch(tasks);
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
                let selected = self.selected_tempo_points.clone();
                let mut state = self.state.blocking_write();
                state
                    .tempo_points
                    .retain(|p| p.sample == 0 || !selected.contains(&p.sample));
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
                let selected = self.selected_time_signature_points.clone();
                let mut state = self.state.blocking_write();
                state
                    .time_signature_points
                    .retain(|p| p.sample == 0 || !selected.contains(&p.sample));
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

                if !selected_indices.is_empty()
                    && let Some(piano) = state.piano.as_mut()
                {
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
                    muted,
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
                                    muted: *muted,
                                    max_length_samples,
                                    peaks_file: None,
                                    peaks: audio_peaks,
                                    fade_enabled: *fade_enabled,
                                    fade_in_samples: *fade_in_samples,
                                    fade_out_samples: *fade_out_samples,
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
                }
                Action::SetClipMuted {
                    track_name,
                    clip_index,
                    kind,
                    muted,
                } => {
                    let mut state = self.state.blocking_write();
                    if let Some(track) = state.tracks.iter_mut().find(|t| &t.name == track_name) {
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
                    if name == "hw:out" {
                        let mut state = self.state.blocking_write();
                        state.hw_out_muted = !state.hw_out_muted;
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
                            Self::compute_audio_clip_peaks(&render_path, 512).unwrap_or_default();
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
                        if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name)
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
                        if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name)
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
                        if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name)
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

                    let mut pending_queries: Vec<Task<Message>> = vec![];
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
                                    && (plugin.uri == pending_uri
                                        || plugin.plugin_id == pending_uri)
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
                    state.message = format!("Renamed track to '{}'", new_name);
                }
                _ => {}
            },
            Message::Response(Err(ref e)) => {
                if !self.pending_track_freeze_bounce.is_empty() {
                    self.pending_track_freeze_bounce.clear();
                }
                self.freeze_in_progress = false;
                self.freeze_track_name = None;
                self.freeze_cancel_requested = false;
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
                        crate::message::TrackAutomationTarget::Lv2Parameter {
                            instance_id,
                            index,
                            min,
                            max,
                        } => {
                            #[cfg(all(unix, not(target_os = "macos")))]
                            {
                                OfflineAutomationTarget::Lv2Parameter {
                                    instance_id,
                                    index,
                                    min,
                                    max,
                                }
                            }
                            #[cfg(not(all(unix, not(target_os = "macos"))))]
                            {
                                continue;
                            }
                        }
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
            Message::TrackAutomationAddLv2Lanes {
                ref track_name,
                ref plugin_uri,
            } => {
                #[cfg(all(unix, not(target_os = "macos")))]
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
                #[cfg(not(all(unix, not(target_os = "macos"))))]
                {
                    self.state.blocking_write().message =
                        "LV2 automation lanes are unavailable on this platform".to_string();
                }
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
                    && state.midi_clip_create_start.is_none()
                    && can_start_midi_drag
                {
                    state.midi_clip_create_start = Some(position);
                    state.midi_clip_create_end = Some(position);
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
                                                    muted: false,
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
                                                    muted: false,
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

                            let options = super::ExportSessionOptions {
                                export_path: export_path.clone(),
                                sample_rate,
                                bit_depth: export_bit_depth,
                                normalize: export_normalize,
                                normalize_target_dbfs,
                                normalize_mode,
                                normalize_target_lufs,
                                normalize_true_peak_dbtp,
                                normalize_tp_limiter,
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
