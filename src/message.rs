use iced::{Point, Rectangle, Size, mouse, widget::Id};
use maolan_engine::{kind::Kind, message::Action};
use std::path::PathBuf;

use crate::state::AudioBackendOption;
#[cfg(any(
    target_os = "linux",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
use crate::state::AudioDeviceOption;
use std::fmt;

#[derive(Debug, Clone, Copy)]
pub enum Show {
    AddTrack,
    TrackPluginList,
    ExportSettings,
    Save,
    SaveAs,
    SaveTemplateAs,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapMode {
    NoSnap,
    Bar,
    Beat,
    Eighth,
    Sixteenth,
    ThirtySecond,
    SixtyFourth,
}

impl fmt::Display for SnapMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSnap => write!(f, "No Snap"),
            Self::Bar => write!(f, "Bar"),
            Self::Beat => write!(f, "Beat"),
            Self::Eighth => write!(f, "1/8"),
            Self::Sixteenth => write!(f, "1/16"),
            Self::ThirtySecond => write!(f, "1/32"),
            Self::SixtyFourth => write!(f, "1/64"),
        }
    }
}

impl SnapMode {
    pub const ALL: [SnapMode; 7] = [
        SnapMode::NoSnap,
        SnapMode::Bar,
        SnapMode::Beat,
        SnapMode::Eighth,
        SnapMode::Sixteenth,
        SnapMode::ThirtySecond,
        SnapMode::SixtyFourth,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PianoControllerLane {
    Controller,
    Velocity,
    Rpn,
    Nrpn,
    SysEx,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TrackAutomationTarget {
    Volume,
    Balance,
    Mute,
    ClapParameter {
        instance_id: usize,
        param_id: u32,
        min: f64,
        max: f64,
    },
}

impl fmt::Display for TrackAutomationTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Volume => write!(f, "Volume"),
            Self::Balance => write!(f, "Balance"),
            Self::Mute => write!(f, "Mute"),
            Self::ClapParameter {
                instance_id,
                param_id,
                ..
            } => write!(f, "CLAP {}:{}", instance_id, param_id),
        }
    }
}

