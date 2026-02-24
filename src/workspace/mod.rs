mod editor;
mod mixer;
mod ruler;
mod tempo;
mod tracks;

use crate::{
    message::{DraggedClip, Message},
    state::State,
};
use iced::{
    Background, Color, Element, Length, Point,
    widget::{Stack, column, container, mouse_area, pin, row, slider},
};
use std::collections::HashMap;

pub struct Workspace {
    state: State,
    editor: editor::Editor,
    mixer: mixer::Mixer,
    ruler: ruler::Ruler,
    tempo: tempo::Tempo,
    tracks: tracks::Tracks,
}

impl Workspace {
    pub fn new(state: State) -> Self {
        Self {
            state: state.clone(),
            editor: editor::Editor::new(state.clone()),
            mixer: mixer::Mixer::new(state.clone()),
            ruler: ruler::Ruler::new(),
            tempo: tempo::Tempo::new(),
            tracks: tracks::Tracks::new(state.clone()),
        }
    }

    pub fn update(&mut self, _message: Message) {}

    fn playhead_line() -> Element<'static, Message> {
        container("")
            .width(Length::Fixed(2.0))
            .height(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(Background::Color(Color {
                    r: 0.95,
                    g: 0.18,
                    b: 0.14,
                    a: 0.95,
                })),
                ..container::Style::default()
            })
            .into()
    }

    pub fn view(
        &self,
        playhead_samples: Option<f64>,
        pixels_per_sample: f32,
        beat_pixels: f32,
        samples_per_bar: f32,
        loop_range_samples: Option<(usize, usize)>,
        punch_range_samples: Option<(usize, usize)>,
        zoom_visible_bars: f32,
        tracks_resize_hovered: bool,
        mixer_resize_hovered: bool,
        active_clip_drag: Option<&DraggedClip>,
        active_clip_target_track: Option<&str>,
        recording_preview_bounds: Option<(usize, usize)>,
        recording_preview_peaks: Option<HashMap<String, Vec<Vec<f32>>>>,
    ) -> Element<'_, Message> {
        let tracks_width = self.state.blocking_read().tracks_width;
        let playhead_x =
            playhead_samples.map(|sample| (sample as f32 * pixels_per_sample).max(0.0));

        let editor_with_playhead = if let Some(x) = playhead_x {
            Stack::from_vec(vec![
                self.editor.view(
                    pixels_per_sample,
                    samples_per_bar,
                    active_clip_drag,
                    active_clip_target_track,
                    recording_preview_bounds,
                    recording_preview_peaks.clone(),
                ),
                pin(Self::playhead_line())
                    .position(Point::new(x.max(0.0), 0.0))
                    .into(),
            ])
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        } else {
            self.editor.view(
                pixels_per_sample,
                samples_per_bar,
                active_clip_drag,
                active_clip_target_track,
                recording_preview_bounds,
                recording_preview_peaks.clone(),
            )
        };

        let editor_with_zoom = Stack::from_vec(vec![
            editor_with_playhead,
            pin(container(
                slider(
                    1.0..=256.0,
                    zoom_visible_bars,
                    Message::ZoomVisibleBarsChanged,
                )
                .width(Length::Fixed(105.0)),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(8)
            .align_x(iced::alignment::Horizontal::Right)
            .align_y(iced::alignment::Vertical::Bottom))
            .into(),
        ])
        .width(Length::Fill)
        .height(Length::Fill);

        column![
            // Tempo Ruler
            row![
                container("")
                    .width(tracks_width)
                    .height(Length::Fill)
                    .style(|_theme| container::Style {
                        background: Some(Background::Color(Color {
                            r: 0.1,
                            g: 0.1,
                            b: 0.1,
                            a: 1.0,
                        })),
                        ..container::Style::default()
                    }),
                container("")
                    .width(Length::Fixed(3.0))
                    .height(Length::Fill)
                    .style(|_theme| {
                        container::Style {
                            background: Some(Background::Color(Color {
                                r: 0.5,
                                g: 0.5,
                                b: 0.5,
                                a: 0.5,
                            })),
                            ..container::Style::default()
                        }
                    }),
                self.tempo.view(
                    120.0,
                    (4, 4),
                    beat_pixels,
                    pixels_per_sample,
                    playhead_x,
                    punch_range_samples,
                ), // TODO: Get BPM and Time Signature from State
            ]
            .height(Length::Fixed(self.tempo.height())),
            row![
                container("")
                    .width(tracks_width)
                    .height(Length::Fill)
                    .style(|_theme| container::Style {
                        background: Some(Background::Color(Color {
                            r: 0.1,
                            g: 0.1,
                            b: 0.1,
                            a: 1.0,
                        })),
                        ..container::Style::default()
                    }),
                container("")
                    .width(Length::Fixed(3.0))
                    .height(Length::Fill)
                    .style(|_theme| {
                        container::Style {
                            background: Some(Background::Color(Color {
                                r: 0.5,
                                g: 0.5,
                                b: 0.5,
                                a: 0.5,
                            })),
                            ..container::Style::default()
                        }
                    }),
                self.ruler.view(
                    playhead_x,
                    beat_pixels,
                    pixels_per_sample,
                    loop_range_samples,
                ),
            ]
            .height(Length::Fixed(self.ruler.height())),
            row![
                self.tracks.view(),
                mouse_area(
                    container("")
                        .width(Length::Fixed(3.0))
                        .height(Length::Fill)
                        .style(move |_theme| container::Style {
                            background: Some(Background::Color(Color {
                                r: 0.7,
                                g: 0.7,
                                b: 0.7,
                                a: if tracks_resize_hovered { 0.95 } else { 0.6 },
                            })),
                            ..container::Style::default()
                        }),
                )
                .on_enter(Message::TracksResizeHover(true))
                .on_exit(Message::TracksResizeHover(false))
                .on_press(Message::TracksResizeStart),
                editor_with_zoom,
            ]
            .height(Length::Fill),
            mouse_area(
                container("")
                    .width(Length::Fill)
                    .height(Length::Fixed(3.0))
                    .style(move |_theme| container::Style {
                        background: Some(Background::Color(Color {
                            r: 0.7,
                            g: 0.7,
                            b: 0.7,
                            a: if mixer_resize_hovered { 0.95 } else { 0.6 },
                        })),
                        ..container::Style::default()
                    }),
            )
            .on_enter(Message::MixerResizeHover(true))
            .on_exit(Message::MixerResizeHover(false))
            .on_press(Message::MixerResizeStart),
            self.mixer.view(),
        ]
        .width(Length::Fill)
        .into()
    }
}
