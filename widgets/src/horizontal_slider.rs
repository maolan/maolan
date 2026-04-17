use crate::consts::DOUBLE_CLICK;
use iced::advanced::Shell;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Tree, Widget};
use iced::mouse;
use iced::{Border, Color, Element, Event, Length, Point, Rectangle, Size};
use std::time::Instant;

pub struct HorizontalSlider<'a, Message> {
    range: std::ops::RangeInclusive<f32>,
    value: f32,
    on_change: Box<dyn Fn(f32) -> Message + 'a>,
    width: Length,
    height: Length,
    handle_width: f32,
    step: Option<f32>,
    double_click_reset: f32,
    fill_mode: FillMode,
    filled_color: Color,
    handle_color: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FillMode {
    Center,
    Start,
}

impl<'a, Message> HorizontalSlider<'a, Message> {
    pub fn new<F>(range: std::ops::RangeInclusive<f32>, value: f32, on_change: F) -> Self
    where
        F: Fn(f32) -> Message + 'a,
    {
        Self {
            range,
            value,
            on_change: Box::new(on_change),
            width: Length::Fixed(52.0),
            height: Length::Fixed(12.0),
            handle_width: 2.0,
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
}

pub fn horizontal_slider<'a, Message, F>(
    range: std::ops::RangeInclusive<f32>,
    value: f32,
    on_change: F,
) -> HorizontalSlider<'a, Message>
where
    F: Fn(f32) -> Message + 'a,
{
    HorizontalSlider::new(range, value, on_change)
}

#[derive(Default)]
struct State {
    is_dragging: bool,
    last_click_at: Option<Instant>,
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for HorizontalSlider<'a, Message>
where
    Renderer: renderer::Renderer,
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
        let border_width = 1.0;
        let twice_border = border_width * 2.0;
        let value_bounds_x = bounds.x + (self.handle_width / 2.0);
        let value_bounds_width = (bounds.width - self.handle_width).max(0.0);
        let normalized =
            (self.value - self.range.start()) / (self.range.end() - self.range.start());
        let handle_offset =
            (value_bounds_x + (value_bounds_width - twice_border) * normalized).round();

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
        let border_radius = 2.0;

        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle {
                    x: bounds.x,
                    y: bounds.y,
                    width: bounds.width,
                    height: bounds.height,
                },
                border: Border {
                    radius: border_radius.into(),
                    width: border_width,
                    color: border_color,
                },
                ..Default::default()
            },
            back_color,
        );

        let handle_center = handle_offset + self.handle_width * 0.5;
        let (filled_x_start, filled_width) = match self.fill_mode {
            FillMode::Center => {
                let center_x = bounds.x + bounds.width * 0.5;
                if handle_center >= center_x {
                    (center_x, (handle_center - center_x).max(0.0))
                } else {
                    (
                        handle_center.min(center_x),
                        (center_x - handle_center.min(center_x)).max(0.0),
                    )
                }
            }
            FillMode::Start => (bounds.x, (handle_center - bounds.x).max(0.0)),
        };

        if filled_width > 0.0 {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: filled_x_start,
                        y: bounds.y,
                        width: filled_width,
                        height: bounds.height,
                    },
                    border: Border {
                        radius: border_radius.into(),
                        width: border_width,
                        color: Color::TRANSPARENT,
                    },
                    ..Default::default()
                },
                self.filled_color,
            );
        }

        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle {
                    x: handle_offset,
                    y: bounds.y,
                    width: self.handle_width + twice_border,
                    height: bounds.height,
                },
                border: Border {
                    radius: border_radius.into(),
                    width: border_width,
                    color: Color::TRANSPARENT,
                },
                ..Default::default()
            },
            self.handle_color,
        );
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
                    let new_value = self.calculate_value(cursor_position, bounds);
                    shell.publish((self.on_change)(new_value));
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if state.is_dragging =>
            {
                state.is_dragging = false;
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if state.is_dragging
                    && let Some(cursor_position) = cursor.position()
                {
                    let new_value = self.calculate_value(cursor_position, bounds);
                    shell.publish((self.on_change)(new_value));
                }
            }
            _ => {}
        }
    }
}

