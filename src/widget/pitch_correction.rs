use crate::{
    consts::widget_piano::{
        H_SCROLL_ID, H_ZOOM_MAX, H_ZOOM_MIN, KEYBOARD_WIDTH, KEYS_SCROLL_ID, MAIN_SPLIT_SPACING,
        NOTES_PER_OCTAVE, NOTES_SCROLL_ID, OCTAVES, PITCH_MAX, RIGHT_SCROLL_GUTTER_WIDTH,
        TOOLS_STRIP_WIDTH, V_SCROLL_ID, WHITE_KEY_HEIGHT, WHITE_KEYS_PER_OCTAVE,
    },
    message::Message,
    state::{PitchCorrectionData, PitchCorrectionPoint, State},
};
use iced::{
    Background, Color, Element, Event, Length, Point, Rectangle, Renderer, Size, Theme, mouse,
    widget::{
        Id, Stack, button, canvas,
        canvas::{Action as CanvasAction, Frame, Geometry, Path, Program},
        checkbox, column, container, pin, row, scrollable, slider, text, vertical_slider,
    },
};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct PitchCorrection {
    state: State,
}

impl PitchCorrection {
    const FRAME_LIKENESS_MIN: f32 = 0.05;
    const FRAME_LIKENESS_MAX: f32 = 2.0;

    pub fn new(state: State) -> Self {
        Self { state }
    }

    fn is_black_key(pitch: u8) -> bool {
        matches!(pitch % 12, 1 | 3 | 6 | 8 | 10)
    }

    fn zoom_x_to_slider(zoom_x: f32) -> f32 {
        (H_ZOOM_MIN + H_ZOOM_MAX - zoom_x).clamp(H_ZOOM_MIN, H_ZOOM_MAX)
    }

    fn slider_to_zoom_x(slider_value: f32) -> f32 {
        (H_ZOOM_MIN + H_ZOOM_MAX - slider_value).clamp(H_ZOOM_MIN, H_ZOOM_MAX)
    }

    fn pitch_color(point: &PitchCorrectionPoint) -> Color {
        let clarity = point.clarity.clamp(0.0, 1.0);
        Color {
            r: 0.35 + 0.35 * clarity,
            g: 0.55 + 0.25 * clarity,
            b: 0.82,
            a: 0.55 + 0.35 * clarity,
        }
    }

