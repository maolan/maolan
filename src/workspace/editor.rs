use crate::{
    message::{DraggedClip, Message, SnapMode},
    state::{State, StateData, Track},
};
use iced::{
    Background, Border, Color, Element, Length, Point,
    widget::{Stack, button, column, container, mouse_area, pin, row, text},
};
use iced_aw::ContextMenu;
use maolan_engine::kind::Kind;
use std::collections::HashMap;

const CLIP_RESIZE_HANDLE_WIDTH: f32 = 5.0;

struct TrackElementViewArgs<'a> {
    state: &'a StateData,
    track: Track,
    pixels_per_sample: f32,
    samples_per_bar: f32,
    snap_mode: SnapMode,
    samples_per_beat: f64,
    active_clip_drag: Option<&'a DraggedClip>,
    active_target_track: Option<&'a str>,
    recording_preview_bounds: Option<(usize, usize)>,
    recording_preview_peaks: Option<&'a HashMap<String, Vec<Vec<f32>>>>,
}

fn clean_clip_name(name: &str) -> String {
    let mut cleaned = name.to_string();
    if let Some(stripped) = cleaned.strip_prefix("audio/") {
        cleaned = stripped.to_string();
    }
    if let Some(stripped) = cleaned.strip_suffix(".wav") {
        cleaned = stripped.to_string();
    }
    cleaned
}

