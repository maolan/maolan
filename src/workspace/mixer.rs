use crate::{
    message::Message,
    state::State,
    style,
    widget::{horizontal_slider::HorizontalSlider, slider::Slider},
};
use iced::{
    Alignment, Background, Color, Element, Length, Point,
    widget::{Space, Stack, column, container, mouse_area, pin, row, scrollable, text},
};
use maolan_engine::message::Action;
use std::sync::LazyLock;

#[derive(Debug, Default)]
pub struct Mixer {
    state: State,
}

const TICK_VALUES: [f32; 13] = [
    20.0, 12.0, 6.0, 0.0, -6.0, -12.0, -18.0, -24.0, -36.0, -48.0, -60.0, -72.0, -90.0,
];
const TICK_LABELS: [&str; 13] = [
    "+20", "+12", "+6", "0", "-6", "-12", "-18", "-24", "-36", "-48", "-60", "-72", "-90",
];

static LEVEL_LABELS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    let mut labels = Vec::with_capacity(1101);
    for i in 0..=1100 {
        let level = -90.0 + (i as f32) * 0.1;
        let s: &'static str = Box::leak(format!("{:+.1} dB", level).into_boxed_str());
        labels.push(s);
    }
    labels
});

static BALANCE_LABELS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    let mut labels = Vec::with_capacity(201);
    for i in -100..=100 {
        let s: &'static str = if i == 0 {
            "C"
        } else if i < 0 {
            Box::leak(format!("L{}", -i).into_boxed_str())
        } else {
            Box::leak(format!("R{}", i).into_boxed_str())
        };
        labels.push(s);
    }
    labels
});

impl Mixer {
    const FADER_MIN_DB: f32 = -90.0;
    const FADER_MAX_DB: f32 = 20.0;
    const STRIP_WIDTH: f32 = 96.0;
    const PAN_SLIDER_WIDTH: f32 = 52.0;
    const READOUT_WIDTH: f32 = 72.0;
    const FADER_WIDTH: f32 = 14.0;
    const SCALE_WIDTH: f32 = 22.0;
    const PAN_ROW_HEIGHT: f32 = 12.0;
    const STRIP_NAME_CHAR_PX: f32 = 6.3;
    const STRIP_NAME_SIDE_PADDING: f32 = 4.0;

    pub fn new(state: State) -> Self {
        Self { state }
    }

    fn level_to_meter_fill(level_db: f32) -> f32 {
        ((level_db - Self::FADER_MIN_DB) / (Self::FADER_MAX_DB - Self::FADER_MIN_DB))
            .clamp(0.0, 1.0)
    }

    fn fader_height_from_panel(height: Length) -> f32 {
        match height {
            Length::Fixed(panel_h) => (panel_h - 146.0).max(92.0),
            _ => 160.0,
        }
    }

    fn db_to_y(db: f32, fader_height: f32) -> f32 {
        let normalized =
            ((db - Self::FADER_MIN_DB) / (Self::FADER_MAX_DB - Self::FADER_MIN_DB)).clamp(0.0, 1.0);
        fader_height * (1.0 - normalized)
    }

    fn tick_layout(fader_height: f32) -> Vec<(f32, &'static str)> {
        let mut out = Vec::with_capacity(TICK_VALUES.len());
        for (idx, db) in TICK_VALUES.iter().copied().enumerate() {
            let y = Self::db_to_y(db, fader_height).clamp(0.0, fader_height - 1.0);
            let label_y = (y - 4.0).clamp(0.0, (fader_height - 10.0).max(0.0));
            out.push((label_y, TICK_LABELS[idx]));
        }
        out
    }

    fn tick_mark() -> iced::widget::container::Style {
        iced::widget::container::Style {
            background: Some(Background::Color(Color {
                r: 0.62,
                g: 0.67,
                b: 0.77,
                a: 0.78,
            })),
            ..iced::widget::container::Style::default()
        }
    }

    fn trim_strip_name(name: &str, width: f32) -> String {
        let usable_width = (width - (Self::STRIP_NAME_SIDE_PADDING * 2.0)).max(0.0);
        let max_chars = (usable_width / Self::STRIP_NAME_CHAR_PX).floor() as usize;
        if max_chars == 0 {
            String::new()
        } else {
            name.chars().take(max_chars).collect()
        }
    }

