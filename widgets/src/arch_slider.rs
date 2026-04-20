use crate::consts::DOUBLE_CLICK;
use iced::advanced::Shell;
use iced::advanced::graphics::geometry;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Tree, Widget};
use iced::mouse;
use iced::widget::canvas::path::Arc as PathArc;
use iced::widget::canvas::{Frame, Path, Stroke};
use iced::{Color, Element, Event, Length, Point, Radians, Rectangle, Size, Vector};
use std::f32::consts::PI;
use std::time::Instant;

const START_ANGLE: f32 = 3.0 * PI / 4.0;
const SWEEP_ANGLE: f32 = 3.0 * PI / 2.0;
const END_ANGLE: f32 = START_ANGLE + SWEEP_ANGLE;

pub struct ArchSlider<'a, Message> {
    range: std::ops::RangeInclusive<f32>,
    value: f32,
    on_change: Box<dyn Fn(f32) -> Message + 'a>,
    width: Length,
    height: Length,
    handle_radius: f32,
    track_width: f32,
    step: Option<f32>,
    double_click_reset: f32,
    fill_mode: FillMode,
    filled_color: Color,
    handle_color: Color,
    on_release: Option<Message>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FillMode {
    Center,
    Start,
}

impl<'a, Message> ArchSlider<'a, Message> {
    pub fn new<F>(range: std::ops::RangeInclusive<f32>, value: f32, on_change: F) -> Self
    where
        F: Fn(f32) -> Message + 'a,
    {
        Self {
            range,
            value,
            on_change: Box::new(on_change),
            width: Length::Fixed(64.0),
            height: Length::Fixed(64.0),
            handle_radius: 4.0,
            track_width: 6.0,
            step: None,
            double_click_reset: 0.0,
            fill_mode: FillMode::Center,
            filled_color: Color::from_rgb(
                0x29 as f32 / 255.0,
                0x66 as f32 / 255.0,
                0xA3 as f32 / 255.0,
            ),
            handle_color: Color::from_rgb(
                0x75 as f32 / 255.0,
                0xC2 as f32 / 255.0,
                0xFF as f32 / 255.0,
            ),
            on_release: None,
        }
    }

    pub fn width(mut self, width: Length) -> Self {
        self.width = width;
        self
    }

    pub fn height(mut self, height: Length) -> Self {
        self.height = height;
        self
    }

    pub fn step(mut self, step: f32) -> Self {
        self.step = Some(step.abs()).filter(|step| *step > 0.0);
        self
    }

    pub fn double_click_reset(mut self, value: f32) -> Self {
        self.double_click_reset = value;
        self
    }

    pub fn fill_from_start(mut self) -> Self {
        self.fill_mode = FillMode::Start;
        self
    }

    pub fn filled_color(mut self, color: Color) -> Self {
        self.filled_color = color;
        self
    }

    pub fn handle_color(mut self, color: Color) -> Self {
        self.handle_color = color;
        self
    }

    pub fn on_release(mut self, message: Message) -> Self {
        self.on_release = Some(message);
        self
    }
}

pub fn arch_slider<'a, Message, F>(
    range: std::ops::RangeInclusive<f32>,
    value: f32,
    on_change: F,
) -> ArchSlider<'a, Message>
where
    F: Fn(f32) -> Message + 'a,
{
    ArchSlider::new(range, value, on_change)
}

