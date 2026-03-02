mod editor;
mod mixer;
mod ruler;
mod tempo;
mod tracks;

use crate::{
    message::{DraggedClip, Message, SnapMode},
    state::State,
    widget::piano,
};
use iced::{
    Background, Color, Element, Length, Point,
    widget::{Id, Stack, column, container, mouse_area, pin, row, scrollable, slider},
};
use editor::EditorViewArgs;
use ruler::RulerViewArgs;
use std::collections::HashMap;
use tempo::TempoViewArgs;

pub const EDITOR_SCROLL_ID: &str = "workspace.editor.scroll";
pub const EDITOR_H_SCROLL_ID: &str = "workspace.editor.h_scroll";

pub struct Workspace {
    state: State,
    editor: editor::Editor,
    mixer: mixer::Mixer,
    piano: piano::Piano,
    ruler: ruler::Ruler,
    tempo: tempo::Tempo,
    tracks: tracks::Tracks,
}

pub struct WorkspaceViewArgs<'a> {
    pub playhead_samples: Option<f64>,
    pub pixels_per_sample: f32,
    pub beat_pixels: f32,
    pub samples_per_bar: f32,
    pub loop_range_samples: Option<(usize, usize)>,
    pub punch_range_samples: Option<(usize, usize)>,
    pub snap_mode: SnapMode,
    pub samples_per_beat: f64,
    pub zoom_visible_bars: f32,
    pub tracks_resize_hovered: bool,
    pub mixer_resize_hovered: bool,
    pub active_clip_drag: Option<&'a DraggedClip>,
    pub active_clip_target_track: Option<&'a str>,
    pub recording_preview_bounds: Option<(usize, usize)>,
    pub recording_preview_peaks: Option<HashMap<String, Vec<Vec<f32>>>>,
}

impl Workspace {
    const MIN_TIMELINE_BARS: f32 = 256.0;

    pub fn new(state: State) -> Self {
        Self {
            state: state.clone(),
            editor: editor::Editor::new(state.clone()),
            mixer: mixer::Mixer::new(state.clone()),
            piano: piano::Piano::new(state.clone()),
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

    pub fn view(&self, args: WorkspaceViewArgs<'_>) -> Element<'_, Message> {
        let WorkspaceViewArgs {
            playhead_samples,
            pixels_per_sample,
            beat_pixels,
            samples_per_bar,
            loop_range_samples,
            punch_range_samples,
            snap_mode,
            samples_per_beat,
            zoom_visible_bars,
            tracks_resize_hovered,
            mixer_resize_hovered,
            active_clip_drag,
            active_clip_target_track,
            recording_preview_bounds,
            recording_preview_peaks,
        } = args;
        let (tracks_width, max_end_samples) = {
            let state = self.state.blocking_read();
            let max_end_samples = state
                .tracks
                .iter()
                .map(|track| {
                    let audio_max = track
                        .audio
                        .clips
                        .iter()
                        .map(|clip| clip.start.saturating_add(clip.length))
                        .max()
                        .unwrap_or(0);
                    let midi_max = track
                        .midi
                        .clips
                        .iter()
                        .map(|clip| clip.start.saturating_add(clip.length))
                        .max()
                        .unwrap_or(0);
                    audio_max.max(midi_max)
                })
                .max()
                .unwrap_or(0);
            (state.tracks_width, max_end_samples)
        };
        let min_visible_samples = (samples_per_bar * zoom_visible_bars).max(1.0) as usize;
        let min_timeline_samples = (samples_per_bar * Self::MIN_TIMELINE_BARS).max(1.0) as usize;
        let timeline_samples = max_end_samples
            .max(min_visible_samples)
            .max(min_timeline_samples);
        let editor_content_width = (timeline_samples as f32 * pixels_per_sample).max(1.0);
        let playhead_x =
            playhead_samples.map(|sample| (sample as f32 * pixels_per_sample).max(0.0));

        let editor_with_playhead = if let Some(x) = playhead_x {
            Stack::from_vec(vec![
                self.editor.view(EditorViewArgs {
                    pixels_per_sample,
                    samples_per_bar,
                    snap_mode,
                    samples_per_beat,
                    active_clip_drag,
                    active_target_track: active_clip_target_track,
                    recording_preview_bounds,
                    recording_preview_peaks: recording_preview_peaks.as_ref(),
                }),
                pin(Self::playhead_line())
                    .position(Point::new(x.max(0.0), 0.0))
                    .into(),
            ])
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        } else {
            self.editor.view(EditorViewArgs {
                pixels_per_sample,
                samples_per_bar,
                snap_mode,
                samples_per_beat,
                active_clip_drag,
                active_target_track: active_clip_target_track,
                recording_preview_bounds,
                recording_preview_peaks: recording_preview_peaks.as_ref(),
            })
        };

        let right_lanes_scrolled = scrollable(
            column![
                container(self.tempo.view(TempoViewArgs {
                    bpm: 120.0,
                    time_signature: (4, 4),
                    pixels_per_sample,
                    playhead_x,
                    punch_range_samples,
                    snap_mode,
                    samples_per_beat,
                    content_width: editor_content_width,
                }))
                .height(Length::Fixed(self.tempo.height())),
                container(self.ruler.view(RulerViewArgs {
                    playhead_x,
                    beat_pixels,
                    pixels_per_sample,
                    loop_range_samples,
                    snap_mode,
                    samples_per_beat,
                    content_width: editor_content_width,
                }))
                .height(Length::Fixed(self.ruler.height())),
                container(editor_with_playhead)
                    .width(Length::Fixed(editor_content_width))
                    .height(Length::Fill),
            ]
            .height(Length::Fill),
        )
        .id(Id::new(EDITOR_SCROLL_ID))
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::hidden(),
        ))
        .on_scroll(|viewport| Message::EditorScrollXChanged(viewport.relative_offset().x))
        .width(Length::Fill)
        .height(Length::Fill);

        let h_scroll = scrollable(
            container("")
                .width(Length::Fixed(editor_content_width))
                .height(Length::Fixed(1.0)),
        )
        .id(Id::new(EDITOR_H_SCROLL_ID))
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new(),
        ))
        .on_scroll(|viewport| Message::EditorScrollXChanged(viewport.relative_offset().x))
        .width(Length::Fill)
        .height(Length::Fixed(16.0));

