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
    SessionMetadata,
    Preferences,
    AutosaveRecovery,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SnapMode {
    NoSnap,
    Bar,
    Beat,
    Eighth,
    Sixteenth,
    ThirtySecond,
    SixtyFourth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditTool {
    Select,
    Comp,
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

impl fmt::Display for EditTool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Select => write!(f, "Select"),
            Self::Comp => write!(f, "Comp"),
        }
    }
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
    Lv2Parameter {
        instance_id: usize,
        index: u32,
        min: f32,
        max: f32,
    },
    Vst3Parameter {
        instance_id: usize,
        param_id: u32,
    },
    ClapParameter {
        instance_id: usize,
        param_id: u32,
        min: f64,
        max: f64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TrackAutomationMode {
    Read,
    Touch,
    Latch,
    Write,
}

impl fmt::Display for TrackAutomationMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read => write!(f, "Read"),
            Self::Touch => write!(f, "Touch"),
            Self::Latch => write!(f, "Latch"),
            Self::Write => write!(f, "Write"),
        }
    }
}

impl fmt::Display for TrackAutomationTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Volume => write!(f, "Volume"),
            Self::Balance => write!(f, "Balance"),
            Self::Mute => write!(f, "Mute"),
            Self::Lv2Parameter {
                instance_id, index, ..
            } => write!(f, "LV2 {}:{}", instance_id, index),
            Self::Vst3Parameter {
                instance_id,
                param_id,
            } => write!(f, "VST3 {}:{}", instance_id, param_id),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PianoScaleRoot {
    C,
    CSharp,
    D,
    DSharp,
    E,
    F,
    FSharp,
    G,
    GSharp,
    A,
    ASharp,
    B,
}

impl PianoScaleRoot {
    pub const ALL: [PianoScaleRoot; 12] = [
        PianoScaleRoot::C,
        PianoScaleRoot::CSharp,
        PianoScaleRoot::D,
        PianoScaleRoot::DSharp,
        PianoScaleRoot::E,
        PianoScaleRoot::F,
        PianoScaleRoot::FSharp,
        PianoScaleRoot::G,
        PianoScaleRoot::GSharp,
        PianoScaleRoot::A,
        PianoScaleRoot::ASharp,
        PianoScaleRoot::B,
    ];

    pub fn semitone(self) -> u8 {
        match self {
            PianoScaleRoot::C => 0,
            PianoScaleRoot::CSharp => 1,
            PianoScaleRoot::D => 2,
            PianoScaleRoot::DSharp => 3,
            PianoScaleRoot::E => 4,
            PianoScaleRoot::F => 5,
            PianoScaleRoot::FSharp => 6,
            PianoScaleRoot::G => 7,
            PianoScaleRoot::GSharp => 8,
            PianoScaleRoot::A => 9,
            PianoScaleRoot::ASharp => 10,
            PianoScaleRoot::B => 11,
        }
    }
}

impl fmt::Display for PianoScaleRoot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PianoScaleRoot::C => write!(f, "C"),
            PianoScaleRoot::CSharp => write!(f, "C#"),
            PianoScaleRoot::D => write!(f, "D"),
            PianoScaleRoot::DSharp => write!(f, "D#"),
            PianoScaleRoot::E => write!(f, "E"),
            PianoScaleRoot::F => write!(f, "F"),
            PianoScaleRoot::FSharp => write!(f, "F#"),
            PianoScaleRoot::G => write!(f, "G"),
            PianoScaleRoot::GSharp => write!(f, "G#"),
            PianoScaleRoot::A => write!(f, "A"),
            PianoScaleRoot::ASharp => write!(f, "A#"),
            PianoScaleRoot::B => write!(f, "B"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PianoChordKind {
    MajorTriad,
    MinorTriad,
    Dominant7,
    Major7,
    Minor7,
}

impl PianoChordKind {
    pub const ALL: [PianoChordKind; 5] = [
        PianoChordKind::MajorTriad,
        PianoChordKind::MinorTriad,
        PianoChordKind::Dominant7,
        PianoChordKind::Major7,
        PianoChordKind::Minor7,
    ];

    pub fn intervals(self) -> &'static [u8] {
        match self {
            PianoChordKind::MajorTriad => &[4, 7],
            PianoChordKind::MinorTriad => &[3, 7],
            PianoChordKind::Dominant7 => &[4, 7, 10],
            PianoChordKind::Major7 => &[4, 7, 11],
            PianoChordKind::Minor7 => &[3, 7, 10],
        }
    }
}

