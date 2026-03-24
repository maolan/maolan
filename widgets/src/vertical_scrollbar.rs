use iced::advanced::Shell;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Tree, Widget};
use iced::mouse;
use iced::{Border, Color, Element, Event, Length, Point, Rectangle, Size};

pub struct VerticalScrollbar<'a, Message> {
    content_height: f32,
    value: f32,
    on_change: Box<dyn Fn(f32) -> Message + 'a>,
    width: Length,
    height: Length,
    min_handle_height: f32,
}

impl<'a, Message> VerticalScrollbar<'a, Message> {
    pub fn new<F>(content_height: f32, value: f32, on_change: F) -> Self
    where
        F: Fn(f32) -> Message + 'a,
    {
        Self {
            content_height: content_height.max(1.0),
            value,
            on_change: Box::new(on_change),
            width: Length::Fixed(16.0),
            height: Length::Fill,
            min_handle_height: 12.0,
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

    fn normalized_value(&self) -> f32 {
        self.value.clamp(0.0, 1.0)
    }

    fn handle_height(&self, bounds: Rectangle) -> f32 {
        if self.content_height <= bounds.height {
            bounds.height
        } else {
            (bounds.height * (bounds.height / self.content_height))
                .clamp(self.min_handle_height, bounds.height)
        }
    }

    fn handle_bounds(&self, bounds: Rectangle) -> Rectangle {
        let handle_height = self.handle_height(bounds);
        let travel = (bounds.height - handle_height).max(0.0);
        let handle_y = bounds.y + travel * self.normalized_value();
        Rectangle {
            x: bounds.x,
            y: handle_y,
            width: bounds.width,
            height: handle_height,
        }
    }

    fn drag_value(&self, cursor_position: Point, bounds: Rectangle, drag_offset_y: f32) -> f32 {
        let handle_height = self.handle_height(bounds);
        let travel = (bounds.height - handle_height).max(0.0);
        if travel <= f32::EPSILON {
            return 0.0;
        }
        let handle_top = (cursor_position.y - bounds.y - drag_offset_y).clamp(0.0, travel);
        (handle_top / travel).clamp(0.0, 1.0)
    }

    fn page_step(&self, bounds: Rectangle) -> f32 {
        let max_scroll = (self.content_height - bounds.height).max(0.0);
        if max_scroll <= f32::EPSILON {
            1.0
        } else {
            (bounds.height / max_scroll).clamp(0.0, 1.0)
        }
    }

    fn page_click_value(&self, cursor_position: Point, bounds: Rectangle) -> f32 {
        let handle_bounds = self.handle_bounds(bounds);
        let page_step = self.page_step(bounds);
        let current = self.normalized_value();
        if cursor_position.y < handle_bounds.y {
            (current - page_step).clamp(0.0, 1.0)
        } else {
            (current + page_step).clamp(0.0, 1.0)
        }
    }
}

#[derive(Default)]
struct State {
    is_dragging: bool,
    drag_offset_y: f32,
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for VerticalScrollbar<'a, Message>
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
        tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let state = tree.state.downcast_ref::<State>();
        let handle_bounds = self.handle_bounds(bounds);
        let handle_hovered = cursor.is_over(handle_bounds);
        let border_width = 1.0;
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
        let handle_color = if state.is_dragging || handle_hovered {
            Color::from_rgb(
                0x75 as f32 / 255.0,
                0xC2 as f32 / 255.0,
                0xFF as f32 / 255.0,
            )
        } else {
            Color::from_rgb(
                0x8B as f32 / 255.0,
                0x90 as f32 / 255.0,
                0x97 as f32 / 255.0,
            )
        };
        let border_radius = 2.0;

        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: Border {
                    radius: border_radius.into(),
                    width: border_width,
                    color: border_color,
                },
                ..Default::default()
            },
            back_color,
        );

