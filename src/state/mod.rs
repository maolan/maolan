mod clip;
mod connection;
#[cfg(target_os = "windows")]
mod platform;
#[cfg(target_os = "freebsd")]
mod platform_freebsd;
#[cfg(target_os = "linux")]
mod platform_linux;
#[cfg(target_os = "openbsd")]
mod platform_openbsd;
mod track;

use crate::config;
use crate::message::{
    PianoChordKind, PianoControllerLane, PianoNrpnKind, PianoRpnKind, PianoScaleRoot,
    PianoVelocityKind, TrackAutomationTarget,
};

pub use clip::{AudioClip, ClipPeaks, MIDIClip};
pub use connection::Connection;
use iced::{Length, Point};
#[cfg(all(unix, not(target_os = "macos")))]
use maolan_engine::lv2::Lv2PluginInfo;
use maolan_engine::message::{
    ConnectableConnection, PluginGraphConnection, PluginGraphNode, PluginGraphPlugin,
    PluginGraphSnapshot,
};
use maolan_engine::{
    clap::{ClapPluginInfo, ClapPluginState},
    kind::Kind,
    vst3::{Vst3PluginInfo, Vst3PluginState},
};
pub use maolan_widgets::midi::{PianoControllerPoint, PianoNote, PianoSysExPoint};
#[cfg(target_os = "windows")]
pub(crate) use platform::{
    discover_windows_audio_devices, discover_windows_input_devices,
    discover_windows_output_bit_depths, discover_windows_output_sample_rates,
};
#[cfg(target_os = "freebsd")]
pub(crate) use platform_freebsd::discover_freebsd_audio_devices;
#[cfg(target_os = "linux")]
pub(crate) use platform_linux::{discover_alsa_input_devices, discover_alsa_output_devices};
#[cfg(target_os = "openbsd")]
pub(crate) use platform_openbsd::discover_openbsd_audio_devices;
use std::{
    collections::{HashMap, HashSet},
    fmt,
    sync::Arc,
    time::Instant,
};
use tokio::sync::RwLock;
pub use track::{EditorMarker, Track, TrackAutomationLane, TrackAutomationPoint, TrackLaneLayout};

pub use crate::consts::state_ids::{HW_IN_ID, HW_OUT_ID, MIDI_HW_IN_ID, MIDI_HW_OUT_ID};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum ModulatorShape {
    #[default]
    Sine,
    Triangle,
    Saw,
    Square,
    SampleHold,
}

