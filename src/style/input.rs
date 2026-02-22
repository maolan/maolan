use iced::{Background, Color, Theme, widget::button::Style};

pub fn style(theme: &Theme, enabled: bool) -> Style {
    let palette = theme.extended_palette();
    if enabled {
        Style {
            background: Some(Background::Color(Color {
                r: 0.2,
                g: 0.7,
                b: 0.95,
                a: 1.0,
            })),
            ..Style::default()
        }
    } else {
        Style {
            background: Some(Background::Color(Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            })),
            text_color: palette.background.base.text,
            ..Style::default()
        }
    }
}
