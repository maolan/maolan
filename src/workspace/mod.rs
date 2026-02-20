mod editor;
mod mixer;
mod ruler;
mod tracks;

use crate::{message::Message, state::State};
use iced::{
    Background, Color, Element, Length, Point,
    widget::{Stack, column, container, mouse_area, pin, row},
};

pub struct Workspace {
    state: State,
    editor: editor::Editor,
    mixer: mixer::Mixer,
    ruler: ruler::Ruler,
    tracks: tracks::Tracks,
}

impl Workspace {
    pub fn new(state: State) -> Self {
        Self {
            state: state.clone(),
            editor: editor::Editor::new(state.clone()),
            mixer: mixer::Mixer::new(state.clone()),
            ruler: ruler::Ruler::new(),
            tracks: tracks::Tracks::new(state.clone()),
        }
    }

    pub fn update(&mut self, _message: Message) {}

    fn playhead_line() -> Element<'static, Message> {
        container("")
            .width(Length::Fixed(2.0))
            .height(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(Background::Color(Color {
                    r: 0.95,
                    g: 0.18,
                    b: 0.14,
                    a: 0.95,
                })),
                ..container::Style::default()
            })
            .into()
    }

    pub fn view(&self, playhead_x: Option<f32>) -> Element<'_, Message> {
        let tracks_width = self.state.blocking_read().tracks_width;

        let editor_with_playhead = if let Some(x) = playhead_x {
            Stack::from_vec(vec![
                self.editor.view(),
                pin(Self::playhead_line())
                    .position(Point::new(x.max(0.0), 0.0))
                    .into(),
            ])
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        } else {
            self.editor.view()
        };

        column![
            row![
                container("")
                    .width(tracks_width)
                    .height(Length::Fill)
                    .style(|_theme| container::Style {
                        background: Some(Background::Color(Color {
                            r: 0.1,
                            g: 0.1,
                            b: 0.1,
                            a: 1.0,
                        })),
                        ..container::Style::default()
                    }),
                container("")
                    .width(Length::Fixed(3.0))
                    .height(Length::Fill)
                    .style(|_theme| {
                        container::Style {
                            background: Some(Background::Color(Color {
                                r: 0.5,
                                g: 0.5,
                                b: 0.5,
                                a: 0.5,
                            })),
                            ..container::Style::default()
                        }
                    }),
                self.ruler.view(playhead_x),
            ]
            .height(Length::Fixed(self.ruler.height())),
            row![
                self.tracks.view(),
                mouse_area(
                    container("")
                        .width(Length::Fixed(3.0))
                        .height(Length::Fill)
                        .style(|_theme| {
                            container::Style {
                                background: Some(Background::Color(Color {
                                    r: 0.5,
                                    g: 0.5,
                                    b: 0.5,
                                    a: 0.5,
                                })),
                                ..container::Style::default()
                            }
                        }),
                )
                .on_press(Message::TracksResizeStart),
                editor_with_playhead,
            ]
            .height(Length::Fill),
            mouse_area(
                container("")
                    .width(Length::Fill)
                    .height(Length::Fixed(3.0))
                    .style(|_theme| {
                        container::Style {
                            background: Some(Background::Color(Color {
                                r: 0.5,
                                g: 0.5,
                                b: 0.5,
                                a: 0.5,
                            })),
                            ..container::Style::default()
                        }
                    }),
            )
            .on_press(Message::MixerResizeStart),
            self.mixer.view(),
        ]
        .width(Length::Fill)
        .into()
    }
}
