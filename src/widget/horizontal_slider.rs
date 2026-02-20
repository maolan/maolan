use iced::advanced::Shell;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Tree, Widget};
use iced::mouse;
use iced::{Border, Color, Element, Event, Length, Point, Rectangle, Size};
use std::time::Instant;

use crate::ui_timing::DOUBLE_CLICK;

pub struct HorizontalSlider<'a, Message> {
    range: std::ops::RangeInclusive<f32>,
    value: f32,
    on_change: Box<dyn Fn(f32) -> Message + 'a>,
    width: Length,
    height: Length,
    handle_width: f32,
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
            width: Length::Fixed(120.0),
            height: Length::Fixed(14.0),
            handle_width: 1.0,
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
}

#[derive(Default)]
struct State {
    is_dragging: bool,
    last_click_at: Option<Instant>,
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer> for HorizontalSlider<'a, Message>
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
        let value_bounds_width = bounds.width - self.handle_width;
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

        let center_x = bounds.x + bounds.width * 0.5;
        if handle_offset >= center_x {
            let filled_x_start = center_x;
            let filled_width = (handle_offset - center_x + handle_filled_gap).max(0.0);
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
                    filled_color,
                );
            }
        } else {
            let filled_x_start = (handle_offset + self.handle_width + handle_filled_gap).max(bounds.x);
            let filled_width = (center_x - filled_x_start).max(0.0);
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
                    filled_color,
                );
            }
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
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if cursor.is_over(bounds) {
                    let now = Instant::now();
                    let is_double_click = state
                        .last_click_at
                        .is_some_and(|last| now.duration_since(last) <= DOUBLE_CLICK);
                    state.last_click_at = Some(now);
                    state.is_dragging = true;
                    if is_double_click {
                        let default_value = 0.0_f32.clamp(*self.range.start(), *self.range.end());
                        shell.publish((self.on_change)(default_value));
                    } else if let Some(cursor_position) = cursor.position() {
                        let new_value = self.calculate_value(cursor_position, bounds);
                        shell.publish((self.on_change)(new_value));
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.is_dragging {
                    state.is_dragging = false;
                }
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
        let x = cursor_position.x - bounds.x;
        let normalized = (x / bounds.width).clamp(0.0, 1.0);
        let value = self.range.start() + normalized * (self.range.end() - self.range.start());
        value.clamp(*self.range.start(), *self.range.end())
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
