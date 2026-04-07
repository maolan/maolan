use iced::{Color, Theme, widget::button::Style};

pub fn style(theme: &Theme, armed: bool) -> Style {
    super::track_toggle_button_style(theme, armed, Color::from_rgb(0.95, 0.20, 0.22))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arm_style_returns_style() {
        let theme = Theme::Dark;
        let style = style(&theme, false);
        assert!(style.background.is_some());
    }

    #[test]
    fn arm_style_changes_with_state() {
        let theme = Theme::Dark;
        let unarmed = style(&theme, false);
        let armed = style(&theme, true);
        // Background should be different
        assert_ne!(unarmed.background.is_some(), armed.background.is_none());
    }
}
