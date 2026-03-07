use iced::{Color, Theme, widget::button::Style};

pub fn style(theme: &Theme, armed: bool) -> Style {
    super::track_toggle_button_style(theme, armed, Color::from_rgb(0.95, 0.20, 0.22))
}
