use crate::{message::Message, state::Modulator, state::State};
use iced::{
    Element, Length,
    widget::{canvas, container},
};

/// Wrap any canvas program in a fill container.
pub fn view<'a, P>(program: P) -> Element<'a, Message>
where
    P: canvas::Program<Message> + 'a,
{
    container(canvas(program).width(Length::Fill).height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// Render the track/folder connections graph.
pub fn tracks(
    state: State,
    focus: Option<String>,
    selected_modulator: Option<Modulator>,
) -> Element<'static, Message> {
    view(crate::connections::tracks::Graph::new_with_focus(
        state,
        focus,
        selected_modulator,
    ))
}

/// Render the per-track plugin graph.
pub fn plugin_graph(state: State) -> Element<'static, Message> {
    view(crate::connections::plugins::Graph::new(state))
}
