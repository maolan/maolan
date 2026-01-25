use iced::widget::pane_grid;
use maolan_engine::message::Action;

#[derive(Debug, Clone)]
pub enum Show {
    AddTrack,
}

#[derive(Debug, Clone)]
pub enum TrackKind {
    Audio,
    MIDI,
}

#[derive(Debug, Clone)]
pub enum AddTrack {
    Kind(TrackKind),
    Name(String),
    Ins(usize),
    AudioOuts(usize),
    MIDIOuts(usize),
}

#[derive(Debug, Clone)]
pub enum Message {
    Debug(String),

    Request(Action),
    Response(Result<Action, String>),

    PaneResized(pane_grid::ResizeEvent),
    Show(Show),
    Cancel(Show),

    AddTrack(AddTrack),
}
