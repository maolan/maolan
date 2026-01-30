use crate::{
    message::Message,
    state::{Track},
};
use iced::{
    Background, Border, Color, Element, Length, Point, Renderer, Theme,
    widget::{Stack, column, container, pin, text},
};
use maolan_engine::message::{Action, TrackKind};

#[derive(Debug, Default)]
pub struct Editor {
    tracks: Vec<Track>,
}

impl Editor {
    fn update_children(&mut self, message: Message) {
        match message {
            _ => {
                for track in &mut self.tracks {
                    track.update(message.clone());
                }
            }
        }
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::Response(Ok(ref a)) => match a {
                Action::AddTrack {
                    name,
                    kind,
                    ins,
                    audio_outs,
                    midi_outs,
                } => match kind {
                    TrackKind::Audio => {
                        self.tracks.push(Track::new(
                            name.clone(),
                            TrackKind::Audio,
                            0.0,
                            ins.clone(),
                            audio_outs.clone(),
                            midi_outs.clone(),
                        ));
                    }
                    TrackKind::MIDI => {
                        self.tracks.push(Track::new(
                            name.clone(),
                            TrackKind::MIDI,
                            0.0,
                            ins.clone(),
                            audio_outs.clone(),
                            midi_outs.clone(),
                        ));
                    }
                },
                _ => {}
            },
            _ => {}
        }
        self.update_children(message);
    }

    pub fn view(&self) -> Element<'_, Message> {
        let mut result = column![];
        for track in &self.tracks {
            let mut clips: Vec<Element<'_, Message, Theme, Renderer>> = vec![];
            for clip in &track.clips {
                clips.push(
                    pin(text(clip.name.clone()))
                        .position(Point::new(clip.start, 0.0))
                        .into(),
                );
            }
            result = result.push(
                container(
                    Stack::from_vec(clips)
                        .height(Length::Fill)
                        .width(Length::Fill),
                )
                .width(Length::Fill)
                .height(Length::Fixed(60.0))
                .padding(5)
                .style(|_theme| {
                    use container::Style;

                    Style {
                        background: Some(Background::Color(Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 0.0,
                        })),
                        border: Border {
                            color: Color {
                                r: 0.0,
                                g: 0.0,
                                b: 0.0,
                                a: 1.0,
                            },
                            width: 1.0,
                            radius: 0.0.into(),
                        },
                        ..Style::default()
                    }
                }),
            );
        }
        result.into()
    }
}
