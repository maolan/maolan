use super::VisibleTrackWindow;
use crate::{
    consts::{
        state_ids::METRONOME_TRACK_ID,
        workspace::{AUDIO_CLIP_SELECTED_BASE, MIDI_CLIP_SELECTED_BASE, MIN_TICK_SPACING_PX},
        workspace_editor::CHECKPOINTS,
    },
    message::{DraggedClip, Message, SnapMode},
    state::{ClipPeaks, MidiClipPreviewMap, State, StateData, Track},
    widget::clip::{AudioClip as AudioClipWidget, MIDIClip as MIDIClipWidget},
};
use iced::{
    Background, Border, Color, Element, Length, Point, Rectangle, Renderer, Theme, mouse,
    widget::{
        Space, Stack, canvas,
        canvas::{Frame, Geometry, Path},
        column, container, mouse_area, pin, text,
    },
};
use maolan_engine::kind::Kind;
use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
    path::PathBuf,
};

struct TrackElementViewArgs<'a> {
    state: &'a StateData,
    track: &'a Track,
    session_root: Option<&'a PathBuf>,
    pixels_per_sample: f32,
    samples_per_bar: f32,
    snap_mode: SnapMode,
    samples_per_beat: f64,
    active_clip_drag: Option<&'a DraggedClip>,
    active_target_track: Option<&'a str>,
    active_target_valid: bool,
    recording_preview_bounds: Option<(usize, usize)>,
    recording_preview_peaks: Option<&'a HashMap<String, ClipPeaks>>,
    midi_clip_previews: Option<&'a MidiClipPreviewMap>,
}

fn automation_point_color(target: crate::message::TrackAutomationTarget) -> Color {
    match target {
        crate::message::TrackAutomationTarget::Volume => Color::from_rgba(0.98, 0.78, 0.22, 0.95),
        crate::message::TrackAutomationTarget::Balance => Color::from_rgba(0.88, 0.62, 0.24, 0.95),
        crate::message::TrackAutomationTarget::Mute => Color::from_rgba(0.95, 0.45, 0.22, 0.95),
        crate::message::TrackAutomationTarget::Lv2Parameter { .. } => {
            Color::from_rgba(0.6, 0.5, 0.95, 0.95)
        }
        crate::message::TrackAutomationTarget::Vst3Parameter { .. } => {
            Color::from_rgba(0.28, 0.82, 0.78, 0.95)
        }
        crate::message::TrackAutomationTarget::ClapParameter { .. } => {
            Color::from_rgba(0.4, 0.72, 0.98, 0.95)
        }
    }
}

#[derive(Clone)]
struct TrackBarGridCanvas {
    bar_pixels: f32,
}

#[derive(Default)]
struct TrackBarGridCanvasState {
    cache: canvas::Cache,
    last_hash: Cell<u64>,
}

impl TrackBarGridCanvas {
    fn step_for_spacing(base_px: f32, min_spacing_px: f32) -> usize {
        if base_px <= 0.0 {
            return 1;
        }
        let mut step = 1usize;
        while base_px * (step as f32) < min_spacing_px {
            step *= 2;
        }
        step
    }

    fn shape_hash(&self, bounds: Rectangle) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        bounds.width.to_bits().hash(&mut hasher);
        bounds.height.to_bits().hash(&mut hasher);
        self.bar_pixels.to_bits().hash(&mut hasher);
        hasher.finish()
    }
}

impl canvas::Program<Message> for TrackBarGridCanvas {
    type State = TrackBarGridCanvasState;

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        if bounds.width <= 0.0 || bounds.height <= 0.0 {
            return vec![];
        }

        let hash = self.shape_hash(bounds);
        if state.last_hash.get() != hash {
            state.cache.clear();
            state.last_hash.set(hash);
        }

        let geom = state
            .cache
            .draw(renderer, bounds.size(), |frame: &mut Frame| {
                let step = self.bar_pixels.max(1.0)
                    * Self::step_for_spacing(self.bar_pixels, MIN_TICK_SPACING_PX) as f32;
                let color = Color::from_rgba(0.86, 0.96, 0.74, 0.14);
                let mut x = 0.0_f32;
                while x <= bounds.width + 1.0 {
                    frame.stroke(
                        &Path::line(Point::new(x, 0.0), Point::new(x, bounds.height)),
                        canvas::Stroke::default().with_color(color).with_width(1.0),
                    );
                    x += step;
                }
            });
        vec![geom]
    }
}

fn track_bar_grid_overlay(
    height: f32,
    samples_per_bar: f32,
    pixels_per_sample: f32,
) -> Element<'static, Message> {
    let bar_pixels = (samples_per_bar.max(1.0) * pixels_per_sample).max(1.0);
    canvas(TrackBarGridCanvas { bar_pixels })
        .width(Length::Fill)
        .height(Length::Fixed(height))
        .into()
}

#[derive(Clone)]
struct TrackLaneBackgroundCanvas {
    header_height: f32,
    lane_height: f32,
    audio_lanes: usize,
    midi_lanes: usize,
}

#[derive(Default)]
struct TrackLaneBackgroundCanvasState {
    cache: canvas::Cache,
    last_hash: Cell<u64>,
}

impl TrackLaneBackgroundCanvas {
    fn shape_hash(&self, bounds: Rectangle) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        bounds.width.to_bits().hash(&mut hasher);
        bounds.height.to_bits().hash(&mut hasher);
        self.header_height.to_bits().hash(&mut hasher);
        self.lane_height.to_bits().hash(&mut hasher);
        self.audio_lanes.hash(&mut hasher);
        self.midi_lanes.hash(&mut hasher);
        hasher.finish()
    }
}

impl canvas::Program<Message> for TrackLaneBackgroundCanvas {
    type State = TrackLaneBackgroundCanvasState;

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        if bounds.width <= 0.0 || bounds.height <= 0.0 {
            return vec![];
        }

        let hash = self.shape_hash(bounds);
        if state.last_hash.get() != hash {
            state.cache.clear();
            state.last_hash.set(hash);
        }

        let geom = state
            .cache
            .draw(renderer, bounds.size(), |frame: &mut Frame| {
                frame.fill(
                    &Path::rectangle(
                        Point::new(0.0, 0.0),
                        iced::Size::new(bounds.width, self.header_height),
                    ),
                    Color::from_rgba(0.08, 0.08, 0.08, 0.12),
                );

                for lane in 0..self.audio_lanes {
                    let y = self.header_height + lane as f32 * self.lane_height;
                    frame.fill(
                        &Path::rectangle(
                            Point::new(0.0, y),
                            iced::Size::new(bounds.width, self.lane_height),
                        ),
                        Color::from_rgba(0.15, 0.20, 0.28, 0.22),
                    );
                }

                for lane in 0..self.midi_lanes {
                    let y =
                        self.header_height + (self.audio_lanes + lane) as f32 * self.lane_height;
                    frame.fill(
                        &Path::rectangle(
                            Point::new(0.0, y),
                            iced::Size::new(bounds.width, self.lane_height),
                        ),
                        Color::from_rgba(0.12, 0.26, 0.14, 0.25),
                    );
                }
            });