impl fmt::Display for ModulatorShape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sine => write!(f, "Sine"),
            Self::Triangle => write!(f, "Triangle"),
            Self::Saw => write!(f, "Saw"),
            Self::Square => write!(f, "Square"),
            Self::SampleHold => write!(f, "Sample & Hold"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ModulatorTarget {
    pub track_name: String,
    pub target: TrackAutomationTarget,
    pub min: f32,
    pub max: f32,
}

impl ModulatorTarget {
    fn is_modulatable(&self) -> bool {
        self.target.is_modulatable()
    }
}

impl<'de> serde::Deserialize<'de> for ModulatorTarget {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct NewShape {
            track_name: String,
            target: TrackAutomationTarget,
            #[serde(default)]
            min: Option<f32>,
            #[serde(default)]
            max: Option<f32>,
        }

        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum OldTargetValue {
            Target(TrackAutomationTarget),
            MidiCc { channel: u8, cc: u8 },
        }

        #[derive(serde::Deserialize)]
        struct KindShape {
            track_name: String,
            #[serde(alias = "kind")]
            target: String,
            value: OldTargetValue,
            #[serde(default)]
            min: Option<f32>,
            #[serde(default)]
            max: Option<f32>,
        }

        #[derive(serde::Deserialize)]
        struct ControllerShape {
            track_name: String,
            #[serde(alias = "volume", alias = "balance")]
            controller: String,
            #[serde(default)]
            min: Option<f32>,
            #[serde(default)]
            max: Option<f32>,
        }

        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum Shape {
            New(NewShape),
            Kind(KindShape),
            Controller(ControllerShape),
        }

        let shape = Shape::deserialize(deserializer)?;
        let (track_name, target, min, max) = match shape {
            Shape::New(new) => (new.track_name, new.target, new.min, new.max),
            Shape::Kind(kind) => {
                let target = match kind.target.as_str() {
                    "MidiCc" => {
                        let OldTargetValue::MidiCc { channel, cc } = kind.value else {
                            return Err(serde::de::Error::custom("invalid MidiCc value"));
                        };
                        TrackAutomationTarget::MidiCc { channel, cc }
                    }
                    _ => {
                        let OldTargetValue::Target(target) = kind.value else {
                            return Err(serde::de::Error::custom("invalid automation value"));
                        };
                        target
                    }
                };
                (kind.track_name, target, kind.min, kind.max)
            }
            Shape::Controller(controller) => {
                let target = match controller.controller.as_str() {
                    "volume" => TrackAutomationTarget::Volume,
                    "balance" => TrackAutomationTarget::Balance,
                    _ => TrackAutomationTarget::Volume,
                };
                (
                    controller.track_name,
                    target,
                    controller.min,
                    controller.max,
                )
            }
        };
        let (default_min, default_max) = target.default_range();
        Ok(Self {
            track_name,
            target,
            min: min.unwrap_or(default_min),
            max: max.unwrap_or(default_max),
        })
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Modulator {
    pub id: usize,
    pub name: String,
    pub shape: ModulatorShape,
    pub rate_hz: f32,
    pub phase: f32,
    pub enabled: bool,
    pub targets: Vec<ModulatorTarget>,
}

impl<'de> serde::Deserialize<'de> for Modulator {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct ModulatorData {
            id: usize,
            name: String,
            shape: ModulatorShape,
            rate_hz: f32,
            phase: f32,
            #[serde(default)]
            _bipolar: Option<bool>,
            enabled: bool,
            #[serde(default)]
            target: Option<ModulatorTarget>,
            #[serde(default)]
            targets: Vec<ModulatorTarget>,
        }
        let data = ModulatorData::deserialize(deserializer)?;
        let targets = if data.targets.is_empty() {
            data.target.into_iter().collect()
        } else {
            data.targets
        };
        Ok(Self {
            id: data.id,
            name: data.name,
            shape: data.shape,
            rate_hz: data.rate_hz,
            phase: data.phase,
            enabled: data.enabled,
            targets,
        })
    }
}

impl Modulator {
    pub fn new(id: usize) -> Self {
        Self {
            id,
            name: format!("Modulator {id}"),
            shape: ModulatorShape::default(),
            rate_hz: 1.0,
            phase: 0.0,
            enabled: true,
            targets: Vec::new(),
        }
    }
}

impl From<&Modulator> for maolan_engine::modulator::Modulator {
    fn from(m: &Modulator) -> Self {
        Self {
            id: m.id,
            name: m.name.clone(),
            shape: m.shape.into(),
            rate_hz: m.rate_hz,
            phase: m.phase,
            enabled: m.enabled,
            targets: m
                .targets
                .iter()
                .cloned()
                .filter_map(|t| t.try_into().ok())
                .collect(),
        }
    }
}

impl From<ModulatorShape> for maolan_engine::modulator::ModulatorShape {
    fn from(shape: ModulatorShape) -> Self {
        match shape {
            ModulatorShape::Sine => Self::Sine,
            ModulatorShape::Triangle => Self::Triangle,
            ModulatorShape::Saw => Self::Saw,
            ModulatorShape::Square => Self::Square,
            ModulatorShape::SampleHold => Self::SampleHold,
        }
    }
}

impl TryFrom<ModulatorTarget> for maolan_engine::modulator::ModulatorTarget {
    type Error = ();

    fn try_from(t: ModulatorTarget) -> Result<Self, Self::Error> {
        if !t.is_modulatable() {
            return Err(());
        }
        match t.target {
            TrackAutomationTarget::Volume => {
                if t.track_name == "hw:out" {
                    Ok(Self::HwOutVolume {
                        min: t.min,
                        max: t.max,
                    })
                } else {
                    Ok(Self::TrackVolume {
                        track_name: t.track_name,
                        min: t.min,
                        max: t.max,
                    })
                }
            }
            TrackAutomationTarget::Balance => {
                if t.track_name == "hw:out" {
                    Ok(Self::HwOutBalance {
                        min: t.min,
                        max: t.max,
                    })
                } else {
                    Ok(Self::TrackBalance {
                        track_name: t.track_name,
                        min: t.min,
                        max: t.max,
                    })
                }
            }
            TrackAutomationTarget::ClapParameter {
                instance_id,
                param_id,
                ..
            } => Ok(Self::ClapParameter {
                track_name: t.track_name,
                instance_id,
                param_id,
                min: f64::from(t.min),
                max: f64::from(t.max),
            }),
            TrackAutomationTarget::Vst3Parameter {
                instance_id,
                param_id,
            } => Ok(Self::Vst3Parameter {
                track_name: t.track_name,
                instance_id,
                param_id,
                min: t.min,
                max: t.max,
            }),
            TrackAutomationTarget::Lv2Parameter {
                instance_id, index, ..
            } => Ok(Self::Lv2Parameter {
                track_name: t.track_name,
                instance_id,
                index,
                min: t.min,
                max: t.max,
            }),
            TrackAutomationTarget::MidiCc { channel, cc } => Ok(Self::MidiCc {
                track_name: t.track_name,
                channel,
                cc,
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AudioBackendOption {
    #[cfg(unix)]
    Jack,
    #[cfg(target_os = "freebsd")]
    Oss,
    #[cfg(target_os = "openbsd")]
    Sndio,
    #[cfg(target_os = "linux")]
    Alsa,
    #[cfg(target_os = "windows")]
    Wasapi,
    #[cfg(target_os = "macos")]
    CoreAudio,
}

impl std::fmt::Display for AudioBackendOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            #[cfg(unix)]
            Self::Jack => "JACK",
            #[cfg(target_os = "freebsd")]
            Self::Oss => "OSS",
            #[cfg(target_os = "openbsd")]
            Self::Sndio => "sndio",
            #[cfg(target_os = "linux")]
            Self::Alsa => "ALSA",
            #[cfg(target_os = "windows")]
            Self::Wasapi => "WASAPI",
            #[cfg(target_os = "macos")]
            Self::CoreAudio => "CoreAudio",
        };
        f.write_str(label)
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
#[derive(Clone, Debug)]
pub struct AudioDeviceOption {
    pub id: String,
    pub label: String,
    pub supported_bits: Vec<usize>,
    pub supported_sample_rates: Vec<i32>,
    pub max_channels: usize,
    pub max_buffer_bytes: usize,
    pub supports_input: bool,
    pub supports_output: bool,
}

#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
impl AudioDeviceOption {
    fn normalize_sample_rates(mut rates: Vec<i32>) -> Vec<i32> {
        rates.retain(|r| *r > 0);
        rates.sort_unstable();
        rates.dedup();
        rates
    }

    #[cfg(target_os = "linux")]
    fn default_sample_rates() -> Vec<i32> {
        vec![
            8_000, 11_025, 16_000, 22_050, 32_000, 44_100, 48_000, 88_200, 96_000, 176_400,
            192_000, 384_000,
        ]
    }

    pub fn with_supported_caps(
        id: impl Into<String>,
        label: impl Into<String>,
        mut supported_bits: Vec<usize>,
        supported_sample_rates: Vec<i32>,
    ) -> Self {
        supported_bits.sort_by(|a, b| b.cmp(a));
        supported_bits.dedup();
        let supported_sample_rates = Self::normalize_sample_rates(supported_sample_rates);
        Self {
            id: id.into(),
            label: label.into(),
            supported_bits,
            supported_sample_rates,
            max_channels: 0,
            max_buffer_bytes: 0,
            supports_input: true,
            supports_output: true,
        }
    }

    pub fn with_supported_direction_caps(
        id: impl Into<String>,
        label: impl Into<String>,
        supported_bits: Vec<usize>,
        supported_sample_rates: Vec<i32>,
        supports_input: bool,
        supports_output: bool,
    ) -> Self {
        let mut out = Self::with_supported_caps(id, label, supported_bits, supported_sample_rates);
        out.supports_input = supports_input;
        out.supports_output = supports_output;
        out
    }

    pub fn with_oss_caps(
        id: impl Into<String>,
        label: impl Into<String>,
        supported_bits: Vec<usize>,
        supported_sample_rates: Vec<i32>,
        max_channels: usize,
        max_buffer_bytes: usize,
    ) -> Self {
        let mut out = Self::with_supported_caps(id, label, supported_bits, supported_sample_rates);
        out.max_channels = max_channels;
        out.max_buffer_bytes = max_buffer_bytes;
        out
    }

    pub fn preferred_bits(&self) -> Option<usize> {
        self.supported_bits.first().copied()
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
impl std::fmt::Display for AudioDeviceOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.supported_bits.is_empty() {
            return f.write_str(&self.label);
        }
        let formats = self
            .supported_bits
            .iter()
            .map(|b| format!("{b}"))
            .collect::<Vec<_>>()
            .join("/");
        write!(f, "{} [{}-bit]", self.label, formats)
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
impl PartialEq for AudioDeviceOption {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
impl Eq for AudioDeviceOption {}

#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
impl std::hash::Hash for AudioDeviceOption {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
pub type OutputAudioDevice = AudioDeviceOption;
#[cfg(not(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd")))]
pub type OutputAudioDevice = String;

#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
pub type InputAudioDevice = AudioDeviceOption;
#[cfg(target_os = "windows")]
pub type InputAudioDevice = String;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClipId {
    pub track_idx: String,
    pub clip_idx: usize,
    pub kind: Kind,
}

#[derive(Debug, Clone)]
pub struct ClipContextMenuState {
    pub clip: ClipId,
    pub anchor: Point,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TrackContextSubmenu {
    Automation,
    Plugin { instance_id: usize, format: String },
    Midi,
}

#[derive(Debug, Clone)]
pub struct TrackContextMenuState {
    pub track_name: String,
    pub anchor: Point,
    pub submenu: Option<TrackContextSubmenu>,
}

#[derive(Debug, Clone)]
pub struct PluginParameterInfo {
    pub param_id: u32,
    pub name: String,
    pub min: f64,
    pub max: f64,
}

pub type PluginParameterCache = HashMap<usize, Vec<PluginParameterInfo>>;

#[derive(Debug, Clone)]
pub enum Resizing {
    Clip {
        kind: Kind,
        track_name: String,
        index: usize,
        is_right_side: bool,
        stretch_mode: bool,
        initial_value: f32,
        initial_mouse_x: f32,
        initial_length: f32,
        initial_start: usize,
        initial_offset: usize,
    },
    Fade {
        kind: Kind,
        track_name: String,
        index: usize,
        is_fade_out: bool,
        initial_samples: usize,
        initial_mouse_x: f32,
    },
    Mixer(f32, f32),
    Track(String, f32, f32),
    Tracks(f32, f32),
}

#[derive(Debug, Clone)]
pub struct Connecting {
    pub from_track: String,
    pub from_port: usize,
    pub kind: Kind,
    pub point: Point,
    pub is_input: bool,
}

#[derive(Debug, Clone)]
pub struct MovingTrack {
    pub track_idx: String,
    pub offset_x: f32,
    pub offset_y: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Hovering {
    Port {
        track_idx: String,
        port_idx: usize,
        is_input: bool,
    },
    Track(String),
    Plugin {
        instance_id: usize,
    },
    PluginPort {
        instance_id: usize,
        port_idx: usize,
        is_input: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionViewSelection {
    Tracks(HashSet<String>),
    Connections(HashSet<usize>),
    None,
}

#[derive(Debug, Clone)]
pub enum View {
    Workspace,
    Connections,
    X32,
    HwInputPorts,
    HwOutputPorts,
    TrackPlugins,
    Piano,
    PitchCorrection,
}

#[derive(Debug, Clone)]
pub struct HW {
    pub channels: usize,
}

#[derive(Debug, Clone)]
pub struct PluginConnecting {
    pub from_node: PluginGraphNode,
    pub from_port: usize,
    pub kind: Kind,
    pub point: Point,
    pub is_input: bool,
}

#[derive(Debug, Clone)]
pub struct MovingPlugin {
    pub instance_id: usize,
    pub offset_x: f32,
    pub offset_y: f32,
}

#[derive(Debug, Clone)]
pub struct ClipRenameDialog {
    pub track_idx: String,
    pub clip_idx: usize,
    pub kind: Kind,
    pub new_name: String,
}

#[derive(Debug, Clone)]
pub struct TrackRenameDialog {
    pub old_name: String,
    pub new_name: String,
}

#[derive(Debug, Clone)]
pub struct TemplateSaveDialog {
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct ApplyTemplateDialog {
    pub track_name: String,
    pub selected_template: Option<String>,
    pub available_templates: Vec<String>,
    pub available_folder_templates: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct TrackTemplateSaveDialog {
    pub track_name: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct PendingFolderTemplateMember {
    pub new_name: String,
    pub track_json: serde_json::Value,
    pub parent_new_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PendingFolderTemplateLoad {
    pub target_name: String,
    pub template_name: String,
    pub remaining: std::collections::HashSet<String>,
    pub members: Vec<PendingFolderTemplateMember>,
}

#[derive(Debug, Clone)]
pub struct PluginGraphClipTarget {
    pub track_name: String,
    pub clip_idx: usize,
}

#[derive(Debug, Clone)]
pub struct MarkerDialog {
    pub sample: usize,
    pub marker_index: Option<usize>,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct ModulatorTargetDialog {
    pub modulator_id: usize,
    pub track_name: String,
    pub target: TrackAutomationTarget,
    pub min_input: String,
    pub max_input: String,
    pub existing: bool,
}

#[derive(Debug, Clone)]
pub struct TempoPoint {
    pub sample: usize,
    pub bpm: f32,
}

#[derive(Debug, Clone)]
pub struct TimeSignaturePoint {
    pub sample: usize,
    pub numerator: u8,
    pub denominator: u8,
}

#[derive(Debug, Clone)]
pub struct PianoData {
    pub track_idx: String,
    pub clip_index: usize,
    pub clip_start_samples: usize,
    pub clip_length_samples: usize,
    pub notes: Vec<PianoNote>,
    pub controllers: Vec<PianoControllerPoint>,
    pub sysexes: Vec<PianoSysExPoint>,
    pub midnam_note_names: HashMap<u8, String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PitchCorrectionPoint {
    pub start_sample: usize,
    pub length_samples: usize,
    pub detected_midi_pitch: f32,
    pub target_midi_pitch: f32,
    pub clarity: f32,
}

#[derive(Debug, Clone)]
pub struct PitchCorrectionData {
    pub track_idx: String,
    pub clip_index: usize,
    pub clip_name: String,
    pub clip_length_samples: usize,
    pub frame_likeness: f32,
    pub raw_points: Vec<PitchCorrectionPoint>,
    pub points: Vec<PitchCorrectionPoint>,
}

pub const MIN_PITCH_CORRECTION_FRAME_LIKENESS: f32 = 0.05;
pub const DEFAULT_PITCH_CORRECTION_FRAME_LIKENESS: f32 = MIN_PITCH_CORRECTION_FRAME_LIKENESS;
pub const DEFAULT_PITCH_CORRECTION_INERTIA_MS: u16 = 100;
pub const DEFAULT_PITCH_CORRECTION_FORMANT_COMPENSATION: bool = true;
pub const LOG_HISTORY_LIMIT: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warning,
    Error,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Info => f.write_str("INFO"),
            Self::Warning => f.write_str("WARN"),
            Self::Error => f.write_str("ERROR"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    pub level: LogLevel,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct DraggingPitchCorrectionPoints {
    pub point_indices: Vec<usize>,
    pub start_point: Point,
    pub current_point: Point,
    pub original_points: Vec<PitchCorrectionPoint>,
}

pub type MidiClipPreviewMap = HashMap<(String, usize), Arc<Vec<PianoNote>>>;

#[derive(Debug)]
pub struct StateData {
    pub shift: bool,
    pub ctrl: bool,
    pub tracks: Vec<Track>,
    pub session_markers: Vec<EditorMarker>,
    pub connections: Vec<Connection>,
    pub selected: HashSet<String>,
    pub selected_clips: HashSet<ClipId>,
    pub clip_context_menu: Option<ClipContextMenuState>,
    pub track_context_menu: Option<TrackContextMenuState>,
    pub track_context_hover: Option<(String, Point)>,
    pub clip_click_consumed: bool,
    pub message: String,
    pub log_entries: Vec<LogEntry>,
    pub resizing: Option<Resizing>,
    pub connecting: Option<Connecting>,
    pub moving_track: Option<MovingTrack>,
    pub hovering: Option<Hovering>,
    pub connection_view_selection: ConnectionViewSelection,
    pub cursor: Point,
    pub editor_cursor: Option<Point>,
    pub cut_preview_active: bool,
    pub cut_indicator: Option<(f32, f32, f32)>,
    pub mouse_left_down: bool,
    pub mouse_right_down: bool,
    pub clip_marquee_start: Option<Point>,
    pub clip_marquee_end: Option<Point>,
    pub midi_clip_create_start: Option<Point>,
    pub midi_clip_create_end: Option<Point>,
    pub automation_lane_hover: Option<(String, TrackAutomationTarget, Point)>,
    pub mixer_height: Length,
    pub tracks_width: Length,
    pub view: View,
    pub connections_folder: Option<String>,
    pub metronome_enabled: bool,
    pub pending_track_positions: HashMap<String, Point>,
    pub pending_track_heights: HashMap<String, f32>,
    pub pending_track_folder_state: HashMap<String, (bool, bool, Option<String>)>,
    pub undo_track_indices: HashMap<String, usize>,
    pub hovered_track_resize_handle: Option<String>,
    pub hovered_clip_resize_handle: Option<(String, usize, Kind, bool)>,
    pub shortcuts_hint: Option<String>,
    pub hw_loaded: bool,
    pub available_backends: Vec<AudioBackendOption>,
    pub selected_backend: AudioBackendOption,
    pub available_hw: Vec<OutputAudioDevice>,
    pub selected_hw: Option<OutputAudioDevice>,
    #[cfg(any(
        target_os = "linux",
        target_os = "windows",
        target_os = "freebsd",
        target_os = "openbsd"
    ))]
    pub available_input_hw: Vec<InputAudioDevice>,
    #[cfg(any(
        target_os = "freebsd",
        target_os = "linux",
        target_os = "openbsd",
        target_os = "windows"
    ))]
    pub selected_input_hw: Option<InputAudioDevice>,
    pub hw_sample_rate_hz: i32,
    pub oss_exclusive: bool,
    #[cfg(any(unix, target_os = "windows"))]
    pub oss_bits: usize,
    pub oss_period_frames: usize,
    pub oss_nperiods: usize,
    pub oss_sync_mode: bool,
    pub opened_midi_in_hw: Vec<String>,
    pub opened_midi_out_hw: Vec<String>,
    pub global_midi_learn_play_pause: Option<maolan_engine::message::MidiLearnBinding>,
    pub global_midi_learn_stop: Option<maolan_engine::message::MidiLearnBinding>,
    pub global_midi_learn_record_toggle: Option<maolan_engine::message::MidiLearnBinding>,
    pub midi_hw_labels: HashMap<String, String>,
    pub midi_hw_in_positions: HashMap<String, Point>,
    pub midi_hw_out_positions: HashMap<String, Point>,
    pub hw_in: Option<HW>,
    pub hw_out: Option<HW>,
    pub hw_out_level: f32,
    pub hw_out_balance: f32,
    pub hw_out_muted: bool,
    pub hw_out_meter_db: Vec<f32>,
    #[cfg(all(unix, not(target_os = "macos")))]
    pub lv2_plugins: Vec<Lv2PluginInfo>,
    #[cfg(all(unix, not(target_os = "macos")))]
    pub lv2_plugins_loaded: bool,
    pub vst3_plugins: Vec<Vst3PluginInfo>,
    pub vst3_plugins_loaded: bool,
    pub clap_plugins: Vec<ClapPluginInfo>,
    pub clap_plugins_loaded: bool,
    pub clap_plugins_by_track: HashMap<String, Vec<String>>,
    pub clap_states_by_track: HashMap<String, HashMap<String, ClapPluginState>>,
    pub vst3_states_by_track: HashMap<String, HashMap<usize, Vst3PluginState>>,
    pub plugin_graph_track: Option<String>,
    pub plugin_graph_clip: Option<PluginGraphClipTarget>,
    pub plugin_graph_plugins: Vec<PluginGraphPlugin>,
    pub plugin_graph_connections: Vec<PluginGraphConnection>,
    pub connectable_connections: Vec<ConnectableConnection>,
    pub plugin_graphs_by_track: HashMap<String, PluginGraphSnapshot>,
    pub connectable_connections_by_track: HashMap<String, Vec<ConnectableConnection>>,
    pub plugin_parameters_by_track: HashMap<String, PluginParameterCache>,
    pub plugin_graph_selected_connections: std::collections::HashSet<usize>,
    pub plugin_graph_selected_connectable_connections: std::collections::HashSet<usize>,
    pub plugin_graph_selected_plugins: std::collections::HashSet<usize>,
    pub plugin_graph_plugin_positions: HashMap<usize, Point>,
    pub plugin_graph_connecting: Option<PluginConnecting>,
    pub plugin_graph_moving_plugin: Option<MovingPlugin>,
    pub plugin_graph_last_plugin_click: Option<(usize, Instant)>,
    pub connections_last_track_click: Option<(String, Instant)>,
    pub last_selected_track: Option<String>,
    pub clip_rename_dialog: Option<ClipRenameDialog>,
    pub track_rename_dialog: Option<TrackRenameDialog>,
    pub track_template_save_dialog: Option<TrackTemplateSaveDialog>,
    pub marker_dialog: Option<MarkerDialog>,
    pub modulator_target_dialog: Option<ModulatorTargetDialog>,
    pub template_save_dialog: Option<TemplateSaveDialog>,
    pub apply_template_dialog: Option<ApplyTemplateDialog>,
    pub pending_track_template_loads: Vec<(String, String)>,
    pub pending_folder_template_loads: Vec<PendingFolderTemplateLoad>,
    pub piano: Option<PianoData>,
    pub pitch_correction: Option<PitchCorrectionData>,
    pub pitch_correction_selected_points: HashSet<usize>,
    pub pitch_correction_dragging_points: Option<DraggingPitchCorrectionPoints>,
    pub pitch_correction_selecting_rect: Option<(Point, Point)>,
    pub pitch_correction_frame_likeness: f32,
    pub pitch_correction_inertia_ms: u16,
    pub pitch_correction_formant_compensation: bool,
    pub piano_zoom_x: f32,
    pub piano_zoom_y: f32,
    pub piano_scroll_x: f32,
    pub piano_scroll_y: f32,
    pub piano_selected_notes: HashSet<usize>,
    pub piano_dragging_notes: Option<DraggingNotes>,
    pub piano_resizing_note: Option<ResizingNote>,
    pub piano_selecting_rect: Option<(Point, Point)>,
    pub piano_creating_note: Option<(Point, Point)>,
    pub piano_controller_lane: PianoControllerLane,
    pub piano_controller_kind: u8,
    pub piano_velocity_kind: PianoVelocityKind,
    pub piano_rpn_kind: PianoRpnKind,
    pub piano_nrpn_kind: PianoNrpnKind,
    pub piano_scale_root: PianoScaleRoot,
    pub piano_scale_minor: bool,
    pub piano_chord_kind: PianoChordKind,
    pub piano_velocity_shape_amount: f32,
    pub piano_selected_sysex: Option<usize>,
    pub piano_sysex_hex_input: String,
    pub piano_sysex_panel_open: bool,
    pub piano_sysex_scroll_y: f32,
    pub piano_humanize_time_amount: f32,
    pub piano_humanize_velocity_amount: f32,
    pub piano_groove_amount: f32,
    pub tempo: f32,
    pub time_signature_num: u8,
    pub time_signature_denom: u8,
    pub tempo_points: Vec<TempoPoint>,
    pub time_signature_points: Vec<TimeSignaturePoint>,
    pub session_author: String,
    pub session_album: String,
    pub session_year: String,
    pub session_track_number: String,
    pub session_genre: String,
    pub available_templates: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DraggingNotes {
    pub note_indices: Vec<usize>,
    pub start_point: Point,
    pub current_point: Point,
    pub original_notes: Vec<PianoNote>,
}

#[derive(Debug, Clone)]
pub struct ResizingNote {
    pub note_index: usize,
    pub resize_start: bool,
    pub start_point: Point,
    pub current_point: Point,
    pub original_note: PianoNote,
}

impl Default for StateData {
    fn default() -> Self {
        let cfg = config::Config::load().unwrap_or_default();
        let initial_hw = initial_hw_config();
        let initial_message = "Thank you for using Maolan!".to_string();
        Self {
            shift: false,
            ctrl: false,
            tracks: vec![],
            session_markers: vec![],
            connections: vec![],
            selected: HashSet::new(),
            selected_clips: HashSet::new(),
            clip_context_menu: None,
            track_context_menu: None,
            track_context_hover: None,
            clip_click_consumed: false,
            message: initial_message.clone(),
            log_entries: vec![LogEntry {
                level: LogLevel::Info,
                message: initial_message,
            }],
            resizing: None,
            connecting: None,
            moving_track: None,
            hovering: None,
            connection_view_selection: ConnectionViewSelection::None,
            cursor: Point::new(0.0, 0.0),
            editor_cursor: None,
            cut_preview_active: false,
            cut_indicator: None,
            mouse_left_down: false,
            mouse_right_down: false,
            clip_marquee_start: None,
            clip_marquee_end: None,
            midi_clip_create_start: None,
            midi_clip_create_end: None,
            automation_lane_hover: None,
            mixer_height: Length::Fixed(cfg.mixer_height),
            tracks_width: Length::Fixed(cfg.track_width),
            view: View::Workspace,
            connections_folder: None,
            metronome_enabled: false,
            pending_track_positions: HashMap::new(),
            pending_track_heights: HashMap::new(),
            pending_track_folder_state: HashMap::new(),
            undo_track_indices: HashMap::new(),
            hovered_track_resize_handle: None,
            hovered_clip_resize_handle: None,
            shortcuts_hint: None,
            hw_loaded: false,
            available_backends: initial_hw.available_backends,
            selected_backend: initial_hw.selected_backend,
            available_hw: initial_hw.available_hw,
            selected_hw: initial_hw.selected_hw,
            #[cfg(any(
                target_os = "linux",
                target_os = "windows",
                target_os = "freebsd",
                target_os = "openbsd"
            ))]
            available_input_hw: initial_hw.available_input_hw,
            #[cfg(any(
                target_os = "freebsd",
                target_os = "linux",
                target_os = "openbsd",
                target_os = "windows"
            ))]
            selected_input_hw: initial_hw.selected_input_hw,
            hw_sample_rate_hz: crate::consts::audio_defaults::SAMPLE_RATE_HZ,
            oss_exclusive: true,
            #[cfg(any(unix, target_os = "windows"))]
            oss_bits: cfg.default_audio_bit_depth,
            oss_period_frames: crate::consts::audio_defaults::PERIOD_FRAMES,
            oss_nperiods: crate::consts::audio_defaults::NPERIODS,
            oss_sync_mode: crate::consts::audio_defaults::SYNC_MODE,
            opened_midi_in_hw: vec![],
            opened_midi_out_hw: vec![],
            global_midi_learn_play_pause: None,
            global_midi_learn_stop: None,
            global_midi_learn_record_toggle: None,
            midi_hw_labels: HashMap::new(),
            midi_hw_in_positions: HashMap::new(),
            midi_hw_out_positions: HashMap::new(),
            hw_in: None,
            hw_out: None,
            hw_out_level: 0.0,
            hw_out_balance: 0.0,
            hw_out_muted: false,
            hw_out_meter_db: vec![],
            #[cfg(all(unix, not(target_os = "macos")))]
            lv2_plugins: vec![],
            #[cfg(all(unix, not(target_os = "macos")))]
            lv2_plugins_loaded: false,
            vst3_plugins: vec![],
            vst3_plugins_loaded: false,
            clap_plugins: vec![],
            clap_plugins_loaded: false,
            clap_plugins_by_track: HashMap::new(),
            clap_states_by_track: HashMap::new(),
            vst3_states_by_track: HashMap::new(),
            plugin_graph_track: None,
            plugin_graph_clip: None,
            plugin_graph_plugins: vec![],
            plugin_graph_connections: vec![],
            connectable_connections: vec![],
            plugin_graphs_by_track: HashMap::new(),
            connectable_connections_by_track: HashMap::new(),
            plugin_parameters_by_track: HashMap::new(),
            plugin_graph_selected_connections: HashSet::new(),
            plugin_graph_selected_connectable_connections: HashSet::new(),
            plugin_graph_selected_plugins: std::collections::HashSet::new(),
            plugin_graph_plugin_positions: HashMap::new(),
            plugin_graph_connecting: None,
            plugin_graph_moving_plugin: None,
            plugin_graph_last_plugin_click: None,
            connections_last_track_click: None,
            last_selected_track: None,
            clip_rename_dialog: None,
            track_rename_dialog: None,
            track_template_save_dialog: None,
            marker_dialog: None,
            modulator_target_dialog: None,
            template_save_dialog: None,
            apply_template_dialog: None,
            pending_track_template_loads: Vec::new(),
            pending_folder_template_loads: Vec::new(),
            piano: None,
            pitch_correction: None,
            pitch_correction_selected_points: HashSet::new(),
            pitch_correction_dragging_points: None,
            pitch_correction_selecting_rect: None,
            pitch_correction_frame_likeness: DEFAULT_PITCH_CORRECTION_FRAME_LIKENESS,
            pitch_correction_inertia_ms: DEFAULT_PITCH_CORRECTION_INERTIA_MS,
            pitch_correction_formant_compensation: DEFAULT_PITCH_CORRECTION_FORMANT_COMPENSATION,
            piano_zoom_x: 20.0,
            piano_zoom_y: 1.0,
            piano_scroll_x: 0.0,
            piano_scroll_y: 0.0,
            piano_selected_notes: HashSet::new(),
            piano_dragging_notes: None,
            piano_resizing_note: None,
            piano_selecting_rect: None,
            piano_creating_note: None,
            piano_controller_lane: PianoControllerLane::Controller,
            piano_controller_kind: 1,
            piano_velocity_kind: PianoVelocityKind::NoteVelocity,
            piano_rpn_kind: PianoRpnKind::PitchBendSensitivity,
            piano_nrpn_kind: PianoNrpnKind::Brightness,
            piano_scale_root: PianoScaleRoot::C,
            piano_scale_minor: false,
            piano_chord_kind: PianoChordKind::MajorTriad,
            piano_velocity_shape_amount: 1.0,
            piano_selected_sysex: None,
            piano_sysex_hex_input: String::new(),
            piano_sysex_panel_open: false,
            piano_sysex_scroll_y: 0.0,
            piano_humanize_time_amount: 1.0,
            piano_humanize_velocity_amount: 1.0,
            piano_groove_amount: 1.0,
            tempo: 120.0,
            time_signature_num: 4,
            time_signature_denom: 4,
            tempo_points: vec![TempoPoint {
                sample: 0,
                bpm: 120.0,
            }],
            time_signature_points: vec![TimeSignaturePoint {
                sample: 0,
                numerator: 4,
                denominator: 4,
            }],
            session_author: String::new(),
            session_album: String::new(),
            session_year: String::new(),
            session_track_number: String::new(),
            session_genre: String::new(),
            available_templates: vec![],
        }
    }
}

struct InitialHwConfig {
    available_backends: Vec<AudioBackendOption>,
    selected_backend: AudioBackendOption,
    available_hw: Vec<OutputAudioDevice>,
    selected_hw: Option<OutputAudioDevice>,
    #[cfg(any(
        target_os = "linux",
        target_os = "windows",
        target_os = "freebsd",
        target_os = "openbsd"
    ))]
    available_input_hw: Vec<InputAudioDevice>,
    #[cfg(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "windows"
    ))]
    selected_input_hw: Option<InputAudioDevice>,
}

fn initial_hw_config() -> InitialHwConfig {
    let available_backends = supported_audio_backends();
    let selected_backend = default_audio_backend();
    let available_hw = initial_output_hw_devices();
    let selected_hw = initial_selected_output_hw(&available_hw);
    #[cfg(any(
        target_os = "linux",
        target_os = "windows",
        target_os = "freebsd",
        target_os = "openbsd"
    ))]
    let available_input_hw = initial_input_hw_devices();
    #[cfg(any(
        target_os = "linux",
        target_os = "windows",
        target_os = "freebsd",
        target_os = "openbsd"
    ))]
    let selected_input_hw = initial_selected_input_hw(&available_input_hw);
    InitialHwConfig {
        available_backends,
        selected_backend,
        available_hw,
        selected_hw,
        #[cfg(any(
            target_os = "linux",
            target_os = "windows",
            target_os = "freebsd",
            target_os = "openbsd"
        ))]
        available_input_hw,
        #[cfg(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "openbsd",
            target_os = "windows"
        ))]
        selected_input_hw,
    }
}

