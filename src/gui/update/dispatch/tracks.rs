use super::*;

#[derive(Clone, Copy)]
enum TrackMonitorKind {
    Disk,
    Input,
    MidiDisk,
    MidiInput,
}

impl Maolan {
    fn toggle_track_monitor(&mut self, track_name: &str, kind: TrackMonitorKind) -> Task<Message> {
        let actions: Vec<Action> = {
            let state = self.state.read().expect("state lock poisoned");
            let Some(track) = state.tracks.iter().find(|t| t.name == track_name) else {
                return Task::none();
            };
            let current: &[bool] = match kind {
                TrackMonitorKind::Disk => &track.disk_monitor,
                TrackMonitorKind::Input => &track.input_monitor,
                TrackMonitorKind::MidiDisk => &track.midi_disk_monitor,
                TrackMonitorKind::MidiInput => &track.midi_input_monitor,
            };
            let desired = !current.first().copied().unwrap_or(false);
            current
                .iter()
                .enumerate()
                .filter(|(_, monitor)| **monitor != desired)
                .map(|(lane, _)| {
                    let track_name = track_name.to_string();
                    match kind {
                        TrackMonitorKind::Disk => {
                            Action::TrackToggleDiskMonitor { track_name, lane }
                        }
                        TrackMonitorKind::Input => {
                            Action::TrackToggleInputMonitor { track_name, lane }
                        }
                        TrackMonitorKind::MidiDisk => {
                            Action::TrackToggleMidiDiskMonitor { track_name, lane }
                        }
                        TrackMonitorKind::MidiInput => {
                            Action::TrackToggleMidiInputMonitor { track_name, lane }
                        }
                    }
                })
                .collect()
        };
        let mut task = Task::none();
        for action in actions {
            task = task.chain(self.send(action));
        }
        task
    }