impl fmt::Display for PianoChordKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PianoChordKind::MajorTriad => write!(f, "Maj"),
            PianoChordKind::MinorTriad => write!(f, "Min"),
            PianoChordKind::Dominant7 => write!(f, "7"),
            PianoChordKind::Major7 => write!(f, "Maj7"),
            PianoChordKind::Minor7 => write!(f, "Min7"),
        }
    }
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
pub enum ExportFormat {
    Wav,
    Mp3,
    Ogg,
    Flac,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportMp3Mode {
    Cbr,
    Vbr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportNormalizeMode {
    Peak,
    Loudness,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportRenderMode {
    Mixdown,
    StemsPostFader,
    StemsPreFader,
}

impl ExportNormalizeMode {
    pub const ALL: [ExportNormalizeMode; 2] =
        [ExportNormalizeMode::Peak, ExportNormalizeMode::Loudness];
}

impl ExportMp3Mode {
    pub const ALL: [ExportMp3Mode; 2] = [ExportMp3Mode::Cbr, ExportMp3Mode::Vbr];
}

impl ExportRenderMode {
    pub const ALL: [ExportRenderMode; 3] = [
        ExportRenderMode::Mixdown,
        ExportRenderMode::StemsPostFader,
        ExportRenderMode::StemsPreFader,
    ];
}

impl fmt::Display for ExportNormalizeMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExportNormalizeMode::Peak => write!(f, "Peak"),
            ExportNormalizeMode::Loudness => write!(f, "Loudness"),
        }
    }
}

impl fmt::Display for ExportFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExportFormat::Wav => write!(f, "WAV"),
            ExportFormat::Mp3 => write!(f, "MP3"),
            ExportFormat::Ogg => write!(f, "OGG"),
            ExportFormat::Flac => write!(f, "FLAC"),
        }
    }
}

impl fmt::Display for ExportMp3Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExportMp3Mode::Cbr => write!(f, "CBR"),
            ExportMp3Mode::Vbr => write!(f, "VBR"),
        }
    }
}

