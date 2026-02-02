use iced::widget::canvas::{self, Frame, Geometry, Path, Program};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme, mouse};
use std::collections::HashSet;

pub fn draw_octave(
    renderer: &iced::Renderer,
    bounds: Rectangle,
    pressed_notes: &std::collections::HashSet<u8>,
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
    }

    let black_key_offsets = [1, 2, 4, 5, 6];
    let black_note_ids = [1, 3, 6, 8, 10];
    let black_key_width = bounds.width * 0.6;
    let black_key_height = white_key_height * 0.6;

    for (idx, offset) in black_key_offsets.iter().enumerate() {
        let is_pressed = pressed_notes.contains(&black_note_ids[idx]);
        let y_pos_black =
            bounds.height - (*offset as f32 * white_key_height) - (black_key_height / 2.0);
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
#[derive(Default, Debug)]
pub struct PianoKeyboard {
    pub pressed_notes: HashSet<u8>,
}

impl<Message> Program<Message> for PianoKeyboard {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        draw_octave(renderer, bounds, &self.pressed_notes)
    }
}
