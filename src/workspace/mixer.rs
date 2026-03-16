use crate::{
    consts::{
        state_ids::METRONOME_TRACK_ID,
        workspace::{TICK_LABELS, TICK_VALUES},
        workspace_mixer::*,
    },
    message::Message,
    state::{State, Track},
    style,
    ui_timing::DOUBLE_CLICK,
};
use iced::{
    Alignment, Color, Element, Length, Point, Rectangle, Renderer, Theme,
    event::Event,
    mouse,
    widget::{
        Space, canvas,
        canvas::{Action as CanvasAction, Frame, Geometry, Path, Stroke, Text},
        column, container, lazy, mouse_area, row, scrollable, text, text_input,
    },
};
use maolan_engine::message::Action;
use std::{
    cell::Cell,
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    time::Instant,
};

const STRIP_SPACING: f32 = 2.0;
const STRIP_ROW_PADDING_X: f32 = 8.0;
const MIXER_OVERSCAN_PX: f32 = 160.0;

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

struct TrackStripSpec<'a> {
    track: &'a Track,
    width: f32,
}

struct FaderBayState {
    dragging: bool,
    last_click_at: Option<Instant>,
    cache: canvas::Cache,
    last_hash: Cell<u64>,
}

impl Default for FaderBayState {
    fn default() -> Self {
        Self {
            dragging: false,
            last_click_at: None,
            cache: canvas::Cache::default(),
            last_hash: Cell::new(0),
        }
    }
}

struct PanState {
    dragging: bool,
    last_click_at: Option<Instant>,
    cache: canvas::Cache,
    last_hash: Cell<u64>,
}

impl Default for PanState {
    fn default() -> Self {
        Self {
            dragging: false,
            last_click_at: None,
            cache: canvas::Cache::default(),
            last_hash: Cell::new(0),
        }
    }
}

#[derive(Clone)]
struct PanCanvas {
    track_name: String,
    value: f32,
}

#[derive(Clone)]
struct SmallMeterLevels {
    len: usize,
    data: [u8; 32],
}

impl SmallMeterLevels {
    fn from_db(levels_db: &[f32], channels: usize) -> Self {
        let len = channels.clamp(1, 32);
        let mut data = [0; 32];
        for (idx, slot) in data.iter_mut().take(len).enumerate() {
            *slot = Mixer::level_to_qdb(levels_db.get(idx).copied().unwrap_or(FADER_MIN_DB));
        }
        Self { len, data }
    }

    fn get(&self, idx: usize) -> u8 {
        if idx < self.len { self.data[idx] } else { 0 }
    }
}

#[derive(Clone)]
struct FaderBayCanvas {
    track_name: String,
    channels: usize,
    levels_qdb: SmallMeterLevels,
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

