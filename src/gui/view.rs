use super::Maolan;
use crate::{
    message::{Message, Show},
    state::View,
    toolbar::ToolbarViewState,
    workspace::WorkspaceViewArgs,
};
use iced::{
    Length,
    widget::{button, column, container, progress_bar, row, text},
};

impl Maolan {
    pub fn view(&self) -> iced::Element<'_, Message> {
        let state = self.state.blocking_read();
        if state.hw_loaded {
            if state.clip_rename_dialog.is_some() {
                return self.clip_rename.view();
            }
            if state.track_rename_dialog.is_some() {
                return self.track_rename.view();
            }
            match self.modal {
                Some(Show::AddTrack) => self.add_track.view(),
                #[cfg(all(unix, not(target_os = "macos")))]
                Some(Show::TrackPluginList) => self.track_plugin_list_view(),
                #[cfg(any(target_os = "windows", target_os = "macos"))]
                Some(Show::TrackPluginList) => self.track_plugin_list_view(),
                _ => {
                    let view = match state.view {
                        View::Workspace => self.workspace.view(WorkspaceViewArgs {
                            playhead_samples: Some(self.transport_samples),
                            pixels_per_sample: self.pixels_per_sample(),
                            beat_pixels: self.beat_pixels(),
                            samples_per_bar: self.samples_per_bar() as f32,
                            loop_range_samples: self.loop_range_samples,
                            punch_range_samples: self.punch_range_samples,
                            zoom_visible_bars: self.zoom_visible_bars,
                            tracks_resize_hovered: self.tracks_resize_hovered,
                            mixer_resize_hovered: self.mixer_resize_hovered,
                            active_clip_drag: self.clip.as_ref(),
                            active_clip_target_track: self.clip_preview_target_track.as_deref(),
                            recording_preview_bounds: self.recording_preview_bounds(),
                            recording_preview_peaks: Some(self.recording_preview_peaks.clone()),
                        }),
                        View::Connections => self.connections.view(),
                        #[cfg(all(unix, not(target_os = "macos")))]
                        View::TrackPlugins => self.track_plugins.view(),
                        View::Piano => self
                            .workspace
                            .piano_view(self.pixels_per_sample(), self.samples_per_bar() as f32),
                        #[cfg(any(target_os = "windows", target_os = "macos"))]
                        View::TrackPlugins => self.connections.view(),
                    };

                    let has_session_end = state
                        .tracks
                        .iter()
                        .any(|track| !track.audio.clips.is_empty() || !track.midi.clips.is_empty());

                    let mut content = column![
                        self.menu.view(),
                        self.toolbar.view(ToolbarViewState {
                            playing: self.playing,
                            paused: self.paused,
                            recording: self.record_armed,
                            has_session_end,
                            has_loop_range: self.loop_range_samples.is_some(),
                            loop_enabled: self.loop_enabled,
                            has_punch_range: self.punch_range_samples.is_some(),
                            punch_enabled: self.punch_enabled,
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
                    content = content.push(text(format!("Last message: {}", state.message)));
                    container(content)
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .into()
                }
            }
        } else {
            column![
                self.hw.audio_view(),
                text(format!("Last message: {}", state.message)),
            ]
            .into()
        }
    }
}
