use crate::{message::Message, state::State, style};
use iced::{
    Background, Border, Color, Element, Length,
    widget::{Column, Space, button, column, container, mouse_area, row, text},
};
use iced_drop::droppable;
use maolan_engine::message::Action;

#[derive(Debug, Default)]
pub struct Tracks {
    state: State,
}

impl Tracks {
    pub fn new(state: State) -> Self {
        Self { state }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let (tracks, selected, width, hovered_resize_track) = {
            let state = self.state.blocking_read();
            (
                state.tracks.clone(),
                state.selected.clone(),
                state.tracks_width,
                state.hovered_track_resize_handle.clone(),
            )
        };

        let result = Column::with_children(tracks.into_iter().enumerate().map(|(index, track)| {
            let selected = selected.contains(&track.name);
            let height = track.height;
            let is_resize_hovered = hovered_resize_track.as_deref() == Some(track.name.as_str());
            let track_ui: Column<'_, Message> = column![
                text(track.name.clone()),
                row![
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
                    button("In")
                        .padding(3)
                        .style(move |theme, _state| {
                            style::input::style(theme, track.input_monitor)
                        })
                        .on_press(Message::Request(Action::TrackToggleInputMonitor(
                            track.name.clone()
                        ))),
                    button("Disk")
                        .padding(3)
                        .style(move |theme, _state| {
                            style::disk::style(theme, track.disk_monitor)
                        })
                        .on_press(Message::Request(Action::TrackToggleDiskMonitor(
                            track.name.clone()
                        ))),
                ],
                Space::new().height(Length::Fill),
                mouse_area(
                    container("")
                        .width(Length::Fill)
                        .height(Length::Fixed(3.0))
                        .style(move |_theme| {
                            use container::Style;
                            Style {
                                background: Some(Background::Color(Color {
                                    r: 0.5,
                                    g: 0.5,
                                    b: 0.5,
                                    a: if is_resize_hovered { 0.8 } else { 0.5 },
                                })),
                                ..Style::default()
                            }
                        }),
                )
                .on_enter(Message::TrackResizeHover(track.name.clone(), true))
                .on_exit(Message::TrackResizeHover(track.name.clone(), false))
                .on_press(Message::TrackResizeStart(track.name.clone())),
            ];

            droppable(
                mouse_area(
                    container(track_ui)
                        .id(track.name.clone())
                        .width(Length::Fill)
                        .height(Length::Fixed(height))
                        .padding(5)
                        .style(move |_theme| container::Style {
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
                            ..container::Style::default()
                        }),
                )
                .on_press(Message::SelectTrack(track.name.clone()))
                .on_double_click(Message::OpenTrackPlugins(track.name.clone())),
            )
            .on_drag(move |_, _| Message::TrackDrag(index))
            .on_drop(Message::TrackDropped)
            .into()
        }));
        mouse_area(result.width(width))
            .on_press(Message::DeselectAll)
            .into()
    }
}
