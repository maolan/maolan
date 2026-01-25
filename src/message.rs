use iced::widget::pane_grid;
use maolan_engine::message::Action;

#[derive(Debug, Clone)]
pub enum Show {
    AddTrack,
}

#[derive(Debug, Clone)]
pub enum Message {
    Debug(String),

    Request(Action),
    Response(Result<Action, String>),

    PaneResized(pane_grid::ResizeEvent),
    Show(Show),
}