    pub(super) fn handle_tracks_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::TrackToggleDiskMonitor { ref track_name } => {
                return self.toggle_track_monitor(track_name, TrackMonitorKind::Disk);
            }
            Message::TrackToggleInputMonitor { ref track_name } => {
                return self.toggle_track_monitor(track_name, TrackMonitorKind::Input);
            }
            Message::TrackToggleMidiDiskMonitor { ref track_name } => {
                return self.toggle_track_monitor(track_name, TrackMonitorKind::MidiDisk);
            }
            Message::TrackToggleMidiInputMonitor { ref track_name } => {
                return self.toggle_track_monitor(track_name, TrackMonitorKind::MidiInput);
            }
            Message::AddTrack(crate::message::AddTrack::TrackType(track_type)) => {
                self.add_track.update(&message);
                return if track_type == crate::message::AddTrackType::MixOsc {
                    Task::perform(
                        async { mixosc::DiscoveryProbe::new().discover().unwrap_or_default() },
                        |mixers| {
                            Message::AddTrack(crate::message::AddTrack::MixersDiscovered(mixers))
                        },
                    )
                } else {
                    Task::none()
                };
            }
            Message::AddTrack(crate::message::AddTrack::MixerSelected(_))
            | Message::AddTrack(crate::message::AddTrack::MixersDiscovered(_)) => {
                self.add_track.update(&message);
                return Task::none();
            }
            Message::TrackToggleFolder { ref track_name } => {
                {
                    let mut state = self.state.write().expect("state lock poisoned");
                    if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name) {
                        track.folder_open = !track.folder_open;
                    }
                }
                return self.send(Action::TrackToggleFolder {
                    track_name: track_name.clone(),
                });
            }
            Message::TrackSetFolder {
                ref track_name,
                is_folder,
            } => {
                if is_folder {
                    let is_master = self
                        .state
                        .read()
                        .expect("state lock poisoned")
                        .tracks
                        .iter()
                        .find(|t| t.name == *track_name)
                        .is_some_and(|t| t.is_master);
                    if is_master {
                        self.state.write().expect("state lock poisoned").message = format!(
                            "Track '{}' is the master track and cannot be made a folder",
                            track_name
                        );
                        return Task::none();
                    }
                }
                let mut child_tasks = vec![];
                let mut clear_audio_indices = Vec::new();
                let mut clear_midi_indices = Vec::new();
                {
                    let mut state = self.state.write().expect("state lock poisoned");
                    if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name) {
                        let was_folder = track.is_folder;
                        track.is_folder = is_folder;
                        if is_folder {
                            track.folder_open = true;
                            if !was_folder {
                                clear_audio_indices = (0..track.audio.clips.len()).collect();
                                clear_midi_indices = (0..track.midi.clips.len()).collect();
                                track.audio.clips.clear();
                                track.midi.clips.clear();
                                track.frozen_audio_backup.clear();
                                track.frozen_midi_backup.clear();
                                track.frozen_render_clip = None;
                                track.frozen = false;
                            }
                        } else if was_folder {
                            let children: Vec<String> = state
                                .tracks
                                .iter()
                                .filter(|t| t.parent_track.as_deref() == Some(track_name.as_str()))
                                .map(|t| t.name.clone())
                                .collect();
                            for child_name in children {
                                if let Some(child) =
                                    state.tracks.iter_mut().find(|t| t.name == child_name)
                                {
                                    child.parent_track = None;
                                }
                                child_tasks.push(self.send(Action::TrackSetParent {
                                    track_name: child_name,
                                    parent_name: None,
                                }));
                            }
                        }
                    }
                }
                if !clear_audio_indices.is_empty() {
                    child_tasks.push(self.send(Action::RemoveClip {
                        track_name: track_name.clone(),
                        kind: Kind::Audio,
                        clip_indices: clear_audio_indices,
                    }));
                }
                if !clear_midi_indices.is_empty() {
                    child_tasks.push(self.send(Action::RemoveClip {
                        track_name: track_name.clone(),
                        kind: Kind::MIDI,
                        clip_indices: clear_midi_indices,
                    }));
                }
                child_tasks.push(self.send(Action::TrackSetFolder {
                    track_name: track_name.clone(),
                    is_folder,
                }));
                return Task::batch(child_tasks);
            }
            Message::TrackSetParent {
                ref track_name,
                ref parent_name,
            } => {
                let mut state = self.state.write().expect("state lock poisoned");

                if let Some(parent) = parent_name {
                    if parent == track_name {
                        state.message = "Track cannot be its own parent".to_string();
                        return Task::none();
                    }

                    let mut current = parent.as_str();
                    loop {
                        if current == track_name.as_str() {
                            state.message = "Cannot create circular folder hierarchy".to_string();
                            return Task::none();
                        }
                        if let Some(next) = state
                            .tracks
                            .iter()
                            .find(|t| t.name == current)
                            .and_then(|t| t.parent_track.as_deref())
                        {
                            current = next;
                        } else {
                            break;
                        }
                    }

                    // The master track cannot be made part of a folder.
                    if state
                        .tracks
                        .iter()
                        .find(|t| t.name == *track_name)
                        .is_some_and(|t| t.is_master)
                    {
                        state.message = format!(
                            "Track '{}' is the master track and cannot be made part of a folder",
                            track_name
                        );
                        return Task::none();
                    }
                }
                if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name) {
                    track.parent_track = parent_name.clone();
                }
                drop(state);
                return self.send(Action::TrackSetParent {
                    track_name: track_name.clone(),
                    parent_name: parent_name.clone(),
                });
            }
            Message::TrackMidiLaneChannelSelected {
                ref track_name,
                lane,
                channel,
            } => {
                return self.send(Action::TrackSetMidiLaneChannel {
                    track_name: track_name.clone(),
                    lane,
                    channel: channel.to_engine(),
                });
            }
            Message::TrackSetupToggle(ref track_name) => {
                let mut state = self.state.write().expect("state lock poisoned");
                let mut opened_setup = false;
                if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name)
                    && !track.is_folder
                {
                    track.setup_open = !track.setup_open;
                    opened_setup = track.setup_open;
                }
                if opened_setup {
                    let current_width = match state.tracks_width {
                        Length::Fixed(width) => width,
                        _ => 0.0,
                    };
                    if current_width < TRACK_SETUP_MIN_TRACKS_WIDTH {
                        state.tracks_width = Length::Fixed(TRACK_SETUP_MIN_TRACKS_WIDTH);
                    }
                }
            }
            Message::TrackMidiSetupChannelSelected {
                ref track_name,
                channel,
            } => {
                let engine_channel = channel.to_engine();
                {
                    let mut state = self.state.write().expect("state lock poisoned");
                    if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name) {
                        for lane_channel in track.midi_lane_channels.iter_mut() {
                            *lane_channel = engine_channel;
                        }
                    }
                }
                return self.send(Action::TrackSetMidiLaneChannel {
                    track_name: track_name.clone(),
                    lane: 0,
                    channel: engine_channel,
                });
            }
            Message::MpeConfigShow { ref track_name } => {
                self.state
                    .write()
                    .expect("state lock poisoned")
                    .track_context_menu = None;
                return Task::done(Message::Show(Show::MpeConfig {
                    track_name: track_name.clone(),
                }));
            }
            Message::MpeConfigSetZone {
                ref track_name,
                manager_channel,
                member_count,
            } => {
                {
                    let mut state = self.state.write().expect("state lock poisoned");
                    if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name) {
                        track
                            .mpe_state
                            .configure_zone(manager_channel, member_count);
                    }
                }
                return self.send(Action::TrackSetMpeZone {
                    track_name: track_name.clone(),
                    manager_channel,
                    member_count,
                });
            }
            Message::MpeConfigSetPitchBendSensitivity {
                ref track_name,
                channel,
                semitones,
            } => {
                {
                    let mut state = self.state.write().expect("state lock poisoned");
                    if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name) {
                        track
                            .mpe_state
                            .set_pitch_bend_sensitivity(channel, semitones);
                    }
                }
                return self.send(Action::TrackSetMpePitchBendSensitivity {
                    track_name: track_name.clone(),
                    channel,
                    semitones,
                });
            }
            Message::TrackAddReturn(ref track_name) => {
                self.state
                    .write()
                    .expect("state lock poisoned")
                    .track_context_menu = None;
                return self.send(Action::TrackAddAudioInput(track_name.clone()));
            }
            Message::TrackAddSend(ref track_name) => {
                self.state
                    .write()
                    .expect("state lock poisoned")
                    .track_context_menu = None;
                return self.send(Action::TrackAddAudioOutput(track_name.clone()));
            }
            Message::TrackMidiLearnArm {
                ref track_name,
                target,
            } => {
                self.state.write().expect("state lock poisoned").message = format!(
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
                self.state.write().expect("state lock poisoned").message = format!(
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
            Message::SessionMidiLearnArm { target } => {
                self.state.write().expect("state lock poisoned").message = format!(
                    "Session MIDI learn armed for {:?}. Move a hardware MIDI CC control.",
                    target
                );
                return self.send(Action::SessionArmMidiLearn { target });
            }
            Message::SessionMidiLearnClear { target } => {
                return self.send(Action::SetSessionMidiLearnBinding {
                    target,
                    binding: None,
                });
            }
            Message::TrackColorChanged {
                ref track_name,
                color,
            } => {
                let engine_color = color.map(|c| maolan_engine::message::TrackColor {
                    r: c.r,
                    g: c.g,
                    b: c.b,
                    a: c.a,
                });
                return self.send(maolan_engine::message::Action::TrackSetColor {
                    track_name: track_name.clone(),
                    color: engine_color,
                });
            }
            Message::TrackColorClear(ref track_name) => {
                return self.send(maolan_engine::message::Action::TrackSetColor {
                    track_name: track_name.clone(),
                    color: None,
                });
            }
            Message::RemoveSelectedTracks => {
                let (selected, viewed) = {
                    let state = self.state.read().expect("state lock poisoned");
                    let viewed = state
                        .plugin_graph_clip
                        .is_none()
                        .then(|| state.plugin_graph_track.clone())
                        .flatten();
                    (state.selected.clone(), viewed)
                };
                let mut removed_viewed = false;
                let mut actions = vec![Action::BeginHistoryGroup];
                for name in &selected {
                    if viewed.as_deref() == Some(name.as_str()) {
                        removed_viewed = true;
                        continue;
                    }
                    actions.push(Action::RemoveTrack(name.clone()));
                }
                if removed_viewed && let Some(viewed) = viewed {
                    self.state.write().expect("state lock poisoned").message = format!(
                        "Cannot delete '{}' while its connections are being viewed",
                        viewed
                    );
                }
                if actions.len() > 1 {
                    actions.push(Action::EndHistoryGroup);
                    return Self::restore_actions_task(actions);
                }
            }
            Message::TrackRenameShow(ref track_name) => {
                self.track_rename.open(crate::state::TrackRenameDialog {
                    old_name: track_name.clone(),
                    new_name: track_name.clone(),
                });
            }
            Message::TrackRenameInput(_) => {}
            Message::TemplateSaveInput(_) => {
                self.template_save.update(&message);
            }
            Message::TrackRenameConfirm => {
                let dialog = self.track_rename.dialog.clone();
                let Some(dialog) = dialog else {
                    return Task::none();
                };

                let new_name = dialog.new_name.trim().to_string();
                if new_name.is_empty() || new_name == dialog.old_name {
                    return Task::none();
                }

                self.track_rename.close();

                return self.send(Action::RenameTrack {
                    old_name: dialog.old_name,
                    new_name,
                });
            }
            Message::TrackRenameCancel => {
                self.track_rename.close();
            }
            Message::TrackTemplateSaveShow(ref track_name) => {
                self.track_template_save
                    .open(crate::state::TrackTemplateSaveDialog {
                        track_name: track_name.clone(),
                        name: String::new(),
                    });
                self.modal = Some(Show::SaveTemplateAs);
            }
            Message::TrackTemplateSaveInput(_) => {
                self.track_template_save.update(&message);
            }
            Message::TrackTemplateSaveConfirm => {
                let dialog = self.track_template_save.dialog.clone();
                let Some(dialog) = dialog else {
                    return Task::none();
                };

                let name = dialog.name.trim().to_string();
                if name.is_empty() {
                    return Task::none();
                }

                self.track_template_save.close();
                self.modal = None;

                let template_path = crate::config::daw_config_dir()
                    .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"))
                    .join("track_templates")
                    .join(&name);
                let template_path = template_path.to_string_lossy().into_owned();

                return self
                    .refresh_graph_then_save_track_template(dialog.track_name, template_path);
            }
            Message::TrackTemplateSaveCancel => {
                self.track_template_save.close();
                self.modal = None;
            }
            Message::TrackContextMenuHover {
                ref track_name,
                position,
            } => {
                let mut state = self.state.write().expect("state lock poisoned");
                state.track_context_hover = Some((track_name.clone(), position));
            }
            Message::TrackContextMenuSubmenuOpen(ref submenu) => {
                let track_name = {
                    let mut state = self.state.write().expect("state lock poisoned");
                    if let Some(menu) = &mut state.track_context_menu {
                        menu.submenu = Some(submenu.clone());
                        Some(menu.track_name.clone())
                    } else {
                        None
                    }
                };
                if let Some(track_name) = track_name {
                    let state = self.state.read().expect("state lock poisoned");
                    let plugins = state
                        .plugin_graphs_by_track
                        .get(&track_name)
                        .map(|(plugins, _)| plugins.clone())
                        .unwrap_or_default();
                    drop(state);
                    let mut tasks = Vec::new();
                    match submenu {
                        crate::state::TrackContextSubmenu::Automation
                        | crate::state::TrackContextSubmenu::Plugin { .. } => {
                            for plugin in &plugins {
                                if plugin.format.eq_ignore_ascii_case("CLAP") {
                                    tasks.push(self.send(Action::TrackGetClapParameters {
                                        track_name: track_name.clone(),
                                        instance_id: plugin.instance_id,
                                    }));
                                } else if plugin.format.eq_ignore_ascii_case("VST3") {
                                    tasks.push(self.send(Action::TrackGetVst3Parameters {
                                        track_name: track_name.clone(),
                                        instance_id: plugin.instance_id,
                                    }));
                                }
                            }
                        }
                        _ => {}
                    }
                    if !tasks.is_empty() {
                        return Task::batch(tasks);
                    }
                }
            }
            Message::TrackContextMenuSubmenuClose => {
                let mut state = self.state.write().expect("state lock poisoned");
                if let Some(menu) = &mut state.track_context_menu {
                    menu.submenu = None;
                }
            }
            Message::TrackContextMenuToggle(ref track_name) => {
                let mut state = self.state.write().expect("state lock poisoned");
                if state
                    .track_context_menu
                    .as_ref()
                    .is_some_and(|menu| menu.track_name == *track_name)
                {
                    state.track_context_menu = None;
                } else {
                    let anchor = state
                        .track_context_hover
                        .as_ref()
                        .filter(|(hover_track, _)| hover_track == track_name)
                        .map(|(_, point)| *point)
                        .unwrap_or(Point::new(8.0, 24.0));
                    state.track_context_menu = Some(crate::state::TrackContextMenuState {
                        track_name: track_name.clone(),
                        anchor,
                        submenu: None,
                    });
                }
                state.clip_context_menu = None;
                state.clip_click_consumed = true;
            }
            Message::TemplateSaveConfirm => {
                let dialog = self.template_save.dialog.clone();
                let Some(dialog) = dialog else {
                    return Task::none();
                };

                let name = dialog.name.trim().to_string();
                if name.is_empty() {
                    return Task::none();
                }

                self.template_save.close();
                self.modal = None;

                let template_path = crate::config::daw_config_dir()
                    .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"))
                    .join("session_templates")
                    .join(&name);
                let template_path = template_path.to_string_lossy().into_owned();

                return self.refresh_graphs_then_save_template(template_path);
            }
            Message::TemplateSaveCancel => {
                self.template_save.close();
                self.modal = None;
            }
            Message::RemoveSelected => {
                let state = self.state.read().expect("state lock poisoned");
                match &state.connection_view_selection {
                    ConnectionViewSelection::Tracks(set) => {
                        let viewed = state
                            .plugin_graph_clip
                            .is_none()
                            .then(|| state.plugin_graph_track.clone())
                            .flatten();
                        let mut removed_viewed = false;
                        let mut actions = vec![Action::BeginHistoryGroup];
                        for name in set {
                            if viewed.as_deref() == Some(name.as_str()) {
                                removed_viewed = true;
                                continue;
                            }
                            actions.push(Action::RemoveTrack(name.clone()));
                        }
                        drop(state);
                        if removed_viewed && let Some(viewed) = viewed {
                            self.state.write().expect("state lock poisoned").message = format!(
                                "Cannot delete '{}' while its connections are being viewed",
                                viewed
                            );
                        }
                        self.state
                            .write()
                            .expect("state lock poisoned")
                            .connection_view_selection = ConnectionViewSelection::None;
                        if actions.len() > 1 {
                            actions.push(Action::EndHistoryGroup);
                            return Self::restore_actions_task(actions);
                        }
                    }
                    ConnectionViewSelection::Connections(set) => {
                        let actions = connections::selection::track_disconnect_actions(&state, set);
                        let tasks = actions
                            .into_iter()
                            .map(|a| self.send(a))
                            .collect::<Vec<_>>();
                        drop(state);
                        self.state
                            .write()
                            .expect("state lock poisoned")
                            .connection_view_selection = ConnectionViewSelection::None;
                        return Task::batch(tasks);
                    }
                    ConnectionViewSelection::None => {}
                }
            }

            Message::Remove => {
                if !self.timing.selected_tempo_points.is_empty() {
                    return self.update(Message::TempoSelectionDelete);
                }
                if !self.timing.selected_time_signature_points.is_empty() {
                    return self.update(Message::TimeSignatureSelectionDelete);
                }

                let state = self.state.read().expect("state lock poisoned");
                let view = state.view.clone();
                let has_piano_notes =
                    state.piano.is_some() && !state.piano_selected_notes.is_empty();
                drop(state);

                if matches!(view, crate::state::View::Piano) && has_piano_notes {
                    return self.update(Message::PianoDeleteSelectedNotes);
                }

                let selected_clips: Vec<_> = if matches!(view, crate::state::View::Workspace) {
                    self.state
                        .read()
                        .expect("state lock poisoned")
                        .selected_clips
                        .iter()
                        .cloned()
                        .collect()
                } else {
                    Vec::new()
                };
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

                    self.state
                        .write()
                        .expect("state lock poisoned")
                        .selected_clips
                        .clear();

                    let mut actions = vec![Action::BeginHistoryGroup];
                    for (track_name, mut clip_indices) in audio_by_track {
                        clip_indices.sort_unstable_by(|a, b| b.cmp(a));
                        clip_indices.dedup();
                        for clip_index in clip_indices {
                            actions.push(Action::MoveClipToUnused {
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
                            actions.push(Action::MoveClipToUnused {
                                track_name: track_name.clone(),
                                kind: Kind::MIDI,
                                clip_indices: vec![clip_index],
                            });
                        }
                    }
                    actions.push(Action::EndHistoryGroup);
                    return Self::restore_actions_task(actions);
                }
                let has_selected_track_connections = {
                    let state = self.state.read().expect("state lock poisoned");
                    matches!(
                        &state.connection_view_selection,
                        ConnectionViewSelection::Connections(set) if !set.is_empty()
                    )
                };
                if has_selected_track_connections {
                    return self.update(Message::RemoveSelected);
                }
                let view = self.state.read().expect("state lock poisoned").view.clone();
                match view {
                    crate::state::View::Connections => {
                        let state = self.state.read().expect("state lock poisoned");
                        let has_folder_or_graph = state.connections_folder.is_some()
                            || state.plugin_graph_track.is_some();
                        drop(state);
                        if !has_folder_or_graph {
                            return self.update(Message::RemoveSelected);
                        }
                        if let Some(task) = self.remove_selected_track_plugin_graph_items() {
                            return task;
                        }
                        return Task::none();
                    }
                    crate::state::View::Workspace => {
                        if let Some(task) = self.remove_selected_track_plugin_graph_items() {
                            return task;
                        }
                        return self.update(Message::RemoveSelectedTracks);
                    }
                    crate::state::View::TrackPlugins => {
                        #[cfg(unix)]
                        {
                            let (selected_plugins, selected_indices) = {
                                let state = self.state.read().expect("state lock poisoned");
                                (
                                    state.plugin_graph_selected_plugins.clone(),
                                    state.plugin_graph_selected_connections.clone(),
                                )
                            };
                            let clip_target = self
                                .state
                                .read()
                                .expect("state lock poisoned")
                                .plugin_graph_clip
                                .clone();
                            if clip_target.is_some() {
                                let mut state = self.state.write().expect("state lock poisoned");
                                if let Some(&instance_id) = selected_plugins.iter().next() {
                                    let Some(selected_node) = state
                                        .plugin_graph_plugins
                                        .iter()
                                        .find(|p| p.instance_id == instance_id)
                                        .map(|p| p.node.clone())
                                    else {
                                        return Task::none();
                                    };
                                    state
                                        .plugin_graph_plugins
                                        .retain(|plugin| plugin.instance_id != instance_id);
                                    state.plugin_graph_connections.retain(|connection| {
                                        connection.from_node != selected_node
                                            && connection.to_node != selected_node
                                    });
                                    state.plugin_graph_selected_plugins.clear();
                                    state.plugin_graph_selected_connections.clear();
                                    state.plugin_graph_selected_connectable_connections.clear();
                                    let sync = Self::save_open_clip_plugin_graph(&mut state);
                                    return sync
                                        .map_or_else(Task::none, |action| self.send(action));
                                }
                                let selected = selected_indices.clone();
                                let existing = state.plugin_graph_connections.clone();
                                state.plugin_graph_connections = existing
                                    .into_iter()
                                    .enumerate()
                                    .filter_map(|(idx, connection)| {
                                        (!selected.contains(&idx)).then_some(connection)
                                    })
                                    .collect();
                                state.plugin_graph_selected_connections.clear();
                                state.plugin_graph_selected_plugins.clear();
                                state.plugin_graph_selected_connectable_connections.clear();
                                let sync = Self::save_open_clip_plugin_graph(&mut state);
                                return sync.map_or_else(Task::none, |action| self.send(action));
                            }
                            if let Some(task) = self.remove_selected_track_plugin_graph_items() {
                                return task;
                            }
                        }
                    }
                    crate::state::View::Piano => {
                        return self.update(Message::RemoveSelected);
                    }
                    crate::state::View::JackConnections
                    | crate::state::View::HwInputPorts
                    | crate::state::View::HwOutputPorts => {
                        return Task::none();
                    }
                    crate::state::View::PitchCorrection
                    | crate::state::View::AudioEditor
                    | crate::state::View::X32
                    | crate::state::View::Session => {
                        return Task::none();
                    }
                }

                if !self.state.read().expect("state lock poisoned").hw_loaded {
                    return Task::none();
                }
            }
            Message::TrackResizeStart(ref index) => {
                let mut state = self.state.write().expect("state lock poisoned");
                if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *index) {
                    let height = track.height;
                    state.resizing = Some(Resizing::Track(index.clone(), height, state.cursor.y));
                }
            }
            Message::TrackResizeHover(ref track_name, hovered) => {
                let mut state = self.state.write().expect("state lock poisoned");
                if hovered {
                    state.hovered_track_resize_handle = Some(track_name.clone());
                } else if state.hovered_track_resize_handle.as_deref() == Some(track_name.as_str())
                {
                    state.hovered_track_resize_handle = None;
                }
            }
            Message::TrackLaneResizeStart {
                ref track_name,
                divider,
            } => {
                let payload = {
                    let state = self.state.read().expect("state lock poisoned");
                    state
                        .tracks
                        .iter()
                        .find(|t| t.name == *track_name)
                        .map(|track| {
                            let available = if track.is_folder {
                                track.folder_content_height()
                            } else {
                                track.height
                            };
                            (track.resolved_lane_heights(available), state.cursor.y)
                        })
                };
                if let Some((heights, y)) = payload {
                    self.state.write().expect("state lock poisoned").resizing =
                        Some(Resizing::Lane(track_name.clone(), divider, heights, y));
                }
            }
            Message::TrackLaneDividerReset {
                ref track_name,
                divider: _,
            } => {
                let mut state = self.state.write().expect("state lock poisoned");
                if let Some(track) = state.tracks.iter_mut().find(|t| t.name == *track_name) {
                    track.reset_lane_heights();
                }
            }
            Message::TracksResizeStart => {
                let (initial_width, initial_mouse_x) = {
                    let state = self.state.read().expect("state lock poisoned");
                    let width = match state.tracks_width {
                        Length::Fixed(v) => v,
                        _ => 200.0,
                    };
                    (width, state.cursor.x)
                };
                self.state.write().expect("state lock poisoned").resizing =
                    Some(Resizing::Tracks(initial_width, initial_mouse_x));
            }
            Message::MixerResizeStart => {
                let (initial_height, initial_mouse_y) = {
                    let state = self.state.read().expect("state lock poisoned");
                    let height = match state.mixer_height {
                        Length::Fixed(v) => v,
                        _ => 300.0,
                    };
                    (height, state.cursor.y)
                };
                self.state.write().expect("state lock poisoned").resizing =
                    Some(Resizing::Mixer(initial_height, initial_mouse_y));
            }
            Message::TrackDrag { index, position } => {
                const TRACK_DRAG_SCROLL_UP_HOTZONE_HEIGHT: f32 = 24.0;

                let state = self.state.read().expect("state lock poisoned");
                if index < state.tracks.len() {
                    let track_name = state.tracks[index].name.clone();
                    drop(state);
                    self.track = Some(track_name);

                    if position.y <= TRACK_DRAG_SCROLL_UP_HOTZONE_HEIGHT {
                        return Task::batch(vec![
                            operation::scroll_by(
                                Id::new(EDITOR_SCROLL_ID),
                                operation::AbsoluteOffset { x: 0.0, y: -28.0 },
                            ),
                            operation::scroll_by(
                                Id::new(TRACKS_SCROLL_ID),
                                operation::AbsoluteOffset { x: 0.0, y: -28.0 },
                            ),
                        ]);
                    }
                }
            }
            Message::TrackDropped(point, _rect) => {
                if self.track.is_some() {
                    return maolan_widgets::iced_drop::zones_on_point(
                        Message::HandleTrackZones,
                        point,
                        None,
                        None,
                    );
                }
                self.track = None;
            }
            Message::HandleTrackZones(ref zones) => {
                if let Some(dragged_name) = self.track.clone() {
                    let dragged_id = Id::from(dragged_name.clone());

                    // Dropped on empty space -> move the track to the root.
                    if zones.is_empty() {
                        let mut state = self.state.write().expect("state lock poisoned");
                        if let Some(dragged_index) =
                            state.tracks.iter().position(|t| t.name == *dragged_name)
                        {
                            let mut moved_track = state.tracks.remove(dragged_index);
                            let had_parent = moved_track.parent_track.is_some();
                            moved_track.parent_track = None;
                            state.tracks.push(moved_track);
                            drop(state);
                            self.track = None;
                            if had_parent {
                                return self.send(Action::TrackSetParent {
                                    track_name: dragged_name.clone(),
                                    parent_name: None,
                                });
                            }
                        }
                        self.track = None;
                        return Task::none();
                    }

                    let target_zone = {
                        let state = self.state.read().expect("state lock poisoned");
                        zones.iter().find(|(zone_id, _)| {
                            *zone_id != dragged_id
                                && state
                                    .tracks
                                    .iter()
                                    .any(|track| Id::from(track.name.clone()) == *zone_id)
                        })
                    };
                    if let Some((track_id, _)) = target_zone {
                        let mut state = self.state.write().expect("state lock poisoned");
                        if let Some(dragged_index) =
                            state.tracks.iter().position(|t| t.name == *dragged_name)
                        {
                            let target_name = state
                                .tracks
                                .iter()
                                .find(|t| Id::from(t.name.clone()) == *track_id)
                                .map(|t| t.name.clone());

                            if let Some(target_name) = target_name {
                                if target_name == *dragged_name {
                                    // Dropped on itself; nothing to do.
                                } else if state
                                    .tracks
                                    .iter()
                                    .any(|t| t.name == target_name && t.is_folder)
                                {
                                    // Dropping onto a folder makes the dragged track a child.
                                    if state.tracks[dragged_index].is_master {
                                        state.message = format!(
                                            "Track '{}' is the master track and cannot be made part of a folder",
                                            dragged_name
                                        );
                                        self.track = None;
                                        return Task::none();
                                    }
                                    let mut current = target_name.as_str();
                                    let mut circular = false;
                                    while !circular {
                                        if let Some(parent) = state
                                            .tracks
                                            .iter()
                                            .find(|t| t.name == current)
                                            .and_then(|t| t.parent_track.as_deref())
                                        {
                                            if parent == dragged_name.as_str() {
                                                circular = true;
                                            } else {
                                                current = parent;
                                            }
                                        } else {
                                            break;
                                        }
                                    }

                                    if !circular {
                                        let mut moved_track = state.tracks.remove(dragged_index);
                                        moved_track.parent_track = Some(target_name.clone());

                                        if let Some(target_index) =
                                            state.tracks.iter().position(|t| t.name == target_name)
                                        {
                                            let target_depth = state.tracks[target_index]
                                                .folder_depth(&state.tracks);
                                            let mut insert_index = target_index + 1;
                                            while insert_index < state.tracks.len() {
                                                let depth = state.tracks[insert_index]
                                                    .folder_depth(&state.tracks);
                                                if depth > target_depth {
                                                    insert_index += 1;
                                                } else {
                                                    break;
                                                }
                                            }
                                            state.tracks.insert(insert_index, moved_track);
                                        } else {
                                            state.tracks.push(moved_track);
                                        }

                                        let mut tasks: Vec<Task<Message>> = vec![];
                                        if let Some(folder) =
                                            state.tracks.iter_mut().find(|t| t.name == target_name)
                                            && !folder.folder_open
                                        {
                                            folder.folder_open = true;
                                            tasks.push(self.send(Action::TrackToggleFolder {
                                                track_name: target_name.clone(),
                                            }));
                                        }
                                        drop(state);
                                        tasks.push(self.send(Action::TrackSetParent {
                                            track_name: dragged_name.clone(),
                                            parent_name: Some(target_name),
                                        }));
                                        self.track = None;
                                        return Task::batch(tasks);
                                    }
                                } else {
                                    // Dropping onto a non-folder reorders the track and removes it
                                    // from any folder.
                                    let mut moved_track = state.tracks.remove(dragged_index);
                                    let had_parent = moved_track.parent_track.is_some();
                                    moved_track.parent_track = None;
                                    let to_index = state
                                        .tracks
                                        .iter()
                                        .position(|t| Id::from(t.name.clone()) == *track_id);

                                    if let Some(t_idx) = to_index {
                                        state.tracks.insert(t_idx, moved_track);
                                    } else {
                                        state.tracks.push(moved_track);
                                    }

                                    drop(state);
                                    if had_parent {
                                        self.track = None;
                                        return self.send(Action::TrackSetParent {
                                            track_name: dragged_name.clone(),
                                            parent_name: None,
                                        });
                                    }
                                }
                            }
                        }
                    } else if !zones.iter().any(|(zone_id, _)| *zone_id == dragged_id) {
                        // The workspace drop zone was hit without a track beneath the pointer.
                        let mut state = self.state.write().expect("state lock poisoned");
                        if let Some(dragged_index) =
                            state.tracks.iter().position(|t| t.name == *dragged_name)
                        {
                            let mut moved_track = state.tracks.remove(dragged_index);
                            let had_parent = moved_track.parent_track.is_some();
                            moved_track.parent_track = None;
                            state.tracks.push(moved_track);
                            drop(state);
                            self.track = None;
                            if had_parent {
                                return self.send(Action::TrackSetParent {
                                    track_name: dragged_name.clone(),
                                    parent_name: None,
                                });
                            }
                        }
                    }
                }
                self.track = None;
            }
            _ => {}
        }
        self.update_children(&message);
        Task::none()
    }
}
