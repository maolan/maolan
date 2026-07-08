use crate::message::{Message, ModulatorChange};
use crate::state::{Modulator, ModulatorRate, ModulatorShape, MusicalDivision};
use iced::{
    Alignment, Background, Border, Color, Element, Length,
    widget::{
        button, checkbox, column, container, mouse_area, pick_list, row, scrollable, slider, text,
        text_input,
    },
};

pub struct ModulatorsPane;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RateMode {
    Hz,
    Musical,
}

impl std::fmt::Display for RateMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hz => write!(f, "Hz"),
            Self::Musical => write!(f, "Musical"),
        }
    }
}

impl ModulatorsPane {
    pub fn view<'a>(
        modulators: &'a [Modulator],
        selected_id: Option<usize>,
    ) -> Element<'a, Message> {
        let header = row![
            text("Modulators").size(16),
            button("+").on_press(Message::ModulatorAdd),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let mut content = column![header].spacing(12);

        if modulators.is_empty() {
            content = content.push(text("No modulators. Press + to add one.").size(11));
        } else {
            for m in modulators {
                content = content.push(modulator_card(m, selected_id == Some(m.id)));
            }
        }

        container(
            column![scrollable(content).height(Length::Fill)]
                .spacing(10)
                .padding(12),
        )
        .style(|_theme| container::Style {
            border: Border {
                color: Color::from_rgba(0.34, 0.42, 0.56, 0.72),
                width: 1.0,
                ..Border::default()
            },
            ..crate::style::app_background()
        })
        .padding(12)
        .width(Length::Fixed(340.0))
        .height(Length::Fill)
        .into()
    }
}

fn modulator_card<'a>(m: &'a Modulator, selected: bool) -> Element<'a, Message> {
    let name_row = row![
        text_input("Name", &m.name)
            .on_input(|s| Message::ModulatorUpdate {
                id: m.id,
                change: ModulatorChange::Name(s),
            })
            .width(Length::Fill),
        checkbox(m.enabled).on_toggle(|v| Message::ModulatorUpdate {
            id: m.id,
            change: ModulatorChange::Enabled(v),
        }),
        button("×").on_press(Message::ModulatorRemove(m.id)),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let shape_row = row![
        text("Shape").size(11).width(Length::Fixed(50.0)),
        pick_list(
            vec![
                ModulatorShape::Sine,
                ModulatorShape::Triangle,
                ModulatorShape::Saw,
                ModulatorShape::Square,
                ModulatorShape::SampleHold,
            ],
            Some(m.shape),
            move |s| Message::ModulatorUpdate {
                id: m.id,
                change: ModulatorChange::Shape(s),
            },
        )
        .width(Length::Fill),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let rate_mode_row = row![
        text("Rate").size(11).width(Length::Fixed(50.0)),
        pick_list(
            vec![RateMode::Hz, RateMode::Musical],
            Some(match m.rate {
                ModulatorRate::Hz(_) => RateMode::Hz,
                ModulatorRate::Musical(_) => RateMode::Musical,
            }),
            move |mode| Message::ModulatorUpdate {
                id: m.id,
                change: match mode {
                    RateMode::Hz => ModulatorChange::Rate(ModulatorRate::Hz(match m.rate {
                        ModulatorRate::Hz(hz) => hz,
                        ModulatorRate::Musical(_) => 1.0,
                    })),
                    RateMode::Musical => {
                        ModulatorChange::Rate(ModulatorRate::Musical(MusicalDivision::Beat))
                    }
                },
            },
        )
        .width(Length::Fixed(90.0)),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let rate_value_row: Element<'a, Message> = match m.rate {
        ModulatorRate::Hz(rate_hz) => labeled_slider(
            "",
            0.01..=20.0,
            rate_hz,
            0.01,
            |v| Message::ModulatorUpdate {
                id: m.id,
                change: ModulatorChange::Rate(ModulatorRate::Hz(v)),
            },
            "Hz",
        ),
        ModulatorRate::Musical(division) => row![
            text("").size(11).width(Length::Fixed(50.0)),
            pick_list(MusicalDivision::ALL.to_vec(), Some(division), move |div| {
                Message::ModulatorUpdate {
                    id: m.id,
                    change: ModulatorChange::Rate(ModulatorRate::Musical(div)),
                }
            },)
            .width(Length::Fill),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .into(),
    };

    let phase_row = labeled_slider(
        "Phase",
        0.0..=1.0,
        m.phase,
        0.001,
        |v| Message::ModulatorUpdate {
            id: m.id,
            change: ModulatorChange::Phase(v),
        },
        "",
    );

    let card = container(
        column![
            name_row,
            shape_row,
            rate_mode_row,
            rate_value_row,
            phase_row,
        ]
        .spacing(8),
    )
    .style(move |_theme| container::Style {
        background: Some(Background::Color(Color::from_rgba(0.18, 0.2, 0.24, 0.6))),
        border: Border {
            color: if selected {
                Color::from_rgb(0.9, 0.75, 0.25)
            } else {
                Color::from_rgba(0.34, 0.42, 0.56, 0.72)
            },
            width: if selected { 2.0 } else { 1.0 },
            radius: 6.0.into(),
        },
        ..container::Style::default()
    })
    .padding(10);

    mouse_area(card)
        .on_press(Message::ModulatorSelect(Some(m.id)))
        .into()
}

fn labeled_slider<'a>(
    label: &'a str,
    range: std::ops::RangeInclusive<f32>,
    value: f32,
    step: f32,
    on_change: impl Fn(f32) -> Message + 'a,
    unit: &'a str,
) -> Element<'a, Message> {
    let value_text = if unit.is_empty() {
        format!("{:.3}", value)
    } else {
        format!("{:.2} {}", value, unit)
    };
    row![
        text(label).size(11).width(Length::Fixed(50.0)),
        slider(range, value, on_change)
            .step(step)
            .width(Length::Fill),
        text(value_text).size(11).width(Length::Fixed(70.0)),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}
