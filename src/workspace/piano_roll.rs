use crate::{
    message::Message,
    state::{PianoRollNote, State},
};
use iced::{
    Background, Color, Element, Length, Point,
    widget::{Stack, button, column, container, pin, row, scrollable, text},
};

#[derive(Debug)]
pub struct PianoRoll {
    state: State,
}

impl PianoRoll {
    pub fn new(state: State) -> Self {
        Self { state }
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

    fn pitch_range(notes: &[PianoRollNote]) -> (u8, u8) {
        if notes.is_empty() {
            return (36, 84);
        }
        let min = notes.iter().map(|n| n.pitch).min().unwrap_or(36);
        let max = notes.iter().map(|n| n.pitch).max().unwrap_or(84);
        let min = min.saturating_sub(6);
        let max = (max.saturating_add(6)).min(127);
        (min.min(max), max.max(min))
    }

    pub fn view(&self, pixels_per_sample: f32, samples_per_bar: f32) -> Element<'_, Message> {
        let state = self.state.blocking_read();
        let Some(roll) = state.piano_roll.as_ref() else {
            return container(
                column![
                    row![
                        button("Back").on_press(Message::ClosePianoRoll),
                        text("Piano Roll").size(18),
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

        let (pitch_min, pitch_max) = Self::pitch_range(&roll.notes);
        let pitch_count = usize::from(pitch_max.saturating_sub(pitch_min)) + 1;
        let row_h = 14.0_f32;
        let notes_h = (pitch_count as f32 * row_h).max(240.0);
        let ctrl_h = 140.0_f32;
        let total_h = notes_h + ctrl_h;
        let pps = pixels_per_sample.max(0.0001);
        let content_w = (roll.clip_length_samples as f32 * pps).max(1200.0);

        let mut layers: Vec<Element<'_, Message>> = vec![];
        for i in 0..pitch_count {
            let pitch = pitch_max.saturating_sub(i as u8);
            let is_black = matches!(pitch % 12, 1 | 3 | 6 | 8 | 10);
            layers.push(
                pin(
                    container("")
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
                        }),
                )
                .position(Point::new(0.0, i as f32 * row_h))
                .into(),
            );
        }

        let beat_samples = (samples_per_bar / 4.0).max(1.0);
        let mut beat = 0usize;
        loop {
            let x = beat as f32 * beat_samples * pps;
            if x > content_w {
                break;
            }
            let bar_line = beat % 4 == 0;
            layers.push(
                pin(
                    container("")
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
                        }),
                )
                .position(Point::new(x, 0.0))
                .into(),
            );
            beat += 1;
        }

        for note in &roll.notes {
            if note.pitch < pitch_min || note.pitch > pitch_max {
                continue;
            }
            let y_idx = usize::from(pitch_max.saturating_sub(note.pitch));
            let y = y_idx as f32 * row_h + 1.0;
            let x = note.start_sample as f32 * pps;
            let w = (note.length_samples as f32 * pps).max(2.0);
            let color = Self::note_color(note.velocity, note.channel);
            layers.push(
                pin(
                    container("")
                        .width(Length::Fixed(w))
                        .height(Length::Fixed((row_h - 2.0).max(2.0)))
                        .style(move |_theme| container::Style {
                            background: Some(Background::Color(color)),
                            ..container::Style::default()
                        }),
                )
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
                pin(
                    container("")
                        .width(Length::Fixed(2.0))
                        .height(Length::Fixed(h))
                        .style(move |_theme| container::Style {
                            background: Some(Background::Color(color)),
                            ..container::Style::default()
                        }),
                )
                .position(Point::new(x, ctrl_base_y + (ctrl_h - h)))
                .into(),
            );
        }

        let content = Stack::from_vec(layers)
            .width(Length::Fixed(content_w))
            .height(Length::Fixed(total_h));

        container(
            column![
                row![
                    button("Back").on_press(Message::ClosePianoRoll),
                    text(format!(
                        "Piano Roll: {} (track: {}, clip #{})",
                        roll.clip_name, roll.track_idx, roll.clip_idx
                    ))
                    .size(18),
                ]
                .spacing(10),
                container(scrollable(content))
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
