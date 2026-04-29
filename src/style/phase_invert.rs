use iced::{Color, Theme, widget::button::Style};

pub fn style(theme: &Theme, phase_inverted: bool) -> Style {
    super::track_toggle_button_style(theme, phase_inverted, Color::from_rgb(0.20, 0.82, 0.92))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_invert_style_returns_style() {
        let theme = Theme::Dark;
        let style = style(&theme, false);
        assert!(style.background.is_some());
    }

    #[test]
    fn phase_invert_style_changes_with_state() {
        let theme = Theme::Dark;
        let normal = style(&theme, false);
        let inverted = style(&theme, true);
        assert_ne!(normal.background.is_some(), inverted.background.is_none());
    }
}