#[derive(Default)]
struct State {
    is_dragging: bool,
    last_click_at: Option<Instant>,
    drag_start_y: f32,
    drag_start_value: f32,
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer> for ArchSlider<'a, Message>
where
    Renderer: geometry::Renderer,
    Message: Clone,
{
    fn size(&self) -> Size<Length> {
        Size {
            width: self.width,
            height: self.height,
        }
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let size = limits.width(self.width).height(self.height).resolve(
            self.width,
            self.height,
            Size::ZERO,
        );
        layout::Node::new(size)
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let center = Point::new(bounds.width / 2.0, bounds.height / 2.0);
        let radius = (center.x.min(center.y) - self.handle_radius - 2.0).max(1.0);

        let normalized =
            (self.value - self.range.start()) / (self.range.end() - self.range.start());
        let current_angle = START_ANGLE + normalized * SWEEP_ANGLE;

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

        renderer.with_translation(Vector::new(bounds.x, bounds.y), |renderer| {
            let mut frame = Frame::new(renderer, bounds.size());

            // Draw border arc (slightly thicker)
            let border_path = Path::new(|builder| {
                builder.arc(PathArc {
                    center: Point::new(center.x, center.y),
                    radius,
                    start_angle: Radians(START_ANGLE),
                    end_angle: Radians(END_ANGLE),
                });
            });
            frame.stroke(
                &border_path,
                Stroke::default()
                    .with_width(self.track_width + 2.0)
                    .with_color(border_color),
            );

            // Draw background track
            let track_path = Path::new(|builder| {
                builder.arc(PathArc {
                    center: Point::new(center.x, center.y),
                    radius,
                    start_angle: Radians(START_ANGLE),
                    end_angle: Radians(END_ANGLE),
                });
            });
            frame.stroke(
                &track_path,
                Stroke::default()
                    .with_width(self.track_width)
                    .with_color(back_color),
            );

            // Draw filled portion
            let (filled_start, filled_end) = match self.fill_mode {
                FillMode::Center => {
                    let center_angle = START_ANGLE + SWEEP_ANGLE / 2.0;
                    if current_angle >= center_angle {
                        (center_angle, current_angle)
                    } else {
                        (current_angle, center_angle)
                    }
                }
                FillMode::Start => (START_ANGLE, current_angle),
            };

            if filled_end > filled_start {
                let filled_path = Path::new(|builder| {
                    builder.arc(PathArc {
                        center: Point::new(center.x, center.y),
                        radius,
                        start_angle: Radians(filled_start),
                        end_angle: Radians(filled_end),
                    });
                });
                frame.stroke(
                    &filled_path,
                    Stroke::default()
                        .with_width(self.track_width)
                        .with_color(self.filled_color),
                );
            }

            // Draw handle
            let handle_x = center.x + current_angle.cos() * radius;
            let handle_y = center.y + current_angle.sin() * radius;
            let handle_path = Path::circle(Point::new(handle_x, handle_y), self.handle_radius);
            frame.fill(&handle_path, self.handle_color);

            renderer.draw_geometry(frame.into_geometry());
        });
    }

    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<State>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(State::default())
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn iced::advanced::Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let bounds = layout.bounds();

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if cursor.is_over(bounds) =>
            {
                let now = Instant::now();
                let is_double_click = state
                    .last_click_at
                    .is_some_and(|last| now.duration_since(last) <= DOUBLE_CLICK);
                state.last_click_at = Some(now);
                if is_double_click {
                    let default_value = self
                        .double_click_reset
                        .clamp(*self.range.start(), *self.range.end());
                    shell.publish((self.on_change)(default_value));
                } else if let Some(cursor_position) = cursor.position() {
                    state.is_dragging = true;
                    state.drag_start_y = cursor_position.y;
                    state.drag_start_value = self.value;
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if state.is_dragging =>
            {
                state.is_dragging = false;
                if let Some(message) = self.on_release.as_ref() {
                    shell.publish(message.clone());
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if state.is_dragging
                    && let Some(cursor_position) = cursor.position()
                {
                    let delta_y = state.drag_start_y - cursor_position.y;
                    let range_size = self.range.end() - self.range.start();
                    let value_change = (delta_y / 200.0) * range_size;
                    let raw_value = state.drag_start_value + value_change;
                    let new_value = self.clamp_to_step(raw_value);
                    shell.publish((self.on_change)(new_value));
                }
            }
            _ => {}
        }
    }
}

impl<'a, Message> ArchSlider<'a, Message> {
    #[allow(dead_code)]
    fn calculate_value(&self, cursor_position: Point, bounds: Rectangle) -> f32 {
        let center = Point::new(
            bounds.x + bounds.width / 2.0,
            bounds.y + bounds.height / 2.0,
        );
        let dx = cursor_position.x - center.x;
        let dy = cursor_position.y - center.y;
        let mut angle = dy.atan2(dx);
        if angle < 0.0 {
            angle += 2.0 * PI;
        }

        let start = START_ANGLE;
        let sweep = SWEEP_ANGLE;

        let mut effective_angle = angle;
        if effective_angle < start {
            effective_angle += 2.0 * PI;
        }

        let clamped = effective_angle.clamp(start, start + sweep);
        let normalized = (clamped - start) / sweep;
        let value = self.range.start() + normalized * (self.range.end() - self.range.start());
        self.clamp_to_step(value)
    }

    fn clamp_to_step(&self, value: f32) -> f32 {
        let clamped = value.clamp(*self.range.start(), *self.range.end());
        let Some(step) = self.step else {
            return clamped;
        };

        let start = *self.range.start();
        let end = *self.range.end();
        let steps = ((clamped - start) / step).round();
        (start + steps * step).clamp(start, end)
    }
}

impl<'a, Message, Theme, Renderer> From<ArchSlider<'a, Message>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a + Clone,
    Theme: 'a,
    Renderer: 'a + geometry::Renderer,
{
    fn from(slider: ArchSlider<'a, Message>) -> Self {
        Self::new(slider)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::Event;
    use iced::advanced::{
        Layout, Shell, clipboard, layout,
        widget::{self, Tree, Widget},
    };
    use std::time::Instant;

    fn test_tree_with_state(state: State) -> Tree {
        Tree {
            tag: widget::tree::Tag::of::<State>(),
            state: widget::tree::State::new(state),
            children: Vec::new(),
        }
    }

    #[test]
    fn calculate_value_clamps_to_range() {
        let slider = ArchSlider::new(-1.0..=1.0, 0.0, |value| value);
        let bounds = Rectangle {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 100.0,
        };

        // Bottom-left (start of arc) gives minimum value
        assert_eq!(
            slider.calculate_value(Point::new(10.0, 120.0), bounds),
            -1.0
        );
        // Bottom-right (end of arc) gives maximum value
        assert_eq!(
            slider.calculate_value(Point::new(110.0, 120.0), bounds),
            1.0
        );
        // Top (middle of arc) gives center value
        assert!((slider.calculate_value(Point::new(60.0, 20.0), bounds) - 0.0).abs() < 0.001);
    }

    #[test]
    fn calculate_value_snaps_to_step() {
        let slider = ArchSlider::new(-1.0..=1.0, 0.0, |value| value).step(0.1);
        let bounds = Rectangle {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        };

        // Top of arc is 0.0; small offsets should snap to nearest step
        assert!((slider.calculate_value(Point::new(50.0, 10.0), bounds) - 0.0).abs() < 0.001);
        assert!((slider.calculate_value(Point::new(50.0, 15.0), bounds) - 0.0).abs() < 0.001);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn update_dragging_publishes_vertical_motion() {
        let mut slider =
            ArchSlider::new(-1.0..=1.0, 0.0, |value| value).width(Length::Fixed(100.0));
        let mut tree = test_tree_with_state(State::default());
        let node = layout::Node::new(Size::new(100.0, 100.0));
        let layout = Layout::new(&node);
        let mut messages = Vec::new();
        let renderer = ();
        let mut clipboard = clipboard::Null;
        let viewport = Rectangle::new(Point::ORIGIN, Size::new(100.0, 100.0));

        // Click to start dragging
        {
            let mut shell = Shell::new(&mut messages);
            <ArchSlider<'_, f32> as Widget<f32, iced::Theme, ()>>::update(
                &mut slider,
                &mut tree,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                layout,
                mouse::Cursor::Available(Point::new(50.0, 50.0)),
                &renderer,
                &mut clipboard,
                &mut shell,
                &viewport,
            );
        }
        assert!(messages.is_empty());

        // Drag up 100 pixels should cover half the range (200px = full range, so +1.0)
        {
            let mut shell = Shell::new(&mut messages);
            <ArchSlider<'_, f32> as Widget<f32, iced::Theme, ()>>::update(
                &mut slider,
                &mut tree,
                &Event::Mouse(mouse::Event::CursorMoved {
                    position: Point::new(50.0, -50.0),
                }),
                layout,
                mouse::Cursor::Available(Point::new(50.0, -50.0)),
                &renderer,
                &mut clipboard,
                &mut shell,
                &viewport,
            );
        }

        assert_eq!(messages.len(), 1);
        assert!((messages[0] - 1.0).abs() < 0.001);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn update_double_click_resets_to_zero() {
        let mut slider =
            ArchSlider::new(-1.0..=1.0, 0.75, |value| value).width(Length::Fixed(100.0));
        let mut tree = test_tree_with_state(State {
            is_dragging: false,
            last_click_at: Some(Instant::now()),
            drag_start_y: 0.0,
            drag_start_value: 0.0,
        });
        let node = layout::Node::new(Size::new(100.0, 100.0));
        let layout = Layout::new(&node);
        let mut messages = Vec::new();
        let mut shell = Shell::new(&mut messages);
        let renderer = ();
        let mut clipboard = clipboard::Null;
        let viewport = Rectangle::new(Point::ORIGIN, Size::new(100.0, 100.0));

        <ArchSlider<'_, f32> as Widget<f32, iced::Theme, ()>>::update(
            &mut slider,
            &mut tree,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            layout,
            mouse::Cursor::Available(Point::new(90.0, 90.0)),
            &renderer,
            &mut clipboard,
            &mut shell,
            &viewport,
        );

        assert_eq!(messages, vec![0.0]);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn update_double_click_resets_to_custom_value() {
        let mut slider = ArchSlider::new(0.0..=1.0, 0.75, |value| value)
            .width(Length::Fixed(100.0))
            .double_click_reset(0.5);
        let mut tree = test_tree_with_state(State {
            is_dragging: false,
            last_click_at: Some(Instant::now()),
            drag_start_y: 0.0,
            drag_start_value: 0.0,
        });
        let node = layout::Node::new(Size::new(100.0, 100.0));
        let layout = Layout::new(&node);
        let mut messages = Vec::new();
        let mut shell = Shell::new(&mut messages);
        let renderer = ();
        let mut clipboard = clipboard::Null;
        let viewport = Rectangle::new(Point::ORIGIN, Size::new(100.0, 100.0));

        <ArchSlider<'_, f32> as Widget<f32, iced::Theme, ()>>::update(
            &mut slider,
            &mut tree,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            layout,
            mouse::Cursor::Available(Point::new(90.0, 90.0)),
            &renderer,
            &mut clipboard,
            &mut shell,
            &viewport,
        );

        assert_eq!(messages, vec![0.5]);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn update_publishes_release_message() {
        let mut slider = ArchSlider::new(-1.0..=1.0, 0.0, |value| value)
            .width(Length::Fixed(100.0))
            .on_release(99.0);
        let mut tree = test_tree_with_state(State {
            is_dragging: true,
            last_click_at: None,
            drag_start_y: 0.0,
            drag_start_value: 0.0,
        });
        let node = layout::Node::new(Size::new(100.0, 100.0));
        let layout = Layout::new(&node);
        let mut messages = Vec::new();
        let mut shell = Shell::new(&mut messages);
        let renderer = ();
        let mut clipboard = clipboard::Null;
        let viewport = Rectangle::new(Point::ORIGIN, Size::new(100.0, 100.0));

        <ArchSlider<'_, f32> as Widget<f32, iced::Theme, ()>>::update(
            &mut slider,
            &mut tree,
            &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            layout,
            mouse::Cursor::Unavailable,
            &renderer,
            &mut clipboard,
            &mut shell,
            &viewport,
        );

        assert_eq!(messages, vec![99.0]);
    }
}
