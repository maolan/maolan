mod editor;
mod mixer;
mod ruler;
mod tempo;
mod tracks;

use crate::{
    consts::{
        state_ids::METRONOME_TRACK_ID,
        widget_piano::{
            KEYBOARD_WIDTH, MAIN_SPLIT_SPACING, RIGHT_SCROLL_GUTTER_WIDTH, TOOLS_STRIP_WIDTH,
        },
        workspace::{MIN_TIMELINE_BARS, PLAYHEAD_WIDTH_PX, TIMELINE_LEFT_INSET_PX},
    },
    gui::visible_bars_to_zoom_slider,
    message::{DraggedClip, Message, SnapMode},
    state::{ClipPeaks, MidiClipPreviewMap, State},
    widget::{midi_edit, pitch_correction},
};
use editor::{EditorViewArgs, OwnedEditorViewArgs};
use iced::{
    Background, Color, Element, Length, Point,
    widget::{Id, Space, Stack, column, container, lazy, mouse_area, pin, row, scrollable, slider},
};
use maolan_widgets::{
    horizontal_scrollbar::HorizontalScrollbar, vertical_scrollbar::VerticalScrollbar,
};
use ruler::RulerViewArgs;
use std::{collections::HashMap, path::PathBuf};
use tempo::TempoViewArgs;

pub use crate::consts::workspace_ids::{
    EDITOR_SCROLL_ID, EDITOR_TIMELINE_SCROLL_ID, PIANO_RULER_SCROLL_ID, PIANO_TEMPO_SCROLL_ID,
    TRACKS_SCROLL_ID, WORKSPACE_RULER_SCROLL_ID, WORKSPACE_TEMPO_SCROLL_ID,
};

pub(crate) fn timeline_sample_to_x_f64(sample: f64, pixels_per_sample: f32, inset_px: f32) -> f32 {
    inset_px + (sample as f32 * pixels_per_sample).max(0.0)
}

pub(crate) fn timeline_sample_to_x(sample: usize, pixels_per_sample: f32, inset_px: f32) -> f32 {
    timeline_sample_to_x_f64(sample as f64, pixels_per_sample, inset_px)
}

