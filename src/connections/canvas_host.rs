use crate::message::Message;
use iced::{
    Element, Length,
    widget::{canvas, container},
};

pub struct CanvasHost<G> {
    graph: G,
}

impl<G> CanvasHost<G> {
    pub fn new(graph: G) -> Self {
        Self { graph }
    }

    pub fn update(&mut self, _message: Message) {}

    pub fn view(&self) -> Element<'_, Message>
    where
        G: canvas::Program<Message>,
    {
        container(canvas(&self.graph).width(Length::Fill).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}