impl<'a, Message> HorizontalSlider<'a, Message> {
    fn calculate_value(&self, cursor_position: Point, bounds: Rectangle) -> f32 {
        let usable_width = (bounds.width - self.handle_width).max(0.0);
        if usable_width <= f32::EPSILON {
            return self.clamp_to_step(*self.range.start());
        }

        let x = (cursor_position.x - bounds.x - self.handle_width / 2.0).clamp(0.0, usable_width);
        let normalized = (x / usable_width).clamp(0.0, 1.0);
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

impl<'a, Message, Theme, Renderer> From<HorizontalSlider<'a, Message>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: renderer::Renderer + 'a,
{
    fn from(slider: HorizontalSlider<'a, Message>) -> Self {
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
        let slider = HorizontalSlider::new(-1.0..=1.0, 0.0, |value| value);
        let bounds = Rectangle {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 14.0,
        };

        assert_eq!(slider.calculate_value(Point::new(10.0, 25.0), bounds), -1.0);
        assert_eq!(slider.calculate_value(Point::new(110.0, 25.0), bounds), 1.0);
        assert!((slider.calculate_value(Point::new(60.0, 25.0), bounds) - 0.0).abs() < 0.001);
    }

    #[test]
    fn calculate_value_snaps_to_step() {
        let slider = HorizontalSlider::new(-1.0..=1.0, 0.0, |value| value).step(0.1);
        let bounds = Rectangle {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 12.0,
        };

        assert!((slider.calculate_value(Point::new(75.0, 6.0), bounds) - 0.5).abs() < 0.001);
        assert!((slider.calculate_value(Point::new(76.0, 6.0), bounds) - 0.5).abs() < 0.001);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn update_publishes_clicked_value() {
        let mut slider =
            HorizontalSlider::new(-1.0..=1.0, 0.0, |value| value).width(Length::Fixed(100.0));
        let mut tree = test_tree_with_state(State::default());
        let node = layout::Node::new(Size::new(100.0, 14.0));
        let layout = Layout::new(&node);
        let mut messages = Vec::new();
        let mut shell = Shell::new(&mut messages);
        let renderer = ();
        let mut clipboard = clipboard::Null;
        let viewport = Rectangle::new(Point::ORIGIN, Size::new(100.0, 14.0));

        <HorizontalSlider<'_, f32> as Widget<f32, iced::Theme, ()>>::update(
            &mut slider,
            &mut tree,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            layout,
            mouse::Cursor::Available(Point::new(74.5, 7.0)),
            &renderer,
            &mut clipboard,
            &mut shell,
            &viewport,
        );

        assert_eq!(messages.len(), 1);
        assert!((messages[0] - 0.5).abs() < 0.001);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn update_double_click_resets_to_zero() {
        let mut slider =
            HorizontalSlider::new(-1.0..=1.0, 0.75, |value| value).width(Length::Fixed(100.0));
        let mut tree = test_tree_with_state(State {
            is_dragging: false,
            last_click_at: Some(Instant::now()),
        });
        let node = layout::Node::new(Size::new(100.0, 12.0));
        let layout = Layout::new(&node);
        let mut messages = Vec::new();
        let mut shell = Shell::new(&mut messages);
        let renderer = ();
        let mut clipboard = clipboard::Null;
        let viewport = Rectangle::new(Point::ORIGIN, Size::new(100.0, 12.0));

        <HorizontalSlider<'_, f32> as Widget<f32, iced::Theme, ()>>::update(
            &mut slider,
            &mut tree,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            layout,
            mouse::Cursor::Available(Point::new(90.0, 6.0)),
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
        let mut slider = HorizontalSlider::new(0.0..=1.0, 0.75, |value| value)
            .width(Length::Fixed(100.0))
            .double_click_reset(0.5);
        let mut tree = test_tree_with_state(State {
            is_dragging: false,
            last_click_at: Some(Instant::now()),
        });
        let node = layout::Node::new(Size::new(100.0, 12.0));
        let layout = Layout::new(&node);
        let mut messages = Vec::new();
        let mut shell = Shell::new(&mut messages);
        let renderer = ();
        let mut clipboard = clipboard::Null;
        let viewport = Rectangle::new(Point::ORIGIN, Size::new(100.0, 12.0));

        <HorizontalSlider<'_, f32> as Widget<f32, iced::Theme, ()>>::update(
            &mut slider,
            &mut tree,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            layout,
            mouse::Cursor::Available(Point::new(90.0, 6.0)),
            &renderer,
            &mut clipboard,
            &mut shell,
            &viewport,
        );

        assert_eq!(messages, vec![0.5]);
    }
}
