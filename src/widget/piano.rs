use crate::{message::Message, state::State};
use iced::{
    Background, Color, Element, Event, Length, Point, Rectangle, Renderer, Size, Theme, mouse,
    widget::{
        Id, Stack,
        canvas::{self, Action as CanvasAction, Frame, Geometry, Path, Program},
        column, container, pin, row, scrollable, slider, text, vertical_slider,
    },
};
use std::collections::{HashMap, HashSet};

#[derive(Debug)]
pub struct Piano {
    state: State,
}

pub const KEYS_SCROLL_ID: &str = "piano.keys.scroll";
pub const NOTES_SCROLL_ID: &str = "piano.notes.scroll";
pub const CTRL_SCROLL_ID: &str = "piano.ctrl.scroll";
pub const H_SCROLL_ID: &str = "piano.h.scroll";
pub const V_SCROLL_ID: &str = "piano.v.scroll";

impl Piano {
    pub const KEYBOARD_WIDTH: f32 = 84.0;
    const H_ZOOM_MIN: f32 = 1.0;
    const H_ZOOM_MAX: f32 = 127.0;
    const OCTAVES: usize = 10;
    const WHITE_KEYS_PER_OCTAVE: usize = 7;
    const NOTES_PER_OCTAVE: usize = 12;
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

    fn zoom_x_to_slider(zoom_x: f32) -> f32 {
        (Self::H_ZOOM_MIN + Self::H_ZOOM_MAX - zoom_x).clamp(Self::H_ZOOM_MIN, Self::H_ZOOM_MAX)
    }

    fn slider_to_zoom_x(slider_value: f32) -> f32 {
        (Self::H_ZOOM_MIN + Self::H_ZOOM_MAX - slider_value)
            .clamp(Self::H_ZOOM_MIN, Self::H_ZOOM_MAX)
    }

    pub fn view(&self, pixels_per_sample: f32, samples_per_bar: f32) -> Element<'_, Message> {
        let state = self.state.blocking_read();
        let zoom_x = state.piano_zoom_x;
        let zoom_y = state.piano_zoom_y;

