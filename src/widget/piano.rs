use crate::{
    consts::widget_piano::{NOTES_PER_OCTAVE, PITCH_MAX, WHITE_KEY_HEIGHT, WHITE_KEYS_PER_OCTAVE},
    message::Message,
    state::State,
};
use iced::{
    Background, Color, Event, Point, Rectangle, Renderer, Size, Theme, gradient, mouse,
    widget::canvas::{self, Action as CanvasAction, Frame, Geometry, Path, Program},
};
use std::collections::{HashMap, HashSet};

pub fn is_black_key(pitch: u8) -> bool {
    matches!(pitch % 12, 1 | 3 | 6 | 8 | 10)
}

pub fn note_color(velocity: u8, channel: u8) -> Color {
    let t = (velocity as f32 / 127.0).clamp(0.0, 1.0);
    let c = (channel as f32 / 15.0).clamp(0.0, 1.0);
    Color {
        r: 0.25 + 0.45 * t,
        g: 0.35 + 0.4 * (1.0 - c),
        b: 0.65 + 0.3 * c,
        a: 0.9,
    }
}

pub fn brighten(color: Color, amount: f32) -> Color {
    Color {
        r: (color.r + amount).min(1.0),
        g: (color.g + amount).min(1.0),
        b: (color.b + amount).min(1.0),
        a: color.a,
    }
}

pub fn darken(color: Color, amount: f32) -> Color {
    Color {
        r: (color.r - amount).max(0.0),
        g: (color.g - amount).max(0.0),
        b: (color.b - amount).max(0.0),
        a: color.a,
    }
}

pub fn note_two_edge_gradient(base: Color) -> Background {
    let edge = brighten(base, 0.08);
    let middle = darken(base, 0.08);
    Background::Gradient(
        gradient::Linear::new(0.0)
            .add_stop(0.0, edge)
            .add_stop(0.5, middle)
            .add_stop(1.0, edge)
            .into(),
    )
}

pub fn draw_octave_with_midnam(
    renderer: &Renderer,
    bounds: Rectangle,
    pressed_notes: &HashSet<u8>,
    octave: u8,
    midnam_note_names: &HashMap<u8, String>,
) -> Vec<canvas::Geometry> {
    let mut frame = Frame::new(renderer, bounds.size());
    let note_height = bounds.height / 12.0;

    for i in 0..12 {
        let note_in_octave = 11 - i;
        let midi_note = octave * 12 + note_in_octave;
        let is_pressed = pressed_notes.contains(&note_in_octave);
        let y_pos = i as f32 * note_height;

        let rect = Path::rectangle(
            Point::new(0.0, y_pos),
            Size::new(bounds.width, note_height - 1.0),
        );

        frame.fill(
            &rect,
            if is_pressed {
                Color::from_rgb(0.2, 0.6, 0.9)
            } else {
                Color::from_rgb(0.18, 0.18, 0.2)
            },
        );
        frame.stroke(
            &rect,
            canvas::Stroke::default()
                .with_width(1.0)
                .with_color(Color::from_rgb(0.25, 0.25, 0.28)),
        );

        if let Some(note_name) = midnam_note_names.get(&midi_note) {
            use iced::widget::canvas::Text;
            let text_pos = Point::new(4.0, y_pos + note_height * 0.5 - 6.0);
            frame.fill_text(Text {
                content: note_name.clone(),
                position: text_pos,
                color: Color::from_rgb(0.85, 0.85, 0.9),
                size: 11.0.into(),
                ..Text::default()
            });
        }
    }

    vec![frame.into_geometry()]
}