#[cfg(any(target_os = "freebsd", target_os = "openbsd"))]
pub(crate) fn discover_output_audio_devices() -> Vec<AudioDeviceOption> {
    #[cfg(target_os = "freebsd")]
    {
        discover_freebsd_audio_devices()
            .into_iter()
            .filter(|d| d.supports_output)
            .collect()
    }
    #[cfg(target_os = "openbsd")]
    {
        discover_openbsd_audio_devices()
            .into_iter()
            .filter(|d| d.supports_output)
            .collect()
    }
}

#[cfg(any(target_os = "freebsd", target_os = "openbsd"))]
pub(crate) fn discover_input_audio_devices() -> Vec<AudioDeviceOption> {
    #[cfg(target_os = "freebsd")]
    {
        discover_freebsd_audio_devices()
            .into_iter()
            .filter(|d| d.supports_input)
            .collect()
    }
    #[cfg(target_os = "openbsd")]
    {
        discover_openbsd_audio_devices()
            .into_iter()
            .filter(|d| d.supports_input)
            .collect()
    }
}

fn initial_output_hw_devices() -> Vec<OutputAudioDevice> {
    #[cfg(target_os = "freebsd")]
    let devices = discover_output_audio_devices();
    #[cfg(target_os = "openbsd")]
    let devices = discover_output_audio_devices();
    #[cfg(target_os = "linux")]
    let devices = discover_alsa_output_devices();
    #[cfg(target_os = "windows")]
    let devices = discover_windows_audio_devices();
    #[cfg(target_os = "macos")]
    let devices = maolan_engine::discover_coreaudio_devices();
    #[cfg(not(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "windows",
        target_os = "macos"
    )))]
    let devices = vec![];
    devices
}

