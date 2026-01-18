use crate::message::Message;
use iced::widget::{button, column, container, container::Style, row};
use iced::{Color, Element, Length, Theme};
use maolan_engine::message::Action;

#[derive(Default)]
pub struct MaolanWorkspace {}

impl MaolanWorkspace {
    pub fn update(&mut self, message: Message) {
        match message {
            _ => {}
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        column![
            row![
                container(
                    button("track list")
                        .padding([4, 8])
                        .on_press(Message::Request(Action::Echo("something".to_string()))),
                )
                .style(|_theme: &Theme| {
                    // let palette = theme.extended_palette();
                    let s = Style::default().background(Color::from_rgb(0.0, 0.0, 0.0));
                    s
                })
                .width(Length::Fill)
                .height(Length::Fill),
                container(
                    button("tracks")
                        .padding([4, 8])
                        .on_press(Message::Request(Action::Echo("else".to_string()))),
                )
                .style(|_theme: &Theme| {
                    Style::default().background(Color::from_rgb(0.5, 0.5, 0.5))
                })
                .width(Length::Fill)
                .height(Length::Fill)
            ],
            container(
                button("mixer")
                    .padding([4, 8])
                    .on_press(Message::Request(Action::Echo("mixer".to_string()))),
            )
            .style(|_theme: &Theme| { Style::default().background(Color::from_rgb(0.3, 0.3, 0.3)) })
            .width(Length::Fill)
            .height(Length::Shrink),
        ]
        .into()
    }
}
