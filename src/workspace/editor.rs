use crate::{
    message::{DraggedClip, Message},
    state::{State, StateData, Track},
};
use iced::{
    Background, Border, Color, Element, Length, Point,
    widget::{Stack, column, container, mouse_area, pin, row, text},
};
use iced_drop::droppable;
use maolan_engine::kind::Kind;

fn view_track_elements(state: &StateData, track: Track) -> Element<'static, Message> {
    let mut clips: Vec<Element<'static, Message>> = vec![];
    let height = track.height;
    let track_name_cloned = track.name.clone();

    for (index, clip) in track.audio.clips.iter().enumerate() {
        let clip_name = clip.name.clone();
        let is_selected = state.selected_clips.contains(&crate::state::ClipId {
            track_idx: track_name_cloned.clone(),
            clip_idx: index,
            kind: Kind::Audio,
        });

        let left_handle = mouse_area(
            container("")
                .width(Length::Fixed(5.0))
                .height(Length::Fill)
                .style(|_theme| {
                    use container::Style;
                    Style {
                        background: Some(Background::Color(Color {
                            r: 0.2,
                            g: 0.4,
                            b: 0.6,
                            a: 0.9,
                        })),
                        ..Style::default()
                    }
                }),
        )
        .on_press(Message::ClipResizeStart(
            Kind::Audio,
            track_name_cloned.clone(),
            index,
            false,
        ));

        let right_handle = mouse_area(
            container("")
                .width(Length::Fixed(5.0))
                .height(Length::Fill)
                .style(|_theme| {
                    use container::Style;
                    Style {
                        background: Some(Background::Color(Color {
                            r: 0.2,
                            g: 0.4,
                            b: 0.6,
                            a: 0.9,
                        })),
                        ..Style::default()
                    }
                }),
        )
        .on_press(Message::ClipResizeStart(
            Kind::MIDI,
            track_name_cloned.clone(),
            index,
            true,
        ));

        let clip_content = mouse_area(
            container(text(clip_name.clone()).size(12))
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(5)
                .style(move |_theme| {
                    use container::Style;
                    Style {
                        background: Some(Background::Color(if is_selected {
                            Color {
                                r: 0.4,
                                g: 0.6,
                                b: 0.8,
                                a: 1.0,
                            }
                        } else {
                            Color {
                                r: 0.3,
                                g: 0.5,
                                b: 0.7,
                                a: 0.8,
                            }
                        })),
                        ..Style::default()
                    }
                }),
        )
        .on_press(Message::SelectClip {
            track_idx: track_name_cloned.clone(),
            clip_idx: index,
            kind: Kind::Audio,
        });

        let clip_widget = container(row![left_handle, clip_content, right_handle])
            .width(Length::Fixed(clip.length as f32))
            .height(Length::Fill)
            .style(|_theme| container::Style {
                background: None,
                border: Border {
                    color: Color {
                        r: 0.2,
                        g: 0.4,
                        b: 0.6,
                        a: 1.0,
                    },
                    width: 1.0,
                    radius: 3.0.into(),
                },
                ..container::Style::default()
            });

        clips.push(
            droppable(pin(clip_widget).position(Point::new(clip.start as f32, 0.0)))
                .on_drag({
                    let track_name_for_drag_closure = track_name_cloned.clone();
                    move |point, _| {
                        let mut clip_data = DraggedClip::new(
                            Kind::Audio,
                            index,
                            track_name_for_drag_closure.clone(),
                        );
                        clip_data.start = point;
                        Message::ClipDrag(clip_data)
                    }
                })
                .on_drop(Message::ClipDropped)
                .into(),
        );
    }
    container(
        Stack::from_vec(clips)
            .height(Length::Fill)
            .width(Length::Fill),
    )
    .id(track_name_cloned)
    .width(Length::Fill)
    .height(Length::Fixed(height))
    .padding(5)
    .style(|_theme| container::Style {
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
        ..container::Style::default()
    })
    .into()
}

#[derive(Debug)]
pub struct Editor {
    state: State,
}

impl Editor {
    pub fn new(state: State) -> Self {
        Self { state }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let mut result = column![];
        let state = self.state.blocking_read();
        for track in state.tracks.iter() {
            result = result.push(view_track_elements(&state, track.clone()));
        }
        mouse_area(result.width(Length::Fill).height(Length::Fill))
            .on_press(Message::DeselectAll)
            .into()
    }
}