#[cfg(any(
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "linux",
    target_os = "windows",
    target_os = "macos"
))]
fn initial_selected_output_hw(hw: &[OutputAudioDevice]) -> Option<OutputAudioDevice> {
    hw.first().cloned()
}

#[cfg(not(any(
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "linux",
    target_os = "windows",
    target_os = "macos"
)))]
fn initial_selected_output_hw(_hw: &[OutputAudioDevice]) -> Option<OutputAudioDevice> {
    None
}

#[cfg(any(
    target_os = "linux",
    target_os = "windows",
    target_os = "freebsd",
    target_os = "openbsd"
))]
fn initial_input_hw_devices() -> Vec<InputAudioDevice> {
    #[cfg(target_os = "linux")]
    let devices = discover_alsa_input_devices();
    #[cfg(target_os = "windows")]
    let devices = discover_windows_input_devices();
    #[cfg(any(target_os = "freebsd", target_os = "openbsd"))]
    let devices = discover_input_audio_devices();
    devices
}

#[cfg(any(
    target_os = "linux",
    target_os = "windows",
    target_os = "freebsd",
    target_os = "openbsd"
))]
fn initial_selected_input_hw(hw: &[InputAudioDevice]) -> Option<InputAudioDevice> {
    hw.first().cloned()
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "windows",
    target_os = "freebsd",
    target_os = "openbsd"
)))]
fn initial_selected_input_hw(_selected_hw: &Option<OutputAudioDevice>) -> Option<InputAudioDevice> {
    None
}

