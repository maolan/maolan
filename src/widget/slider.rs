use iced::advanced::Shell;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Tree, Widget};
use iced::mouse;
use iced::{Border, Color, Element, Event, Length, Point, Rectangle, Size};
use std::time::Instant;

use crate::ui_timing::DOUBLE_CLICK;

pub struct Slider<'a, Message> {
    range: std::ops::RangeInclusive<f32>,
    value: f32,
    on_change: Box<dyn Fn(f32) -> Message + 'a>,
    width: Length,
    height: Length,
    handle_height: f32,
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
            handle_height: 10.0,
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
        let rgb =
            |r: u8, g: u8, b: u8| Color::from_rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);

        let border_width = 1.0;
        let handle_height = self.handle_height.min((bounds.height - 10.0).max(8.0));
        let slot_width = (bounds.width - 8.0).clamp(5.0, 7.0);
        let slot_x = (bounds.x + (bounds.width - slot_width) * 0.5).round();
        let slot_y = bounds.y + 3.0;
        let slot_height = (bounds.height - 6.0).max(12.0);
        let normalized =
            (self.value - self.range.start()) / (self.range.end() - self.range.start());
        let travel_height = (slot_height - handle_height).max(1.0);
        let handle_offset = (slot_y + travel_height * (1.0 - normalized)).round();

        let rail_dark = rgb(24, 28, 40);
        let rail_light = rgb(78, 88, 111);
        let slot_color = rgb(56, 64, 83);
        let slot_inner = rgb(93, 104, 130);
        let slot_shadow = rgb(18, 22, 32);
        let filled_color = Color {
            r: 0.78,
            g: 0.82,
            b: 0.90,
            a: 0.12,
        };
        let handle_color = rgb(124, 136, 160);
        let handle_border = rgb(71, 79, 98);
        let handle_shadow = rgb(53, 60, 76);
        let handle_highlight = rgb(210, 218, 233);

        let slot_bounds = Rectangle {
            x: slot_x,
            y: slot_y,
            width: slot_width,
            height: slot_height,
        };

        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle {
                    x: slot_x - 1.0,
                    y: slot_y - 1.0,
                    width: 1.0,
                    height: slot_height + 2.0,
                },
                border: Border {
                    radius: 0.0.into(),
                    width: 0.0,
                    color: Color::TRANSPARENT,
                },
                ..Default::default()
            },
            rail_light,
        );

        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle {
                    x: slot_x + slot_width,
                    y: slot_y - 1.0,
                    width: 1.0,
                    height: slot_height + 2.0,
                },
                border: Border {
                    radius: 0.0.into(),
                    width: 0.0,
                    color: Color::TRANSPARENT,
                },
                ..Default::default()
            },
            rail_dark,
        );

        renderer.fill_quad(
            renderer::Quad {
                bounds: slot_bounds,
                border: Border {
                    radius: 2.0.into(),
                    width: border_width,
                    color: slot_shadow,
                },
                ..Default::default()
            },
            slot_color,
        );

        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle {
                    x: slot_x + 1.0,
                    y: slot_y + 1.0,
                    width: (slot_width - 2.0).max(1.0),
                    height: 1.0,
                },
                border: Border::default(),
                ..Default::default()
            },
            slot_inner,
        );

        let filled_y_start = (handle_offset + handle_height * 0.6).min(slot_y + slot_height);
        let filled_height = (slot_y + slot_height - filled_y_start).max(0.0);

        if filled_height > 0.0 {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: slot_x + 1.0,
                        y: filled_y_start,
                        width: (slot_width - 2.0).max(1.0),
                        height: filled_height,
                    },
                    border: Border {
                        radius: 2.0.into(),
                        width: 0.0,
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
                    height: handle_height,
                },
                border: Border {
                    radius: 3.0.into(),
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
                    x: bounds.x + 1.0,
                    y: handle_offset + 1.0,
                    width: (bounds.width - 2.0).max(2.0),
                    height: 1.0,
                },
                border: Border::default(),
                ..Default::default()
            },
            handle_highlight,
        );

        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle {
                    x: bounds.x + 1.0,
                    y: handle_offset + handle_height - 2.0,
                    width: (bounds.width - 2.0).max(2.0),
                    height: 1.0,
                },
                border: Border::default(),
                ..Default::default()
            },
            handle_shadow,
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

impl<'a, Message> Slider<'a, Message> {
    fn calculate_value(&self, cursor_position: Point, bounds: Rectangle) -> f32 {
        let y = cursor_position.y - bounds.y;
        let normalized = 1.0 - (y / bounds.height).clamp(0.0, 1.0);
        let value = self.range.start() + normalized * (self.range.end() - self.range.start());
        value.clamp(*self.range.start(), *self.range.end())
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