        vec![geom]
    }
}

fn track_lane_background_overlay(
    height: f32,
    header_height: f32,
    lane_height: f32,
    audio_lanes: usize,
    midi_lanes: usize,
) -> Element<'static, Message> {
    canvas(TrackLaneBackgroundCanvas {
        header_height,
        lane_height,
        audio_lanes,
        midi_lanes,
    })
    .width(Length::Fill)
    .height(Length::Fixed(height))
    .into()
}

fn view_track_elements(args: TrackElementViewArgs<'_>) -> Element<'static, Message> {
    let TrackElementViewArgs {
        state,
        track,
        session_root,
        pixels_per_sample,
        samples_per_bar,
        snap_mode,
        samples_per_beat,
        active_clip_drag,
        active_target_track,
        active_target_valid,
        recording_preview_bounds,
        recording_preview_peaks,
        midi_clip_previews,
    } = args;
    let snap_sample = |sample: f32, delta_samples: f32| -> f32 {
        snap_mode.snap_sample_drag(
            sample as f64,
            delta_samples as f64,
            samples_per_beat,
            samples_per_bar as f64,
        ) as f32
    };
    let mut clips: Vec<Element<'static, Message>> = vec![
        mouse_area(container("").width(Length::Fill).height(Length::Fill))
            .on_press(Message::DeselectClips)
            .into(),
    ];
    let height = track.height;
    let layout = track.lane_layout();
    let lane_height = layout.lane_height.max(12.0);
    let lane_clip_height = (lane_height - 6.0).max(12.0);
    let track_name_cloned = track.name.clone();
    let mut selected_audio_indices = HashSet::new();
    let mut selected_midi_indices = HashSet::new();
    for selected in &state.selected_clips {
        if selected.track_idx != track_name_cloned {
            continue;
        }
        match selected.kind {
            Kind::Audio => {
                selected_audio_indices.insert(selected.clip_idx);
            }
            Kind::MIDI => {
                selected_midi_indices.insert(selected.clip_idx);
            }
        }
    }
    let selected_audio_count = selected_audio_indices.len();
    let selected_midi_count = selected_midi_indices.len();
    clips.push(
        pin(track_lane_background_overlay(
            height,
            layout.header_height,
            lane_height,
            track.audio.ins,
            track.midi.ins,
        ))
        .position(Point::new(0.0, 0.0))
        .into(),
    );
    clips.push(
        pin(track_bar_grid_overlay(
            height,
            samples_per_bar,
            pixels_per_sample,
        ))
        .position(Point::new(0.0, 0.0))
        .into(),
    );
    clips.push(
        pin(mouse_area(
            container("")
                .width(Length::Fill)
                .height(Length::Fixed(layout.header_height)),
        )
        .on_right_press(Message::TrackMarkerCreate(track_name_cloned.clone())))
        .position(Point::new(0.0, 0.0))
        .into(),
    );
    for (marker_index, marker) in track.editor_markers.iter().enumerate() {
        let marker_track_name = track_name_cloned.clone();
        let marker_x = marker.sample as f32 * pixels_per_sample;
        let marker_name = marker.name.trim().to_string();
        let marker_has_name = !marker_name.is_empty();
        let marker_color = Color::from_rgba(0.96, 0.72, 0.18, 0.95);
        let marker_hitbox = mouse_area(
            container(Stack::with_children(vec![
                pin(container(text(marker_name).size(10))
                    .padding([1, 4])
                    .style(|_theme| container::Style {
                        background: Some(Background::Color(Color::from_rgba(
                            0.28, 0.20, 0.06, 0.92,
                        ))),
                        text_color: Some(Color::from_rgba(0.98, 0.92, 0.72, 0.96)),
                        border: Border {
                            color: Color::from_rgba(0.78, 0.62, 0.18, 0.85),
                            width: 1.0,
                            radius: 3.0.into(),
                        },
                        ..container::Style::default()
                    }))
                .position(Point::new(10.0, 0.0))
                .into(),
                pin(container("")
                    .width(Length::Fixed(2.0))
                    .height(Length::Fixed((layout.header_height - 8.0).max(8.0)))
                    .style(move |_theme| container::Style {
                        background: Some(Background::Color(marker_color)),
                        ..container::Style::default()
                    }))
                .position(Point::new(3.0, 6.0))
                .into(),
                pin(container("")
                    .width(Length::Fixed(8.0))
                    .height(Length::Fixed(8.0))
                    .style(move |_theme| container::Style {
                        background: Some(Background::Color(marker_color)),
                        border: Border {
                            color: Color::from_rgba(0.2, 0.16, 0.04, 0.95),
                            width: 1.0,
                            radius: 2.0.into(),
                        },
                        ..container::Style::default()
                    }))
                .position(Point::new(0.0, 0.0))
                .into(),
            ]))
            .width(Length::Fixed(if marker_has_name { 112.0 } else { 8.0 }))
            .height(Length::Fixed(layout.header_height.max(12.0))),
        )
        .interaction(mouse::Interaction::ResizingHorizontally)
        .on_press(Message::TrackMarkerDragStart {
            track_name: marker_track_name.clone(),
            marker_index,
        })
        .on_right_press(Message::TrackMarkerRenameShow {
            track_name: marker_track_name.clone(),
            marker_index,
        })
        .on_middle_press(Message::TrackMarkerDelete {
            track_name: marker_track_name,
            marker_index,
        });
        clips.push(
            pin(marker_hitbox)
                .position(Point::new((marker_x - 4.0).max(0.0), 0.0))
                .into(),
        );
    }

    let visible_automation_lanes: Vec<_> = track
        .automation_lanes
        .iter()
        .filter(|lane| lane.visible)
        .collect();
    for (lane_index, lane) in visible_automation_lanes.iter().enumerate() {
        let lane_top = track.automation_lane_top(lane_index);
        let lane_track_name = track_name_cloned.clone();
        let lane_target = lane.target;
        clips.push(
            pin(mouse_area(
                container("")
                    .width(Length::Fill)
                    .height(Length::Fixed(lane_height))
                    .style(|_theme| container::Style {
                        background: Some(Background::Color(Color {
                            r: 0.26,
                            g: 0.18,
                            b: 0.1,
                            a: 0.22,
                        })),
                        ..container::Style::default()
                    }),
            )
            .on_move(move |position| Message::TrackAutomationLaneHover {
                track_name: lane_track_name.clone(),
                target: lane_target,
                position,
            })
            .on_press(Message::TrackAutomationLaneInsertPoint {
                track_name: track_name_cloned.clone(),
                target: lane.target,
            }))
            .position(Point::new(0.0, lane_top))
            .into(),
        );
        clips.push(
            pin(
                container(text(format!("Automation {}", lane.target)).size(10))
                    .width(Length::Shrink)
                    .height(Length::Fixed(12.0))
                    .padding([1, 4])
                    .style(|_theme| container::Style {
                        background: Some(Background::Color(Color {
                            r: 0.35,
                            g: 0.24,
                            b: 0.12,
                            a: 0.45,
                        })),
                        ..container::Style::default()
                    }),
            )
            .position(Point::new(4.0, lane_top + 2.0))
            .into(),
        );

        let point_color = automation_point_color(lane.target);
        let mut sorted_indices: Vec<usize> = (0..lane.points.len()).collect();
        sorted_indices.sort_unstable_by_key(|&idx| lane.points[idx].sample);
        for pair in sorted_indices.windows(2) {
            let left = &lane.points[pair[0]];
            let right = &lane.points[pair[1]];
            let left_x = left.sample as f32 * pixels_per_sample;
            let right_x = right.sample as f32 * pixels_per_sample;
            let left_y =
                lane_top + 3.0 + (lane_clip_height - 2.0) * (1.0 - left.value.clamp(0.0, 1.0));
            let right_y =
                lane_top + 3.0 + (lane_clip_height - 2.0) * (1.0 - right.value.clamp(0.0, 1.0));
            let width = (right_x - left_x).abs().max(1.0);
            let min_y = left_y.min(right_y);
            clips.push(
                pin(container("")
                    .width(Length::Fixed(width))
                    .height(Length::Fixed(1.0))
                    .style(move |_theme| container::Style {
                        background: Some(Background::Color(point_color)),
                        ..container::Style::default()
                    }))
                .position(Point::new(left_x.min(right_x), min_y))
                .into(),
            );
        }
        for point in &lane.points {
            let clamped_value = point.value.clamp(0.0, 1.0);
            let x = point.sample as f32 * pixels_per_sample;
            let y = lane_top + 3.0 + (lane_clip_height - 2.0) * (1.0 - clamped_value);
            let point_track_name = track_name_cloned.clone();
            let point_target = lane.target;
            let point_sample = point.sample;
            clips.push(
                pin(mouse_area(
                    container("")
                        .width(Length::Fixed(5.0))
                        .height(Length::Fixed(5.0))
                        .style(move |_theme| container::Style {
                            background: Some(Background::Color(point_color)),
                            border: Border {
                                color: Color::from_rgba(0.1, 0.1, 0.1, 0.9),
                                width: 1.0,
                                radius: 2.5.into(),
                            },
                            ..container::Style::default()
                        }),
                )
                .on_right_press(Message::TrackAutomationLaneDeletePoint {
                    track_name: point_track_name,
                    target: point_target,
                    sample: point_sample,
                }))
                .position(Point::new((x - 2.0).max(0.0), y.max(lane_top + 2.0)))
                .into(),
            );
        }
    }

    for (index, clip) in track.audio.clips.iter().enumerate() {
        let clip_name = clip.name.clone();
        let clip_label = format!(
            "{}{}{}",
            AudioClipWidget::clean_name(&clip_name),
            if clip.take_lane_pinned { " [P]" } else { "" },
            if clip.take_lane_locked { " [L]" } else { "" }
        );
        let is_selected = selected_audio_indices.contains(&index);
        let active_drag = active_clip_drag.filter(|d| {
            d.kind == Kind::Audio && d.track_index == track_name_cloned && d.index == index
        });
        let group_drag = active_clip_drag.filter(|d| {
            d.kind == Kind::Audio
                && d.track_index == track_name_cloned
                && selected_audio_indices.contains(&d.index)
                && selected_audio_count > 1
                && is_selected
        });
        let drag_for_clip = group_drag.or(active_drag);
        let dragged_to_other_track = drag_for_clip.is_some_and(|d| {
            !d.copy
                && active_target_valid
                && active_target_track.is_some_and(|target| target != track_name_cloned.as_str())
        });
        let show_preview_in_this_track = drag_for_clip.is_some_and(|_| {
            active_target_track.is_some_and(|target| target == track_name_cloned.as_str())
        });
        let dragged_start = drag_for_clip
            .filter(|d| !d.copy && !show_preview_in_this_track)
            .map(|d| {
                let delta_samples = (d.end.x - d.start.x) / pixels_per_sample.max(1.0e-6);
                snap_sample(clip.start as f32 + delta_samples, delta_samples)
            })
            .unwrap_or(clip.start as f32);
        // All audio clips are displayed on lane 0 (single audio lane)
        let lane = 0;
        let lane_top_base = track.lane_top(Kind::Audio, lane) + 3.0;
        let lane_top = lane_top_base + 1.0;
        let clip_width = (clip.length as f32 * pixels_per_sample).max(12.0);
        let clip_height = (lane_clip_height - 2.0).max(8.0);
        let display_clip_label = AudioClipWidget::label_for_width(&clip_label, clip_width);
        let audio_left_handle_hovered = state.hovered_clip_resize_handle.as_ref().is_some_and(
            |(track_idx, clip_idx, kind, is_right_side)| {
                track_idx == &track_name_cloned
                    && *clip_idx == index
                    && *kind == Kind::Audio
                    && !*is_right_side
            },
        );
        let audio_right_handle_hovered = state.hovered_clip_resize_handle.as_ref().is_some_and(
            |(track_idx, clip_idx, kind, is_right_side)| {
                track_idx == &track_name_cloned
                    && *clip_idx == index
                    && *kind == Kind::Audio
                    && *is_right_side
            },
        );

        if !dragged_to_other_track {
            clips.push(
                pin(AudioClipWidget::new(clip)
                    .with_session_root(session_root)
                    .with_pixels_per_sample(pixels_per_sample)
                    .with_size(clip_width, clip_height)
                    .with_label(display_clip_label.clone())
                    .selected(is_selected)
                    .hovered_handles(audio_left_handle_hovered, audio_right_handle_hovered)
                    .interactive(
                        track_name_cloned.clone(),
                        index,
                        Message::SelectClip {
                            track_idx: track_name_cloned.clone(),
                            clip_idx: index,
                            kind: Kind::Audio,
                        },
                        Message::OpenClipPlugins {
                            track_idx: track_name_cloned.clone(),
                            clip_idx: index,
                        },
                        !clip.take_lane_locked,
                    )
                    .into_element())
                .position(Point::new(dragged_start * pixels_per_sample, lane_top))
                .into(),
            );
        }

        if let Some(drag) = drag_for_clip.filter(|_| show_preview_in_this_track) {
            let delta_samples = (drag.end.x - drag.start.x) / pixels_per_sample.max(1.0e-6);
            let preview_start = snap_sample(clip.start as f32 + delta_samples, delta_samples);
            let preview_fill = if active_target_valid {
                Background::Color(Color {
                    r: 0.72,
                    g: 0.86,
                    b: 1.0,
                    a: 0.7,
                })
            } else {
                Background::Color(Color {
                    r: 0.92,
                    g: 0.32,
                    b: 0.32,
                    a: 0.55,
                })
            };
            let preview_border = if active_target_valid {
                Color {
                    r: 0.98,
                    g: 0.98,
                    b: 0.98,
                    a: 0.9,
                }
            } else {
                Color {
                    r: 1.0,
                    g: 0.45,
                    b: 0.45,
                    a: 0.95,
                }
            };
            clips.push(
                pin(AudioClipWidget::new(clip)
                    .with_session_root(session_root)
                    .with_size(clip_width, clip_height)
                    .with_label(display_clip_label.clone())
                    .preview(preview_fill, preview_border)
                    .into_element())
                .position(Point::new(preview_start * pixels_per_sample, lane_top))
                .into(),
            );
        }
    }
    for (index, clip) in track.midi.clips.iter().enumerate() {
        let clip_name = clip.name.clone();
        let clip_label = format!(
            "{}{}{}",
            MIDIClipWidget::clean_name(&clip_name),
            if clip.take_lane_pinned { " [P]" } else { "" },
            if clip.take_lane_locked { " [L]" } else { "" }
        );
        let is_selected = selected_midi_indices.contains(&index);
        let active_drag = active_clip_drag.filter(|d| {
            d.kind == Kind::MIDI && d.track_index == track_name_cloned && d.index == index
        });
        let group_drag = active_clip_drag.filter(|d| {
            d.kind == Kind::MIDI
                && d.track_index == track_name_cloned
                && selected_midi_indices.contains(&d.index)
                && selected_midi_count > 1
                && is_selected
        });
        let drag_for_clip = group_drag.or(active_drag);
        let dragged_to_other_track = drag_for_clip.is_some_and(|d| {
            !d.copy
                && active_target_valid
                && active_target_track.is_some_and(|target| target != track_name_cloned.as_str())
        });
        let show_preview_in_this_track = drag_for_clip.is_some_and(|_| {
            active_target_track.is_some_and(|target| target == track_name_cloned.as_str())
        });
        let dragged_start = drag_for_clip
            .filter(|d| !d.copy && !show_preview_in_this_track)
            .map(|d| {
                let delta_samples = (d.end.x - d.start.x) / pixels_per_sample.max(1.0e-6);
                snap_sample(clip.start as f32 + delta_samples, delta_samples)
            })
            .unwrap_or(clip.start as f32);
        let lane = clip.input_channel.min(track.midi.ins.saturating_sub(1));
        let lane_top_base = track.lane_top(Kind::MIDI, lane) + 3.0;
        let lane_top = lane_top_base + 1.0;
        let clip_width = (clip.length as f32 * pixels_per_sample).max(12.0);
        let clip_height = (lane_clip_height - 2.0).max(8.0);
        let display_clip_label = MIDIClipWidget::label_for_width(&clip_label, clip_width);
        let midi_left_handle_hovered = state.hovered_clip_resize_handle.as_ref().is_some_and(
            |(track_idx, clip_idx, kind, is_right_side)| {
                track_idx == &track_name_cloned
                    && *clip_idx == index
                    && *kind == Kind::MIDI
                    && !*is_right_side
            },
        );
        let midi_right_handle_hovered = state.hovered_clip_resize_handle.as_ref().is_some_and(
            |(track_idx, clip_idx, kind, is_right_side)| {
                track_idx == &track_name_cloned
                    && *clip_idx == index
                    && *kind == Kind::MIDI
                    && *is_right_side
            },
        );
        let midi_notes_for_clip = midi_clip_previews
            .and_then(|map| map.get(&(track_name_cloned.clone(), index)))
            .cloned();

        if !dragged_to_other_track {
            clips.push(
                pin(MIDIClipWidget::new(clip)
                    .with_size(clip_width, clip_height)
                    .with_label(display_clip_label.clone())
                    .selected(is_selected)
                    .hovered_handles(midi_left_handle_hovered, midi_right_handle_hovered)
                    .with_notes(midi_notes_for_clip.clone())
                    .interactive(
                        track_name_cloned.clone(),
                        index,
                        Message::SelectClip {
                            track_idx: track_name_cloned.clone(),
                            clip_idx: index,
                            kind: Kind::MIDI,
                        },
                        Message::OpenMidiPiano {
                            track_idx: track_name_cloned.clone(),
                            clip_idx: index,
                        },
                        !clip.take_lane_locked,
                    )
                    .into_element())
                .position(Point::new(dragged_start * pixels_per_sample, lane_top))
                .into(),
            );
        }

        if let Some(drag) = drag_for_clip.filter(|_| show_preview_in_this_track) {
            let delta_samples = (drag.end.x - drag.start.x) / pixels_per_sample.max(1.0e-6);
            let preview_start = snap_sample(clip.start as f32 + delta_samples, delta_samples);
            let preview_fill = if active_target_valid {
                MIDIClipWidget::two_edge_gradient(MIDI_CLIP_SELECTED_BASE, 0.66, 0.66, false)
            } else {
                MIDIClipWidget::two_edge_gradient(
                    Color {
                        r: 0.72,
                        g: 0.18,
                        b: 0.18,
                        a: 1.0,
                    },
                    0.72,
                    0.72,
                    false,
                )
            };
            let preview_border = if active_target_valid {
                Color {
                    r: 0.88,
                    g: 1.0,
                    b: 0.78,
                    a: 0.92,
                }
            } else {
                Color {
                    r: 1.0,
                    g: 0.45,
                    b: 0.45,
                    a: 0.95,
                }
            };
            clips.push(
                pin(MIDIClipWidget::new(clip)
                    .with_size(clip_width, clip_height)
                    .with_label(display_clip_label.clone())
                    .with_notes(midi_notes_for_clip.clone())
                    .preview(preview_fill, preview_border, 8.0)
                    .into_element())
                .position(Point::new(preview_start * pixels_per_sample, lane_top))
                .into(),
            );
        }
    }

    if let Some(drag) = active_clip_drag
        && let Some(target) = active_target_track
        && target == track_name_cloned.as_str()
        && drag.track_index != track_name_cloned
    {
        let delta_samples = (drag.end.x - drag.start.x) / pixels_per_sample.max(1.0e-6);
        if let Some(source_track) = state.tracks.iter().find(|t| t.name == drag.track_index) {
            match drag.kind {
                Kind::Audio => {
                    let mut preview_indices: Vec<usize> = state
                        .selected_clips
                        .iter()
                        .filter(|id| {
                            id.kind == Kind::Audio
                                && id.track_idx == drag.track_index
                                && id.clip_idx < source_track.audio.clips.len()
                        })
                        .map(|id| id.clip_idx)
                        .collect();
                    preview_indices.sort_unstable();
                    preview_indices.dedup();
                    if preview_indices.len() <= 1 || !preview_indices.contains(&drag.index) {
                        preview_indices = vec![drag.index];
                    }
                    for clip_index in preview_indices {
                        let Some(source_clip) = source_track.audio.clips.get(clip_index) else {
                            continue;
                        };
                        let preview_fill = if active_target_valid {
                            AudioClipWidget::two_edge_gradient(
                                AUDIO_CLIP_SELECTED_BASE,
                                0.7,
                                0.7,
                                true,
                            )
                        } else {
                            AudioClipWidget::two_edge_gradient(
                                Color {
                                    r: 0.72,
                                    g: 0.18,
                                    b: 0.18,
                                    a: 1.0,
                                },
                                0.72,
                                0.72,
                                true,
                            )
                        };
                        let preview_border = if active_target_valid {
                            Color {
                                r: 0.98,
                                g: 0.98,
                                b: 0.98,
                                a: 0.9,
                            }
                        } else {
                            Color {
                                r: 1.0,
                                g: 0.45,
                                b: 0.45,
                                a: 0.95,
                            }
                        };
                        let clip_width = (source_clip.length as f32 * pixels_per_sample).max(12.0);
                        let clip_height = lane_clip_height;
                        let lane_top = if active_target_valid || track.audio.ins > 0 {
                            track.lane_top(Kind::Audio, 0) + 3.0
                        } else if track.midi.ins > 0 {
                            track.lane_top(Kind::MIDI, 0) + 3.0
                        } else {
                            track.lane_layout().header_height + 3.0
                        };
                        let preview_start =
                            snap_sample(source_clip.start as f32 + delta_samples, delta_samples);
                        let display_clip_label = AudioClipWidget::label_for_width(
                            &AudioClipWidget::clean_name(&source_clip.name),
                            clip_width,
                        );
                        clips.push(
                            pin(AudioClipWidget::new(source_clip)
                                .with_session_root(session_root)
                                .with_size(clip_width, clip_height)
                                .with_label(display_clip_label)
                                .preview(
                                    if active_target_valid {
                                        preview_fill
                                    } else {
                                        Background::Color(Color::TRANSPARENT)
                                    },
                                    preview_border,
                                )
                                .into_element())
                            .position(Point::new(preview_start * pixels_per_sample, lane_top))
                            .into(),
                        );
                    }
                }
                Kind::MIDI => {
                    let mut preview_indices: Vec<usize> = state
                        .selected_clips
                        .iter()
                        .filter(|id| {
                            id.kind == Kind::MIDI
                                && id.track_idx == drag.track_index
                                && id.clip_idx < source_track.midi.clips.len()
                        })
                        .map(|id| id.clip_idx)
                        .collect();
                    preview_indices.sort_unstable();
                    preview_indices.dedup();
                    if preview_indices.len() <= 1 || !preview_indices.contains(&drag.index) {
                        preview_indices = vec![drag.index];
                    }
                    for clip_index in preview_indices {
                        let Some(source_clip) = source_track.midi.clips.get(clip_index) else {
                            continue;
                        };
                        let preview_fill = if active_target_valid {
                            MIDIClipWidget::two_edge_gradient(
                                MIDI_CLIP_SELECTED_BASE,
                                0.7,
                                0.7,
                                false,
                            )
                        } else {
                            MIDIClipWidget::two_edge_gradient(
                                Color {
                                    r: 0.72,
                                    g: 0.18,
                                    b: 0.18,
                                    a: 1.0,
                                },
                                0.72,
                                0.72,
                                false,
                            )
                        };
                        let preview_border = if active_target_valid {
                            Color {
                                r: 0.98,
                                g: 0.98,
                                b: 0.98,
                                a: 0.9,
                            }
                        } else {
                            Color {
                                r: 1.0,
                                g: 0.45,
                                b: 0.45,
                                a: 0.95,
                            }
                        };
                        let clip_width = (source_clip.length as f32 * pixels_per_sample).max(12.0);
                        let lane_top = if active_target_valid && track.midi.ins > 0 {
                            let lane = source_clip
                                .input_channel
                                .min(track.midi.ins.saturating_sub(1));
                            track.lane_top(Kind::MIDI, lane) + 3.0
                        } else if track.audio.ins > 0 {
                            track.lane_top(Kind::Audio, 0) + 3.0
                        } else if track.midi.ins > 0 {
                            track.lane_top(Kind::MIDI, 0) + 3.0
                        } else {
                            track.lane_layout().header_height + 3.0
                        };
                        let preview_start =
                            snap_sample(source_clip.start as f32 + delta_samples, delta_samples);
                        let display_clip_label = MIDIClipWidget::label_for_width(
                            &MIDIClipWidget::clean_name(&source_clip.name),
                            clip_width,
                        );
                        clips.push(
                            pin(MIDIClipWidget::new(source_clip)
                                .with_size(clip_width, lane_clip_height)
                                .with_label(display_clip_label)
                                .preview(
                                    if active_target_valid {
                                        preview_fill
                                    } else {
                                        Background::Color(Color::TRANSPARENT)
                                    },
                                    preview_border,
                                    3.0,
                                )
                                .into_element())
                            .position(Point::new(preview_start * pixels_per_sample, lane_top))
                            .into(),
                        );
                    }
                }
            }
        }
    }

    if track.armed
        && let Some((preview_start, preview_current)) = recording_preview_bounds
        && preview_current > preview_start
    {
        let preview_width =
            ((preview_current - preview_start) as f32 * pixels_per_sample).max(12.0);
        let preview_top = track.lane_top(Kind::Audio, 0) + 3.0;
        let preview_peaks = recording_preview_peaks
            .and_then(|map| map.get(&track.name))
            .cloned()
            .unwrap_or_default();
        let preview_length = preview_current - preview_start;
        let preview_clip = container(
            container(Stack::with_children(vec![
                AudioClipWidget::waveform_overlay(
                    preview_peaks,
                    None,
                    0,
                    preview_length,
                    preview_length,
                ),
                container(text("REC").size(12))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .padding(5)
                    .into(),
            ]))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(0)
            .style(|_theme| container::Style {
                background: Some(Background::Color(Color {
                    r: 0.85,
                    g: 0.25,
                    b: 0.25,
                    a: 0.35,
                })),
                ..container::Style::default()
            }),
        )
        .width(Length::Fixed(preview_width))
        .height(Length::Fill)
        .style(|_theme| container::Style {
            background: None,
            border: Border {
                color: Color {
                    r: 0.9,
                    g: 0.3,
                    b: 0.3,
                    a: 0.9,
                },
                width: 1.0,
                radius: 3.0.into(),
            },
            ..container::Style::default()
        });
        clips.push(
            pin(preview_clip)
                .position(Point::new(
                    preview_start as f32 * pixels_per_sample,
                    preview_top,
                ))
                .into(),
        );
    }
    container(
        Stack::from_vec(clips)
            .height(Length::Fill)
            .width(Length::Fill),
    )
    .id(track_name_cloned)
    .width(Length::Fill)
    .height(Length::Fixed(height))
    .style(|_theme| container::Style {
        background: Some(Background::Color(Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        })),
        border: Border {
            color: Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
            width: 1.0,
            radius: 0.0.into(),
        },
        ..container::Style::default()
    })
    .into()
}

