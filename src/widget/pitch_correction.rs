use crate::{
    consts::widget_piano::{
        H_ZOOM_MAX, H_ZOOM_MIN, KEYBOARD_WIDTH, MAIN_SPLIT_SPACING, MIDI_NOTE_COUNT,
        NOTES_PER_OCTAVE, OCTAVES, PITCH_MAX, TOOLS_STRIP_WIDTH, WHITE_KEY_HEIGHT,
        WHITE_KEYS_PER_OCTAVE,
    },
    message::Message,
    state::{
        MIN_PITCH_CORRECTION_FRAME_LIKENESS, PitchCorrectionData, PitchCorrectionPoint, State,
    },
    widget::piano,
};
use iced::{
    Background, Color, Element, Event, Length, Point, Rectangle, Renderer, Size, Theme, mouse,
    widget::{
        canvas,
        canvas::{Action as CanvasAction, Frame, Geometry, Path, Program},
        checkbox, column, container, pin, row, slider, text, vertical_slider,
    },
};
use maolan_widgets::{
    note_area::{NoteArea, PianoGridScrolls, piano_grid_scrollers},
    piano::OctaveKeyboard,
};
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct PitchCorrection {
    state: State,
}

impl PitchCorrection {
    const FRAME_LIKENESS_MIN: f32 = MIN_PITCH_CORRECTION_FRAME_LIKENESS;
    const FRAME_LIKENESS_MAX: f32 = 2.0;

    pub fn new(state: State) -> Self {
        Self { state }
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
        samples_per_bar: f32,
        playhead_x: Option<f32>,
    ) -> Element<'a, Message> {
        let (
            zoom_x,
            zoom_y,
            scroll_x,
            scroll_y,
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
                state.piano_scroll_x,
                state.piano_scroll_y,
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

        let row_h = ((WHITE_KEY_HEIGHT * WHITE_KEYS_PER_OCTAVE as f32 / NOTES_PER_OCTAVE as f32)
            * zoom_y)
            .max(1.0);
        let pps = (pixels_per_sample * zoom_x).max(0.0001);
        let notes_w = (roll.clip_length_samples as f32 * pps).max(1.0);
        let notes_h = MIDI_NOTE_COUNT as f32 * row_h;

        let content = vec![
            pin(canvas(PitchRollCanvas::new(
                roll.clone(),
                selected_points,
                dragging_points,
                selecting_rect,
                pps,
                row_h,
                None, // note_area handles playhead
            ))
            .width(Length::Fixed(notes_w))
            .height(Length::Fixed(notes_h)))
            .position(Point::new(0.0, 0.0))
            .into(),
        ];

        let notes_content = NoteArea {
            zoom_x,
            zoom_y,
            pixels_per_sample,
            samples_per_bar: Some(samples_per_bar),
            playhead_x,
            playhead_width: crate::consts::workspace::PLAYHEAD_WIDTH_PX,
            clip_length_samples: roll.clip_length_samples,
        }
        .view(content);

        let keyboard = (0..OCTAVES).fold(column![], |col, octave_idx| {
            let octave = (OCTAVES - 1 - octave_idx) as u8;
            let octave_h = piano::octave_note_count(octave) as f32 * row_h;
            col.push(
                canvas(OctaveKeyboard::new(
                    octave,
                    HashMap::new(),
                    |_| Message::None,
                    |_| Message::None,
                ))
                .width(Length::Fixed(KEYBOARD_WIDTH))
                .height(Length::Fixed(octave_h)),
            )
        });
        let keyboard = container(keyboard).style(|_theme| container::Style {
            background: Some(Background::Color(Color::from_rgba(0.12, 0.12, 0.12, 1.0))),
            ..container::Style::default()
        });
        let PianoGridScrolls {
            keyboard_scroll,
            note_scroll,
            h_scroll,
            v_scroll,
        } = piano_grid_scrollers(
            keyboard.into(),
            notes_content,
            notes_h,
            notes_w,
            scroll_x,
            scroll_y,
            Message::PianoScrollYChanged,
            |x, y| Message::PianoScrollChanged { x, y },
        );

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
            Event::Mouse(mouse::Event::WheelScrolled { .. }) => {
                if state.drag_start.is_some() {
                    state.drag_start = None;
                    state.dragging_mode = PitchDraggingMode::None;
                    state.selection_dragged = false;
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
                    point.target_midi_pitch = (point.target_midi_pitch + delta_pitch)
                        .clamp(0.0, f32::from(PITCH_MAX) + 0.999);
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

#[cfg(test)]
mod tests {
    use super::*;
    use iced::{Point, Rectangle, event, mouse};

    fn action_message(action: CanvasAction<Message>) -> (Option<Message>, event::Status) {
        let (message, _redraw, status) = action.into_inner();
        (message, status)
    }

    fn sample_roll() -> PitchCorrectionData {
        PitchCorrectionData {
            track_idx: "Track".to_string(),
            clip_index: 0,
            clip_name: "clip.wav".to_string(),
            clip_length_samples: 128,
            frame_likeness: 0.5,
            raw_points: Vec::new(),
            points: vec![PitchCorrectionPoint {
                start_sample: 10,
                length_samples: 8,
                detected_midi_pitch: 60.0,
                target_midi_pitch: 60.0,
                clarity: 1.0,
            }],
        }
    }

    #[test]
    fn update_double_clicking_point_snaps_to_nearest_pitch() {
        let canvas = PitchRollCanvas::new(
            sample_roll(),
            std::collections::HashSet::new(),
            None,
            None,
            2.0,
            10.0,
            None,
        );
        let point = canvas.point_bounds(
            &canvas.roll.points[0],
            Rectangle::new(Point::ORIGIN, Size::new(400.0, 400.0)),
        );
        let cursor = mouse::Cursor::Available(Point::new(point.x + 1.0, point.y + 1.0));
        let mut state = PitchRollCanvasState {
            last_click: Some((0, Instant::now())),
            ..PitchRollCanvasState::default()
        };

        let action = canvas
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                Rectangle::new(Point::ORIGIN, Size::new(400.0, 400.0)),
                cursor,
            )
            .expect("action");

        let (message, status) = action_message(action);
        match message {
            Some(Message::PitchCorrectionSnapToNearest { point_index }) => {
                assert_eq!(point_index, 0);
            }
            other => panic!("unexpected message: {other:?}"),
        }
        assert_eq!(status, event::Status::Captured);
        assert!(state.drag_start.is_none());
        assert!(matches!(state.dragging_mode, PitchDraggingMode::None));
    }

    #[test]
    fn update_clicking_empty_space_starts_selection() {
        let canvas = PitchRollCanvas::new(
            sample_roll(),
            std::collections::HashSet::new(),
            None,
            None,
            2.0,
            10.0,
            None,
        );
        let mut state = PitchRollCanvasState::default();
        let cursor = mouse::Cursor::Available(Point::new(150.0, 150.0));

        let action = canvas
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                Rectangle::new(Point::ORIGIN, Size::new(400.0, 400.0)),
                cursor,
            )
            .expect("action");

        let (message, status) = action_message(action);
        match message {
            Some(Message::PitchCorrectionSelectRectStart { position }) => {
                assert_eq!(position, Point::new(150.0, 150.0));
            }
            other => panic!("unexpected message: {other:?}"),
        }
        assert_eq!(status, event::Status::Captured);
        assert_eq!(state.drag_start, Some(Point::new(150.0, 150.0)));
        assert!(matches!(
            state.dragging_mode,
            PitchDraggingMode::SelectingRect
        ));
    }
}
