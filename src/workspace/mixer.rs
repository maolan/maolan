use crate::{
    consts::{
        state_ids::METRONOME_TRACK_ID,
        workspace::{TICK_LABELS, TICK_VALUES},
        workspace_mixer::*,
    },
    message::Message,
    state::State,
    style,
    ui_timing::DOUBLE_CLICK,
    widget::horizontal_slider::HorizontalSlider,
};
use iced::{
    Alignment, Color, Element, Length, Point, Rectangle, Renderer, Theme, event::Event, mouse,
    widget::{
        Space, canvas,
        canvas::{Action as CanvasAction, Frame, Geometry, Path, Stroke, Text},
        column, container, lazy, mouse_area, row, scrollable, text, text_input,
    },
};
use maolan_engine::message::Action;
use std::{sync::Arc, time::Instant};

#[derive(Debug, Default)]
pub struct Mixer {
    state: State,
}

struct StripReadout<'a> {
    track_name: String,
    editing: bool,
    edit_input: &'a str,
    level_label: &'static str,
}

#[derive(Default)]
struct FaderBayState {
    dragging: bool,
    last_click_at: Option<Instant>,
}

#[derive(Clone)]
struct FaderBayCanvas {
    track_name: String,
    channels: usize,
    levels_qdb: Arc<[u8]>,
    value: f32,
    show_ticks: bool,
    fader_height: f32,
}

impl FaderBayCanvas {
    const OUTER_PAD_X: f32 = 8.0;
    const OUTER_PAD_Y: f32 = 7.0;
    const INNER_GAP: f32 = 8.0;
    const SCALE_GAP: f32 = 3.0;

    fn slider_block_width(&self) -> f32 {
        if self.show_ticks {
            FADER_WIDTH + Self::SCALE_GAP + SCALE_WIDTH
        } else {
            FADER_WIDTH
        }
    }

    fn meter_block_width(&self) -> f32 {
        Mixer::meter_total_width(self.channels)
    }

    fn slider_bounds(&self, bounds: Rectangle) -> Rectangle {
        Rectangle {
            x: bounds.x + Self::OUTER_PAD_X,
            y: bounds.y + Self::OUTER_PAD_Y,
            width: FADER_WIDTH,
            height: self.fader_height,
        }
    }

    fn value_from_cursor(&self, cursor_position: Point, bounds: Rectangle) -> f32 {
        let slider = self.slider_bounds(bounds);
        let y = cursor_position.y - slider.y;
        let normalized = 1.0 - (y / slider.height.max(1.0)).clamp(0.0, 1.0);
        let value = FADER_MIN_DB + normalized * (FADER_MAX_DB - FADER_MIN_DB);
        value.clamp(FADER_MIN_DB, FADER_MAX_DB)
    }
}

impl canvas::Program<Message> for FaderBayCanvas {
    type State = FaderBayState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<CanvasAction<Message>> {
        let slider_bounds = self.slider_bounds(bounds);
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if cursor.is_over(slider_bounds) {
                    let now = Instant::now();
                    let is_double_click = state
                        .last_click_at
                        .is_some_and(|last| now.duration_since(last) <= DOUBLE_CLICK);
                    state.last_click_at = Some(now);
                    state.dragging = true;
                    if is_double_click {
                        return Some(CanvasAction::publish(Message::Request(Action::TrackLevel(
                            self.track_name.clone(),
                            0.0,
                        ))));
                    }
                    if let Some(cursor_position) = cursor.position() {
                        return Some(CanvasAction::publish(Message::Request(Action::TrackLevel(
                            self.track_name.clone(),
                            self.value_from_cursor(cursor_position, bounds),
                        ))));
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.dragging = false;
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if state.dragging
                    && let Some(cursor_position) = cursor.position()
                {
                    return Some(CanvasAction::publish(Message::Request(Action::TrackLevel(
                        self.track_name.clone(),
                        self.value_from_cursor(cursor_position, bounds),
                    ))));
                }
            }
            _ => {}
        }
        None
    }

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

        let slider_bounds = self.slider_bounds(bounds);
        let meter_x = bounds.x + Self::OUTER_PAD_X + self.slider_block_width() + Self::INNER_GAP;
        let meter_y = bounds.y + Self::OUTER_PAD_Y;
        let meter_inner_h = self.fader_height.max(1.0);
        let meter_total_w = self.meter_block_width();

        let mut frame = Frame::new(renderer, bounds.size());

        let local = |x: f32, y: f32| Point::new(x - bounds.x, y - bounds.y);

        let back_color = Color::from_rgb(0x42 as f32 / 255.0, 0x46 as f32 / 255.0, 0x4D as f32 / 255.0);
        let border_color = Color::from_rgb(0x30 as f32 / 255.0, 0x33 as f32 / 255.0, 0x3C as f32 / 255.0);
        let filled_color = Color::from_rgb(0x29 as f32 / 255.0, 0x66 as f32 / 255.0, 0xA3 as f32 / 255.0);
        let handle_color = Color::from_rgb(0x75 as f32 / 255.0, 0xC2 as f32 / 255.0, 0xFF as f32 / 255.0);
        let border_width = 1.0;
        let handle_height = 2.0;
        let normalized = (self.value - FADER_MIN_DB) / (FADER_MAX_DB - FADER_MIN_DB);
        let handle_y = slider_bounds.y
            + ((slider_bounds.height - handle_height - border_width * 2.0) * (1.0 - normalized)).round();

