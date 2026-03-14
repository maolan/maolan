use super::Maolan;
use crate::{
    consts::state_ids::METRONOME_TRACK_ID,
    message::{Message, Show},
    state::View,
    toolbar::ToolbarViewState,
    workspace::WorkspaceViewArgs,
};
use iced::{
    Length,
    widget::{button, column, container, progress_bar, row, scrollable, text, text_input},
};

impl Maolan {
    fn playhead_bar_beat(&self, state: &crate::state::StateData, sample: usize) -> (u64, u64) {
        let mut bpm = state
            .tempo_points
            .iter()
            .rfind(|p| p.sample == 0)
            .map(|p| p.bpm)
            .unwrap_or(state.tempo)
            .clamp(20.0, 300.0);
        let (mut numerator, mut denominator) = state
            .time_signature_points
            .iter()
            .rfind(|p| p.sample == 0)
            .map(|p| (p.numerator.max(1), p.denominator.max(1)))
            .unwrap_or((
                state.time_signature_num.max(1),
                state.time_signature_denom.max(1),
            ));

        let mut tempo_idx = state.tempo_points.partition_point(|p| p.sample == 0);
        let mut tsig_idx = state
            .time_signature_points
            .partition_point(|p| p.sample == 0);

        let mut cursor = 0usize;
        let mut bar_number = 0u64;
        let mut bar_progress_quarters = 0.0_f64;

        while cursor < sample {
            let next_tempo = state
                .tempo_points
                .get(tempo_idx)
                .map(|p| p.sample)
                .unwrap_or(usize::MAX);
            let next_tsig = state
                .time_signature_points
                .get(tsig_idx)
                .map(|p| p.sample)
                .unwrap_or(usize::MAX);
            let next_change = next_tempo.min(next_tsig).min(sample);
            let segment_len = next_change.saturating_sub(cursor);

            if segment_len > 0 {
                let samples_per_quarter =
                    (self.playback_rate_hz.max(1.0) * 60.0 / bpm.max(1.0) as f64).max(1.0);
                let segment_quarters = segment_len as f64 / samples_per_quarter;
                let quarters_per_bar =
                    (numerator.max(1) as f64 * (4.0 / denominator.max(1) as f64)).max(1.0e-9);
                let total_quarters = bar_progress_quarters + segment_quarters;
                let advanced_bars = (total_quarters / quarters_per_bar).floor() as u64;
                bar_number = bar_number.saturating_add(advanced_bars);
                bar_progress_quarters = total_quarters - advanced_bars as f64 * quarters_per_bar;
            }

            cursor = next_change;

            while let Some(point) = state.tempo_points.get(tempo_idx) {
                if point.sample != cursor {
                    break;
                }
                bpm = point.bpm.clamp(20.0, 300.0);
                tempo_idx += 1;
            }
            while let Some(point) = state.time_signature_points.get(tsig_idx) {
                if point.sample != cursor {
                    break;
                }
                numerator = point.numerator.max(1);
                denominator = point.denominator.max(1);
                tsig_idx += 1;
            }
        }

        let beat_len_quarters = (4.0 / denominator.max(1) as f64).max(1.0e-9);
        let beat = ((bar_progress_quarters / beat_len_quarters).floor() as u64)
            .min(numerator.max(1) as u64 - 1)
            + 1;
        (bar_number, beat)
    }

