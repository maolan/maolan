use crate::{
    message::{DraggedClip, Message},
    state::{State, StateData, Track},
};
use iced::{
    Background, Border, Color, Element, Length, Point,
    widget::{Stack, column, container, mouse_area, pin, row, text},
};
use iced_drop::droppable;
use maolan_engine::kind::Kind;
use std::collections::HashMap;

fn audio_waveform_overlay(
    peaks: &[Vec<f32>],
    clip_width: f32,
    clip_height: f32,
) -> Element<'static, Message> {
    if peaks.is_empty() {
        return container("").width(Length::Fill).height(Length::Fill).into();
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
                pin(
                    container("")
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
                        }),
                )
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
    recording_preview_bounds: Option<(usize, usize)>,
    recording_preview_peaks: Option<&HashMap<String, Vec<Vec<f32>>>>,
) -> Element<'static, Message> {
    let mut clips: Vec<Element<'static, Message>> = vec![];
    let height = track.height;
    let track_name_cloned = track.name.clone();

    for (index, clip) in track.audio.clips.iter().enumerate() {
        let clip_name = clip.name.clone();
        let clip_peaks = clip.peaks.clone();
        let clip_width = (clip.length as f32 * pixels_per_sample).max(12.0);
        let clip_height = (height - 10.0).max(12.0);
        let is_selected = state.selected_clips.contains(&crate::state::ClipId {
            track_idx: track_name_cloned.clone(),
            clip_idx: index,
            kind: Kind::Audio,
        });

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

        let clip_content = mouse_area(
            container(Stack::with_children(vec![
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
                            r: 0.4,
                            g: 0.6,
                            b: 0.8,
                            a: 1.0,
                        }
                    } else {
                        Color {
                            r: 0.3,
                            g: 0.5,
                            b: 0.7,
                            a: 0.8,
                        }
                    })),
                    ..Style::default()
                }
            }),
        )
        .on_press(Message::SelectClip {
            track_idx: track_name_cloned.clone(),
            clip_idx: index,
            kind: Kind::Audio,
        });

        let clip_widget = container(row![left_handle, clip_content, right_handle])
            .width(Length::Fixed(clip_width))
            .height(Length::Fill)
            .style(|_theme| container::Style {
                background: None,
                border: Border {
                    color: Color {
                        r: 0.2,
                        g: 0.4,
                        b: 0.6,
                        a: 1.0,
                    },
                    width: 1.0,
                    radius: 3.0.into(),
                },
                ..container::Style::default()
            });

        clips.push(
            droppable(
                pin(clip_widget).position(Point::new(clip.start as f32 * pixels_per_sample, 0.0)),
            )
                .on_drag({
                    let track_name_for_drag_closure = track_name_cloned.clone();
                    move |point, _| {
                        let mut clip_data = DraggedClip::new(
                            Kind::Audio,
                            index,
                            track_name_for_drag_closure.clone(),
                        );
                        clip_data.start = point;
                        Message::ClipDrag(clip_data)
                    }
                })
                .on_drop(Message::ClipDropped)
                .into(),
        );
    }
    if track.armed
        && let Some((preview_start, preview_current)) = recording_preview_bounds
        && preview_current > preview_start
    {
        let preview_width = ((preview_current - preview_start) as f32 * pixels_per_sample).max(12.0);
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
                recording_preview_bounds,
                recording_preview_peaks.as_ref(),
            ));
        }
        mouse_area(result.width(Length::Fill).height(Length::Fill))
            .on_press(Message::DeselectAll)
            .into()
    }
}
