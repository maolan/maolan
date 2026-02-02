use crate::{
    message::Message,
    state::{State, Track},
    style,
};
use iced::{
    Background, Border, Color, Element, Length,
    widget::{Column, Space, button, column, container, mouse_area, row, text},
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
            let height = track.height;

            track_ui = track_ui.push(text(track.name.clone()));
            track_ui = track_ui.push(row![
                button("R")
                    .padding(3)
                    .style(move |theme, _state| { style::arm::style(theme, track.armed) })
                    .on_press(Message::Request(Action::TrackToggleArm(track.name.clone()))),
                button("M")
                    .padding(3)
                    .style(move |theme, _state| { style::mute::style(theme, track.muted) })
                    .on_press(Message::Request(Action::TrackToggleMute(
                        track.name.clone()
                    ))),
                button("S")
                    .padding(3)
                    .style(move |theme, _state| { style::solo::style(theme, track.soloed) })
                    .on_press(Message::Request(Action::TrackToggleSolo(
                        track.name.clone()
                    ))),
            ]);

            track_ui = track_ui.push(Space::new().height(Length::Fill));

            let resize_handle = mouse_area(
                container("")
                    .width(Length::Fill)
                    .height(Length::Fixed(3.0))
                    .style(|_theme| {
                        use container::Style;
                        Style {
                            background: Some(Background::Color(Color {
                                r: 0.5,
                                g: 0.5,
                                b: 0.5,
                                a: 0.5,
                            })),
                            ..Style::default()
                        }
                    }),
            )
            .on_press(Message::TrackResizeStart(track.name.clone()));

            track_ui = track_ui.push(resize_handle);

            result = result.push(
                mouse_area(
                    container(track_ui)
                        .width(Length::Fill)
                        .height(Length::Fixed(height))
                        .padding(5)
                        .style(move |_theme| {
                            use container::Style;

                            Style {
                                background: if selected {
                                    Some(Background::Color(Color {
                                        r: 1.0,
                                        g: 1.0,
                                        b: 1.0,
                                        a: 0.1,
                                    }))
                                } else {
                                    Some(Background::Color(Color {
                                        r: 0.0,
                                        g: 0.0,
                                        b: 0.0,
                                        a: 0.0,
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
        result.width(self.state.blocking_read().tracks_width).into()
    }
}
