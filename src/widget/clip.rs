use crate::{
    consts::{
        widget_piano::PITCH_MAX,
        workspace::{
            AUDIO_CLIP_BASE, AUDIO_CLIP_BORDER, AUDIO_CLIP_SELECTED_BASE,
            AUDIO_CLIP_SELECTED_BORDER, CLIP_RESIZE_HANDLE_WIDTH, MIDI_CLIP_BASE, MIDI_CLIP_BORDER,
            MIDI_CLIP_SELECTED_BASE, MIDI_CLIP_SELECTED_BORDER,
        },
        workspace_editor::{CHECKPOINTS, MAX_RENDER_COLUMNS, RENDER_MARGIN_COLUMNS},
    },
    message::{DraggedClip, Message},
    state::{AudioClip as AudioClipState, ClipPeaks, MIDIClip as MIDIClipState, PianoNote},
};
use iced::{
    Background, Border, Color, Element, Length, Point, Rectangle, Renderer, Theme, gradient, mouse,
    widget::{
        Space, Stack, canvas,
        canvas::{Frame, Geometry, Path},
        column, container, mouse_area, pin, text,
    },
};
use maolan_engine::kind::Kind;
use std::{
    cell::Cell,
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::Arc,
};
use wavers::Wav;

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

fn visible_fade_overlay_width(fade_samples: usize, pixels_per_sample: f32) -> f32 {
    fade_samples as f32 * pixels_per_sample
}

fn should_draw_fade_overlay(fade_samples: usize, pixels_per_sample: f32) -> bool {
    fade_samples as f32 * pixels_per_sample > 3.0
}

#[derive(Debug, Clone, Copy)]
struct FadeBezierCanvas {
    color: Color,
    fade_out: bool,
}

impl canvas::Program<Message> for FadeBezierCanvas {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let start = if self.fade_out {
            Point::new(0.0, 0.0)
        } else {
            Point::new(0.0, bounds.height)
        };
        let end = if self.fade_out {
            Point::new(bounds.width, bounds.height)
        } else {
            Point::new(bounds.width, 0.0)
        };
        let c1 = if self.fade_out {
            Point::new(bounds.width * 0.2, 0.0)
        } else {
            Point::new(bounds.width * 0.2, bounds.height)
        };
        let c2 = if self.fade_out {
            Point::new(bounds.width * 0.8, bounds.height)
        } else {
            Point::new(bounds.width * 0.8, 0.0)
        };
        let fill = Path::new(|builder| {
            if self.fade_out {
                builder.move_to(Point::new(0.0, 0.0));
                builder.line_to(Point::new(bounds.width, 0.0));
                builder.line_to(end);
            } else {
                builder.move_to(Point::new(0.0, 0.0));
                builder.line_to(end);
            }
            builder.bezier_curve_to(c2, c1, start);
            builder.line_to(Point::new(0.0, 0.0));
        });
        frame.fill(&fill, Color::from_rgba(0.0, 0.0, 0.0, 0.22));

        let path = Path::new(|builder| {
            builder.move_to(start);
            builder.bezier_curve_to(c1, c2, end);
        });
        frame.stroke(
            &path,
            canvas::Stroke::default()
                .with_width(1.0)
                .with_color(self.color),
        );
        vec![frame.into_geometry()]
    }
}

fn fade_bezier_overlay(
    width: f32,
    height: f32,
    color: Color,
    fade_out: bool,
) -> Element<'static, Message> {
    canvas(FadeBezierCanvas { color, fade_out })
        .width(Length::Fixed(width.max(0.0)))
        .height(Length::Fixed(height.max(0.0)))
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
                let min_pitch = 0_u8;
                let max_pitch = PITCH_MAX;
                let pitch_span = f32::from(PITCH_MAX) + 1.0;
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

fn resolve_audio_clip_path(session_root: Option<&PathBuf>, clip_name: &str) -> Option<PathBuf> {
    let path = PathBuf::from(clip_name);
    if path.is_absolute() {
        Some(path)
    } else {
        session_root.map(|root| root.join(path))
    }
}

