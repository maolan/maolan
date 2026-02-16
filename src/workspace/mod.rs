mod editor;
mod mixer;
mod tracks;

use crate::{message::Message, state::State};
use iced::{
    Background, Color, Element, Length,
    widget::{column, container, mouse_area, row},
};

pub struct Workspace {
    editor: editor::Editor,
    mixer: mixer::Mixer,
    tracks: tracks::Tracks,
}

impl Workspace {
    pub fn new(state: State) -> Self {
        Self {
            editor: editor::Editor::new(state.clone()),
            mixer: mixer::Mixer::new(state.clone()),
            tracks: tracks::Tracks::new(state.clone()),
        }
    }

    pub fn update(&mut self, _message: Message) {}

    pub fn view(&self) -> Element<'_, Message> {
        column![
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
                self.editor.view(),
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
