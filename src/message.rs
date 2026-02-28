use iced::{Point, Rectangle, Size, mouse, widget::Id};
use maolan_engine::{kind::Kind, message::Action};
use std::path::PathBuf;

use crate::state::AudioBackendOption;
#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
use crate::state::AudioDeviceOption;
use std::fmt;

#[derive(Debug, Clone, Copy)]
pub enum Show {
    AddTrack,
    TrackPluginList,
    Save,
    SaveAs,
    Open,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginFormat {
    Lv2,
    Clap,
    Vst3,
}

impl fmt::Display for PluginFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lv2 => write!(f, "LV2"),
            Self::Clap => write!(f, "CLAP"),
            Self::Vst3 => write!(f, "VST3"),
        }
    }
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
    EditorScrollXChanged(f32),
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
    PianoZoomXChanged(f32),
    PianoZoomYChanged(f32),
    PianoScrollChanged {
        x: f32,
        y: f32,
    },
    PianoScrollXChanged(f32),
    PianoScrollYChanged(f32),
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
    TransportPause,
    TransportStop,
    JumpToStart,
    JumpToEnd,
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
    OpenMidiPiano {
        track_idx: String,
        clip_idx: usize,
    },
    PianoKeyPressed(u8),
    PianoKeyReleased(u8),
    #[cfg(all(unix, not(target_os = "macos")))]
    RefreshLv2Plugins,
    #[cfg(all(unix, not(target_os = "macos")))]
    FilterLv2Plugins(String),
    #[cfg(all(unix, not(target_os = "macos")))]
    SelectLv2Plugin(String),
    #[cfg(all(unix, not(target_os = "macos")))]
    LoadSelectedLv2Plugins,
    #[cfg(all(unix, not(target_os = "macos")))]
    OpenLv2PluginUi {
        track_name: String,
        instance_id: usize,
    },
    #[cfg(all(unix, not(target_os = "macos")))]
    PumpLv2Ui,
    RefreshVst3Plugins,
    FilterVst3Plugins(String),
    SelectVst3Plugin(String),
    LoadSelectedVst3Plugins,
    RefreshClapPlugins,
    FilterClapPlugin(String),
    SelectClapPlugin(String),
    LoadSelectedClapPlugins,
    PluginFormatSelected(PluginFormat),
    UnloadClapPlugin(String),
    ShowClapPluginUi(String),
    OpenVst3PluginUi {
        plugin_path: String,
        plugin_name: String,
        plugin_id: String,
    },

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
