use crate::{
    message::Message,
    state::{State, Track},
    style,
};
use iced::{
    Background, Border, Color, Element, Length,
    widget::{Column, button, column, container, mouse_area, row, text},
};
use maolan_engine::message::Action;

#[derive(Debug, Default)]
pub struct Tracks {
    state: State,
}

impl Tracks {
    pub fn new(state: State) -> Self {
        Self { state }
    }

    pub fn update(&mut self, _: Message) {}

    pub fn view(&self) -> Element<'_, Message> {
        let mut result = column![];
        let tracks: Vec<Track> = self.state.blocking_read().tracks.clone();

        for track in tracks {
            let mut track_ui: Column<'_, Message> = column![];
            let name = track.name.clone();
            let selected = self.state.blocking_read().selected.contains(&track.name);

            track_ui = track_ui.push(text(track.name.clone()));
            track_ui = track_ui.push(row![
                button("R")
                    .padding(3)
                    .style(move |theme, _state| { style::arm::style(theme, track.armed) })
                    .on_press(Message::Request(Action::TrackToggleArm(track.name.clone()))),
                button("M")
                    .padding(3)
                    .style(move |theme, _state| { style::mute::style(theme, track.muted) })
                    .on_press(Message::Request(Action::TrackToggleMute(track.name.clone()))),
                button("S")
                    .padding(3)
                    .style(move |theme, _state| { style::solo::style(theme, track.soloed) })
                    .on_press(Message::Request(Action::TrackToggleSolo(track.name.clone()))),
            ]);
            result = result.push(
                mouse_area(
                    container(track_ui)
                        .width(Length::Fill)
                        .height(Length::Fixed(60.0))
                        .padding(5)
                        .style(move |_theme| {
                            use container::Style;

                            Style {
                                background: if selected {
                                    Some(Background::Color(Color {
                                        r: 1.0,
                                        g: 1.0,
                                        b: 1.0,
                                        a: 1.0,
                                    }))
                                } else {
                                    Some(Background::Color(Color {
                                        r: 0.8,
                                        g: 0.8,
                                        b: 0.8,
                                        a: 0.8,
                                    }))
                                },
                                border: Border {
                                    color: Color {
                                        r: 0.0,
                                        g: 0.0,
                                        b: 0.0,
                                        a: 1.0,
                                    },
                                    width: 1.0,
                                    radius: 5.0.into(),
                                },
                                ..Style::default()
                            }
                        }),
                )
                .on_press(Message::SelectTrack(name)),
            );
        }
        result.into()
    }
}
