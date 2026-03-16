use crate::{
    consts::{
        state_ids::METRONOME_TRACK_ID,
        workspace::{
            AUDIO_CLIP_BASE, AUDIO_CLIP_BORDER, AUDIO_CLIP_SELECTED_BASE,
            AUDIO_CLIP_SELECTED_BORDER, CLIP_RESIZE_HANDLE_WIDTH, MIDI_CLIP_BASE, MIDI_CLIP_BORDER,
            MIDI_CLIP_SELECTED_BASE, MIDI_CLIP_SELECTED_BORDER,
        },
        workspace_editor::{CHECKPOINTS, MAX_RENDER_COLUMNS, RENDER_MARGIN_COLUMNS},
    },
    message::{DraggedClip, Message, SnapMode},
    state::{ClipPeaks, MidiClipPreviewMap, PianoNote, State, StateData, Track},
};
use iced::{
    Background, Border, Color, Element, Length, Point, Rectangle, Renderer, Theme, gradient, mouse,
    widget::{
        Space, Stack, canvas,
        canvas::{Frame, Geometry, Path},
        column, container, mouse_area, pin, row, text,
    },
};
use maolan_engine::kind::Kind;
use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::Arc,
};
use wavers::Wav;
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

fn clean_clip_name(name: &str) -> String {
    let mut cleaned = name.to_string();
    if let Some(stripped) = cleaned.strip_prefix("audio/") {
        cleaned = stripped.to_string();
    }
    if let Some(stripped) = cleaned.strip_prefix("midi/") {
        cleaned = stripped.to_string();
    }
    if let Some(stripped) = cleaned.strip_suffix(".wav") {
        cleaned = stripped.to_string();
    }
    if let Some(stripped) = cleaned.strip_suffix(".midi") {
        cleaned = stripped.to_string();
    } else if let Some(stripped) = cleaned.strip_suffix(".mid") {
        cleaned = stripped.to_string();
    }
    cleaned
}

fn trim_label_to_width(label: &str, width_px: f32) -> String {
    let max_chars = ((width_px - 10.0) / 7.0).floor() as i32;
    if max_chars <= 0 {
        return String::new();
    }
    let max_chars = max_chars as usize;
    if label.chars().count() <= max_chars {
        return label.to_string();
    }
    label.chars().take(max_chars).collect()
}

fn clip_label_overlay(label: String) -> Element<'static, Message> {
    container(
        column![
            Space::new().height(Length::FillPortion(1)),
            text(label)
                .size(12)
                .width(Length::Fill)
                .align_x(iced::alignment::Horizontal::Left),
            Space::new().height(Length::FillPortion(1)),
        ]
        .width(Length::Fill)
        .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .padding([0, 5])
    .into()
}

fn brighten(color: Color, amount: f32) -> Color {
    Color {
        r: (color.r + amount).min(1.0),
        g: (color.g + amount).min(1.0),
        b: (color.b + amount).min(1.0),
        a: color.a,
    }
}

fn darken(color: Color, amount: f32) -> Color {
    Color {
        r: (color.r - amount).max(0.0),
        g: (color.g - amount).max(0.0),
        b: (color.b - amount).max(0.0),
        a: color.a,
    }
}

