use maolan_widgets::iced::{
    Element, Event, Length, Rectangle, Renderer, Size, Theme, Vector,
    advanced::{
        Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
        widget::{Operation, Tree},
    },
};

pub struct DragPreview<'a, Message> {
    content: Element<'a, Message>,
    name: String,
    color: maolan_widgets::iced::Color,
    active: bool,
}

impl<'a, Message> DragPreview<'a, Message> {
    pub fn new(
        content: impl Into<Element<'a, Message>>,
        name: String,
        color: maolan_widgets::iced::Color,
        active: bool,
    ) -> Self {
        Self {
            content: content.into(),
            name,
            color,
            active,
        }
    }
}

impl<Message> Widget<Message, Theme, Renderer> for DragPreview<'_, Message> {
    fn tag(&self) -> maolan_widgets::iced::advanced::widget::tree::Tag {
        self.content.as_widget().tag()
    }

    fn state(&self) -> maolan_widgets::iced::advanced::widget::tree::State {
        self.content.as_widget().state()
    }

    fn children(&self) -> Vec<Tree> {
        self.content.as_widget().children()
    }

    fn diff(&self, tree: &mut Tree) {
        self.content.as_widget().diff(tree);
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content.as_widget_mut().layout(tree, renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(tree, layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.content.as_widget_mut().update(
            tree, event, layout, cursor, renderer, clipboard, shell, viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content
            .as_widget()
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content
            .as_widget()
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
    }

    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut Tree,
        layout: Layout<'a>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'a, Message, Theme, Renderer>> {
        let inner =
            self.content
                .as_widget_mut()
                .overlay(tree, layout, renderer, viewport, translation)?;
        if self.active {
            Some(overlay::Element::new(Box::new(PreviewOverlay {
                inner,
                name: &self.name,
                color: self.color,
            })))
        } else {
            Some(inner)
        }
    }
}

impl<'a, Message: 'a> From<DragPreview<'a, Message>> for Element<'a, Message> {
    fn from(value: DragPreview<'a, Message>) -> Self {
        Self::new(value)
    }
}

// Only the floating drag image is replaced; the source row draws normally.
struct PreviewOverlay<'a, Message> {
    inner: overlay::Element<'a, Message, Theme, Renderer>,
    name: &'a str,
    color: maolan_widgets::iced::Color,
}

impl<Message> overlay::Overlay<Message, Theme, Renderer> for PreviewOverlay<'_, Message> {
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        self.inner.as_overlay_mut().layout(renderer, bounds)
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
    ) {
        use maolan_widgets::iced::advanced::{renderer::Renderer as _, text, text::Renderer as _};
        use maolan_widgets::iced::{Border, Color, Font, Point, alignment};
        const ALPHA: f32 = 0.3;
        let bounds = layout.bounds();
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: Border {
                    color: Color::from_rgba(0.78, 0.87, 0.99, ALPHA),
                    width: 1.0,
                    radius: 8.0.into(),
                },
                ..Default::default()
            },
            Color {
                a: ALPHA,
                ..self.color
            },
        );
        renderer.fill_text(
            text::Text {
                content: self.name.to_owned(),
                bounds: Size::new((bounds.width - 16.0).max(0.0), 22.0),
                size: 13.0.into(),
                line_height: text::LineHeight::default(),
                font: Font::DEFAULT,
                align_x: text::Alignment::Left,
                align_y: alignment::Vertical::Top,
                shaping: text::Shaping::Basic,
                wrapping: text::Wrapping::None,
            },
            Point::new(bounds.x + 8.0, bounds.y + 4.0),
            Color::from_rgba(0.92, 0.95, 1.0, ALPHA),
            bounds,
        );
    }
}