fn grouped_audio_waveform_overlay(
    clip: &AudioClipState,
    session_root: Option<&PathBuf>,
    pixels_per_sample: f32,
    clip_height: f32,
) -> Element<'static, Message> {
    let mut stack = Stack::new();
    for child in &clip.grouped_clips {
        let child_width = (child.length as f32 * pixels_per_sample).max(12.0);
        let child_overlay = if child.is_group() {
            grouped_audio_waveform_overlay(child, session_root, pixels_per_sample, clip_height)
        } else {
            audio_waveform_overlay(
                child.peaks.clone(),
                resolve_audio_clip_path(session_root, &child.name),
                child.offset,
                child.length,
                child.max_length_samples,
            )
        };
        stack = stack.push(
            pin(container(child_overlay)
                .width(Length::Fixed(child_width))
                .height(Length::Fixed(clip_height)))
            .position(Point::new(child.start as f32 * pixels_per_sample, 0.0)),
        );
    }
    container(stack)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

#[derive(Clone, Copy)]
enum AudioClipMode {
    Widget,
    Preview,
}

pub struct AudioClip<'a> {
    clip: &'a AudioClipState,
    session_root: Option<&'a PathBuf>,
    pixels_per_sample: f32,
    clip_width: f32,
    clip_height: f32,
    label: String,
    is_selected: bool,
    left_handle_hovered: bool,
    right_handle_hovered: bool,
    track_name: Option<String>,
    clip_index: usize,
    on_select: Option<Message>,
    on_open: Option<Message>,
    drag_enabled: bool,
    background: Option<Background>,
    border_color: Option<Color>,
    radius: f32,
    mode: AudioClipMode,
}

impl<'a> AudioClip<'a> {
    pub fn new(clip: &'a AudioClipState) -> Self {
        Self {
            clip,
            session_root: None,
            pixels_per_sample: 1.0,
            clip_width: 12.0,
            clip_height: 8.0,
            label: String::new(),
            is_selected: false,
            left_handle_hovered: false,
            right_handle_hovered: false,
            track_name: None,
            clip_index: 0,
            on_select: None,
            on_open: None,
            drag_enabled: false,
            background: None,
            border_color: None,
            radius: 3.0,
            mode: AudioClipMode::Widget,
        }
    }

    pub fn with_session_root(mut self, session_root: Option<&'a PathBuf>) -> Self {
        self.session_root = session_root;
        self
    }

    pub fn with_pixels_per_sample(mut self, pixels_per_sample: f32) -> Self {
        self.pixels_per_sample = pixels_per_sample;
        self
    }

    pub fn with_size(mut self, clip_width: f32, clip_height: f32) -> Self {
        self.clip_width = clip_width;
        self.clip_height = clip_height;
        self
    }

    pub fn with_label(mut self, label: String) -> Self {
        self.label = label;
        self
    }

    pub fn selected(mut self, is_selected: bool) -> Self {
        self.is_selected = is_selected;
        self
    }

    pub fn hovered_handles(
        mut self,
        left_handle_hovered: bool,
        right_handle_hovered: bool,
    ) -> Self {
        self.left_handle_hovered = left_handle_hovered;
        self.right_handle_hovered = right_handle_hovered;
        self
    }

    pub fn interactive(
        mut self,
        track_name: String,
        clip_index: usize,
        on_select: Message,
        on_open: Message,
        drag_enabled: bool,
    ) -> Self {
        self.track_name = Some(track_name);
        self.clip_index = clip_index;
        self.on_select = Some(on_select);
        self.on_open = Some(on_open);
        self.drag_enabled = drag_enabled;
        self.mode = AudioClipMode::Widget;
        self
    }

    pub fn preview(mut self, background: Background, border_color: Color) -> Self {
        self.background = Some(background);
        self.border_color = Some(border_color);
        self.mode = AudioClipMode::Preview;
        self
    }

    pub fn clean_name(name: &str) -> String {
        clean_clip_name(name)
    }

    pub fn label_for_width(label: &str, width_px: f32) -> String {
        trim_label_to_width(label, width_px)
    }

    pub fn two_edge_gradient(
        base: Color,
        muted_alpha: f32,
        normal_alpha: f32,
        reverse: bool,
    ) -> Background {
        clip_two_edge_gradient(base, muted_alpha, normal_alpha, reverse)
    }

