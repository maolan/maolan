use crate::{
    kind::Kind,
    message::{Action, ClipMoveFrom, ClipMoveTo},
    state::State,
};
use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub struct UndoEntry {
    pub forward_action: Action,
    pub inverse_action: Action,
}

pub struct History {
    undo_stack: VecDeque<UndoEntry>,
    redo_stack: VecDeque<UndoEntry>,
    max_history: usize,
}

impl History {
    pub fn new(max_history: usize) -> Self {
        Self {
            undo_stack: VecDeque::new(),
            redo_stack: VecDeque::new(),
            max_history,
        }
    }

    pub fn record(&mut self, entry: UndoEntry) {
        self.undo_stack.push_back(entry);
        self.redo_stack.clear(); // Clear redo stack on new action

        // Limit history size
        if self.undo_stack.len() > self.max_history {
            self.undo_stack.pop_front();
        }
    }

    pub fn undo(&mut self) -> Option<Action> {
        self.undo_stack.pop_back().map(|entry| {
            let inverse = entry.inverse_action.clone();
            self.redo_stack.push_back(entry);
            inverse
        })
    }

    pub fn redo(&mut self) -> Option<Action> {
        self.redo_stack.pop_back().map(|entry| {
            let forward = entry.forward_action.clone();
            self.undo_stack.push_back(entry);
            forward
        })
    }

    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}

impl Default for History {
    fn default() -> Self {
        Self::new(100)
    }
}

/// Check if an action should be recorded in history
pub fn should_record(action: &Action) -> bool {
    matches!(
        action,
        Action::AddTrack { .. }
            | Action::RemoveTrack(_)
            | Action::RenameTrack { .. }
            | Action::TrackLevel(_, _)
            | Action::TrackBalance(_, _)
            | Action::TrackToggleArm(_)
            | Action::TrackToggleMute(_)
            | Action::TrackToggleSolo(_)
            | Action::TrackToggleInputMonitor(_)
            | Action::TrackToggleDiskMonitor(_)
            | Action::AddClip { .. }
            | Action::RemoveClip { .. }
            | Action::RenameClip { .. }
            | Action::ClipMove { .. }
            | Action::SetClipFade { .. }
            | Action::Connect { .. }
            | Action::Disconnect { .. }
            | Action::TrackLoadClapPlugin { .. }
            | Action::TrackUnloadClapPlugin { .. }
            | Action::TrackLoadVst3Plugin { .. }
            | Action::TrackUnloadVst3PluginInstance { .. }
            | Action::TrackSetClapParameter { .. }
            | Action::TrackSetVst3Parameter { .. }
            | Action::TrackClearDefaultPassthrough { .. }
            | Action::ModifyMidiNotes { .. }
            | Action::DeleteMidiNotes { .. }
            | Action::InsertMidiNotes { .. }
    )
}

