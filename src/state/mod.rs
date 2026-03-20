mod clip;
mod connection;
#[cfg(target_os = "freebsd")]
mod platform_freebsd;
#[cfg(target_os = "linux")]
mod platform_linux;
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
use maolan_engine::{
    clap::{ClapPluginInfo, ClapPluginState},
    kind::Kind,
    message::{PluginGraphConnection, PluginGraphNode, PluginGraphPlugin, PluginGraphSnapshot},
    vst3::{Vst3PluginInfo, Vst3PluginState},
};
#[cfg(target_os = "freebsd")]
pub(crate) use platform_freebsd::discover_freebsd_audio_devices;
#[cfg(target_os = "linux")]
pub(crate) use platform_linux::{discover_alsa_input_devices, discover_alsa_output_devices};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Instant,
};
use tokio::sync::RwLock;
pub use track::{EditorMarker, Track, TrackAutomationLane, TrackAutomationPoint, TrackLaneLayout};

pub use crate::consts::state_ids::{HW_IN_ID, HW_OUT_ID, MIDI_HW_IN_ID, MIDI_HW_OUT_ID};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AudioBackendOption {
    #[cfg(unix)]
    Jack,
    #[cfg(target_os = "freebsd")]
    Oss,
    #[cfg(target_os = "linux")]
    Alsa,
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
            #[cfg(target_os = "linux")]
            Self::Alsa => "ALSA",
            #[cfg(target_os = "macos")]
            Self::CoreAudio => "CoreAudio",
        };
        f.write_str(label)
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
#[derive(Clone, Debug)]
pub struct AudioDeviceOption {
    pub id: String,
    pub label: String,
    pub supported_bits: Vec<usize>,
    pub supported_sample_rates: Vec<i32>,
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
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
        }
    }

    pub fn preferred_bits(&self) -> Option<usize> {
        self.supported_bits.first().copied()
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
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

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
impl PartialEq for AudioDeviceOption {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
impl Eq for AudioDeviceOption {}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
impl std::hash::Hash for AudioDeviceOption {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
pub type OutputAudioDevice = AudioDeviceOption;
#[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
pub type OutputAudioDevice = String;

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
pub type InputAudioDevice = AudioDeviceOption;

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

#[derive(Debug, Clone)]
pub struct TrackContextMenuState {
    pub track_name: String,
    pub anchor: Point,
}

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
    TrackMarker {
        track_name: String,
        marker_index: usize,
        initial_sample: usize,
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

#[cfg(all(unix, not(target_os = "macos")))]
#[derive(Debug, Clone)]
pub struct PluginConnecting {
    pub from_node: PluginGraphNode,
    pub from_port: usize,
    pub kind: Kind,
    pub point: Point,
    pub is_input: bool,
}

#[cfg(all(unix, not(target_os = "macos")))]
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
pub struct TrackGroupDialog {
    pub selected_tracks: Vec<String>,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct TemplateSaveDialog {
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct TrackTemplateSaveDialog {
    pub track_name: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct PluginGraphClipTarget {
    pub track_name: String,
    pub clip_idx: usize,
}

#[derive(Debug, Clone)]
pub struct TrackMarkerDialog {
    pub track_name: String,
    pub sample: usize,
    pub marker_index: Option<usize>,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct PianoNote {
    pub start_sample: usize,
    pub length_samples: usize,
    pub pitch: u8,
    pub velocity: u8,
    pub channel: u8,
}

#[derive(Debug, Clone)]
pub struct PianoControllerPoint {
    pub sample: usize,
    pub controller: u8,
    pub value: u8,
    pub channel: u8,
}

#[derive(Debug, Clone)]
pub struct PianoSysExPoint {
    pub sample: usize,
    pub data: Vec<u8>,
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

#[allow(dead_code)]
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
    pub connections: Vec<Connection>,
    pub selected: HashSet<String>,
    pub selected_clips: HashSet<ClipId>,
    pub clip_context_menu: Option<ClipContextMenuState>,
    pub track_context_menu: Option<TrackContextMenuState>,
    pub track_context_hover: Option<(String, Point)>,
    pub clip_click_consumed: bool,
    pub message: String,
    pub diagnostics_report: Option<String>,
    pub resizing: Option<Resizing>,
    pub connecting: Option<Connecting>,
    pub moving_track: Option<MovingTrack>,
    pub hovering: Option<Hovering>,
    pub connection_view_selection: ConnectionViewSelection,
    pub cursor: Point,
    pub editor_cursor: Option<Point>,
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
    pub metronome_enabled: bool,
    pub pending_track_positions: HashMap<String, Point>,
    pub pending_track_heights: HashMap<String, f32>,
    pub hovered_track_resize_handle: Option<String>,
    pub hovered_clip_resize_handle: Option<(String, usize, Kind, bool)>,
    pub hw_loaded: bool,
    pub available_backends: Vec<AudioBackendOption>,
    pub selected_backend: AudioBackendOption,
    pub available_hw: Vec<OutputAudioDevice>,
    pub selected_hw: Option<OutputAudioDevice>,
    #[cfg(target_os = "linux")]
    pub available_input_hw: Vec<InputAudioDevice>,
    #[cfg(any(target_os = "freebsd", target_os = "linux"))]
    pub selected_input_hw: Option<InputAudioDevice>,
    pub hw_sample_rate_hz: i32,
    pub oss_exclusive: bool,
    #[cfg(unix)]
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
    #[cfg(all(unix, not(target_os = "macos")))]
    pub plugin_graph_plugins: Vec<PluginGraphPlugin>,
    #[cfg(all(unix, not(target_os = "macos")))]
    pub plugin_graph_connections: Vec<PluginGraphConnection>,
    #[cfg(all(unix, not(target_os = "macos")))]
    pub plugin_graphs_by_track: HashMap<String, PluginGraphSnapshot>,
    pub plugin_graph_selected_connections: std::collections::HashSet<usize>,
    pub plugin_graph_selected_plugin: Option<usize>,
    pub plugin_graph_plugin_positions: HashMap<usize, Point>,
    #[cfg(all(unix, not(target_os = "macos")))]
    pub plugin_graph_connecting: Option<PluginConnecting>,
    #[cfg(all(unix, not(target_os = "macos")))]
    pub plugin_graph_moving_plugin: Option<MovingPlugin>,
    pub plugin_graph_last_plugin_click: Option<(usize, Instant)>,
    pub connections_last_track_click: Option<(String, Instant)>,
    pub clip_rename_dialog: Option<ClipRenameDialog>,
    pub track_rename_dialog: Option<TrackRenameDialog>,
    pub track_group_dialog: Option<TrackGroupDialog>,
    pub track_template_save_dialog: Option<TrackTemplateSaveDialog>,
    pub track_marker_dialog: Option<TrackMarkerDialog>,
    pub template_save_dialog: Option<TemplateSaveDialog>,
    pub pending_track_template_loads: Vec<(String, String)>, // [(track_name, template_name)]
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
        Self {
            shift: false,
            ctrl: false,
            tracks: vec![],
            connections: vec![],
            selected: HashSet::new(),
            selected_clips: HashSet::new(),
            clip_context_menu: None,
            track_context_menu: None,
            track_context_hover: None,
            clip_click_consumed: false,
            message: "Thank you for using Maolan!".to_string(),
            diagnostics_report: None,
            resizing: None,
            connecting: None,
            moving_track: None,
            hovering: None,
            connection_view_selection: ConnectionViewSelection::None,
            cursor: Point::new(0.0, 0.0),
            editor_cursor: None,
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
            metronome_enabled: false,
            pending_track_positions: HashMap::new(),
            pending_track_heights: HashMap::new(),
            hovered_track_resize_handle: None,
            hovered_clip_resize_handle: None,
            hw_loaded: false,
            available_backends: initial_hw.available_backends,
            selected_backend: initial_hw.selected_backend,
            available_hw: initial_hw.available_hw,
            selected_hw: initial_hw.selected_hw,
            #[cfg(target_os = "linux")]
            available_input_hw: initial_hw.available_input_hw,
            #[cfg(any(target_os = "freebsd", target_os = "linux"))]
            selected_input_hw: initial_hw.selected_input_hw,
            hw_sample_rate_hz: 48_000,
            oss_exclusive: true,
            #[cfg(unix)]
            oss_bits: cfg.default_audio_bit_depth,
            oss_period_frames: 1024,
            oss_nperiods: 1,
            oss_sync_mode: false,
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
            #[cfg(all(unix, not(target_os = "macos")))]
            plugin_graph_plugins: vec![],
            #[cfg(all(unix, not(target_os = "macos")))]
            plugin_graph_connections: vec![],
            #[cfg(all(unix, not(target_os = "macos")))]
            plugin_graphs_by_track: HashMap::new(),
            plugin_graph_selected_connections: HashSet::new(),
            plugin_graph_selected_plugin: None,
            plugin_graph_plugin_positions: HashMap::new(),
            #[cfg(all(unix, not(target_os = "macos")))]
            plugin_graph_connecting: None,
            #[cfg(all(unix, not(target_os = "macos")))]
            plugin_graph_moving_plugin: None,
            plugin_graph_last_plugin_click: None,
            connections_last_track_click: None,
            clip_rename_dialog: None,
            track_rename_dialog: None,
            track_group_dialog: None,
            track_template_save_dialog: None,
            track_marker_dialog: None,
            template_save_dialog: None,
            pending_track_template_loads: Vec::new(),
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
    #[cfg(target_os = "linux")]
    available_input_hw: Vec<InputAudioDevice>,
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    selected_input_hw: Option<InputAudioDevice>,
}

fn initial_hw_config() -> InitialHwConfig {
    let available_backends = supported_audio_backends();
    let selected_backend = default_audio_backend();
    let available_hw = initial_output_hw_devices();
    let selected_hw = initial_selected_output_hw(&available_hw);
    #[cfg(target_os = "linux")]
    let available_input_hw = initial_input_hw_devices();
    #[cfg(target_os = "linux")]
    let selected_input_hw = initial_selected_input_hw();
    #[cfg(target_os = "freebsd")]
    let selected_input_hw = initial_selected_input_hw(&selected_hw);
    InitialHwConfig {
        available_backends,
        selected_backend,
        available_hw,
        selected_hw,
        #[cfg(target_os = "linux")]
        available_input_hw,
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        selected_input_hw,
    }
}

fn initial_output_hw_devices() -> Vec<OutputAudioDevice> {
    #[cfg(target_os = "freebsd")]
    let devices = discover_freebsd_audio_devices();
    #[cfg(target_os = "linux")]
    let devices = discover_alsa_output_devices();
    #[cfg(target_os = "macos")]
    let devices = maolan_engine::discover_coreaudio_devices();
    #[cfg(not(any(target_os = "linux", target_os = "freebsd", target_os = "macos")))]
    let devices = vec![];
    devices
}

#[cfg(target_os = "freebsd")]
fn initial_selected_output_hw(hw: &[OutputAudioDevice]) -> Option<OutputAudioDevice> {
    hw.first().cloned()
}

#[cfg(not(target_os = "freebsd"))]
fn initial_selected_output_hw(_hw: &[OutputAudioDevice]) -> Option<OutputAudioDevice> {
    None
}

#[cfg(target_os = "linux")]
fn initial_input_hw_devices() -> Vec<InputAudioDevice> {
    #[cfg(target_os = "linux")]
    let devices = discover_alsa_input_devices();
    devices
}

#[cfg(target_os = "linux")]
fn initial_selected_input_hw() -> Option<InputAudioDevice> {
    None
}

#[cfg(target_os = "freebsd")]
fn initial_selected_input_hw(selected_hw: &Option<OutputAudioDevice>) -> Option<InputAudioDevice> {
    selected_hw.clone()
}

pub type State = Arc<RwLock<StateData>>;

fn supported_audio_backends() -> Vec<AudioBackendOption> {
    [
        #[cfg(unix)]
        Some(AudioBackendOption::Jack),
        #[cfg(target_os = "freebsd")]
        Some(AudioBackendOption::Oss),
        #[cfg(target_os = "linux")]
        Some(AudioBackendOption::Alsa),
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
        #[cfg(target_os = "linux")]
        AudioBackendOption::Alsa => 0,
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