pub(super) fn clip_context_menu_overlay(
    state: &StateData,
    transport_active: bool,
) -> Option<(Point, Element<'static, Message>)> {
    fn can_group_selected_clips(state: &StateData, menu_clip: &crate::state::ClipId) -> bool {
        let mut selected: Vec<_> = state.selected_clips.iter().collect();
        if selected.len() < 2 {
            return false;
        }
        if !selected.contains(&menu_clip) {
            return false;
        }
        selected.sort_by_key(|clip| clip.clip_idx);
        let Some(first) = selected.first() else {
            return false;
        };
        if selected
            .iter()
            .any(|clip| clip.track_idx != first.track_idx || clip.kind != first.kind)
        {
            return false;
        }
        let Some(track) = state
            .tracks
            .iter()
            .find(|track| track.name == first.track_idx)
        else {
            return false;
        };
        match first.kind {
            Kind::Audio => selected.iter().all(|clip| {
                track
                    .audio
                    .clips
                    .get(clip.clip_idx)
                    .is_some_and(|clip| !clip.is_group())
            }),
            Kind::MIDI => selected.iter().all(|clip| {
                track
                    .midi
                    .clips
                    .get(clip.clip_idx)
                    .is_some_and(|clip| !clip.is_group())
            }),
        }
    }

    let menu = state.clip_context_menu.as_ref()?;
    let clip = &menu.clip;
    let track = state.tracks.iter().find(|t| t.name == clip.track_idx)?;

    let content: Element<'static, Message> = match clip.kind {
        Kind::Audio => {
            let clip_ref = track.audio.clips.get(clip.clip_idx)?;
            let fade_enabled = clip_ref.fade_enabled;
            let muted = clip_ref.muted;
            let is_group = clip_ref.is_group();
            let track_idx = clip.track_idx.clone();
            let clip_idx = clip.clip_idx;
            column![
                crate::menu::menu_item_maybe(
                    if is_group { "Ungroup" } else { "Group" },
                    if is_group {
                        Some(Message::UngroupClip {
                            track_idx: track_idx.clone(),
                            clip_idx,
                            kind: Kind::Audio,
                        })
                    } else {
                        can_group_selected_clips(state, clip).then_some(Message::GroupSelectedClips)
                    },
                ),
                crate::menu::menu_item(
                    "Rename",
                    Message::ClipRenameShow {
                        track_idx: track_idx.clone(),
                        clip_idx,
                        kind: Kind::Audio,
                    },
                ),
                crate::menu::menu_item(
                    if muted { "Unmute" } else { "Mute" },
                    Message::ClipSetMuted {
                        track_idx: track_idx.clone(),
                        clip_idx,
                        kind: Kind::Audio,
                        muted: !muted,
                    },
                ),
                crate::menu::menu_item_maybe(
                    "Pitch Correction",
                    (!transport_active).then_some(Message::ClipOpenPitchCorrection {
                        track_idx: track_idx.clone(),
                        clip_idx,
                    }),
                ),
                crate::menu::menu_item(
                    if fade_enabled {
                        "Disable Fade"
                    } else {
                        "Enable Fade"
                    },
                    Message::ClipToggleFade {
                        track_idx,
                        clip_idx,
                        kind: Kind::Audio,
                    },
                ),
            ]
            .spacing(2)
            .into()
        }
        Kind::MIDI => {
            let clip_ref = track.midi.clips.get(clip.clip_idx)?;
            let muted = clip_ref.muted;
            let is_group = clip_ref.is_group();
            let track_idx = clip.track_idx.clone();
            let clip_idx = clip.clip_idx;
            column![
                crate::menu::menu_item_maybe(
                    if is_group { "Ungroup" } else { "Group" },
                    if is_group {
                        Some(Message::UngroupClip {
                            track_idx: track_idx.clone(),
                            clip_idx,
                            kind: Kind::MIDI,
                        })
                    } else {
                        can_group_selected_clips(state, clip).then_some(Message::GroupSelectedClips)
                    },
                ),
                crate::menu::menu_item(
                    "Rename",
                    Message::ClipRenameShow {
                        track_idx: track_idx.clone(),
                        clip_idx,
                        kind: Kind::MIDI,
                    },
                ),
                crate::menu::menu_item(
                    if muted { "Unmute" } else { "Mute" },
                    Message::ClipSetMuted {
                        track_idx: track_idx.clone(),
                        clip_idx,
                        kind: Kind::MIDI,
                        muted: !muted,
                    },
                ),
            ]
            .spacing(2)
            .into()
        }
    };

    let panel = container(content)
        .width(Length::Fixed(210.0))
        .padding(6)
        .style(|theme| {
            let palette = theme.extended_palette();
            container::Style {
                background: Some(Background::Color(palette.background.weak.color)),
                border: Border {
                    color: palette.background.strong.color,
                    width: 1.0,
                    radius: 6.0.into(),
                },
                ..container::Style::default()
            }
        })
        .into();

    Some((menu.anchor, panel))
}