fn clip_two_edge_gradient(
    base: Color,
    muted_alpha: f32,
    normal_alpha: f32,
    reverse: bool,
) -> Background {
    let alpha = normal_alpha;
    let (edge, center) = if reverse {
        (
            Color {
                a: alpha,
                ..darken(base, 0.05)
            },
            Color {
                a: alpha,
                ..brighten(base, 0.06)
            },
        )
    } else {
        (
            Color {
                a: alpha,
                ..brighten(base, 0.06)
            },
            Color {
                a: alpha,
                ..darken(base, 0.05)
            },
        )
    };
    let edge_muted = Color {
        a: muted_alpha,
        ..edge
    };
    let center_muted = Color {
        a: muted_alpha,
        ..center
    };

    let (top_bottom, middle) = if muted_alpha < normal_alpha {
        (edge_muted, center_muted)
    } else {
        (edge, center)
    };
    Background::Gradient(
        gradient::Linear::new(0.0)
            .add_stop(0.0, top_bottom)
            .add_stop(0.5, middle)
            .add_stop(1.0, top_bottom)
            .into(),
    )
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

#[derive(Default)]
struct WaveformCanvasState {
    cache: canvas::Cache,
    last_hash: Cell<u64>,
}

#[derive(Clone)]
struct WaveformCanvas {
    peaks: ClipPeaks,
    source_wav_path: Option<PathBuf>,
    clip_offset: usize,
    clip_length: usize,
    max_length: usize,
}

impl WaveformCanvas {
    fn shape_hash(&self, bounds: Rectangle) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        bounds.width.to_bits().hash(&mut hasher);
        bounds.height.to_bits().hash(&mut hasher);
        self.clip_offset.hash(&mut hasher);
        self.clip_length.hash(&mut hasher);
        self.max_length.hash(&mut hasher);
        self.peaks.len().hash(&mut hasher);
        for channel in self.peaks.iter() {
            channel.len().hash(&mut hasher);
            if channel.is_empty() {
                continue;
            }
            // Sample multiple checkpoints so streamed peak updates invalidate cache quickly
            // without hashing every bin.
            for i in 0..CHECKPOINTS {
                let idx = (i * channel.len()) / CHECKPOINTS;
                let sample = channel[idx.min(channel.len() - 1)];
                sample[0].to_bits().hash(&mut hasher);
                sample[1].to_bits().hash(&mut hasher);
            }
        }
        hasher.finish()
    }

    fn aggregate_column_peak(
        channel_peaks: &[[f32; 2]],
        src_start: usize,
        src_end: usize,
    ) -> Option<(f32, f32)> {
        if src_start >= src_end || src_end > channel_peaks.len() {
            return None;
        }
        let mut min_val = 1.0_f32;
        let mut max_val = -1.0_f32;
        for pair in &channel_peaks[src_start..src_end] {
            min_val = min_val.min(pair[0].clamp(-1.0, 1.0));
            max_val = max_val.max(pair[1].clamp(-1.0, 1.0));
        }
        Some((min_val, max_val))
    }

    fn source_column_peaks(
        source_wav_path: &PathBuf,
        channel_count: usize,
        source_start_sample: usize,
        source_end_sample: usize,
        total_columns: usize,
    ) -> Option<Vec<Vec<[f32; 2]>>> {
        if total_columns == 0 || source_end_sample <= source_start_sample || channel_count == 0 {
            return None;
        }
        let mut wav = Wav::<f32>::from_path(source_wav_path).ok()?;
        let wav_channels = wav.n_channels().max(1) as usize;
        let use_channels = channel_count.min(wav_channels).max(1);
        let total_frames = wav.n_samples() / wav_channels;
        if source_start_sample >= total_frames {
            return None;
        }
        let read_end = source_end_sample.min(total_frames);
        let read_frames = read_end.saturating_sub(source_start_sample);
        if read_frames == 0 {
            return None;
        }

        wav.to_data().ok()?;
        wav.seek_by_samples((source_start_sample.saturating_mul(wav_channels)) as u64)
            .ok()?;
        let chunk = wav
            .read_samples(read_frames.saturating_mul(wav_channels))
            .ok()?;
        if chunk.is_empty() {
            return None;
        }

        let mut out = vec![vec![[0.0_f32, 0.0_f32]; total_columns]; channel_count];
        for col in 0..total_columns {
            let frame_start = (col * read_frames) / total_columns;
            let mut frame_end = ((col + 1) * read_frames) / total_columns;
            if frame_end <= frame_start {
                frame_end = (frame_start + 1).min(read_frames);
            }
            if frame_start >= frame_end {
                continue;
            }
            for (ch, out_channel) in out.iter_mut().enumerate().take(use_channels) {
                let mut min_val = 1.0_f32;
                let mut max_val = -1.0_f32;
                for frame_idx in frame_start..frame_end {
                    let sample_idx = frame_idx.saturating_mul(wav_channels).saturating_add(ch);
                    let s = chunk
                        .get(sample_idx)
                        .copied()
                        .unwrap_or(0.0)
                        .clamp(-1.0, 1.0);
                    min_val = min_val.min(s);
                    max_val = max_val.max(s);
                }
                out_channel[col] = [min_val, max_val];
            }
        }

        Some(out)
    }
}

