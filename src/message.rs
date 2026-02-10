use iced::{Point, Rectangle, Size, mouse, widget::Id};
use maolan_engine::{kind::Kind, message::Action};

#[derive(Debug, Clone, Copy)]
pub enum Show {
    AddTrack,
    Save,
    Open,
}

#[derive(Debug, Clone)]
pub enum AddTrack {
    Name(String),
    AudioIns(usize),
    AudioOuts(usize),
    MIDIIns(usize),
    MIDIOuts(usize),
}

#[derive(Debug, Clone)]
pub struct DraggedClip {
    pub kind: Kind,
    pub index: usize,
    pub track_index: usize,
    pub start: Point,
    pub end: Point,
}

impl DraggedClip {
    pub fn new(kind: Kind, index: usize, track_index: usize) -> Self {
        Self {
            kind,
            index,
            track_index,
            start: Point::new(0.0, 0.0),
            end: Point::new(0.0, 0.0),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    None,

    Request(Action),
    Response(Result<Action, String>),

    Show(Show),
    Cancel,

    AddTrack(AddTrack),
    SelectTrack(String),
    RemoveSelectedTracks,

    Save(String),
    SavePath(String),
    Open(String),
    OpenPath(String),

    TrackResizeStart(usize),
    TracksResizeStart,
    MixerResizeStart,
    ClipResizeStart(Kind, usize, usize, bool),

    ClipDrag(DraggedClip),
    ClipDropped(Point, Rectangle),
    HandleClipZones(Vec<(Id, Rectangle)>),

    TrackDrag(usize),
    TrackDropped(Point, Rectangle),
    HandleTrackZones(Vec<(Id, Rectangle)>),

    MouseMoved(mouse::Event),
    MouseReleased,

    ShiftPressed,
    CtrlPressed,
    ShiftReleased,
    CtrlReleased,

    WindowResized(Size),

    OpenFileImporter,
    ImportFilesSelected(Option<Vec<std::path::PathBuf>>),

    SendMessageFinished(Result<(), String>),

    Workspace,
    Connections,
}
