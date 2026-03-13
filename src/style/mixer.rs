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