pub fn draw_octave(
    renderer: &Renderer,
    bounds: Rectangle,
    pressed_notes: &HashSet<u8>,
    octave: u8,
    midnam_note_names: &HashMap<u8, String>,
) -> Vec<canvas::Geometry> {
    let mut frame = Frame::new(renderer, bounds.size());
    let white_key_height = bounds.height / 7.0;

    // Draw White Keys
    for i in 0..7 {
        let note_id = match i {
            0 => 0,
            1 => 2,
            2 => 4,
            3 => 5,
            4 => 7,
            5 => 9,
            6 => 11,
            _ => 0,
        };
        let midi_note = octave * 12 + note_id;
        let is_pressed = pressed_notes.contains(&note_id);
        let y_pos = bounds.height - ((i + 1) as f32 * white_key_height);
        let rect = Path::rectangle(
            Point::new(0.0, y_pos),
            Size::new(bounds.width, white_key_height - 1.0),
        );

        frame.fill(
            &rect,
            if is_pressed {
                Color::from_rgb(0.0, 0.5, 1.0)
            } else {
                Color::WHITE
            },
        );
        frame.stroke(&rect, canvas::Stroke::default().with_width(1.0));

        if let Some(note_name) = midnam_note_names.get(&midi_note) {
            use iced::widget::canvas::Text;
            frame.fill_text(Text {
                content: note_name.clone(),
                position: Point::new(bounds.width - 25.0, y_pos + white_key_height * 0.5 - 6.0),
                color: Color::BLACK,
                size: 10.0.into(),
                ..Text::default()
            });
        }
    }

    // Draw Black Keys
    let black_key_offsets = [1, 2, 4, 5, 6];
    let black_note_ids = [1, 3, 6, 8, 10];
    let black_key_width = bounds.width * 0.6;
    let black_key_height = white_key_height * 0.6;

    for (idx, offset) in black_key_offsets.iter().enumerate() {
        let note_id = black_note_ids[idx];
        let is_pressed = pressed_notes.contains(&note_id);
        let y_pos_black =
            bounds.height - (*offset as f32 * white_key_height) - (black_key_height * 0.5);
        let rect = Path::rectangle(
            Point::new(0.0, y_pos_black),
            Size::new(black_key_width, black_key_height),
        );

        frame.fill(
            &rect,
            if is_pressed {
                Color::from_rgb(0.0, 0.4, 0.8)
            } else {
                Color::BLACK
            },
        );
    }

    vec![frame.into_geometry()]
}

#[derive(Debug, Clone)]
pub struct OctaveKeyboard {
    pub octave: u8,
    pub midnam_note_names: HashMap<u8, String>,
}

impl OctaveKeyboard {
    pub fn new(octave: u8, midnam_note_names: HashMap<u8, String>) -> Self {
        Self {
            octave,
            midnam_note_names,
        }
    }

    fn note_class_at(&self, cursor: Point, bounds: Rectangle) -> Option<u8> {
        let white_key_height = bounds.height / 7.0;
        let black_key_offsets = [1, 2, 4, 5, 6];
        let black_note_ids = [1, 3, 6, 8, 10];
        let black_key_width = bounds.width * 0.6;
        let black_key_height = white_key_height * 0.6;

        if cursor.x <= black_key_width {
            for (idx, offset) in black_key_offsets.iter().enumerate() {
                let y_pos_black =
                    bounds.height - (*offset as f32 * white_key_height) - (black_key_height * 0.5);
                if cursor.y >= y_pos_black && cursor.y <= y_pos_black + black_key_height {
                    return Some(black_note_ids[idx]);
                }
            }
        }

        for i in 0..7 {
            let note_id = match i {
                0 => 0,
                1 => 2,
                2 => 4,
                3 => 5,
                4 => 7,
                5 => 9,
                6 => 11,
                _ => 0,
            };
            let y_pos = bounds.height - ((i + 1) as f32 * white_key_height);
            if cursor.y >= y_pos && cursor.y <= y_pos + white_key_height {
                return Some(note_id);
            }
        }
        None
    }

    fn midi_note(&self, note_class: u8) -> u8 {
        (usize::from(self.octave) * 12 + usize::from(note_class)) as u8
    }
}

