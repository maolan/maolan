use crate::{
    midi::{PITCH_MAX, PianoNote},
    piano::{note_color, note_two_edge_gradient, row_height},
};
use iced::{
    Element, Length, Point,
    widget::{container, pin},
};

pub struct PianoRoll<'a, Message> {
    notes: Vec<PianoNote>,
    clip_length_samples: usize,
    zoom_y: f32,
    pixels_per_sample: f32,
    zoom_x: f32,
    interaction: Element<'a, Message>,
}

impl<'a, Message: 'a> PianoRoll<'a, Message> {
    pub fn new(
        notes: Vec<PianoNote>,
        clip_length_samples: usize,
        zoom_y: f32,
        pixels_per_sample: f32,
        zoom_x: f32,
        interaction: Element<'a, Message>,
    ) -> Self {
        Self {
            notes,
            clip_length_samples,
            zoom_y,
            pixels_per_sample,
            zoom_x,
            interaction,
        }
    }

    pub fn into_element(self) -> Element<'a, Message> {
        let row_h = row_height(self.zoom_y);
        let pps = (self.pixels_per_sample * self.zoom_x).max(0.0001);
        let notes_w = (self.clip_length_samples as f32 * pps).max(1.0);
        let notes_h = crate::midi::MIDI_NOTE_COUNT as f32 * row_h;

        let mut layers: Vec<Element<'a, Message>> = vec![];
        for note in &self.notes {
            if note.pitch > PITCH_MAX {
                continue;
            }
            let y_idx = usize::from(PITCH_MAX.saturating_sub(note.pitch));
            let y = y_idx as f32 * row_h + 1.0;
            let x = note.start_sample as f32 * pps;
            let w = (note.length_samples as f32 * pps).max(2.0);
            let color = note_color(note.velocity, note.channel);
            layers.push(
                pin(container("")
                    .width(Length::Fixed(w))
                    .height(Length::Fixed((row_h - 2.0).max(2.0)))
                    .style(move |_theme| container::Style {
                        background: Some(note_two_edge_gradient(color)),
                        ..container::Style::default()
                    }))
                .position(Point::new(x, y))
                .into(),
            );
        }

        layers.push(pin(self.interaction).position(Point::new(0.0, 0.0)).into());

        iced::widget::Stack::from_vec(layers)
            .width(Length::Fixed(notes_w))
            .height(Length::Fixed(notes_h))
            .into()
    }
}
