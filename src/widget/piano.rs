use crate::{message::Message, state::State};
use iced::{
    Background, Color, Element, Event, Length, Point, Rectangle, Renderer, Size, Theme, mouse,
    widget::{
        Stack, button,
        canvas::{self, Action as CanvasAction, Frame, Geometry, Path, Program},
        column, container, pin, row, slider, text,
    },
};
use std::collections::HashSet;

#[derive(Debug)]
pub struct Piano {
    state: State,
}

impl Piano {
    const KEYBOARD_WIDTH: f32 = 84.0;
    const OCTAVES: usize = 10;
    const WHITE_KEYS_PER_OCTAVE: usize = 7;
    const NOTES_PER_OCTAVE: usize = 12;
    const PITCH_MIN: u8 = 0;
    const PITCH_MAX: u8 = (Self::OCTAVES as u8 * Self::NOTES_PER_OCTAVE as u8) - 1;
    const WHITE_KEY_HEIGHT: f32 = 14.0;

    pub fn new(state: State) -> Self {
        Self { state }
    }

    fn is_black_key(pitch: u8) -> bool {
        matches!(pitch % 12, 1 | 3 | 6 | 8 | 10)
    }

    fn note_color(velocity: u8, channel: u8) -> Color {
        let t = (velocity as f32 / 127.0).clamp(0.0, 1.0);
        let c = (channel as f32 / 15.0).clamp(0.0, 1.0);
        Color {
            r: 0.25 + 0.45 * t,
            g: 0.35 + 0.4 * (1.0 - c),
            b: 0.65 + 0.3 * c,
            a: 0.9,
        }
    }

    fn controller_color(controller: u8, channel: u8) -> Color {
        let h = (controller as f32 / 127.0).clamp(0.0, 1.0);
        let c = (channel as f32 / 15.0).clamp(0.0, 1.0);
        Color {
            r: 0.3 + 0.5 * h,
            g: 0.85 - 0.45 * h,
            b: 0.25 + 0.45 * (1.0 - c),
            a: 0.85,
        }
    }

    pub fn view(&self, pixels_per_sample: f32, samples_per_bar: f32) -> Element<'_, Message> {
        let state = self.state.blocking_read();
        let zoom_x = state.piano_zoom_x;
        let zoom_y = state.piano_zoom_y;