#[derive(Debug, Clone)]
pub struct Editor {
    state: State,
}

pub struct EditorViewArgs<'a> {
    pub session_root: Option<&'a PathBuf>,
    pub pixels_per_sample: f32,
    pub samples_per_bar: f32,
    pub snap_mode: SnapMode,
    pub samples_per_beat: f64,
    pub active_clip_drag: Option<&'a DraggedClip>,
    pub active_target_track: Option<&'a str>,
    pub active_target_valid: bool,
    pub recording_preview_bounds: Option<(usize, usize)>,
    pub recording_preview_peaks: Option<&'a HashMap<String, ClipPeaks>>,
    pub midi_clip_previews: Option<&'a MidiClipPreviewMap>,
    pub visible_track_window: VisibleTrackWindow,
}

#[derive(Clone)]
pub struct OwnedEditorViewArgs {
    pub session_root: Option<PathBuf>,
    pub pixels_per_sample: f32,
    pub samples_per_bar: f32,
    pub snap_mode: SnapMode,
    pub samples_per_beat: f64,
    pub active_clip_drag: Option<DraggedClip>,
    pub active_target_track: Option<String>,
    pub active_target_valid: bool,
    pub recording_preview_bounds: Option<(usize, usize)>,
    pub recording_preview_peaks: Option<HashMap<String, ClipPeaks>>,
    pub midi_clip_previews: Option<MidiClipPreviewMap>,
    pub visible_track_window: VisibleTrackWindow,
}

