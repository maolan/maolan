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
            handle_width: 10.0,
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
        let rgb = |r: u8, g: u8, b: u8| Color::from_rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);

        let border_width = 1.0;
        let handle_width = self.handle_width.min((bounds.width - 4.0).max(8.0));
        let groove_height = (bounds.height - 8.0).clamp(2.0, 4.0);
        let groove_y = bounds.y + (bounds.height - groove_height) * 0.5;
        let groove_x = bounds.x + 2.0;
        let groove_width = (bounds.width - 4.0).max(8.0);
        let normalized =
            (self.value - self.range.start()) / (self.range.end() - self.range.start());
        let travel_width = (groove_width - handle_width).max(1.0);
        let handle_offset = (groove_x + travel_width * normalized).round();

        let shell_color = rgb(31, 37, 50);
        let shell_border = rgb(58, 67, 86);
        let groove_color = rgb(11, 15, 23);
        let groove_border = rgb(69, 79, 101);
        let filled_color = Color {
            r: 0.38,
            g: 0.49,
            b: 0.67,
            a: 0.55,
        };
        let handle_color = rgb(125, 139, 168);
        let handle_border = rgb(85, 95, 115);
        let handle_highlight = rgb(201, 210, 227);

        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: Border {
                    radius: 4.0.into(),
                    width: border_width,
                    color: shell_border,
                },
                ..Default::default()
            },
            shell_color,
        );

        let groove_bounds = Rectangle {
            x: groove_x,
            y: groove_y,
            width: groove_width,
            height: groove_height,
        };

        renderer.fill_quad(
            renderer::Quad {
                bounds: groove_bounds,
                border: Border {
                    radius: 2.0.into(),
                    width: border_width,
                    color: groove_border,
                },
                ..Default::default()
            },
            groove_color,
        );

        let center_x = groove_x + groove_width * 0.5;
        let handle_center = handle_offset + handle_width * 0.5;
        if handle_center >= center_x {
            let filled_x_start = center_x;
            let filled_width = (handle_center - center_x).max(0.0);
            if filled_width > 0.0 {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: Rectangle {
                            x: filled_x_start,
                            y: groove_y + 1.0,
                            width: filled_width,
                            height: (groove_height - 2.0).max(1.0),
                        },
                        border: Border {
                            radius: 1.0.into(),
                            width: 0.0,
                            color: Color::TRANSPARENT,
                        },
                        ..Default::default()
                    },
                    filled_color,
                );
            }
        } else {
            let filled_x_start = handle_center.min(center_x);
            let filled_width = (center_x - filled_x_start).max(0.0);
            if filled_width > 0.0 {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: Rectangle {
                            x: filled_x_start,
                            y: groove_y + 1.0,
                            width: filled_width,
                            height: (groove_height - 2.0).max(1.0),
                        },
                        border: Border {
                            radius: 1.0.into(),
                            width: 0.0,
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
                    y: bounds.y + 1.0,
                    width: handle_width,
                    height: (bounds.height - 2.0).max(6.0),
                },
                border: Border {
                    radius: 4.0.into(),
                    width: border_width,
                    color: handle_border,
                },
                ..Default::default()
            },
            handle_color,
        );

        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle {
                    x: handle_offset + 1.0,
                    y: bounds.y + 2.0,
                    width: (handle_width - 2.0).max(2.0),
                    height: 1.0,
                },
                border: Border::default(),
                ..Default::default()
            },
            handle_highlight,
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