pub(crate) fn timeline_x_to_sample_f32(x: f32, pixels_per_sample: f32, inset_px: f32) -> f32 {
    if pixels_per_sample <= 1.0e-9 {
        0.0
    } else {
        ((x - inset_px).max(0.0) / pixels_per_sample).max(0.0)
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct VisibleTrackWindow {
    pub start_index: usize,
    pub end_index: usize,
    pub top_padding: f32,
    pub bottom_padding: f32,
}

fn compute_visible_track_window(
    track_heights: &[f32],
    scroll_y: f32,
    viewport_height: f32,
) -> VisibleTrackWindow {
    if track_heights.is_empty() {
        return VisibleTrackWindow {
            start_index: 0,
            end_index: 0,
            top_padding: 0.0,
            bottom_padding: 0.0,
        };
    }

    const OVERSCAN_PX: f32 = 240.0;

    let total_height = track_heights.iter().sum::<f32>().max(1.0);
    let viewport_height = viewport_height.max(track_heights[0]).min(total_height);
    let max_scroll = (total_height - viewport_height).max(0.0);
    let scroll_top = scroll_y.clamp(0.0, 1.0) * max_scroll;
    let visible_top = (scroll_top - OVERSCAN_PX).max(0.0);
    let visible_bottom = (scroll_top + viewport_height + OVERSCAN_PX).min(total_height);

    let mut top_padding = 0.0;
    let mut start_index = 0;
    while start_index < track_heights.len()
        && top_padding + track_heights[start_index] <= visible_top
    {
        top_padding += track_heights[start_index];
        start_index += 1;
    }

    let mut bottom_edge = top_padding;
    let mut end_index = start_index;
    while end_index < track_heights.len() && bottom_edge < visible_bottom {
        bottom_edge += track_heights[end_index];
        end_index += 1;
    }

    if start_index == end_index {
        end_index = (start_index + 1).min(track_heights.len());
        bottom_edge = top_padding + track_heights[start_index.min(track_heights.len() - 1)];
    }

    VisibleTrackWindow {
        start_index,
        end_index,
        top_padding,
        bottom_padding: (total_height - bottom_edge).max(0.0),
    }
}

pub struct Workspace {
    state: State,
    editor: editor::Editor,
    mixer: mixer::Mixer,
    midi_edit: midi_edit::MIDIEdit,
    pitch_correction: pitch_correction::PitchCorrection,
    ruler: ruler::Ruler,
    tempo: tempo::Tempo,
    tracks: tracks::Tracks,
}

pub struct WorkspaceViewArgs<'a> {
    pub session_root: Option<&'a PathBuf>,
    pub playhead_samples: Option<f64>,
    pub transport_active: bool,
    pub pixels_per_sample: f32,
    pub beat_pixels: f32,
    pub samples_per_bar: f32,
    pub loop_range_samples: Option<(usize, usize)>,
    pub punch_range_samples: Option<(usize, usize)>,
    pub snap_mode: SnapMode,
    pub samples_per_beat: f64,
    pub zoom_visible_bars: f32,
    pub editor_scroll_x: f32,
    pub mixer_scroll_x: f32,
    pub window_width: f32,
    pub window_height: f32,
    pub editor_scroll_y: f32,
    pub track_drag_active: bool,
    pub tracks_resize_hovered: bool,
    pub mixer_resize_hovered: bool,
    pub tracks_visible: bool,
    pub editor_visible: bool,
    pub mixer_visible: bool,
    pub active_clip_drag: Option<&'a DraggedClip>,
    pub active_clip_target_track: Option<&'a str>,
    pub active_clip_target_valid: bool,
    pub recording_preview_bounds: Option<(usize, usize)>,
    pub recording_preview_peaks: Option<&'a HashMap<String, ClipPeaks>>,
    pub midi_clip_previews: Option<&'a MidiClipPreviewMap>,
    pub shift_pressed: bool,
    pub selected_tempo_points: Vec<usize>,
    pub selected_time_signature_points: Vec<usize>,
    pub mixer_level_edit_track: Option<&'a str>,
    pub mixer_level_edit_input: &'a str,
}

impl Workspace {
    pub fn new(state: State) -> Self {
        Self {
            state: state.clone(),
            editor: editor::Editor::new(state.clone()),
            mixer: mixer::Mixer::new(state.clone()),
            midi_edit: midi_edit::MIDIEdit::new(state.clone()),
            pitch_correction: pitch_correction::PitchCorrection::new(state.clone()),
            ruler: ruler::Ruler::new(),
            tempo: tempo::Tempo::new(),
            tracks: tracks::Tracks::new(state.clone()),
        }
    }

    pub fn update(&mut self, _message: &Message) {}

    fn playhead_line() -> Element<'static, Message> {
        container("")
            .width(Length::Fixed(PLAYHEAD_WIDTH_PX))
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

    pub fn view<'a>(&'a self, args: WorkspaceViewArgs<'a>) -> Element<'a, Message> {
        let WorkspaceViewArgs {
            session_root,
            playhead_samples,
            transport_active,
            pixels_per_sample,
            beat_pixels,
            samples_per_bar,
            loop_range_samples,
            punch_range_samples,
            snap_mode,
            samples_per_beat,
            zoom_visible_bars,
            editor_scroll_x,
            mixer_scroll_x,
            window_width,
            window_height,
            editor_scroll_y,
            track_drag_active,
            tracks_resize_hovered,
            mixer_resize_hovered,
            tracks_visible,
            editor_visible,
            mixer_visible,
            active_clip_drag,
            active_clip_target_track,
            active_clip_target_valid,
            recording_preview_bounds,
            recording_preview_peaks,
            midi_clip_previews,
            shift_pressed,
            selected_tempo_points,
            selected_time_signature_points,
            mixer_level_edit_track,
            mixer_level_edit_input,
        } = args;
        let (
            tracks_width,
            tracks_width_px,
            max_end_samples,
            tracks_total_height,
            track_heights,
            tempo,
            time_signature,
            tempo_points,
            time_signature_points,
            mixer_height_px,
        ) = {
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
            let track_heights = state
                .tracks
                .iter()
                .filter(|track| track.name != METRONOME_TRACK_ID)
                .map(|track| track.height)
                .collect::<Vec<_>>();
            (
                state.tracks_width,
                match state.tracks_width {
                    Length::Fixed(width) => width,
                    _ => 200.0,
                },
                max_end_samples,
                track_heights.iter().sum::<f32>().max(1.0),
                track_heights,
                state.tempo,
                (state.time_signature_num, state.time_signature_denom),
                state
                    .tempo_points
                    .iter()
                    .map(|p| (p.sample, p.bpm))
                    .collect::<Vec<_>>(),
                state
                    .time_signature_points
                    .iter()
                    .map(|p| (p.sample, p.numerator, p.denominator))
                    .collect::<Vec<_>>(),
                match state.mixer_height {
                    Length::Fixed(height) => height,
                    _ => 300.0,
                },
            )
        };
        const TOP_CHROME_ESTIMATE_PX: f32 = 72.0;
        const MIXER_SPLITTER_HEIGHT_PX: f32 = 3.0;
        let track_viewport_height = (window_height
            - TOP_CHROME_ESTIMATE_PX
            - if mixer_visible {
                mixer_height_px + MIXER_SPLITTER_HEIGHT_PX
            } else {
                0.0
            }
            - self.tempo.height()
            - self.ruler.height())
        .max(160.0);
        let visible_track_window =
            compute_visible_track_window(&track_heights, editor_scroll_y, track_viewport_height);
        let tracks_visible_window = if track_drag_active {
            VisibleTrackWindow {
                start_index: 0,
                end_index: track_heights.len(),
                top_padding: 0.0,
                bottom_padding: 0.0,
            }
        } else {
            visible_track_window
        };
        let min_visible_samples = (samples_per_bar * zoom_visible_bars).max(1.0) as usize;
        let min_timeline_samples = (samples_per_bar * MIN_TIMELINE_BARS).max(1.0) as usize;
        let right_padding_samples = ((samples_per_bar * zoom_visible_bars) * 0.5).max(1.0) as usize;
        let playhead_extent_samples = playhead_samples
            .map(|sample| sample.max(0.0) as usize)
            .unwrap_or(0)
            .saturating_add(right_padding_samples);
        let content_extent_samples = max_end_samples.saturating_add(right_padding_samples);
        let timeline_samples = max_end_samples
            .max(playhead_extent_samples)
            .max(content_extent_samples)
            .max(min_visible_samples)
            .max(min_timeline_samples);
        let editor_content_width = (timeline_samples as f32 * pixels_per_sample).max(1.0);
        let workspace_content_height =
            self.tempo.height() + self.ruler.height() + tracks_total_height;
        let track_context_menu_overlay = {
            let state = self.state.blocking_read();
            tracks::track_context_menu_overlay(&state)
        };
        let clip_context_menu_overlay = {
            let state = self.state.blocking_read();
            editor::clip_context_menu_overlay(&state, transport_active)
        };
        let playhead_x_timeline = playhead_samples.map(|sample| {
            timeline_sample_to_x_f64(sample, pixels_per_sample, TIMELINE_LEFT_INSET_PX)
        });

        let editor_render_hash = self.editor.render_hash(&EditorViewArgs {
            session_root,
            pixels_per_sample,
            samples_per_bar,
            snap_mode,
            samples_per_beat,
            active_clip_drag,
            active_target_track: active_clip_target_track,
            active_target_valid: active_clip_target_valid,
            recording_preview_bounds,
            recording_preview_peaks,
            midi_clip_previews,
            visible_track_window,
        });
        let editor = self.editor.clone();
        let editor_args_owned = OwnedEditorViewArgs {
            session_root: session_root.cloned(),
            pixels_per_sample,
            samples_per_bar,
            snap_mode,
            samples_per_beat,
            active_clip_drag: active_clip_drag.cloned(),
            active_target_track: active_clip_target_track.map(str::to_string),
            active_target_valid: active_clip_target_valid,
            recording_preview_bounds,
            recording_preview_peaks: recording_preview_peaks.cloned(),
            midi_clip_previews: midi_clip_previews.cloned(),
            visible_track_window,
        };
        let editor_body: Element<'_, Message> = lazy(editor_render_hash, move |_| {
            editor.clone().into_view_owned(editor_args_owned.clone())
        })
        .into();
        let editor_with_playhead = if let Some(x) = playhead_x_timeline {
            Stack::from_vec(vec![
                editor_body,
                pin(Self::playhead_line())
                    .position(Point::new(x.max(0.0), 0.0))
                    .into(),
            ])
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        } else {
            editor_body
        };

        let editor_timeline_scrolled = scrollable(
            container(editor_with_playhead)
                .width(Length::Fixed(editor_content_width))
                .height(Length::Fixed(tracks_total_height)),
        )
        .id(Id::new(EDITOR_TIMELINE_SCROLL_ID))
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::hidden(),
        ))
        .on_scroll(|viewport| Message::EditorScrollXChanged(viewport.relative_offset().x))
        .width(Length::Fill)
        .height(Length::Fixed(tracks_total_height));

        let right_lanes_scrolled = scrollable(editor_timeline_scrolled)
            .id(Id::new(EDITOR_SCROLL_ID))
            .direction(scrollable::Direction::Vertical(
                scrollable::Scrollbar::hidden(),
            ))
            .on_scroll(|viewport| Message::EditorScrollYChanged(viewport.relative_offset().y))
            .width(Length::Fill)
            .height(Length::Fill);
        let right_lanes_with_scrollbar: Element<'_, Message> =
            if tracks_total_height > track_viewport_height + f32::EPSILON {
                row![
                    right_lanes_scrolled,
                    VerticalScrollbar::new(
                        tracks_total_height,
                        editor_scroll_y,
                        Message::EditorScrollYChanged,
                    )
                    .width(Length::Fixed(16.0))
                    .height(Length::Fill),
                ]
                .spacing(0)
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
            } else {
                right_lanes_scrolled.into()
            };

        let h_scroll = HorizontalScrollbar::new(
            editor_content_width,
            editor_scroll_x,
            Message::EditorScrollXChanged,
        )
        .width(Length::Fill)
        .height(Length::Fixed(16.0));

        let editor_with_zoom = right_lanes_with_scrollbar;
        let tracks_scrolled = scrollable(self.tracks.view(tracks_visible_window))
            .id(Id::new(TRACKS_SCROLL_ID))
            .direction(scrollable::Direction::Vertical(
                scrollable::Scrollbar::hidden(),
            ))
            .on_scroll(|viewport| Message::EditorScrollYChanged(viewport.relative_offset().y))
            .width(tracks_width)
            .height(Length::Fill);

        let tempo_scrolled: Element<'_, Message> = scrollable(self.tempo.view(TempoViewArgs {
            bpm: tempo,
            time_signature,
            pixels_per_sample,
            playhead_x: playhead_x_timeline.map(|x| x.max(0.0)),
            punch_range_samples,
            snap_mode,
            samples_per_beat,
            samples_per_bar: samples_per_bar as f64,
            content_width: editor_content_width,
            tempo_points,
            time_signature_points,
            shift_pressed,
            selected_tempo_points,
            selected_time_signature_points,
            timeline_left_inset_px: TIMELINE_LEFT_INSET_PX,
        }))
        .id(Id::new(WORKSPACE_TEMPO_SCROLL_ID))
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::hidden(),
        ))
        .on_scroll(|viewport| Message::EditorScrollXChanged(viewport.relative_offset().x))
        .height(Length::Fixed(self.tempo.height()))
        .into();
        let ruler_scrolled: Element<'_, Message> = scrollable(self.ruler.view(RulerViewArgs {
            playhead_x: playhead_x_timeline.map(|x| x.max(0.0)),
            beat_pixels,
            pixels_per_sample,
            loop_range_samples,
            snap_mode,
            samples_per_beat,
            content_width: editor_content_width,
            timeline_left_inset_px: TIMELINE_LEFT_INSET_PX,
        }))
        .id(Id::new(WORKSPACE_RULER_SCROLL_ID))
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::hidden(),
        ))
        .on_scroll(|viewport| Message::EditorScrollXChanged(viewport.relative_offset().x))
        .height(Length::Fixed(self.ruler.height()))
        .into();

        let right_panel = column![
            tempo_scrolled,
            ruler_scrolled,
            container(editor_with_zoom).height(Length::Fixed(tracks_total_height)),
        ]
        .width(Length::Fill);

        let left_panel = column![
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
            tracks_scrolled,
        ]
        .width(tracks_width);

        let tracks_splitter = mouse_area(column![
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
        ])
        .on_enter(Message::TracksResizeHover(true))
        .on_exit(Message::TracksResizeHover(false))
        .on_press(Message::TracksResizeStart);

        let shared_workspace: Element<'_, Message> = match (tracks_visible, editor_visible) {
            (true, true) => row![left_panel, tracks_splitter, right_panel]
                .height(Length::Fixed(workspace_content_height))
                .into(),
            (true, false) => row![left_panel]
                .height(Length::Fixed(workspace_content_height))
                .into(),
            (false, true) => row![right_panel]
                .height(Length::Fixed(workspace_content_height))
                .into(),
            (false, false) => container("")
                .width(Length::Fill)
                .height(Length::Fixed(workspace_content_height))
                .into(),
        };

        let shared_workspace: Element<'_, Message> = {
            let mut stack = Stack::new().push(shared_workspace);
            if let Some((anchor, menu)) = track_context_menu_overlay {
                stack = stack.push(pin(menu).position(Point::new(
                    anchor.x.max(0.0),
                    self.tempo.height() + self.ruler.height() + anchor.y.max(0.0),
                )));
            }
            if let Some((anchor, menu)) = clip_context_menu_overlay {
                stack = stack.push(pin(menu).position(Point::new(
                    tracks_width_px + 3.0 + anchor.x.max(0.0),
                    self.tempo.height() + self.ruler.height() + anchor.y.max(0.0),
                )));
            }
            stack.into()
        };

        let editor_footer: Element<'_, Message> = if editor_visible {
            container(
                row![
                    Space::new().width(Length::Fixed(if tracks_visible {
                        tracks_width_px + 3.0
                    } else {
                        0.0
                    })),
                    container(
                        row![
                            h_scroll,
                            slider(
                                0.0..=1.0,
                                visible_bars_to_zoom_slider(zoom_visible_bars),
                                Message::ZoomSliderChanged,
                            )
                            .step(0.001)
                            .width(Length::Fixed(105.0)),
                        ]
                        .spacing(8),
                    )
                    .width(Length::Fill)
                    .height(Length::Fixed(16.0))
                    .padding([0, 8]),
                ]
                .height(Length::Fill)
                .align_y(iced::alignment::Vertical::Bottom),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        } else {
            Space::new().into()
        };

        let workspace_with_footer = Stack::from_vec(vec![
            shared_workspace,
            container(editor_footer)
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
        ])
        .width(Length::Fill)
        .height(Length::Fill);
        let workspace_body: Element<'_, Message> = if mixer_visible {
            column![
                workspace_with_footer,
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
                self.mixer.view(
                    mixer_level_edit_track,
                    mixer_level_edit_input,
                    window_width,
                    mixer_scroll_x,
                ),
            ]
            .width(Length::Fill)
            .into()
        } else {
            column![workspace_with_footer].width(Length::Fill).into()
        };
        container(workspace_body)
            .style(|_theme| crate::style::app_background())
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    pub fn piano_view<'a>(&'a self, args: WorkspaceViewArgs<'a>) -> Element<'a, Message> {
        let WorkspaceViewArgs {
            playhead_samples,
            pixels_per_sample,
            beat_pixels,
            samples_per_bar,
            snap_mode,
            samples_per_beat,
            shift_pressed,
            mixer_visible: _,
            selected_tempo_points,
            selected_time_signature_points,
            ..
        } = args;

        let (
            tempo,
            time_signature,
            tempo_points,
            time_signature_points,
            clip_length_samples,
            zoom_x,
        ) = {
            let state = self.state.blocking_read();
            (
                state.tempo,
                (state.time_signature_num, state.time_signature_denom),
                state
                    .tempo_points
                    .iter()
                    .map(|p| (p.sample, p.bpm))
                    .collect::<Vec<_>>(),
                state
                    .time_signature_points
                    .iter()
                    .map(|p| (p.sample, p.numerator, p.denominator))
                    .collect::<Vec<_>>(),
                state
                    .piano
                    .as_ref()
                    .map(|roll| roll.clip_length_samples)
                    .unwrap_or(samples_per_bar.max(1.0) as usize),
                state.piano_zoom_x,
            )
        };
        let horizontal_zoom = zoom_x.max(1.0);
        let horizontal_pixels_per_sample = (pixels_per_sample * horizontal_zoom).max(0.0001);
        let horizontal_beat_pixels = (beat_pixels * horizontal_zoom).max(0.0001);
        let timeline_content_width =
            (clip_length_samples.max(1) as f32 * horizontal_pixels_per_sample).max(1.0);
        let playhead_x =
            playhead_samples.map(|sample| (sample as f32 * horizontal_pixels_per_sample).max(0.0));

        let piano_content = self
            .midi_edit
            .view(pixels_per_sample, samples_per_bar, playhead_x);

        container(
            column![
                row![
                    container("")
                        .width(Length::Fixed(TOOLS_STRIP_WIDTH + MAIN_SPLIT_SPACING,))
                        .height(Length::Fill),
                    container("")
                        .width(Length::Fixed(KEYBOARD_WIDTH))
                        .height(Length::Fill),
                    scrollable(container(self.tempo.view(TempoViewArgs {
                        bpm: tempo,
                        time_signature,
                        pixels_per_sample: horizontal_pixels_per_sample,
                        playhead_x,
                        punch_range_samples: None,
                        snap_mode,
                        samples_per_beat,
                        samples_per_bar: samples_per_bar as f64,
                        content_width: timeline_content_width,
                        tempo_points,
                        time_signature_points,
                        shift_pressed,
                        selected_tempo_points,
                        selected_time_signature_points,
                        timeline_left_inset_px: 0.0,
                    })))
                    .id(Id::new(PIANO_TEMPO_SCROLL_ID))
                    .direction(scrollable::Direction::Horizontal(
                        scrollable::Scrollbar::hidden(),
                    ))
                    .on_scroll(|viewport| Message::PianoScrollXChanged(
                        viewport.relative_offset().x
                    ))
                    .width(Length::Fill)
                    .height(Length::Fill),
                    container("")
                        .width(Length::Fixed(RIGHT_SCROLL_GUTTER_WIDTH))
                        .height(Length::Fill),
                ]
                .width(Length::Fill)
                .height(Length::Fixed(self.tempo.height())),
                row![
                    container("")
                        .width(Length::Fixed(TOOLS_STRIP_WIDTH + MAIN_SPLIT_SPACING,))
                        .height(Length::Fill),
                    container("")
                        .width(Length::Fixed(KEYBOARD_WIDTH))
                        .height(Length::Fill),
                    scrollable(container(self.ruler.view(RulerViewArgs {
                        playhead_x,
                        beat_pixels: horizontal_beat_pixels,
                        pixels_per_sample: horizontal_pixels_per_sample,
                        loop_range_samples: None,
                        snap_mode,
                        samples_per_beat,
                        content_width: timeline_content_width,
                        timeline_left_inset_px: 0.0,
                    })))
                    .id(Id::new(PIANO_RULER_SCROLL_ID))
                    .direction(scrollable::Direction::Horizontal(
                        scrollable::Scrollbar::hidden(),
                    ))
                    .on_scroll(|viewport| Message::PianoScrollXChanged(
                        viewport.relative_offset().x
                    ))
                    .width(Length::Fill)
                    .height(Length::Fill),
                    container("")
                        .width(Length::Fixed(RIGHT_SCROLL_GUTTER_WIDTH))
                        .height(Length::Fill),
                ]
                .width(Length::Fill)
                .height(Length::Fixed(self.ruler.height())),
                piano_content,
            ]
            .width(Length::Fill)
            .height(Length::Fill),
        )
        .style(|_theme| crate::style::app_background())
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    pub fn pitch_correction_view<'a>(
        &'a self,
        args: WorkspaceViewArgs<'a>,
    ) -> Element<'a, Message> {
        let WorkspaceViewArgs {
            playhead_samples,
            pixels_per_sample,
            beat_pixels,
            samples_per_bar,
            snap_mode,
            samples_per_beat,
            shift_pressed,
            selected_tempo_points,
            selected_time_signature_points,
            ..
        } = args;
        let (
            clip_length_samples,
            zoom_x,
            tempo,
            time_signature,
            tempo_points,
            time_signature_points,
        ) = {
            let state = self.state.blocking_read();
            (
                state
                    .pitch_correction
                    .as_ref()
                    .map(|roll| roll.clip_length_samples)
                    .unwrap_or(samples_per_bar.max(1.0) as usize),
                state.piano_zoom_x,
                state.tempo,
                (state.time_signature_num, state.time_signature_denom),
                state
                    .tempo_points
                    .iter()
                    .map(|p| (p.sample, p.bpm))
                    .collect::<Vec<_>>(),
                state
                    .time_signature_points
                    .iter()
                    .map(|p| (p.sample, p.numerator, p.denominator))
                    .collect::<Vec<_>>(),
            )
        };
        let horizontal_zoom = zoom_x.max(1.0);
        let horizontal_pixels_per_sample = (pixels_per_sample * horizontal_zoom).max(0.0001);
        let horizontal_beat_pixels = (beat_pixels * horizontal_zoom).max(0.0001);
        let timeline_content_width =
            (clip_length_samples.max(1) as f32 * horizontal_pixels_per_sample).max(1.0);
        let playhead_x =
            playhead_samples.map(|sample| (sample as f32 * horizontal_pixels_per_sample).max(0.0));
        let pitch_correction_content =
            self.pitch_correction
                .view(pixels_per_sample, samples_per_bar, playhead_x);

        container(
            column![
                row![
                    container("")
                        .width(Length::Fixed(TOOLS_STRIP_WIDTH + MAIN_SPLIT_SPACING,))
                        .height(Length::Fill),
                    container("")
                        .width(Length::Fixed(KEYBOARD_WIDTH))
                        .height(Length::Fill),
                    scrollable(container(self.tempo.view(TempoViewArgs {
                        bpm: tempo,
                        time_signature,
                        pixels_per_sample: horizontal_pixels_per_sample,
                        playhead_x,
                        punch_range_samples: None,
                        snap_mode,
                        samples_per_beat,
                        samples_per_bar: samples_per_bar as f64,
                        content_width: timeline_content_width,
                        tempo_points,
                        time_signature_points,
                        shift_pressed,
                        selected_tempo_points,
                        selected_time_signature_points,
                        timeline_left_inset_px: 0.0,
                    })))
                    .id(Id::new(PIANO_TEMPO_SCROLL_ID))
                    .direction(scrollable::Direction::Horizontal(
                        scrollable::Scrollbar::hidden(),
                    ))
                    .on_scroll(|viewport| Message::PianoScrollXChanged(
                        viewport.relative_offset().x
                    ))
                    .width(Length::Fill)
                    .height(Length::Fill),
                    container("")
                        .width(Length::Fixed(RIGHT_SCROLL_GUTTER_WIDTH))
                        .height(Length::Fill),
                ]
                .width(Length::Fill)
                .height(Length::Fixed(self.tempo.height())),
                row![
                    container("")
                        .width(Length::Fixed(TOOLS_STRIP_WIDTH + MAIN_SPLIT_SPACING,))
                        .height(Length::Fill),
                    container("")
                        .width(Length::Fixed(KEYBOARD_WIDTH))
                        .height(Length::Fill),
                    scrollable(container(self.ruler.view(RulerViewArgs {
                        playhead_x,
                        beat_pixels: horizontal_beat_pixels,
                        pixels_per_sample: horizontal_pixels_per_sample,
                        loop_range_samples: None,
                        snap_mode,
                        samples_per_beat,
                        content_width: timeline_content_width,
                        timeline_left_inset_px: 0.0,
                    })))
                    .id(Id::new(PIANO_RULER_SCROLL_ID))
                    .direction(scrollable::Direction::Horizontal(
                        scrollable::Scrollbar::hidden(),
                    ))
                    .on_scroll(|viewport| Message::PianoScrollXChanged(
                        viewport.relative_offset().x
                    ))
                    .width(Length::Fill)
                    .height(Length::Fill),
                    container("")
                        .width(Length::Fixed(RIGHT_SCROLL_GUTTER_WIDTH))
                        .height(Length::Fill),
                ]
                .width(Length::Fill)
                .height(Length::Fixed(self.ruler.height())),
                pitch_correction_content,
            ]
            .width(Length::Fill)
            .height(Length::Fill),
        )
        .style(|_theme| crate::style::app_background())
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    #[test]
    fn update_is_a_no_op() {
        let state = Arc::new(RwLock::new(crate::state::StateData::default()));
        let mut workspace = Workspace::new(state);

        workspace.update(&Message::Cancel);
    }
}
