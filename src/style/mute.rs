use iced::{Color, Theme, widget::button::Style};

pub fn style(theme: &Theme, muted: bool) -> Style {
    super::track_toggle_button_style(theme, muted, Color::from_rgb(0.92, 0.85, 0.20))
}
