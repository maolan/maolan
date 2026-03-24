use crate::midi::{MIDI_NOTE_COUNT, NOTES_PER_OCTAVE, WHITE_KEY_HEIGHT, WHITE_KEYS_PER_OCTAVE};
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

pub fn octave_note_count(octave: u8) -> u8 {
    let start = usize::from(octave) * NOTES_PER_OCTAVE;
    if start >= MIDI_NOTE_COUNT {
        0
    } else {
        (MIDI_NOTE_COUNT - start).min(NOTES_PER_OCTAVE) as u8
    }
}

fn draw_chromatic_rows(
    renderer: &Renderer,
    bounds: Rectangle,
    pressed_notes: &HashSet<u8>,
    octave: u8,
    midnam_note_names: &HashMap<u8, String>,
    note_count: u8,
) -> Vec<canvas::Geometry> {
    let mut frame = Frame::new(renderer, bounds.size());
    let note_height = bounds.height / f32::from(note_count.max(1));

    for i in 0..note_count {
        let note_in_octave = note_count - 1 - i;
        let midi_note = octave * 12 + note_in_octave;
        let is_pressed = pressed_notes.contains(&note_in_octave);
        let y_pos = f32::from(i) * note_height;

        let rect = Path::rectangle(
            Point::new(0.0, y_pos),
            Size::new(bounds.width, note_height - 1.0),
        );
        let is_black = is_black_key(note_in_octave);

        frame.fill(
            &rect,
            if is_pressed {
                Color::from_rgb(0.2, 0.6, 0.9)
            } else if is_black {
                Color::from_rgb(0.18, 0.18, 0.2)
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

        if let Some(note_name) = midnam_note_names.get(&midi_note) {
            use iced::widget::canvas::Text;
            frame.fill_text(Text {
                content: note_name.clone(),
                position: Point::new(4.0, y_pos + note_height * 0.5 - 6.0),
                color: if is_black { Color::WHITE } else { Color::BLACK },
                size: 11.0.into(),
                ..Text::default()
            });
        }
    }

    vec![frame.into_geometry()]
}

fn draw_partial_octave(
    renderer: &Renderer,
    bounds: Rectangle,
    pressed_notes: &HashSet<u8>,
    octave: u8,
    midnam_note_names: &HashMap<u8, String>,
    note_count: u8,
) -> Vec<canvas::Geometry> {
    let mut frame = Frame::new(renderer, bounds.size());
    let white_note_ids = [0_u8, 2, 4, 5, 7];
    let black_key_offsets = [1_u8, 2, 4];
    let black_note_ids = [1_u8, 3, 6];
    let white_key_height = bounds.height / white_note_ids.len() as f32;
    let black_key_height = white_key_height * 0.6;
    let black_key_width = bounds.width * 0.6;

    for (i, note_id) in white_note_ids.iter().enumerate() {
        let midi_note = octave * 12 + *note_id;
        let is_pressed = pressed_notes.contains(note_id);
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

    for (idx, offset) in black_key_offsets.iter().enumerate() {
        let note_id = black_note_ids[idx];
        if note_id >= note_count {
            continue;
        }
        let is_pressed = pressed_notes.contains(&note_id);
        let y_pos_black =
            bounds.height - (f32::from(*offset) * white_key_height) - (black_key_height * 0.5);
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

pub fn draw_octave(
    renderer: &Renderer,
    bounds: Rectangle,
    pressed_notes: &HashSet<u8>,
    octave: u8,
    midnam_note_names: &HashMap<u8, String>,
) -> Vec<canvas::Geometry> {
    let mut frame = Frame::new(renderer, bounds.size());
    let white_key_height = bounds.height / 7.0;

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
pub struct OctaveKeyboard<Message, Press, Release>
where
    Press: Fn(u8) -> Message + Clone,
    Release: Fn(u8) -> Message + Clone,
{
    pub octave: u8,
    pub note_count: u8,
    pub midnam_note_names: HashMap<u8, String>,
    on_press: Press,
    on_release: Release,
}

impl<Message, Press, Release> OctaveKeyboard<Message, Press, Release>
where
    Press: Fn(u8) -> Message + Clone,
    Release: Fn(u8) -> Message + Clone,
{
    pub fn new(
        octave: u8,
        midnam_note_names: HashMap<u8, String>,
        on_press: Press,
        on_release: Release,
    ) -> Self {
        Self {
            octave,
            note_count: octave_note_count(octave),
            midnam_note_names,
            on_press,
            on_release,
        }
    }

    fn note_class_at(&self, cursor: Point, bounds: Rectangle) -> Option<u8> {
        if self.note_count == 0 {
            return None;
        }
        if self.note_count < NOTES_PER_OCTAVE as u8 {
            let white_note_ids = [0_u8, 2, 4, 5, 7];
            let black_key_offsets = [1_u8, 2, 4];
            let black_note_ids = [1_u8, 3, 6];
            let white_key_height = bounds.height / white_note_ids.len() as f32;
            let black_key_width = bounds.width * 0.6;
            let black_key_height = white_key_height * 0.6;

            if cursor.x <= black_key_width {
                for (idx, offset) in black_key_offsets.iter().enumerate() {
                    let note_id = black_note_ids[idx];
                    if note_id >= self.note_count {
                        continue;
                    }
                    let y_pos_black = bounds.height
                        - (f32::from(*offset) * white_key_height)
                        - (black_key_height * 0.5);
                    if cursor.y >= y_pos_black && cursor.y <= y_pos_black + black_key_height {
                        return Some(note_id);
                    }
                }
            }

            for (i, note_id) in white_note_ids.iter().enumerate() {
                let y_pos = bounds.height - ((i + 1) as f32 * white_key_height);
                if cursor.y >= y_pos && cursor.y <= y_pos + white_key_height {
                    return Some(*note_id);
                }
            }
            return None;
        }
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

impl<Message, Press, Release> Program<Message> for OctaveKeyboard<Message, Press, Release>
where
    Message: 'static,
    Press: Fn(u8) -> Message + Clone,
    Release: Fn(u8) -> Message + Clone,
{
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
                        CanvasAction::publish((self.on_press.clone())(self.midi_note(note_class)))
                            .and_capture(),
                    );
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if let Some(note_class) = state.active_note_class.take() {
                    state.pressed_notes.clear();
                    return Some(CanvasAction::publish((self.on_release.clone())(
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
            if self.note_count == NOTES_PER_OCTAVE as u8 {
                draw_octave(
                    renderer,
                    bounds,
                    &state.pressed_notes,
                    self.octave,
                    &self.midnam_note_names,
                )
            } else {
                draw_partial_octave(
                    renderer,
                    bounds,
                    &state.pressed_notes,
                    self.octave,
                    &self.midnam_note_names,
                    self.note_count,
                )
            }
        } else {
            draw_chromatic_rows(
                renderer,
                bounds,
                &state.pressed_notes,
                self.octave,
                &self.midnam_note_names,
                self.note_count,
            )
        }
    }
}

pub fn row_height(zoom_y: f32) -> f32 {
    ((WHITE_KEY_HEIGHT * WHITE_KEYS_PER_OCTAVE as f32 / NOTES_PER_OCTAVE as f32) * zoom_y).max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::widget::canvas::Program;
    use iced::{Point, Rectangle, Size, event, mouse};

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum TestMessage {
        Pressed(u8),
        Released(u8),
    }

    fn action_message(action: CanvasAction<TestMessage>) -> (Option<TestMessage>, event::Status) {
        let (message, _redraw, status) = action.into_inner();
        (message, status)
    }

    #[test]
    fn octave_keyboard_update_publishes_pressed_and_released_notes() {
        let keyboard = OctaveKeyboard::new(
            4,
            HashMap::new(),
            TestMessage::Pressed,
            TestMessage::Released,
        );
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(20.0, 70.0));
        let press_cursor = mouse::Cursor::Available(Point::new(15.0, 65.0));
        let release_cursor = mouse::Cursor::Available(Point::new(15.0, 65.0));
        let mut state = OctaveKeyboardState::default();

        let press = keyboard
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                press_cursor,
            )
            .expect("press action");
        let (message, status) = action_message(press);
        assert_eq!(message, Some(TestMessage::Pressed(48)));
        assert_eq!(status, event::Status::Captured);
        assert_eq!(state.active_note_class, Some(0));
        assert!(state.pressed_notes.contains(&0));

        let release = keyboard
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                release_cursor,
            )
            .expect("release action");
        let (message, status) = action_message(release);
        assert_eq!(message, Some(TestMessage::Released(48)));
        assert_eq!(status, event::Status::Ignored);
        assert!(state.pressed_notes.is_empty());
    }

    #[test]
    fn partial_octave_keyboard_maps_top_note_to_midi_127() {
        let keyboard = OctaveKeyboard::new(
            10,
            HashMap::new(),
            TestMessage::Pressed,
            TestMessage::Released,
        );
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(20.0, 80.0));
        let cursor = mouse::Cursor::Available(Point::new(15.0, 5.0));
        let mut state = OctaveKeyboardState::default();

        let press = keyboard
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                cursor,
            )
            .expect("press action");
        let (message, status) = action_message(press);

        assert_eq!(message, Some(TestMessage::Pressed(127)));
        assert_eq!(status, event::Status::Captured);
        assert_eq!(state.active_note_class, Some(7));
    }
}
