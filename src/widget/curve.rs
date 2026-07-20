use crate::message::Message;
use iced::{
    Color, Point, Rectangle, Renderer, Size, Theme, mouse,
    widget::{
        canvas::Cache, canvas::Frame, canvas::Geometry, canvas::Path, canvas::Program,
    },
};
use std::{
    cell::Cell,
    hash::{Hash, Hasher},
};

/// A single sample/value point on a normalized curve.
///
/// `value` is expected to be in the `[0, 1]` range; it is clamped when drawn.
#[derive(Debug, Clone, Copy)]
pub struct CurvePoint {
    pub sample: usize,
    pub value: f32,
}

impl CurvePoint {
    /// Compute the local position of this point inside `bounds`.
    ///
    /// The returned `Point` is relative to the top-left corner of the canvas,
    /// since `iced` translates the renderer by `(bounds.x, bounds.y)` before
    /// calling `Program::draw`.
    pub fn position(&self, bounds: Rectangle, pixels_per_sample: f32) -> Point {
        let x = self.sample as f32 * pixels_per_sample;
        let y = if bounds.height <= f32::EPSILON {
            0.0
        } else {
            bounds.height * (1.0 - self.value.clamp(0.0, 1.0))
        };
        Point::new(x, y)
    }
}

#[derive(Clone)]
pub struct CurveCanvas {
    pub points: Vec<CurvePoint>,
    pub pixels_per_sample: f32,
    pub color: Color,
    pub dot_radius: f32,
    pub line_width: f32,
}

#[derive(Default)]
pub struct CurveCanvasState {
    cache: Cache,
    last_hash: Cell<u64>,
}

impl CurveCanvas {
    fn shape_hash(&self, bounds: Rectangle) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        bounds.width.to_bits().hash(&mut hasher);
        bounds.height.to_bits().hash(&mut hasher);
        self.pixels_per_sample.to_bits().hash(&mut hasher);
        self.color.r.to_bits().hash(&mut hasher);
        self.color.g.to_bits().hash(&mut hasher);
        self.color.b.to_bits().hash(&mut hasher);
        self.color.a.to_bits().hash(&mut hasher);
        self.dot_radius.to_bits().hash(&mut hasher);
        self.line_width.to_bits().hash(&mut hasher);
        self.points.len().hash(&mut hasher);
        for point in &self.points {
            point.sample.hash(&mut hasher);
            point.value.to_bits().hash(&mut hasher);
        }
        hasher.finish()
    }
}

impl Program<Message> for CurveCanvas {
    type State = CurveCanvasState;

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        if bounds.width <= f32::EPSILON || bounds.height <= f32::EPSILON {
            return vec![];
        }

        let hash = self.shape_hash(bounds);
        if state.last_hash.get() != hash {
            state.cache.clear();
            state.last_hash.set(hash);
        }

        let geometry = state
            .cache
            .draw(renderer, bounds.size(), |frame: &mut Frame| {
                if self.points.len() < 2 {
                    return;
                }

                let mut sorted = self.points.clone();
                sorted.sort_unstable_by_key(|point| point.sample);

                let bar_width = self.line_width.max(1.0);
                for point in &sorted {
                    let top = point.position(bounds, self.pixels_per_sample);
                    let rect = Path::rectangle(
                        Point::new(top.x - bar_width / 2.0, top.y),
                        Size::new(bar_width, (bounds.height - top.y).max(1.0)),
                    );
                    frame.fill(&rect, self.color);
                    let circle = Path::circle(top, self.dot_radius.max(0.0));
                    frame.fill(&circle, self.color);
                }
            });

        vec![geometry]
    }
}