        renderer.fill_quad(
            renderer::Quad {
                bounds: handle_bounds,
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
                if cursor.is_over(bounds)
                    && let Some(cursor_position) = cursor.position()
                {
                    let handle_bounds = self.handle_bounds(bounds);
                    if cursor_position.y >= handle_bounds.y
                        && cursor_position.y <= handle_bounds.y + handle_bounds.height
                    {
                        state.is_dragging = true;
                        state.drag_offset_y =
                            (cursor_position.y - handle_bounds.y).clamp(0.0, handle_bounds.height);
                    } else {
                        shell.publish((self.on_change)(
                            self.page_click_value(cursor_position, bounds),
                        ));
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.is_dragging = false;
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if state.is_dragging
                    && let Some(cursor_position) = cursor.position()
                {
                    shell.publish((self.on_change)(self.drag_value(
                        cursor_position,
                        bounds,
                        state.drag_offset_y,
                    )));
                }
            }
            _ => {}
        }
    }
}

impl<'a, Message, Theme, Renderer> From<VerticalScrollbar<'a, Message>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: renderer::Renderer + 'a,
{
    fn from(scrollbar: VerticalScrollbar<'a, Message>) -> Self {
        Self::new(scrollbar)
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

    fn test_layout(width: f32, height: f32) -> Layout<'static> {
        let node = Box::leak(Box::new(layout::Node::new(Size::new(width, height))));
        Layout::new(node)
    }

    #[cfg(debug_assertions)]
    #[test]
    fn click_below_handle_pages_down_by_one_viewport() {
        let mut scrollbar =
            VerticalScrollbar::new(300.0, 0.25, |value| value).height(Length::Fixed(100.0));
        let mut tree = Tree {
            tag: widget::tree::Tag::of::<State>(),
            state: widget::tree::State::new(State::default()),
            children: Vec::new(),
        };
        let layout = test_layout(16.0, 100.0);
        let mut messages = Vec::new();
        let mut shell = Shell::new(&mut messages);
        let renderer = ();
        let mut clipboard = clipboard::Null;
        let viewport = Rectangle::new(Point::ORIGIN, Size::new(16.0, 100.0));

        <VerticalScrollbar<'_, f32> as Widget<f32, iced::Theme, ()>>::update(
            &mut scrollbar,
            &mut tree,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            layout,
            mouse::Cursor::Available(Point::new(8.0, 90.0)),
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
    fn click_above_handle_pages_up_by_one_viewport() {
        let mut scrollbar =
            VerticalScrollbar::new(300.0, 0.75, |value| value).height(Length::Fixed(100.0));
        let mut tree = Tree {
            tag: widget::tree::Tag::of::<State>(),
            state: widget::tree::State::new(State::default()),
            children: Vec::new(),
        };
        let layout = test_layout(16.0, 100.0);
        let mut messages = Vec::new();
        let mut shell = Shell::new(&mut messages);
        let renderer = ();
        let mut clipboard = clipboard::Null;
        let viewport = Rectangle::new(Point::ORIGIN, Size::new(16.0, 100.0));

        <VerticalScrollbar<'_, f32> as Widget<f32, iced::Theme, ()>>::update(
            &mut scrollbar,
            &mut tree,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            layout,
            mouse::Cursor::Available(Point::new(8.0, 10.0)),
            &renderer,
            &mut clipboard,
            &mut shell,
            &viewport,
        );

        assert_eq!(messages.len(), 1);
        assert!((messages[0] - 0.25).abs() < 0.01);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn dragging_handle_uses_grab_offset_instead_of_jumping() {
        let mut scrollbar =
            VerticalScrollbar::new(400.0, 0.5, |value| value).height(Length::Fixed(100.0));
        let mut tree = Tree {
            tag: widget::tree::Tag::of::<State>(),
            state: widget::tree::State::new(State::default()),
            children: Vec::new(),
        };
        let layout = test_layout(16.0, 100.0);
        let renderer = ();
        let mut clipboard = clipboard::Null;
        let viewport = Rectangle::new(Point::ORIGIN, Size::new(16.0, 100.0));
        let mut messages = Vec::new();

        {
            let mut shell = Shell::new(&mut messages);
            <VerticalScrollbar<'_, f32> as Widget<f32, iced::Theme, ()>>::update(
                &mut scrollbar,
                &mut tree,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                layout,
                mouse::Cursor::Available(Point::new(8.0, 42.0)),
                &renderer,
                &mut clipboard,
                &mut shell,
                &viewport,
            );
        }

        assert!(messages.is_empty());

        {
            let mut shell = Shell::new(&mut messages);
            <VerticalScrollbar<'_, f32> as Widget<f32, iced::Theme, ()>>::update(
                &mut scrollbar,
                &mut tree,
                &Event::Mouse(mouse::Event::CursorMoved {
                    position: Point::new(8.0, 52.0),
                }),
                layout,
                mouse::Cursor::Available(Point::new(8.0, 52.0)),
                &renderer,
                &mut clipboard,
                &mut shell,
                &viewport,
            );
        }

        assert_eq!(messages.len(), 1);
        assert!((messages[0] - 0.6333).abs() < 0.02);
    }
}
