use crate::{
    consts::{
        state_ids::METRONOME_TRACK_ID,
        workspace::{TICK_LABELS, TICK_VALUES},
        workspace_mixer::*,
    },
    message::Message,
    state::State,
    style,
    widget::{horizontal_slider::HorizontalSlider, slider::Slider},
};
use iced::{
    Alignment, Background, Color, Element, Length, Point, Rectangle, Renderer, Theme, mouse,
    widget::{
        Space, Stack, canvas,
        canvas::{Geometry, Path},
        column, container, lazy, mouse_area, pin, row, scrollable, text, text_input,
    },
};
use maolan_engine::message::Action;
use std::sync::Arc;

#[derive(Debug, Default)]
pub struct Mixer {
    state: State,
}

#[derive(Clone)]
struct VuMeterCanvas {
    channels: usize,
    levels_qdb: Arc<[u8]>,
    meter_height: f32,
}

struct StripReadout<'a> {
    track_name: String,
    editing: bool,
    edit_input: &'a str,
    level_label: &'static str,
}

impl canvas::Program<Message> for VuMeterCanvas {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        if bounds.width <= 0.0 || bounds.height <= 0.0 {
            return vec![];
        }
        let channels = self.channels.max(1);
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let bar_w = METER_BAR_WIDTH;
        let bar_gap = METER_BAR_GAP;
        let inner_h = self.meter_height.max(1.0);
        for channel_idx in 0..channels {
            let q = self.levels_qdb.get(channel_idx).copied().unwrap_or(0);
            let db = Mixer::qdb_to_level(q);
            let fill = Mixer::level_to_meter_fill(db);
            let filled_h = (inner_h * fill).max(1.0);
            let y = (inner_h - filled_h).max(0.0);
            let x = channel_idx as f32 * (bar_w + bar_gap);
            frame.fill(
                &Path::rectangle(Point::new(x, y), iced::Size::new(bar_w, filled_h)),
                Mixer::meter_fill_color(db),
            );
        }
        vec![frame.into_geometry()]
    }
}

impl Mixer {
    pub fn new(state: State) -> Self {
        Self { state }
    }

    fn level_to_meter_fill(level_db: f32) -> f32 {
        ((level_db - FADER_MIN_DB) / (FADER_MAX_DB - FADER_MIN_DB)).clamp(0.0, 1.0)
    }

    fn fader_height_from_panel(height: Length) -> f32 {
        match height {
            Length::Fixed(panel_h) => (panel_h - 146.0).max(92.0),
            _ => 160.0,
        }
    }

    fn db_to_y(db: f32, fader_height: f32) -> f32 {
        let normalized = ((db - FADER_MIN_DB) / (FADER_MAX_DB - FADER_MIN_DB)).clamp(0.0, 1.0);
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
        let usable_width = (width - (STRIP_NAME_SIDE_PADDING * 2.0)).max(0.0);
        let max_chars = (usable_width / STRIP_NAME_CHAR_PX).floor() as usize;
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
        Self::meter_total_width(channels)
    }

    fn meter_inner_width(channels: usize) -> f32 {
        let channels = channels.max(1);
        channels as f32 * METER_BAR_WIDTH + (channels.saturating_sub(1) as f32 * METER_BAR_GAP)
    }

    fn meter_total_width(channels: usize) -> f32 {
        Self::meter_inner_width(channels) + (METER_PAD_X * 2.0)
    }