impl Editor {
    pub fn new(state: State) -> Self {
        Self { state }
    }

    pub fn render_hash(&self, args: &EditorViewArgs<'_>) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        let state = self.state.blocking_read();

        args.session_root.hash(&mut hasher);
        args.pixels_per_sample.to_bits().hash(&mut hasher);
        args.samples_per_bar.to_bits().hash(&mut hasher);
        args.samples_per_beat.to_bits().hash(&mut hasher);
        std::mem::discriminant(&args.snap_mode).hash(&mut hasher);
        args.recording_preview_bounds.hash(&mut hasher);
        args.active_target_track.hash(&mut hasher);
        args.active_target_valid.hash(&mut hasher);
        args.visible_track_window.start_index.hash(&mut hasher);
        args.visible_track_window.end_index.hash(&mut hasher);
        args.visible_track_window
            .top_padding
            .to_bits()
            .hash(&mut hasher);
        args.visible_track_window
            .bottom_padding
            .to_bits()
            .hash(&mut hasher);
        if let Some(drag) = args.active_clip_drag {
            std::mem::discriminant(&drag.kind).hash(&mut hasher);
            drag.index.hash(&mut hasher);
            drag.track_index.hash(&mut hasher);
            drag.start.x.to_bits().hash(&mut hasher);
            drag.start.y.to_bits().hash(&mut hasher);
            drag.end.x.to_bits().hash(&mut hasher);
            drag.end.y.to_bits().hash(&mut hasher);
            drag.copy.hash(&mut hasher);
        }

        let mut selected_clips: Vec<_> = state.selected_clips.iter().collect();
        selected_clips.sort_by(|a, b| {
            a.track_idx
                .cmp(&b.track_idx)
                .then_with(|| {
                    let ak = match a.kind {
                        Kind::Audio => 0_u8,
                        Kind::MIDI => 1_u8,
                    };
                    let bk = match b.kind {
                        Kind::Audio => 0_u8,
                        Kind::MIDI => 1_u8,
                    };
                    ak.cmp(&bk)
                })
                .then_with(|| a.clip_idx.cmp(&b.clip_idx))
        });
        for clip in selected_clips {
            clip.track_idx.hash(&mut hasher);
            clip.clip_idx.hash(&mut hasher);
            std::mem::discriminant(&clip.kind).hash(&mut hasher);
        }

        state.hovered_clip_resize_handle.hash(&mut hasher);
        state
            .clip_marquee_start
            .map(|p| (p.x.to_bits(), p.y.to_bits()))
            .hash(&mut hasher);
        state
            .clip_marquee_end
            .map(|p| (p.x.to_bits(), p.y.to_bits()))
            .hash(&mut hasher);
        state
            .midi_clip_create_start
            .map(|p| (p.x.to_bits(), p.y.to_bits()))
            .hash(&mut hasher);
        state
            .midi_clip_create_end
            .map(|p| (p.x.to_bits(), p.y.to_bits()))
            .hash(&mut hasher);

        if let Some(menu) = state.clip_context_menu.as_ref() {
            menu.clip.track_idx.hash(&mut hasher);
            menu.clip.clip_idx.hash(&mut hasher);
            std::mem::discriminant(&menu.clip.kind).hash(&mut hasher);
            menu.anchor.x.to_bits().hash(&mut hasher);
            menu.anchor.y.to_bits().hash(&mut hasher);
        }

        for track in state
            .tracks
            .iter()
            .filter(|track| track.name != METRONOME_TRACK_ID)
            .skip(args.visible_track_window.start_index)
            .take(
                args.visible_track_window
                    .end_index
                    .saturating_sub(args.visible_track_window.start_index),
            )
        {
            track.name.hash(&mut hasher);
            track.height.to_bits().hash(&mut hasher);
            track.armed.hash(&mut hasher);
            track.audio.ins.hash(&mut hasher);
            track.midi.ins.hash(&mut hasher);
            track.editor_markers.hash(&mut hasher);
            track.midi_lane_channels.hash(&mut hasher);
            std::mem::discriminant(&track.automation_mode).hash(&mut hasher);

            for lane in &track.automation_lanes {
                lane.visible.hash(&mut hasher);
                std::mem::discriminant(&lane.target).hash(&mut hasher);
                lane.points.len().hash(&mut hasher);
                if let Some(first) = lane.points.first() {
                    first.sample.hash(&mut hasher);
                    first.value.to_bits().hash(&mut hasher);
                }
                if let Some(last) = lane.points.last() {
                    last.sample.hash(&mut hasher);
                    last.value.to_bits().hash(&mut hasher);
                }
            }

            for clip in &track.audio.clips {
                clip.name.hash(&mut hasher);
                clip.start.hash(&mut hasher);
                clip.length.hash(&mut hasher);
                clip.offset.hash(&mut hasher);
                clip.input_channel.hash(&mut hasher);
                clip.muted.hash(&mut hasher);
                clip.fade_enabled.hash(&mut hasher);
                clip.fade_in_samples.hash(&mut hasher);
                clip.fade_out_samples.hash(&mut hasher);
                clip.take_lane_override.hash(&mut hasher);
                clip.take_lane_pinned.hash(&mut hasher);
                clip.take_lane_locked.hash(&mut hasher);
                clip.peaks.len().hash(&mut hasher);
                for channel in clip.peaks.iter() {
                    channel.len().hash(&mut hasher);
                    if channel.is_empty() {
                        continue;
                    }
                    for i in 0..CHECKPOINTS {
                        let idx = (i * channel.len()) / CHECKPOINTS;
                        let sample = channel[idx.min(channel.len() - 1)];
                        sample[0].to_bits().hash(&mut hasher);
                        sample[1].to_bits().hash(&mut hasher);
                    }
                }
            }

            for clip in &track.midi.clips {
                clip.name.hash(&mut hasher);
                clip.start.hash(&mut hasher);
                clip.length.hash(&mut hasher);
                clip.offset.hash(&mut hasher);
                clip.input_channel.hash(&mut hasher);
                clip.muted.hash(&mut hasher);
                clip.take_lane_override.hash(&mut hasher);
                clip.take_lane_pinned.hash(&mut hasher);
                clip.take_lane_locked.hash(&mut hasher);
            }
        }

        if let Some(peaks_by_track) = args.recording_preview_peaks {
            let mut keys: Vec<_> = peaks_by_track.keys().collect();
            keys.sort_unstable();
            for key in keys {
                key.hash(&mut hasher);
                if let Some(peaks) = peaks_by_track.get(key) {
                    peaks.len().hash(&mut hasher);
                    for channel in peaks.iter() {
                        channel.len().hash(&mut hasher);
                        if let Some(first) = channel.first() {
                            first[0].to_bits().hash(&mut hasher);
                            first[1].to_bits().hash(&mut hasher);
                        }
                        if let Some(last) = channel.last() {
                            last[0].to_bits().hash(&mut hasher);
                            last[1].to_bits().hash(&mut hasher);
                        }
                    }
                }
            }
        }

        if let Some(previews) = args.midi_clip_previews {
            let mut keys: Vec<_> = previews.keys().collect();
            keys.sort_unstable();
            for (track_name, clip_index) in keys {
                track_name.hash(&mut hasher);
                clip_index.hash(&mut hasher);
                if let Some(notes) = previews.get(&(track_name.clone(), *clip_index)) {
                    notes.len().hash(&mut hasher);
                    if let Some(first) = notes.first() {
                        first.start_sample.hash(&mut hasher);
                        first.length_samples.hash(&mut hasher);
                        first.pitch.hash(&mut hasher);
                        first.velocity.hash(&mut hasher);
                    }
                    if let Some(last) = notes.last() {
                        last.start_sample.hash(&mut hasher);
                        last.length_samples.hash(&mut hasher);
                        last.pitch.hash(&mut hasher);
                        last.velocity.hash(&mut hasher);
                    }
                }
            }
        }

        hasher.finish()
    }

