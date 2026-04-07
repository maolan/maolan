use iced::widget::{
    canvas,
    canvas::{Frame, Geometry, Path},
};
use iced::{Color, Element, Length, Point, Rectangle, Renderer, Theme, mouse};
use std::{
    cell::Cell,
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

const FADER_MIN_DB: f32 = -90.0;
const FADER_MAX_DB: f32 = 20.0;
const METER_BAR_WIDTH: f32 = 3.0;
const METER_BAR_GAP: f32 = 2.0;
const METER_PAD_X: f32 = 3.0;
const METER_PAD_Y: f32 = 3.0;
const OUTER_PAD_Y: f32 = 7.0;

#[derive(Default)]
struct State {
    cache: canvas::Cache,
    last_hash: Cell<u64>,
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
            *slot = level_to_qdb(levels_db.get(idx).copied().unwrap_or(FADER_MIN_DB));
        }
        Self { len, data }
    }

    fn get(&self, idx: usize) -> u8 {
        if idx < self.len { self.data[idx] } else { 0 }
    }
}

#[derive(Clone)]
struct MetersCanvas {
    channels: usize,
    levels_qdb: SmallMeterLevels,
    fader_height: f32,
}

impl MetersCanvas {
    fn static_hash(&self, bounds: Rectangle) -> u64 {
        let mut hasher = DefaultHasher::new();
        bounds.width.to_bits().hash(&mut hasher);
        bounds.height.to_bits().hash(&mut hasher);
        self.channels.hash(&mut hasher);
        self.fader_height.to_bits().hash(&mut hasher);
        hasher.finish()
    }
}

impl<Message> canvas::Program<Message> for MetersCanvas {
    type State = State;

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

        let static_hash = self.static_hash(bounds);
        if state.last_hash.get() != static_hash {
            state.cache.clear();
            state.last_hash.set(static_hash);
        }

        let static_geometry = state.cache.draw(renderer, bounds.size(), |frame| {
            frame.fill(
                &Path::rectangle(Point::new(0.0, 0.0), bounds.size()),
                Color::from_rgba(0.09, 0.10, 0.12, 1.0),
            );
        });

        let meter_inner_h = self.fader_height.max(1.0);
        let mut dynamic_frame = Frame::new(renderer, bounds.size());
        for channel_idx in 0..self.channels.max(1) {
            let db = qdb_to_level(self.levels_qdb.get(channel_idx));
            let fill = level_to_meter_fill(db);
            let filled_h = (meter_inner_h * fill).max(1.0);
            let y = (meter_inner_h - filled_h).max(0.0);
            let x = METER_PAD_X + channel_idx as f32 * (METER_BAR_WIDTH + METER_BAR_GAP);
            dynamic_frame.fill(
                &Path::rectangle(
                    Point::new(x, METER_PAD_Y + y),
                    iced::Size::new(METER_BAR_WIDTH, (filled_h - METER_PAD_Y).max(1.0)),
                ),
                meter_fill_color(db),
            );
        }

        vec![static_geometry, dynamic_frame.into_geometry()]
    }
}