impl canvas::Program<Message> for WaveformCanvas {
    type State = WaveformCanvasState;

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        if self.peaks.is_empty() || bounds.width <= 0.0 || bounds.height <= 0.0 {
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
                let inner_w = bounds.width.max(4.0);
                let inner_h = bounds.height.max(4.0);
                let channel_count = self.peaks.len().max(1);
                let channel_h = inner_h / channel_count as f32;
                let waveform_fill = Color::from_rgba(0.86, 0.94, 1.0, 0.34);
                let waveform_edge = Color::from_rgba(0.96, 0.98, 1.0, 0.62);
                let zero_line = Color::from_rgba(0.74, 0.86, 1.0, 0.28);
                let clip_color = Color::from_rgba(1.0, 0.42, 0.30, 0.78);
                let clip_level = 0.90_f32;
                let edge_shade = darken(waveform_fill, 0.08);

                for (channel_idx, channel_peaks) in self.peaks.iter().enumerate() {
                    if channel_peaks.is_empty() {
                        continue;
                    }
                    let channel_top = channel_h * channel_idx as f32;
                    let center_y = channel_top + channel_h * 0.5;
                    let half_span = (channel_h * 0.45).max(1.0);
                    let total_peaks = channel_peaks.len();
                    let max_len = self.max_length.max(1);
                    let start_idx = ((self.clip_offset * total_peaks) / max_len)
                        .min(total_peaks.saturating_sub(1));
                    let clip_end_sample = self
                        .clip_offset
                        .saturating_add(self.clip_length)
                        .min(max_len);
                    let mut end_idx = ((clip_end_sample * total_peaks) / max_len).min(total_peaks);
                    if end_idx <= start_idx {
                        end_idx = (start_idx + 1).min(total_peaks);
                    }
                    let visible_bins = end_idx.saturating_sub(start_idx).max(1);
                    let visible_columns =
                        inner_w.ceil().max(1.0).min(MAX_RENDER_COLUMNS as f32) as usize;
                    let x_step = inner_w / visible_columns as f32;
                    let margin_columns = RENDER_MARGIN_COLUMNS;
                    let total_columns = visible_columns + (margin_columns * 2);
                    let margin_bins = ((visible_bins * margin_columns) / visible_columns).max(1);
                    let render_start_idx = start_idx.saturating_sub(margin_bins);
                    let render_end_idx = end_idx.saturating_add(margin_bins).min(total_peaks);
                    let render_bins = render_end_idx.saturating_sub(render_start_idx).max(1);
                    let stored_samples_per_bin = max_len as f32 / total_peaks.max(1) as f32;
                    let visible_source_samples =
                        clip_end_sample.saturating_sub(self.clip_offset).max(1);
                    let required_samples_per_column =
                        visible_source_samples as f32 / visible_columns.max(1) as f32;
                    let high_zoom_source_mode = required_samples_per_column < 1.0;
                    let trace_mode = high_zoom_source_mode
                        || required_samples_per_column <= 4.0
                        || visible_bins <= visible_columns.saturating_mul(2);
                    let use_source_columns = self.source_wav_path.is_some()
                        && required_samples_per_column + f32::EPSILON < stored_samples_per_bin;
                    let mut source_mode_columns = total_columns;
                    let mut source_mode_margin = margin_columns;
                    let mut source_mode_x_step = x_step;
                    let mut source_mode_bin_w = x_step.max(1.0);
                    let source_columns = if use_source_columns {
                        let source_margin_samples = if high_zoom_source_mode {
                            margin_columns
                        } else {
                            ((visible_source_samples * margin_columns) / visible_columns).max(1)
                        };
                        if high_zoom_source_mode {
                            source_mode_columns =
                                visible_source_samples + (source_margin_samples * 2);
                            source_mode_margin = source_margin_samples;
                            source_mode_x_step = inner_w / visible_source_samples.max(1) as f32;
                            source_mode_bin_w = 1.0;
                        }
                        let source_start = self.clip_offset.saturating_sub(source_margin_samples);
                        let source_end = clip_end_sample
                            .saturating_add(source_margin_samples)
                            .min(self.max_length.max(1));
                        self.source_wav_path.as_ref().and_then(|path| {
                            Self::source_column_peaks(
                                path,
                                self.peaks.len(),
                                source_start,
                                source_end,
                                source_mode_columns,
                            )
                        })
                    } else {
                        None
                    };

                    frame.fill(
                        &Path::rectangle(Point::new(0.0, center_y), iced::Size::new(inner_w, 1.0)),
                        zero_line,
                    );

                    let draw_columns = if source_columns.is_some() {
                        source_mode_columns
                    } else {
                        total_columns
                    };
                    if trace_mode {
                        let trace = Path::new(|builder| {
                            let mut started = false;
                            for col in 0..draw_columns {
                                let pair = if let Some(columns) = source_columns.as_ref() {
                                    columns
                                        .get(channel_idx)
                                        .and_then(|ch| ch.get(col))
                                        .copied()
                                        .unwrap_or([0.0, 0.0])
                                } else {
                                    let src_start = render_start_idx
                                        + ((col * render_bins) / draw_columns).min(render_bins);
                                    let mut src_end = render_start_idx
                                        + (((col + 1) * render_bins) / draw_columns)
                                            .min(render_bins);
                                    if src_end <= src_start {
                                        src_end = (src_start + 1).min(total_peaks);
                                    }
                                    let pair = Self::aggregate_column_peak(
                                        channel_peaks,
                                        src_start,
                                        src_end,
                                    )
                                    .unwrap_or((0.0, 0.0));
                                    [pair.0, pair.1]
                                };
                                let sample = ((pair[0] + pair[1]) * 0.5).clamp(-1.0, 1.0);
                                let x = if source_columns.is_some() {
                                    (col as f32 - source_mode_margin as f32) * source_mode_x_step
                                } else {
                                    (col as f32 - margin_columns as f32) * x_step
                                };
                                let y = (center_y - (sample * half_span))
                                    .clamp(channel_top, channel_top + channel_h);
                                if !started {
                                    builder.move_to(Point::new(x, y));
                                    started = true;
                                } else {
                                    builder.line_to(Point::new(x, y));
                                }
                            }
                        });
                        frame.stroke(
                            &trace,
                            canvas::Stroke::default()
                                .with_color(waveform_edge)
                                .with_width(1.0),
                        );
                        continue;
                    }

                    for col in 0..draw_columns {
                        let (min_val, max_val) = if let Some(columns) = source_columns.as_ref() {
                            let pair = columns
                                .get(channel_idx)
                                .and_then(|ch| ch.get(col))
                                .copied()
                                .unwrap_or([0.0, 0.0]);
                            (pair[0], pair[1])
                        } else {
                            let src_start = render_start_idx
                                + ((col * render_bins) / total_columns).min(render_bins);
                            let mut src_end = render_start_idx
                                + (((col + 1) * render_bins) / total_columns).min(render_bins);
                            if src_end <= src_start {
                                src_end = (src_start + 1).min(total_peaks);
                            }
                            let Some(pair) =
                                Self::aggregate_column_peak(channel_peaks, src_start, src_end)
                            else {
                                continue;
                            };
                            pair
                        };
                        let top = (center_y - (max_val * half_span))
                            .clamp(channel_top, channel_top + channel_h);
                        let bottom = (center_y - (min_val * half_span))
                            .clamp(channel_top, channel_top + channel_h);
                        let y = top.min(bottom);
                        let h = (bottom - top).abs().max(1.0);
                        let (x, bin_w) = if source_columns.is_some() {
                            (
                                (col as f32 - source_mode_margin as f32) * source_mode_x_step,
                                source_mode_bin_w,
                            )
                        } else {
                            (
                                (col as f32 - margin_columns as f32) * x_step,
                                x_step.max(1.0),
                            )
                        };

                        frame.fill(
                            &Path::rectangle(Point::new(x, y), iced::Size::new(bin_w, h)),
                            waveform_fill,
                        );
                        let edge_h = (h * 0.2).clamp(1.0, 3.0);
                        frame.fill(
                            &Path::rectangle(Point::new(x, y), iced::Size::new(bin_w, edge_h)),
                            edge_shade,
                        );
                        frame.fill(
                            &Path::rectangle(
                                Point::new(x, y + h - edge_h),
                                iced::Size::new(bin_w, edge_h),
                            ),
                            edge_shade,
                        );

                        if h >= 3.0 {
                            frame.fill(
                                &Path::rectangle(Point::new(x, y), iced::Size::new(bin_w, 1.0)),
                                waveform_edge,
                            );
                            frame.fill(
                                &Path::rectangle(
                                    Point::new(x, y + h - 1.0),
                                    iced::Size::new(bin_w, 1.0),
                                ),
                                waveform_edge,
                            );
                        }

                        if max_val >= clip_level {
                            let clip_h = h.clamp(1.0, 3.0);
                            frame.fill(
                                &Path::rectangle(Point::new(x, y), iced::Size::new(bin_w, clip_h)),
                                clip_color,
                            );
                        }
                        if -min_val >= clip_level {
                            let clip_h = h.clamp(1.0, 3.0);
                            frame.fill(
                                &Path::rectangle(
                                    Point::new(x, y + h - clip_h),
                                    iced::Size::new(bin_w, clip_h),
                                ),
                                clip_color,
                            );
                        }
                    }
                }
            });
        vec![geom]
    }
}

