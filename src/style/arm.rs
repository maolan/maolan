use iced::{Background, Color, Theme, widget::button::Style};

pub fn style(theme: &Theme, armed: bool) -> Style {
    let palette = theme.extended_palette();
    if armed {
        Style {
            background: Some(Background::Color(Color {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            })),
            ..Style::default()
        }
    } else {
        Style {
            background: Some(Background::Color(palette.background.base.color)),
            text_color: palette.background.base.text,
            ..Style::default()
        }
    }
}