    fn tick_scale_cached(fader_height: f32) -> Element<'static, Message> {
        let dep = (fader_height * 10.0).round().clamp(0.0, u16::MAX as f32) as u16;
        lazy(dep, move |dep_h| -> Element<'static, Message> {
            let fader_height = (*dep_h as f32) / 10.0;
            let tick_layout = Self::tick_layout(fader_height);
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

            Stack::from_vec(marks)
                .width(Length::Fixed(SCALE_WIDTH))
                .height(Length::Fixed(fader_height))
                .into()
        })
        .into()
    }

    fn slider_with_ticks<F>(
        value: f32,
        fader_height: f32,
        on_change: F,
    ) -> Element<'static, Message>
    where
        F: Fn(f32) -> Message + 'static,
    {
        row![
            Slider::new(FADER_MIN_DB..=FADER_MAX_DB, value, on_change)
                .width(Length::Fixed(FADER_WIDTH))
                .height(Length::Fixed(fader_height)),
            Self::tick_scale_cached(fader_height),
        ]
        .spacing(3)
        .align_y(Alignment::End)
        .into()
    }

    fn slider_plain<F>(value: f32, fader_height: f32, on_change: F) -> Element<'static, Message>
    where
        F: Fn(f32) -> Message + 'static,
    {
        Slider::new(FADER_MIN_DB..=FADER_MAX_DB, value, on_change)
            .width(Length::Fixed(FADER_WIDTH))
            .height(Length::Fixed(fader_height))
            .into()
    }

    fn balance_slider<F>(value: f32, on_change: F) -> Element<'static, Message>
    where
        F: Fn(f32) -> Message + 'static,
    {
        HorizontalSlider::new(-1.0..=1.0, value.clamp(-1.0, 1.0), on_change)
            .width(Length::Fixed(PAN_SLIDER_WIDTH))
            .height(Length::Fixed(PAN_ROW_HEIGHT))
            .into()
    }

    fn format_level_db(level: f32) -> &'static str {
        if level <= FADER_MIN_DB {
            "-inf dB"
        } else {
            let clamped = level.clamp(FADER_MIN_DB, FADER_MAX_DB);
            let idx = ((clamped - FADER_MIN_DB) * 10.0).round() as usize;
            LEVEL_LABELS[idx.min(LEVEL_LABELS.len() - 1)]
        }
    }

    fn format_balance(balance: f32) -> &'static str {
        let b = balance.clamp(-1.0, 1.0);
        let idx = ((b * 100.0).round() as i32 + 100).clamp(0, 200) as usize;
        BALANCE_LABELS[idx]
    }

    fn value_pill<'a>(
        track_name: String,
        content: &'static str,
        editing: bool,
        edit_input: &'a str,
    ) -> Element<'a, Message> {
        if editing {
            container(
                text_input("dB", edit_input)
                    .on_input(Message::MixerLevelEditInput)
                    .on_submit(Message::MixerLevelEditCommit)
                    .padding([2, 4])
                    .size(11),
            )
            .width(Length::Fixed(READOUT_WIDTH))
            .style(|_theme| style::mixer::readout())
            .into()
        } else {
            mouse_area(
                container(text(content).size(11))
                    .width(Length::Fixed(READOUT_WIDTH))
                    .padding([4, 6])
                    .align_x(Alignment::Center)
                    .style(|_theme| style::mixer::readout()),
            )
            .on_press(Message::MixerLevelEditStart(track_name))
            .into()
        }
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

    fn quantized_db_tenths(level: f32) -> i16 {
        (level.clamp(FADER_MIN_DB, FADER_MAX_DB) * 10.0).round() as i16
    }

    fn quantized_balance_hundredths(balance: f32) -> i16 {
        (balance.clamp(-1.0, 1.0) * 100.0).round() as i16
    }

    fn quantized_height_tenths(height: f32) -> u16 {
        (height.max(0.0) * 10.0).round().clamp(0.0, u16::MAX as f32) as u16
    }

    fn level_to_qdb(level_db: f32) -> u8 {
        (level_db
            .clamp(FADER_MIN_DB, FADER_MAX_DB)
            .round()
            .max(FADER_MIN_DB) as i16)
            .saturating_add(90)
            .clamp(0, 110) as u8
    }

    fn qdb_to_level(q: u8) -> f32 {
        q as f32 - 90.0
    }

    fn quantized_meter_levels(levels_db: &[f32], channels: usize) -> Arc<[u8]> {
        (0..channels.max(1))
            .map(|idx| Self::level_to_qdb(levels_db.get(idx).copied().unwrap_or(-90.0)))
            .collect::<Vec<_>>()
            .into()
    }

    fn pan_section_cached(track_name: String, value: f32) -> Element<'static, Message> {
        let dep = (track_name, Self::quantized_balance_hundredths(value));
        lazy(
            dep,
            move |(track_name, value_hundredths)| -> Element<'static, Message> {
                let value = (*value_hundredths as f32) / 100.0;
                Self::pan_section(value, {
                    let track_name = track_name.clone();
                    move |new_val| {
                        Message::Request(Action::TrackBalance(track_name.clone(), new_val))
                    }
                })
            },
        )
        .into()
    }

    fn slider_cached(
        track_name: String,
        value: f32,
        fader_height: f32,
        show_ticks: bool,
    ) -> Element<'static, Message> {
        let dep = (
            track_name,
            Self::quantized_db_tenths(value),
            Self::quantized_height_tenths(fader_height),
            show_ticks,
        );
        lazy(
            dep,
            move |(track_name, value_tenths, height_tenths, show_ticks)| -> Element<'static, Message> {
                let value = (*value_tenths as f32) / 10.0;
                let fader_height = (*height_tenths as f32) / 10.0;
                if *show_ticks {
                    Self::slider_with_ticks(value, fader_height, {
                        let track_name = track_name.clone();
                        move |new_val| Message::Request(Action::TrackLevel(track_name.clone(), new_val))
                    })
                } else {
                    Self::slider_plain(value, fader_height, {
                        let track_name = track_name.clone();
                        move |new_val| Message::Request(Action::TrackLevel(track_name.clone(), new_val))
                    })
                }
            },
        )
        .into()
    }

    fn pan_placeholder() -> Element<'static, Message> {
        Space::new()
            .width(Length::Fill)
            .height(Length::Fixed(PAN_ROW_HEIGHT))
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
        let inner_h = (meter_h - (METER_PAD_Y * 2.0)).max(1.0);
        let q_levels = Self::quantized_meter_levels(levels_db, channels);
        let dep = (
            channels as u16,
            Self::quantized_height_tenths(inner_h),
            q_levels,
        );
        let meter_canvas: Element<'static, Message> = lazy(
            dep,
            |(channels, inner_h_tenths, q_levels)| -> Element<'static, Message> {
                let channels = *channels as usize;
                let inner_h = *inner_h_tenths as f32 / 10.0;
                canvas(VuMeterCanvas {
                    channels,
                    levels_qdb: q_levels.clone(),
                    meter_height: inner_h,
                })
                .width(Length::Fixed(Self::meter_inner_width(channels)))
                .height(Length::Fixed(inner_h))
                .into()
            },
        )
        .into();
        container(meter_canvas)
            .width(Length::Fixed(Self::meter_total_width(channels)))
            .height(Length::Fixed(meter_h))
            .padding([METER_PAD_Y as u16, METER_PAD_X as u16])
            .style(|_theme| style::mixer::meter())
            .into()
    }

    fn fader_bay(
        track_name: String,
        channels: usize,
        levels_db: &[f32],
        value: f32,
        fader_height: f32,
        show_ticks: bool,
    ) -> Element<'static, Message> {
        let slider = Self::slider_cached(track_name, value, fader_height, show_ticks);
        container(
            row![slider, Self::vu_meter(channels, levels_db, fader_height),]
                .spacing(8)
                .align_y(Alignment::End),
        )
        .width(Length::Fill)
        .padding([8, 7])
        .style(|_theme| style::mixer::bay())
        .into()
    }

    fn strip_shell<'a>(
        name: String,
        selected: bool,
        width: f32,
        pan_section: Option<Element<'static, Message>>,
        bay: Element<'static, Message>,
        readout: StripReadout<'a>,
    ) -> Element<'a, Message> {
        let mut content = column![].spacing(8).width(Length::Fill);
        content = content.push(pan_section.unwrap_or_else(Self::pan_placeholder));
        content = content.push(bay).push(Self::value_pill(
            readout.track_name,
            readout.level_label,
            readout.editing,
            readout.edit_input,
        ));
        content = content.push(Self::strip_name(name, width));

        container(content)
            .width(Length::Fixed(width))
            .height(Length::Fill)
            .padding([8, 6])
            .style(move |_theme| style::mixer::strip(selected))
            .into()
    }

    pub fn view<'a>(
        &'a self,
        editing_track: Option<&'a str>,
        editing_input: &'a str,
    ) -> Element<'a, Message> {
        let mut strips = row![].spacing(2).align_y(Alignment::Start);
        let state = self.state.blocking_read();
        let height = state.mixer_height;
        let hw_out_channels = state.hw_out.as_ref().map(|hw| hw.channels).unwrap_or(0);
        let hw_out_level = state.hw_out_level;
        let hw_out_balance = state.hw_out_balance;
        let master_selected = state.selected.contains("hw:out");
        let fader_height = Self::fader_height_from_panel(height);
        let metronome_enabled = state.metronome_enabled;
        let mut metronome_strip = None;

        for track in &state.tracks {
            if track.name == METRONOME_TRACK_ID && !metronome_enabled {
                continue;
            }
            let strip_name = track.name.clone();
            let select_name = track.name.clone();
            let pan = if track.audio.outs == 2 {
                Some(Self::pan_section_cached(track.name.clone(), track.balance))
            } else {
                None
            };
            let bay = Self::fader_bay(
                track.name.clone(),
                track.audio.outs,
                &track.meter_out_db,
                track.level,
                fader_height,
                true,
            );
            let strip = mouse_area(Self::strip_shell(
                strip_name,
                state.selected.contains(track.name.as_str()),
                STRIP_WIDTH,
                pan,
                bay,
                StripReadout {
                    track_name: track.name.clone(),
                    editing: editing_track == Some(track.name.as_str()),
                    edit_input: editing_input,
                    level_label: Self::format_level_db(track.level),
                },
            ))
            .on_press(Message::SelectTrackFromMixer(select_name));

            if track.name == METRONOME_TRACK_ID {
                metronome_strip = Some(strip);
            } else {
                strips = strips.push(strip);
            }
        }

        let master_strip_width = (FADER_WIDTH
            + SCALE_WIDTH
            + 3.0
            + 8.0
            + Self::master_meter_width(hw_out_channels.max(1))
            + 16.0)
            .max(STRIP_WIDTH);
        let master_strip = mouse_area(Self::strip_shell(
            "Master".to_string(),
            master_selected,
            master_strip_width,
            if hw_out_channels == 2 {
                Some(Self::pan_section_cached(
                    "hw:out".to_string(),
                    hw_out_balance,
                ))
            } else {
                None
            },
            Self::fader_bay(
                "hw:out".to_string(),
                hw_out_channels.max(1),
                &state.hw_out_meter_db,
                hw_out_level,
                fader_height,
                true,
            ),
            StripReadout {
                track_name: "hw:out".to_string(),
                editing: editing_track == Some("hw:out"),
                edit_input: editing_input,
                level_label: Self::format_level_db(hw_out_level),
            },
        ))
        .on_press(Message::SelectTrackFromMixer("hw:out".to_string()));
        let mut output_strips = row![].spacing(2).align_y(Alignment::Start);
        if let Some(strip) = metronome_strip {
            output_strips = output_strips.push(strip);
        }
        output_strips = output_strips.push(master_strip);

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

        container(
            mouse_area(
                row![track_strips, output_strips]
                    .height(height)
                    .align_y(Alignment::Start),
            )
            .on_press(Message::DeselectAll),
        )
        .style(|_theme| crate::style::app_background())
        .width(Length::Fill)
        .height(height)
        .into()
    }
}