impl fmt::Display for PianoControllerLane {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Controller => write!(f, "Controller"),
            Self::Velocity => write!(f, "Velocity"),
            Self::Rpn => write!(f, "RPN"),
            Self::Nrpn => write!(f, "NRPN"),
            Self::SysEx => write!(f, "SysEx"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PianoVelocityKind {
    NoteVelocity,
    ReleaseVelocity,
}

impl PianoVelocityKind {
    pub const ALL: [PianoVelocityKind; 2] = [
        PianoVelocityKind::NoteVelocity,
        PianoVelocityKind::ReleaseVelocity,
    ];
}

impl fmt::Display for PianoVelocityKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoteVelocity => write!(f, "Note Velocity"),
            Self::ReleaseVelocity => write!(f, "Release Velocity"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PianoRpnKind {
    PitchBendSensitivity,
    FineTuning,
    CoarseTuning,
}

impl PianoRpnKind {
    pub const ALL: [PianoRpnKind; 3] = [
        PianoRpnKind::PitchBendSensitivity,
        PianoRpnKind::FineTuning,
        PianoRpnKind::CoarseTuning,
    ];
}

impl fmt::Display for PianoRpnKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PitchBendSensitivity => write!(f, "Pitch Bend Sensitivity"),
            Self::FineTuning => write!(f, "Fine Tuning"),
            Self::CoarseTuning => write!(f, "Coarse Tuning"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PianoNrpnKind {
    Brightness,
    VibratoRate,
    VibratoDepth,
}

impl PianoNrpnKind {
    pub const ALL: [PianoNrpnKind; 3] = [
        PianoNrpnKind::Brightness,
        PianoNrpnKind::VibratoRate,
        PianoNrpnKind::VibratoDepth,
    ];
}

impl fmt::Display for PianoNrpnKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Brightness => write!(f, "Brightness"),
            Self::VibratoRate => write!(f, "Vibrato Rate"),
            Self::VibratoDepth => write!(f, "Vibrato Depth"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportBitDepth {
    Int16,
    Int24,
    Int32,
    Float32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportNormalizeMode {
    Peak,
    Loudness,
}

impl ExportNormalizeMode {
    pub const ALL: [ExportNormalizeMode; 2] =
        [ExportNormalizeMode::Peak, ExportNormalizeMode::Loudness];
}

impl fmt::Display for ExportNormalizeMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExportNormalizeMode::Peak => write!(f, "Peak"),
            ExportNormalizeMode::Loudness => write!(f, "Loudness"),
        }
    }
}

impl ExportBitDepth {
    pub const ALL: [ExportBitDepth; 4] = [
        ExportBitDepth::Int16,
        ExportBitDepth::Int24,
        ExportBitDepth::Int32,
        ExportBitDepth::Float32,
    ];
}

impl fmt::Display for ExportBitDepth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExportBitDepth::Int16 => write!(f, "16-bit PCM"),
            ExportBitDepth::Int24 => write!(f, "24-bit PCM"),
            ExportBitDepth::Int32 => write!(f, "32-bit PCM"),
            ExportBitDepth::Float32 => write!(f, "32-bit float"),
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
    TemplateSelected(String),
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

    AddTrack(AddTrack),
    SelectTrack(String),
    TrackAutomationToggle {
        track_name: String,
    },
    TrackAutomationAddLane {
        track_name: String,
        target: TrackAutomationTarget,
    },
    TrackAutomationAddClapLanes {
        track_name: String,
        plugin_path: String,
    },
    TrackAutomationLaneHover {
        track_name: String,
        target: TrackAutomationTarget,
        position: Point,
    },
    TrackAutomationLaneInsertPoint {
        track_name: String,
        target: TrackAutomationTarget,
    },
    TrackAutomationLaneDeletePoint {
        track_name: String,
        target: TrackAutomationTarget,
        sample: usize,
    },
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
    OpenExporter,
    ExportSampleRateSelected(u32),
    ExportBitDepthSelected(ExportBitDepth),
    ExportNormalizeToggled(bool),
    ExportNormalizeModeSelected(ExportNormalizeMode),
    ExportNormalizeDbfsInput(String),
    ExportNormalizeLufsInput(String),
    ExportNormalizeDbtpInput(String),
    ExportNormalizeLimiterToggled(bool),
    ExportSettingsConfirm,
    ExportFileSelected(Option<PathBuf>),
    ExportProgress {
        progress: f32,
        operation: Option<String>,
    },

    TrackResizeStart(String),
    TrackResizeHover(String, bool),
    TracksResizeStart,
    MixerResizeStart,
    ClipResizeStart(Kind, String, usize, bool),
    FadeResizeStart {
        kind: Kind,
        track_idx: String,
        clip_idx: usize,
        is_fade_out: bool,
    },

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
    WindowCloseRequested,
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
    SetSnapMode(SnapMode),
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
    PianoNoteClick {
        note_index: usize,
        position: Point,
    },
    PianoNotesDrag {
        position: Point,
    },
    PianoNotesEndDrag,
    PianoNoteResizeStart {
        note_index: usize,
        position: Point,
        resize_start: bool,
    },
    PianoNoteResizeDrag {
        position: Point,
    },
    PianoNoteResizeEnd,
    PianoSelectRectStart {
        position: Point,
    },
    PianoSelectRectDrag {
        position: Point,
    },
    PianoSelectRectEnd,
    PianoCreateNoteStart {
        position: Point,
    },
    PianoCreateNoteDrag {
        position: Point,
    },
    PianoCreateNoteEnd,
    PianoAdjustVelocity {
        note_index: usize,
        delta: i8,
    },
    PianoSetVelocity {
        note_index: usize,
        velocity: u8,
    },
    PianoAdjustController {
        controller_index: usize,
        delta: i8,
    },
    PianoSetControllerValue {
        controller_index: usize,
        value: u8,
    },
    PianoInsertControllers {
        controllers: Vec<PianoControllerEditData>,
    },
    PianoSysExSelect(Option<usize>),
    PianoSysExOpenEditor(Option<usize>),
    PianoSysExCloseEditor,
    PianoSysExHexInput(String),
    PianoSysExAdd,
    PianoSysExUpdate,
    PianoSysExDelete,
    PianoSysExMove {
        index: usize,
        sample: usize,
    },
    PianoControllerLaneSelected(PianoControllerLane),
    PianoControllerKindSelected(u8),
    PianoVelocityKindSelected(PianoVelocityKind),
    PianoRpnKindSelected(PianoRpnKind),
    PianoNrpnKindSelected(PianoNrpnKind),
    PianoDeleteSelectedNotes,
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
    ToggleClapCapabilityScanning(bool),
    FilterClapPlugin(String),
    SelectClapPlugin(String),
    LoadSelectedClapPlugins,
    PluginFormatSelected(PluginFormat),
    UnloadClapPlugin(String),
    ShowClapPluginUi(String),
    OpenVst3PluginUi {
        track_name: String,
        instance_id: usize,
        plugin_path: String,
        plugin_name: String,
        plugin_id: String,
        audio_inputs: usize,
        audio_outputs: usize,
    },

    #[cfg(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    HWSelected(AudioDeviceOption),
    #[cfg(not(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    )))]
    HWSelected(String),
    HWBackendSelected(AudioBackendOption),
    HWExclusiveToggled(bool),
    #[cfg(unix)]
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

    ClipToggleFade {
        track_idx: String,
        clip_idx: usize,
        kind: Kind,
    },

    TrackRenameShow(String),
    TrackRenameInput(String),
    TrackRenameConfirm,
    TrackRenameCancel,

    TrackTemplateSaveShow(String),
    TrackTemplateSaveInput(String),
    TrackTemplateSaveConfirm,
    TrackTemplateSaveCancel,

    TemplateSaveInput(String),
    TemplateSaveConfirm,
    TemplateSaveCancel,

    NewSession,
    NewFromTemplate(String),

    AddTrackFromTemplate {
        name: String,
        template: String,
        audio_ins: usize,
        midi_ins: usize,
        audio_outs: usize,
        midi_outs: usize,
    },

    Undo,
    Redo,
}

#[derive(Debug, Clone)]
pub struct PianoControllerEditData {
    pub sample: usize,
    pub controller: u8,
    pub value: u8,
    pub channel: u8,
}
