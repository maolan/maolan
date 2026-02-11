mod add_track;
mod editor;
mod mixer;
mod tracks;

use crate::{
    message::{Message, Show},
    state::State,
};
use iced::{
    Background, Color, Element, Length,
    widget::{column, container, mouse_area, row},
};
use maolan_engine::message::Action;

pub struct Workspace {
    add_track: add_track::AddTrackView,
    editor: editor::Editor,
    mixer: mixer::Mixer,
    modal: Option<Show>,
    tracks: tracks::Tracks,
}

impl Workspace {
    pub fn new(state: State) -> Self {
        Self {
            add_track: add_track::AddTrackView::default(),
            editor: editor::Editor::new(state.clone()),
            mixer: mixer::Mixer::new(state.clone()),
            modal: None,
            tracks: tracks::Tracks::new(state.clone()),
        }
    }

    fn update_children(&mut self, message: Message) {
        self.add_track.update(message.clone());
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::Show(Show::AddTrack) => self.modal = Some(Show::AddTrack),
            Message::Cancel => self.modal = None,
            Message::Response(Ok(Action::AddTrack { .. })) => self.modal = None,
            _ => {}
        }
        self.update_children(message);
    }

    pub fn view(&self) -> Element<'_, Message> {
        match &self.modal {
            Some(Show::AddTrack) => self.add_track.view(),
            Some(Show::Save) | Some(Show::Open) | None => column![
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
            .into(),
        }
    }
}
