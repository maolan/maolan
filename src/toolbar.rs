use crate::message::Message;
use iced::{
    Background, Color, Length, Theme,
    widget::{button, row, text},
};
use iced_fonts::lucide::{audio_lines, cable, circle, play, square};
use maolan_engine::message::Action;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransportLatch {
    Play,
    Stop,
}

#[derive(Debug)]
pub struct Toolbar {
    latch: TransportLatch,
}

impl Toolbar {
    pub fn new() -> Self {
        Self {
            latch: TransportLatch::Stop,
        }
    }
    pub fn update(&mut self, message: Message) {
        match message {
            Message::TransportPlay => self.latch = TransportLatch::Play,
            Message::TransportStop => self.latch = TransportLatch::Stop,
            Message::ToggleTransport => {
                self.latch = if self.latch == TransportLatch::Play {
                    TransportLatch::Stop
                } else {
                    TransportLatch::Play
                };
            }
            Message::Response(Ok(Action::Play)) => self.latch = TransportLatch::Play,
            Message::Response(Ok(Action::Stop)) => self.latch = TransportLatch::Stop,
            _ => {}
        }
    }

    fn button_style(
        enabled: bool,
        active: bool,
        active_color: Color,
    ) -> impl Fn(&Theme, button::Status) -> button::Style + Copy {
        move |theme: &Theme, _status: button::Status| {
            let status = if enabled {
                button::Status::Active
            } else {
                button::Status::Disabled
            };
            let mut style = button::secondary(theme, status);
            style.border.radius = 3.0.into();
            style.border.width = 1.0;
            style.border.color = Color::BLACK;
            style.text_color = if enabled {
                Color::from_rgb(0.92, 0.92, 0.92)
            } else {
                Color::from_rgba(0.92, 0.92, 0.92, 0.45)
            };
            style.background = Some(Background::Color(if active {
                active_color
            } else {
                Color::TRANSPARENT
            }));
            style
        }
    }

    pub fn view(
        &self,
        _playing: bool,
        recording: bool,
        has_loop_range: bool,
        loop_enabled: bool,
    ) -> iced::Element<'_, Message> {
        let play_active = self.latch == TransportLatch::Play;
        let stop_active = self.latch == TransportLatch::Stop;
        let rec_active = recording;
        let loop_active = has_loop_range && loop_enabled;
        let loop_button = if has_loop_range {
            button(text("Loop"))
                .style(Self::button_style(
                    has_loop_range,
                    loop_active,
                    Color::from_rgba(0.2, 0.55, 0.9, 0.35),
                ))
                .on_press(Message::ToggleLoop)
        } else {
            button(text("Loop")).style(Self::button_style(
                has_loop_range,
                loop_active,
                Color::from_rgba(0.2, 0.55, 0.9, 0.35),
            ))
        };
        row![
            row![
                button(play())
                    .style(Self::button_style(
                        true,
                        play_active,
                        Color::from_rgba(0.2, 0.7, 0.35, 0.35)
                    ))
                    .on_press(Message::TransportPlay),
                button(square())
                    .style(Self::button_style(
                        true,
                        stop_active,
                        Color::from_rgba(0.45, 0.45, 0.45, 0.35)
                    ))
                    .on_press(Message::TransportStop),
                button(circle())
                    .style(Self::button_style(
                        true,
                        rec_active,
                        Color::from_rgba(0.9, 0.15, 0.15, 0.45)
                    ))
                    .on_press(Message::TransportRecordToggle),
                loop_button,
            ]
            .width(Length::Fill),
            button(audio_lines())
                .style(Self::button_style(true, false, Color::TRANSPARENT))
                .on_press(Message::Workspace),
            button(cable())
                .style(Self::button_style(true, false, Color::TRANSPARENT))
                .on_press(Message::Connections),
        ]
        .into()
    }
}

impl Default for Toolbar {
    fn default() -> Self {
        Self::new()
    }
}