#[derive(Default)]
struct MidiClipNotesCanvasState {
    cache: canvas::Cache,
    last_hash: Cell<u64>,
}

#[derive(Clone)]
struct MidiClipNotesCanvas {
    notes: Arc<Vec<PianoNote>>,
    clip_offset_samples: usize,
    clip_visible_length_samples: usize,
}

impl MidiClipNotesCanvas {
    fn shape_hash(&self, bounds: Rectangle) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        bounds.width.to_bits().hash(&mut hasher);
        bounds.height.to_bits().hash(&mut hasher);
        self.clip_offset_samples.hash(&mut hasher);
        self.clip_visible_length_samples.hash(&mut hasher);
        self.notes.len().hash(&mut hasher);
        if let Some(first) = self.notes.first() {
            first.start_sample.hash(&mut hasher);
            first.length_samples.hash(&mut hasher);
            first.pitch.hash(&mut hasher);
            first.velocity.hash(&mut hasher);
        }
        if let Some(last) = self.notes.last() {
            last.start_sample.hash(&mut hasher);
            last.length_samples.hash(&mut hasher);
            last.pitch.hash(&mut hasher);
            last.velocity.hash(&mut hasher);
        }
        hasher.finish()
    }
}

impl canvas::Program<Message> for MidiClipNotesCanvas {
    type State = MidiClipNotesCanvasState;

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        if self.notes.is_empty() || bounds.width <= 0.0 || bounds.height <= 0.0 {
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
                let inner_w = bounds.width.max(1.0);
                let inner_h = bounds.height.max(1.0);
                let visible_start = self.clip_offset_samples;
                let visible_len = self.clip_visible_length_samples.max(1);
                let visible_end = visible_start.saturating_add(visible_len);
                let clip_len = visible_len as f32;
                // Keep note preview mapping stable: always show 10 octaves (C-1..B8 => 0..119).
                let min_pitch = 0_u8;
                let max_pitch = 119_u8;
                let pitch_span = 120.0_f32;
                let note_color = Color::from_rgba(0.68, 0.92, 0.40, 0.82);
                let note_edge = Color::from_rgba(0.86, 0.98, 0.62, 0.95);
                let grid_major = Color::from_rgba(0.74, 0.95, 0.58, 0.14);
                let grid_minor = Color::from_rgba(0.62, 0.86, 0.48, 0.07);
                let horizon = Color::from_rgba(0.88, 0.98, 0.72, 0.22);

                for step in 0..=16 {
                    let x = (step as f32 / 16.0) * inner_w;
                    let color = if step % 4 == 0 {
                        grid_major
                    } else {
                        grid_minor
                    };
                    frame.stroke(
                        &Path::line(Point::new(x, 0.0), Point::new(x, inner_h)),
                        canvas::Stroke::default().with_color(color).with_width(1.0),
                    );
                }

                for row in 0..=10 {
                    let y = (row as f32 / 10.0) * inner_h;
                    frame.stroke(
                        &Path::line(Point::new(0.0, y), Point::new(inner_w, y)),
                        canvas::Stroke::default()
                            .with_color(if row % 2 == 0 { grid_minor } else { grid_major })
                            .with_width(0.5),
                    );
                }
                let horizon_y = inner_h * 0.84;
                frame.stroke(
                    &Path::line(Point::new(0.0, horizon_y), Point::new(inner_w, horizon_y)),
                    canvas::Stroke::default()
                        .with_color(horizon)
                        .with_width(1.0),
                );

                for note in self.notes.iter() {
                    let note_start = note.start_sample;
                    let note_end = note.start_sample.saturating_add(note.length_samples.max(1));
                    if note_end <= visible_start || note_start >= visible_end {
                        continue;
                    }
                    let pitch = note.pitch.clamp(min_pitch, max_pitch);
                    let clipped_start = note_start.max(visible_start);
                    let clipped_end = note_end.min(visible_end);
                    let rel_start = clipped_start.saturating_sub(visible_start);
                    let rel_len = clipped_end.saturating_sub(clipped_start).max(1);
                    let x = (rel_start as f32 / clip_len) * inner_w;
                    let w = ((rel_len as f32 / clip_len) * inner_w).max(1.0);
                    let pitch_pos = (i16::from(max_pitch) - i16::from(pitch)) as f32 / pitch_span;
                    let y = pitch_pos * inner_h;
                    let h = (inner_h / pitch_span).clamp(1.0, 8.0);
                    let rect = Path::rectangle(Point::new(x, y), iced::Size::new(w, h));
                    frame.fill(&rect, note_color);
                    frame.stroke(
                        &rect,
                        canvas::Stroke::default()
                            .with_color(note_edge)
                            .with_width(0.5),
                    );
                }
            });

        vec![geom]
    }
}