    pub fn waveform_overlay(
        peaks: ClipPeaks,
        source_wav_path: Option<PathBuf>,
        clip_offset: usize,
        clip_length: usize,
        max_length: usize,
    ) -> Element<'static, Message> {
        audio_waveform_overlay(peaks, source_wav_path, clip_offset, clip_length, max_length)
    }

    pub fn into_element(self) -> Element<'static, Message> {
        match self.mode {
            AudioClipMode::Preview => {
                let preview_content = container(Stack::with_children(vec![
                    audio_waveform_overlay(
                        self.clip.peaks.clone(),
                        resolve_audio_clip_path(self.session_root, &self.clip.name),
                        self.clip.offset,
                        self.clip.length,
                        self.clip.max_length_samples,
                    ),
                    clip_label_overlay(self.label),
                ]))
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(0)
                .style(move |_theme| {
                    use container::Style;
                    Style {
                        background: self.background,
                        ..Style::default()
                    }
                });
                container(preview_content)
                    .width(Length::Fixed(self.clip_width))
                    .height(Length::Fixed(self.clip_height))
                    .style(move |_theme| container::Style {
                        background: None,
                        border: Border {
                            color: self.border_color.unwrap_or(Color::TRANSPARENT),
                            width: 2.0,
                            radius: self.radius.into(),
                        },
                        ..container::Style::default()
                    })
                    .into()
            }
            AudioClipMode::Widget => {
                let track_name = self.track_name.expect("audio clip widget track name");
                let on_select = self.on_select.expect("audio clip widget on_select");
                let on_open = self.on_open.expect("audio clip widget on_open");
                let clip_muted = self.clip.muted;
                let left_edge_zone = mouse_area(
                    Space::new()
                        .width(Length::Fixed(CLIP_RESIZE_HANDLE_WIDTH))
                        .height(Length::Fill),
                )
                .interaction(mouse::Interaction::ResizingColumn)
                .on_enter(Message::ClipResizeHandleHover {
                    kind: Kind::Audio,
                    track_idx: track_name.clone(),
                    clip_idx: self.clip_index,
                    is_right_side: false,
                    hovered: true,
                })
                .on_exit(Message::ClipResizeHandleHover {
                    kind: Kind::Audio,
                    track_idx: track_name.clone(),
                    clip_idx: self.clip_index,
                    is_right_side: false,
                    hovered: false,
                })
                .on_press(Message::ClipResizeStart(
                    Kind::Audio,
                    track_name.clone(),
                    self.clip_index,
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
                    track_idx: track_name.clone(),
                    clip_idx: self.clip_index,
                    is_right_side: true,
                    hovered: true,
                })
                .on_exit(Message::ClipResizeHandleHover {
                    kind: Kind::Audio,
                    track_idx: track_name.clone(),
                    clip_idx: self.clip_index,
                    is_right_side: true,
                    hovered: false,
                })
                .on_press(Message::ClipResizeStart(
                    Kind::Audio,
                    track_name.clone(),
                    self.clip_index,
                    true,
                ));

                let clip_content = container(Stack::with_children(vec![
                    if self.clip.is_group() {
                        grouped_audio_waveform_overlay(
                            self.clip,
                            self.session_root,
                            self.pixels_per_sample,
                            self.clip_height,
                        )
                    } else {
                        audio_waveform_overlay(
                            self.clip.peaks.clone(),
                            resolve_audio_clip_path(self.session_root, &self.clip.name),
                            self.clip.offset,
                            self.clip.length,
                            self.clip.max_length_samples,
                        )
                    },
                    clip_label_overlay(self.label),
                ]))
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(0)
                .style(move |_theme| {
                    use container::Style;
                    let base = if self.is_selected {
                        AUDIO_CLIP_SELECTED_BASE
                    } else {
                        AUDIO_CLIP_BASE
                    };
                    let (muted_alpha, normal_alpha) =
                        if clip_muted { (0.45, 0.45) } else { (1.0, 1.0) };
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
                        .position(Point::new(self.clip_width - CLIP_RESIZE_HANDLE_WIDTH, 0.0))
                        .into(),
                ]))
                .width(Length::Fixed(self.clip_width))
                .height(Length::Fixed(self.clip_height))
                .style(move |_theme| container::Style {
                    background: None,
                    border: Border {
                        color: if self.is_selected {
                            AUDIO_CLIP_SELECTED_BORDER
                        } else {
                            AUDIO_CLIP_BORDER
                        },
                        width: if self.is_selected { 2.0 } else { 1.0 },
                        radius: self.radius.into(),
                    },
                    ..container::Style::default()
                });

                let clip_with_fades: Element<'static, Message> = if self.clip.fade_enabled {
                    let fade_in_width = visible_fade_overlay_width(
                        self.clip.fade_in_samples,
                        self.pixels_per_sample,
                    );
                    let fade_out_width = visible_fade_overlay_width(
                        self.clip.fade_out_samples,
                        self.pixels_per_sample,
                    );
                    let mut stack = Stack::new().push(clip_widget);
                    if should_draw_fade_overlay(self.clip.fade_in_samples, self.pixels_per_sample) {
                        let fade_in_handle = mouse_area(
                            container("")
                                .width(Length::Fixed(6.0))
                                .height(Length::Fixed(6.0))
                                .style(|_theme| container::Style {
                                    background: Some(Background::Color(Color::from_rgba(
                                        1.0, 1.0, 1.0, 0.9,
                                    ))),
                                    border: Border {
                                        color: Color::from_rgba(0.3, 0.3, 0.3, 1.0),
                                        width: 1.0,
                                        radius: 3.0.into(),
                                    },
                                    ..container::Style::default()
                                }),
                        )
                        .on_press(Message::FadeResizeStart {
                            kind: Kind::Audio,
                            track_idx: track_name.clone(),
                            clip_idx: self.clip_index,
                            is_fade_out: false,
                        });
                        stack = stack.push(
                            pin(fade_in_handle).position(Point::new(fade_in_width - 3.0, -3.0)),
                        );
                        stack = stack.push(
                            pin(fade_bezier_overlay(
                                fade_in_width,
                                self.clip_height,
                                Color::from_rgba(0.0, 0.0, 0.0, 0.3),
                                false,
                            ))
                            .position(Point::new(0.0, 0.0)),
                        );
                    }
                    if should_draw_fade_overlay(self.clip.fade_out_samples, self.pixels_per_sample)
                    {
                        let fade_out_handle = mouse_area(
                            container("")
                                .width(Length::Fixed(6.0))
                                .height(Length::Fixed(6.0))
                                .style(|_theme| container::Style {
                                    background: Some(Background::Color(Color::from_rgba(
                                        1.0, 1.0, 1.0, 0.9,
                                    ))),
                                    border: Border {
                                        color: Color::from_rgba(0.3, 0.3, 0.3, 1.0),
                                        width: 1.0,
                                        radius: 3.0.into(),
                                    },
                                    ..container::Style::default()
                                }),
                        )
                        .on_press(Message::FadeResizeStart {
                            kind: Kind::Audio,
                            track_idx: track_name.clone(),
                            clip_idx: self.clip_index,
                            is_fade_out: true,
                        });
                        stack = stack.push(
                            pin(fade_out_handle)
                                .position(Point::new(self.clip_width - fade_out_width - 3.0, -3.0)),
                        );
                        stack = stack.push(
                            pin(fade_bezier_overlay(
                                fade_out_width,
                                self.clip_height,
                                Color::from_rgba(0.0, 0.0, 0.0, 0.3),
                                true,
                            ))
                            .position(Point::new(self.clip_width - fade_out_width, 0.0)),
                        );
                    }
                    stack.into()
                } else {
                    clip_widget.into()
                };

                let base = mouse_area(clip_with_fades);
                let base = if self.left_handle_hovered || self.right_handle_hovered {
                    base.interaction(mouse::Interaction::ResizingColumn)
                } else {
                    base
                };
                let base = base.on_press(on_select).on_double_click(on_open);
                if self.drag_enabled {
                    base.on_move(move |point| {
                        let mut clip_data =
                            DraggedClip::new(Kind::Audio, self.clip_index, track_name.clone());
                        clip_data.start = point;
                        Message::ClipDrag(clip_data)
                    })
                    .into()
                } else {
                    base.into()
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
enum MIDIClipMode {
    Widget,
    Preview,
}

pub struct MIDIClip<'a> {
    clip: &'a MIDIClipState,
    clip_width: f32,
    clip_height: f32,
    label: String,
    is_selected: bool,
    left_handle_hovered: bool,
    right_handle_hovered: bool,
    midi_notes: Option<Arc<Vec<PianoNote>>>,
    track_name: Option<String>,
    clip_index: usize,
    on_select: Option<Message>,
    on_open: Option<Message>,
    drag_enabled: bool,
    background: Option<Background>,
    border_color: Option<Color>,
    radius: f32,
    mode: MIDIClipMode,
}

