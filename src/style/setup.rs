use iced::{Color, Theme, widget::button::Style};

pub fn style(theme: &Theme, active: bool) -> Style {
    super::track_icon_button_style(theme, active, Color::from_rgb(0.51, 0.68, 0.92))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_style_returns_style() {
        let theme = Theme::Dark;
        let style = style(&theme, false);
        assert!(style.border.radius == 6.0.into());
    }

    #[test]
    fn setup_style_changes_with_state() {
        let theme = Theme::Dark;
        let inactive = style(&theme, false);
        let active = style(&theme, true);
        assert!(inactive.background.is_none());
        assert!(active.background.is_some());
    }
}
