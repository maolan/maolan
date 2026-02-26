use iced::{Point, Rectangle, Size, mouse, widget::Id};
use maolan_engine::{kind::Kind, message::Action};
use std::path::PathBuf;

use crate::state::AudioBackendOption;
#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
use crate::state::AudioDeviceOption;

#[derive(Debug, Clone, Copy)]
pub enum Show {
    AddTrack,
    TrackPluginList,
    Save,
    SaveAs,
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
    pub copy: bool,
}

impl DraggedClip {
    pub fn new(kind: Kind, index: usize, track_index: String) -> Self {
        Self {
            kind,
            index,
            track_index: track_index.clone(),
            start: Point::new(0.0, 0.0),
            end: Point::new(0.0, 0.0),
            copy: false,
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
    DeselectClips,
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
    HandleClipZones(Vec<(Id, Rectangle)>),
    HandleClipPreviewZones(Vec<(Id, Rectangle)>),

    TrackDrag(usize),
    TrackDropped(Point, Rectangle),
    HandleTrackZones(Vec<(Id, Rectangle)>),

    StartMovingTrackAndSelect(crate::state::MovingTrack, String),

    MouseMoved(mouse::Event),
    EditorMouseMoved(Point),
    MousePressed(mouse::Button),
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
    ImportProgress {
        file_index: usize,
        total_files: usize,
        file_progress: f32,
        filename: String,
        operation: Option<String>,
    },
    TransportPlay,
    TransportStop,
    TransportRecordToggle,
    ToggleLoop,
    SetLoopRange(Option<(usize, usize)>),
    TogglePunch,
    SetPunchRange(Option<(usize, usize)>),
    RecordFolderSelected(Option<PathBuf>),

    SendMessageFinished(Result<(), String>),

    Workspace,
    Connections,
    OpenTrackPlugins(String),
    #[cfg(all(unix, not(target_os = "macos")))]
    RefreshLv2Plugins,
    #[cfg(all(unix, not(target_os = "macos")))]
    FilterLv2Plugins(String),
    #[cfg(all(unix, not(target_os = "macos")))]
    SelectLv2Plugin(String),
    #[cfg(all(unix, not(target_os = "macos")))]
    LoadSelectedLv2Plugins,
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    RefreshVst3Plugins,
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    FilterVst3Plugins(String),
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    SelectVst3Plugin(String),
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    LoadSelectedVst3Plugins,

    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
    HWSelected(AudioDeviceOption),
    #[cfg(not(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd")))]
    HWSelected(String),
    HWBackendSelected(AudioBackendOption),
    HWExclusiveToggled(bool),
    HWBitsChanged(usize),
    HWPeriodFramesChanged(usize),
    HWNPeriodsChanged(usize),
    HWSyncModeToggled(bool),

    ClipRenameShow {
        track_idx: String,
        clip_idx: usize,
        kind: Kind,
    },
    ClipRenameInput(String),
    ClipRenameConfirm,
    ClipRenameCancel,

    TrackRenameShow(String),
    TrackRenameInput(String),
    TrackRenameConfirm,
    TrackRenameCancel,
}