impl<'a> MIDIClip<'a> {
    pub fn new(clip: &'a MIDIClipState) -> Self {
        Self {
            clip,
            clip_width: 12.0,
            clip_height: 8.0,
            label: String::new(),
            is_selected: false,
            left_handle_hovered: false,
            right_handle_hovered: false,
            midi_notes: None,
            track_name: None,
            clip_index: 0,
            on_select: None,
            on_open: None,
            drag_enabled: false,
            background: None,
            border_color: None,
            radius: 8.0,
            mode: MIDIClipMode::Widget,
        }
    }

    pub fn with_size(mut self, clip_width: f32, clip_height: f32) -> Self {
        self.clip_width = clip_width;
        self.clip_height = clip_height;
        self
    }

    pub fn with_label(mut self, label: String) -> Self {
        self.label = label;
        self
    }

    pub fn selected(mut self, is_selected: bool) -> Self {
        self.is_selected = is_selected;
        self
    }

    pub fn hovered_handles(
        mut self,
        left_handle_hovered: bool,
        right_handle_hovered: bool,
    ) -> Self {
        self.left_handle_hovered = left_handle_hovered;
        self.right_handle_hovered = right_handle_hovered;
        self
    }

    pub fn with_notes(mut self, midi_notes: Option<Arc<Vec<PianoNote>>>) -> Self {
        self.midi_notes = midi_notes;
        self
    }

