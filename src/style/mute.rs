use iced::{Color, Theme, widget::button::Style};

pub fn style(theme: &Theme, muted: bool) -> Style {
    super::track_toggle_button_style(theme, muted, Color::from_rgb(0.92, 0.85, 0.20))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mute_style_returns_style() {
        let theme = Theme::Dark;
        let style = style(&theme, false);
        assert!(style.background.is_some());
    }

    #[test]
    fn mute_style_changes_with_state() {
        let theme = Theme::Dark;
        let unmuted = style(&theme, false);
        let muted = style(&theme, true);
        assert_ne!(unmuted.background.is_some(), muted.background.is_none());
    }
}