        let Some(roll) = state.piano.as_ref() else {
            return container(text("No MIDI clip selected."))
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
        let pps_notes = (pixels_per_sample * zoom_x).max(0.0001);
        let pps_ctrl = (pixels_per_sample * zoom_x).max(0.0001);
        let notes_w = (roll.clip_length_samples as f32 * pps_notes).max(1.0);
        let ctrl_w = (roll.clip_length_samples as f32 * pps_ctrl).max(1.0);

        let mut note_layers: Vec<Element<'_, Message>> = vec![];
        for i in 0..pitch_count {
            let pitch = Self::PITCH_MAX.saturating_sub(i as u8);
            let is_black = Self::is_black_key(pitch);
            note_layers.push(
                pin(container("")
                    .width(Length::Fixed(notes_w))
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

        let mut ctrl_layers: Vec<Element<'_, Message>> = vec![
            pin(container("")
                .width(Length::Fixed(ctrl_w))
                .height(Length::Fixed(ctrl_h))
                .style(|_theme| container::Style {
                    background: Some(Background::Color(Color {
                        r: 0.16,
                        g: 0.16,
                        b: 0.18,
                        a: 0.9,
                    })),
                    ..container::Style::default()
                }))
            .position(Point::new(0.0, 0.0))
            .into(),
        ];

        let beat_samples = (samples_per_bar / 4.0).max(1.0);
        let mut beat = 0usize;
        loop {
            let x_notes = beat as f32 * beat_samples * pps_notes;
            let x_ctrl = beat as f32 * beat_samples * pps_ctrl;
            if x_notes > notes_w && x_ctrl > ctrl_w {
                break;
            }
            let bar_line = beat.is_multiple_of(4);
            if x_notes <= notes_w {
                note_layers.push(
                    pin(container("")
                        .width(Length::Fixed(if bar_line { 2.0 } else { 1.0 }))
                        .height(Length::Fixed(notes_h))
                        .style(move |_theme| container::Style {
                            background: Some(Background::Color(Color {
                                r: if bar_line { 0.5 } else { 0.35 },
                                g: if bar_line { 0.5 } else { 0.35 },
                                b: if bar_line { 0.55 } else { 0.35 },
                                a: 0.45,
                            })),
                            ..container::Style::default()
                        }))
                    .position(Point::new(x_notes, 0.0))
                    .into(),
                );
            }
            if x_ctrl <= ctrl_w {
                ctrl_layers.push(
                    pin(container("")
                        .width(Length::Fixed(if bar_line { 2.0 } else { 1.0 }))
                        .height(Length::Fixed(ctrl_h))
                        .style(move |_theme| container::Style {
                            background: Some(Background::Color(Color {
                                r: if bar_line { 0.5 } else { 0.35 },
                                g: if bar_line { 0.5 } else { 0.35 },
                                b: if bar_line { 0.55 } else { 0.35 },
                                a: 0.45,
                            })),
                            ..container::Style::default()
                        }))
                    .position(Point::new(x_ctrl, 0.0))
                    .into(),
                );
            }
            beat += 1;
        }

        for note in &roll.notes {
            if note.pitch > Self::PITCH_MAX {
                continue;
            }
            let y_idx = usize::from(Self::PITCH_MAX.saturating_sub(note.pitch));
            let y = y_idx as f32 * row_h + 1.0;
            let x = note.start_sample as f32 * pps_notes;
            let w = (note.length_samples as f32 * pps_notes).max(2.0);
            let color = Self::note_color(note.velocity, note.channel);
            note_layers.push(
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

        for ctrl in &roll.controllers {
            let x = ctrl.sample as f32 * pps_ctrl;
            let h = ((ctrl.value as f32 / 127.0) * (ctrl_h - 10.0)).max(1.0);
            let color = Self::controller_color(ctrl.controller, ctrl.channel);
            ctrl_layers.push(
                pin(container("")
                    .width(Length::Fixed(2.0))
                    .height(Length::Fixed(h))
                    .style(move |_theme| container::Style {
                        background: Some(Background::Color(color)),
                        ..container::Style::default()
                    }))
                .position(Point::new(x, 4.0 + (ctrl_h - h)))
                .into(),
            );
        }

        // Add interactive canvas overlay for note selection and dragging
        note_layers.push(
            pin(iced::widget::canvas(PianoRollInteraction::new(
                self.state.clone(),
                pixels_per_sample,
            ))
            .width(Length::Fixed(notes_w))
            .height(Length::Fixed(notes_h)))
            .position(Point::new(0.0, 0.0))
            .into(),
        );

        let notes_content = Stack::from_vec(note_layers)
            .width(Length::Fixed(notes_w))
            .height(Length::Fixed(notes_h));
        let ctrl_content = Stack::from_vec(ctrl_layers)
            .width(Length::Fixed(ctrl_w))
            .height(Length::Fixed(ctrl_h));

        let octave_h = (notes_h / Self::OCTAVES as f32).max(1.0);
        let midnam_note_names = roll.midnam_note_names.clone();
        let keyboard = (0..Self::OCTAVES).fold(column![], |col, octave_idx| {
            let octave = (Self::OCTAVES - 1 - octave_idx) as u8;
            col.push(
                iced::widget::canvas(OctaveKeyboard::new(octave, midnam_note_names.clone()))
                    .width(Length::Fixed(Self::KEYBOARD_WIDTH))
                    .height(Length::Fixed(octave_h)),
            )
        });
        let piano_note_keys = keyboard
            .width(Length::Fixed(Self::KEYBOARD_WIDTH))
            .height(Length::Fill);
        let controller_key = container(text("Controllers").size(11))
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
            });

        let keyboard_scroll = scrollable(
            container(piano_note_keys)
                .width(Length::Fixed(Self::KEYBOARD_WIDTH))
                .height(Length::Fixed(notes_h))
                .style(|_theme| container::Style {
                    background: Some(Background::Color(Color {
                        r: 0.12,
                        g: 0.12,
                        b: 0.12,
                        a: 1.0,
                    })),
                    ..container::Style::default()
                }),
        )
        .id(Id::new(KEYS_SCROLL_ID))
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::hidden(),
        ))
        .on_scroll(|viewport| Message::PianoScrollYChanged(viewport.relative_offset().y))
        .width(Length::Fixed(Self::KEYBOARD_WIDTH))
        .height(Length::Fill);

        let note_scroll = scrollable(
            container(notes_content)
                .width(Length::Shrink)
                .height(Length::Fixed(notes_h))
                .style(|_theme| container::Style {
                    background: Some(Background::Color(Color {
                        r: 0.07,
                        g: 0.07,
                        b: 0.09,
                        a: 1.0,
                    })),
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

        let ctrl_scroll = scrollable(
            container(ctrl_content)
                .width(Length::Shrink)
                .height(Length::Fixed(ctrl_h))
                .style(|_theme| container::Style {
                    background: Some(Background::Color(Color {
                        r: 0.12,
                        g: 0.12,
                        b: 0.13,
                        a: 1.0,
                    })),
                    ..container::Style::default()
                }),
        )
        .id(Id::new(CTRL_SCROLL_ID))
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::hidden(),
        ))
        .on_scroll(|viewport| Message::PianoScrollXChanged(viewport.relative_offset().x))
        .width(Length::Fill)
        .height(Length::Fixed(ctrl_h));

        let h_scroll = scrollable(
            container("")
                .width(Length::Fixed(notes_w.max(ctrl_w)))
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
        .width(Length::Fixed(16.0))
        .height(Length::Fill);

        container(row![
            column![
                row![keyboard_scroll, note_scroll]
                    .height(Length::Fill)
                    .width(Length::Fill),
                row![controller_key, ctrl_scroll],
                row![
                    container("")
                        .width(Length::Fixed(Self::KEYBOARD_WIDTH))
                        .height(Length::Fixed(16.0)),
                    row![
                        h_scroll,
                        slider(
                            Self::H_ZOOM_MIN..=Self::H_ZOOM_MAX,
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
            .spacing(3),
            column![
                v_scroll,
                vertical_slider(1.0..=8.0, zoom_y, Message::PianoZoomYChanged)
                    .step(0.1)
                    .height(Length::Fixed(100.0)),
            ]
            .spacing(8)
            .height(Length::Fill),
        ])
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }
}

fn draw_octave_with_midnam(
    renderer: &Renderer,
    bounds: Rectangle,
    pressed_notes: &HashSet<u8>,
    octave: u8,
    midnam_note_names: &HashMap<u8, String>,
) -> Vec<canvas::Geometry> {
    let mut frame = Frame::new(renderer, bounds.size());
    let note_height = bounds.height / 12.0;

    // Draw rectangles for each note in the octave (12 notes)
    for i in 0..12 {
        let note_in_octave = 11 - i; // Draw from top to bottom (high to low)
        let midi_note = octave * 12 + note_in_octave;
        let is_pressed = pressed_notes.contains(&note_in_octave);
        let y_pos = i as f32 * note_height;

        // Draw the rectangle
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

        // Draw the note name if available
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

#[derive(Debug, Clone)]
struct OctaveKeyboard {
    octave: u8,
    midnam_note_names: HashMap<u8, String>,
}

impl OctaveKeyboard {
    fn new(octave: u8, midnam_note_names: HashMap<u8, String>) -> Self {
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
        if self.midnam_note_names.is_empty() {
            draw_octave(renderer, bounds, &state.pressed_notes)
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
            if note.pitch > Piano::PITCH_MAX {
                continue;
            }
            let y_idx = usize::from(Piano::PITCH_MAX.saturating_sub(note.pitch));
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
}

#[derive(Default, Debug)]
pub struct PianoRollInteractionState {
    dragging_mode: DraggingMode,
    drag_start: Option<Point>,
}

#[derive(Default, Debug, Clone, Copy, PartialEq)]
enum DraggingMode {
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
        let Some(roll) = app_state.piano.as_ref() else {
            return None;
        };

        let zoom_x = app_state.piano_zoom_x;
        let zoom_y = app_state.piano_zoom_y;
        let row_h = ((Piano::WHITE_KEY_HEIGHT * Piano::WHITE_KEYS_PER_OCTAVE as f32
            / Piano::NOTES_PER_OCTAVE as f32)
            * zoom_y)
            .max(1.0);
        let pps = (self.pixels_per_sample * zoom_x).max(0.0001);
        let notes = roll.notes.clone();
        drop(app_state);

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(position) = cursor.position_in(bounds) {
                    if let Some(note_idx) = self.note_at_position(position, row_h, pps, &notes) {
                        let Some(note) = notes.get(note_idx) else {
                            return None;
                        };
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
                        // Clicking on empty space starts rectangle selection
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
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some(position) = cursor.position_in(bounds) {
                    if state.drag_start.is_some() {
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

        let row_h = ((Piano::WHITE_KEY_HEIGHT * Piano::WHITE_KEYS_PER_OCTAVE as f32
            / Piano::NOTES_PER_OCTAVE as f32)
            * zoom_y)
            .max(1.0);
        let pps = (self.pixels_per_sample * zoom_x).max(0.0001);

        let mut frame = Frame::new(renderer, bounds.size());

        // Draw selection highlights for selected notes
        for &note_idx in selected_notes {
            if let Some(note) = roll.notes.get(note_idx) {
                if note.pitch > Piano::PITCH_MAX {
                    continue;
                }
                let y_idx = usize::from(Piano::PITCH_MAX.saturating_sub(note.pitch));
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

        // Draw dragging notes preview
        if let Some(dragging) = dragging_notes {
            let delta_x = dragging.current_point.x - dragging.start_point.x;
            let delta_y = dragging.current_point.y - dragging.start_point.y;

            for note in &dragging.original_notes {
                if note.pitch > Piano::PITCH_MAX {
                    continue;
                }
                let y_idx = usize::from(Piano::PITCH_MAX.saturating_sub(note.pitch));
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

        // Draw rectangle selection box
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

        // Draw note-resize preview
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

            if original.pitch <= Piano::PITCH_MAX {
                let y_idx = usize::from(Piano::PITCH_MAX.saturating_sub(original.pitch));
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

        // Draw note-creation preview from right-click drag
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
