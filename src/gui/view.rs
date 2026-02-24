use super::Maolan;
use crate::{
    message::{Message, Show},
    state::View,
};
use iced::{
    Length,
    widget::{button, column, container, row, text},
};

impl Maolan {
    pub fn view(&self) -> iced::Element<'_, Message> {
        let state = self.state.blocking_read();
        if state.hw_loaded {
            match self.modal {
                Some(Show::AddTrack) => self.add_track.view(),
                Some(Show::TrackPluginList) => self.track_plugin_list_view(),
                _ => {
                    let view = match state.view {
                        View::Workspace => self.workspace.view(
                            Some(self.transport_samples),
                            self.pixels_per_sample(),
                            self.beat_pixels(),
                            self.loop_range_samples,
                            self.punch_range_samples,
                            self.zoom_visible_bars,
                            self.tracks_resize_hovered,
                            self.mixer_resize_hovered,
                            self.clip.as_ref(),
                            self.clip_preview_target_track.as_deref(),
                            self.recording_preview_bounds(),
                            Some(self.recording_preview_peaks.clone()),
                        ),
                        View::Connections => self.connections.view(),
                        View::TrackPlugins => self.track_plugins.view(),
                    };

                    let mut content = column![
                        self.menu.view(),
                        self.toolbar.view(
                            self.playing,
                            self.record_armed,
                            self.loop_range_samples.is_some(),
                            self.loop_enabled,
                            self.punch_range_samples.is_some(),
                            self.punch_enabled,
                        )
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
