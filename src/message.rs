use iced::{Point, Rectangle, Size, mouse, widget::Id};
use maolan_engine::{kind::Kind, message::Action};
use std::path::PathBuf;

#[cfg(target_os = "linux")]
use crate::state::AudioDeviceOption;

#[derive(Debug, Clone, Copy)]
pub enum Show {
    AddTrack,
    TrackPluginList,
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
    pub track_index: String,
    pub start: Point,
    pub end: Point,
}

impl DraggedClip {
    pub fn new(kind: Kind, index: usize, track_index: String) -> Self {
        Self {
            kind,
            index,
            track_index: track_index.clone(),
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
    NewSession,

    AddTrack(AddTrack),
    SelectTrack(String),
    SelectClip {
        track_idx: String,
        clip_idx: usize,
        kind: Kind,
    },
    RemoveSelectedTracks,
    RemoveSelected,
    Remove,
    DeselectAll,

    ConnectionViewSelectTrack(String),
    ConnectionViewSelectConnection(usize),

    SaveFolderSelected(Option<PathBuf>),
    OpenFolderSelected(Option<PathBuf>),

    TrackResizeStart(String),
    TrackResizeHover(String, bool),
    TracksResizeStart,
    MixerResizeStart,
    ClipResizeStart(Kind, String, usize, bool),

    ClipDrag(DraggedClip),
    ClipDropped(Point, Rectangle),
    HandleClipZones(Vec<(Id, Rectangle)>),

    TrackDrag(usize),
    TrackDropped(Point, Rectangle),
    HandleTrackZones(Vec<(Id, Rectangle)>),

    StartMovingTrackAndSelect(crate::state::MovingTrack, String),

    MouseMoved(mouse::Event),
    MouseReleased,

    ShiftPressed,
    CtrlPressed,
    ShiftReleased,
    CtrlReleased,

    WindowResized(Size),
    PlaybackTick,
    RecordingPreviewTick,
    RecordingPreviewPeaksTick,
    ToggleTransport,
    ZoomVisibleBarsChanged(f32),
    TracksResizeHover(bool),
    MixerResizeHover(bool),

    OpenFileImporter,
    ImportFilesSelected(Option<Vec<std::path::PathBuf>>),
    TransportPlay,
    TransportPause,
    TransportStop,
    TransportRecordToggle,
    RecordFolderSelected(Option<PathBuf>),

    SendMessageFinished(Result<(), String>),

    Workspace,
    Connections,
    OpenTrackPlugins(String),
    RefreshLv2Plugins,
    FilterLv2Plugins(String),
    SelectLv2Plugin(String),
    LoadSelectedLv2Plugins,

    #[cfg(target_os = "linux")]
    HWSelected(AudioDeviceOption),
    #[cfg(not(target_os = "linux"))]
    HWSelected(String),
    HWExclusiveToggled(bool),
    HWPeriodFramesChanged(usize),
    HWNPeriodsChanged(usize),
    HWSyncModeToggled(bool),
}
