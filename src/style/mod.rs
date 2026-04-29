pub mod arm;
pub mod disk;
pub mod input;
pub mod mixer;
pub mod mute;
pub mod phase_invert;
pub mod solo;

use crate::consts::APP_BACKGROUND_COLOR;
use iced::{
    Background, Border, Color, Theme,
    widget::{button::Style, container},
};

pub fn app_background() -> container::Style {
    container::Style {
        background: Some(Background::Color(APP_BACKGROUND_COLOR)),
        ..container::Style::default()
    }
}

fn track_toggle_button_style(theme: &Theme, active: bool, accent: Color) -> Style {
    let palette = theme.extended_palette();
    let idle_bg = Color::from_rgba(0.10, 0.13, 0.19, 0.96);
    let idle_border = Color::from_rgba(0.34, 0.42, 0.56, 0.72);
    let active_bg = Color { a: 0.96, ..accent };
    let active_border = Color {
        r: (accent.r + 0.20).min(1.0),
        g: (accent.g + 0.20).min(1.0),
        b: (accent.b + 0.20).min(1.0),
        a: 1.0,
    };
    Style {
        background: Some(Background::Color(if active { active_bg } else { idle_bg })),
        text_color: if active {
            Color::from_rgb(0.08, 0.10, 0.14)
        } else {
            palette.background.base.text
        },
        border: Border {
            color: if active { active_border } else { idle_border },
            width: if active { 1.6 } else { 1.0 },
            radius: 6.0.into(),
        },
        ..Style::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_background_returns_style() {
        let style = app_background();
        assert!(style.background.is_some());
    }

    #[test]
    fn track_toggle_button_style_returns_style() {
        let theme = Theme::Dark;
        let style = track_toggle_button_style(&theme, false, Color::from_rgb(1.0, 0.0, 0.0));
        assert!(style.background.is_some());
        assert!(style.border.width > 0.0);
    }

    #[test]
    fn track_toggle_button_style_active_has_different_border() {
        let theme = Theme::Dark;
        let inactive = track_toggle_button_style(&theme, false, Color::from_rgb(1.0, 0.0, 0.0));
        let active = track_toggle_button_style(&theme, true, Color::from_rgb(1.0, 0.0, 0.0));
        assert_ne!(inactive.border.width, active.border.width);
    }
}