        frame.fill(
            &Path::rectangle(
                local(slider_bounds.x, slider_bounds.y),
                iced::Size::new(slider_bounds.width, slider_bounds.height),
            ),
            back_color,
        );
        frame.stroke(
            &Path::rectangle(
                local(slider_bounds.x, slider_bounds.y),
                iced::Size::new(slider_bounds.width, slider_bounds.height),
            ),
            Stroke::default().with_width(border_width).with_color(border_color),
        );

        let filled_y = handle_y + handle_height + 1.0;
        let filled_h = (slider_bounds.y + slider_bounds.height - filled_y).max(0.0);
        if filled_h > 0.0 {
            frame.fill(
                &Path::rectangle(
                    local(slider_bounds.x, filled_y),
                    iced::Size::new(slider_bounds.width, filled_h),
                ),
                filled_color,
            );
        }
        frame.fill(
            &Path::rectangle(
                local(slider_bounds.x, handle_y),
                iced::Size::new(slider_bounds.width, handle_height + border_width * 2.0),
            ),
            handle_color,
        );

        if self.show_ticks {
            let tick_x = slider_bounds.x + FADER_WIDTH + Self::SCALE_GAP;
            for (label_y, label) in Mixer::tick_layout(self.fader_height) {
                frame.fill(
                    &Path::rectangle(
                        local(tick_x, bounds.y + Self::OUTER_PAD_Y + label_y + 4.0),
                        iced::Size::new(4.0, 1.0),
                    ),
                    Color::from_rgba(0.62, 0.67, 0.77, 0.78),
                );
                frame.fill_text(Text {
                    content: label.to_string(),
                    position: local(tick_x + 6.0, bounds.y + Self::OUTER_PAD_Y + label_y),
                    color: Color::from_rgba(0.9, 0.92, 0.96, 0.9),
                    size: 8.0.into(),
                    ..Default::default()
                });
            }
        }

        frame.fill(
            &Path::rectangle(
                local(meter_x, meter_y),
                iced::Size::new(meter_total_w, self.fader_height),
            ),
            Color::from_rgba(0.09, 0.10, 0.12, 1.0),
        );

        let bar_w = METER_BAR_WIDTH;
        let bar_gap = METER_BAR_GAP;
        for channel_idx in 0..self.channels.max(1) {
            let q = self.levels_qdb.get(channel_idx).copied().unwrap_or(0);
            let db = Mixer::qdb_to_level(q);
            let fill = Mixer::level_to_meter_fill(db);
            let filled_h = (meter_inner_h * fill).max(1.0);
            let y = (meter_inner_h - filled_h).max(0.0);
            let x = meter_x + METER_PAD_X + channel_idx as f32 * (bar_w + bar_gap);
            frame.fill(
                &Path::rectangle(
                    local(x, meter_y + METER_PAD_Y + y),
                    iced::Size::new(bar_w, (filled_h - METER_PAD_Y).max(1.0)),
                ),
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

    fn strip_name_cached(name: String, width: f32) -> Element<'static, Message> {
        let dep = (
            name,
            (width.max(0.0) * 10.0).round().clamp(0.0, u16::MAX as f32) as u16,
        );
        lazy(dep, |(name, width_tenths)| -> Element<'static, Message> {
            Self::strip_name(name.clone(), *width_tenths as f32 / 10.0)
        })
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

    fn value_pill_cached<'a>(
        track_name: String,
        content: &'static str,
        editing: bool,
        edit_input: &'a str,
    ) -> Element<'a, Message> {
        if editing {
            return Self::value_pill(track_name, content, true, edit_input);
        }
        let dep = (track_name, content);
        lazy(dep, |(track_name, content)| -> Element<'static, Message> {
            mouse_area(
                container(text(*content).size(11))
                    .width(Length::Fixed(READOUT_WIDTH))
                    .padding([4, 6])
                    .align_x(Alignment::Center)
                    .style(|_theme| style::mixer::readout()),
            )
            .on_press(Message::MixerLevelEditStart(track_name.clone()))
            .into()
        })
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

    fn quantized_balance_hundredths(balance: f32) -> i16 {
        (balance.clamp(-1.0, 1.0) * 100.0).round() as i16
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

    fn fader_bay(
        track_name: String,
        channels: usize,
        levels_db: &[f32],
        value: f32,
        fader_height: f32,
        show_ticks: bool,
    ) -> Element<'static, Message> {
        let inner_width = if show_ticks {
            FADER_WIDTH + 3.0 + SCALE_WIDTH
        } else {
            FADER_WIDTH
        } + 8.0
            + Self::meter_total_width(channels.max(1));
        let total_width = inner_width + (FaderBayCanvas::OUTER_PAD_X * 2.0);
        container(
            canvas(FaderBayCanvas {
                track_name,
                channels: channels.max(1),
                levels_qdb: Self::quantized_meter_levels(levels_db, channels.max(1)),
                value,
                show_ticks,
                fader_height,
            })
            .width(Length::Fixed(total_width))
            .height(Length::Fixed(fader_height + 14.0)),
        )
        .width(Length::Fill)
        .style(|_theme| style::mixer::bay())
        .into()
    }

    fn fader_bay_cached(
        track_name: String,
        channels: usize,
        levels_db: &[f32],
        value: f32,
        fader_height: f32,
        show_ticks: bool,
    ) -> Element<'static, Message> {
        Self::fader_bay(track_name, channels, levels_db, value, fader_height, show_ticks)
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
        content = content.push(bay).push(Self::value_pill_cached(
            readout.track_name,
            readout.level_label,
            readout.editing,
            readout.edit_input,
        ));
        content = content.push(Self::strip_name_cached(name, width));

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
            let bay = Self::fader_bay_cached(
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
            Self::fader_bay_cached(
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