    fn strip_name(name: String, width: f32) -> Element<'static, Message> {
        container(text(Self::trim_strip_name(&name, width)).size(12))
            .width(Length::Fill)
            .align_x(Alignment::Center)
            .padding([0, 2])
            .into()
    }

    fn master_meter_width(channels: usize) -> f32 {
        let channels = channels.max(1) as f32;
        6.0 + channels * 2.5
    }

    fn slider_with_ticks<F>(
        value: f32,
        fader_height: f32,
        tick_layout: &[(f32, &'static str)],
        on_change: F,
    ) -> Element<'static, Message>
    where
        F: Fn(f32) -> Message + 'static,
    {
        let mut marks: Vec<Element<'static, Message>> = Vec::with_capacity(tick_layout.len());
        for (label_y, label) in tick_layout.iter().copied() {
            marks.push(
                pin(row![
                    container("")
                        .width(Length::Fixed(4.0))
                        .height(Length::Fixed(1.0))
                        .style(|_theme| Self::tick_mark()),
                    text(label).size(8),
                ]
                .spacing(2)
                .align_y(Alignment::Center))
                .position(Point::new(0.0, label_y))
                .into(),
            );
        }

        let scale = Stack::from_vec(marks)
            .width(Length::Fixed(Self::SCALE_WIDTH))
            .height(Length::Fixed(fader_height));

        row![
            Slider::new(Self::FADER_MIN_DB..=Self::FADER_MAX_DB, value, on_change)
                .width(Length::Fixed(Self::FADER_WIDTH))
                .height(Length::Fixed(fader_height)),
            scale,
        ]
        .spacing(3)
        .align_y(Alignment::End)
        .into()
    }

    fn slider_plain<F>(value: f32, fader_height: f32, on_change: F) -> Element<'static, Message>
    where
        F: Fn(f32) -> Message + 'static,
    {
        Slider::new(Self::FADER_MIN_DB..=Self::FADER_MAX_DB, value, on_change)
            .width(Length::Fixed(Self::FADER_WIDTH))
            .height(Length::Fixed(fader_height))
            .into()
    }

    fn balance_slider<F>(value: f32, on_change: F) -> Element<'static, Message>
    where
        F: Fn(f32) -> Message + 'static,
    {
        HorizontalSlider::new(-1.0..=1.0, value.clamp(-1.0, 1.0), on_change)
            .width(Length::Fixed(Self::PAN_SLIDER_WIDTH))
            .height(Length::Fixed(Self::PAN_ROW_HEIGHT))
            .into()
    }

    fn format_level_db(level: f32) -> &'static str {
        if level <= Self::FADER_MIN_DB {
            "-inf dB"
        } else {
            let clamped = level.clamp(Self::FADER_MIN_DB, Self::FADER_MAX_DB);
            let idx = ((clamped - Self::FADER_MIN_DB) * 10.0).round() as usize;
            LEVEL_LABELS[idx.min(LEVEL_LABELS.len() - 1)]
        }
    }

    fn format_balance(balance: f32) -> &'static str {
        let b = balance.clamp(-1.0, 1.0);
        let idx = ((b * 100.0).round() as i32 + 100).clamp(0, 200) as usize;
        BALANCE_LABELS[idx]
    }

    fn value_pill(content: &'static str) -> Element<'static, Message> {
        container(text(content).size(11))
            .width(Length::Fixed(Self::READOUT_WIDTH))
            .padding([4, 6])
            .align_x(Alignment::Center)
            .style(|_theme| style::mixer::readout())
            .into()
    }

    fn pan_section<F>(value: f32, on_change: F) -> Element<'static, Message>
    where
        F: Fn(f32) -> Message + 'static,
    {
        row![
            container(text(Self::format_balance(value)).size(9))
                .width(Length::Fixed(24.0))
                .align_x(Alignment::Center),
            Self::balance_slider(value, on_change),
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .into()
    }

    fn pan_placeholder() -> Element<'static, Message> {
        Space::new()
            .width(Length::Fill)
            .height(Length::Fixed(Self::PAN_ROW_HEIGHT))
            .into()
    }

    fn meter_fill_color(level_db: f32) -> Color {
        if level_db >= 0.0 {
            Color::from_rgb(0.96, 0.47, 0.34)
        } else if level_db >= -12.0 {
            Color::from_rgb(0.69, 0.86, 0.41)
        } else {
            Color::from_rgb(0.20, 0.78, 0.51)
        }
    }

    fn vu_meter(channels: usize, levels_db: &[f32], meter_h: f32) -> Element<'static, Message> {
        let channels = channels.max(1);
        let mut strips = row![].spacing(2).align_y(Alignment::End);

        for channel_idx in 0..channels {
            let db = levels_db.get(channel_idx).copied().unwrap_or(-90.0);
            let fill = Self::level_to_meter_fill(db);
            let filled_h = (meter_h * fill).max(1.0);
            let empty_h = (meter_h - filled_h).max(0.0);
            strips = strips.push(
                column![
                    Space::new().height(Length::Fixed(empty_h)),
                    container("")
                        .width(Length::Fixed(3.0))
                        .height(Length::Fixed(filled_h))
                        .style(move |_theme| iced::widget::container::Style {
                            background: Some(Background::Color(Self::meter_fill_color(db))),
                            ..iced::widget::container::Style::default()
                        }),
                ]
                .spacing(0),
            );
        }

        container(strips)
            .padding([3, 3])
            .height(Length::Fixed(meter_h))
            .style(|_theme| style::mixer::meter())
            .into()
    }

    fn fader_bay<F>(
        channels: usize,
        levels_db: &[f32],
        value: f32,
        fader_height: f32,
        tick_layout: &[(f32, &'static str)],
        show_ticks: bool,
        on_change: F,
    ) -> Element<'static, Message>
    where
        F: Fn(f32) -> Message + 'static,
    {
        let slider: Element<'static, Message> = if show_ticks {
            Self::slider_with_ticks(value, fader_height, tick_layout, on_change)
        } else {
            Self::slider_plain(value, fader_height, on_change)
        };
        container(
            row![
                slider,
                Self::vu_meter(channels, levels_db, fader_height),
            ]
            .spacing(8)
            .align_y(Alignment::End),
        )
        .width(Length::Fill)
        .padding([8, 7])
        .style(|_theme| style::mixer::bay())
        .into()
    }

    fn strip_shell(
        name: String,
        selected: bool,
        width: f32,
        pan_section: Option<Element<'static, Message>>,
        bay: Element<'static, Message>,
        level_label: &'static str,
    ) -> Element<'static, Message> {
        let mut content = column![].spacing(8).width(Length::Fill);
        content = content.push(pan_section.unwrap_or_else(Self::pan_placeholder));
        content = content
            .push(bay)
            .push(Self::value_pill(level_label))
            .push(Self::strip_name(name, width));

        container(content)
            .width(Length::Fixed(width))
            .height(Length::Fill)
            .padding([8, 6])
            .style(move |_theme| style::mixer::strip(selected))
            .into()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let mut strips = row![].spacing(2).align_y(Alignment::Start);
        let state = self.state.blocking_read();
        let height = state.mixer_height;
        let hw_out_channels = state.hw_out.as_ref().map(|hw| hw.channels).unwrap_or(0);
        let hw_out_level = state.hw_out_level;
        let hw_out_balance = state.hw_out_balance;
        let master_selected = state.selected.contains("hw:out");
        let fader_height = Self::fader_height_from_panel(height);
        let tick_layout = Self::tick_layout(fader_height);

        for track in &state.tracks {
            let strip_name = track.name.clone();
            let select_name = track.name.clone();
            let pan = if track.audio.outs == 2 {
                Some(Self::pan_section(track.balance, {
                    let name = track.name.clone();
                    move |new_val| Message::Request(Action::TrackBalance(name.clone(), new_val))
                }))
            } else {
                None
            };
            let bay = Self::fader_bay(
                track.audio.outs,
                &track.meter_out_db,
                track.level,
                fader_height,
                &tick_layout,
                false,
                {
                    let name = track.name.clone();
                    move |new_val| Message::Request(Action::TrackLevel(name.clone(), new_val))
                },
            );
            strips = strips.push(
                mouse_area(Self::strip_shell(
                    strip_name,
                    state.selected.contains(track.name.as_str()),
                    Self::STRIP_WIDTH,
                    pan,
                    bay,
                    Self::format_level_db(track.level),
                ))
                .on_press(Message::SelectTrackFromMixer(select_name)),
            );
        }

        let master_strip_width = (Self::FADER_WIDTH
            + Self::SCALE_WIDTH
            + 3.0
            + 8.0
            + Self::master_meter_width(hw_out_channels.max(1))
            + 16.0)
            .max(Self::STRIP_WIDTH);
        let master_strip = mouse_area(Self::strip_shell(
            "Master".to_string(),
            master_selected,
            master_strip_width,
            if hw_out_channels == 2 {
                Some(Self::pan_section(hw_out_balance, move |new_val| {
                    Message::Request(Action::TrackBalance("hw:out".to_string(), new_val))
                }))
            } else {
                None
            },
            Self::fader_bay(
                hw_out_channels.max(1),
                &state.hw_out_meter_db,
                hw_out_level,
                fader_height,
                &tick_layout,
                true,
                move |new_val| Message::Request(Action::TrackLevel("hw:out".to_string(), new_val)),
            ),
            Self::format_level_db(hw_out_level),
        ))
        .on_press(Message::SelectTrackFromMixer("hw:out".to_string()));

        let track_strips = scrollable(
            row![strips, Space::new().width(Length::Fill)]
                .height(height)
                .padding([8, 6])
                .align_y(Alignment::Start),
        )
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new(),
        ))
        .width(Length::Fill)
        .height(height);

        mouse_area(
            row![track_strips, master_strip]
                .height(height)
                .align_y(Alignment::Start),
        )
        .on_press(Message::DeselectAll)
        .into()
    }
}
