use crate::{message::Message, state::State, style};
use iced::{
    Background, Border, Color, Element, Length,
    widget::{Column, Space, button, column, container, mouse_area, row, text},
};
use iced_aw::ContextMenu;
use iced_drop::droppable;
use iced_fonts::lucide::{audio_waveform, disc};
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
            let layout = track.lane_layout();
            let lane_h = layout.lane_height.max(12.0);
            let mut lane_rows: Column<'_, Message> = column![];
            for lane in 0..track.audio.ins {
                lane_rows = lane_rows.push(
                    container(text(format!("Audio {:02}", lane + 1)).size(11))
                        .width(Length::Fill)
                        .height(Length::Fixed(lane_h))
                        .padding(4)
                        .style(|_theme| container::Style {
                            background: Some(Background::Color(Color {
                                r: 0.16,
                                g: 0.23,
                                b: 0.3,
                                a: 0.5,
                            })),
                            ..container::Style::default()
                        }),
                );
            }
            for lane in 0..track.midi.ins {
                lane_rows = lane_rows.push(
                    container(text(format!("MIDI {:02}", lane + 1)).size(11))
                        .width(Length::Fill)
                        .height(Length::Fixed(lane_h))
                        .padding(4)
                        .style(|_theme| container::Style {
                            background: Some(Background::Color(Color {
                                r: 0.14,
                                g: 0.28,
                                b: 0.17,
                                a: 0.55,
                            })),
                            ..container::Style::default()
                        }),
                );
            }

            let track_ui: Column<'_, Message> = column![
                row![
                    text(format!("▾ {}", track.name.clone())),
                    Space::new().width(Length::Fill),
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
                    button(audio_waveform())
                        .padding(3)
                        .style(move |theme, _state| {
                            style::input::style(theme, track.input_monitor)
                        })
                        .on_press(Message::Request(Action::TrackToggleInputMonitor(
                            track.name.clone()
                        ))),
                    button(disc())
                        .padding(3)
                        .style(move |theme, _state| {
                            style::disk::style(theme, track.disk_monitor)
                        })
                        .on_press(Message::Request(Action::TrackToggleDiskMonitor(
                            track.name.clone()
                        ))),
                ]
                .height(Length::Fixed(layout.header_height)),
                lane_rows.height(Length::Fill),
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
            ]
            .spacing(2.0);

            {
                let track_name_for_menu = track.name.clone();
                let track_with_mouse = mouse_area(
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
                .on_double_click(Message::OpenTrackPlugins(track.name.clone()));

                let track_with_context = ContextMenu::new(
                    track_with_mouse,
                    move || {
                        button("Rename")
                            .on_press(Message::TrackRenameShow(track_name_for_menu.clone()))
                            .into()
                    },
                );

                droppable(track_with_context)
                    .on_drag(move |_, _| Message::TrackDrag(index))
                    .on_drop(Message::TrackDropped)
                    .into()
            }
        }));
        mouse_area(result.width(width))
            .on_press(Message::DeselectAll)
            .into()
    }
}