#[derive(Default, Debug)]
pub struct OctaveKeyboardState {
    pub pressed_notes: HashSet<u8>,
    pub active_note_class: Option<u8>,
}

impl Program<Message> for OctaveKeyboard {
    type State = OctaveKeyboardState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<CanvasAction<Message>> {
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(position) = cursor.position_in(bounds)
                    && let Some(note_class) = self.note_class_at(position, bounds)
                {
                    state.active_note_class = Some(note_class);
                    state.pressed_notes.clear();
                    state.pressed_notes.insert(note_class);
                    return Some(
                        CanvasAction::publish(Message::PianoKeyPressed(self.midi_note(note_class)))
                            .and_capture(),
                    );
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if let Some(note_class) = state.active_note_class.take() {
                    state.pressed_notes.clear();
                    return Some(CanvasAction::publish(Message::PianoKeyReleased(
                        self.midi_note(note_class),
                    )));
                }
            }
            _ => {}
        }
        None
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let is_piano = self.midnam_note_names.is_empty()
            || self.midnam_note_names.values().all(|name| {
                matches!(
                    name.as_str(),
                    "C" | "C#" | "D" | "D#" | "E" | "F" | "F#" | "G" | "G#" | "A" | "A#" | "B"
                ) || name.starts_with('C')
                    || name.starts_with('D')
                    || name.starts_with('E')
                    || name.starts_with('F')
                    || name.starts_with('G')
                    || name.starts_with('A')
                    || name.starts_with('B')
            });

        if is_piano {
            draw_octave(
                renderer,
                bounds,
                &state.pressed_notes,
                self.octave,
                &self.midnam_note_names,
            )
        } else {
            draw_octave_with_midnam(
                renderer,
                bounds,
                &state.pressed_notes,
                self.octave,
                &self.midnam_note_names,
            )
        }
    }
}

#[derive(Debug)]
pub struct PianoRollInteraction {
    pub state: State,
    pub pixels_per_sample: f32,
}

impl PianoRollInteraction {
    pub fn new(state: State, pixels_per_sample: f32) -> Self {
        Self {
            state,
            pixels_per_sample,
        }
    }

    fn note_at_position(
        &self,
        position: Point,
        row_h: f32,
        pps: f32,
        notes: &[crate::state::PianoNote],
    ) -> Option<usize> {
        for (idx, note) in notes.iter().enumerate() {
            if note.pitch > PITCH_MAX {
                continue;
            }
            let y_idx = usize::from(PITCH_MAX.saturating_sub(note.pitch));
            let y = y_idx as f32 * row_h + 1.0;
            let x = note.start_sample as f32 * pps;
            let w = (note.length_samples as f32 * pps).max(2.0);
            let h = (row_h - 2.0).max(2.0);

            if position.x >= x && position.x <= x + w && position.y >= y && position.y <= y + h {
                return Some(idx);
            }
        }
        None
    }

    pub fn velocity_delta_from_scroll(delta: &mouse::ScrollDelta) -> i8 {
        let raw = match delta {
            mouse::ScrollDelta::Lines { y, .. } => *y,
            mouse::ScrollDelta::Pixels { y, .. } => *y / 16.0,
        };
        let mut steps = raw.round() as i32;
        if steps == 0 && raw.abs() > f32::EPSILON {
            steps = raw.signum() as i32;
        }
        steps.clamp(-24, 24) as i8
    }
}

#[derive(Default, Debug)]
pub struct PianoRollInteractionState {
    pub dragging_mode: DraggingMode,
    pub drag_start: Option<Point>,
}

#[derive(Default, Debug, Clone, Copy, PartialEq)]
pub enum DraggingMode {
    #[default]
    None,
    SelectingRect,
    DraggingNotes,
    ResizingNote,
    CreatingNote,
}