fn assign_take_lanes<T, FBase, FStart, FLen>(
    clips: &[T],
    base_lane: FBase,
    start_sample: FStart,
    length_samples: FLen,
) -> (Vec<usize>, Vec<usize>)
where
    FBase: Fn(&T) -> usize,
    FStart: Fn(&T) -> usize,
    FLen: Fn(&T) -> usize,
{
    let mut take_index_by_clip = vec![0_usize; clips.len()];
    let mut max_takes_by_lane: HashMap<usize, usize> = HashMap::new();
    let mut active_by_lane: HashMap<usize, Vec<(usize, usize)>> = HashMap::new();

    let mut order: Vec<usize> = (0..clips.len()).collect();
    order.sort_by_key(|idx| {
        let clip = &clips[*idx];
        (base_lane(clip), start_sample(clip), *idx)
    });

    for idx in order {
        let clip = &clips[idx];
        let lane = base_lane(clip);
        let start = start_sample(clip);
        let end = start.saturating_add(length_samples(clip));
        let active = active_by_lane.entry(lane).or_default();

        // Keep only active overlaps in this lane (touching edges is not overlap).
        active.retain(|(existing_end, _)| *existing_end > start);

        let mut take_idx = 0_usize;
        while active.iter().any(|(_, existing_take)| *existing_take == take_idx) {
            take_idx = take_idx.saturating_add(1);
        }
        active.push((end, take_idx));
        take_index_by_clip[idx] = take_idx;
        max_takes_by_lane
            .entry(lane)
            .and_modify(|max_take| *max_take = (*max_take).max(take_idx.saturating_add(1)))
            .or_insert(take_idx.saturating_add(1));
    }

    let take_count_by_clip = clips
        .iter()
        .map(|clip| {
            let lane = base_lane(clip);
            max_takes_by_lane.get(&lane).copied().unwrap_or(1).max(1)
        })
        .collect::<Vec<_>>();

    (take_index_by_clip, take_count_by_clip)
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

fn audio_waveform_overlay(
    peaks: &[Vec<f32>],
    clip_width: f32,
    clip_height: f32,
    clip_offset: usize,
    clip_length: usize,
    max_length: usize,
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
        let display_bins =
            ((inner_w / 2.0) as usize).clamp(1, clip_length.min(channel_peaks.len()));
        let x_step = inner_w / display_bins as f32;
        let center_y = channel_h * (channel_idx as f32 + 0.5);

        // Calculate the range of peaks to display based on clip offset and length
        let total_peaks = channel_peaks.len();
        let max_len = max_length.max(1);

        for i in 0..display_bins {
            // Map display bin to position within the clip (0 to clip_length)
            let clip_sample_pos = (i * clip_length) / display_bins;
            // Add offset to get absolute position in the audio file
            let absolute_sample_pos = clip_offset + clip_sample_pos;
            // Map absolute sample position to peak index
            let src_idx =
                ((absolute_sample_pos * total_peaks) / max_len).min(total_peaks.saturating_sub(1));

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

fn view_track_elements(args: TrackElementViewArgs<'_>) -> Element<'static, Message> {
    let TrackElementViewArgs {
        state,
        track,
        pixels_per_sample,
        samples_per_bar,
        snap_mode,
        samples_per_beat,
        active_clip_drag,
        active_target_track,
        recording_preview_bounds,
        recording_preview_peaks,
    } = args;
    let snap_sample = |sample: f32| -> f32 {
        match snap_mode {
            SnapMode::NoSnap => sample.max(0.0),
            SnapMode::Bar => {
                let interval = samples_per_bar.max(1.0);
                (sample.max(0.0) / interval).round() * interval
            }
            SnapMode::Beat => {
                let interval = samples_per_beat.max(1.0) as f32;
                (sample.max(0.0) / interval).round() * interval
            }
            SnapMode::Eighth => {
                let interval = (samples_per_beat / 2.0).max(1.0) as f32;
                (sample.max(0.0) / interval).round() * interval
            }
            SnapMode::Sixteenth => {
                let interval = (samples_per_beat / 4.0).max(1.0) as f32;
                (sample.max(0.0) / interval).round() * interval
            }
            SnapMode::ThirtySecond => {
                let interval = (samples_per_beat / 8.0).max(1.0) as f32;
                (sample.max(0.0) / interval).round() * interval
            }
            SnapMode::SixtyFourth => {
                let interval = (samples_per_beat / 16.0).max(1.0) as f32;
                (sample.max(0.0) / interval).round() * interval
            }
        }
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
    let (audio_take_idx, audio_take_count) =
        assign_take_lanes(&track.audio.clips, |_| 0, |clip| clip.start, |clip| clip.length);
    let (midi_take_idx, midi_take_count) = assign_take_lanes(
        &track.midi.clips,
        |clip| clip.input_channel.min(track.midi.ins.saturating_sub(1)),
        |clip| clip.start,
        |clip| clip.length,
    );

    clips.push(
        pin(container(text(track.name.clone()).size(11))
            .width(Length::Fill)
            .height(Length::Fixed(layout.header_height))
            .padding(4)
            .style(|_theme| container::Style {
                background: Some(Background::Color(Color {
                    r: 0.08,
                    g: 0.08,
                    b: 0.1,
                    a: 0.5,
                })),
                ..container::Style::default()
            }))
        .position(Point::new(0.0, 0.0))
        .into(),
    );

    for lane in 0..track.audio.ins {
        let y = track.lane_top(Kind::Audio, lane);
        clips.push(
            pin(container("")
                .width(Length::Fill)
                .height(Length::Fixed(lane_height))
                .style(|_theme| container::Style {
                    background: Some(Background::Color(Color {
                        r: 0.15,
                        g: 0.2,
                        b: 0.28,
                        a: 0.22,
                    })),
                    ..container::Style::default()
                }))
            .position(Point::new(0.0, y))
            .into(),
        );
    }
    for lane in 0..track.midi.ins {
        let y = track.lane_top(Kind::MIDI, lane);
        clips.push(
            pin(container("")
                .width(Length::Fill)
                .height(Length::Fixed(lane_height))
                .style(|_theme| container::Style {
                    background: Some(Background::Color(Color {
                        r: 0.12,
                        g: 0.26,
                        b: 0.14,
                        a: 0.25,
                    })),
                    ..container::Style::default()
                }))
            .position(Point::new(0.0, y))
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
            pin(container(text(format!("Automation {}", lane.target)).size(10))
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
                }))
            .position(Point::new(4.0, lane_top + 2.0))
            .into(),
        );

        let point_color = automation_point_color(lane.target);
        let mut sorted_points = lane.points.clone();
        sorted_points.sort_unstable_by_key(|p| p.sample);
        for segment in sorted_points.windows(2) {
            let left = &segment[0];
            let right = &segment[1];
            let left_x = left.sample as f32 * pixels_per_sample;
            let right_x = right.sample as f32 * pixels_per_sample;
            let left_y = lane_top + 3.0 + (lane_clip_height - 2.0) * (1.0 - left.value.clamp(0.0, 1.0));
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
        let clip_peaks = clip.peaks.clone();
        let clip_muted = clip.muted;
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
                snap_sample(clip.start as f32 + delta_samples)
            })
            .unwrap_or(clip.start as f32);
        // All audio clips are displayed on lane 0 (single audio lane)
        let lane = 0;
        let lane_top_base = track.lane_top(Kind::Audio, lane) + 3.0;
        let take_count = audio_take_count.get(index).copied().unwrap_or(1).max(1);
        let take_idx = audio_take_idx.get(index).copied().unwrap_or(0);
        let take_slot_height = (lane_clip_height / take_count as f32).max(8.0);
        let lane_top = lane_top_base + take_idx as f32 * take_slot_height + 1.0;
        let clip_width =
            ((clip.length as f32 * pixels_per_sample) - CLIP_RESIZE_HANDLE_WIDTH * 2.0).max(12.0);
        let clip_height = (take_slot_height - 2.0).max(8.0);

        let left_handle = mouse_area(
            container("")
                .width(Length::Fixed(CLIP_RESIZE_HANDLE_WIDTH))
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
                .width(Length::Fixed(CLIP_RESIZE_HANDLE_WIDTH))
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
            audio_waveform_overlay(
                &clip_peaks,
                clip_width,
                clip_height,
                clip.offset,
                clip.length,
                clip.max_length_samples,
            ),
            container(text(clean_clip_name(&clip_name)).size(12))
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
                        a: if clip_muted { 0.45 } else { 1.0 },
                    }
                } else {
                    Color {
                        r: 0.27,
                        g: 0.45,
                        b: 0.62,
                        a: if clip_muted { 0.35 } else { 0.8 },
                    }
                })),
                ..Style::default()
            }
        });

        let clip_widget = container(row![left_handle, clip_content, right_handle])
            .width(Length::Fixed(clip_width))
            .height(Length::Fixed(clip_height))
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

        // Add fade handles if fades are enabled
        let clip_with_fades: Element<'_, Message> = if clip.fade_enabled {
            let fade_in_width = (clip.fade_in_samples as f32 * pixels_per_sample).max(5.0);
            let fade_out_width = (clip.fade_out_samples as f32 * pixels_per_sample).max(5.0);

            let mut stack = Stack::new().push(clip_widget);

            // Draw fade-in curve
            if fade_in_width > 5.0 {
                let num_points = (fade_in_width / 3.0).min(20.0) as usize;
                for i in 0..=num_points {
                    let t = i as f32 / num_points as f32;
                    let gain = (t * std::f32::consts::FRAC_PI_2).sin(); // Constant-power fade-in
                    let x = CLIP_RESIZE_HANDLE_WIDTH + t * fade_in_width;
                    let y = clip_height * (1.0 - gain);

                    let point = container("")
                        .width(Length::Fixed(2.0))
                        .height(Length::Fixed(2.0))
                        .style(|_theme| container::Style {
                            background: Some(Background::Color(Color {
                                r: 0.8,
                                g: 0.8,
                                b: 0.8,
                                a: 0.6,
                            })),
                            ..container::Style::default()
                        });
                    stack = stack.push(pin(point).position(Point::new(x, y)));
                }

                // Fade-in drag handle
                let fade_in_track_name = track_name_cloned.clone();
                let fade_in_handle = mouse_area(
                    container("")
                        .width(Length::Fixed(6.0))
                        .height(Length::Fixed(6.0))
                        .style(|_theme| container::Style {
                            background: Some(Background::Color(Color {
                                r: 1.0,
                                g: 1.0,
                                b: 1.0,
                                a: 0.9,
                            })),
                            border: Border {
                                color: Color {
                                    r: 0.3,
                                    g: 0.3,
                                    b: 0.3,
                                    a: 1.0,
                                },
                                width: 1.0,
                                radius: 3.0.into(),
                            },
                            ..container::Style::default()
                        }),
                )
                .on_press(Message::FadeResizeStart {
                    kind: Kind::Audio,
                    track_idx: fade_in_track_name,
                    clip_idx: index,
                    is_fade_out: false,
                });
                stack = stack.push(pin(fade_in_handle).position(Point::new(
                    CLIP_RESIZE_HANDLE_WIDTH + fade_in_width - 3.0,
                    -3.0,
                )));
            }

            // Draw fade-out curve
            if fade_out_width > 5.0 {
                let num_points = (fade_out_width / 3.0).min(20.0) as usize;
                for i in 0..=num_points {
                    let t = i as f32 / num_points as f32;
                    let gain = (t * std::f32::consts::FRAC_PI_2).cos(); // Constant-power fade-out
                    let x =
                        CLIP_RESIZE_HANDLE_WIDTH + clip_width - fade_out_width + t * fade_out_width;
                    let y = clip_height * (1.0 - gain);

                    let point = container("")
                        .width(Length::Fixed(2.0))
                        .height(Length::Fixed(2.0))
                        .style(|_theme| container::Style {
                            background: Some(Background::Color(Color {
                                r: 0.8,
                                g: 0.8,
                                b: 0.8,
                                a: 0.6,
                            })),
                            ..container::Style::default()
                        });
                    stack = stack.push(pin(point).position(Point::new(x, y)));
                }

                // Fade-out drag handle
                let fade_out_track_name = track_name_cloned.clone();
                let fade_out_handle = mouse_area(
                    container("")
                        .width(Length::Fixed(6.0))
                        .height(Length::Fixed(6.0))
                        .style(|_theme| container::Style {
                            background: Some(Background::Color(Color {
                                r: 1.0,
                                g: 1.0,
                                b: 1.0,
                                a: 0.9,
                            })),
                            border: Border {
                                color: Color {
                                    r: 0.3,
                                    g: 0.3,
                                    b: 0.3,
                                    a: 1.0,
                                },
                                width: 1.0,
                                radius: 3.0.into(),
                            },
                            ..container::Style::default()
                        }),
                )
                .on_press(Message::FadeResizeStart {
                    kind: Kind::Audio,
                    track_idx: fade_out_track_name,
                    clip_idx: index,
                    is_fade_out: true,
                });
                stack = stack.push(pin(fade_out_handle).position(Point::new(
                    CLIP_RESIZE_HANDLE_WIDTH + clip_width - fade_out_width - 3.0,
                    -3.0,
                )));
            }

            stack.into()
        } else {
            clip_widget.into()
        };

        if !dragged_to_other_track {
            let clip_with_mouse = mouse_area(clip_with_fades)
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
                });

            let track_idx_for_menu = track_name_cloned.clone();
            let index_for_menu = index;
            let fade_enabled = clip.fade_enabled;
            let muted_for_menu = clip_muted;
            let clip_with_context = ContextMenu::new(clip_with_mouse, move || {
                column![
                    button("Rename").on_press(Message::ClipRenameShow {
                        track_idx: track_idx_for_menu.clone(),
                        clip_idx: index_for_menu,
                        kind: Kind::Audio,
                    }),
                    button("Set Active Take").on_press(Message::ClipSetActiveTake {
                        track_idx: track_idx_for_menu.clone(),
                        clip_idx: index_for_menu,
                        kind: Kind::Audio,
                    }),
                    button("Next Active Take").on_press(Message::ClipCycleActiveTake {
                        track_idx: track_idx_for_menu.clone(),
                        clip_idx: index_for_menu,
                        kind: Kind::Audio,
                    }),
                    button("Unmute All Takes").on_press(Message::ClipUnmuteTakesInRange {
                        track_idx: track_idx_for_menu.clone(),
                        clip_idx: index_for_menu,
                        kind: Kind::Audio,
                    }),
                    button(if muted_for_menu {
                        "Unmute Take"
                    } else {
                        "Mute Take"
                    })
                    .on_press(Message::ClipSetMuted {
                        track_idx: track_idx_for_menu.clone(),
                        clip_idx: index_for_menu,
                        kind: Kind::Audio,
                        muted: !muted_for_menu,
                    }),
                    button(if fade_enabled {
                        "Disable Fade"
                    } else {
                        "Enable Fade"
                    })
                    .on_press(Message::ClipToggleFade {
                        track_idx: track_idx_for_menu.clone(),
                        clip_idx: index_for_menu,
                        kind: Kind::Audio,
                    }),
                ]
                .into()
            });

            clips.push(
                pin(clip_with_context)
                    .position(Point::new(dragged_start * pixels_per_sample, lane_top))
                    .into(),
            );
        }

        if let Some(drag) = drag_for_clip.filter(|_| show_preview_in_this_track) {
            let delta_samples = (drag.end.x - drag.start.x) / pixels_per_sample.max(1.0e-6);
            let preview_start = snap_sample(clip.start as f32 + delta_samples);
            let preview_content = container(Stack::with_children(vec![
                audio_waveform_overlay(
                    &clip_peaks,
                    clip_width,
                    clip_height,
                    clip.offset,
                    clip.length,
                    clip.max_length_samples,
                ),
                container(text(clean_clip_name(&clip_name)).size(12))
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
                container("")
                    .width(Length::Fixed(CLIP_RESIZE_HANDLE_WIDTH))
                    .height(Length::Fill),
                preview_content,
                container("")
                    .width(Length::Fixed(CLIP_RESIZE_HANDLE_WIDTH))
                    .height(Length::Fill)
            ])
            .width(Length::Fixed(clip_width))
            .height(Length::Fixed(clip_height))
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
                    .position(Point::new(preview_start * pixels_per_sample, lane_top))
                    .into(),
            );
        }
    }
    for (index, clip) in track.midi.clips.iter().enumerate() {
        let clip_name = clip.name.clone();
        let clip_muted = clip.muted;
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
                snap_sample(clip.start as f32 + delta_samples)
            })
            .unwrap_or(clip.start as f32);
        let lane = clip.input_channel.min(track.midi.ins.saturating_sub(1));
        let lane_top_base = track.lane_top(Kind::MIDI, lane) + 3.0;
        let take_count = midi_take_count.get(index).copied().unwrap_or(1).max(1);
        let take_idx = midi_take_idx.get(index).copied().unwrap_or(0);
        let take_slot_height = (lane_clip_height / take_count as f32).max(8.0);
        let lane_top = lane_top_base + take_idx as f32 * take_slot_height + 1.0;
        let clip_width =
            ((clip.length as f32 * pixels_per_sample) - CLIP_RESIZE_HANDLE_WIDTH * 2.0).max(12.0);
        let clip_height = (take_slot_height - 2.0).max(8.0);

        let left_handle = mouse_area(
            container("")
                .width(Length::Fixed(CLIP_RESIZE_HANDLE_WIDTH))
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
                .width(Length::Fixed(CLIP_RESIZE_HANDLE_WIDTH))
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

        let clip_content =
            container(container(text(clean_clip_name(&clip_name)).size(12)).padding(5))
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
                                a: if clip_muted { 0.45 } else { 1.0 },
                            }
                        } else {
                            Color {
                                r: 0.24,
                                g: 0.5,
                                b: 0.26,
                                a: if clip_muted { 0.35 } else { 0.82 },
                            }
                        })),
                        ..Style::default()
                    }
                });

        let clip_widget = container(row![left_handle, clip_content, right_handle])
            .width(Length::Fixed(clip_width))
            .height(Length::Fixed(clip_height))
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

        // Add fade handles if fades are enabled (MIDI clips)
        let clip_with_fades: Element<'_, Message> = if clip.fade_enabled {
            let fade_in_width = (clip.fade_in_samples as f32 * pixels_per_sample).max(5.0);
            let fade_out_width = (clip.fade_out_samples as f32 * pixels_per_sample).max(5.0);

            let mut stack = Stack::new().push(clip_widget);

            // Draw fade-in curve
            if fade_in_width > 5.0 {
                let num_points = (fade_in_width / 3.0).min(20.0) as usize;
                for i in 0..=num_points {
                    let t = i as f32 / num_points as f32;
                    let gain = (t * std::f32::consts::FRAC_PI_2).sin(); // Constant-power fade-in
                    let x = CLIP_RESIZE_HANDLE_WIDTH + t * fade_in_width;
                    let y = clip_height * (1.0 - gain);

                    let point = container("")
                        .width(Length::Fixed(2.0))
                        .height(Length::Fixed(2.0))
                        .style(|_theme| container::Style {
                            background: Some(Background::Color(Color {
                                r: 0.7,
                                g: 0.9,
                                b: 0.7,
                                a: 0.6,
                            })),
                            ..container::Style::default()
                        });
                    stack = stack.push(pin(point).position(Point::new(x, y)));
                }

                // Fade-in drag handle
                let fade_in_track_name = track_name_cloned.clone();
                let fade_in_handle = mouse_area(
                    container("")
                        .width(Length::Fixed(6.0))
                        .height(Length::Fixed(6.0))
                        .style(|_theme| container::Style {
                            background: Some(Background::Color(Color {
                                r: 0.9,
                                g: 1.0,
                                b: 0.9,
                                a: 0.9,
                            })),
                            border: Border {
                                color: Color {
                                    r: 0.3,
                                    g: 0.5,
                                    b: 0.3,
                                    a: 1.0,
                                },
                                width: 1.0,
                                radius: 3.0.into(),
                            },
                            ..container::Style::default()
                        }),
                )
                .on_press(Message::FadeResizeStart {
                    kind: Kind::MIDI,
                    track_idx: fade_in_track_name,
                    clip_idx: index,
                    is_fade_out: false,
                });
                stack = stack.push(pin(fade_in_handle).position(Point::new(
                    CLIP_RESIZE_HANDLE_WIDTH + fade_in_width - 3.0,
                    -3.0,
                )));
            }

            // Draw fade-out curve
            if fade_out_width > 5.0 {
                let num_points = (fade_out_width / 3.0).min(20.0) as usize;
                for i in 0..=num_points {
                    let t = i as f32 / num_points as f32;
                    let gain = (t * std::f32::consts::FRAC_PI_2).cos(); // Constant-power fade-out
                    let x =
                        CLIP_RESIZE_HANDLE_WIDTH + clip_width - fade_out_width + t * fade_out_width;
                    let y = clip_height * (1.0 - gain);

                    let point = container("")
                        .width(Length::Fixed(2.0))
                        .height(Length::Fixed(2.0))
                        .style(|_theme| container::Style {
                            background: Some(Background::Color(Color {
                                r: 0.7,
                                g: 0.9,
                                b: 0.7,
                                a: 0.6,
                            })),
                            ..container::Style::default()
                        });
                    stack = stack.push(pin(point).position(Point::new(x, y)));
                }

                // Fade-out drag handle
                let fade_out_track_name = track_name_cloned.clone();
                let fade_out_handle = mouse_area(
                    container("")
                        .width(Length::Fixed(6.0))
                        .height(Length::Fixed(6.0))
                        .style(|_theme| container::Style {
                            background: Some(Background::Color(Color {
                                r: 0.9,
                                g: 1.0,
                                b: 0.9,
                                a: 0.9,
                            })),
                            border: Border {
                                color: Color {
                                    r: 0.3,
                                    g: 0.5,
                                    b: 0.3,
                                    a: 1.0,
                                },
                                width: 1.0,
                                radius: 3.0.into(),
                            },
                            ..container::Style::default()
                        }),
                )
                .on_press(Message::FadeResizeStart {
                    kind: Kind::MIDI,
                    track_idx: fade_out_track_name,
                    clip_idx: index,
                    is_fade_out: true,
                });
                stack = stack.push(pin(fade_out_handle).position(Point::new(
                    CLIP_RESIZE_HANDLE_WIDTH + clip_width - fade_out_width - 3.0,
                    -3.0,
                )));
            }

            stack.into()
        } else {
            clip_widget.into()
        };

        if !dragged_to_other_track {
            let clip_with_mouse = mouse_area(clip_with_fades)
                .on_press(Message::SelectClip {
                    track_idx: track_name_cloned.clone(),
                    clip_idx: index,
                    kind: Kind::MIDI,
                })
                .on_double_click(Message::OpenMidiPiano {
                    track_idx: track_name_cloned.clone(),
                    clip_idx: index,
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
                });

            let track_idx_for_menu = track_name_cloned.clone();
            let index_for_menu = index;
            let fade_enabled = clip.fade_enabled;
            let muted_for_menu = clip_muted;
            let clip_with_context = ContextMenu::new(clip_with_mouse, move || {
                column![
                    button("Rename").on_press(Message::ClipRenameShow {
                        track_idx: track_idx_for_menu.clone(),
                        clip_idx: index_for_menu,
                        kind: Kind::MIDI,
                    }),
                    button("Set Active Take").on_press(Message::ClipSetActiveTake {
                        track_idx: track_idx_for_menu.clone(),
                        clip_idx: index_for_menu,
                        kind: Kind::MIDI,
                    }),
                    button("Next Active Take").on_press(Message::ClipCycleActiveTake {
                        track_idx: track_idx_for_menu.clone(),
                        clip_idx: index_for_menu,
                        kind: Kind::MIDI,
                    }),
                    button("Unmute All Takes").on_press(Message::ClipUnmuteTakesInRange {
                        track_idx: track_idx_for_menu.clone(),
                        clip_idx: index_for_menu,
                        kind: Kind::MIDI,
                    }),
                    button(if muted_for_menu {
                        "Unmute Take"
                    } else {
                        "Mute Take"
                    })
                    .on_press(Message::ClipSetMuted {
                        track_idx: track_idx_for_menu.clone(),
                        clip_idx: index_for_menu,
                        kind: Kind::MIDI,
                        muted: !muted_for_menu,
                    }),
                    button(if fade_enabled {
                        "Disable Fade"
                    } else {
                        "Enable Fade"
                    })
                    .on_press(Message::ClipToggleFade {
                        track_idx: track_idx_for_menu.clone(),
                        clip_idx: index_for_menu,
                        kind: Kind::MIDI,
                    }),
                ]
                .into()
            });

            clips.push(
                pin(clip_with_context)
                    .position(Point::new(dragged_start * pixels_per_sample, lane_top))
                    .into(),
            );
        }

        if let Some(drag) = drag_for_clip.filter(|_| show_preview_in_this_track) {
            let delta_samples = (drag.end.x - drag.start.x) / pixels_per_sample.max(1.0e-6);
            let preview_start = snap_sample(clip.start as f32 + delta_samples);
            let preview_content =
                container(container(text(clean_clip_name(&clip_name)).size(12)).padding(5))
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
                container("")
                    .width(Length::Fixed(CLIP_RESIZE_HANDLE_WIDTH))
                    .height(Length::Fill),
                preview_content,
                container("")
                    .width(Length::Fixed(CLIP_RESIZE_HANDLE_WIDTH))
                    .height(Length::Fill)
            ])
            .width(Length::Fixed(clip_width))
            .height(Length::Fixed(clip_height))
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
                        let clip_width = ((source_clip.length as f32 * pixels_per_sample)
                            - CLIP_RESIZE_HANDLE_WIDTH * 2.0)
                            .max(12.0);
                        let clip_height = lane_clip_height;
                        // All audio clips are displayed on lane 0 (single audio lane)
                        let lane = 0;
                        let lane_top = track.lane_top(Kind::Audio, lane) + 3.0;
                        let preview_start = snap_sample(source_clip.start as f32 + delta_samples);
                        let preview_content = container(Stack::with_children(vec![
                            audio_waveform_overlay(
                                &source_clip.peaks,
                                clip_width,
                                clip_height,
                                source_clip.offset,
                                source_clip.length,
                                source_clip.max_length_samples,
                            ),
                            container(text(clean_clip_name(&source_clip.name)).size(12))
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
                            container("")
                                .width(Length::Fixed(CLIP_RESIZE_HANDLE_WIDTH))
                                .height(Length::Fill),
                            preview_content,
                            container("")
                                .width(Length::Fixed(CLIP_RESIZE_HANDLE_WIDTH))
                                .height(Length::Fill)
                        ])
                        .width(Length::Fixed(clip_width))
                        .height(Length::Fixed(clip_height))
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
                        let clip_width = ((source_clip.length as f32 * pixels_per_sample)
                            - CLIP_RESIZE_HANDLE_WIDTH * 2.0)
                            .max(12.0);
                        let lane = source_clip
                            .input_channel
                            .min(track.midi.ins.saturating_sub(1));
                        let lane_top = track.lane_top(Kind::MIDI, lane) + 3.0;
                        let preview_start = snap_sample(source_clip.start as f32 + delta_samples);
                        let preview_content = container(
                            container(text(clean_clip_name(&source_clip.name)).size(12)).padding(5),
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
                            container("")
                                .width(Length::Fixed(CLIP_RESIZE_HANDLE_WIDTH))
                                .height(Length::Fill),
                            preview_content,
                            container("")
                                .width(Length::Fixed(CLIP_RESIZE_HANDLE_WIDTH))
                                .height(Length::Fill)
                        ])
                        .width(Length::Fixed(clip_width))
                        .height(Length::Fixed(lane_clip_height))
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
        let preview_height = lane_clip_height;
        let preview_top = track.lane_top(Kind::Audio, 0) + 3.0;
        let preview_peaks = recording_preview_peaks
            .and_then(|map| map.get(&track.name))
            .cloned()
            .unwrap_or_default();
        let preview_length = preview_current - preview_start;
        let preview_clip = container(
            container(Stack::with_children(vec![
                audio_waveform_overlay(
                    &preview_peaks,
                    preview_width,
                    preview_height,
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

pub struct EditorViewArgs<'a> {
    pub pixels_per_sample: f32,
    pub samples_per_bar: f32,
    pub snap_mode: SnapMode,
    pub samples_per_beat: f64,
    pub active_clip_drag: Option<&'a DraggedClip>,
    pub active_target_track: Option<&'a str>,
    pub recording_preview_bounds: Option<(usize, usize)>,
    pub recording_preview_peaks: Option<&'a HashMap<String, Vec<Vec<f32>>>>,
}

impl Editor {
    pub fn new(state: State) -> Self {
        Self { state }
    }

    pub fn view(&self, args: EditorViewArgs<'_>) -> Element<'_, Message> {
        let EditorViewArgs {
            pixels_per_sample,
            samples_per_bar,
            snap_mode,
            samples_per_beat,
            active_clip_drag,
            active_target_track,
            recording_preview_bounds,
            recording_preview_peaks,
        } = args;
        let mut result = column![];
        let state = self.state.blocking_read();
        for track in state.tracks.iter() {
            result = result.push(view_track_elements(TrackElementViewArgs {
                state: &state,
                track: track.clone(),
                pixels_per_sample,
                samples_per_bar,
                snap_mode,
                samples_per_beat,
                active_clip_drag,
                active_target_track,
                recording_preview_bounds,
                recording_preview_peaks,
            }));
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
