use iced::widget::{
    canvas,
    canvas::{Geometry, Path, Text},
};
use iced::{Color, Element, Length, Point, Rectangle, Renderer, Theme, mouse};
use std::{
    cell::Cell,
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

const SCALE_WIDTH: f32 = 22.0;
const SCALE_GAP: f32 = 3.0;
const OUTER_PAD_Y: f32 = 7.0;

#[derive(Default)]
struct State {
    cache: canvas::Cache,
    last_hash: Cell<u64>,
}

#[derive(Clone)]
struct TicksCanvas {
    spec: TickSpec,
    height: f32,
}

#[derive(Clone)]
enum TickSpec {
    Range(std::ops::RangeInclusive<f32>),
    Custom(Vec<f32>, TickMapping),
}

#[derive(Clone, Copy)]
enum TickMapping {
    Linear { min: f32, max: f32 },
    X32Fader,
}

impl TicksCanvas {
    fn static_hash(&self, bounds: Rectangle) -> u64 {
        let mut hasher = DefaultHasher::new();
        bounds.width.to_bits().hash(&mut hasher);
        bounds.height.to_bits().hash(&mut hasher);
        match &self.spec {
            TickSpec::Range(range) => {
                range.start().to_bits().hash(&mut hasher);
                range.end().to_bits().hash(&mut hasher);
            }
            TickSpec::Custom(values, mapping) => {
                for value in values {
                    value.to_bits().hash(&mut hasher);
                }
                match mapping {
                    TickMapping::Linear { min, max } => {
                        0_u8.hash(&mut hasher);
                        min.to_bits().hash(&mut hasher);
                        max.to_bits().hash(&mut hasher);
                    }
                    TickMapping::X32Fader => {
                        1_u8.hash(&mut hasher);
                    }
                }
            }
        }
        self.height.to_bits().hash(&mut hasher);
        hasher.finish()
    }

    fn range_value_to_y(
        range: &std::ops::RangeInclusive<f32>,
        value: f32,
        fader_height: f32,
    ) -> f32 {
        let start = *range.start();
        let end = *range.end();
        let span = (end - start).abs().max(f32::EPSILON);
        let normalized = ((value - start) / span).clamp(0.0, 1.0);
        fader_height * (1.0 - normalized)
    }

    fn tick_layout(&self) -> Vec<(f32, String)> {
        self.tick_values()
            .into_iter()
            .map(|value| {
                let y = self.value_to_y(value).clamp(0.0, self.height - 1.0);
                let label_y = (y - 4.0).clamp(0.0, (self.height - 10.0).max(0.0));
                (label_y, Self::format_tick_label(value))
            })
            .collect()
    }

    fn tick_values(&self) -> Vec<f32> {
        match &self.spec {
            TickSpec::Range(range) => Self::range_tick_values(range),
            TickSpec::Custom(values, _) => values.clone(),
        }
    }

    fn value_to_y(&self, value: f32) -> f32 {
        match &self.spec {
            TickSpec::Range(range) => Self::range_value_to_y(range, value, self.height),
            TickSpec::Custom(_, TickMapping::Linear { min, max }) => {
                let span = (max - min).abs().max(f32::EPSILON);
                let normalized = ((value - min) / span).clamp(0.0, 1.0);
                self.height * (1.0 - normalized)
            }
            TickSpec::Custom(_, TickMapping::X32Fader) => {
                let normalized = x32_db_to_normalized(value);
                self.height * (1.0 - normalized)
            }
        }
    }

    fn range_tick_values(range: &std::ops::RangeInclusive<f32>) -> Vec<f32> {
        let start = *range.start();
        let end = *range.end();
        let min = start.min(end);
        let max = start.max(end);
        let span = (max - min).max(f32::EPSILON);
        let step = Self::nice_step(span / 9.0);

        let mut values = Vec::new();
        values.push(min);

        let first = (min / step).ceil() * step;
        let mut value = first;
        while value < max {
            if (value - min).abs() > 0.0001 && (value - max).abs() > 0.0001 {
                values.push(Self::normalize_zero(value));
            }
            value += step;
        }

        if (max - min).abs() > 0.0001 {
            values.push(max);
        }

        values.sort_by(|a, b| a.total_cmp(b));
        values.dedup_by(|a, b| (*a - *b).abs() < 0.0001);
        values
    }

    fn nice_step(rough_step: f32) -> f32 {
        if rough_step <= 0.0 {
            return 1.0;
        }

        let base_exp = rough_step.log10().floor() as i32;
        let mut best = rough_step;
        let mut best_diff = f32::INFINITY;

        for exp in (base_exp - 1)..=(base_exp + 1) {
            let scale = 10_f32.powi(exp);
            for multiplier in [1.0, 2.0, 2.5, 5.0, 10.0] {
                let candidate = multiplier * scale;
                let diff = (candidate - rough_step).abs();
                if diff < best_diff {
                    best = candidate;
                    best_diff = diff;
                }
            }
        }

        best.max(f32::EPSILON)
    }

    fn normalize_zero(value: f32) -> f32 {
        if value.abs() < 0.0001 { 0.0 } else { value }
    }

    fn format_tick_label(value: f32) -> String {
        let value = Self::normalize_zero(value);
        if value == 0.0 {
            return "0".to_string();
        }

        if (value.fract()).abs() < 0.0001 {
            format!("{value:+.0}")
        } else {
            format!("{value:+.1}")
        }
    }
}

impl<Message> canvas::Program<Message> for TicksCanvas {
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
            let effective_height = (bounds.height - (OUTER_PAD_Y * 2.0)).max(1.0);
            let layout = Self {
                spec: self.spec.clone(),
                height: effective_height,
            }
            .tick_layout();
            let tick_x = SCALE_GAP;
            for (label_y, label) in layout {
                frame.fill(
                    &Path::rectangle(
                        Point::new(tick_x, OUTER_PAD_Y + label_y + 4.0),
                        iced::Size::new(4.0, 1.0),
                    ),
                    Color::from_rgba(0.62, 0.67, 0.77, 0.78),
                );
                frame.fill_text(Text {
                    content: label,
                    position: Point::new(tick_x + 6.0, OUTER_PAD_Y + label_y),
                    color: Color::from_rgba(0.9, 0.92, 0.96, 0.9),
                    size: 8.0.into(),
                    ..Default::default()
                });
            }
        });

        vec![static_geometry]
    }
}

