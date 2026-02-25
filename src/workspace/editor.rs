use crate::{
    message::{DraggedClip, Message},
    state::{State, StateData, Track},
};
use iced::{
    Background, Border, Color, Element, Length, Point,
    widget::{Stack, column, container, mouse_area, pin, row, text},
};
use maolan_engine::kind::Kind;
use std::collections::HashMap;

fn audio_waveform_overlay(
    peaks: &[Vec<f32>],
    clip_width: f32,
    clip_height: f32,
) -> Element<'static, Message> {
    if peaks.is_empty() {
        return container("")
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
    }
    let inner_w = (clip_width - 10.0).max(2.0);
    let inner_h = (clip_height - 8.0).max(6.0);
    let channel_count = peaks.len().max(1);
    let channel_h = inner_h / channel_count as f32;
    let mut bars: Vec<Element<'static, Message>> = vec![];
    for (channel_idx, channel_peaks) in peaks.iter().enumerate() {
        if channel_peaks.is_empty() {
            continue;
        }
        let display_bins = ((inner_w / 2.0) as usize).clamp(1, channel_peaks.len());
        let x_step = inner_w / display_bins as f32;
        let center_y = channel_h * (channel_idx as f32 + 0.5);
        for i in 0..display_bins {
            let src_idx = i * channel_peaks.len() / display_bins;
            let amp = channel_peaks[src_idx].clamp(0.0, 1.0);
            let bar_h = (amp * channel_h).max(1.0);
            bars.push(
                pin(container("")
                    .width(Length::Fixed(1.0))
                    .height(Length::Fixed(bar_h))
                    .style(|_theme| container::Style {
                        background: Some(Background::Color(Color {
                            r: 0.8,
                            g: 0.9,
                            b: 1.0,
                            a: 0.45,
                        })),
                        ..container::Style::default()
                    }))
                .position(Point::new(i as f32 * x_step, center_y - bar_h * 0.5))
                .into(),
            );
        }
    }
    Stack::from_vec(bars)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn view_track_elements(
    state: &StateData,
    track: Track,
    pixels_per_sample: f32,
    samples_per_bar: f32,
    active_clip_drag: Option<&DraggedClip>,
    active_target_track: Option<&str>,
    recording_preview_bounds: Option<(usize, usize)>,
    recording_preview_peaks: Option<&HashMap<String, Vec<Vec<f32>>>>,
) -> Element<'static, Message> {
    let snap_sample_to_bar = |sample: f32| -> f32 {
        let bar = samples_per_bar.max(1.0);
        (sample.max(0.0) / bar).round() * bar
    };
    let mut clips: Vec<Element<'static, Message>> = vec![
        mouse_area(container("").width(Length::Fill).height(Length::Fill))
            .on_press(Message::DeselectClips)
            .into(),
    ];
    let height = track.height;
    let track_name_cloned = track.name.clone();

    for (index, clip) in track.audio.clips.iter().enumerate() {
        let clip_name = clip.name.clone();
        let clip_peaks = clip.peaks.clone();
        let clip_id = crate::state::ClipId {
            track_idx: track_name_cloned.clone(),
            clip_idx: index,
            kind: Kind::Audio,
        };
        let is_selected = state.selected_clips.contains(&clip_id);
        let active_drag = active_clip_drag.filter(|d| {
            d.kind == Kind::Audio && d.track_index == track_name_cloned && d.index == index
        });
        let group_drag = active_clip_drag.filter(|d| {
            d.kind == Kind::Audio
                && d.track_index == track_name_cloned
                && state.selected_clips.contains(&crate::state::ClipId {
                    track_idx: track_name_cloned.clone(),
                    clip_idx: d.index,
                    kind: Kind::Audio,
                })
                && state
                    .selected_clips
                    .iter()
                    .filter(|id| id.kind == Kind::Audio && id.track_idx == track_name_cloned)
                    .count()
                    > 1
                && is_selected
        });
        let drag_for_clip = group_drag.or(active_drag);
        let dragged_to_other_track = drag_for_clip.is_some_and(|d| {
            !d.copy
                && active_target_track.is_some_and(|target| target != track_name_cloned.as_str())
        });
        let show_preview_in_this_track = drag_for_clip.is_some_and(|d| {
            active_target_track.is_some_and(|target| target == track_name_cloned.as_str())
                && (d.copy || d.track_index != track_name_cloned)
        });
        let dragged_start = drag_for_clip
            .filter(|d| !d.copy)
            .map(|d| {
                let delta_samples = (d.end.x - d.start.x) / pixels_per_sample.max(1.0e-6);
                snap_sample_to_bar(clip.start as f32 + delta_samples)
            })
            .unwrap_or(clip.start as f32);
        let clip_width = (clip.length as f32 * pixels_per_sample).max(12.0);
        let clip_height = (height - 10.0).max(12.0);

        let left_handle = mouse_area(
            container("")
                .width(Length::Fixed(5.0))
                .height(Length::Fill)
                .style(|_theme| {
                    use container::Style;
                    Style {
                        background: Some(Background::Color(Color {
                            r: 0.2,
                            g: 0.4,
                            b: 0.6,
                            a: 0.9,
                        })),
                        ..Style::default()
                    }
                }),
        )
        .on_press(Message::ClipResizeStart(
            Kind::Audio,
            track_name_cloned.clone(),
            index,
            false,
        ));

        let right_handle = mouse_area(
            container("")
                .width(Length::Fixed(5.0))
                .height(Length::Fill)
                .style(|_theme| {
                    use container::Style;
                    Style {
                        background: Some(Background::Color(Color {
                            r: 0.2,
                            g: 0.4,
                            b: 0.6,
                            a: 0.9,
                        })),
                        ..Style::default()
                    }
                }),
        )
        .on_press(Message::ClipResizeStart(
            Kind::Audio,
            track_name_cloned.clone(),
            index,
            true,
        ));

        let clip_content = container(Stack::with_children(vec![
            audio_waveform_overlay(&clip_peaks, clip_width, clip_height),
            container(text(clip_name.clone()).size(12))
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(5)
                .into(),
        ]))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(0)
        .style(move |_theme| {
            use container::Style;
            Style {
                background: Some(Background::Color(if is_selected {
                    Color {
                        r: 0.72,
                        g: 0.86,
                        b: 1.0,
                        a: 1.0,
                    }
                } else {
                    Color {
                        r: 0.27,
                        g: 0.45,
                        b: 0.62,
                        a: 0.8,
                    }
                })),
                ..Style::default()
            }
        });

        let clip_widget = container(row![left_handle, clip_content, right_handle])
            .width(Length::Fixed(clip_width))
            .height(Length::Fill)
            .style(move |_theme| container::Style {
                background: None,
                border: Border {
                    color: if is_selected {
                        Color {
                            r: 0.98,
                            g: 0.98,
                            b: 0.98,
                            a: 1.0,
                        }
                    } else {
                        Color {
                            r: 0.2,
                            g: 0.4,
                            b: 0.6,
                            a: 1.0,
                        }
                    },
                    width: if is_selected { 2.0 } else { 1.0 },
                    radius: 3.0.into(),
                },
                ..container::Style::default()
            });

        if !dragged_to_other_track {
            clips.push(
                pin(mouse_area(clip_widget)
                    .on_press(Message::SelectClip {
                        track_idx: track_name_cloned.clone(),
                        clip_idx: index,
                        kind: Kind::Audio,
                    })
                    .on_move({
                        let track_name_for_drag_closure = track_name_cloned.clone();
                        move |point| {
                            let mut clip_data = DraggedClip::new(
                                Kind::Audio,
                                index,
                                track_name_for_drag_closure.clone(),
                            );
                            clip_data.start = point;
                            Message::ClipDrag(clip_data)
                        }
                    }))
                .position(Point::new(dragged_start * pixels_per_sample, 0.0))
                .into(),
            );
        }

        if let Some(drag) = drag_for_clip.filter(|_| show_preview_in_this_track) {
            let delta_samples = (drag.end.x - drag.start.x) / pixels_per_sample.max(1.0e-6);
            let preview_start = snap_sample_to_bar(clip.start as f32 + delta_samples);
            let preview_content = container(Stack::with_children(vec![
                audio_waveform_overlay(&clip_peaks, clip_width, clip_height),
                container(text(clip_name.clone()).size(12))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .padding(5)
                    .into(),
            ]))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(0)
            .style(|_theme| {
                use container::Style;
                Style {
                    background: Some(Background::Color(Color {
                        r: 0.72,
                        g: 0.86,
                        b: 1.0,
                        a: 0.7,
                    })),
                    ..Style::default()
                }
            });
            let preview = container(row![
                container("").width(Length::Fixed(5.0)).height(Length::Fill),
                preview_content,
                container("").width(Length::Fixed(5.0)).height(Length::Fill)
            ])
            .width(Length::Fixed(clip_width))
            .height(Length::Fill)
            .style(|_theme| container::Style {
                background: None,
                border: Border {
                    color: Color {
                        r: 0.98,
                        g: 0.98,
                        b: 0.98,
                        a: 0.9,
                    },
                    width: 2.0,
                    radius: 3.0.into(),
                },
                ..container::Style::default()
            });
            clips.push(
                pin(preview)
                    .position(Point::new(preview_start * pixels_per_sample, 0.0))
                    .into(),
            );
        }
    }
    for (index, clip) in track.midi.clips.iter().enumerate() {
        let clip_name = clip.name.clone();
        let clip_id = crate::state::ClipId {
            track_idx: track_name_cloned.clone(),
            clip_idx: index,
            kind: Kind::MIDI,
        };
        let is_selected = state.selected_clips.contains(&clip_id);
        let active_drag = active_clip_drag.filter(|d| {
            d.kind == Kind::MIDI && d.track_index == track_name_cloned && d.index == index
        });
        let group_drag = active_clip_drag.filter(|d| {
            d.kind == Kind::MIDI
                && d.track_index == track_name_cloned
                && state.selected_clips.contains(&crate::state::ClipId {
                    track_idx: track_name_cloned.clone(),
                    clip_idx: d.index,
                    kind: Kind::MIDI,
                })
                && state
                    .selected_clips
                    .iter()
                    .filter(|id| id.kind == Kind::MIDI && id.track_idx == track_name_cloned)
                    .count()
                    > 1
                && is_selected
        });
        let drag_for_clip = group_drag.or(active_drag);
        let dragged_to_other_track = drag_for_clip.is_some_and(|d| {
            !d.copy
                && active_target_track.is_some_and(|target| target != track_name_cloned.as_str())
        });
        let show_preview_in_this_track = drag_for_clip.is_some_and(|d| {
            active_target_track.is_some_and(|target| target == track_name_cloned.as_str())
                && (d.copy || d.track_index != track_name_cloned)
        });
        let dragged_start = drag_for_clip
            .filter(|d| !d.copy)
            .map(|d| {
                let delta_samples = (d.end.x - d.start.x) / pixels_per_sample.max(1.0e-6);
                snap_sample_to_bar(clip.start as f32 + delta_samples)
            })
            .unwrap_or(clip.start as f32);
        let clip_width = (clip.length as f32 * pixels_per_sample).max(12.0);

        let left_handle = mouse_area(
            container("")
                .width(Length::Fixed(5.0))
                .height(Length::Fill)
                .style(|_theme| {
                    use container::Style;
                    Style {
                        background: Some(Background::Color(Color {
                            r: 0.25,
                            g: 0.55,
                            b: 0.25,
                            a: 0.9,
                        })),
                        ..Style::default()
                    }
                }),
        )
        .on_press(Message::ClipResizeStart(
            Kind::MIDI,
            track_name_cloned.clone(),
            index,
            false,
        ));

        let right_handle = mouse_area(
            container("")
                .width(Length::Fixed(5.0))
                .height(Length::Fill)
                .style(|_theme| {
                    use container::Style;
                    Style {
                        background: Some(Background::Color(Color {
                            r: 0.25,
                            g: 0.55,
                            b: 0.25,
                            a: 0.9,
                        })),
                        ..Style::default()
                    }
                }),
        )
        .on_press(Message::ClipResizeStart(
            Kind::MIDI,
            track_name_cloned.clone(),
            index,
            true,
        ));

        let clip_content = container(container(text(clip_name.clone()).size(12)).padding(5))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(0)
            .style(move |_theme| {
                use container::Style;
                Style {
                    background: Some(Background::Color(if is_selected {
                        Color {
                            r: 0.82,
                            g: 1.0,
                            b: 0.84,
                            a: 1.0,
                        }
                    } else {
                        Color {
                            r: 0.24,
                            g: 0.5,
                            b: 0.26,
                            a: 0.82,
                        }
                    })),
                    ..Style::default()
                }
            });

        let clip_widget = container(row![left_handle, clip_content, right_handle])
            .width(Length::Fixed(clip_width))
            .height(Length::Fill)
            .style(move |_theme| container::Style {
                background: None,
                border: Border {
                    color: if is_selected {
                        Color {
                            r: 0.98,
                            g: 0.98,
                            b: 0.98,
                            a: 1.0,
                        }
                    } else {
                        Color {
                            r: 0.2,
                            g: 0.45,
                            b: 0.2,
                            a: 1.0,
                        }
                    },
                    width: if is_selected { 2.0 } else { 1.0 },
                    radius: 3.0.into(),
                },
                ..container::Style::default()
            });

        if !dragged_to_other_track {
            clips.push(
                pin(mouse_area(clip_widget)
                    .on_press(Message::SelectClip {
                        track_idx: track_name_cloned.clone(),
                        clip_idx: index,
                        kind: Kind::MIDI,
                    })
                    .on_move({
                        let track_name_for_drag_closure = track_name_cloned.clone();
                        move |point| {
                            let mut clip_data = DraggedClip::new(
                                Kind::MIDI,
                                index,
                                track_name_for_drag_closure.clone(),
                            );
                            clip_data.start = point;
                            Message::ClipDrag(clip_data)
                        }
                    }))
                .position(Point::new(dragged_start * pixels_per_sample, 0.0))
                .into(),
            );
        }

        if let Some(drag) = drag_for_clip.filter(|_| show_preview_in_this_track) {
            let delta_samples = (drag.end.x - drag.start.x) / pixels_per_sample.max(1.0e-6);
            let preview_start = snap_sample_to_bar(clip.start as f32 + delta_samples);
            let preview_content = container(container(text(clip_name.clone()).size(12)).padding(5))
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(0)
                .style(|_theme| {
                    use container::Style;
                    Style {
                        background: Some(Background::Color(Color {
                            r: 0.82,
                            g: 1.0,
                            b: 0.84,
                            a: 0.7,
                        })),
                        ..Style::default()
                    }
                });
            let preview = container(row![
                container("").width(Length::Fixed(5.0)).height(Length::Fill),
                preview_content,
                container("").width(Length::Fixed(5.0)).height(Length::Fill)
            ])
            .width(Length::Fixed(clip_width))
            .height(Length::Fill)
            .style(|_theme| container::Style {
                background: None,
                border: Border {
                    color: Color {
                        r: 0.98,
                        g: 0.98,
                        b: 0.98,
                        a: 0.9,
                    },
                    width: 2.0,
                    radius: 3.0.into(),
                },
                ..container::Style::default()
            });
            clips.push(
                pin(preview)
                    .position(Point::new(preview_start * pixels_per_sample, 0.0))
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
                        let clip_width = (source_clip.length as f32 * pixels_per_sample).max(12.0);
                        let clip_height = (height - 10.0).max(12.0);
                        let preview_start =
                            snap_sample_to_bar(source_clip.start as f32 + delta_samples);
                        let preview_content = container(Stack::with_children(vec![
                            audio_waveform_overlay(&source_clip.peaks, clip_width, clip_height),
                            container(text(source_clip.name.clone()).size(12))
                                .width(Length::Fill)
                                .height(Length::Fill)
                                .padding(5)
                                .into(),
                        ]))
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .padding(0)
                        .style(|_theme| {
                            use container::Style;
                            Style {
                                background: Some(Background::Color(Color {
                                    r: 0.72,
                                    g: 0.86,
                                    b: 1.0,
                                    a: 0.7,
                                })),
                                ..Style::default()
                            }
                        });
                        let preview = container(row![
                            container("").width(Length::Fixed(5.0)).height(Length::Fill),
                            preview_content,
                            container("").width(Length::Fixed(5.0)).height(Length::Fill)
                        ])
                        .width(Length::Fixed(clip_width))
                        .height(Length::Fill)
                        .style(|_theme| container::Style {
                            background: None,
                            border: Border {
                                color: Color {
                                    r: 0.98,
                                    g: 0.98,
                                    b: 0.98,
                                    a: 0.9,
                                },
                                width: 2.0,
                                radius: 3.0.into(),
                            },
                            ..container::Style::default()
                        });
                        clips.push(
                            pin(preview)
                                .position(Point::new(preview_start * pixels_per_sample, 0.0))
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
                        let clip_width = (source_clip.length as f32 * pixels_per_sample).max(12.0);
                        let preview_start =
                            snap_sample_to_bar(source_clip.start as f32 + delta_samples);
                        let preview_content = container(
                            container(text(source_clip.name.clone()).size(12)).padding(5),
                        )
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .padding(0)
                        .style(|_theme| {
                            use container::Style;
                            Style {
                                background: Some(Background::Color(Color {
                                    r: 0.82,
                                    g: 1.0,
                                    b: 0.84,
                                    a: 0.7,
                                })),
                                ..Style::default()
                            }
                        });
                        let preview = container(row![
                            container("").width(Length::Fixed(5.0)).height(Length::Fill),
                            preview_content,
                            container("").width(Length::Fixed(5.0)).height(Length::Fill)
                        ])
                        .width(Length::Fixed(clip_width))
                        .height(Length::Fill)
                        .style(|_theme| container::Style {
                            background: None,
                            border: Border {
                                color: Color {
                                    r: 0.98,
                                    g: 0.98,
                                    b: 0.98,
                                    a: 0.9,
                                },
                                width: 2.0,
                                radius: 3.0.into(),
                            },
                            ..container::Style::default()
                        });
                        clips.push(
                            pin(preview)
                                .position(Point::new(preview_start * pixels_per_sample, 0.0))
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
        let preview_height = (height - 10.0).max(12.0);
        let preview_peaks = recording_preview_peaks
            .and_then(|map| map.get(&track.name))
            .cloned()
            .unwrap_or_default();
        let preview_clip = container(
            container(Stack::with_children(vec![
                audio_waveform_overlay(&preview_peaks, preview_width, preview_height),
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
                .position(Point::new(preview_start as f32 * pixels_per_sample, 0.0))
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
    .padding(5)
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

#[derive(Debug)]
pub struct Editor {
    state: State,
}

impl Editor {
    pub fn new(state: State) -> Self {
        Self { state }
    }

    pub fn view(
        &self,
        pixels_per_sample: f32,
        samples_per_bar: f32,
        active_clip_drag: Option<&DraggedClip>,
        active_target_track: Option<&str>,
        recording_preview_bounds: Option<(usize, usize)>,
        recording_preview_peaks: Option<HashMap<String, Vec<Vec<f32>>>>,
    ) -> Element<'_, Message> {
        let mut result = column![];
        let state = self.state.blocking_read();
        for track in state.tracks.iter() {
            result = result.push(view_track_elements(
                &state,
                track.clone(),
                pixels_per_sample,
                samples_per_bar,
                active_clip_drag,
                active_target_track,
                recording_preview_bounds,
                recording_preview_peaks.as_ref(),
            ));
        }
        let mut layers: Vec<Element<'_, Message>> =
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
        mouse_area(
            Stack::from_vec(layers)
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .on_move(Message::EditorMouseMoved)
        .on_press(Message::DeselectClips)
        .into()
    }
}
