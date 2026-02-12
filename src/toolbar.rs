use crate::{message::Message, state::State};
use iced::{
    Background, Color, Length, Theme,
    widget::{button, row, tooltip, tooltip::Position},
};
use iced_fonts::lucide::{audio_lines, cable, pause, play, square};

#[derive(Default)]
pub struct Toolbar {
    state: State,
}

impl Toolbar {
    pub fn new(state: State) -> Self {
        Self { state }
    }
    pub fn update(&mut self, _message: Message) {}

    pub fn view(&self) -> iced::Element<'_, Message> {
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
                tooltip(
                    button(play()).style(btn_style).on_press(Message::Workspace),
                    "Play",
                    Position::Bottom
                ),
                tooltip(
                    button(pause())
                        .style(btn_style)
                        .on_press(Message::Workspace),
                    "Pause",
                    Position::Bottom
                ),
                tooltip(
                    button(square())
                        .style(btn_style)
                        .on_press(Message::Workspace),
                    "Stop",
                    Position::Bottom
                ),
            ]
            .width(Length::Fill),
            tooltip(
                button(audio_lines())
                    .style(btn_style)
                    .on_press(Message::Workspace),
                "Workspace",
                Position::Bottom
            ),
            tooltip(
                button(cable())
                    .style(btn_style)
                    .on_press(Message::Connections),
                "Connections",
                Position::Bottom
            ),
        ]
        .into()
    }
}
