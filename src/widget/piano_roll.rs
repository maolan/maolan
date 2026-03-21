use crate::{
    consts::widget_piano::{
        MIDI_NOTE_COUNT, NOTES_PER_OCTAVE, PITCH_MAX, WHITE_KEY_HEIGHT, WHITE_KEYS_PER_OCTAVE,
    },
    message::Message,
    state::State,
    widget::piano::{self, PianoRollInteraction},
};
use iced::{
    Element, Length, Point,
    widget::{container, pin},
};

pub fn view(state_handle: State, pixels_per_sample: f32) -> Element<'static, Message> {
    let state = state_handle.blocking_read();
    let zoom_x = state.piano_zoom_x;
    let zoom_y = state.piano_zoom_y;

    let roll = match state.piano.as_ref() {
        Some(r) => r,
        None => return container("").into(),
    };

    let row_h = ((WHITE_KEY_HEIGHT * WHITE_KEYS_PER_OCTAVE as f32 / NOTES_PER_OCTAVE as f32)
        * zoom_y)
        .max(1.0);
    let pps = (pixels_per_sample * zoom_x).max(0.0001);
    let notes_w = (roll.clip_length_samples as f32 * pps).max(1.0);
    let notes_h = MIDI_NOTE_COUNT as f32 * row_h;

    let mut layers: Vec<Element<'static, Message>> = vec![];
    for note in &roll.notes {
        if note.pitch > PITCH_MAX {
            continue;
        }
        let y_idx = usize::from(PITCH_MAX.saturating_sub(note.pitch));
        let y = y_idx as f32 * row_h + 1.0;
        let x = note.start_sample as f32 * pps;
        let w = (note.length_samples as f32 * pps).max(2.0);
        let color = piano::note_color(note.velocity, note.channel);
        layers.push(
            pin(container("")
                .width(Length::Fixed(w))
                .height(Length::Fixed((row_h - 2.0).max(2.0)))
                .style(move |_theme| container::Style {
                    background: Some(piano::note_two_edge_gradient(color)),
                    ..container::Style::default()
                }))
            .position(Point::new(x, y))
            .into(),
        );
    }

    layers.push(
        pin(iced::widget::canvas(PianoRollInteraction::new(
            state_handle.clone(),
            pixels_per_sample,
        ))
        .width(Length::Fixed(notes_w))
        .height(Length::Fixed(notes_h)))
        .position(Point::new(0.0, 0.0))
        .into(),
    );

    iced::widget::Stack::from_vec(layers)
        .width(Length::Fixed(notes_w))
        .height(Length::Fixed(notes_h))
        .into()
}