    pub fn into_view_owned(self, args: OwnedEditorViewArgs) -> Element<'static, Message> {
        let OwnedEditorViewArgs {
            session_root,
            pixels_per_sample,
            samples_per_bar,
            snap_mode,
            samples_per_beat,
            active_clip_drag,
            active_target_track,
            active_target_valid,
            recording_preview_bounds,
            recording_preview_peaks,
            midi_clip_previews,
            visible_track_window,
        } = args;
        let state_handle = self.state;
        let session_root_ref = session_root.as_ref();
        let active_clip_drag_ref = active_clip_drag.as_ref();
        let active_target_track_ref = active_target_track.as_deref();
        let recording_preview_peaks_ref = recording_preview_peaks.as_ref();
        let midi_clip_previews_ref = midi_clip_previews.as_ref();

        let mut result = column![];
        if visible_track_window.top_padding > 0.0 {
            result =
                result.push(Space::new().height(Length::Fixed(visible_track_window.top_padding)));
        }
        let state = state_handle.blocking_read();
        for track in state
            .tracks
            .iter()
            .filter(|track| track.name != METRONOME_TRACK_ID)
            .skip(visible_track_window.start_index)
            .take(
                visible_track_window
                    .end_index
                    .saturating_sub(visible_track_window.start_index),
            )
        {
            result = result.push(view_track_elements(TrackElementViewArgs {
                state: &state,
                track,
                session_root: session_root_ref,
                pixels_per_sample,
                samples_per_bar,
                snap_mode,
                samples_per_beat,
                active_clip_drag: active_clip_drag_ref,
                active_target_track: active_target_track_ref,
                active_target_valid,
                recording_preview_bounds,
                recording_preview_peaks: recording_preview_peaks_ref,
                midi_clip_previews: midi_clip_previews_ref,
            }));
        }
        if visible_track_window.bottom_padding > 0.0 {
            result = result
                .push(Space::new().height(Length::Fixed(visible_track_window.bottom_padding)));
        }
        let mut layers: Vec<Element<'static, Message>> =
            vec![result.width(Length::Fill).height(Length::Fill).into()];
        if let (Some(start), Some(end)) = (state.clip_marquee_start, state.clip_marquee_end) {
            let mut x = start.x.min(end.x);
            let mut y = start.y.min(end.y);
            let mut w = (start.x - end.x).abs();
            let mut h = (start.y - end.y).abs();
            if w > 1.0 || h > 1.0 {
                w = w.max(2.0);
                h = h.max(2.0);
                x = x.max(0.0);
                y = y.max(0.0);
                layers.push(
                    pin(container("")
                        .width(Length::Fixed(w))
                        .height(Length::Fixed(h))
                        .style(|_theme| container::Style {
                            background: Some(Background::Color(Color {
                                r: 0.45,
                                g: 0.75,
                                b: 1.0,
                                a: 0.12,
                            })),
                            border: Border {
                                color: Color {
                                    r: 0.65,
                                    g: 0.85,
                                    b: 1.0,
                                    a: 0.95,
                                },
                                width: 1.0,
                                radius: 0.0.into(),
                            },
                            ..container::Style::default()
                        }))
                    .position(Point::new(x, y))
                    .into(),
                );
            }
        }
        if let (Some(start), Some(end)) = (state.midi_clip_create_start, state.midi_clip_create_end)
        {
            let x = start.x.min(end.x).max(0.0);
            let y = start.y.min(end.y).max(0.0);
            let w = (start.x - end.x).abs().max(2.0);
            let h = (start.y - end.y).abs().max(2.0);
            layers.push(
                pin(container("")
                    .width(Length::Fixed(w))
                    .height(Length::Fixed(h))
                    .style(|_theme| container::Style {
                        background: Some(Background::Color(Color {
                            r: 0.5,
                            g: 0.9,
                            b: 0.55,
                            a: 0.18,
                        })),
                        border: Border {
                            color: Color {
                                r: 0.7,
                                g: 1.0,
                                b: 0.72,
                                a: 0.95,
                            },
                            width: 1.0,
                            radius: 0.0.into(),
                        },
                        ..container::Style::default()
                    }))
                .position(Point::new(x, y))
                .into(),
            );
        }
        container(
            mouse_area(
                Stack::from_vec(layers)
                    .width(Length::Fill)
                    .height(Length::Fill),
            )
            .on_move(Message::EditorMouseMoved)
            .on_press(Message::DeselectClips),
        )
        .style(|_theme| crate::style::app_background())
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }
}
