use iced::{Background, Border, Color, gradient, widget::container};

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
}

fn brighten(color: Color, amount: f32) -> Color {
    Color {
        r: (color.r + amount).min(1.0),
        g: (color.g + amount).min(1.0),
        b: (color.b + amount).min(1.0),
        a: color.a,
    }
}

fn darken(color: Color, amount: f32) -> Color {
    Color {
        r: (color.r - amount).max(0.0),
        g: (color.g - amount).max(0.0),
        b: (color.b - amount).max(0.0),
        a: color.a,
    }
}

pub fn strip(selected: bool) -> container::Style {
    let border_color = if selected {
        rgb(114, 170, 240)
    } else {
        rgb(39, 47, 63)
    };
    let base = rgb(26, 32, 46);
    let side = darken(base, 0.040);
    let center = brighten(base, 0.045);

    container::Style {
        background: Some(Background::Gradient(
            gradient::Linear::new(90.0)
                .add_stop(0.0, side)
                .add_stop(0.5, center)
                .add_stop(1.0, side)
                .into(),
        )),
        border: Border {
            color: border_color,
            width: if selected { 1.5 } else { 1.0 },
            radius: 7.0.into(),
        },
        ..container::Style::default()
    }
}

pub fn bay() -> container::Style {
    container::Style {
        background: Some(Background::Color(rgb(20, 25, 37))),
        border: Border {
            color: rgb(51, 59, 77),
            width: 1.0,
            radius: 5.0.into(),
        },
        ..container::Style::default()
    }
}

pub fn readout() -> container::Style {
    container::Style {
        background: Some(Background::Color(rgb(31, 38, 52))),
        border: Border {
            color: rgb(63, 74, 95),
            width: 1.0,
            radius: 4.0.into(),
        },
        ..container::Style::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_converts_bytes_to_color() {
        let color = rgb(255, 128, 0);
        assert_eq!(color.r, 1.0);
        assert_eq!(color.g, 128.0 / 255.0);
        assert_eq!(color.b, 0.0);
    }

    #[test]
    fn brighten_increases_color_values() {
        let base = Color::from_rgb(0.5, 0.5, 0.5);
        let brightened = brighten(base, 0.2);
        assert!(brightened.r > base.r);
        assert!(brightened.g > base.g);
        assert!(brightened.b > base.b);
    }

    #[test]
    fn brighten_clamps_at_1() {
        let base = Color::from_rgb(0.9, 0.9, 0.9);
        let brightened = brighten(base, 0.2);
        assert_eq!(brightened.r, 1.0);
        assert_eq!(brightened.g, 1.0);
        assert_eq!(brightened.b, 1.0);
    }

    #[test]
    fn darken_decreases_color_values() {
        let base = Color::from_rgb(0.5, 0.5, 0.5);
        let darkened = darken(base, 0.2);
        assert!(darkened.r < base.r);
        assert!(darkened.g < base.g);
        assert!(darkened.b < base.b);
    }

    #[test]
    fn darken_clamps_at_0() {
        let base = Color::from_rgb(0.1, 0.1, 0.1);
        let darkened = darken(base, 0.2);
        assert_eq!(darkened.r, 0.0);
        assert_eq!(darkened.g, 0.0);
        assert_eq!(darkened.b, 0.0);
    }

    #[test]
    fn strip_returns_style() {
        let style = strip(false);
        assert!(style.background.is_some());
        assert_eq!(style.border.width, 1.0);
    }

    #[test]
    fn strip_selected_has_thicker_border() {
        let unselected = strip(false);
        let selected = strip(true);
        assert!(selected.border.width > unselected.border.width);
    }

    #[test]
    fn bay_returns_style() {
        let style = bay();
        assert!(style.background.is_some());
        assert_eq!(style.border.width, 1.0);
    }

    #[test]
    fn readout_returns_style() {
        let style = readout();
        assert!(style.background.is_some());
        assert_eq!(style.border.width, 1.0);
    }
}