    pub fn view(&self) -> iced::Element<'_, Message> {
        let state = self.state.blocking_read();
        if state.hw_loaded {
            if state.clip_rename_dialog.is_some() {
                return self.clip_rename.view();
            }
            if state.track_rename_dialog.is_some() {
                return self.track_rename.view();
            }
            if state.track_group_dialog.is_some() {
                return self.track_group.view();
            }
            if state.track_template_save_dialog.is_some() {
                return self.track_template_save.view();
            }
            if state.template_save_dialog.is_some() {
                return self.template_save.view();
            }
            match self.modal {
                Some(Show::AddTrack) => self.add_track.view(),
                Some(Show::ExportSettings) => self.export_settings_view(),
                Some(Show::SessionMetadata) => self.session_metadata_view(),
                Some(Show::Preferences) => self.preferences_view(),
                Some(Show::AutosaveRecovery) => self.autosave_recovery_view(),
                Some(Show::UnsavedChanges) => self.unsaved_changes_view(),
                #[cfg(all(unix, not(target_os = "macos")))]
                Some(Show::TrackPluginList) => self.track_plugin_list_view(),
                #[cfg(target_os = "macos")]
                Some(Show::TrackPluginList) => self.track_plugin_list_view(),
                _ => {
                    let view = match state.view {
                        View::Workspace => self.workspace.view(WorkspaceViewArgs {
                            session_root: self.session_dir.as_ref(),
                            playhead_samples: Some(self.transport_samples),
                            pixels_per_sample: self.pixels_per_sample(),
                            beat_pixels: self.beat_pixels(),
                            samples_per_bar: self.samples_per_bar() as f32,
                            loop_range_samples: self.loop_range_samples,
                            punch_range_samples: self.punch_range_samples,
                            snap_mode: self.snap_mode,
                            samples_per_beat: self.samples_per_beat(),
                            zoom_visible_bars: self.zoom_visible_bars,
                            tracks_resize_hovered: self.tracks_resize_hovered,
                            mixer_resize_hovered: self.mixer_resize_hovered,
                            tracks_visible: self.tracks_visible,
                            editor_visible: self.editor_visible,
                            mixer_visible: self.mixer_visible,
                            active_clip_drag: self.clip.as_ref(),
                            active_clip_target_track: self.clip_preview_target_track.as_deref(),
                            recording_preview_bounds: self.recording_preview_bounds(),
                            recording_preview_peaks: Some(&self.recording_preview_peaks),
                            midi_clip_previews: Some(&self.midi_clip_previews),
                            shift_pressed: state.shift,
                            selected_tempo_points: self
                                .selected_tempo_points
                                .iter()
                                .copied()
                                .collect(),
                            selected_time_signature_points: self
                                .selected_time_signature_points
                                .iter()
                                .copied()
                                .collect(),
                            mixer_level_edit_track: self.mixer_level_edit_track.as_deref(),
                            mixer_level_edit_input: &self.mixer_level_edit_input,
                        }),
                        View::Connections => self.connections.view(),
                        #[cfg(all(unix, not(target_os = "macos")))]
                        View::TrackPlugins => self.track_plugins.view(),
                        View::Piano => self.workspace.piano_view(WorkspaceViewArgs {
                            session_root: None,
                            playhead_samples: Some(self.transport_samples),
                            pixels_per_sample: self.pixels_per_sample(),
                            beat_pixels: self.beat_pixels(),
                            samples_per_bar: self.samples_per_bar() as f32,
                            loop_range_samples: None,
                            punch_range_samples: None,
                            snap_mode: self.snap_mode,
                            samples_per_beat: self.samples_per_beat(),
                            zoom_visible_bars: self.zoom_visible_bars,
                            tracks_resize_hovered: false,
                            mixer_resize_hovered: false,
                            tracks_visible: false,
                            editor_visible: false,
                            mixer_visible: false,
                            active_clip_drag: None,
                            active_clip_target_track: None,
                            recording_preview_bounds: None,
                            recording_preview_peaks: None,
                            midi_clip_previews: None,
                            shift_pressed: state.shift,
                            selected_tempo_points: self
                                .selected_tempo_points
                                .iter()
                                .copied()
                                .collect(),
                            selected_time_signature_points: self
                                .selected_time_signature_points
                                .iter()
                                .copied()
                                .collect(),
                            mixer_level_edit_track: None,
                            mixer_level_edit_input: "",
                        }),
                        #[cfg(target_os = "macos")]
                        View::TrackPlugins => self.connections.view(),
                    };

                    let has_session_end = state
                        .tracks
                        .iter()
                        .filter(|track| track.name != METRONOME_TRACK_ID)
                        .any(|track| !track.audio.clips.is_empty() || !track.midi.clips.is_empty());
                    let playhead_sample = self.transport_samples.max(0.0) as usize;
                    let playhead_seconds = playhead_sample as f64 / self.playback_rate_hz.max(1.0);
                    let minutes = (playhead_seconds / 60.0).floor() as u64;
                    let seconds = (playhead_seconds % 60.0).floor() as u64;
                    let millis = (playhead_seconds.fract() * 1000.0) as u64;
                    let playhead_time_label = format!("{minutes:02}:{seconds:02}.{millis:03}");
                    let (playhead_bar, playhead_beat) =
                        self.playhead_bar_beat(&state, playhead_sample);

                    let mut content = column![
                        self.menu.view(
                            self.tracks_visible,
                            self.editor_visible,
                            self.mixer_visible
                        ),
                        self.toolbar.view(ToolbarViewState {
                            playing: self.playing,
                            paused: self.paused,
                            recording: self.record_armed,
                            metronome_enabled: self.metronome_enabled,
                            has_session_end,
                            has_loop_range: self.loop_range_samples.is_some(),
                            loop_enabled: self.loop_enabled,
                            has_punch_range: self.punch_range_samples.is_some(),
                            punch_enabled: self.punch_enabled,
                            snap_mode: self.snap_mode,
                            tempo_input: self.tempo_input.clone(),
                            tsig_num_input: self.time_signature_num_input.clone(),
                            tsig_denom_input: self.time_signature_denom_input.clone(),
                            playhead_time_label,
                            playhead_bar,
                            playhead_beat,
                        })
                    ];
                    if matches!(state.view, View::TrackPlugins) {
                        content = content.push(
                            container(
                                row![
                                    button("Plugin List")
                                        .on_press(Message::Show(Show::TrackPluginList))
                                ]
                                .spacing(8),
                            )
                            .padding(8),
                        );
                    }
                    let has_timing_selection = !self.selected_tempo_points.is_empty()
                        || !self.selected_time_signature_points.is_empty();
                    let mut view: iced::Element<'_, Message> =
                        if matches!(state.view, View::Workspace | View::Piano)
                            && has_timing_selection
                        {
                            let lane_label = match self.timing_selection_lane {
                                Some(super::TimingSelectionLane::Tempo) => "Tempo Points",
                                Some(super::TimingSelectionLane::TimeSignature) => {
                                    "Time Signature Points"
                                }
                                None => "Timing Points",
                            };
                            let selected_count = if !self.selected_tempo_points.is_empty() {
                                self.selected_tempo_points.len()
                            } else {
                                self.selected_time_signature_points.len()
                            };
                            let editor_panel = container(
                                column![
                                    text(lane_label),
                                    text(format!("{selected_count} selected")).size(11),
                                    text_input("BPM", &self.tempo_input)
                                        .on_input(Message::TempoInputChanged)
                                        .on_submit(Message::TempoInputCommit),
                                    row![
                                        text_input("Num", &self.time_signature_num_input)
                                            .on_input(Message::TimeSignatureNumeratorInputChanged)
                                            .on_submit(Message::TimeSignatureInputCommit)
                                            .width(Length::Fill),
                                        text_input("Den", &self.time_signature_denom_input)
                                            .on_input(Message::TimeSignatureDenominatorInputChanged)
                                            .on_submit(Message::TimeSignatureInputCommit)
                                            .width(Length::Fill),
                                    ]
                                    .spacing(6),
                                    row![
                                        button("Duplicate").on_press(
                                            if !self.selected_tempo_points.is_empty() {
                                                Message::TempoSelectionDuplicate
                                            } else {
                                                Message::TimeSignatureSelectionDuplicate
                                            }
                                        ),
                                        button("Reset").on_press(
                                            if !self.selected_tempo_points.is_empty() {
                                                Message::TempoSelectionResetToPrevious
                                            } else {
                                                Message::TimeSignatureSelectionResetToPrevious
                                            }
                                        ),
                                    ]
                                    .spacing(6),
                                    row![
                                        button("Delete").on_press(
                                            if !self.selected_tempo_points.is_empty() {
                                                Message::TempoSelectionDelete
                                            } else {
                                                Message::TimeSignatureSelectionDelete
                                            }
                                        ),
                                        button("Clear")
                                            .on_press(Message::ClearTimingPointSelection),
                                    ]
                                    .spacing(6),
                                ]
                                .spacing(8),
                            )
                            .width(Length::Fixed(220.0))
                            .padding(8);
                            row![container(view).width(Length::Fill), editor_panel]
                                .width(Length::Fill)
                                .height(Length::Fill)
                                .into()
                        } else {
                            view
                        };
                    if self.midi_mappings_panel_open {
                        let mappings_list = if self.midi_mappings_report_lines.is_empty() {
                            column![text("No MIDI mappings loaded").size(11)]
                        } else {
                            self.midi_mappings_report_lines
                                .iter()
                                .fold(column![].spacing(4), |col, line| {
                                    col.push(text(line.clone()).size(11))
                                })
                        };
                        let mappings_panel = container(
                            column![
                                row![
                                    text("MIDI Mappings"),
                                    button("Refresh")
                                        .on_press(Message::MidiLearnMappingsReportRequest),
                                    button("Clear All")
                                        .on_press(Message::MidiLearnMappingsClearAllRequest),
                                    button("Hide").on_press(Message::MidiLearnMappingsPanelToggle),
                                ]
                                .spacing(6),
                                scrollable(mappings_list).height(Length::Fill),
                            ]
                            .spacing(8),
                        )
                        .width(Length::Fixed(340.0))
                        .height(Length::Fill)
                        .padding(8);
                        view = row![container(view).width(Length::Fill), mappings_panel]
                            .width(Length::Fill)
                            .height(Length::Fill)
                            .into();
                    }
                    content = content.push(view);
                    if self.import_in_progress {
                        let overall_progress = if self.import_total_files > 0 {
                            (self.import_current_file as f32 - 1.0 + self.import_file_progress)
                                / self.import_total_files as f32
                        } else {
                            0.0
                        }
                        .clamp(0.0, 1.0);

                        let operation_text = if let Some(ref op) = self.import_current_operation {
                            format!(" [{}]", op)
                        } else {
                            String::new()
                        };

                        content = content.push(
                            container(
                                column![
                                    text(format!(
                                        "Importing file {}/{}{}{}",
                                        self.import_current_file,
                                        self.import_total_files,
                                        operation_text,
                                        if self.import_current_filename.is_empty() {
                                            String::new()
                                        } else {
                                            format!(": {}", self.import_current_filename)
                                        }
                                    )),
                                    row![
                                        text("File:"),
                                        progress_bar(0.0..=1.0, self.import_file_progress),
                                        text(format!("{:.0}%", self.import_file_progress * 100.0))
                                    ]
                                    .spacing(8)
                                    .align_y(iced::Alignment::Center),
                                    row![
                                        text("Total:"),
                                        progress_bar(0.0..=1.0, overall_progress),
                                        text(format!("{:.0}%", overall_progress * 100.0))
                                    ]
                                    .spacing(8)
                                    .align_y(iced::Alignment::Center),
                                ]
                                .spacing(8),
                            )
                            .width(Length::Fill)
                            .padding(8),
                        );
                    }
                    if let Some(diag) = state.diagnostics_report.as_ref() {
                        content = content.push(text(format!("Diagnostics: {}", diag)));
                    }
                    let status_bar = container(text(format!("Last message: {}", state.message)))
                        .width(Length::Fill)
                        .padding(8);
                    column![
                        container(content).width(Length::Fill).height(Length::Fill),
                        status_bar,
                    ]
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
                }
            }
        } else {
            column![
                container(self.hw.audio_view())
                    .width(Length::Fill)
                    .height(Length::Fill),
                container(text(format!("Last message: {}", state.message)))
                    .width(Length::Fill)
                    .padding(8),
            ]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        }
    }
}