    pub fn view<'a>(
        &'a self,
        pixels_per_sample: f32,
        _samples_per_bar: f32,
        playhead_x: Option<f32>,
    ) -> Element<'a, Message> {
        let (
            zoom_x,
            zoom_y,
            roll,
            selected_points,
            dragging_points,
            selecting_rect,
            frame_likeness,
            inertia_ms,
            formant_compensation,
        ) = {
            let state = self.state.blocking_read();
            (
                state.piano_zoom_x.max(1.0),
                state.piano_zoom_y.max(1.0),
                state.pitch_correction.clone(),
                state.pitch_correction_selected_points.clone(),
                state.pitch_correction_dragging_points.clone(),
                state.pitch_correction_selecting_rect,
                state
                    .pitch_correction_frame_likeness
                    .clamp(Self::FRAME_LIKENESS_MIN, Self::FRAME_LIKENESS_MAX),
                state.pitch_correction_inertia_ms.min(1000),
                state.pitch_correction_formant_compensation,
            )
        };
        let Some(roll) = roll else {
            return container(text("No audio clip selected for pitch correction."))
                .width(Length::Fill)
                .height(Length::Fill)
                .into();
        };

        let pitch_count = OCTAVES * NOTES_PER_OCTAVE;
        let row_h = ((WHITE_KEY_HEIGHT * WHITE_KEYS_PER_OCTAVE as f32 / NOTES_PER_OCTAVE as f32)
            * zoom_y)
            .max(1.0);
        let notes_h = pitch_count as f32 * row_h;
        let pps_notes = (pixels_per_sample * zoom_x).max(0.0001);
        let notes_w = (roll.clip_length_samples as f32 * pps_notes).max(1.0);

        let mut note_layers: Vec<Element<'_, Message>> = vec![];
        for i in 0..pitch_count {
            let pitch = PITCH_MAX.saturating_sub(i as u8);
            let is_black = Self::is_black_key(pitch);
            note_layers.push(
                pin(container("")
                    .width(Length::Fixed(notes_w))
                    .height(Length::Fixed(row_h))
                    .style(move |_theme| container::Style {
                        background: Some(Background::Color(if is_black {
                            Color::from_rgba(0.08, 0.08, 0.10, 0.85)
                        } else {
                            Color::from_rgba(0.12, 0.12, 0.14, 0.85)
                        })),
                        ..container::Style::default()
                    }))
                .position(Point::new(0.0, i as f32 * row_h))
                .into(),
            );
        }
        note_layers.push(
            pin(canvas(PitchRollCanvas::new(
                roll.clone(),
                selected_points,
                dragging_points,
                selecting_rect,
                pps_notes,
                row_h,
                playhead_x,
            ))
            .width(Length::Fixed(notes_w))
            .height(Length::Fixed(notes_h)))
            .position(Point::new(0.0, 0.0))
            .into(),
        );

        let notes_content = Stack::from_vec(note_layers)
            .width(Length::Fixed(notes_w))
            .height(Length::Fixed(notes_h));
        let octave_h = (notes_h / OCTAVES as f32).max(1.0);
        let keyboard = (0..OCTAVES).fold(column![], |col, octave_idx| {
            let octave = (OCTAVES - 1 - octave_idx) as u8;
            col.push(
                canvas(OctaveKeyboardDisplay::new(octave))
                    .width(Length::Fixed(KEYBOARD_WIDTH))
                    .height(Length::Fixed(octave_h)),
            )
        });
        let keyboard_scroll = scrollable(
            container(keyboard)
                .width(Length::Fixed(KEYBOARD_WIDTH))
                .height(Length::Fixed(notes_h))
                .style(|_theme| container::Style {
                    background: Some(Background::Color(Color::from_rgba(0.12, 0.12, 0.12, 1.0))),
                    ..container::Style::default()
                }),
        )
        .id(Id::new(KEYS_SCROLL_ID))
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::hidden(),
        ))
        .on_scroll(|viewport| Message::PianoScrollYChanged(viewport.relative_offset().y))
        .width(Length::Fixed(KEYBOARD_WIDTH))
        .height(Length::Fill);

        let note_scroll = scrollable(
            container(notes_content)
                .width(Length::Shrink)
                .height(Length::Fixed(notes_h))
                .style(|_theme| container::Style {
                    background: Some(Background::Color(Color::from_rgba(0.07, 0.07, 0.09, 1.0))),
                    ..container::Style::default()
                }),
        )
        .id(Id::new(NOTES_SCROLL_ID))
        .direction(scrollable::Direction::Both {
            vertical: scrollable::Scrollbar::hidden(),
            horizontal: scrollable::Scrollbar::hidden(),
        })
        .on_scroll(|viewport| {
            let offset = viewport.relative_offset();
            Message::PianoScrollChanged {
                x: offset.x,
                y: offset.y,
            }
        })
        .width(Length::Fill)
        .height(Length::Fill);

        let h_scroll = scrollable(
            container("")
                .width(Length::Fixed(notes_w))
                .height(Length::Fixed(1.0)),
        )
        .id(Id::new(H_SCROLL_ID))
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new(),
        ))
        .on_scroll(|viewport| Message::PianoScrollXChanged(viewport.relative_offset().x))
        .width(Length::Fill)
        .height(Length::Fixed(16.0));

        let v_scroll = scrollable(
            container("")
                .width(Length::Fixed(1.0))
                .height(Length::Fixed(notes_h)),
        )
        .id(Id::new(V_SCROLL_ID))
        .direction(scrollable::Direction::Vertical(scrollable::Scrollbar::new()))
        .on_scroll(|viewport| Message::PianoScrollYChanged(viewport.relative_offset().y))
        .width(Length::Fixed(RIGHT_SCROLL_GUTTER_WIDTH))
        .height(Length::Fill);

        let info_strip = container(
            column![
                text(format!("Frame likeness: {:.2}", frame_likeness)).size(10),
                slider(
                    Self::FRAME_LIKENESS_MIN..=Self::FRAME_LIKENESS_MAX,
                    frame_likeness,
                    Message::PitchCorrectionFrameLikenessChanged,
                )
                .step(0.05),
                text(format!("Inertia: {} ms", inertia_ms)).size(10),
                slider(0..=1000, inertia_ms, Message::PitchCorrectionInertiaChanged).step(1_u16),
                checkbox(formant_compensation)
                    .label("Formant compensation")
                    .on_toggle(Message::PitchCorrectionFormantCompensationChanged),
                button(text("Apply").size(11)).on_press(Message::PitchCorrectionApply),
            ]
            .spacing(8)
            .width(Length::Fill),
        )
        .width(Length::Fixed(TOOLS_STRIP_WIDTH))
        .height(Length::Fill)
        .padding([8, 8])
        .style(|_theme| container::Style {
            background: Some(Background::Color(Color::from_rgba(0.10, 0.10, 0.12, 1.0))),
            ..container::Style::default()
        });

        let layout = row![
            row![
                info_strip,
                column![
                    row![keyboard_scroll, note_scroll]
                        .height(Length::Fill)
                        .width(Length::Fill),
                    row![
                        container("")
                            .width(Length::Fixed(KEYBOARD_WIDTH))
                            .height(Length::Fixed(16.0)),
                        row![
                            h_scroll,
                            slider(
                                H_ZOOM_MIN..=H_ZOOM_MAX,
                                Self::zoom_x_to_slider(zoom_x),
                                |value| Message::PianoZoomXChanged(Self::slider_to_zoom_x(value)),
                            )
                            .step(0.1)
                            .width(Length::Fixed(100.0)),
                        ]
                        .spacing(8)
                        .width(Length::Fill),
                    ]
                ]
                .spacing(3)
                .width(Length::Fill)
                .height(Length::Fill),
            ]
            .spacing(MAIN_SPLIT_SPACING)
            .width(Length::Fill)
            .height(Length::Fill),
            column![
                v_scroll,
                vertical_slider(1.0..=8.0, zoom_y, Message::PianoZoomYChanged)
                    .step(0.1)
                    .height(Length::Fixed(100.0)),
            ]
            .spacing(8)
            .height(Length::Fill),
        ];

        container(layout)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