impl fmt::Display for ExportRenderMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExportRenderMode::Mixdown => write!(f, "Mixdown"),
            ExportRenderMode::StemsPostFader => write!(f, "Stems (Post-Fader)"),
            ExportRenderMode::StemsPreFader => write!(f, "Stems (Pre-Fader)"),
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
    SelectTrackFromMixer(String),
    TrackAutomationToggle {
        track_name: String,
    },
    TrackFreezeToggle {
        track_name: String,
    },
    TrackFreezeFlatten {
        track_name: String,
    },
    TrackSetVcaMaster {
        track_name: String,
        master_track: Option<String>,
    },
    TrackCreateAuxReturnFromSelection,
    TrackAuxSendLevelAdjust {
        track_name: String,
        aux_track: String,
        delta_db: f32,
    },
    TrackAuxSendPanAdjust {
        track_name: String,
        aux_track: String,
        delta: f32,
    },
    TrackAuxSendTogglePrePost {
        track_name: String,
        aux_track: String,
    },
    TrackMidiLearnArm {
        track_name: String,
        target: maolan_engine::message::TrackMidiLearnTarget,
    },
    TrackMidiLearnClear {
        track_name: String,
        target: maolan_engine::message::TrackMidiLearnTarget,
    },
    GlobalMidiLearnArm {
        target: maolan_engine::message::GlobalMidiLearnTarget,
    },
    GlobalMidiLearnClear {
        target: maolan_engine::message::GlobalMidiLearnTarget,
    },
    TrackAutomationCycleMode {
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
    TrackAutomationAddVst3Lanes {
        track_name: String,
        plugin_path: String,
    },
    #[cfg(all(unix, not(target_os = "macos")))]
    TrackAutomationAddLv2Lanes {
        track_name: String,
        plugin_uri: String,
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
    LoadSessionPath(PathBuf),
    RecoverAutosaveSnapshot,
    RecoverOlderAutosaveSnapshot,
    RecoverAutosaveIgnore,
    OpenExporter,
    ExportDiagnosticsBundleRequest,
    SessionDiagnosticsRequest,
    MidiLearnMappingsPanelToggle,
    MidiLearnMappingsReportRequest,
    MidiLearnMappingsExportRequest,
    MidiLearnMappingsImportRequest,
    MidiLearnMappingsClearAllRequest,
    ExportSampleRateSelected(u32),
    ExportFormatWavToggled(bool),
    ExportFormatMp3Toggled(bool),
    ExportFormatOggToggled(bool),
    ExportFormatFlacToggled(bool),
    ExportMp3ModeSelected(ExportMp3Mode),
    ExportMp3BitrateSelected(u16),
    ExportOggQualityInput(String),
    ExportRenderModeSelected(ExportRenderMode),
    ExportRealtimeFallbackToggled(bool),
    ExportBitDepthSelected(ExportBitDepth),
    ExportNormalizeToggled(bool),
    ExportNormalizeModeSelected(ExportNormalizeMode),
    ExportNormalizeDbfsInput(String),
    ExportNormalizeLufsInput(String),
    ExportNormalizeDbtpInput(String),
    ExportNormalizeLimiterToggled(bool),
    ExportMasterLimiterToggled(bool),
    ExportMasterLimiterCeilingInput(String),
    ExportSettingsConfirm,
    ExportFileSelected(Option<PathBuf>),
    ExportProgress {
        progress: f32,
        operation: Option<String>,
    },
    PreferencesSampleRateSelected(u32),
    PreferencesSnapModeSelected(SnapMode),
    PreferencesSave,
    SessionMetadataAuthorInput(String),
    SessionMetadataAlbumInput(String),
    SessionMetadataYearInput(String),
    SessionMetadataTrackNumberInput(String),
    SessionMetadataGenreInput(String),
    SessionMetadataSave,

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

    MouseMoved(mouse::Event),
    EditorMouseMoved(Point),
    EditorScrollChanged {
        x: f32,
        y: f32,
    },
    EditorScrollXChanged(f32),
    EditorScrollYChanged(f32),
    MousePressed(mouse::Button),
    MouseReleased,

    ShiftPressed,
    CtrlPressed,
    ShiftReleased,
    CtrlReleased,

    WindowResized(Size),
    WindowCloseRequested,
    PlaybackTick,
    AutosaveSnapshotTick,
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
    DrainAudioPeakUpdates,
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
    TempoAdjust(f32),
    TempoPointAdd(usize),
    TempoPointSelect {
        sample: usize,
        additive: bool,
    },
    TempoPointsMove {
        from_samples: Vec<usize>,
        to_samples: Vec<usize>,
    },
    TempoSelectionDuplicate,
    TempoSelectionResetToPrevious,
    TempoSelectionDelete,
    TimeSignaturePointAdd(usize),
    TimeSignaturePointSelect {
        sample: usize,
        additive: bool,
    },
    TimeSignaturePointsMove {
        from_samples: Vec<usize>,
        to_samples: Vec<usize>,
    },
    TimeSignatureSelectionDuplicate,
    TimeSignatureSelectionResetToPrevious,
    TimeSignatureSelectionDelete,
    ClearTimingPointSelection,
    TimeSignatureNumeratorAdjust(i8),
    TimeSignatureDenominatorAdjust(i8),
    TempoInputChanged(String),
    TempoInputCommit,
    TimeSignatureNumeratorInputChanged(String),
    TimeSignatureDenominatorInputChanged(String),
    TimeSignatureInputCommit,
    SetSnapMode(SnapMode),
    ToggleCompTool,
    RecordFolderSelected(Option<PathBuf>),

    SendMessageFinished(Result<(), String>),

    Workspace,
    Connections,
    OpenTrackPlugins(String),
    OpenMidiPiano {
        track_idx: String,
        clip_idx: usize,
    },
    MidiClipPreviewLoaded {
        track_idx: String,
        clip_idx: usize,
        clip_name: String,
        notes: Vec<crate::state::PianoNote>,
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
    PianoScaleSelectedNotes,
    PianoScaleRootSelected(PianoScaleRoot),
    PianoScaleMinorToggled(bool),
    PianoChordSelectedNotes,
    PianoChordKindSelected(PianoChordKind),
    PianoLegatoSelectedNotes,
    PianoVelocityShapeSelectedNotes,
    PianoVelocityShapeAmountChanged(f32),
    PianoQuantizeSelectedNotes,
    PianoHumanizeSelectedNotes,
    PianoGrooveSelectedNotes,
    PianoQuantizeStrengthChanged(f32),
    PianoHumanizeTimeAmountChanged(f32),
    PianoHumanizeVelocityAmountChanged(f32),
    PianoGrooveAmountChanged(f32),
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
    #[cfg(any(target_os = "freebsd", target_os = "linux"))]
    HWInputSelected(AudioDeviceOption),
    #[cfg(target_os = "windows")]
    HWInputSelected(String),
    HWBackendSelected(AudioBackendOption),
    HWExclusiveToggled(bool),
    #[cfg(unix)]
    HWBitsChanged(usize),
    HWSampleRateChanged(i32),
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
    ClipSetMuted {
        track_idx: String,
        clip_idx: usize,
        kind: Kind,
        muted: bool,
    },
    ClipWarpReset {
        track_idx: String,
        clip_idx: usize,
    },
    ClipWarpHalfSpeed {
        track_idx: String,
        clip_idx: usize,
    },
    ClipWarpDoubleSpeed {
        track_idx: String,
        clip_idx: usize,
    },
    ClipWarpAddMarker {
        track_idx: String,
        clip_idx: usize,
    },
    ClipSetActiveTake {
        track_idx: String,
        clip_idx: usize,
        kind: Kind,
    },
    ClipCycleActiveTake {
        track_idx: String,
        clip_idx: usize,
        kind: Kind,
    },
    ClipUnmuteTakesInRange {
        track_idx: String,
        clip_idx: usize,
        kind: Kind,
    },
    ClipTakeLanePinToggle {
        track_idx: String,
        clip_idx: usize,
        kind: Kind,
    },
    ClipTakeLaneLockToggle {
        track_idx: String,
        clip_idx: usize,
        kind: Kind,
    },
    ClipTakeLaneMove {
        track_idx: String,
        clip_idx: usize,
        kind: Kind,
        delta: i8,
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