/// Create an inverse action that will undo the given action
/// Returns None if the action cannot be inverted
pub fn create_inverse_action(action: &Action, state: &State) -> Option<Action> {
    match action {
        Action::AddTrack { name, .. } => Some(Action::RemoveTrack(name.clone())),

        Action::RemoveTrack(name) => {
            // Find the track to capture its data
            let track = state.tracks.get(name)?;
            let track_lock = track.lock();
            Some(Action::AddTrack {
                name: track_lock.name.clone(),
                audio_ins: track_lock.audio.ins.len(),
                midi_ins: track_lock.midi.ins.len(),
                audio_outs: track_lock.audio.outs.len(),
                midi_outs: track_lock.midi.outs.len(),
            })
        }

        Action::RenameTrack { old_name, new_name } => Some(Action::RenameTrack {
            old_name: new_name.clone(),
            new_name: old_name.clone(),
        }),

        Action::TrackLevel(name, _new_level) => {
            // Find current level
            let track = state.tracks.get(name)?;
            let track_lock = track.lock();
            Some(Action::TrackLevel(name.clone(), track_lock.level))
        }

        Action::TrackBalance(name, _new_balance) => {
            // Find current balance
            let track = state.tracks.get(name)?;
            let track_lock = track.lock();
            Some(Action::TrackBalance(name.clone(), track_lock.balance))
        }

        Action::TrackToggleArm(name) => Some(Action::TrackToggleArm(name.clone())),
        Action::TrackToggleMute(name) => Some(Action::TrackToggleMute(name.clone())),
        Action::TrackToggleSolo(name) => Some(Action::TrackToggleSolo(name.clone())),
        Action::TrackToggleInputMonitor(name) => {
            Some(Action::TrackToggleInputMonitor(name.clone()))
        }
        Action::TrackToggleDiskMonitor(name) => Some(Action::TrackToggleDiskMonitor(name.clone())),

        Action::AddClip { track_name, kind, .. } => {
            // To undo adding a clip, we need to know which index it will have
            let track = state.tracks.get(track_name)?;
            let track_lock = track.lock();
            let clip_index = match kind {
                Kind::Audio => track_lock.audio.clips.len(),
                Kind::MIDI => track_lock.midi.clips.len(),
            };
            Some(Action::RemoveClip {
                track_name: track_name.clone(),
                kind: *kind,
                clip_indices: vec![clip_index],
            })
        }

        Action::RemoveClip {
            track_name,
            kind,
            clip_indices,
        } => {
            // To undo removing clips, we need to capture their data
            let track = state.tracks.get(track_name)?;
            let track_lock = track.lock();

            // For now, we only support undoing single clip removal
            if clip_indices.len() != 1 {
                return None;
            }

            let clip_idx = clip_indices[0];
            match kind {
                Kind::Audio => {
                    let clip = track_lock.audio.clips.get(clip_idx)?;
                    let length = clip.end.saturating_sub(clip.start);
                    Some(Action::AddClip {
                        name: clip.name.clone(),
                        track_name: track_name.clone(),
                        start: clip.start,
                        length,
                        offset: clip.offset,
                        input_channel: clip.input_channel,
                        kind: Kind::Audio,
                        fade_enabled: clip.fade_enabled,
                        fade_in_samples: clip.fade_in_samples,
                        fade_out_samples: clip.fade_out_samples,
                    })
                }
                Kind::MIDI => {
                    let clip = track_lock.midi.clips.get(clip_idx)?;
                    let length = clip.end.saturating_sub(clip.start);
                    Some(Action::AddClip {
                        name: clip.name.clone(),
                        track_name: track_name.clone(),
                        start: clip.start,
                        length,
                        offset: clip.offset,
                        input_channel: clip.input_channel,
                        kind: Kind::MIDI,
                        fade_enabled: true,  // Default value for MIDI clips
                        fade_in_samples: 240,  // Default value
                        fade_out_samples: 240,  // Default value
                    })
                }
            }
        }

        Action::RenameClip {
            track_name,
            kind,
            clip_index,
            new_name: _,
        } => {
            // Find current name
            let track = state.tracks.get(track_name)?;
            let track_lock = track.lock();
            let old_name = match kind {
                Kind::Audio => track_lock.audio.clips.get(*clip_index)?.name.clone(),
                Kind::MIDI => track_lock.midi.clips.get(*clip_index)?.name.clone(),
            };
            Some(Action::RenameClip {
                track_name: track_name.clone(),
                kind: *kind,
                clip_index: *clip_index,
                new_name: old_name,
            })
        }

        Action::ClipMove { kind, from, to, copy } => {
            if *copy {
                // If it was a copy, we need to remove the newly created clip
                let dest_track = state.tracks.get(&to.track_name)?;
                let dest_lock = dest_track.lock();
                let clip_idx = match kind {
                    Kind::Audio => dest_lock.audio.clips.len(),
                    Kind::MIDI => dest_lock.midi.clips.len(),
                };
                Some(Action::RemoveClip {
                    track_name: to.track_name.clone(),
                    kind: *kind,
                    clip_indices: vec![clip_idx],
                })
            } else {
                // If it was a move, reverse the move
                let track = state.tracks.get(&from.track_name)?;
                let track_lock = track.lock();
                let (original_start, original_input_channel) = match kind {
                    Kind::Audio => {
                        let clip = track_lock.audio.clips.get(from.clip_index)?;
                        (clip.start, clip.input_channel)
                    }
                    Kind::MIDI => {
                        let clip = track_lock.midi.clips.get(from.clip_index)?;
                        (clip.start, clip.input_channel)
                    }
                };
                Some(Action::ClipMove {
                    kind: *kind,
                    from: ClipMoveFrom {
                        track_name: to.track_name.clone(),
                        clip_index: 0, // Will need to be adjusted
                    },
                    to: ClipMoveTo {
                        track_name: from.track_name.clone(),
                        sample_offset: original_start,
                        input_channel: original_input_channel,
                    },
                    copy: false,
                })
            }
        }

        Action::SetClipFade {
            track_name,
            clip_index,
            kind,
            ..
        } => {
            // Capture current fade settings
            let track = state.tracks.get(track_name)?;
            let track_lock = track.lock();
            match kind {
                Kind::Audio => {
                    let clip = track_lock.audio.clips.get(*clip_index)?;
                    Some(Action::SetClipFade {
                        track_name: track_name.clone(),
                        clip_index: *clip_index,
                        kind: *kind,
                        fade_enabled: clip.fade_enabled,
                        fade_in_samples: clip.fade_in_samples,
                        fade_out_samples: clip.fade_out_samples,
                    })
                }
                Kind::MIDI => {
                    // MIDI clips don't have fade fields in engine, use defaults
                    Some(Action::SetClipFade {
                        track_name: track_name.clone(),
                        clip_index: *clip_index,
                        kind: *kind,
                        fade_enabled: true,
                        fade_in_samples: 240,
                        fade_out_samples: 240,
                    })
                }
            }
        }

        Action::Connect {
            from_track,
            from_port,
            to_track,
            to_port,
            kind,
        } => Some(Action::Disconnect {
            from_track: from_track.clone(),
            from_port: *from_port,
            to_track: to_track.clone(),
            to_port: *to_port,
            kind: *kind,
        }),

        Action::Disconnect {
            from_track,
            from_port,
            to_track,
            to_port,
            kind,
        } => Some(Action::Connect {
            from_track: from_track.clone(),
            from_port: *from_port,
            to_track: to_track.clone(),
            to_port: *to_port,
            kind: *kind,
        }),

        Action::TrackLoadClapPlugin {
            track_name,
            plugin_path,
        } => Some(Action::TrackUnloadClapPlugin {
            track_name: track_name.clone(),
            plugin_path: plugin_path.clone(),
        }),

        Action::TrackUnloadClapPlugin {
            track_name,
            plugin_path,
        } => Some(Action::TrackLoadClapPlugin {
            track_name: track_name.clone(),
            plugin_path: plugin_path.clone(),
        }),

        Action::ModifyMidiNotes {
            track_name,
            clip_index,
            note_indices,
            new_notes,
            old_notes,
        } => Some(Action::ModifyMidiNotes {
            track_name: track_name.clone(),
            clip_index: *clip_index,
            note_indices: note_indices.clone(),
            new_notes: old_notes.clone(),
            old_notes: new_notes.clone(),
        }),

        Action::DeleteMidiNotes {
            track_name,
            clip_index,
            deleted_notes,
            ..
        } => Some(Action::InsertMidiNotes {
            track_name: track_name.clone(),
            clip_index: *clip_index,
            notes: deleted_notes.clone(),
        }),

        Action::InsertMidiNotes {
            track_name,
            clip_index,
            notes,
        } => {
            let mut note_indices: Vec<usize> = notes.iter().map(|(idx, _)| *idx).collect();
            note_indices.sort_unstable_by(|a, b| b.cmp(a));
            Some(Action::DeleteMidiNotes {
                track_name: track_name.clone(),
                clip_index: *clip_index,
                note_indices,
                deleted_notes: notes.clone(),
            })
        }

        // These are more complex and would need additional state tracking
        Action::TrackLoadVst3Plugin { .. } => None,
        Action::TrackUnloadVst3PluginInstance { .. } => None,
        Action::TrackSetClapParameter { .. } => None,
        Action::TrackSetVst3Parameter { .. } => None,

        _ => None,
    }
}
