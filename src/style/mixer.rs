use iced::{Background, Border, Color, widget::container};

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
}

pub fn strip(selected: bool) -> container::Style {
    let border_color = if selected {
        rgb(114, 170, 240)
    } else {
        rgb(39, 47, 63)
    };

    container::Style {
        background: Some(Background::Color(rgb(26, 32, 46))),
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

pub fn meter() -> container::Style {
    container::Style {
        background: Some(Background::Color(rgb(12, 16, 25))),
        border: Border {
            color: rgb(52, 60, 74),
            width: 1.0,
            radius: 4.0.into(),
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
