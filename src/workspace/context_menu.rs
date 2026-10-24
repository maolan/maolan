use crate::message::Message;
use maolan_widgets::iced::{
    Element, Event, Length, Point, Rectangle, Renderer, Size, Theme, Vector,
    advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer, widget::Tree},
};

/// Keeps menu layout independent of the space below its timeline anchor.
pub(super) struct ContextMenu<'a> {
    content: Element<'a, Message>,
    anchor: Point,
}

impl<'a> ContextMenu<'a> {
    pub(super) fn new(content: Element<'a, Message>, anchor: Point) -> Self {
        Self { content, anchor }
    }
}

impl Widget<Message, Theme, Renderer> for ContextMenu<'_> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Shrink, Length::Shrink)
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        _limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(Size::ZERO)
    }

    fn draw(
        &self,
        _tree: &Tree,
        _renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
    }

    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut Tree,
        layout: Layout<'a>,
        _renderer: &Renderer,
        _viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'a, Message, Theme, Renderer>> {
        let origin = layout.position() + translation;
        Some(overlay::Element::new(Box::new(MenuOverlay {
            content: &mut self.content,
            tree: &mut tree.children[0],
            anchor: origin + Vector::new(self.anchor.x, self.anchor.y),
        })))
    }
}

fn menu_position(anchor: Point, menu: Size, window: Size) -> Point {
    Point::new(
        anchor.x.clamp(0.0, (window.width - menu.width).max(0.0)),
        anchor.y.clamp(0.0, (window.height - menu.height).max(0.0)),
    )
}

struct MenuOverlay<'a, 'b> {
    content: &'a mut Element<'b, Message>,
    tree: &'a mut Tree,
    anchor: Point,
}

impl overlay::Overlay<Message, Theme, Renderer> for MenuOverlay<'_, '_> {
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        // Use the full window, not the remaining height of the bottom track.
        let node = self.content.as_widget_mut().layout(
            self.tree,
            renderer,
            &layout::Limits::new(Size::ZERO, bounds),
        );
        let position = menu_position(self.anchor, node.size(), bounds);
        node.move_to(position)
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        self.content.as_widget().draw(
            self.tree,
            renderer,
            theme,
            style,
            layout,
            cursor,
            &layout.bounds(),
        );
    }

    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        self.content.as_widget_mut().update(
            self.tree,
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            &layout.bounds(),
        );
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            self.tree,
            layout,
            cursor,
            &layout.bounds(),
            renderer,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bottom_track_menu_moves_up_and_right_edge_menu_moves_left() {
        assert_eq!(
            menu_position(
                Point::new(790.0, 590.0),
                Size::new(210.0, 300.0),
                Size::new(800.0, 600.0)
            ),
            Point::new(590.0, 300.0)
        );
    }

    #[test]
    fn menu_keeps_anchor_when_it_fits() {
        assert_eq!(
            menu_position(
                Point::new(100.0, 120.0),
                Size::new(210.0, 300.0),
                Size::new(800.0, 600.0)
            ),
            Point::new(100.0, 120.0)
        );
    }

    #[test]
    fn menu_handles_scrolled_anchors_and_small_windows() {
        assert_eq!(
            menu_position(
                Point::new(-20.0, -40.0),
                Size::new(210.0, 300.0),
                Size::new(100.0, 200.0)
            ),
            Point::ORIGIN
        );
    }
}
