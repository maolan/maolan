use iced::{Color, Theme, widget::button::Style};

pub fn style(theme: &Theme, enabled: bool) -> Style {
    super::track_toggle_button_style(theme, enabled, Color::from_rgb(0.94, 0.42, 0.28))
}
