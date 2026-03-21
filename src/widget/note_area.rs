use crate::{
    consts::widget_piano::{
        KEYBOARD_WIDTH, KEYS_SCROLL_ID, NOTES_SCROLL_ID, RIGHT_SCROLL_GUTTER_WIDTH,
    },
    consts::{
        widget_piano::{
            MIDI_NOTE_COUNT, NOTES_PER_OCTAVE, PITCH_MAX, WHITE_KEY_HEIGHT, WHITE_KEYS_PER_OCTAVE,
        },
        workspace::PLAYHEAD_WIDTH_PX,
    },
    message::Message,
    widget::{
        horizontal_scrollbar::HorizontalScrollbar, piano, vertical_scrollbar::VerticalScrollbar,
    },
};
use iced::{
    Background, Color, Element, Length, Point,
    widget::{Id, Stack, container, pin, scrollable},
};

pub struct NoteArea {
    pub zoom_x: f32,
    pub zoom_y: f32,
    pub pixels_per_sample: f32,
    pub samples_per_bar: Option<f32>,
    pub playhead_x: Option<f32>,
    pub clip_length_samples: usize,
}

impl NoteArea {
    pub fn view(self, content: Vec<Element<'static, Message>>) -> Element<'static, Message> {
        let pitch_count = MIDI_NOTE_COUNT;
        let row_h = ((WHITE_KEY_HEIGHT * WHITE_KEYS_PER_OCTAVE as f32 / NOTES_PER_OCTAVE as f32)
            * self.zoom_y)
            .max(1.0);
        let notes_h = pitch_count as f32 * row_h;
        let pps = (self.pixels_per_sample * self.zoom_x).max(0.0001);
        let notes_w = (self.clip_length_samples as f32 * pps).max(1.0);

        let mut layers: Vec<Element<'static, Message>> = vec![];

        // Background Grid
        for i in 0..pitch_count {
            let pitch = PITCH_MAX.saturating_sub(i as u8);
            let is_black = piano::is_black_key(pitch);
            layers.push(
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

        // Beat Lines
        if let Some(samples_per_bar) = self.samples_per_bar {
            let beat_samples = (samples_per_bar / 4.0).max(1.0);
            let mut beat = 0usize;
            loop {
                let x = beat as f32 * beat_samples * pps;
                if x > notes_w {
                    break;
                }
                let bar_line = beat.is_multiple_of(4);
                layers.push(
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
                    .position(Point::new(x, 0.0))
                    .into(),
                );
                beat += 1;
            }
        }

        // Content
        for item in content {
            layers.push(item);
        }

        // Playhead
        if let Some(x) = self.playhead_x {
            let x = x.max(0.0);
            layers.push(
                pin(container("")
                    .width(Length::Fixed(PLAYHEAD_WIDTH_PX))
                    .height(Length::Fixed(notes_h))
                    .style(|_theme| container::Style {
                        background: Some(Background::Color(Color::from_rgba(
                            0.95, 0.18, 0.14, 0.95,
                        ))),
                        ..container::Style::default()
                    }))
                .position(Point::new(x, 0.0))
                .into(),
            );
        }

        Stack::from_vec(layers)
            .width(Length::Fixed(notes_w))
            .height(Length::Fixed(notes_h))
            .into()
    }
}

pub struct PianoGridScrolls {
    pub keyboard_scroll: Element<'static, Message>,
    pub note_scroll: Element<'static, Message>,
    pub h_scroll: Element<'static, Message>,
    pub v_scroll: Element<'static, Message>,
}

pub fn piano_grid_scrollers(
    keyboard: Element<'static, Message>,
    notes_content: Element<'static, Message>,
    notes_h: f32,
    notes_w: f32,
    scroll_x: f32,
    scroll_y: f32,
) -> PianoGridScrolls {
    let keyboard_scroll = scrollable(
        container(keyboard)
            .width(Length::Fixed(KEYBOARD_WIDTH))
            .height(Length::Fixed(notes_h)),
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

    let h_scroll = HorizontalScrollbar::new(notes_w, scroll_x, Message::PianoScrollXChanged)
        .width(Length::Fill)
        .height(Length::Fixed(16.0));

    let v_scroll = VerticalScrollbar::new(notes_h, scroll_y, Message::PianoScrollYChanged)
        .width(Length::Fixed(RIGHT_SCROLL_GUTTER_WIDTH))
        .height(Length::Fill);

    PianoGridScrolls {
        keyboard_scroll: keyboard_scroll.into(),
        note_scroll: note_scroll.into(),
        h_scroll: h_scroll.into(),
        v_scroll: v_scroll.into(),
    }
}
