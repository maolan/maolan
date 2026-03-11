use crate::{
    audio::io::AudioIO,
    kind::Kind,
    message::{Action, ClipMoveFrom, ClipMoveTo},
    midi::io::MIDIIO,
    state::State,
};
use std::collections::VecDeque;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct UndoEntry {
    pub forward_actions: Vec<Action>,
    pub inverse_actions: Vec<Action>,
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

    pub fn undo(&mut self) -> Option<Vec<Action>> {
        self.undo_stack.pop_back().map(|entry| {
            let inverse = entry.inverse_actions.clone();
            self.redo_stack.push_back(entry);
            inverse
        })
    }

    pub fn redo(&mut self) -> Option<Vec<Action>> {
        self.redo_stack.pop_back().map(|entry| {
            let forward = entry.forward_actions.clone();
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
        Action::SetTempo(_)
            | Action::SetLoopEnabled(_)
            | Action::SetLoopRange(_)
            | Action::SetPunchEnabled(_)
            | Action::SetPunchRange(_)
            | Action::SetMetronomeEnabled(_)
            | Action::SetTimeSignature { .. }
            | Action::SetClipPlaybackEnabled(_)
            | Action::SetRecordEnabled(_)
            | Action::AddTrack { .. }
            | Action::RemoveTrack(_)
            | Action::RenameTrack { .. }
            | Action::TrackLevel(_, _)
            | Action::TrackBalance(_, _)
            | Action::TrackToggleArm(_)
            | Action::TrackToggleMute(_)
            | Action::TrackToggleSolo(_)
            | Action::TrackToggleInputMonitor(_)
            | Action::TrackToggleDiskMonitor(_)
            | Action::TrackSetMidiLearnBinding { .. }
            | Action::SetGlobalMidiLearnBinding { .. }
            | Action::TrackSetVcaMaster { .. }
            | Action::TrackSetFrozen { .. }
            | Action::TrackAddAudioInput(_)
            | Action::TrackAddAudioOutput(_)
            | Action::TrackRemoveAudioInput(_)
            | Action::TrackRemoveAudioOutput(_)
            | Action::AddClip { .. }
            | Action::RemoveClip { .. }
            | Action::RenameClip { .. }
            | Action::ClipMove { .. }
            | Action::SetClipFade { .. }
            | Action::SetClipMuted { .. }
            | Action::SetAudioClipWarpMarkers { .. }
            | Action::ClearAllMidiLearnBindings
            | Action::Connect { .. }
            | Action::Disconnect { .. }
            | Action::TrackConnectVst3Audio { .. }
            | Action::TrackDisconnectVst3Audio { .. }
            | Action::TrackConnectPluginAudio { .. }
            | Action::TrackDisconnectPluginAudio { .. }
            | Action::TrackConnectPluginMidi { .. }
            | Action::TrackDisconnectPluginMidi { .. }
            | Action::TrackLoadClapPlugin { .. }
            | Action::TrackUnloadClapPlugin { .. }
            | Action::TrackLoadLv2Plugin { .. }
            | Action::TrackUnloadLv2PluginInstance { .. }
            | Action::TrackLoadVst3Plugin { .. }
            | Action::TrackUnloadVst3PluginInstance { .. }
            | Action::TrackSetLv2ControlValue { .. }
            | Action::TrackSetClapParameter { .. }
            | Action::TrackSetVst3Parameter { .. }
            | Action::ModifyMidiNotes { .. }
            | Action::ModifyMidiControllers { .. }
            | Action::DeleteMidiControllers { .. }
            | Action::InsertMidiControllers { .. }
            | Action::DeleteMidiNotes { .. }
            | Action::InsertMidiNotes { .. }
            | Action::SetMidiSysExEvents { .. }
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
                audio_ins: track_lock.primary_audio_ins(),
                midi_ins: track_lock.midi.ins.len(),
                audio_outs: track_lock.primary_audio_outs(),
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
        Action::TrackSetMidiLearnBinding {
            track_name, target, ..
        } => {
            let track = state.tracks.get(track_name)?;
            let track_lock = track.lock();
            let binding = match target {
                crate::message::TrackMidiLearnTarget::Volume => {
                    track_lock.midi_learn_volume.clone()
                }
                crate::message::TrackMidiLearnTarget::Balance => {
                    track_lock.midi_learn_balance.clone()
                }
                crate::message::TrackMidiLearnTarget::Mute => track_lock.midi_learn_mute.clone(),
                crate::message::TrackMidiLearnTarget::Solo => track_lock.midi_learn_solo.clone(),
                crate::message::TrackMidiLearnTarget::Arm => track_lock.midi_learn_arm.clone(),
                crate::message::TrackMidiLearnTarget::InputMonitor => {
                    track_lock.midi_learn_input_monitor.clone()
                }
                crate::message::TrackMidiLearnTarget::DiskMonitor => {
                    track_lock.midi_learn_disk_monitor.clone()
                }
            };
            Some(Action::TrackSetMidiLearnBinding {
                track_name: track_name.clone(),
                target: *target,
                binding,
            })
        }
        Action::TrackSetVcaMaster { track_name, .. } => {
            let track = state.tracks.get(track_name)?;
            let track_lock = track.lock();
            Some(Action::TrackSetVcaMaster {
                track_name: track_name.clone(),
                master_track: track_lock.vca_master(),
            })
        }
        Action::TrackSetFrozen { track_name, .. } => {
            let track = state.tracks.get(track_name)?;
            let track_lock = track.lock();
            Some(Action::TrackSetFrozen {
                track_name: track_name.clone(),
                frozen: track_lock.frozen(),
            })
        }
        Action::TrackAddAudioInput(name) => Some(Action::TrackRemoveAudioInput(name.clone())),
        Action::TrackAddAudioOutput(name) => Some(Action::TrackRemoveAudioOutput(name.clone())),
        Action::TrackRemoveAudioInput(name) => Some(Action::TrackAddAudioInput(name.clone())),
        Action::TrackRemoveAudioOutput(name) => Some(Action::TrackAddAudioOutput(name.clone())),

        Action::AddClip {
            track_name, kind, ..
        } => {
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
                        muted: clip.muted,
                        kind: Kind::Audio,
                        fade_enabled: clip.fade_enabled,
                        fade_in_samples: clip.fade_in_samples,
                        fade_out_samples: clip.fade_out_samples,
                        warp_markers: clip.warp_markers.clone(),
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
                        muted: clip.muted,
                        kind: Kind::MIDI,
                        fade_enabled: true,    // Default value for MIDI clips
                        fade_in_samples: 240,  // Default value
                        fade_out_samples: 240, // Default value
                        warp_markers: vec![],
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

        Action::ClipMove {
            kind,
            from,
            to,
            copy,
        } => {
            let (original_start, original_input_channel) = {
                let source_track = state.tracks.get(&from.track_name)?;
                let source_lock = source_track.lock();
                match kind {
                    Kind::Audio => {
                        let clip = source_lock.audio.clips.get(from.clip_index)?;
                        (clip.start, clip.input_channel)
                    }
                    Kind::MIDI => {
                        let clip = source_lock.midi.clips.get(from.clip_index)?;
                        (clip.start, clip.input_channel)
                    }
                }
            };

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
                // If it was a move, reverse the move from the destination track.
                let dest_track = state.tracks.get(&to.track_name)?;
                let dest_lock = dest_track.lock();
                let dest_len = match kind {
                    Kind::Audio => {
                        if dest_lock.audio.clips.is_empty() {
                            return None;
                        }
                        dest_lock.audio.clips.len()
                    }
                    Kind::MIDI => {
                        if dest_lock.midi.clips.is_empty() {
                            return None;
                        }
                        dest_lock.midi.clips.len()
                    }
                };
                let moved_clip_index = if from.track_name == to.track_name {
                    dest_len.saturating_sub(1)
                } else {
                    dest_len
                };
                Some(Action::ClipMove {
                    kind: *kind,
                    from: ClipMoveFrom {
                        track_name: to.track_name.clone(),
                        clip_index: moved_clip_index,
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
        Action::SetClipMuted {
            track_name,
            clip_index,
            kind,
            ..
        } => {
            let track = state.tracks.get(track_name)?;
            let track_lock = track.lock();
            let muted = match kind {
                Kind::Audio => track_lock.audio.clips.get(*clip_index)?.muted,
                Kind::MIDI => track_lock.midi.clips.get(*clip_index)?.muted,
            };
            Some(Action::SetClipMuted {
                track_name: track_name.clone(),
                clip_index: *clip_index,
                kind: *kind,
                muted,
            })
        }
        Action::SetAudioClipWarpMarkers {
            track_name,
            clip_index,
            ..
        } => {
            let track = state.tracks.get(track_name)?;
            let track_lock = track.lock();
            let clip = track_lock.audio.clips.get(*clip_index)?;
            Some(Action::SetAudioClipWarpMarkers {
                track_name: track_name.clone(),
                clip_index: *clip_index,
                warp_markers: clip.warp_markers.clone(),
            })
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
        Action::TrackConnectVst3Audio {
            track_name,
            from_node,
            from_port,
            to_node,
            to_port,
        } => Some(Action::TrackDisconnectVst3Audio {
            track_name: track_name.clone(),
            from_node: from_node.clone(),
            from_port: *from_port,
            to_node: to_node.clone(),
            to_port: *to_port,
        }),
        Action::TrackDisconnectVst3Audio {
            track_name,
            from_node,
            from_port,
            to_node,
            to_port,
        } => Some(Action::TrackConnectVst3Audio {
            track_name: track_name.clone(),
            from_node: from_node.clone(),
            from_port: *from_port,
            to_node: to_node.clone(),
            to_port: *to_port,
        }),
        Action::TrackConnectPluginAudio {
            track_name,
            from_node,
            from_port,
            to_node,
            to_port,
        } => Some(Action::TrackDisconnectPluginAudio {
            track_name: track_name.clone(),
            from_node: from_node.clone(),
            from_port: *from_port,
            to_node: to_node.clone(),
            to_port: *to_port,
        }),
        Action::TrackDisconnectPluginAudio {
            track_name,
            from_node,
            from_port,
            to_node,
            to_port,
        } => Some(Action::TrackConnectPluginAudio {
            track_name: track_name.clone(),
            from_node: from_node.clone(),
            from_port: *from_port,
            to_node: to_node.clone(),
            to_port: *to_port,
        }),
        Action::TrackConnectPluginMidi {
            track_name,
            from_node,
            from_port,
            to_node,
            to_port,
        } => Some(Action::TrackDisconnectPluginMidi {
            track_name: track_name.clone(),
            from_node: from_node.clone(),
            from_port: *from_port,
            to_node: to_node.clone(),
            to_port: *to_port,
        }),
        Action::TrackDisconnectPluginMidi {
            track_name,
            from_node,
            from_port,
            to_node,
            to_port,
        } => Some(Action::TrackConnectPluginMidi {
            track_name: track_name.clone(),
            from_node: from_node.clone(),
            from_port: *from_port,
            to_node: to_node.clone(),
            to_port: *to_port,
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
        Action::TrackLoadLv2Plugin {
            track_name,
            plugin_uri: _,
        } => {
            let track = state.tracks.get(track_name)?;
            let track = track.lock();
            Some(Action::TrackUnloadLv2PluginInstance {
                track_name: track_name.clone(),
                instance_id: track.next_lv2_instance_id,
            })
        }
        Action::TrackUnloadLv2PluginInstance {
            track_name,
            instance_id,
        } => {
            let track = state.tracks.get(track_name)?;
            let track = track.lock();
            let plugin_uri = track
                .loaded_lv2_instances()
                .into_iter()
                .find(|(id, _)| *id == *instance_id)
                .map(|(_, uri)| uri)?;
            Some(Action::TrackLoadLv2Plugin {
                track_name: track_name.clone(),
                plugin_uri,
            })
        }
        Action::TrackLoadVst3Plugin {
            track_name,
            plugin_path: _,
        } => {
            let track = state.tracks.get(track_name)?;
            let track = track.lock();
            Some(Action::TrackUnloadVst3PluginInstance {
                track_name: track_name.clone(),
                instance_id: track.next_plugin_instance_id,
            })
        }
        Action::TrackUnloadVst3PluginInstance {
            track_name,
            instance_id,
        } => {
            let track = state.tracks.get(track_name)?;
            let track = track.lock();
            let plugin_path = track
                .loaded_vst3_instances()
                .into_iter()
                .find(|(id, _, _)| *id == *instance_id)
                .map(|(_, path, _)| path)?;
            Some(Action::TrackLoadVst3Plugin {
                track_name: track_name.clone(),
                plugin_path,
            })
        }
        Action::TrackSetClapParameter {
            track_name,
            instance_id,
            ..
        } => {
            let track = state.tracks.get(track_name)?;
            let track = track.lock();
            let snapshot = track.clap_snapshot_state(*instance_id).ok()?;
            Some(Action::TrackClapRestoreState {
                track_name: track_name.clone(),
                instance_id: *instance_id,
                state: snapshot,
            })
        }
        Action::TrackSetVst3Parameter {
            track_name,
            instance_id,
            ..
        } => {
            let track = state.tracks.get(track_name)?;
            let track = track.lock();
            let snapshot = track.vst3_snapshot_state(*instance_id).ok()?;
            Some(Action::TrackVst3RestoreState {
                track_name: track_name.clone(),
                instance_id: *instance_id,
                state: snapshot,
            })
        }
        Action::TrackSetLv2ControlValue {
            track_name,
            instance_id,
            ..
        } => {
            let track = state.tracks.get(track_name)?;
            let track = track.lock();
            let snapshot = track.lv2_snapshot_state(*instance_id).ok()?;
            Some(Action::TrackSetLv2PluginState {
                track_name: track_name.clone(),
                instance_id: *instance_id,
                state: snapshot,
            })
        }
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
        Action::ModifyMidiControllers {
            track_name,
            clip_index,
            controller_indices,
            new_controllers,
            old_controllers,
        } => Some(Action::ModifyMidiControllers {
            track_name: track_name.clone(),
            clip_index: *clip_index,
            controller_indices: controller_indices.clone(),
            new_controllers: old_controllers.clone(),
            old_controllers: new_controllers.clone(),
        }),
        Action::DeleteMidiControllers {
            track_name,
            clip_index,
            deleted_controllers,
            ..
        } => Some(Action::InsertMidiControllers {
            track_name: track_name.clone(),
            clip_index: *clip_index,
            controllers: deleted_controllers.clone(),
        }),
        Action::InsertMidiControllers {
            track_name,
            clip_index,
            controllers,
        } => {
            let mut controller_indices: Vec<usize> =
                controllers.iter().map(|(idx, _)| *idx).collect();
            controller_indices.sort_unstable_by(|a, b| b.cmp(a));
            Some(Action::DeleteMidiControllers {
                track_name: track_name.clone(),
                clip_index: *clip_index,
                controller_indices,
                deleted_controllers: controllers.clone(),
            })
        }

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
        Action::SetMidiSysExEvents {
            track_name,
            clip_index,
            new_sysex_events,
            old_sysex_events,
        } => Some(Action::SetMidiSysExEvents {
            track_name: track_name.clone(),
            clip_index: *clip_index,
            new_sysex_events: old_sysex_events.clone(),
            old_sysex_events: new_sysex_events.clone(),
        }),

        // These are more complex and would need additional state tracking
        _ => None,
    }
}

pub fn create_inverse_actions(action: &Action, state: &State) -> Option<Vec<Action>> {
    if let Action::ClearAllMidiLearnBindings = action {
        let mut actions = Vec::<Action>::new();
        for (track_name, track) in &state.tracks {
            let t = track.lock();
            let mut push_if_some =
                |target: crate::message::TrackMidiLearnTarget,
                 binding: Option<crate::message::MidiLearnBinding>| {
                    if binding.is_some() {
                        actions.push(Action::TrackSetMidiLearnBinding {
                            track_name: track_name.clone(),
                            target,
                            binding,
                        });
                    }
                };
            push_if_some(
                crate::message::TrackMidiLearnTarget::Volume,
                t.midi_learn_volume.clone(),
            );
            push_if_some(
                crate::message::TrackMidiLearnTarget::Balance,
                t.midi_learn_balance.clone(),
            );
            push_if_some(
                crate::message::TrackMidiLearnTarget::Mute,
                t.midi_learn_mute.clone(),
            );
            push_if_some(
                crate::message::TrackMidiLearnTarget::Solo,
                t.midi_learn_solo.clone(),
            );
            push_if_some(
                crate::message::TrackMidiLearnTarget::Arm,
                t.midi_learn_arm.clone(),
            );
            push_if_some(
                crate::message::TrackMidiLearnTarget::InputMonitor,
                t.midi_learn_input_monitor.clone(),
            );
            push_if_some(
                crate::message::TrackMidiLearnTarget::DiskMonitor,
                t.midi_learn_disk_monitor.clone(),
            );
        }
        return Some(actions);
    }

    if let Action::RemoveTrack(track_name) = action {
        let mut actions = Vec::new();
        {
            let track = state.tracks.get(track_name)?;
            let track = track.lock();
            actions.push(Action::AddTrack {
                name: track.name.clone(),
                audio_ins: track.primary_audio_ins(),
                midi_ins: track.midi.ins.len(),
                audio_outs: track.primary_audio_outs(),
                midi_outs: track.midi.outs.len(),
            });
            for _ in track.primary_audio_ins()..track.audio.ins.len() {
                actions.push(Action::TrackAddAudioInput(track.name.clone()));
            }
            for _ in track.primary_audio_outs()..track.audio.outs.len() {
                actions.push(Action::TrackAddAudioOutput(track.name.clone()));
            }

            if track.level != 0.0 {
                actions.push(Action::TrackLevel(track.name.clone(), track.level));
            }
            if track.balance != 0.0 {
                actions.push(Action::TrackBalance(track.name.clone(), track.balance));
            }
            if track.armed {
                actions.push(Action::TrackToggleArm(track.name.clone()));
            }
            if track.muted {
                actions.push(Action::TrackToggleMute(track.name.clone()));
            }
            if track.soloed {
                actions.push(Action::TrackToggleSolo(track.name.clone()));
            }
            if track.input_monitor {
                actions.push(Action::TrackToggleInputMonitor(track.name.clone()));
            }
            if !track.disk_monitor {
                actions.push(Action::TrackToggleDiskMonitor(track.name.clone()));
            }
            if track.midi_learn_volume.is_some() {
                actions.push(Action::TrackSetMidiLearnBinding {
                    track_name: track.name.clone(),
                    target: crate::message::TrackMidiLearnTarget::Volume,
                    binding: track.midi_learn_volume.clone(),
                });
            }
            if track.midi_learn_balance.is_some() {
                actions.push(Action::TrackSetMidiLearnBinding {
                    track_name: track.name.clone(),
                    target: crate::message::TrackMidiLearnTarget::Balance,
                    binding: track.midi_learn_balance.clone(),
                });
            }
            if track.midi_learn_mute.is_some() {
                actions.push(Action::TrackSetMidiLearnBinding {
                    track_name: track.name.clone(),
                    target: crate::message::TrackMidiLearnTarget::Mute,
                    binding: track.midi_learn_mute.clone(),
                });
            }
            if track.midi_learn_solo.is_some() {
                actions.push(Action::TrackSetMidiLearnBinding {
                    track_name: track.name.clone(),
                    target: crate::message::TrackMidiLearnTarget::Solo,
                    binding: track.midi_learn_solo.clone(),
                });
            }
            if track.midi_learn_arm.is_some() {
                actions.push(Action::TrackSetMidiLearnBinding {
                    track_name: track.name.clone(),
                    target: crate::message::TrackMidiLearnTarget::Arm,
                    binding: track.midi_learn_arm.clone(),
                });
            }
            if track.midi_learn_input_monitor.is_some() {
                actions.push(Action::TrackSetMidiLearnBinding {
                    track_name: track.name.clone(),
                    target: crate::message::TrackMidiLearnTarget::InputMonitor,
                    binding: track.midi_learn_input_monitor.clone(),
                });
            }
            if track.midi_learn_disk_monitor.is_some() {
                actions.push(Action::TrackSetMidiLearnBinding {
                    track_name: track.name.clone(),
                    target: crate::message::TrackMidiLearnTarget::DiskMonitor,
                    binding: track.midi_learn_disk_monitor.clone(),
                });
            }
            if track.vca_master.is_some() {
                actions.push(Action::TrackSetVcaMaster {
                    track_name: track.name.clone(),
                    master_track: track.vca_master(),
                });
            }
            for (other_name, other_track_handle) in &state.tracks {
                if other_name == track_name {
                    continue;
                }
                let other_track = other_track_handle.lock();
                if other_track.vca_master.as_deref() == Some(track_name.as_str()) {
                    actions.push(Action::TrackSetVcaMaster {
                        track_name: other_name.clone(),
                        master_track: Some(track_name.clone()),
                    });
                }
            }

            for clip in &track.audio.clips {
                let length = clip.end.saturating_sub(clip.start).max(1);
                actions.push(Action::AddClip {
                    name: clip.name.clone(),
                    track_name: track.name.clone(),
                    start: clip.start,
                    length,
                    offset: clip.offset,
                    input_channel: clip.input_channel,
                    muted: clip.muted,
                    kind: Kind::Audio,
                    fade_enabled: clip.fade_enabled,
                    fade_in_samples: clip.fade_in_samples,
                    fade_out_samples: clip.fade_out_samples,
                    warp_markers: clip.warp_markers.clone(),
                });
            }
            for clip in &track.midi.clips {
                let length = clip.end.saturating_sub(clip.start).max(1);
                actions.push(Action::AddClip {
                    name: clip.name.clone(),
                    track_name: track.name.clone(),
                    start: clip.start,
                    length,
                    offset: clip.offset,
                    input_channel: clip.input_channel,
                    muted: clip.muted,
                    kind: Kind::MIDI,
                    fade_enabled: true,
                    fade_in_samples: 240,
                    fade_out_samples: 240,
                    warp_markers: vec![],
                });
            }
        }

        let mut seen_audio = std::collections::HashSet::<(String, usize, String, usize)>::new();
        let mut seen_midi = std::collections::HashSet::<(String, usize, String, usize)>::new();

        for (from_name, from_track_handle) in &state.tracks {
            let from_track = from_track_handle.lock();
            for (from_port, out) in from_track.audio.outs.iter().enumerate() {
                let conns: Vec<Arc<AudioIO>> = out.connections.lock().to_vec();
                for conn in conns {
                    for (to_name, to_track_handle) in &state.tracks {
                        let to_track = to_track_handle.lock();
                        for (to_port, to_in) in to_track.audio.ins.iter().enumerate() {
                            if Arc::ptr_eq(&conn, to_in)
                                && (from_name == track_name || to_name == track_name)
                                && seen_audio.insert((
                                    from_name.clone(),
                                    from_port,
                                    to_name.clone(),
                                    to_port,
                                ))
                            {
                                actions.push(Action::Connect {
                                    from_track: from_name.clone(),
                                    from_port,
                                    to_track: to_name.clone(),
                                    to_port,
                                    kind: Kind::Audio,
                                });
                            }
                        }
                    }
                }
            }

            for (from_port, out) in from_track.midi.outs.iter().enumerate() {
                let conns: Vec<Arc<crate::mutex::UnsafeMutex<Box<MIDIIO>>>> =
                    out.lock().connections.to_vec();
                for conn in conns {
                    for (to_name, to_track_handle) in &state.tracks {
                        let to_track = to_track_handle.lock();
                        for (to_port, to_in) in to_track.midi.ins.iter().enumerate() {
                            if Arc::ptr_eq(&conn, to_in)
                                && (from_name == track_name || to_name == track_name)
                                && seen_midi.insert((
                                    from_name.clone(),
                                    from_port,
                                    to_name.clone(),
                                    to_port,
                                ))
                            {
                                actions.push(Action::Connect {
                                    from_track: from_name.clone(),
                                    from_port,
                                    to_track: to_name.clone(),
                                    to_port,
                                    kind: Kind::MIDI,
                                });
                            }
                        }
                    }
                }
            }
        }

        for (to_name, to_track_handle) in &state.tracks {
            if to_name != track_name {
                continue;
            }
            let to_track = to_track_handle.lock();
            for (to_port, to_in) in to_track.audio.ins.iter().enumerate() {
                for (from_name, from_track_handle) in &state.tracks {
                    let from_track = from_track_handle.lock();
                    for (from_port, out) in from_track.audio.outs.iter().enumerate() {
                        let conns: Vec<Arc<AudioIO>> = out.connections.lock().to_vec();
                        if conns.iter().any(|conn| Arc::ptr_eq(conn, to_in))
                            && seen_audio.insert((
                                from_name.clone(),
                                from_port,
                                to_name.clone(),
                                to_port,
                            ))
                        {
                            actions.push(Action::Connect {
                                from_track: from_name.clone(),
                                from_port,
                                to_track: to_name.clone(),
                                to_port,
                                kind: Kind::Audio,
                            });
                        }
                    }
                }
            }
            for (to_port, to_in) in to_track.midi.ins.iter().enumerate() {
                for (from_name, from_track_handle) in &state.tracks {
                    let from_track = from_track_handle.lock();
                    for (from_port, out) in from_track.midi.outs.iter().enumerate() {
                        let conns: Vec<Arc<crate::mutex::UnsafeMutex<Box<MIDIIO>>>> =
                            out.lock().connections.to_vec();
                        if conns.iter().any(|conn| Arc::ptr_eq(conn, to_in))
                            && seen_midi.insert((
                                from_name.clone(),
                                from_port,
                                to_name.clone(),
                                to_port,
                            ))
                        {
                            actions.push(Action::Connect {
                                from_track: from_name.clone(),
                                from_port,
                                to_track: to_name.clone(),
                                to_port,
                                kind: Kind::MIDI,
                            });
                        }
                    }
                }
            }
        }

        return Some(actions);
    }

    create_inverse_action(action, state).map(|a| vec![a])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::clip::AudioClip;
    use crate::kind::Kind;
    use crate::message::{MidiLearnBinding, TrackMidiLearnTarget};
    use crate::mutex::UnsafeMutex;
    use crate::track::Track;
    #[cfg(all(unix, not(target_os = "macos")))]
    use crate::message::Lv2PluginState;
    use crate::vst3::Vst3PluginState;
    use std::sync::Arc;

    fn make_state_with_track(track: Track) -> State {
        let mut state = State::default();
        state.tracks.insert(
            track.name.clone(),
            Arc::new(UnsafeMutex::new(Box::new(track))),
        );
        state
    }

    fn binding(cc: u8) -> MidiLearnBinding {
        MidiLearnBinding {
            device: Some("midi".to_string()),
            channel: 1,
            cc,
        }
    }

    #[test]
    fn history_record_limits_size_and_clears_redo_on_new_entry() {
        let mut history = History::new(2);
        let a = UndoEntry {
            forward_actions: vec![Action::SetTempo(120.0)],
            inverse_actions: vec![Action::SetTempo(110.0)],
        };
        let b = UndoEntry {
            forward_actions: vec![Action::SetLoopEnabled(true)],
            inverse_actions: vec![Action::SetLoopEnabled(false)],
        };
        let c = UndoEntry {
            forward_actions: vec![Action::SetMetronomeEnabled(true)],
            inverse_actions: vec![Action::SetMetronomeEnabled(false)],
        };

        history.record(a);
        history.record(b.clone());
        history.record(c.clone());

        let undo = history.undo().unwrap();
        assert!(matches!(undo.as_slice(), [Action::SetMetronomeEnabled(false)]));

        let redo = history.redo().unwrap();
        assert!(matches!(redo.as_slice(), [Action::SetMetronomeEnabled(true)]));

        history.undo();
        history.record(UndoEntry {
            forward_actions: vec![Action::SetClipPlaybackEnabled(true)],
            inverse_actions: vec![Action::SetClipPlaybackEnabled(false)],
        });

        assert!(history.redo().is_none());
        let undo = history.undo().unwrap();
        assert!(matches!(
            undo.as_slice(),
            [Action::SetClipPlaybackEnabled(false)]
        ));
        let undo = history.undo().unwrap();
        assert!(matches!(undo.as_slice(), [Action::SetLoopEnabled(false)]));
        assert!(history.undo().is_none());
    }

    #[test]
    fn should_record_covers_recent_transport_and_lv2_actions() {
        assert!(should_record(&Action::SetLoopEnabled(true)));
        assert!(should_record(&Action::SetLoopRange(Some((64, 128)))));
        assert!(should_record(&Action::SetPunchEnabled(true)));
        assert!(should_record(&Action::SetPunchRange(Some((32, 96)))));
        assert!(should_record(&Action::SetMetronomeEnabled(true)));
        assert!(should_record(&Action::SetClipPlaybackEnabled(false)));
        assert!(should_record(&Action::SetRecordEnabled(true)));
        assert!(should_record(&Action::TrackLoadVst3Plugin {
            track_name: "t".to_string(),
            plugin_path: "/tmp/test.vst3".to_string(),
        }));
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            assert!(should_record(&Action::TrackLoadLv2Plugin {
                track_name: "t".to_string(),
                plugin_uri: "urn:test".to_string(),
            }));
            assert!(should_record(&Action::TrackSetLv2ControlValue {
                track_name: "t".to_string(),
                instance_id: 0,
                index: 1,
                value: 0.5,
            }));
            assert!(!should_record(&Action::TrackSetLv2PluginState {
                track_name: "t".to_string(),
                instance_id: 0,
                state: Lv2PluginState {
                    port_values: vec![],
                    properties: vec![],
                },
            }));
        }
        assert!(!should_record(&Action::TrackVst3RestoreState {
            track_name: "t".to_string(),
            instance_id: 0,
            state: Vst3PluginState {
                plugin_id: "id".to_string(),
                component_state: vec![],
                controller_state: vec![],
            },
        }));
    }

    #[test]
    fn create_inverse_action_for_add_clip_targets_next_clip_index() {
        let mut track = Track::new("t".to_string(), 1, 1, 0, 0, 64, 48_000.0);
        track.audio.clips.push(AudioClip::new("existing".to_string(), 0, 16));
        let state = make_state_with_track(track);

        let inverse = create_inverse_action(
            &Action::AddClip {
                name: "new".to_string(),
                track_name: "t".to_string(),
                start: 32,
                length: 16,
                offset: 0,
                input_channel: 0,
                muted: false,
                kind: Kind::Audio,
                fade_enabled: false,
                fade_in_samples: 0,
                fade_out_samples: 0,
                warp_markers: vec![],
            },
            &state,
        )
        .unwrap();

        match inverse {
            Action::RemoveClip {
                track_name,
                kind,
                clip_indices,
            } => {
                assert_eq!(track_name, "t");
                assert_eq!(kind, Kind::Audio);
                assert_eq!(clip_indices, vec![1]);
            }
            other => panic!("unexpected inverse action: {other:?}"),
        }
    }

    #[test]
    fn create_inverse_action_for_track_midi_binding_restores_previous_binding() {
        let mut track = Track::new("t".to_string(), 1, 1, 0, 0, 64, 48_000.0);
        track.midi_learn_volume = Some(binding(7));
        let state = make_state_with_track(track);

        let inverse = create_inverse_action(
            &Action::TrackSetMidiLearnBinding {
                track_name: "t".to_string(),
                target: TrackMidiLearnTarget::Volume,
                binding: Some(binding(9)),
            },
            &state,
        )
        .unwrap();

        match inverse {
            Action::TrackSetMidiLearnBinding {
                track_name,
                target,
                binding,
            } => {
                assert_eq!(track_name, "t");
                assert_eq!(target, TrackMidiLearnTarget::Volume);
                assert_eq!(binding.unwrap().cc, 7);
            }
            other => panic!("unexpected inverse action: {other:?}"),
        }
    }

    #[test]
    fn create_inverse_action_for_vst3_load_uses_next_runtime_instance_id() {
        let mut track = Track::new("t".to_string(), 1, 1, 0, 0, 64, 48_000.0);
        track.next_plugin_instance_id = 42;
        let state = make_state_with_track(track);

        let inverse = create_inverse_action(
            &Action::TrackLoadVst3Plugin {
                track_name: "t".to_string(),
                plugin_path: "/tmp/test.vst3".to_string(),
            },
            &state,
        )
        .unwrap();

        match inverse {
            Action::TrackUnloadVst3PluginInstance {
                track_name,
                instance_id,
            } => {
                assert_eq!(track_name, "t");
                assert_eq!(instance_id, 42);
            }
            other => panic!("unexpected inverse action: {other:?}"),
        }
    }

    #[test]
    #[cfg(all(unix, not(target_os = "macos")))]
    fn create_inverse_action_for_lv2_load_uses_next_runtime_instance_id() {
        let mut track = Track::new("t".to_string(), 1, 1, 0, 0, 64, 48_000.0);
        track.next_lv2_instance_id = 5;
        let state = make_state_with_track(track);

        let inverse = create_inverse_action(
            &Action::TrackLoadLv2Plugin {
                track_name: "t".to_string(),
                plugin_uri: "urn:test".to_string(),
            },
            &state,
        )
        .unwrap();

        match inverse {
            Action::TrackUnloadLv2PluginInstance {
                track_name,
                instance_id,
            } => {
                assert_eq!(track_name, "t");
                assert_eq!(instance_id, 5);
            }
            other => panic!("unexpected inverse action: {other:?}"),
        }
    }

    #[test]
    fn create_inverse_actions_for_clear_all_midi_learn_bindings_restores_only_existing_bindings() {
        let mut track = Track::new("t".to_string(), 1, 1, 0, 0, 64, 48_000.0);
        track.midi_learn_volume = Some(binding(7));
        track.midi_learn_disk_monitor = Some(binding(64));
        let state = make_state_with_track(track);

        let inverses = create_inverse_actions(&Action::ClearAllMidiLearnBindings, &state).unwrap();

        assert_eq!(inverses.len(), 2);
        assert!(inverses.iter().any(|action| {
            matches!(
                action,
                Action::TrackSetMidiLearnBinding {
                    target: TrackMidiLearnTarget::Volume,
                    binding: Some(MidiLearnBinding { cc: 7, .. }),
                    ..
                }
            )
        }));
        assert!(inverses.iter().any(|action| {
            matches!(
                action,
                Action::TrackSetMidiLearnBinding {
                    target: TrackMidiLearnTarget::DiskMonitor,
                    binding: Some(MidiLearnBinding { cc: 64, .. }),
                    ..
                }
            )
        }));
    }

    #[test]
    fn create_inverse_actions_for_remove_track_restores_io_flags_and_bindings() {
        let mut track = Track::new("t".to_string(), 1, 1, 1, 1, 64, 48_000.0);
        track.level = -3.0;
        track.balance = 0.25;
        track.armed = true;
        track.muted = true;
        track.soloed = true;
        track.input_monitor = true;
        track.disk_monitor = false;
        track.midi_learn_volume = Some(binding(10));
        track.vca_master = Some("bus".to_string());
        track.audio.ins.push(Arc::new(AudioIO::new(64)));
        track.audio.outs.push(Arc::new(AudioIO::new(64)));
        let state = make_state_with_track(track);

        let inverses = create_inverse_actions(&Action::RemoveTrack("t".to_string()), &state).unwrap();

        assert!(matches!(
            inverses.first(),
            Some(Action::AddTrack {
                name,
                audio_ins: 1,
                audio_outs: 1,
                midi_ins: 1,
                midi_outs: 1,
            }) if name == "t"
        ));
        assert!(inverses
            .iter()
            .any(|action| matches!(action, Action::TrackAddAudioInput(name) if name == "t")));
        assert!(inverses
            .iter()
            .any(|action| matches!(action, Action::TrackAddAudioOutput(name) if name == "t")));
        assert!(inverses
            .iter()
            .any(|action| matches!(action, Action::TrackToggleInputMonitor(name) if name == "t")));
        assert!(inverses
            .iter()
            .any(|action| matches!(action, Action::TrackToggleDiskMonitor(name) if name == "t")));
        assert!(inverses.iter().any(|action| {
            matches!(
                action,
                Action::TrackSetMidiLearnBinding {
                    target: TrackMidiLearnTarget::Volume,
                    binding: Some(MidiLearnBinding { cc: 10, .. }),
                    ..
                }
            )
        }));
        assert!(inverses.iter().any(|action| {
            matches!(
                action,
                Action::TrackSetVcaMaster {
                    track_name,
                    master_track: Some(master),
                } if track_name == "t" && master == "bus"
            )
        }));
    }
}
