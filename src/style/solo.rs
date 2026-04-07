use iced::{Color, Theme, widget::button::Style};

pub fn style(theme: &Theme, soloed: bool) -> Style {
    super::track_toggle_button_style(theme, soloed, Color::from_rgb(0.45, 0.90, 0.28))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solo_style_returns_style() {
        let theme = Theme::Dark;
        let style = style(&theme, false);
        assert!(style.background.is_some());
    }

    #[test]
    fn solo_style_changes_with_state() {
        let theme = Theme::Dark;
        let unsoloed = style(&theme, false);
        let soloed = style(&theme, true);
        assert_ne!(unsoloed.background.is_some(), soloed.background.is_none());
    }
}
