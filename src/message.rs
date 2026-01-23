use iced::widget::pane_grid;
use maolan_engine::message::Action;

#[derive(Debug, Clone)]
pub enum Message {
    Debug(String),

    Request(Action),
    Response(Action),

    PaneResized(pane_grid::ResizeEvent),
    TrackGain(String, f32),
    TrackLevel(String, f32),
}
