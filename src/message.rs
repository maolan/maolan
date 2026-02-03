use iced::{Point, Rectangle, Size, mouse, widget::Id};
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
pub struct DraggedClip {
    pub index: usize,
    pub track_index: usize,
    pub point: Point,
    pub rect: Rectangle,
}

impl DraggedClip {
    pub fn new(index: usize, track_index: usize, point: Point, rect: Rectangle) -> Self {
        Self {
            index,
            track_index,
            point,
            rect,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Ignore,
    Debug(String),

    Request(Action),
    Response(Result<Action, String>),

    Show(Show),
    Cancel,

    AddTrack(AddTrack),
    Save(String),
    SavePath(String),
    Open(String),
    OpenPath(String),
    SelectTrack(String),
    DeleteSelectedTracks,

    TrackResizeStart(usize),
    TracksResizeStart,
    MixerResizeStart,
    ClipResizeStart(usize, usize, bool),
    ClipDrag(DraggedClip),
    ClipDropped(Point, Rectangle),
    HandleZones(Vec<(Id, Rectangle)>),
    MouseMoved(mouse::Event),
    MouseReleased,

    ShiftPressed,
    CtrlPressed,
    ShiftReleased,
    CtrlReleased,

    WindowResized(Size),
}