fn level_to_meter_fill(level_db: f32) -> f32 {
    ((level_db - FADER_MIN_DB) / (FADER_MAX_DB - FADER_MIN_DB)).clamp(0.0, 1.0)
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

pub fn total_width(channels: usize) -> f32 {
    let channels = channels.max(1);
    channels as f32 * METER_BAR_WIDTH
        + (channels.saturating_sub(1) as f32 * METER_BAR_GAP)
        + (METER_PAD_X * 2.0)
}

pub fn meters<'a, Message>(
    channels: usize,
    levels_db: &[f32],
    fader_height: f32,
) -> Element<'a, Message>
where
    Message: 'a,
{
    canvas(MetersCanvas {
        channels: channels.max(1),
        levels_qdb: SmallMeterLevels::from_db(levels_db, channels),
        fader_height,
    })
    .width(Length::Fixed(total_width(channels)))
    .height(Length::Fixed(fader_height + (OUTER_PAD_Y * 2.0)))
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn total_width_uses_minimum_single_channel() {
        assert_eq!(total_width(0), total_width(1));
        assert_eq!(total_width(1), METER_BAR_WIDTH + (METER_PAD_X * 2.0));
    }

    #[test]
    fn total_width_scales_with_channel_count() {
        assert_eq!(total_width(2), 14.0);
        assert_eq!(total_width(4), 24.0);
    }

    #[test]
    fn level_quantization_roundtrips_whole_db_values() {
        for db in [-90.0, -48.0, -12.0, 0.0, 20.0] {
            assert_eq!(qdb_to_level(level_to_qdb(db)), db);
        }
    }

    #[test]
    fn small_meter_levels_clamps_channel_count_and_defaults_missing_values() {
        let levels = SmallMeterLevels::from_db(&[-12.0, 6.0], 3);

        assert_eq!(levels.len, 3);
        assert_eq!(qdb_to_level(levels.get(0)), -12.0);
        assert_eq!(qdb_to_level(levels.get(1)), 6.0);
        assert_eq!(qdb_to_level(levels.get(2)), FADER_MIN_DB);
        assert_eq!(levels.get(31), 0);
    }

    #[test]
    fn meter_fill_color_switches_at_thresholds() {
        assert_eq!(meter_fill_color(1.0), Color::from_rgb(0.96, 0.47, 0.34));
        assert_eq!(meter_fill_color(-6.0), Color::from_rgb(0.69, 0.86, 0.41));
        assert_eq!(meter_fill_color(-18.0), Color::from_rgb(0.20, 0.78, 0.51));
    }

    #[test]
    fn level_to_meter_fill_clamps_range() {
        assert_eq!(level_to_meter_fill(FADER_MIN_DB - 10.0), 0.0);
        assert_eq!(level_to_meter_fill(FADER_MAX_DB + 10.0), 1.0);
        assert!((level_to_meter_fill(-35.0) - 0.5).abs() < 0.001);
    }

    #[test]
    fn level_to_qdb_clamps_extremes() {
        assert_eq!(level_to_qdb(-100.0), 0);
        assert_eq!(level_to_qdb(50.0), 110);
        assert_eq!(level_to_qdb(FADER_MIN_DB), 0);
        assert_eq!(level_to_qdb(FADER_MAX_DB), 110);
    }

    #[test]
    fn qdb_to_level_handles_all_valid_values() {
        assert_eq!(qdb_to_level(0), -90.0);
        assert_eq!(qdb_to_level(90), 0.0);
        assert_eq!(qdb_to_level(110), 20.0);
    }

    #[test]
    fn small_meter_levels_handles_empty_input() {
        let levels = SmallMeterLevels::from_db(&[], 1);
        assert_eq!(levels.len, 1);
        assert_eq!(qdb_to_level(levels.get(0)), FADER_MIN_DB);
    }

    #[test]
    fn small_meter_levels_clamps_to_max_32() {
        let levels = SmallMeterLevels::from_db(&[0.0; 64], 64);
        assert_eq!(levels.len, 32);
    }

    #[test]
    fn small_meter_levels_get_returns_zero_for_out_of_bounds() {
        let levels = SmallMeterLevels::from_db(&[-12.0], 1);
        assert_eq!(levels.get(100), 0);
    }

    #[test]
    fn meter_fill_color_at_exact_thresholds() {
        assert_eq!(meter_fill_color(0.0), Color::from_rgb(0.96, 0.47, 0.34));
        assert_eq!(meter_fill_color(-12.0), Color::from_rgb(0.69, 0.86, 0.41));
    }

    #[test]
    fn total_width_calculates_correctly_for_multiple_channels() {
        // 1 channel: 3 + 3*2 = 9
        // 2 channels: 3*2 + 2*1 + 3*2 = 6 + 2 + 6 = 14
        // 3 channels: 3*3 + 2*2 + 3*2 = 9 + 4 + 6 = 19
        let w1 = total_width(1);
        let w2 = total_width(2);
        let w3 = total_width(3);

        assert!((w2 - w1 - 5.0).abs() < 0.001);
        assert!((w3 - w2 - 5.0).abs() < 0.001);
    }
}
