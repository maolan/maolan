use iced::mouse;
use iced::widget::pane_grid;
use maolan_engine::message::{Action, TrackKind};

#[derive(Debug, Clone, Copy)]
pub enum Show {
    AddTrack,
    Save,
    Open,
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
    Ignore,
    Debug(String),

    Request(Action),
    Response(Result<Action, String>),

    PaneResized(pane_grid::ResizeEvent),
    Show(Show),
    Cancel,

    AddTrack(AddTrack),
    Save(String),
    SavePath(String),
    Open(String),
    OpenPath(String),
    SelectTrack(String),
    DeleteSelectedTracks,

    TrackResizeStart(String),
    ClipResizeStart(String, String, bool),
    MouseMoved(mouse::Event),
    MouseReleased,

    ShiftPressed,
    CtrlPressed,
    ShiftReleased,
    CtrlReleased,
}