pub fn ticks<'a, Message>(
    range: std::ops::RangeInclusive<f32>,
    fader_height: f32,
) -> Element<'a, Message>
where
    Message: 'a,
{
    canvas(TicksCanvas {
        spec: TickSpec::Range(range),
        height: fader_height,
    })
    .width(Length::Fixed(SCALE_GAP + SCALE_WIDTH))
    .height(Length::Fill)
    .into()
}

pub fn x32_ticks<'a, Message>(height: f32) -> Element<'a, Message>
where
    Message: 'a,
{
    canvas(TicksCanvas {
        spec: TickSpec::Custom(
            vec![-90.0, -60.0, -40.0, -20.0, -10.0, -5.0, 0.0, 5.0, 10.0],
            TickMapping::X32Fader,
        ),
        height,
    })
    .width(Length::Fixed(SCALE_GAP + SCALE_WIDTH))
    .height(Length::Fill)
    .into()
}

pub fn meter_ticks<'a, Message>(height: f32) -> Element<'a, Message>
where
    Message: 'a,
{
    canvas(TicksCanvas {
        spec: TickSpec::Custom(
            vec![-90.0, -60.0, -40.0, -20.0, -12.0, -6.0, 0.0, 10.0, 20.0],
            TickMapping::Linear {
                min: -90.0,
                max: 20.0,
            },
        ),
        height,
    })
    .width(Length::Fixed(SCALE_GAP + SCALE_WIDTH))
    .height(Length::Fill)
    .into()
}

fn x32_db_to_normalized(db: f32) -> f32 {
    let db = db.clamp(-90.0, 10.0);
    if db >= -10.0 {
        (db + 30.0) / 40.0
    } else if db >= -30.0 {
        (db + 50.0) / 80.0
    } else if db >= -60.0 {
        (db + 70.0) / 160.0
    } else {
        (db + 90.0) / 480.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_to_y_maps_extremes_and_midpoint() {
        let height = 110.0;
        let range = -90.0..=20.0;

        assert_eq!(TicksCanvas::range_value_to_y(&range, 20.0, height), 0.0);
        assert_eq!(TicksCanvas::range_value_to_y(&range, -90.0, height), height);

        let midpoint = (-90.0 + 20.0) * 0.5;
        assert!((TicksCanvas::range_value_to_y(&range, midpoint, height) - 55.0).abs() < 0.001);
    }

    #[test]
    fn tick_values_include_range_endpoints() {
        let range = -90.0..=20.0;
        let values = TicksCanvas::range_tick_values(&range);

        assert_eq!(values.first().copied(), Some(-90.0));
        assert_eq!(values.last().copied(), Some(20.0));
    }

    #[test]
    fn tick_values_are_generated_from_range() {
        let range = -90.0..=20.0;
        let values = TicksCanvas::range_tick_values(&range);

        assert!(values.contains(&0.0));
        assert!(values.contains(&10.0));
        assert!(!values.contains(&12.0));
    }

    #[test]
    fn tick_layout_positions_stay_in_bounds() {
        let height = 110.0;
        let canvas = TicksCanvas {
            spec: TickSpec::Range(-90.0..=20.0),
            height,
        };
        let layout = canvas.tick_layout();

        for (y, _) in layout {
            assert!(y >= 0.0);
            assert!(y <= (height - 10.0).max(0.0));
        }
    }

    #[test]
    fn format_tick_label_formats_zero_integer_and_decimal_values() {
        assert_eq!(TicksCanvas::format_tick_label(0.0), "0");
        assert_eq!(TicksCanvas::format_tick_label(12.0), "+12");
        assert_eq!(TicksCanvas::format_tick_label(-3.5), "-3.5");
    }

    #[test]
    fn format_tick_label_handles_negative_integers() {
        assert_eq!(TicksCanvas::format_tick_label(-12.0), "-12");
    }

    #[test]
    fn value_to_y_clamps_out_of_range() {
        let height = 110.0;
        let range = -90.0..=20.0;

        // Above max should return 0 (top)
        assert_eq!(TicksCanvas::range_value_to_y(&range, 100.0, height), 0.0);
        // Below min should return height (bottom)
        assert_eq!(
            TicksCanvas::range_value_to_y(&range, -200.0, height),
            height
        );
    }

    #[test]
    fn tick_values_for_small_range() {
        let range = -10.0..=10.0;
        let values = TicksCanvas::range_tick_values(&range);

        assert!(values.contains(&0.0));
        assert!(values.contains(&-10.0));
        assert!(values.contains(&10.0));
    }

    #[test]
    fn tick_values_for_zero_range() {
        let range = 0.0..=0.0;
        let values = TicksCanvas::range_tick_values(&range);

        assert_eq!(values.len(), 1);
        assert_eq!(values[0], 0.0);
    }
}