fn midi_clip_notes_overlay(
    notes: Arc<Vec<PianoNote>>,
    clip_offset_samples: usize,
    clip_visible_length_samples: usize,
) -> Element<'static, Message> {
    canvas(MidiClipNotesCanvas {
        notes,
        clip_offset_samples,
        clip_visible_length_samples,
    })
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn audio_waveform_overlay(
    peaks: ClipPeaks,
    source_wav_path: Option<PathBuf>,
    _clip_width: f32,
    _clip_height: f32,
    clip_offset: usize,
    clip_length: usize,
    max_length: usize,
) -> Element<'static, Message> {
    canvas(WaveformCanvas {
        peaks,
        source_wav_path,
        clip_offset,
        clip_length,
        max_length,
    })
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
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
                let step = self.bar_pixels.max(1.0);
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
    let resolve_audio_clip_path = |clip_name: &str| -> Option<PathBuf> {
        let path = PathBuf::from(clip_name);
        if path.is_absolute() {
            return Some(path);
        }
        session_root.map(|root| root.join(path))
    };
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
                .height(Length::Fixed(layout.header_height))
                .style(|_theme| container::Style {
                    background: Some(Background::Color(Color {
                        r: 0.08,
                        g: 0.08,
                        b: 0.08,
                        a: 0.12,
                    })),
                    ..container::Style::default()
                }),
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
            clean_clip_name(&clip_name),
            if clip.take_lane_pinned { " [P]" } else { "" },
            if clip.take_lane_locked { " [L]" } else { "" }
        );
        let clip_peaks = clip.peaks.clone();
        let clip_muted = clip.muted;
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
        let display_clip_label = trim_label_to_width(&clip_label, clip_width);
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

        let left_edge_zone = mouse_area(
            Space::new()
                .width(Length::Fixed(CLIP_RESIZE_HANDLE_WIDTH))
                .height(Length::Fill),
        )
        .interaction(mouse::Interaction::ResizingColumn)
        .on_enter(Message::ClipResizeHandleHover {
            kind: Kind::Audio,
            track_idx: track_name_cloned.clone(),
            clip_idx: index,
            is_right_side: false,
            hovered: true,
        })
        .on_exit(Message::ClipResizeHandleHover {
            kind: Kind::Audio,
            track_idx: track_name_cloned.clone(),
            clip_idx: index,
            is_right_side: false,
            hovered: false,
        })
        .on_press(Message::ClipResizeStart(
            Kind::Audio,
            track_name_cloned.clone(),
            index,
            false,
        ));

        let right_edge_zone = mouse_area(
            Space::new()
                .width(Length::Fixed(CLIP_RESIZE_HANDLE_WIDTH))
                .height(Length::Fill),
        )
        .interaction(mouse::Interaction::ResizingColumn)
        .on_enter(Message::ClipResizeHandleHover {
            kind: Kind::Audio,
            track_idx: track_name_cloned.clone(),
            clip_idx: index,
            is_right_side: true,
            hovered: true,
        })
        .on_exit(Message::ClipResizeHandleHover {
            kind: Kind::Audio,
            track_idx: track_name_cloned.clone(),
            clip_idx: index,
            is_right_side: true,
            hovered: false,
        })
        .on_press(Message::ClipResizeStart(
            Kind::Audio,
            track_name_cloned.clone(),
            index,
            true,
        ));

        let clip_content = container(Stack::with_children(vec![
            audio_waveform_overlay(
                clip_peaks.clone(),
                resolve_audio_clip_path(&clip_name),
                clip_width,
                clip_height,
                clip.offset,
                clip.length,
                clip.max_length_samples,
            ),
            clip_label_overlay(display_clip_label.clone()),
        ]))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(0)
        .style(move |_theme| {
            use container::Style;
            let base = if is_selected {
                AUDIO_CLIP_SELECTED_BASE
            } else {
                AUDIO_CLIP_BASE
            };
            let (muted_alpha, normal_alpha) = if clip_muted { (0.45, 0.45) } else { (1.0, 1.0) };
            Style {
                background: Some(clip_two_edge_gradient(
                    base,
                    muted_alpha,
                    normal_alpha,
                    true,
                )),
                ..Style::default()
            }
        });

        let clip_widget = container(Stack::with_children(vec![
            clip_content.into(),
            pin(left_edge_zone).position(Point::new(0.0, 0.0)).into(),
            pin(right_edge_zone)
                .position(Point::new(clip_width - CLIP_RESIZE_HANDLE_WIDTH, 0.0))
                .into(),
        ]))
        .width(Length::Fixed(clip_width))
        .height(Length::Fixed(clip_height))
        .style(move |_theme| container::Style {
            background: None,
            border: Border {
                color: if is_selected {
                    AUDIO_CLIP_SELECTED_BORDER
                } else {
                    AUDIO_CLIP_BORDER
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
            let clip_with_mouse = {
                let base = mouse_area(clip_with_fades);
                let base = if audio_left_handle_hovered || audio_right_handle_hovered {
                    base.interaction(mouse::Interaction::ResizingColumn)
                } else {
                    base
                };
                let base = base.on_press(Message::SelectClip {
                    track_idx: track_name_cloned.clone(),
                    clip_idx: index,
                    kind: Kind::Audio,
                });
                if !clip.take_lane_locked {
                    let track_name_for_drag_closure = track_name_cloned.clone();
                    base.on_move(move |point| {
                        let mut clip_data = DraggedClip::new(
                            Kind::Audio,
                            index,
                            track_name_for_drag_closure.clone(),
                        );
                        clip_data.start = point;
                        Message::ClipDrag(clip_data)
                    })
                } else {
                    base
                }
            };

            clips.push(
                pin(clip_with_mouse)
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
            let preview_content = container(Stack::with_children(vec![
                audio_waveform_overlay(
                    clip_peaks,
                    resolve_audio_clip_path(&clip_name),
                    clip_width,
                    clip_height,
                    clip.offset,
                    clip.length,
                    clip.max_length_samples,
                ),
                clip_label_overlay(display_clip_label.clone()),
            ]))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(0)
            .style(move |_theme| {
                use container::Style;
                Style {
                    background: Some(preview_fill),
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
            .style(move |_theme| container::Style {
                background: None,
                border: Border {
                    color: preview_border,
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
        let clip_label = format!(
            "{}{}{}",
            clean_clip_name(&clip_name),
            if clip.take_lane_pinned { " [P]" } else { "" },
            if clip.take_lane_locked { " [L]" } else { "" }
        );
        let clip_muted = clip.muted;
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
        let display_clip_label = trim_label_to_width(&clip_label, clip_width);
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

        let left_edge_zone = mouse_area(
            Space::new()
                .width(Length::Fixed(CLIP_RESIZE_HANDLE_WIDTH))
                .height(Length::Fill),
        )
        .interaction(mouse::Interaction::ResizingColumn)
        .on_enter(Message::ClipResizeHandleHover {
            kind: Kind::MIDI,
            track_idx: track_name_cloned.clone(),
            clip_idx: index,
            is_right_side: false,
            hovered: true,
        })
        .on_exit(Message::ClipResizeHandleHover {
            kind: Kind::MIDI,
            track_idx: track_name_cloned.clone(),
            clip_idx: index,
            is_right_side: false,
            hovered: false,
        })
        .on_press(Message::ClipResizeStart(
            Kind::MIDI,
            track_name_cloned.clone(),
            index,
            false,
        ));

        let right_edge_zone = mouse_area(
            Space::new()
                .width(Length::Fixed(CLIP_RESIZE_HANDLE_WIDTH))
                .height(Length::Fill),
        )
        .interaction(mouse::Interaction::ResizingColumn)
        .on_enter(Message::ClipResizeHandleHover {
            kind: Kind::MIDI,
            track_idx: track_name_cloned.clone(),
            clip_idx: index,
            is_right_side: true,
            hovered: true,
        })
        .on_exit(Message::ClipResizeHandleHover {
            kind: Kind::MIDI,
            track_idx: track_name_cloned.clone(),
            clip_idx: index,
            is_right_side: true,
            hovered: false,
        })
        .on_press(Message::ClipResizeStart(
            Kind::MIDI,
            track_name_cloned.clone(),
            index,
            true,
        ));

        let mut clip_layers = Vec::with_capacity(2);
        if let Some(notes) = midi_notes_for_clip.as_ref() {
            clip_layers.push(midi_clip_notes_overlay(
                notes.clone(),
                clip.offset,
                clip.length.max(1),
            ));
        }
        clip_layers.push(clip_label_overlay(display_clip_label.clone()));

        let clip_content = container(Stack::with_children(clip_layers))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(0)
            .style(move |_theme| {
                use container::Style;
                let base = if is_selected {
                    MIDI_CLIP_SELECTED_BASE
                } else {
                    MIDI_CLIP_BASE
                };
                let (muted_alpha, normal_alpha) = if clip_muted {
                    (0.42, 0.42)
                } else {
                    (0.92, 0.92)
                };
                Style {
                    background: Some(clip_two_edge_gradient(
                        base,
                        muted_alpha,
                        normal_alpha,
                        false,
                    )),
                    ..Style::default()
                }
            });

        let clip_widget = container(Stack::with_children(vec![
            clip_content.into(),
            pin(left_edge_zone).position(Point::new(0.0, 0.0)).into(),
            pin(right_edge_zone)
                .position(Point::new(clip_width - CLIP_RESIZE_HANDLE_WIDTH, 0.0))
                .into(),
        ]))
        .width(Length::Fixed(clip_width))
        .height(Length::Fixed(clip_height))
        .style(move |_theme| container::Style {
            background: None,
            border: Border {
                color: if is_selected {
                    MIDI_CLIP_SELECTED_BORDER
                } else {
                    MIDI_CLIP_BORDER
                },
                width: if is_selected { 2.2 } else { 1.4 },
                radius: 8.0.into(),
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
                                g: 0.96,
                                b: 0.62,
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
                                b: 0.72,
                                a: 0.9,
                            })),
                            border: Border {
                                color: Color {
                                    r: 0.24,
                                    g: 0.42,
                                    b: 0.20,
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
                                g: 0.96,
                                b: 0.62,
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
                                b: 0.72,
                                a: 0.9,
                            })),
                            border: Border {
                                color: Color {
                                    r: 0.24,
                                    g: 0.42,
                                    b: 0.20,
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
            let clip_with_mouse = {
                let base = mouse_area(clip_with_fades);
                let base = if midi_left_handle_hovered || midi_right_handle_hovered {
                    base.interaction(mouse::Interaction::ResizingColumn)
                } else {
                    base
                };
                let base = base
                    .on_press(Message::SelectClip {
                        track_idx: track_name_cloned.clone(),
                        clip_idx: index,
                        kind: Kind::MIDI,
                    })
                    .on_double_click(Message::OpenMidiPiano {
                        track_idx: track_name_cloned.clone(),
                        clip_idx: index,
                    });
                if !clip.take_lane_locked {
                    let track_name_for_drag_closure = track_name_cloned.clone();
                    base.on_move(move |point| {
                        let mut clip_data = DraggedClip::new(
                            Kind::MIDI,
                            index,
                            track_name_for_drag_closure.clone(),
                        );
                        clip_data.start = point;
                        Message::ClipDrag(clip_data)
                    })
                } else {
                    base
                }
            };

            clips.push(
                pin(clip_with_mouse)
                    .position(Point::new(dragged_start * pixels_per_sample, lane_top))
                    .into(),
            );
        }

        if let Some(drag) = drag_for_clip.filter(|_| show_preview_in_this_track) {
            let delta_samples = (drag.end.x - drag.start.x) / pixels_per_sample.max(1.0e-6);
            let preview_start = snap_sample(clip.start as f32 + delta_samples, delta_samples);
            let preview_fill = if active_target_valid {
                clip_two_edge_gradient(MIDI_CLIP_SELECTED_BASE, 0.66, 0.66, false)
            } else {
                clip_two_edge_gradient(
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
            let mut preview_layers = Vec::with_capacity(2);
            if let Some(notes) = midi_notes_for_clip.as_ref() {
                preview_layers.push(midi_clip_notes_overlay(
                    notes.clone(),
                    clip.offset,
                    clip.length.max(1),
                ));
            }
            preview_layers.push(clip_label_overlay(display_clip_label.clone()));
            let preview_content = container(Stack::with_children(preview_layers))
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(0)
                .style(move |_theme| {
                    use container::Style;
                    Style {
                        background: Some(preview_fill),
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
            .style(move |_theme| container::Style {
                background: None,
                border: Border {
                    color: preview_border,
                    width: 2.0,
                    radius: 8.0.into(),
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
                        let preview_fill = if active_target_valid {
                            clip_two_edge_gradient(AUDIO_CLIP_SELECTED_BASE, 0.7, 0.7, true)
                        } else {
                            clip_two_edge_gradient(
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
                        let display_clip_label =
                            trim_label_to_width(&clean_clip_name(&source_clip.name), clip_width);
                        let preview_content = container(Stack::with_children(vec![
                            audio_waveform_overlay(
                                source_clip.peaks.clone(),
                                resolve_audio_clip_path(&source_clip.name),
                                clip_width,
                                clip_height,
                                source_clip.offset,
                                source_clip.length,
                                source_clip.max_length_samples,
                            ),
                            clip_label_overlay(display_clip_label),
                        ]))
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .padding(0)
                        .style(move |_theme| {
                            use container::Style;
                            Style {
                                background: Some(if active_target_valid {
                                    preview_fill
                                } else {
                                    Background::Color(Color::TRANSPARENT)
                                }),
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
                        .style(move |_theme| container::Style {
                            background: Some(if active_target_valid {
                                Background::Color(Color::TRANSPARENT)
                            } else {
                                preview_fill
                            }),
                            border: Border {
                                color: preview_border,
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
                        let preview_fill = if active_target_valid {
                            clip_two_edge_gradient(MIDI_CLIP_SELECTED_BASE, 0.7, 0.7, false)
                        } else {
                            clip_two_edge_gradient(
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
                        let display_clip_label =
                            trim_label_to_width(&clean_clip_name(&source_clip.name), clip_width);
                        let preview_content = container(clip_label_overlay(display_clip_label))
                            .width(Length::Fill)
                            .height(Length::Fill)
                            .padding(0)
                            .style(move |_theme| {
                                use container::Style;
                                Style {
                                    background: Some(if active_target_valid {
                                        preview_fill
                                    } else {
                                        Background::Color(Color::TRANSPARENT)
                                    }),
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
                        .style(move |_theme| container::Style {
                            background: Some(if active_target_valid {
                                Background::Color(Color::TRANSPARENT)
                            } else {
                                preview_fill
                            }),
                            border: Border {
                                color: preview_border,
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
                    preview_peaks,
                    None,
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
    let menu = state.clip_context_menu.as_ref()?;
    let clip = &menu.clip;
    let track = state.tracks.iter().find(|t| t.name == clip.track_idx)?;

    let content: Element<'static, Message> = match clip.kind {
        Kind::Audio => {
            let clip_ref = track.audio.clips.get(clip.clip_idx)?;
            let fade_enabled = clip_ref.fade_enabled;
            let muted = clip_ref.muted;
            let track_idx = clip.track_idx.clone();
            let clip_idx = clip.clip_idx;
            column![
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
            let fade_enabled = clip_ref.fade_enabled;
            let muted = clip_ref.muted;
            let track_idx = clip.track_idx.clone();
            let clip_idx = clip.clip_idx;
            column![
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
                crate::menu::menu_item(
                    if fade_enabled {
                        "Disable Fade"
                    } else {
                        "Enable Fade"
                    },
                    Message::ClipToggleFade {
                        track_idx,
                        clip_idx,
                        kind: Kind::MIDI,
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
            }

            for clip in &track.midi.clips {
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
        } = args;
        let state_handle = self.state;
        let session_root_ref = session_root.as_ref();
        let active_clip_drag_ref = active_clip_drag.as_ref();
        let active_target_track_ref = active_target_track.as_deref();
        let recording_preview_peaks_ref = recording_preview_peaks.as_ref();
        let midi_clip_previews_ref = midi_clip_previews.as_ref();

        let mut result = column![];
        let state = state_handle.blocking_read();
        for track in state
            .tracks
            .iter()
            .filter(|track| track.name != METRONOME_TRACK_ID)
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
