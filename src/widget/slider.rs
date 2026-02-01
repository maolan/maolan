use iced::advanced::Shell;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Tree, Widget};
use iced::mouse;
use iced::widget::image;
use iced::{Border, Color, Element, Event, Length, Point, Rectangle, Size};

pub struct Slider<'a, Message> {
    range: std::ops::RangeInclusive<f32>,
    value: f32,
    on_change: Box<dyn Fn(f32) -> Message + 'a>,
    width: Length,
    height: Length,
    image_handle: Option<image::Handle>,
    image_bounds: Rectangle,
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
            image_handle: None,
            image_bounds: Rectangle {
                x: -10.0,
                y: -19.0,
                width: 20.0,
                height: 38.0,
            },
            handle_height: 38.0,
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

    pub fn texture(mut self, handle: image::Handle, bounds: Rectangle, handle_height: f32) -> Self {
        self.image_handle = Some(handle);
        self.image_bounds = bounds;
        self.handle_height = handle_height;
        self
    }

    pub fn dark_rect_style(mut self) -> Self {
        // Use None for image_handle to trigger rect style rendering
        self.image_handle = None;
        self.handle_height = 4.0;
        self
    }
}

#[derive(Default)]
struct State {
    is_dragging: bool,
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

        // Calculate handle position
        let border_width = 1.0;
        let twice_border = border_width * 2.0;
        let value_bounds_y = bounds.y + (self.handle_height / 2.0);
        let value_bounds_height = bounds.height - self.handle_height;
        let normalized =
            (self.value - self.range.start()) / (self.range.end() - self.range.start());
        let handle_offset =
            (value_bounds_y + (value_bounds_height - twice_border) * (1.0 - normalized)).round();

        // Check which style to render
        if self.image_handle.is_some() {
            // === TEXTURE STYLE ===

            // Classic rail colors
            let left_rail_color = Color::from_rgba(0.26, 0.26, 0.26, 0.75);
            let right_rail_color = Color::from_rgba(0.56, 0.56, 0.56, 0.75);

            let rail_padding = 12.0;
            let left_rail_width = 1.0;
            let right_rail_width = 1.0;
            let full_rail_width = left_rail_width + right_rail_width;
            let rail_start_x = (bounds.x + ((bounds.width - full_rail_width) / 2.0)).round();
            let rail_y = bounds.y + rail_padding;
            let rail_height = bounds.height - (rail_padding * 2.0);

            // Draw left rail
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: rail_start_x,
                        y: rail_y,
                        width: left_rail_width,
                        height: rail_height,
                    },
                    border: Border::default(),
                    ..Default::default()
                },
                left_rail_color,
            );

            // Draw right rail
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: rail_start_x + left_rail_width,
                        y: rail_y,
                        width: right_rail_width,
                        height: rail_height,
                    },
                    border: Border::default(),
                    ..Default::default()
                },
                right_rail_color,
            );

            // Draw textured handle
            let center_x = bounds.center_x();
            let image_x = (center_x + self.image_bounds.x).round();
            let image_y = (handle_offset + self.image_bounds.y).round();
            let texture_color = Color::from_rgb(0.85, 0.87, 0.90);

            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: image_x,
                        y: image_y,
                        width: self.image_bounds.width,
                        height: self.image_bounds.height,
                    },
                    border: Border {
                        radius: 3.0.into(),
                        width: 1.0,
                        color: Color::from_rgb(0.6, 0.6, 0.6),
                    },
                    ..Default::default()
                },
                texture_color,
            );

            let highlight_y = image_y + self.image_bounds.height / 2.0;
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: image_x + 2.0,
                        y: highlight_y - 1.0,
                        width: self.image_bounds.width - 4.0,
                        height: 2.0,
                    },
                    border: Border::default(),
                    ..Default::default()
                },
                Color::from_rgb(0.75, 0.78, 0.82),
            );
        } else if self.handle_height <= 10.0 {
            // === RECT STYLE (Dark Theme) ===

            // Dark theme colors from iced_audio examples
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

            // Draw background rectangle
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

            // Draw filled portion (from handle to bottom)
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

            // Draw handle
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
        } else {
            // === CLASSIC STYLE ===

            let left_rail_color = Color::from_rgba(0.26, 0.26, 0.26, 0.75);
            let right_rail_color = Color::from_rgba(0.56, 0.56, 0.56, 0.75);
            let handle_color = Color::from_rgb(0.97, 0.97, 0.97);
            let border_color = Color::from_rgb(0.315, 0.315, 0.315);
            let notch_color = Color::from_rgb(0.315, 0.315, 0.315);
            let notch_width = 4.0;

            let rail_padding = 12.0;
            let left_rail_width = 1.0;
            let right_rail_width = 1.0;
            let full_rail_width = left_rail_width + right_rail_width;
            let rail_start_x = (bounds.x + ((bounds.width - full_rail_width) / 2.0)).round();
            let rail_y = bounds.y + rail_padding;
            let rail_height = bounds.height - (rail_padding * 2.0);

            // Draw rails
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: rail_start_x,
                        y: rail_y,
                        width: left_rail_width,
                        height: rail_height,
                    },
                    border: Border::default(),
                    ..Default::default()
                },
                left_rail_color,
            );

            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: rail_start_x + left_rail_width,
                        y: rail_y,
                        width: right_rail_width,
                        height: rail_height,
                    },
                    border: Border::default(),
                    ..Default::default()
                },
                right_rail_color,
            );

            // Draw handle
            let handle_y_pos = (value_bounds_y + value_bounds_height * (1.0 - normalized)).round();
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: bounds.x,
                        y: handle_y_pos,
                        width: bounds.width,
                        height: self.handle_height,
                    },
                    border: Border {
                        radius: 2.0.into(),
                        width: 1.0,
                        color: border_color,
                    },
                    ..Default::default()
                },
                handle_color,
            );

            // Draw notch
            let notch_y = (handle_y_pos + (self.handle_height / 2.0) - (notch_width / 2.0)).round();
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: bounds.x,
                        y: notch_y,
                        width: bounds.width,
                        height: notch_width,
                    },
                    border: Border::default(),
                    ..Default::default()
                },
                notch_color,
            );
        }
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
                    state.is_dragging = true;
                    if let Some(cursor_position) = cursor.position() {
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
                if state.is_dragging {
                    if let Some(cursor_position) = cursor.position() {
                        let new_value = self.calculate_value(cursor_position, bounds);
                        shell.publish((self.on_change)(new_value));
                    }
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
