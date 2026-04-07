use iced::{Color, Theme, widget::button::Style};

pub fn style(theme: &Theme, enabled: bool) -> Style {
    super::track_toggle_button_style(theme, enabled, Color::from_rgb(0.36, 0.90, 0.92))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_style_returns_style() {
        let theme = Theme::Dark;
        let style = style(&theme, false);
        assert!(style.background.is_some());
    }

    #[test]
    fn input_style_changes_with_state() {
        let theme = Theme::Dark;
        let disabled = style(&theme, false);
        let enabled = style(&theme, true);
        assert_ne!(disabled.background.is_some(), enabled.background.is_none());
    }
}