        let Some(roll) = state.piano.as_ref() else {
            return container(
                column![
                    row![
                        button("Back").on_press(Message::ClosePiano),
                        text("Piano").size(18),
                    ]
                    .spacing(10),
                    text("No MIDI clip selected."),
                ]
                .spacing(10)
                .padding(12),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
        };

        let pitch_count = Self::OCTAVES * Self::NOTES_PER_OCTAVE;
        let row_h = ((Self::WHITE_KEY_HEIGHT * Self::WHITE_KEYS_PER_OCTAVE as f32
            / Self::NOTES_PER_OCTAVE as f32)
            * zoom_y)
            .max(1.0);
        let notes_h = pitch_count as f32 * row_h;
        let ctrl_h = 140.0_f32;
        let total_h = notes_h + ctrl_h;
        let pps = (pixels_per_sample * zoom_x).max(0.0001);
        let content_w = (roll.clip_length_samples as f32 * pps).max(1.0);

        let mut layers: Vec<Element<'_, Message>> = vec![];
        for i in 0..pitch_count {
            let pitch = Self::PITCH_MAX.saturating_sub(i as u8);
            let is_black = Self::is_black_key(pitch);
            layers.push(
                pin(container("")
                    .width(Length::Fixed(content_w))
                    .height(Length::Fixed(row_h))
                    .style(move |_theme| container::Style {
                        background: Some(Background::Color(if is_black {
                            Color {
                                r: 0.08,
                                g: 0.08,
                                b: 0.1,
                                a: 0.85,
                            }
                        } else {
                            Color {
                                r: 0.12,
                                g: 0.12,
                                b: 0.14,
                                a: 0.85,
                            }
                        })),
                        ..container::Style::default()
                    }))
                .position(Point::new(0.0, i as f32 * row_h))
                .into(),
            );
        }

        // Controller lane base is a touch brighter than note lanes.
        layers.push(
            pin(
                container("")
                    .width(Length::Fixed(content_w))
                    .height(Length::Fixed(ctrl_h))
                    .style(|_theme| container::Style {
                        background: Some(Background::Color(Color {
                            r: 0.16,
                            g: 0.16,
                            b: 0.18,
                            a: 0.9,
                        })),
                        ..container::Style::default()
                    }),
            )
            .position(Point::new(0.0, notes_h))
            .into(),
        );

        let beat_samples = (samples_per_bar / 4.0).max(1.0);
        let mut beat = 0usize;
        loop {
            let x = beat as f32 * beat_samples * pps;
            if x > content_w {
                break;
            }
            let bar_line = beat % 4 == 0;
            layers.push(
                pin(container("")
                    .width(Length::Fixed(if bar_line { 2.0 } else { 1.0 }))
                    .height(Length::Fixed(total_h))
                    .style(move |_theme| container::Style {
                        background: Some(Background::Color(Color {
                            r: if bar_line { 0.5 } else { 0.35 },
                            g: if bar_line { 0.5 } else { 0.35 },
                            b: if bar_line { 0.55 } else { 0.35 },
                            a: 0.45,
                        })),
                        ..container::Style::default()
                    }))
                .position(Point::new(x, 0.0))
                .into(),
            );
            beat += 1;
        }

        for note in &roll.notes {
            if note.pitch < Self::PITCH_MIN || note.pitch > Self::PITCH_MAX {
                continue;
            }
            let y_idx = usize::from(Self::PITCH_MAX.saturating_sub(note.pitch));
            let y = y_idx as f32 * row_h + 1.0;
            let x = note.start_sample as f32 * pps;
            let w = (note.length_samples as f32 * pps).max(2.0);
            let color = Self::note_color(note.velocity, note.channel);
            layers.push(
                pin(container("")
                    .width(Length::Fixed(w))
                    .height(Length::Fixed((row_h - 2.0).max(2.0)))
                    .style(move |_theme| container::Style {
                        background: Some(Background::Color(color)),
                        ..container::Style::default()
                    }))
                .position(Point::new(x, y))
                .into(),
            );
        }

        let ctrl_base_y = notes_h + 4.0;
        for ctrl in &roll.controllers {
            let x = ctrl.sample as f32 * pps;
            let h = ((ctrl.value as f32 / 127.0) * (ctrl_h - 10.0)).max(1.0);
            let color = Self::controller_color(ctrl.controller, ctrl.channel);
            layers.push(
                pin(container("")
                    .width(Length::Fixed(2.0))
                    .height(Length::Fixed(h))
                    .style(move |_theme| container::Style {
                        background: Some(Background::Color(color)),
                        ..container::Style::default()
                    }))
                .position(Point::new(x, ctrl_base_y + (ctrl_h - h)))
                .into(),
            );
        }

        let content = Stack::from_vec(layers)
            .width(Length::Fixed(content_w))
            .height(Length::Fixed(total_h));

        let octave_h = (notes_h / Self::OCTAVES as f32).max(1.0);
        let keyboard = (0..Self::OCTAVES).fold(column![], |col, octave_idx| {
            let octave = (Self::OCTAVES - 1 - octave_idx) as u8;
            col.push(
                iced::widget::canvas(OctaveKeyboard::new(octave))
                    .width(Length::Fixed(Self::KEYBOARD_WIDTH))
                    .height(Length::Fixed(octave_h)),
            )
        });
        let piano_keys = keyboard
            .push(
                container(text("Controllers").size(11))
                    .width(Length::Fixed(Self::KEYBOARD_WIDTH))
                    .height(Length::Fixed(ctrl_h))
                    .padding([4, 6])
                    .style(|_theme| container::Style {
                        background: Some(Background::Color(Color {
                            r: 0.15,
                            g: 0.15,
                            b: 0.16,
                            a: 1.0,
                        })),
                        ..container::Style::default()
                    }),
            )
            .width(Length::Fixed(Self::KEYBOARD_WIDTH))
            .height(Length::Fixed(total_h));

        container(
            column![
                row![
                    button("Back").on_press(Message::ClosePiano),
                    text(format!(
                        "Piano: {} (track: {}, clip #{})",
                        roll.clip_name, roll.track_idx, roll.clip_idx
                    ))
                    .size(18),
                    text("H Zoom").size(12),
                    slider(1.0..=127.0, zoom_x, Message::PianoZoomXChanged)
                        .step(0.1)
                        .width(Length::Fixed(110.0)),
                    text(format!("{zoom_x:.2}x")).size(12),
                    text("V Zoom").size(12),
                    slider(0.25..=6.0, zoom_y, Message::PianoZoomYChanged)
                        .step(0.1)
                        .width(Length::Fixed(110.0)),
                    text(format!("{zoom_y:.2}x")).size(12),
                ]
                .spacing(10),
                row![
                    container(piano_keys)
                        .width(Length::Fixed(Self::KEYBOARD_WIDTH))
                        .height(Length::Fill)
                        .style(|_theme| container::Style {
                            background: Some(Background::Color(Color {
                                r: 0.12,
                                g: 0.12,
                                b: 0.12,
                                a: 1.0,
                            })),
                            ..container::Style::default()
                        }),
                    container(content)
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .style(|_theme| container::Style {
                            background: Some(Background::Color(Color {
                                r: 0.07,
                                g: 0.07,
                                b: 0.09,
                                a: 1.0,
                            })),
                            ..container::Style::default()
                        }),
                ]
                .height(Length::Fill),
                row![
                    text(format!("Notes: {}", roll.notes.len())).size(12),
                    text(format!("Controllers: {}", roll.controllers.len())).size(12),
                ]
                .spacing(12),
            ]
            .spacing(8)
            .padding(10),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }
}

fn draw_octave(
    renderer: &Renderer,
    bounds: Rectangle,
    pressed_notes: &HashSet<u8>,
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

#[derive(Default, Debug)]
struct OctaveKeyboard {
    octave: u8,
}

impl OctaveKeyboard {
    fn new(octave: u8) -> Self {
        Self { octave }
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
struct OctaveKeyboardState {
    pressed_notes: HashSet<u8>,
    active_note_class: Option<u8>,
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
                    return Some(
                        CanvasAction::publish(Message::PianoKeyReleased(self.midi_note(note_class))),
                    );
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
        draw_octave(renderer, bounds, &state.pressed_notes)
    }
}