    fn static_hash(&self, bounds: Rectangle) -> u64 {
        let mut hasher = DefaultHasher::new();
        bounds.width.to_bits().hash(&mut hasher);
        bounds.height.to_bits().hash(&mut hasher);
        self.channels.hash(&mut hasher);
        self.show_ticks.hash(&mut hasher);
        self.fader_height.to_bits().hash(&mut hasher);
        hasher.finish()
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
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        if bounds.width <= 0.0 || bounds.height <= 0.0 {
            return vec![];
        }

        let slider_bounds = Rectangle {
            x: Self::OUTER_PAD_X,
            y: Self::OUTER_PAD_Y,
            width: FADER_WIDTH,
            height: self.fader_height,
        };
        let meter_x = Self::OUTER_PAD_X + self.slider_block_width() + Self::INNER_GAP;
        let meter_y = Self::OUTER_PAD_Y;
        let meter_inner_h = self.fader_height.max(1.0);
        let meter_total_w = self.meter_block_width();
        let static_hash = self.static_hash(bounds);

        if state.last_hash.get() != static_hash {
            state.cache.clear();
            state.last_hash.set(static_hash);
        }

        let back_color = Color::from_rgb(
            0x42 as f32 / 255.0,
            0x46 as f32 / 255.0,
            0x4D as f32 / 255.0,
        );
        let border_color = Color::from_rgb(
            0x30 as f32 / 255.0,
            0x33 as f32 / 255.0,
            0x3C as f32 / 255.0,
        );
        let filled_color = Color::from_rgb(
            0x29 as f32 / 255.0,
            0x66 as f32 / 255.0,
            0xA3 as f32 / 255.0,
        );
        let handle_color = Color::from_rgb(
            0x75 as f32 / 255.0,
            0xC2 as f32 / 255.0,
            0xFF as f32 / 255.0,
        );
        let border_width = 1.0;
        let handle_height = 2.0;
        let static_geometry = state.cache.draw(renderer, bounds.size(), |frame| {
            frame.fill(
                &Path::rectangle(
                    Point::new(slider_bounds.x, slider_bounds.y),
                    iced::Size::new(slider_bounds.width, slider_bounds.height),
                ),
                back_color,
            );
            frame.stroke(
                &Path::rectangle(
                    Point::new(slider_bounds.x, slider_bounds.y),
                    iced::Size::new(slider_bounds.width, slider_bounds.height),
                ),
                Stroke::default()
                    .with_width(border_width)
                    .with_color(border_color),
            );

            if self.show_ticks {
                let tick_x = slider_bounds.x + FADER_WIDTH + Self::SCALE_GAP;
                for (label_y, label) in Mixer::tick_layout(self.fader_height) {
                    frame.fill(
                        &Path::rectangle(
                            Point::new(tick_x, Self::OUTER_PAD_Y + label_y + 4.0),
                            iced::Size::new(4.0, 1.0),
                        ),
                        Color::from_rgba(0.62, 0.67, 0.77, 0.78),
                    );
                    frame.fill_text(Text {
                        content: label.to_string(),
                        position: Point::new(tick_x + 6.0, Self::OUTER_PAD_Y + label_y),
                        color: Color::from_rgba(0.9, 0.92, 0.96, 0.9),
                        size: 8.0.into(),
                        ..Default::default()
                    });
                }
            }

            frame.fill(
                &Path::rectangle(
                    Point::new(meter_x, meter_y),
                    iced::Size::new(meter_total_w, self.fader_height),
                ),
                Color::from_rgba(0.09, 0.10, 0.12, 1.0),
            );
        });

        let normalized = (self.value - FADER_MIN_DB) / (FADER_MAX_DB - FADER_MIN_DB);
        let handle_y = slider_bounds.y
            + ((slider_bounds.height - handle_height - border_width * 2.0) * (1.0 - normalized))
                .round();
        let mut dynamic_frame = Frame::new(renderer, bounds.size());
        let filled_y = handle_y + handle_height + 1.0;
        let filled_h = (slider_bounds.y + slider_bounds.height - filled_y).max(0.0);
        if filled_h > 0.0 {
            dynamic_frame.fill(
                &Path::rectangle(
                    Point::new(slider_bounds.x, filled_y),
                    iced::Size::new(slider_bounds.width, filled_h),
                ),
                filled_color,
            );
        }
        dynamic_frame.fill(
            &Path::rectangle(
                Point::new(slider_bounds.x, handle_y),
                iced::Size::new(slider_bounds.width, handle_height + border_width * 2.0),
            ),
            handle_color,
        );

        let bar_w = METER_BAR_WIDTH;
        let bar_gap = METER_BAR_GAP;
        for channel_idx in 0..self.channels.max(1) {
            let db = Mixer::qdb_to_level(self.levels_qdb.get(channel_idx));
            let fill = Mixer::level_to_meter_fill(db);
            let filled_h = (meter_inner_h * fill).max(1.0);
            let y = (meter_inner_h - filled_h).max(0.0);
            let x = meter_x + METER_PAD_X + channel_idx as f32 * (bar_w + bar_gap);
            dynamic_frame.fill(
                &Path::rectangle(
                    Point::new(x, meter_y + METER_PAD_Y + y),
                    iced::Size::new(bar_w, (filled_h - METER_PAD_Y).max(1.0)),
                ),
                Mixer::meter_fill_color(db),
            );
        }

        vec![static_geometry, dynamic_frame.into_geometry()]
    }
}

