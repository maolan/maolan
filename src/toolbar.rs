use crate::{message::Message, state::State};
use iced::{
    Background, Color, Length, Theme,
    widget::{button, row},
};

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
            row![].width(Length::Fill),
            button("workspace")
                .style(btn_style)
                .on_press(Message::Workspace),
            button("connections")
                .style(btn_style)
                .on_press(Message::Connections)
        ]
        .into()
    }
}
