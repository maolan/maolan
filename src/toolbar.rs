use crate::message::Message;
use iced::{
    Background, Color, Length, Theme,
    widget::{button, row},
};
use iced_fonts::lucide::{audio_lines, cable, pause, play, square};

#[derive(Default)]
pub struct Toolbar {}

impl Toolbar {
    pub fn new() -> Self {
        Self {}
    }
    pub fn update(&mut self, _message: Message) {}

    pub fn view(&self, _playing: bool, recording: bool) -> iced::Element<'_, Message> {
        let btn_style = |theme: &Theme, status: button::Status| {
            let mut style = button::primary(theme, status);

            style.border.radius = 3.0.into();
            style.border.width = 1.0;
            style.border.color = Color::BLACK;
            style.background = Some(Background::Color(Color::TRANSPARENT));
            style
        };
        row![
            row![
                button(play()).style(btn_style).on_press(Message::TransportPlay),
                button(pause())
                    .style(btn_style)
                    .on_press(Message::TransportPause),
                button(square())
                    .style(btn_style)
                    .on_press(Message::TransportStop),
                if recording {
                    button("REC")
                        .style(button::danger)
                        .on_press(Message::TransportRecordToggle)
                } else {
                    button("REC")
                        .style(btn_style)
                        .on_press(Message::TransportRecordToggle)
                },
            ]
            .width(Length::Fill),
            button(audio_lines())
                .style(btn_style)
                .on_press(Message::Workspace),
            button(cable())
                .style(btn_style)
                .on_press(Message::Connections),
        ]
        .into()
    }
}