#[derive(Debug, Clone)]
struct PitchRollCanvas {
    roll: PitchCorrectionData,
    selected_points: std::collections::HashSet<usize>,
    dragging_points: Option<crate::state::DraggingPitchCorrectionPoints>,
    selecting_rect: Option<(Point, Point)>,
    pixels_per_sample: f32,
    row_h: f32,
    playhead_x: Option<f32>,
}

impl PitchRollCanvas {
    fn new(
        roll: PitchCorrectionData,
        selected_points: std::collections::HashSet<usize>,
        dragging_points: Option<crate::state::DraggingPitchCorrectionPoints>,
        selecting_rect: Option<(Point, Point)>,
        pixels_per_sample: f32,
        row_h: f32,
        playhead_x: Option<f32>,
    ) -> Self {
        Self {
            roll,
            selected_points,
            dragging_points,
            selecting_rect,
            pixels_per_sample,
            row_h,
            playhead_x,
        }
    }

    fn point_bounds(&self, point: &PitchCorrectionPoint, bounds: Rectangle) -> Rectangle {
        let x = point.start_sample as f32 * self.pixels_per_sample;
        let width = (point.length_samples as f32 * self.pixels_per_sample).max(6.0);
        let y = (f32::from(PITCH_MAX) - point.target_midi_pitch.clamp(0.0, f32::from(PITCH_MAX))
            + 0.5)
            * self.row_h;
        let height = (self.row_h * (0.45 + 0.35 * point.clarity.clamp(0.0, 1.0))).max(6.0);
        Rectangle {
            x,
            y: (y - height * 0.5).clamp(0.0, bounds.height - height.min(bounds.height)),
            width,
            height: height.min(bounds.height.max(1.0)),
        }
    }