    pub fn interactive(
        mut self,
        track_name: String,
        clip_index: usize,
        on_select: Message,
        on_open: Message,
        drag_enabled: bool,
    ) -> Self {
        self.track_name = Some(track_name);
        self.clip_index = clip_index;
        self.on_select = Some(on_select);
        self.on_open = Some(on_open);
        self.drag_enabled = drag_enabled;
        self.mode = MIDIClipMode::Widget;
        self
    }

    pub fn preview(mut self, background: Background, border_color: Color, radius: f32) -> Self {
        self.background = Some(background);
        self.border_color = Some(border_color);
        self.radius = radius;
        self.mode = MIDIClipMode::Preview;
        self
    }

    pub fn clean_name(name: &str) -> String {
        clean_clip_name(name)
    }

    pub fn label_for_width(label: &str, width_px: f32) -> String {
        trim_label_to_width(label, width_px)
    }

    pub fn two_edge_gradient(
        base: Color,
        muted_alpha: f32,
        normal_alpha: f32,
        reverse: bool,
    ) -> Background {
        clip_two_edge_gradient(base, muted_alpha, normal_alpha, reverse)
    }

    pub fn into_element(self) -> Element<'static, Message> {
        match self.mode {
            MIDIClipMode::Preview => {
                let mut preview_layers = Vec::with_capacity(2);
                if let Some(notes) = self.midi_notes {
                    preview_layers.push(midi_clip_notes_overlay(
                        notes,
                        self.clip.offset,
                        self.clip.length.max(1),
                    ));
                }
                preview_layers.push(clip_label_overlay(self.label));
                let preview_content = container(Stack::with_children(preview_layers))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .padding(0)
                    .style(move |_theme| {
                        use container::Style;
                        Style {
                            background: self.background,
                            ..Style::default()
                        }
                    });
                container(preview_content)
                    .width(Length::Fixed(self.clip_width))
                    .height(Length::Fixed(self.clip_height))
                    .style(move |_theme| container::Style {
                        background: None,
                        border: Border {
                            color: self.border_color.unwrap_or(Color::TRANSPARENT),
                            width: 2.0,
                            radius: self.radius.into(),
                        },
                        ..container::Style::default()
                    })
                    .into()
            }
            MIDIClipMode::Widget => {
                let track_name = self.track_name.expect("midi clip widget track name");
                let on_select = self.on_select.expect("midi clip widget on_select");
                let on_open = self.on_open.expect("midi clip widget on_open");
                let left_edge_zone = mouse_area(
                    Space::new()
                        .width(Length::Fixed(CLIP_RESIZE_HANDLE_WIDTH))
                        .height(Length::Fill),
                )
                .interaction(mouse::Interaction::ResizingColumn)
                .on_enter(Message::ClipResizeHandleHover {
                    kind: Kind::MIDI,
                    track_idx: track_name.clone(),
                    clip_idx: self.clip_index,
                    is_right_side: false,
                    hovered: true,
                })
                .on_exit(Message::ClipResizeHandleHover {
                    kind: Kind::MIDI,
                    track_idx: track_name.clone(),
                    clip_idx: self.clip_index,
                    is_right_side: false,
                    hovered: false,
                })
                .on_press(Message::ClipResizeStart(
                    Kind::MIDI,
                    track_name.clone(),
                    self.clip_index,
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
                    track_idx: track_name.clone(),
                    clip_idx: self.clip_index,
                    is_right_side: true,
                    hovered: true,
                })
                .on_exit(Message::ClipResizeHandleHover {
                    kind: Kind::MIDI,
                    track_idx: track_name.clone(),
                    clip_idx: self.clip_index,
                    is_right_side: true,
                    hovered: false,
                })
                .on_press(Message::ClipResizeStart(
                    Kind::MIDI,
                    track_name.clone(),
                    self.clip_index,
                    true,
                ));

                let mut clip_layers = Vec::with_capacity(2);
                if let Some(notes) = self.midi_notes {
                    clip_layers.push(midi_clip_notes_overlay(
                        notes,
                        self.clip.offset,
                        self.clip.length.max(1),
                    ));
                }
                clip_layers.push(clip_label_overlay(self.label));

                let clip_muted = self.clip.muted;
                let clip_widget = container(Stack::with_children(vec![
                    container(Stack::with_children(clip_layers))
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .padding(0)
                        .style(move |_theme| {
                            use container::Style;
                            let base = if self.is_selected {
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
                        })
                        .into(),
                    pin(left_edge_zone).position(Point::new(0.0, 0.0)).into(),
                    pin(right_edge_zone)
                        .position(Point::new(self.clip_width - CLIP_RESIZE_HANDLE_WIDTH, 0.0))
                        .into(),
                ]))
                .width(Length::Fixed(self.clip_width))
                .height(Length::Fixed(self.clip_height))
                .style(move |_theme| container::Style {
                    background: None,
                    border: Border {
                        color: if self.is_selected {
                            MIDI_CLIP_SELECTED_BORDER
                        } else {
                            MIDI_CLIP_BORDER
                        },
                        width: if self.is_selected { 2.2 } else { 1.4 },
                        radius: self.radius.into(),
                    },
                    ..container::Style::default()
                });

                let base = mouse_area(clip_widget);
                let base = if self.left_handle_hovered || self.right_handle_hovered {
                    base.interaction(mouse::Interaction::ResizingColumn)
                } else {
                    base
                };
                let base = base.on_press(on_select).on_double_click(on_open);
                if self.drag_enabled {
                    base.on_move(move |point| {
                        let mut clip_data =
                            DraggedClip::new(Kind::MIDI, self.clip_index, track_name.clone());
                        clip_data.start = point;
                        Message::ClipDrag(clip_data)
                    })
                    .into()
                } else {
                    base.into()
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{should_draw_fade_overlay, visible_fade_overlay_width};

    #[test]
    fn visible_fade_overlay_width_grows_with_zoom_below_full_size() {
        let low_zoom = visible_fade_overlay_width(240, 0.01);
        let higher_zoom = visible_fade_overlay_width(240, 0.02);

        assert!(higher_zoom > low_zoom);
        assert!((low_zoom - 2.4).abs() < 1.0e-5);
    }

    #[test]
    fn visible_fade_overlay_width_matches_actual_size_once_large_enough() {
        let width = visible_fade_overlay_width(240, 0.1);
        assert_eq!(width, 24.0);
    }

    #[test]
    fn should_draw_fade_overlay_hides_tiny_fades() {
        assert!(!should_draw_fade_overlay(240, 0.0125));
        assert!(should_draw_fade_overlay(240, 0.0126));
    }
}