        let editor_with_zoom = Stack::from_vec(vec![
            right_lanes_scrolled.into(),
            pin(container(
                row![
                    h_scroll,
                    slider(
                        1.0..=256.0,
                        zoom_visible_bars,
                        Message::ZoomVisibleBarsChanged,
                    )
                    .width(Length::Fixed(105.0)),
                ]
                .spacing(8),
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
            row![
                column![
                    container("")
                        .width(tracks_width)
                        .height(Length::Fixed(self.tempo.height()))
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
                        .width(tracks_width)
                        .height(Length::Fixed(self.ruler.height()))
                        .style(|_theme| container::Style {
                            background: Some(Background::Color(Color {
                                r: 0.1,
                                g: 0.1,
                                b: 0.1,
                                a: 1.0,
                            })),
                            ..container::Style::default()
                        }),
                    self.tracks.view(),
                ]
                .width(tracks_width)
                .height(Length::Fill),
                mouse_area(column![
                    container("")
                        .width(Length::Fixed(3.0))
                        .height(Length::Fixed(self.tempo.height()))
                        .style(|_theme| container::Style {
                            background: Some(Background::Color(Color {
                                r: 0.5,
                                g: 0.5,
                                b: 0.5,
                                a: 0.5,
                            })),
                            ..container::Style::default()
                        }),
                    container("")
                        .width(Length::Fixed(3.0))
                        .height(Length::Fixed(self.ruler.height()))
                        .style(|_theme| container::Style {
                            background: Some(Background::Color(Color {
                                r: 0.5,
                                g: 0.5,
                                b: 0.5,
                                a: 0.5,
                            })),
                            ..container::Style::default()
                        }),
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
                ],)
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

    pub fn piano_view(&self, pixels_per_sample: f32, samples_per_bar: f32) -> Element<'_, Message> {
        self.piano.view(pixels_per_sample, samples_per_bar)
    }
}
