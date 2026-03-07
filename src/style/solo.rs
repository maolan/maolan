use iced::{Color, Theme, widget::button::Style};

pub fn style(theme: &Theme, soloed: bool) -> Style {
    super::track_toggle_button_style(theme, soloed, Color::from_rgb(0.45, 0.90, 0.28))
}