    fn hit_test(&self, position: Point, bounds: Rectangle) -> Option<usize> {
        self.roll
            .points
            .iter()
            .enumerate()
            .rev()
            .find_map(|(idx, point)| {
                let rect = self.point_bounds(point, bounds);
                (position.x >= rect.x
                    && position.x <= rect.x + rect.width
                    && position.y >= rect.y
                    && position.y <= rect.y + rect.height)
                    .then_some(idx)
            })
    }
}

#[derive(Default)]
struct PitchRollCanvasState {
    drag_start: Option<Point>,
    dragging_mode: PitchDraggingMode,
    last_click: Option<(usize, Instant)>,
    selection_dragged: bool,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum PitchDraggingMode {
    #[default]
    None,
    DraggingPoints,
    SelectingRect,
}

impl Program<Message> for PitchRollCanvas {
    type State = PitchRollCanvasState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<CanvasAction<Message>> {
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(position) = cursor.position_in(bounds) {
                    state.drag_start = Some(position);
                    state.selection_dragged = false;
                    if let Some(point_index) = self.hit_test(position, bounds) {
                        let now = Instant::now();
                        let double_click = state
                            .last_click
                            .map(|(last_idx, last_time)| {
                                last_idx == point_index
                                    && now.duration_since(last_time) <= Duration::from_millis(350)
                            })
                            .unwrap_or(false);
                        state.last_click = Some((point_index, now));
                        if double_click {
                            state.drag_start = None;
                            state.dragging_mode = PitchDraggingMode::None;
                            return Some(
                                CanvasAction::publish(Message::PitchCorrectionSnapToNearest {
                                    point_index,
                                })
                                .and_capture(),
                            );
                        }
                        state.dragging_mode = PitchDraggingMode::DraggingPoints;
                        return Some(
                            CanvasAction::publish(Message::PitchCorrectionPointClick {
                                point_index,
                                position,
                            })
                            .and_capture(),
                        );
                    }
                    state.last_click = None;
                    state.dragging_mode = PitchDraggingMode::SelectingRect;
                    return Some(
                        CanvasAction::publish(Message::PitchCorrectionSelectRectStart { position })
                            .and_capture(),
                    );
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if state.drag_start.is_some()
                    && let Some(position) = cursor.position_in(bounds)
                {
                    match state.dragging_mode {
                        PitchDraggingMode::DraggingPoints => {
                            return Some(
                                CanvasAction::publish(Message::PitchCorrectionPointsDrag {
                                    position,
                                })
                                .and_capture(),
                            );
                        }
                        PitchDraggingMode::SelectingRect => {
                            if let Some(start) = state.drag_start
                                && ((position.x - start.x).abs() > 2.0
                                    || (position.y - start.y).abs() > 2.0)
                            {
                                state.selection_dragged = true;
                            }
                            return Some(
                                CanvasAction::publish(Message::PitchCorrectionSelectRectDrag {
                                    position,
                                })
                                .and_capture(),
                            );
                        }
                        PitchDraggingMode::None => {}
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.drag_start.is_some() {
                    let mode = state.dragging_mode;
                    state.drag_start = None;
                    state.dragging_mode = PitchDraggingMode::None;
                    return match mode {
                        PitchDraggingMode::DraggingPoints => Some(
                            CanvasAction::publish(Message::PitchCorrectionPointsEndDrag)
                                .and_capture(),
                        ),
                        PitchDraggingMode::SelectingRect => {
                            let message = if state.selection_dragged {
                                Message::PitchCorrectionSelectRectEnd
                            } else {
                                Message::PitchCorrectionClearSelection
                            };
                            state.selection_dragged = false;
                            Some(CanvasAction::publish(message).and_capture())
                        }
                        PitchDraggingMode::None => None,
                    };
                }
            }
            _ => {}
        }
        None
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let dragging_map = self.dragging_points.as_ref().map(|dragging| {
            let delta_y = dragging.current_point.y - dragging.start_point.y;
            let delta_pitch = -(delta_y / self.row_h.max(1.0));
            dragging
                .point_indices
                .iter()
                .copied()
                .zip(dragging.original_points.iter().cloned())
                .map(|(idx, mut point)| {
                    point.target_midi_pitch =
                        (point.target_midi_pitch + delta_pitch).clamp(0.0, 119.999);
                    (idx, point)
                })
                .collect::<std::collections::HashMap<usize, PitchCorrectionPoint>>()
        });
        for (idx, point) in self.roll.points.iter().enumerate() {
            let display_point = dragging_map
                .as_ref()
                .and_then(|map| map.get(&idx))
                .unwrap_or(point);
            let rect_bounds = self.point_bounds(display_point, bounds);
            let rect = Path::rounded_rectangle(
                Point::new(rect_bounds.x, rect_bounds.y),
                Size::new(rect_bounds.width, rect_bounds.height),
                iced::border::Radius::from(2.0),
            );
            frame.fill(&rect, PitchCorrection::pitch_color(display_point));
            if self.selected_points.contains(&idx) {
                frame.stroke(
                    &rect,
                    canvas::Stroke::default()
                        .with_width(2.0)
                        .with_color(Color::from_rgba(1.0, 0.95, 0.72, 0.95)),
                );
            }
        }
        if let Some(playhead_x) = self.playhead_x {
            let line = Path::rectangle(
                Point::new(playhead_x.clamp(0.0, bounds.width), 0.0),
                Size::new(1.0, bounds.height),
            );
            frame.fill(&line, Color::from_rgba(0.95, 0.18, 0.14, 0.95));
        }
        if let Some((start, end)) = self.selecting_rect {
            let left = start.x.min(end.x);
            let top = start.y.min(end.y);
            let width = (start.x - end.x).abs().max(1.0);
            let height = (start.y - end.y).abs().max(1.0);
            let rect = Path::rectangle(Point::new(left, top), Size::new(width, height));
            frame.fill(&rect, Color::from_rgba(0.72, 0.82, 1.0, 0.12));
            frame.stroke(
                &rect,
                canvas::Stroke::default()
                    .with_width(1.0)
                    .with_color(Color::from_rgba(0.82, 0.90, 1.0, 0.9)),
            );
        }
        vec![frame.into_geometry()]
    }
}

#[derive(Debug, Clone, Copy)]
struct OctaveKeyboardDisplay {
    octave: u8,
}

impl OctaveKeyboardDisplay {
    fn new(octave: u8) -> Self {
        Self { octave }
    }
}

impl Program<Message> for OctaveKeyboardDisplay {
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
        let note_height = bounds.height / 12.0;
        let labels = [
            "B", "A#", "A", "G#", "G", "F#", "F", "E", "D#", "D", "C#", "C",
        ];
        for (i, label) in labels.iter().enumerate() {
            let note_in_octave = 11 - i as u8;
            let midi_note = self.octave * 12 + note_in_octave;
            let y = i as f32 * note_height;
            let rect = Path::rectangle(
                Point::new(0.0, y),
                Size::new(bounds.width, (note_height - 1.0).max(1.0)),
            );
            let is_black = PitchCorrection::is_black_key(midi_note);
            frame.fill(
                &rect,
                if is_black {
                    Color::from_rgb(0.18, 0.18, 0.20)
                } else {
                    Color::from_rgb(0.92, 0.92, 0.94)
                },
            );
            frame.stroke(
                &rect,
                canvas::Stroke::default()
                    .with_width(1.0)
                    .with_color(Color::from_rgb(0.25, 0.25, 0.28)),
            );
            frame.fill_text(canvas::Text {
                content: if *label == "C" {
                    format!("{label}{}", self.octave)
                } else {
                    (*label).to_string()
                },
                position: Point::new(4.0, y + note_height * 0.5 - 6.0),
                color: if is_black {
                    Color::from_rgb(0.88, 0.88, 0.92)
                } else {
                    Color::from_rgb(0.10, 0.10, 0.12)
                },
                size: 11.0.into(),
                ..canvas::Text::default()
            });
        }
        vec![frame.into_geometry()]
    }
}
