use iced::{Color, Theme, widget::button::Style};

pub fn style(theme: &Theme, enabled: bool) -> Style {
    super::track_toggle_button_style(theme, enabled, Color::from_rgb(0.94, 0.42, 0.28))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_style_returns_style() {
        let theme = Theme::Dark;
        let style = style(&theme, false);
        assert!(style.background.is_some());
    }

    #[test]
    fn disk_style_changes_with_state() {
        let theme = Theme::Dark;
        let disabled = style(&theme, false);
        let enabled = style(&theme, true);
        assert_ne!(disabled.background.is_some(), enabled.background.is_none());
    }
}
