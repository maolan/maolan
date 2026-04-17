use crate::consts::DOUBLE_CLICK;
use iced::advanced::Shell;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Tree, Widget};
use iced::mouse;
use iced::{Border, Color, Element, Event, Length, Point, Rectangle, Size};
use std::time::Instant;

pub struct Slider<'a, Message> {
    range: std::ops::RangeInclusive<f32>,
    value: f32,
    on_change: Box<dyn Fn(f32) -> Message + 'a>,
    width: Length,
    height: Length,
    handle_height: f32,
    step: Option<f32>,
    double_click_reset: f32,
}

impl<'a, Message> Slider<'a, Message> {
    pub fn new<F>(range: std::ops::RangeInclusive<f32>, value: f32, on_change: F) -> Self
    where
        F: Fn(f32) -> Message + 'a,
    {
        Self {
            range,
            value,
            on_change: Box::new(on_change),
            width: Length::Fixed(14.0),
            height: Length::Fixed(300.0),
            handle_height: 2.0,
            step: None,
            double_click_reset: 0.0,
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
}

pub fn slider<'a, Message, F>(
    range: std::ops::RangeInclusive<f32>,
    value: f32,
    on_change: F,
) -> Slider<'a, Message>
where
    F: Fn(f32) -> Message + 'a,
{
    Slider::new(range, value, on_change)
}

#[derive(Default)]
struct State {
    is_dragging: bool,
    last_click_at: Option<Instant>,
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer> for Slider<'a, Message>
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
        let value_bounds_y = bounds.y + (self.handle_height / 2.0);
        let value_bounds_height = bounds.height - self.handle_height;
        let normalized =
            (self.value - self.range.start()) / (self.range.end() - self.range.start());
        let handle_offset =
            (value_bounds_y + (value_bounds_height - twice_border) * (1.0 - normalized)).round();

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

        let border_radius = 2.0;
        let handle_filled_gap = 1.0;

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

        let filled_y_start = handle_offset + self.handle_height + handle_filled_gap;
        let filled_height = bounds.y + bounds.height - filled_y_start;

        if filled_height > 0.0 {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: bounds.x,
                        y: filled_y_start,
                        width: bounds.width,
                        height: filled_height,
                    },
                    border: Border {
                        radius: border_radius.into(),
                        width: border_width,
                        color: Color::TRANSPARENT,
                    },
                    ..Default::default()
                },
                filled_color,
            );
        }

        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle {
                    x: bounds.x,
                    y: handle_offset,
                    width: bounds.width,
                    height: self.handle_height + twice_border,
                },
                border: Border {
                    radius: border_radius.into(),
                    width: border_width,
                    color: Color::TRANSPARENT,
                },
                ..Default::default()
            },
            handle_color,
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
                state.is_dragging = true;
                if is_double_click {
                    let default_value = self
                        .double_click_reset
                        .clamp(*self.range.start(), *self.range.end());
                    shell.publish((self.on_change)(default_value));
                } else if let Some(cursor_position) = cursor.position() {
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

impl<'a, Message> Slider<'a, Message> {
    fn calculate_value(&self, cursor_position: Point, bounds: Rectangle) -> f32 {
        let y = cursor_position.y - bounds.y;
        let normalized = 1.0 - (y / bounds.height).clamp(0.0, 1.0);
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

impl<'a, Message, Theme, Renderer> From<Slider<'a, Message>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: renderer::Renderer + 'a,
{
    fn from(slider: Slider<'a, Message>) -> Self {
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
        let slider = Slider::new(0.0..=1.0, 0.5, |value| value);
        let bounds = Rectangle {
            x: 10.0,
            y: 20.0,
            width: 14.0,
            height: 100.0,
        };

        assert_eq!(slider.calculate_value(Point::new(15.0, 20.0), bounds), 1.0);
        assert_eq!(slider.calculate_value(Point::new(15.0, 120.0), bounds), 0.0);
        assert!((slider.calculate_value(Point::new(15.0, 70.0), bounds) - 0.5).abs() < 0.001);
    }

    #[test]
    fn calculate_value_snaps_to_step() {
        let slider = Slider::new(-90.0..=20.0, 0.0, |value| value).step(1.0);
        let bounds = Rectangle {
            x: 0.0,
            y: 0.0,
            width: 14.0,
            height: 110.0,
        };

        assert_eq!(slider.calculate_value(Point::new(7.0, 10.4), bounds), 10.0);
        assert_eq!(slider.calculate_value(Point::new(7.0, 10.6), bounds), 9.0);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn update_publishes_clicked_value() {
        let mut slider = Slider::new(0.0..=1.0, 0.5, |value| value).height(Length::Fixed(100.0));
        let mut tree = test_tree_with_state(State::default());
        let node = layout::Node::new(Size::new(14.0, 100.0));
        let layout = Layout::new(&node);
        let mut messages = Vec::new();
        let mut shell = Shell::new(&mut messages);
        let renderer = ();
        let mut clipboard = clipboard::Null;
        let viewport = Rectangle::new(Point::ORIGIN, Size::new(14.0, 100.0));

        <Slider<'_, f32> as Widget<f32, iced::Theme, ()>>::update(
            &mut slider,
            &mut tree,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            layout,
            mouse::Cursor::Available(Point::new(7.0, 25.0)),
            &renderer,
            &mut clipboard,
            &mut shell,
            &viewport,
        );

        assert_eq!(messages.len(), 1);
        assert!((messages[0] - 0.75).abs() < 0.01);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn update_double_click_resets_to_zero() {
        let mut slider = Slider::new(-90.0..=20.0, 6.0, |value| value).height(Length::Fixed(110.0));
        let mut tree = test_tree_with_state(State {
            is_dragging: false,
            last_click_at: Some(Instant::now()),
        });
        let node = layout::Node::new(Size::new(14.0, 110.0));
        let layout = Layout::new(&node);
        let mut messages = Vec::new();
        let mut shell = Shell::new(&mut messages);
        let renderer = ();
        let mut clipboard = clipboard::Null;
        let viewport = Rectangle::new(Point::ORIGIN, Size::new(14.0, 110.0));

        <Slider<'_, f32> as Widget<f32, iced::Theme, ()>>::update(
            &mut slider,
            &mut tree,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            layout,
            mouse::Cursor::Available(Point::new(7.0, 30.0)),
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
        let mut slider = Slider::new(0.0..=1.0, 0.2, |value| value)
            .height(Length::Fixed(110.0))
            .double_click_reset(0.75);
        let mut tree = test_tree_with_state(State {
            is_dragging: false,
            last_click_at: Some(Instant::now()),
        });
        let node = layout::Node::new(Size::new(14.0, 110.0));
        let layout = Layout::new(&node);
        let mut messages = Vec::new();
        let mut shell = Shell::new(&mut messages);
        let renderer = ();
        let mut clipboard = clipboard::Null;
        let viewport = Rectangle::new(Point::ORIGIN, Size::new(14.0, 110.0));

        <Slider<'_, f32> as Widget<f32, iced::Theme, ()>>::update(
            &mut slider,
            &mut tree,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            layout,
            mouse::Cursor::Available(Point::new(7.0, 30.0)),
            &renderer,
            &mut clipboard,
            &mut shell,
            &viewport,
        );

        assert_eq!(messages, vec![0.75]);
    }
}