impl canvas::Program<Message> for PanCanvas {
    type State = PanState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<CanvasAction<Message>> {
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if cursor.is_over(bounds) {
                    let now = Instant::now();
                    let is_double_click = state
                        .last_click_at
                        .is_some_and(|last| now.duration_since(last) <= DOUBLE_CLICK);
                    state.last_click_at = Some(now);
                    state.dragging = true;
                    if is_double_click {
                        return Some(CanvasAction::publish(Message::Request(
                            Action::TrackBalance(self.track_name.clone(), 0.0),
                        )));
                    }
                    if let Some(cursor_position) = cursor.position() {
                        let normalized = ((cursor_position.x - bounds.x) / bounds.width.max(1.0))
                            .clamp(0.0, 1.0);
                        let value = (normalized * 2.0 - 1.0).clamp(-1.0, 1.0);
                        return Some(CanvasAction::publish(Message::Request(
                            Action::TrackBalance(self.track_name.clone(), value),
                        )));
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
                    let normalized =
                        ((cursor_position.x - bounds.x) / bounds.width.max(1.0)).clamp(0.0, 1.0);
                    let value = (normalized * 2.0 - 1.0).clamp(-1.0, 1.0);
                    return Some(CanvasAction::publish(Message::Request(
                        Action::TrackBalance(self.track_name.clone(), value),
                    )));
                }
            }
            _ => {}
        }
        None
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        if bounds.width <= 0.0 || bounds.height <= 0.0 {
            return vec![];
        }
        let mut hasher = DefaultHasher::new();
        bounds.width.to_bits().hash(&mut hasher);
        bounds.height.to_bits().hash(&mut hasher);
        let static_hash = hasher.finish();
        if state.last_hash.get() != static_hash {
            state.cache.clear();
            state.last_hash.set(static_hash);
        }
        let back_color = Color::from_rgb(
            0x42 as f32 / 255.0,
            0x46 as f32 / 255.0,
            0x4D as f32 / 255.0,
        );
        let border_color = Color::from_rgb(
            0x30 as f32 / 255.0,
            0x33 as f32 / 255.0,
            0x3C as f32 / 255.0,
        );
        let filled_color = Color::from_rgb(
            0x29 as f32 / 255.0,
            0x66 as f32 / 255.0,
            0xA3 as f32 / 255.0,
        );
        let handle_color = Color::from_rgb(
            0x75 as f32 / 255.0,
            0xC2 as f32 / 255.0,
            0xFF as f32 / 255.0,
        );
        let border_width = 1.0;
        let handle_width = 2.0;

        let static_geometry = state.cache.draw(renderer, bounds.size(), |frame| {
            frame.fill(
                &Path::rectangle(Point::new(0.0, 0.0), bounds.size()),
                back_color,
            );
            frame.stroke(
                &Path::rectangle(Point::new(0.0, 0.0), bounds.size()),
                Stroke::default()
                    .with_width(border_width)
                    .with_color(border_color),
            );
        });

        let center_x = bounds.width * 0.5;
        let normalized = ((self.value.clamp(-1.0, 1.0) + 1.0) * 0.5).clamp(0.0, 1.0);
        let handle_x = ((bounds.width - handle_width - border_width * 2.0) * normalized).round();
        let handle_center = handle_x + (handle_width + border_width * 2.0) * 0.5;

        let fill_start = center_x.min(handle_center);
        let fill_width = (handle_center - center_x).abs();
        let mut dynamic_frame = Frame::new(renderer, bounds.size());
        if fill_width > 0.0 {
            dynamic_frame.fill(
                &Path::rectangle(
                    Point::new(fill_start, 0.0),
                    iced::Size::new(fill_width, bounds.height),
                ),
                filled_color,
            );
        }
        dynamic_frame.fill(
            &Path::rectangle(
                Point::new(handle_x, 0.0),
                iced::Size::new(handle_width + border_width * 2.0, bounds.height),
            ),
            handle_color,
        );
        vec![static_geometry, dynamic_frame.into_geometry()]
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

