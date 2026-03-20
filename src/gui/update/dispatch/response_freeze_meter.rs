use super::*;
use crate::state::StateData;
use tracing::error;

const MIXER_STRIP_SPACING: f32 = 2.0;
const MIXER_ROW_PADDING_X: f32 = 8.0;
const MIXER_OVERSCAN_PX: f32 = 160.0;

fn mixer_strip_width_for_channels(channels: usize) -> f32 {
    use crate::consts::workspace_mixer::{
        BAY_INSET, FADER_WIDTH, METER_BAR_GAP, METER_BAR_WIDTH, METER_PAD_X, SCALE_WIDTH,
        STRIP_WIDTH,
    };

    let channels = channels.max(1);
    let meter_inner_width =
        channels as f32 * METER_BAR_WIDTH + (channels.saturating_sub(1) as f32 * METER_BAR_GAP);
    let meter_total_width = meter_inner_width + (METER_PAD_X * 2.0);
    (FADER_WIDTH + SCALE_WIDTH + 3.0 + 8.0 + meter_total_width + 16.0 + (BAY_INSET * 2.0))
        .max(STRIP_WIDTH)
}

fn visible_mixer_track_names(
    app: &Maolan,
    state: &StateData,
) -> Option<std::collections::HashSet<String>> {
    if !app.mixer_visible {
        return None;
    }

    let metronome_width = if state.metronome_enabled {
        state
            .tracks
            .iter()
            .find(|track| track.name == crate::consts::state_ids::METRONOME_TRACK_ID)
            .map(|track| mixer_strip_width_for_channels(track.audio.outs))
            .unwrap_or(0.0)
    } else {
        0.0
    };
    let hw_out_channels = state.hw_out.as_ref().map(|hw| hw.channels).unwrap_or(0);
    let master_width = mixer_strip_width_for_channels(hw_out_channels.max(1));
    let output_width = master_width
        + metronome_width
        + if metronome_width > 0.0 {
            MIXER_STRIP_SPACING
        } else {
            0.0
        };
    let viewport_width = (app.size.width - output_width).max(0.0);

    let tracks: Vec<_> = state
        .tracks
        .iter()
        .filter(|track| track.name != crate::consts::state_ids::METRONOME_TRACK_ID)
        .collect();
    if tracks.is_empty() {
        return Some(std::collections::HashSet::new());
    }

    let content_width = tracks
        .iter()
        .map(|track| mixer_strip_width_for_channels(track.audio.outs))
        .sum::<f32>()
        + (MIXER_STRIP_SPACING * tracks.len().saturating_sub(1) as f32)
        + (MIXER_ROW_PADDING_X * 2.0);
    if viewport_width <= 0.0 || content_width <= viewport_width {
        return Some(tracks.into_iter().map(|track| track.name.clone()).collect());
    }

    let max_scroll = (content_width - viewport_width).max(0.0);
    let left_edge = (app.mixer_scroll_x.clamp(0.0, 1.0) * max_scroll - MIXER_OVERSCAN_PX).max(0.0);
    let right_edge = (left_edge + viewport_width + (MIXER_OVERSCAN_PX * 2.0)).min(content_width);
    let mut current_x = MIXER_ROW_PADDING_X;
    let mut visible = std::collections::HashSet::new();
    for track in tracks {
        let width = mixer_strip_width_for_channels(track.audio.outs);
        let strip_start = current_x;
        let strip_end = strip_start + width;
        if strip_end >= left_edge && strip_start <= right_edge {
            visible.insert(track.name.clone());
        }
        current_x = strip_end + MIXER_STRIP_SPACING;
    }
    Some(visible)
}

impl Maolan {
    pub(super) fn handle_response_freeze_meter_action(
        &mut self,
        action: &Action,
    ) -> Option<Task<Message>> {
        match action {
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
                        if let Err(err) = std::fs::remove_file(output_path) {
                            error!(
                                "Failed to remove canceled freeze output '{}': {err}",
                                output_path
                            );
                        }
                        self.state.blocking_write().message =
                            format!("Freeze canceled for '{}'", track_name);
                        return Some(Task::none());
                    }
                    {
                        let mut state = self.state.blocking_write();
                        if let Some(track_mut) =
                            state.tracks.iter_mut().find(|t| t.name == *track_name)
                        {
                            track_mut.frozen_audio_backup = pending.backup_audio.clone();
                            track_mut.frozen_midi_backup = pending.backup_midi.clone();
                            track_mut.frozen_render_clip = Some(pending.rendered_clip_rel.clone());
                            state.message = format!("Frozen track '{}'", track_name);
                        }
                    }
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
                    tasks.push(self.send(Action::TrackSetFrozen {
                        track_name: track_name.clone(),
                        frozen: true,
                    }));
                    tasks.push(self.send(Action::EndHistoryGroup));
                    return Some(Task::batch(tasks));
                }
                Some(Task::none())
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
                Some(Task::none())
            }
            Action::TrackOfflineBounceCanceled { track_name } => {
                self.freeze_in_progress = false;
                self.freeze_track_name = None;
                self.freeze_progress = 0.0;
                self.freeze_cancel_requested = false;
                self.pending_track_freeze_bounce.remove(track_name);
                self.state.blocking_write().message =
                    format!("Freeze canceled for '{}'", track_name);
                Some(Task::none())
            }
            Action::TrackMeters {
                track_name,
                output_db,
            } => {
                if track_name == "hw:out" {
                    let mut state = self.state.blocking_write();
                    Self::smooth_meter_db_levels(&mut state.hw_out_meter_db, output_db);
                    return Some(Task::none());
                }
                let mut state = self.state.blocking_write();
                if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name) {
                    Self::smooth_meter_db_levels(&mut track.meter_out_db, output_db);
                }
                Some(Task::none())
            }
            Action::MeterSnapshot {
                hw_out_db,
                track_meters,
            } => {
                let mut state = self.state.blocking_write();
                let visible_tracks = visible_mixer_track_names(self, &state);
                let has_input_monitor = state.tracks.iter().any(|track| track.input_monitor);
                let allow_live_hw_out = self.playing && (!self.paused || has_input_monitor);
                if (!allow_live_hw_out || hw_out_db.is_empty()) && !state.hw_out_meter_db.is_empty()
                {
                    let silence = vec![-90.0; state.hw_out_meter_db.len()];
                    Self::smooth_meter_db_levels(&mut state.hw_out_meter_db, &silence);
                } else {
                    Self::smooth_meter_db_levels(&mut state.hw_out_meter_db, hw_out_db);
                }
                for track in &mut state.tracks {
                    if let Some(visible_tracks) = visible_tracks.as_ref()
                        && track.name != crate::consts::state_ids::METRONOME_TRACK_ID
                        && !visible_tracks.contains(track.name.as_str())
                    {
                        continue;
                    }
                    let allow_live_track_out =
                        self.playing && (!self.paused || track.input_monitor);
                    if !allow_live_track_out || track_meters.is_empty() {
                        let silence = vec![-90.0; track.meter_out_db.len()];
                        Self::smooth_meter_db_levels(&mut track.meter_out_db, &silence);
                    } else if let Some((_, output_db)) = track_meters
                        .iter()
                        .find(|(track_name, _)| track_name.as_str() == track.name.as_str())
                    {
                        Self::smooth_meter_db_levels(&mut track.meter_out_db, output_db);
                    } else if !track.meter_out_db.is_empty() {
                        let silence = vec![-90.0; track.meter_out_db.len()];
                        Self::smooth_meter_db_levels(&mut track.meter_out_db, &silence);
                    }
                }
                Some(Task::none())
            }
            _ => None,
        }
    }
}