impl Program<Message> for PianoRollInteraction {
    type State = PianoRollInteractionState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<CanvasAction<Message>> {
        let app_state = self.state.blocking_read();
        let roll = app_state.piano.as_ref()?;

        let zoom_x = app_state.piano_zoom_x;
        let zoom_y = app_state.piano_zoom_y;
        let row_h = ((WHITE_KEY_HEIGHT * WHITE_KEYS_PER_OCTAVE as f32 / NOTES_PER_OCTAVE as f32)
            * zoom_y)
            .max(1.0);
        let pps = (self.pixels_per_sample * zoom_x).max(0.0001);
        let notes = &roll.notes;

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(position) = cursor.position_in(bounds) {
                    if let Some(note_idx) = self.note_at_position(position, row_h, pps, notes) {
                        let note = notes.get(note_idx)?;
                        let note_x = note.start_sample as f32 * pps;
                        let note_w = (note.length_samples as f32 * pps).max(2.0);
                        let resize_handle_w = 6.0;
                        if position.x <= note_x + resize_handle_w {
                            state.drag_start = Some(position);
                            state.dragging_mode = DraggingMode::ResizingNote;
                            return Some(
                                CanvasAction::publish(Message::PianoNoteResizeStart {
                                    note_index: note_idx,
                                    position,
                                    resize_start: true,
                                })
                                .and_capture(),
                            );
                        }
                        if position.x >= note_x + note_w - resize_handle_w {
                            state.drag_start = Some(position);
                            state.dragging_mode = DraggingMode::ResizingNote;
                            return Some(
                                CanvasAction::publish(Message::PianoNoteResizeStart {
                                    note_index: note_idx,
                                    position,
                                    resize_start: false,
                                })
                                .and_capture(),
                            );
                        }
                        state.drag_start = Some(position);
                        state.dragging_mode = DraggingMode::DraggingNotes;
                        return Some(
                            CanvasAction::publish(Message::PianoNoteClick {
                                note_index: note_idx,
                                position,
                            })
                            .and_capture(),
                        );
                    } else {
                        state.drag_start = Some(position);
                        state.dragging_mode = DraggingMode::SelectingRect;
                        return Some(
                            CanvasAction::publish(Message::PianoSelectRectStart { position })
                                .and_capture(),
                        );
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                if let Some(position) = cursor.position_in(bounds) {
                    state.drag_start = Some(position);
                    state.dragging_mode = DraggingMode::CreatingNote;
                    return Some(
                        CanvasAction::publish(Message::PianoCreateNoteStart { position })
                            .and_capture(),
                    );
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle)) => {
                if let Some(position) = cursor.position_in(bounds)
                    && let Some(note_idx) = self.note_at_position(position, row_h, pps, notes)
                {
                    return Some(
                        CanvasAction::publish(Message::PianoDeleteNotes {
                            note_indices: vec![note_idx],
                        })
                        .and_capture(),
                    );
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some(position) = cursor.position_in(bounds)
                    && state.drag_start.is_some()
                {
                    match state.dragging_mode {
                        DraggingMode::SelectingRect => {
                            return Some(CanvasAction::publish(Message::PianoSelectRectDrag {
                                position,
                            }));
                        }
                        DraggingMode::DraggingNotes => {
                            return Some(CanvasAction::publish(Message::PianoNotesDrag {
                                position,
                            }));
                        }
                        DraggingMode::ResizingNote => {
                            return Some(CanvasAction::publish(Message::PianoNoteResizeDrag {
                                position,
                            }));
                        }
                        DraggingMode::CreatingNote => {
                            return Some(CanvasAction::publish(Message::PianoCreateNoteDrag {
                                position,
                            }));
                        }
                        DraggingMode::None => {}
                    }
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if let Some(position) = cursor.position_in(bounds)
                    && let Some(note_idx) = self.note_at_position(position, row_h, pps, notes)
                {
                    let velocity_delta = Self::velocity_delta_from_scroll(delta);
                    if velocity_delta != 0 {
                        return Some(
                            CanvasAction::publish(Message::PianoAdjustVelocity {
                                note_index: note_idx,
                                delta: velocity_delta,
                            })
                            .and_capture(),
                        );
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.drag_start.is_some() {
                    let mode = state.dragging_mode;
                    state.drag_start = None;
                    state.dragging_mode = DraggingMode::None;

                    match mode {
                        DraggingMode::SelectingRect => {
                            return Some(CanvasAction::publish(Message::PianoSelectRectEnd));
                        }
                        DraggingMode::DraggingNotes => {
                            return Some(CanvasAction::publish(Message::PianoNotesEndDrag));
                        }
                        DraggingMode::ResizingNote => {
                            return Some(CanvasAction::publish(Message::PianoNoteResizeEnd));
                        }
                        DraggingMode::CreatingNote => {
                            return Some(CanvasAction::publish(Message::PianoCreateNoteEnd));
                        }
                        DraggingMode::None => {}
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)) => {
                if state.drag_start.is_some() {
                    let mode = state.dragging_mode;
                    state.drag_start = None;
                    state.dragging_mode = DraggingMode::None;

                    match mode {
                        DraggingMode::CreatingNote => {
                            return Some(CanvasAction::publish(Message::PianoCreateNoteEnd));
                        }
                        DraggingMode::None => {}
                        DraggingMode::SelectingRect
                        | DraggingMode::DraggingNotes
                        | DraggingMode::ResizingNote => {}
                    }
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
        let app_state = self.state.blocking_read();
        let Some(roll) = app_state.piano.as_ref() else {
            return vec![];
        };

        let zoom_x = app_state.piano_zoom_x;
        let zoom_y = app_state.piano_zoom_y;
        let selected_notes = &app_state.piano_selected_notes;
        let selecting_rect = app_state.piano_selecting_rect;
        let dragging_notes = app_state.piano_dragging_notes.as_ref();
        let resizing_note = app_state.piano_resizing_note.as_ref();
        let creating_note = app_state.piano_creating_note;

        let row_h = ((WHITE_KEY_HEIGHT * WHITE_KEYS_PER_OCTAVE as f32 / NOTES_PER_OCTAVE as f32)
            * zoom_y)
            .max(1.0);
        let pps = (self.pixels_per_sample * zoom_x).max(0.0001);

        let mut frame = Frame::new(renderer, bounds.size());

        for &note_idx in selected_notes {
            if let Some(note) = roll.notes.get(note_idx) {
                if note.pitch > PITCH_MAX {
                    continue;
                }
                let y_idx = usize::from(PITCH_MAX.saturating_sub(note.pitch));
                let y = y_idx as f32 * row_h + 1.0;
                let x = note.start_sample as f32 * pps;
                let w = (note.length_samples as f32 * pps).max(2.0);
                let h = (row_h - 2.0).max(2.0);

                let selection_rect = Rectangle {
                    x: x - 1.0,
                    y: y - 1.0,
                    width: w + 2.0,
                    height: h + 2.0,
                };

                let path = Path::rectangle(
                    Point::new(selection_rect.x, selection_rect.y),
                    Size::new(selection_rect.width, selection_rect.height),
                );
                frame.stroke(
                    &path,
                    canvas::Stroke::default()
                        .with_color(Color::from_rgb(0.9, 0.7, 0.3))
                        .with_width(2.0),
                );
            }
        }

        if let Some(dragging) = dragging_notes {
            let delta_x = dragging.current_point.x - dragging.start_point.x;
            let delta_y = dragging.current_point.y - dragging.start_point.y;

            for note in &dragging.original_notes {
                if note.pitch > PITCH_MAX {
                    continue;
                }
                let y_idx = usize::from(PITCH_MAX.saturating_sub(note.pitch));
                let y = y_idx as f32 * row_h + 1.0 + delta_y;
                let x = note.start_sample as f32 * pps + delta_x;
                let w = (note.length_samples as f32 * pps).max(2.0);
                let h = (row_h - 2.0).max(2.0);

                let note_rect = Rectangle {
                    x,
                    y,
                    width: w,
                    height: h,
                };

                let path = Path::rectangle(
                    Point::new(note_rect.x, note_rect.y),
                    Size::new(note_rect.width, note_rect.height),
                );
                frame.fill(
                    &path,
                    Color {
                        r: 0.5,
                        g: 0.5,
                        b: 0.7,
                        a: 0.5,
                    },
                );
            }
        }

        if let Some((start, end)) = selecting_rect {
            let min_x = start.x.min(end.x);
            let min_y = start.y.min(end.y);
            let max_x = start.x.max(end.x);
            let max_y = start.y.max(end.y);

            let selection_rect = Rectangle {
                x: min_x,
                y: min_y,
                width: max_x - min_x,
                height: max_y - min_y,
            };

            let path = Path::rectangle(
                Point::new(selection_rect.x, selection_rect.y),
                Size::new(selection_rect.width, selection_rect.height),
            );
            frame.fill(
                &path,
                Color {
                    r: 0.3,
                    g: 0.5,
                    b: 0.8,
                    a: 0.2,
                },
            );
            frame.stroke(
                &path,
                canvas::Stroke::default()
                    .with_color(Color::from_rgb(0.4, 0.6, 0.9))
                    .with_width(1.5),
            );
        }

        if let Some(resizing) = resizing_note {
            let delta_x = resizing.current_point.x - resizing.start_point.x;
            let delta_samples = (delta_x / pps) as i64;
            let original = &resizing.original_note;
            let original_end = original
                .start_sample
                .saturating_add(original.length_samples)
                .max(1);
            let (preview_start, preview_len) = if resizing.resize_start {
                let max_start = original_end.saturating_sub(1) as i64;
                let new_start = (original.start_sample as i64 + delta_samples).clamp(0, max_start);
                let new_start = new_start as usize;
                (new_start, original_end.saturating_sub(new_start).max(1))
            } else {
                let min_end = original.start_sample.saturating_add(1) as i64;
                let new_end = (original_end as i64 + delta_samples).max(min_end) as usize;
                (
                    original.start_sample,
                    new_end.saturating_sub(original.start_sample).max(1),
                )
            };

            if original.pitch <= PITCH_MAX {
                let y_idx = usize::from(PITCH_MAX.saturating_sub(original.pitch));
                let y = y_idx as f32 * row_h + 1.0;
                let x = preview_start as f32 * pps;
                let w = (preview_len as f32 * pps).max(2.0);
                let h = (row_h - 2.0).max(2.0);
                let path = Path::rectangle(Point::new(x, y), Size::new(w, h));
                frame.fill(
                    &path,
                    Color {
                        r: 0.95,
                        g: 0.8,
                        b: 0.4,
                        a: 0.35,
                    },
                );
                frame.stroke(
                    &path,
                    canvas::Stroke::default()
                        .with_color(Color::from_rgb(0.95, 0.8, 0.4))
                        .with_width(1.5),
                );
            }
        }

        if let Some((start, end)) = creating_note {
            let start_x = start.x.min(end.x).max(0.0);
            let end_x = start.x.max(end.x).max(0.0);
            let y_row = (start.y / row_h).floor().max(0.0);
            let y = y_row * row_h + 1.0;
            let h = (row_h - 2.0).max(2.0);
            let w = (end_x - start_x).max(2.0);

            let path = Path::rectangle(Point::new(start_x, y), Size::new(w, h));
            frame.fill(
                &path,
                Color {
                    r: 0.6,
                    g: 0.75,
                    b: 0.95,
                    a: 0.35,
                },
            );
            frame.stroke(
                &path,
                canvas::Stroke::default()
                    .with_color(Color::from_rgb(0.7, 0.85, 1.0))
                    .with_width(1.5),
            );
        }

        drop(app_state);
        vec![frame.into_geometry()]
    }
}