    fn meter_inner_width(channels: usize) -> f32 {
        let channels = channels.max(1);
        channels as f32 * METER_BAR_WIDTH + (channels.saturating_sub(1) as f32 * METER_BAR_GAP)
    }

    fn meter_total_width(channels: usize) -> f32 {
        Self::meter_inner_width(channels) + (METER_PAD_X * 2.0)
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

    fn pan_section(track_name: String, value: f32) -> Element<'static, Message> {
        row![
            container(text(Self::format_balance(value)).size(9))
                .width(Length::Fixed(24.0))
                .align_x(Alignment::Center),
            canvas(PanCanvas { track_name, value })
                .width(Length::Fixed(PAN_SLIDER_WIDTH))
                .height(Length::Fixed(PAN_ROW_HEIGHT)),
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .into()
    }

    fn quantized_balance_hundredths(balance: f32) -> i16 {
        (balance.clamp(-1.0, 1.0) * 100.0).round() as i16
    }

    fn pan_section_cached(track_name: String, value: f32) -> Element<'static, Message> {
        let dep = (track_name, Self::quantized_balance_hundredths(value));
        lazy(
            dep,
            move |(track_name, value_hundredths)| -> Element<'static, Message> {
                let value = (*value_hundredths as f32) / 100.0;
                Self::pan_section(track_name.clone(), value)
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
                levels_qdb: SmallMeterLevels::from_db(levels_db, channels.max(1)),
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
        Self::fader_bay(
            track_name,
            channels,
            levels_db,
            value,
            fader_height,
            show_ticks,
        )
    }

    fn strip_width_for_channels(channels: usize) -> f32 {
        (FADER_WIDTH + SCALE_WIDTH + 3.0 + 8.0 + Self::meter_total_width(channels.max(1)) + 16.0)
            .max(STRIP_WIDTH)
    }

    fn output_strips_width(
        metronome_width: Option<f32>,
        master_channels: usize,
        metronome_enabled: bool,
    ) -> f32 {
        let mut widths = vec![Self::strip_width_for_channels(master_channels.max(1))];
        if metronome_enabled && let Some(width) = metronome_width {
            widths.insert(0, width);
        }
        let spacing_count = widths.len().saturating_sub(1) as f32;
        widths.iter().copied().sum::<f32>() + (STRIP_SPACING * spacing_count)
    }

    fn visible_track_window(
        track_specs: &[TrackStripSpec<'_>],
        viewport_width: f32,
        scroll_x: f32,
    ) -> (usize, usize, f32, f32) {
        if track_specs.is_empty() {
            return (0, 0, 0.0, 0.0);
        }

        let content_width = track_specs.iter().map(|spec| spec.width).sum::<f32>()
            + (STRIP_SPACING * track_specs.len().saturating_sub(1) as f32)
            + (STRIP_ROW_PADDING_X * 2.0);
        if viewport_width <= 0.0 || content_width <= viewport_width {
            return (0, track_specs.len(), 0.0, 0.0);
        }

        let max_scroll = (content_width - viewport_width).max(0.0);
        let left_edge = (scroll_x.clamp(0.0, 1.0) * max_scroll - MIXER_OVERSCAN_PX).max(0.0);
        let right_edge =
            (left_edge + viewport_width + (MIXER_OVERSCAN_PX * 2.0)).min(content_width);

        let mut current_x = STRIP_ROW_PADDING_X;
        let mut first_visible = track_specs.len();
        let mut last_visible = 0usize;
        for (idx, spec) in track_specs.iter().enumerate() {
            let strip_start = current_x;
            let strip_end = strip_start + spec.width;
            if strip_end >= left_edge && strip_start <= right_edge {
                first_visible = first_visible.min(idx);
                last_visible = idx + 1;
            }
            current_x = strip_end + STRIP_SPACING;
        }

        if first_visible == track_specs.len() {
            return (0, track_specs.len(), 0.0, 0.0);
        }

        let left_spacer = if first_visible == 0 {
            0.0
        } else {
            STRIP_ROW_PADDING_X
                + track_specs[..first_visible]
                    .iter()
                    .map(|spec| spec.width)
                    .sum::<f32>()
                + (STRIP_SPACING * first_visible as f32)
        };
        let right_spacer = if last_visible >= track_specs.len() {
            0.0
        } else {
            STRIP_ROW_PADDING_X
                + track_specs[last_visible..]
                    .iter()
                    .map(|spec| spec.width)
                    .sum::<f32>()
                + (STRIP_SPACING * (track_specs.len() - last_visible) as f32)
        };

        (first_visible, last_visible, left_spacer, right_spacer)
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
        viewport_width: f32,
        scroll_x: f32,
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
        let mut metronome_strip: Option<Element<'a, Message>> = None;
        let track_specs: Vec<_> = state
            .tracks
            .iter()
            .map(|track| TrackStripSpec {
                track,
                width: Self::strip_width_for_channels(track.audio.outs),
            })
            .collect();
        let metronome_width = track_specs
            .iter()
            .find(|spec| spec.track.name == METRONOME_TRACK_ID)
            .map(|spec| spec.width);
        let output_strips_width =
            Self::output_strips_width(metronome_width, hw_out_channels, metronome_enabled);
        let track_viewport_width = (viewport_width - output_strips_width).max(0.0);
        let normal_track_specs: Vec<_> = track_specs
            .into_iter()
            .filter(|spec| spec.track.name != METRONOME_TRACK_ID)
            .collect();
        let (first_visible, last_visible, left_spacer, right_spacer) =
            Self::visible_track_window(&normal_track_specs, track_viewport_width, scroll_x);

        if metronome_enabled
            && let Some(track) = state
                .tracks
                .iter()
                .find(|track| track.name == METRONOME_TRACK_ID)
        {
            let strip_width = Self::strip_width_for_channels(track.audio.outs);
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
            metronome_strip = Some(
                mouse_area(Self::strip_shell(
                    track.name.clone(),
                    state.selected.contains(track.name.as_str()),
                    strip_width,
                    pan,
                    bay,
                    StripReadout {
                        track_name: track.name.clone(),
                        editing: editing_track == Some(track.name.as_str()),
                        edit_input: editing_input,
                        level_label: Self::format_level_db(track.level),
                    },
                ))
                .on_press(Message::SelectTrackFromMixer(track.name.clone()))
                .into(),
            );
        }

        for (index, spec) in normal_track_specs.iter().enumerate() {
            let track = spec.track;
            if index < first_visible || index >= last_visible {
                continue;
            }
            let strip_name = track.name.clone();
            let strip_width = spec.width;
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
            let strip: Element<'a, Message> = mouse_area(Self::strip_shell(
                strip_name,
                state.selected.contains(track.name.as_str()),
                strip_width,
                pan,
                bay,
                StripReadout {
                    track_name: track.name.clone(),
                    editing: editing_track == Some(track.name.as_str()),
                    edit_input: editing_input,
                    level_label: Self::format_level_db(track.level),
                },
            ))
            .on_press(Message::SelectTrackFromMixer(track.name.clone()))
            .into();

            strips = strips.push(strip);
        }
        if left_spacer > 0.0 {
            strips = row![Space::new().width(Length::Fixed(left_spacer)), strips]
                .spacing(STRIP_SPACING)
                .align_y(Alignment::Start);
        }
        if right_spacer > 0.0 {
            strips = strips.push(Space::new().width(Length::Fixed(right_spacer)));
        }

        let master_strip_width = Self::strip_width_for_channels(hw_out_channels.max(1));
        let master_strip: Element<'a, Message> = mouse_area(Self::strip_shell(
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
        .on_press(Message::SelectTrackFromMixer("hw:out".to_string()))
        .into();
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
        .on_scroll(|viewport| Message::MixerScrollXChanged(viewport.relative_offset().x))
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
