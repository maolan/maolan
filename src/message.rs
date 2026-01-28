use iced::widget::pane_grid;
use maolan_engine::message::Action;

#[derive(Debug, Clone, Copy)]
pub enum Show {
    AddTrack,
    Save,
}

#[derive(Debug, Clone, PartialEq, Copy, Eq)]
pub enum TrackKind {
    Audio,
    MIDI,
}

impl std::fmt::Display for TrackKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Audio => "Audio",
            Self::MIDI => "MIDI",
        })
    }
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
    Cancel,

    AddTrack(AddTrack),
    Save(String),
    SavePath(String),
}
