use iced::{
    Point, Rectangle, Size, mouse,
    widget::{Id, text_editor},
};
use maolan_engine::{kind::Kind, message::Action};
pub use maolan_widgets::midi::{PianoControllerLane, PianoNrpnKind, PianoRpnKind};
use std::path::PathBuf;

use crate::state::AudioBackendOption;
#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
use crate::state::AudioDeviceOption;
use std::fmt;

#[derive(Debug, Clone, Copy)]
pub enum Show {
    AddTrack,
    TrackPluginList,
    GenerateAudio,
    ExportSettings,
    SessionMetadata,
    Preferences,
    AutosaveRecovery,
    UnsavedChanges,
    Save,
    SaveAs,
    SaveTemplateAs,
    Open,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BurnBackendOption {
    Cpu,
    Vulkan,
}

impl BurnBackendOption {
    pub const ALL: [Self; 2] = [Self::Cpu, Self::Vulkan];

    pub fn as_ipc_str(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Vulkan => "vulkan",
        }
    }
}

impl fmt::Display for BurnBackendOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cpu => write!(f, "CPU"),
            Self::Vulkan => write!(f, "Vulkan"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum GenerateAudioModelOption {
    #[serde(rename = "happy-new-year")]
    HappyNewYear,
    #[serde(rename = "RL")]
    Rl,
}

impl GenerateAudioModelOption {
    pub const ALL: [Self; 2] = [Self::HappyNewYear, Self::Rl];
}

impl fmt::Display for GenerateAudioModelOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HappyNewYear => write!(f, "happy-new-year"),
            Self::Rl => write!(f, "RL"),
        }
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum SnapMode {
    #[default]
    NoSnap,
    Clips,
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
            Self::Clips => write!(f, "Clips"),
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
    pub fn interval_samples(self, samples_per_beat: f64, samples_per_bar: f64) -> f64 {
        match self {
            SnapMode::NoSnap => 1.0,
            SnapMode::Clips => 1.0,
            SnapMode::Bar => samples_per_bar.max(1.0),
            SnapMode::Beat => samples_per_beat.max(1.0),
            SnapMode::Eighth => (samples_per_beat / 2.0).max(1.0),
            SnapMode::Sixteenth => (samples_per_beat / 4.0).max(1.0),
            SnapMode::ThirtySecond => (samples_per_beat / 8.0).max(1.0),
            SnapMode::SixtyFourth => (samples_per_beat / 16.0).max(1.0),
        }
    }

    pub fn snap_sample(self, sample: f64, samples_per_beat: f64, samples_per_bar: f64) -> f64 {
        if matches!(self, SnapMode::NoSnap | SnapMode::Clips) {
            return sample.max(0.0);
        }
        let interval = self.interval_samples(samples_per_beat, samples_per_bar);
        ((sample.max(0.0) / interval).round() * interval).max(0.0)
    }

    pub fn snap_sample_drag(
        self,
        sample: f64,
        delta_samples: f64,
        samples_per_beat: f64,
        samples_per_bar: f64,
    ) -> f64 {
        if matches!(self, SnapMode::NoSnap | SnapMode::Clips) {
            return sample.max(0.0);
        }
        let interval = self.interval_samples(samples_per_beat, samples_per_bar);
        if delta_samples >= 0.0 {
            ((sample.max(0.0) / interval).floor() * interval).max(0.0)
        } else {
            ((sample.max(0.0) / interval).ceil() * interval).max(0.0)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PreferencesDeviceOption {
    pub id: String,
    pub label: String,
}

impl fmt::Display for PreferencesDeviceOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label)
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidiLaneChannelSelection {
    Omni,
    Channel(u8),
}

impl MidiLaneChannelSelection {
    pub const ALL: [Self; 17] = [
        Self::Omni,
        Self::Channel(0),
        Self::Channel(1),
        Self::Channel(2),
        Self::Channel(3),
        Self::Channel(4),
        Self::Channel(5),
        Self::Channel(6),
        Self::Channel(7),
        Self::Channel(8),
        Self::Channel(9),
        Self::Channel(10),
        Self::Channel(11),
        Self::Channel(12),
        Self::Channel(13),
        Self::Channel(14),
        Self::Channel(15),
    ];

    pub fn from_engine(channel: Option<u8>) -> Self {
        match channel {
            Some(channel) => Self::Channel(channel.min(15)),
            None => Self::Omni,
        }
    }

    pub fn to_engine(self) -> Option<u8> {
        match self {
            Self::Omni => None,
            Self::Channel(channel) => Some(channel.min(15)),
        }
    }
}

impl fmt::Display for MidiLaneChannelSelection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Omni => write!(f, "Omni"),
            Self::Channel(channel) => write!(f, "Ch {}", channel + 1),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PianoVelocityKind {
    NoteVelocity,
    ReleaseVelocity,
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
    Count(usize),
    AudioIns(usize),
    AudioOuts(usize),
    MIDIIns(usize),
    MIDIOuts(usize),
    TemplateSelected(String),
    Submit,
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
pub struct ClipStretchRequest {
    pub track_idx: String,
    pub clip_idx: usize,
    pub clip_name: String,
    pub start: usize,
    pub original_start: usize,
    pub length: usize,
    pub offset: usize,
    pub input_channel: usize,
    pub muted: bool,
    pub fade_enabled: bool,
    pub fade_in_samples: usize,
    pub fade_out_samples: usize,
    pub stretch_ratio: f32,
}

#[derive(Debug, Clone)]
pub struct ClipPitchCorrectionRequest {
    pub track_idx: String,
    pub clip_idx: usize,
    pub clip_name: String,
    pub start: usize,
    pub source_name: String,
    pub source_offset: usize,
    pub source_length: usize,
    pub frame_likeness: f32,
}

#[derive(Debug, Clone)]
pub struct PreparedFreezeClip {
    pub clip_index: usize,
    pub preview_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MidiEditorViewMode {
    #[default]
    PianoRoll,
    DrumGrid,
}

impl fmt::Display for MidiEditorViewMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PianoRoll => write!(f, "Piano"),
            Self::DrumGrid => write!(f, "Drum"),
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
    TrackFreezePrepared {
        track_name: String,
        prepared_clips: Vec<PreparedFreezeClip>,
        result: Result<(), String>,
    },
    TrackFreezeFlatten {
        track_name: String,
    },
    TrackSetVcaMaster {
        track_name: String,
        master_track: Option<String>,
    },
    TrackAddReturn(String),
    TrackAddSend(String),
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
    TrackMarkerCreate(String),
    TrackMarkerRenameShow {
        track_name: String,
        marker_index: usize,
    },
    TrackMarkerDragStart {
        track_name: String,
        marker_index: usize,
    },
    TrackMarkerNameInput(String),
    TrackMarkerNameConfirm,
    TrackMarkerNameCancel,
    TrackMarkerDelete {
        track_name: String,
        marker_index: usize,
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
    RecoverAutosaveIgnore,
    OpenExporter,
    GenerateAudioModelSelected(GenerateAudioModelOption),
    GenerateAudioPromptAction(text_editor::Action),

    GenerateAudioTagsInput(String),
    GenerateAudioBackendSelected(BurnBackendOption),

    GenerateAudioCfgScaleInput(String),
    GenerateAudioStepsInput(usize),
    GenerateAudioSecondsTotalInput(usize),
    GenerateAudioSubmit,
    GenerateAudioProgress {
        progress: f32,
        operation: Option<String>,
    },
    GenerateAudioCancel,
    GenerateAudioFinished(Result<String, String>),
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
    ExportHwOutPortToggled(usize, bool),
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
    PreferencesMidiSnapModeSelected(SnapMode),
    PreferencesBitDepthSelected(usize),
    PreferencesOscEnabledToggled(bool),
    PreferencesOutputDeviceSelected(PreferencesDeviceOption),
    PreferencesInputDeviceSelected(PreferencesDeviceOption),
    PreferencesSave,
    SessionMetadataAuthorInput(String),
    SessionMetadataAlbumInput(String),
    SessionMetadataYearInput(String),
    SessionMetadataTrackNumberInput(String),
    SessionMetadataGenreInput(String),
    SessionMetadataSave,

    TrackResizeStart(String),
    TrackResizeHover(String, bool),
    ClipResizeHandleHover {
        kind: Kind,
        track_idx: String,
        clip_idx: usize,
        is_right_side: bool,
        hovered: bool,
    },
    TracksResizeStart,
    MixerResizeStart,
    MixerLevelEditStart(String),
    MixerLevelEditInput(String),
    MixerLevelEditCommit,
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
    TrackDrag {
        index: usize,
        position: Point,
    },
    TrackDropped(Point, Rectangle),
    TrackContextMenuHover {
        track_name: String,
        position: Point,
    },
    HandleTrackZones(Vec<(Id, Rectangle)>),

    MouseMoved(mouse::Event),
    EditorMouseMoved(Point),
    EditorScrollXChanged(f32),
    EditorScrollYChanged(f32),
    MixerScrollXChanged(f32),
    MousePressed(mouse::Button),
    MouseReleased,

    ShiftPressed,
    CtrlPressed,
    ShiftReleased,
    CtrlReleased,
    SelectAll,

    WindowResized(Size),
    WindowCloseRequested,
    EscapePressed,
    ConfirmCloseSave,
    ConfirmCloseDiscard,
    ConfirmCloseCancel,
    PlaybackTick,
    MeterPollTick,
    AutosaveSnapshotTick,
    RecordingPreviewTick,
    RecordingPreviewPeaksTick,
    ToggleTransport,
    ZoomSliderChanged(f32),
    PianoZoomXChanged(f32),
    PianoZoomYChanged(f32),
    PianoScrollChanged {
        x: f32,
        y: f32,
    },
    PianoScrollXChanged(f32),
    PianoScrollYChanged(f32),
    PianoSysExScrollYChanged(f32),
    TracksResizeHover(bool),
    MixerResizeHover(bool),

    OpenFileImporter,
    DeleteUnusedSessionMediaFiles,
    ImportFilesSelected(Option<Vec<std::path::PathBuf>>),
    ImportProgress {
        file_index: usize,
        total_files: usize,
        file_progress: f32,
        filename: String,
        operation: Option<String>,
    },
    ImportPreparedAudioPeaks {
        track_name: String,
        clip_name: String,
        start: usize,
        length: usize,
        offset: usize,
        peaks: crate::state::ClipPeaks,
    },
    ClipOpenPitchCorrectionProgress {
        clip_name: String,
        progress: f32,
        operation: Option<String>,
    },
    TrackTemplatesLoaded(Vec<String>),
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    PreferencesDevicesLoaded {
        #[cfg(target_os = "linux")]
        output_devices: Vec<AudioDeviceOption>,
        #[cfg(target_os = "linux")]
        input_devices: Vec<AudioDeviceOption>,
        #[cfg(target_os = "windows")]
        output_devices: Vec<String>,
        #[cfg(target_os = "windows")]
        input_devices: Vec<String>,
    },
    DrainAudioPeakUpdates,
    TransportPlay,
    TransportPause,
    TransportStop,
    TransportPanic,
    JumpToStart,
    JumpToEnd,
    TransportRecordToggle,
    ToggleLoop,
    SetLoopRange(Option<(usize, usize)>),
    TogglePunch,
    SetPunchRange(Option<(usize, usize)>),
    SetClipSnapTargets(Vec<crate::state::ClipId>),
    ToggleMetronome,
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
    SetMidiSnapMode(SnapMode),
    RecordFolderSelected(Option<PathBuf>),

    SendMessageFinished(Result<(), String>),

    Workspace,
    Connections,
    X32,
    ToggleMixerVisibility,
    ToggleTracksVisibility,
    ToggleEditorVisibility,
    ToggleLogVisibility,
    HwMixer(mixosc::app::Message),
    LogViewAction(text_editor::Action),
    OpenTrackPlugins(String),
    OpenHwPorts {
        input: bool,
    },
    OpenClipPlugins {
        track_idx: String,
        clip_idx: usize,
    },
    #[cfg(all(unix, not(target_os = "macos")))]
    ClipConnectPlugin {
        from_node: maolan_engine::message::PluginGraphNode,
        from_port: usize,
        to_node: maolan_engine::message::PluginGraphNode,
        to_port: usize,
        kind: maolan_engine::kind::Kind,
    },
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
    PitchCorrectionPointClick {
        point_index: usize,
        position: Point,
    },
    PitchCorrectionSnapToNearest {
        point_index: usize,
    },
    PitchCorrectionPointsDrag {
        position: Point,
    },
    PitchCorrectionPointsEndDrag,
    PitchCorrectionSelectRectStart {
        position: Point,
    },
    PitchCorrectionSelectRectDrag {
        position: Point,
    },
    PitchCorrectionSelectRectEnd,
    PitchCorrectionClearSelection,
    PitchCorrectionFrameLikenessChanged(f32),
    PitchCorrectionInertiaChanged(u16),
    PitchCorrectionFormantCompensationChanged(bool),
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
    DrumNoteSelected(usize),
    DrumNoteCreate {
        start_sample: usize,
        pitch: u8,
    },
    DrumNoteDelete(usize),
    DrumNoteMove {
        note_index: usize,
        delta_samples: i64,
    },
    PianoControllerLaneSelected(PianoControllerLane),
    MidiEditorViewModeSelected(MidiEditorViewMode),
    PianoControllerKindSelected(u8),
    TrackMidiLaneChannelSelected {
        track_name: String,
        lane: usize,
        channel: MidiLaneChannelSelection,
    },
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
    PianoHumanizeTimeAmountChanged(f32),
    PianoHumanizeVelocityAmountChanged(f32),
    PianoGrooveAmountChanged(f32),
    PianoDeleteSelectedNotes,
    PianoDeleteNotes {
        note_indices: Vec<usize>,
    },
    PianoDeleteControllers {
        controller_indices: Vec<usize>,
    },
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
        clip_idx: Option<usize>,
        instance_id: usize,
    },
    #[cfg(all(unix, not(target_os = "macos")))]
    PumpLv2Ui,
    PumpClapUi,
    RefreshVst3Plugins,
    FilterVst3Plugins(String),
    SelectVst3Plugin(String),
    LoadSelectedVst3Plugins,
    RefreshClapPlugins,
    FilterClapPlugin(String),
    SelectClapPlugin(String),
    LoadSelectedClapPlugins,
    PluginFormatSelected(PluginFormat),
    ShowClapPluginUi {
        track_name: String,
        clip_idx: Option<usize>,
        instance_id: usize,
        plugin_path: String,
    },
    OpenVst3PluginUi {
        track_name: String,
        clip_idx: Option<usize>,
        instance_id: usize,
        plugin_path: String,
    },

    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
    HWSelected(AudioDeviceOption),
    #[cfg(not(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd")))]
    HWSelected(String),
    #[cfg(any(target_os = "freebsd", target_os = "linux", target_os = "openbsd"))]
    HWInputSelected(AudioDeviceOption),
    #[cfg(target_os = "windows")]
    HWInputSelected(String),
    HWBackendSelected(AudioBackendOption),
    HWExclusiveToggled(bool),
    #[cfg(any(unix, target_os = "windows"))]
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
    GroupSelectedClips,
    UngroupClip {
        track_idx: String,
        clip_idx: usize,
        kind: Kind,
    },
    ClipStretchFinished {
        request: ClipStretchRequest,
        result: Result<(String, usize), String>,
    },
    ClipOpenPitchCorrection {
        track_idx: String,
        clip_idx: usize,
    },
    ClipOpenPitchCorrectionFinished {
        request: ClipPitchCorrectionRequest,
        result: Result<crate::state::PitchCorrectionData, String>,
    },
    TrackRenameShow(String),
    TrackRenameInput(String),
    TrackRenameConfirm,
    TrackRenameCancel,
    TrackGroupShow {
        track_name: String,
    },
    TrackGroupInput(String),
    TrackGroupConfirm,
    TrackGroupCancel,

    TrackTemplateSaveShow(String),
    TrackTemplateSaveInput(String),
    TrackTemplateSaveConfirm,
    TrackTemplateSaveCancel,
    TrackGroupTemplateSaveShow(String),
    TrackGroupTemplateSaveInput(String),
    TrackGroupTemplateSaveConfirm,
    TrackGroupTemplateSaveCancel,
    TrackContextMenuToggle(String),

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
    AddGroupFromTemplate {
        base_name: String,
        template: String,
    },

    GroupTemplatesLoaded(Vec<String>),

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_sample_rounds_to_nearest_interval_when_enabled() {
        let snapped = SnapMode::Beat.snap_sample(740.0, 480.0, 1920.0);
        assert_eq!(snapped, 960.0);
    }

    #[test]
    fn snap_sample_leaves_position_when_snap_disabled() {
        let snapped = SnapMode::NoSnap.snap_sample(740.0, 480.0, 1920.0);
        assert_eq!(snapped, 740.0);
    }

    #[test]
    fn snap_mode_interval_samples_bar() {
        assert_eq!(SnapMode::Bar.interval_samples(480.0, 1920.0), 1920.0);
    }

    #[test]
    fn snap_mode_interval_samples_beat() {
        assert_eq!(SnapMode::Beat.interval_samples(480.0, 1920.0), 480.0);
    }

    #[test]
    fn snap_mode_interval_samples_eighth() {
        assert_eq!(SnapMode::Eighth.interval_samples(480.0, 1920.0), 240.0);
    }

    #[test]
    fn snap_mode_interval_samples_sixteenth() {
        assert_eq!(SnapMode::Sixteenth.interval_samples(480.0, 1920.0), 120.0);
    }

    #[test]
    fn snap_mode_interval_samples_no_snap() {
        assert_eq!(SnapMode::NoSnap.interval_samples(480.0, 1920.0), 1.0);
    }

    #[test]
    fn snap_mode_interval_samples_clips() {
        assert_eq!(SnapMode::Clips.interval_samples(480.0, 1920.0), 1.0);
    }

    #[test]
    fn snap_sample_drag_positive_delta() {
        let result = SnapMode::Beat.snap_sample_drag(740.0, 10.0, 480.0, 1920.0);
        assert_eq!(result, 480.0);
    }

    #[test]
    fn snap_sample_drag_negative_delta() {
        let result = SnapMode::Beat.snap_sample_drag(740.0, -10.0, 480.0, 1920.0);
        assert_eq!(result, 960.0);
    }

    #[test]
    fn snap_sample_bar_vs_beat_produces_different_results() {
        // At 120 BPM 4/4, 48kHz: beat = 24000 samples, bar = 96000 samples
        let beat_snapped = SnapMode::Beat.snap_sample(30_000.0, 24_000.0, 96_000.0);
        let bar_snapped = SnapMode::Bar.snap_sample(30_000.0, 24_000.0, 96_000.0);
        // 30000 is closer to beat 1 (24000) than beat 2 (48000)
        assert_eq!(beat_snapped, 24_000.0);
        // 30000 is closer to bar 0 (0) than bar 1 (96000)
        assert_eq!(bar_snapped, 0.0);
    }

    #[test]
    fn snap_sample_sixteenth_snaps_to_grid() {
        let snapped = SnapMode::Sixteenth.snap_sample(7_000.0, 24_000.0, 96_000.0);
        // 6000 is the nearest sixteenth boundary (24000/4 = 6000)
        assert_eq!(snapped, 6_000.0);
    }

    #[test]
    fn midi_lane_channel_selection_from_engine_some() {
        assert_eq!(
            MidiLaneChannelSelection::from_engine(Some(5)),
            MidiLaneChannelSelection::Channel(5)
        );
    }

    #[test]
    fn midi_lane_channel_selection_from_engine_none() {
        assert_eq!(
            MidiLaneChannelSelection::from_engine(None),
            MidiLaneChannelSelection::Omni
        );
    }

    #[test]
    fn midi_lane_channel_selection_to_engine_omni() {
        assert_eq!(MidiLaneChannelSelection::Omni.to_engine(), None);
    }

    #[test]
    fn midi_lane_channel_selection_to_engine_channel() {
        assert_eq!(MidiLaneChannelSelection::Channel(7).to_engine(), Some(7));
    }

    #[test]
    fn midi_lane_channel_selection_to_engine_clamps() {
        assert_eq!(MidiLaneChannelSelection::Channel(20).to_engine(), Some(15));
    }

    #[test]
    fn burn_backend_option_as_ipc_str() {
        assert_eq!(BurnBackendOption::Cpu.as_ipc_str(), "cpu");
        assert_eq!(BurnBackendOption::Vulkan.as_ipc_str(), "vulkan");
    }

    #[test]
    fn plugin_format_display() {
        assert_eq!(format!("{}", PluginFormat::Lv2), "LV2");
        assert_eq!(format!("{}", PluginFormat::Clap), "CLAP");
        assert_eq!(format!("{}", PluginFormat::Vst3), "VST3");
    }

    #[test]
    fn snap_mode_display() {
        assert_eq!(format!("{}", SnapMode::NoSnap), "No Snap");
        assert_eq!(format!("{}", SnapMode::Bar), "Bar");
        assert_eq!(format!("{}", SnapMode::Beat), "Beat");
        assert_eq!(format!("{}", SnapMode::Eighth), "1/8");
        assert_eq!(format!("{}", SnapMode::Sixteenth), "1/16");
    }

    #[test]
    fn track_automation_mode_display() {
        assert_eq!(format!("{}", TrackAutomationMode::Read), "Read");
        assert_eq!(format!("{}", TrackAutomationMode::Touch), "Touch");
        assert_eq!(format!("{}", TrackAutomationMode::Latch), "Latch");
        assert_eq!(format!("{}", TrackAutomationMode::Write), "Write");
    }

    #[test]
    fn piano_scale_root_semitone() {
        assert_eq!(PianoScaleRoot::C.semitone(), 0);
        assert_eq!(PianoScaleRoot::CSharp.semitone(), 1);
        assert_eq!(PianoScaleRoot::D.semitone(), 2);
        assert_eq!(PianoScaleRoot::A.semitone(), 9);
        assert_eq!(PianoScaleRoot::B.semitone(), 11);
    }

    #[test]
    fn piano_scale_root_display() {
        assert_eq!(format!("{}", PianoScaleRoot::C), "C");
        assert_eq!(format!("{}", PianoScaleRoot::CSharp), "C#");
        assert_eq!(format!("{}", PianoScaleRoot::FSharp), "F#");
    }

    #[test]
    fn piano_chord_kind_intervals() {
        assert_eq!(PianoChordKind::MajorTriad.intervals(), &[4, 7]);
        assert_eq!(PianoChordKind::MinorTriad.intervals(), &[3, 7]);
        assert_eq!(PianoChordKind::Dominant7.intervals(), &[4, 7, 10]);
        assert_eq!(PianoChordKind::Major7.intervals(), &[4, 7, 11]);
        assert_eq!(PianoChordKind::Minor7.intervals(), &[3, 7, 10]);
    }

    #[test]
    fn piano_chord_kind_display() {
        assert_eq!(format!("{}", PianoChordKind::MajorTriad), "Maj");
        assert_eq!(format!("{}", PianoChordKind::MinorTriad), "Min");
        assert_eq!(format!("{}", PianoChordKind::Dominant7), "7");
    }

    #[test]
    fn piano_velocity_kind_display() {
        assert_eq!(
            format!("{}", PianoVelocityKind::NoteVelocity),
            "Note Velocity"
        );
        assert_eq!(
            format!("{}", PianoVelocityKind::ReleaseVelocity),
            "Release Velocity"
        );
    }

    #[test]
    fn generate_audio_model_option_display() {
        assert_eq!(
            format!("{}", GenerateAudioModelOption::HappyNewYear),
            "happy-new-year"
        );
        assert_eq!(format!("{}", GenerateAudioModelOption::Rl), "RL");
    }
}
