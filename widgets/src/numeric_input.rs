use iced::{
    Alignment, Background, Border, Color, Element, Length, Theme,
    widget::{button, column, container, row, text_input},
};
use iced_fonts::lucide::{chevron_down, chevron_up};
use std::{
    fmt::Display,
    ops::{Add, RangeInclusive, Sub},
    str::FromStr,
};

fn spinner_button_style(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    let active_bg = palette.primary.strong.color;
    let hovered_bg = palette.primary.base.color;
    let disabled_bg = Color {
        a: active_bg.a * 0.4,
        ..active_bg
    };
    let mut style = button::Style {
        background: Some(Background::Color(match status {
            button::Status::Hovered | button::Status::Pressed => hovered_bg,
            button::Status::Disabled => disabled_bg,
            _ => active_bg,
        })),
        text_color: match status {
            button::Status::Disabled => Color {
                a: palette.primary.strong.text.a * 0.45,
                ..palette.primary.strong.text
            },
            _ => palette.primary.strong.text,
        },
        ..button::Style::default()
    };
    style.border = Border {
        color: Color::from_rgba(0.0, 0.0, 0.0, 0.0),
        width: 1.0,
        radius: 3.0.into(),
    };
    style
}

fn shell_style(_theme: &Theme) -> container::Style {
    container::Style {
        text_color: Some(Color::from_rgb(0.92, 0.92, 0.92)),
        background: Some(Background::Color(Color::from_rgba(0.10, 0.10, 0.10, 1.0))),
        border: Border {
            color: Color::from_rgba(0.28, 0.28, 0.28, 1.0),
            width: 1.0,
            radius: 2.0.into(),
        },
        ..container::Style::default()
    }
}

fn input_style(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let mut style = text_input::default(theme, status);
    style.background = Background::Color(Color::TRANSPARENT);
    style.border = Border {
        color: Color::TRANSPARENT,
        width: 0.0,
        radius: 0.0.into(),
    };
    style
}

pub fn number_input<'a, T, Message>(
    value: &'a T,
    bounds: RangeInclusive<T>,
    on_change: impl Fn(T) -> Message + 'a + Copy,
) -> Element<'a, Message>
where
    T: Copy + Display + FromStr + PartialOrd + Add<Output = T> + Sub<Output = T> + From<u8> + 'a,
    Message: Clone + 'a,
{
    let min = *bounds.start();
    let max = *bounds.end();
    let current = *value;
    let step = T::from(1_u8);
    let dec_value = if current > min + step {
        current - step
    } else {
        min
    };
    let inc_value = if current < max - step {
        current + step
    } else {
        max
    };

    let input = text_input("", &current.to_string())
        .on_input(move |raw| {
            raw.parse::<T>()
                .map(|parsed| {
                    let clamped = if parsed < min {
                        min
                    } else if parsed > max {
                        max
                    } else {
                        parsed
                    };
                    on_change(clamped)
                })
                .unwrap_or_else(|_| on_change(current))
        })
        .style(input_style)
        .padding([5, 8])
        .width(Length::Fixed(72.0))
        .size(14);

    let decrement = button(
        container(chevron_down().size(14))
            .center_x(Length::Fill)
            .center_y(Length::Fill),
    )
    .style(spinner_button_style)
    .padding(0)
    .width(Length::Fixed(22.0))
    .height(Length::Fixed(15.0));
    let decrement = if current > min {
        decrement.on_press(on_change(dec_value))
    } else {
        decrement
    };

    let increment = button(
        container(chevron_up().size(14))
            .center_x(Length::Fill)
            .center_y(Length::Fill),
    )
    .style(spinner_button_style)
    .padding(0)
    .width(Length::Fixed(22.0))
    .height(Length::Fixed(15.0));
    let increment = if current < max {
        increment.on_press(on_change(inc_value))
    } else {
        increment
    };

    container(
        row![
            container(input)
                .width(Length::Fixed(72.0))
                .center_y(Length::Fixed(30.0)),
            column![increment, decrement]
                .spacing(0)
                .width(Length::Fixed(22.0))
                .align_x(Alignment::Center),
        ]
        .spacing(0)
        .align_y(Alignment::Center),
    )
    .style(shell_style)
    .into()
}

fn format_decimal_value(value: f32) -> String {
    let mut formatted = format!("{value:.3}");
    while formatted.contains('.') && formatted.ends_with('0') {
        formatted.pop();
    }
    if formatted.ends_with('.') {
        formatted.pop();
    }
    formatted
}

pub fn number_input_f32<'a, Message>(
    value: &'a str,
    bounds: RangeInclusive<f32>,
    step: f32,
    on_change: impl Fn(String) -> Message + 'a + Copy,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let min = *bounds.start();
    let max = *bounds.end();
    let parsed_current = value.trim().parse::<f32>().ok();
    let current = parsed_current.unwrap_or(min).clamp(min, max);
    let dec_value = (current - step).clamp(min, max);
    let inc_value = (current + step).clamp(min, max);

    let input = text_input("", value)
        .on_input(on_change)
        .style(input_style)
        .padding([5, 8])
        .width(Length::Fixed(72.0))
        .size(14);

    let decrement = button(
        container(chevron_down().size(14))
            .center_x(Length::Fill)
            .center_y(Length::Fill),
    )
    .style(spinner_button_style)
    .padding(0)
    .width(Length::Fixed(22.0))
    .height(Length::Fixed(15.0));
    let decrement = if current > min {
        decrement.on_press(on_change(format_decimal_value(dec_value)))
    } else {
        decrement
    };

    let increment = button(
        container(chevron_up().size(14))
            .center_x(Length::Fill)
            .center_y(Length::Fill),
    )
    .style(spinner_button_style)
    .padding(0)
    .width(Length::Fixed(22.0))
    .height(Length::Fixed(15.0));
    let increment = if current < max {
        increment.on_press(on_change(format_decimal_value(inc_value)))
    } else {
        increment
    };

    container(
        row![
            container(input)
                .width(Length::Fixed(72.0))
                .center_y(Length::Fixed(30.0)),
            column![increment, decrement]
                .spacing(0)
                .width(Length::Fixed(22.0))
                .align_x(Alignment::Center),
        ]
        .spacing(0)
        .align_y(Alignment::Center),
    )
    .style(shell_style)
    .into()
}

#[cfg(test)]
mod tests {
    use super::format_decimal_value;

    #[test]
    fn format_decimal_value_trims_trailing_zeroes() {
        assert_eq!(format_decimal_value(6.1), "6.1");
        assert_eq!(format_decimal_value(6.0), "6");
        assert_eq!(format_decimal_value(6.125), "6.125");
    }
}
