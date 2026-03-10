use super::*;

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
                        let _ = std::fs::remove_file(output_path);
                        self.state.blocking_write().message =
                            format!("Freeze canceled for '{}'", track_name);
                        return Some(Task::none());
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
                            track_mut.frozen_render_clip = Some(pending.rendered_clip_rel.clone());
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
                Self::smooth_meter_db_levels(&mut state.hw_out_meter_db, hw_out_db);
                for (track_name, output_db) in track_meters.iter() {
                    if let Some(track) = state
                        .tracks
                        .iter_mut()
                        .find(|t| t.name == track_name.as_str())
                    {
                        Self::smooth_meter_db_levels(&mut track.meter_out_db, output_db);
                    }
                }
                Some(Task::none())
            }
            _ => None,
        }
    }
}
