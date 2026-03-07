use iced::{Color, Theme, widget::button::Style};

pub fn style(theme: &Theme, enabled: bool) -> Style {
    super::track_toggle_button_style(theme, enabled, Color::from_rgb(0.36, 0.90, 0.92))
}