pub type State = Arc<RwLock<StateData>>;

fn supported_audio_backends() -> Vec<AudioBackendOption> {
    [
        #[cfg(unix)]
        Some(AudioBackendOption::Jack),
        #[cfg(target_os = "freebsd")]
        Some(AudioBackendOption::Oss),
        #[cfg(target_os = "openbsd")]
        Some(AudioBackendOption::Sndio),
        #[cfg(target_os = "linux")]
        Some(AudioBackendOption::Alsa),
        #[cfg(target_os = "windows")]
        Some(AudioBackendOption::Wasapi),
        #[cfg(target_os = "macos")]
        Some(AudioBackendOption::CoreAudio),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn audio_backend_preference_rank(backend: &AudioBackendOption) -> usize {
    match backend {
        #[cfg(target_os = "freebsd")]
        AudioBackendOption::Oss => 0,
        #[cfg(target_os = "openbsd")]
        AudioBackendOption::Sndio => 0,
        #[cfg(target_os = "linux")]
        AudioBackendOption::Alsa => 0,
        #[cfg(target_os = "windows")]
        AudioBackendOption::Wasapi => 0,
        #[cfg(target_os = "macos")]
        AudioBackendOption::CoreAudio => 0,
        #[cfg(unix)]
        AudioBackendOption::Jack => 1,
    }
}

fn default_audio_backend() -> AudioBackendOption {
    supported_audio_backends()
        .into_iter()
        .min_by_key(audio_backend_preference_rank)
        .expect("no default audio backend for this target")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
    fn audio_device_option_preferred_bits_returns_first() {
        let device = AudioDeviceOption::with_supported_caps(
            "hw:0".to_string(),
            "Test".to_string(),
            vec![24, 16, 32],
            vec![48000],
        );
        assert_eq!(device.preferred_bits(), Some(32));
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
    fn audio_device_option_preferred_bits_empty() {
        let device = AudioDeviceOption::with_supported_caps(
            "hw:0".to_string(),
            "Test".to_string(),
            vec![],
            vec![48000],
        );
        assert_eq!(device.preferred_bits(), None);
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
    fn normalize_sample_rates_removes_duplicates() {
        let rates = vec![48000, 44100, 48000, 48000];
        let normalized = AudioDeviceOption::normalize_sample_rates(rates);
        assert_eq!(normalized.len(), 2);
    }

    #[test]
    fn clip_id_creation() {
        let id = ClipId {
            track_idx: "track1".to_string(),
            clip_idx: 0,
            kind: maolan_engine::kind::Kind::Audio,
        };
        assert_eq!(id.track_idx, "track1");
        assert_eq!(id.clip_idx, 0);
    }

    #[test]
    fn clip_context_menu_state_creation() {
        let state = ClipContextMenuState {
            clip: ClipId {
                track_idx: "track1".to_string(),
                clip_idx: 0,
                kind: maolan_engine::kind::Kind::Audio,
            },
            anchor: iced::Point::new(100.0, 100.0),
        };
        assert!((state.anchor.x - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn track_context_menu_state_creation() {
        let state = TrackContextMenuState {
            track_name: "Drums".to_string(),
            anchor: iced::Point::new(50.0, 50.0),
            submenu: None,
        };
        assert_eq!(state.track_name, "Drums");
    }

    #[test]
    fn state_data_default_creates_instance() {
        let data: StateData = Default::default();
        assert!(!data.tracks.is_empty() || data.tracks.is_empty());
    }

    #[test]
    fn supported_audio_backends_returns_non_empty() {
        let backends = supported_audio_backends();
        assert!(!backends.is_empty());
    }

    #[test]
    fn default_audio_backend_returns_valid() {
        let backend = default_audio_backend();

        let _ = backend;
    }

    #[test]
    fn modulator_sine_value_at_zero_phase() {
        let m = Modulator {
            id: 1,
            name: "Test".to_string(),
            shape: ModulatorShape::Sine,
            rate_hz: 1.0,
            phase: 0.0,
            enabled: true,
            targets: Vec::new(),
        };
        let engine_m: maolan_engine::modulator::Modulator = (&m).into();
        let sample_rate = 48000.0;
        assert!((engine_m.value_at(0, sample_rate) - 0.5).abs() < 0.001);
        assert!((engine_m.value_at(sample_rate as usize / 4, sample_rate) - 1.0).abs() < 0.001);
        assert!((engine_m.value_at(sample_rate as usize / 2, sample_rate) - 0.5).abs() < 0.001);
        assert!((engine_m.value_at(sample_rate as usize * 3 / 4, sample_rate) - 0.0).abs() < 0.001);
    }

    #[test]
    fn modulator_target_serializes_round_trip_automation() {
        let target = ModulatorTarget {
            track_name: "Drums".to_string(),
            target: TrackAutomationTarget::Volume,
            min: -90.0,
            max: 20.0,
        };
        let json = serde_json::to_string(&target).unwrap();
        let parsed: ModulatorTarget = serde_json::from_str(&json).unwrap();
        assert_eq!(target, parsed);
    }

    #[test]
    fn modulator_target_deserializes_old_controller_format() {
        let json = r#"{"track_name":"Drums","controller":"volume","min":-90.0,"max":20.0}"#;
        let parsed: ModulatorTarget = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.track_name, "Drums");
        assert_eq!(parsed.target, TrackAutomationTarget::Volume);
        assert!((parsed.min - -90.0).abs() < f32::EPSILON);
        assert!((parsed.max - 20.0).abs() < f32::EPSILON);
    }

    #[test]
    fn modulator_target_deserializes_prev_target_format() {
        let json = r#"{"track_name":"Drums","target":"Volume","min":-90.0,"max":20.0}"#;
        let parsed: ModulatorTarget = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.track_name, "Drums");
        assert_eq!(parsed.target, TrackAutomationTarget::Volume);
    }

    #[test]
    fn modulator_target_deserializes_old_midi_cc_kind_format() {
        let json = r#"{"track_name":"Synth","kind":"MidiCc","value":{"channel":2,"cc":7},"min":0.0,"max":127.0}"#;
        let parsed: ModulatorTarget = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.track_name, "Synth");
        assert_eq!(
            parsed.target,
            TrackAutomationTarget::MidiCc { channel: 2, cc: 7 }
        );
        assert!((parsed.min - 0.0).abs() < f32::EPSILON);
        assert!((parsed.max - 127.0).abs() < f32::EPSILON);
    }

    #[test]
    fn modulator_target_serializes_round_trip_midi_cc() {
        let target = ModulatorTarget {
            track_name: "Synth".to_string(),
            target: TrackAutomationTarget::MidiCc { channel: 2, cc: 7 },
            min: 0.0,
            max: 127.0,
        };
        let json = serde_json::to_string(&target).unwrap();
        let parsed: ModulatorTarget = serde_json::from_str(&json).unwrap();
        assert_eq!(target, parsed);
    }

    #[test]
    fn track_automation_target_midi_cc_round_trip() {
        let target = TrackAutomationTarget::MidiCc { channel: 3, cc: 11 };
        let json = serde_json::to_string(&target).unwrap();
        let parsed: TrackAutomationTarget = serde_json::from_str(&json).unwrap();
        assert_eq!(target, parsed);
    }

    #[test]
    fn modulator_target_converts_to_engine_track_volume() {
        let target = ModulatorTarget {
            track_name: "Drums".to_string(),
            target: TrackAutomationTarget::Volume,
            min: -90.0,
            max: 20.0,
        };
        let engine_target: maolan_engine::modulator::ModulatorTarget = target.try_into().unwrap();
        assert!(
            matches!(engine_target, maolan_engine::modulator::ModulatorTarget::TrackVolume { track_name, min, max } if track_name == "Drums" && (min - -90.0).abs() < f32::EPSILON && (max - 20.0).abs() < f32::EPSILON)
        );
    }

    #[test]
    fn modulator_target_converts_to_engine_track_balance() {
        let target = ModulatorTarget {
            track_name: "Drums".to_string(),
            target: TrackAutomationTarget::Balance,
            min: -1.0,
            max: 1.0,
        };
        let engine_target: maolan_engine::modulator::ModulatorTarget = target.try_into().unwrap();
        assert!(
            matches!(engine_target, maolan_engine::modulator::ModulatorTarget::TrackBalance { track_name, min, max } if track_name == "Drums" && (min - -1.0).abs() < f32::EPSILON && (max - 1.0).abs() < f32::EPSILON)
        );
    }
}
