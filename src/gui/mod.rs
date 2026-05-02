mod platform;
mod session;
mod subscriptions;
mod update;
mod view;

#[cfg(all(unix, not(target_os = "macos")))]
use crate::plugins::lv2::GuiLv2UiHost;
use crate::{
    add_track, clip_rename, config, connections,
    consts::audio_defaults,
    consts::gui as gui_consts,
    consts::gui_mod::{
        AUDIO_BIT_DEPTH_OPTIONS, BINS_PER_UPDATE, CHUNK_FRAMES, CLIENT, MAX_PEAK_BINS,
        MAX_RECENT_SESSIONS, STANDARD_EXPORT_SAMPLE_RATES,
    },
    consts::message_lists::{
        EXPORT_BIT_DEPTH_ALL, EXPORT_MP3_MODE_ALL, EXPORT_NORMALIZE_MODE_ALL,
        EXPORT_RENDER_MODE_ALL, SNAP_MODE_ALL,
    },
    consts::state_ids::METRONOME_TRACK_ID,
    consts::widget_piano::PITCH_MAX,
    hw, menu,
    message::{
        BurnBackendOption, DraggedClip, ExportBitDepth, ExportFormat, ExportMp3Mode,
        ExportNormalizeMode, ExportRenderMode, GenerateAudioModelOption, Message, PluginFormat,
        PreferencesDeviceOption, Show, SnapMode,
    },
    platform_caps,
    plugins::{clap::GuiClapUiHost, vst3::GuiVst3UiHost},
    state::{
        AudioClip, ClipPeaks, LOG_HISTORY_LIMIT, LogEntry, LogLevel, MIDIClip, MidiClipPreviewMap,
        PianoControllerPoint, PianoNote, PianoSysExPoint, PitchCorrectionData,
        PitchCorrectionPoint, State, StateData,
    },
    template_save, toolbar, track_group, track_group_template_save, track_marker, track_rename,
    track_template_save, workspace,
};
use ebur128::{EbuR128, Mode as LoudnessMode};
use ffmpeg_next::{
    Dictionary,
    codec::{Context as CodecContext, Id as CodecId},
    format::output,
    frame::Audio,
};
use flacenc::component::BitRepr;
use flacenc::error::Verify;
use iced::{
    Length, Size, Task,
    widget::{
        button, checkbox, column, container, pick_list, progress_bar, row, scrollable, text,
        text_editor, text_input,
    },
};
use maolan_engine::kind::Kind;
use maolan_engine::message::{Action, Message as EngineMessage};
use maolan_widgets::numeric_input::{number_input, number_input_f32};
use midly::{
    Format, Header, MetaMessage, Smf, Timing, TrackEvent, TrackEventKind,
    num::{u15, u24, u28},
};
use pitch_detection::detector::{PitchDetector, mcleod::McLeodDetector};
use serde::Serialize;
use serde_json::Value;
use serde_json::json;
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs::{self, File},
    hash::{DefaultHasher, Hash, Hasher},
    io::{self, BufReader},
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, LazyLock, Mutex},
    time::{Duration, Instant},
};
use symphonia::core::{
    audio::SampleBuffer, codecs::DecoderOptions, errors::Error as SymphoniaError,
    formats::FormatOptions, io::MediaSourceStream, meta::MetadataOptions, probe::Hint,
};
use tokio::sync::RwLock;
use tracing::error;

use wavers::Wav;

pub(crate) use gui_consts::{MIN_CLIP_WIDTH_PX, PREF_DEVICE_AUTO_ID};
type TickToSampleFn = dyn Fn(u64) -> usize + Send + Sync;
type MidiTickMap = (Box<TickToSampleFn>, u64, u64);

const MAOLAN_BURN_SOCKETPAIR_ENV: &str = "MAOLAN_BURN_SOCKETPAIR";
#[cfg(unix)]
#[derive(Debug, Clone, Serialize)]
struct BurnGenerateRequest {
    model: GenerateAudioModelOption,
    prompt: String,
    output_path: PathBuf,
    tags: Option<String>,
    backend: BurnBackendOption,
    cfg_scale: f32,
    ode_steps: usize,
    length: usize,
}

#[cfg(unix)]
struct BurnGenerateProcessHandle {
    socket: std::os::unix::net::UnixStream,
    stderr_log: Arc<Mutex<Vec<String>>>,
    exit_status: Arc<Mutex<Option<std::process::ExitStatus>>>,
}

type PianoParseResult = (
    Vec<PianoNote>,
    Vec<PianoControllerPoint>,
    Vec<PianoSysExPoint>,
    usize,
);
type TrackFreezeRestore = (Vec<AudioClip>, Vec<MIDIClip>, Option<String>);

pub(crate) const MIN_ZOOM_VISIBLE_BARS: f32 = 0.25;
pub(crate) const MAX_ZOOM_VISIBLE_BARS: f32 = 256.0;
pub(crate) static RUBBERBAND_AVAILABLE: LazyLock<bool> = LazyLock::new(|| {
    Command::new("rubberband")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
});

pub(crate) fn zoom_slider_to_visible_bars(position: f32) -> f32 {
    let clamped = position.clamp(0.0, 1.0);
    let min = MIN_ZOOM_VISIBLE_BARS.log2();
    let max = MAX_ZOOM_VISIBLE_BARS.log2();
    2.0_f32.powf(min + (max - min) * clamped)
}

pub(crate) fn visible_bars_to_zoom_slider(visible_bars: f32) -> f32 {
    let clamped = visible_bars.clamp(MIN_ZOOM_VISIBLE_BARS, MAX_ZOOM_VISIBLE_BARS);
    let min = MIN_ZOOM_VISIBLE_BARS.log2();
    let max = MAX_ZOOM_VISIBLE_BARS.log2();
    ((clamped.log2() - min) / (max - min)).clamp(0.0, 1.0)
}

#[derive(Debug, Clone)]
struct PendingTrackFreezeBounce {
    rendered_clip_rel: String,
    rendered_length: usize,
    backup_audio: Vec<AudioClip>,
    backup_midi: Vec<MIDIClip>,
}

#[derive(Debug, Clone)]
struct PendingAutosaveRecovery {
    session_dir: PathBuf,
    snapshots: Vec<PathBuf>,
    selected_index: usize,
    confirm_armed: bool,
}

#[derive(Debug, Clone)]
struct PitchCorrectionHistoryEntry {
    points: Vec<PitchCorrectionPoint>,
    selected_points: HashSet<usize>,
}

struct ExportSessionOptions {
    export_path: PathBuf,
    sample_rate: i32,
    formats: Vec<ExportFormat>,
    render_mode: ExportRenderMode,
    selected_hw_out_ports: Vec<usize>,
    realtime_fallback: bool,
    bit_depth: ExportBitDepth,
    mp3_mode: ExportMp3Mode,
    mp3_bitrate_kbps: u16,
    ogg_quality: f32,
    normalize: bool,
    normalize_target_dbfs: f32,
    normalize_mode: ExportNormalizeMode,
    normalize_target_lufs: f32,
    normalize_true_peak_dbtp: f32,
    normalize_tp_limiter: bool,
    master_limiter: bool,
    master_limiter_ceiling_dbtp: f32,
    metadata_author: String,
    metadata_album: String,
    metadata_year: Option<u32>,
    metadata_track_number: Option<u32>,
    metadata_genre: String,
    state: State,
    session_root: PathBuf,
}

#[derive(Clone)]
struct ExportMetadata {
    author: String,
    album: String,
    year: Option<u32>,
    track_number: Option<u32>,
    genre: String,
}

#[derive(Clone, Copy)]
struct ExportCodecSettings {
    mp3_mode: ExportMp3Mode,
    mp3_bitrate_kbps: u16,
    ogg_quality: f32,
}

struct ExportWriteRequest<'a> {
    export_path: &'a Path,
    mixed_buffer: &'a [f32],
    sample_rate: i32,
    output_channels: usize,
    bit_depth: ExportBitDepth,
    format: ExportFormat,
    codec: ExportCodecSettings,
    metadata: &'a ExportMetadata,
}

#[derive(Clone)]
struct ExportTrackData {
    name: String,
    level: f32,
    balance: f32,
    muted: bool,
    soloed: bool,
    output_ports: usize,
    clips: Vec<crate::state::AudioClip>,
}

#[derive(Debug, Clone)]
struct AppPreferences {
    osc_enabled: bool,
    default_export_sample_rate_hz: u32,
    default_snap_mode: SnapMode,
    default_midi_snap_mode: SnapMode,
    default_audio_bit_depth: usize,
    default_output_device_id: Option<String>,
    default_input_device_id: Option<String>,
    recent_session_paths: Vec<String>,
}

impl Default for AppPreferences {
    fn default() -> Self {
        Self {
            osc_enabled: false,
            default_export_sample_rate_hz: audio_defaults::SAMPLE_RATE_HZ as u32,
            default_snap_mode: SnapMode::Bar,
            default_midi_snap_mode: SnapMode::Bar,
            default_audio_bit_depth: audio_defaults::BIT_DEPTH,
            default_output_device_id: None,
            default_input_device_id: None,
            recent_session_paths: Vec::new(),
        }
    }
}

#[derive(Clone, Copy)]
struct ExportNormalizeParams {
    mode: ExportNormalizeMode,
    target_dbfs: f32,
    target_lufs: f32,
    true_peak_dbtp: f32,
    tp_limiter: bool,
    sample_rate: i32,
    output_channels: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct AudioClipKey {
    track_name: String,
    clip_name: String,
    start: usize,
    length: usize,
    offset: usize,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct SessionMediaCleanupReport {
    deleted_files: Vec<String>,
    failed_files: Vec<String>,
}

#[derive(Debug, Clone)]
pub(super) struct AudioPeakChunkUpdate {
    pub track_name: String,
    pub clip_name: String,
    pub start: usize,
    pub length: usize,
    pub offset: usize,
    pub channels: usize,
    pub target_bins: usize,
    pub bin_start: usize,
    pub peaks: Vec<Vec<[f32; 2]>>,
    pub done: bool,
}

pub(super) static AUDIO_PEAK_UPDATES: LazyLock<Mutex<Vec<AudioPeakChunkUpdate>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

#[derive(Debug, Clone)]
pub(super) struct PendingClapUiOpen {
    track_name: String,
    clip_idx: Option<usize>,
    instance_id: usize,
    plugin_path: String,
}

#[derive(Debug, Clone)]
pub(super) struct PendingVst3UiOpen {
    track_name: String,
    clip_idx: Option<usize>,
    instance_id: usize,
    plugin_path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum AutomationWriteKey {
    Volume,
    Balance,
    Mute,
    Lv2 { instance_id: usize, index: u32 },
    Vst3 { instance_id: usize, param_id: u32 },
    Clap { instance_id: usize, param_id: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimingSelectionLane {
    Tempo,
    TimeSignature,
}

#[derive(Debug, Clone)]
struct TouchAutomationOverride {
    value: f32,
    updated_at: Instant,
}

#[derive(Debug, Clone, Default)]
struct TrackAutomationRuntime {
    level_db: Option<f32>,
    balance: Option<f32>,
    muted: Option<bool>,
    #[cfg(all(unix, not(target_os = "macos")))]
    lv2_params: HashMap<(usize, u32), f32>,
    vst3_params: HashMap<(usize, u32), f32>,
    clap_params: HashMap<(usize, u32), f64>,
}

pub struct Maolan {
    clip: Option<DraggedClip>,
    clip_preview_target_track: Option<String>,
    clip_preview_target_valid: bool,
    clip_preview_snap_adjust_samples: f32,
    clip_snap_targets: Vec<crate::state::ClipId>,
    menu: menu::Menu,
    size: Size,
    state: State,
    toolbar: toolbar::Toolbar,
    track: Option<String>,
    workspace: workspace::Workspace,
    connections: connections::canvas_host::CanvasHost<connections::tracks::Graph>,
    #[cfg(all(unix, not(target_os = "macos")))]
    track_plugins: connections::canvas_host::CanvasHost<connections::plugins::Graph>,
    hw: hw::HW,
    modal: Option<Show>,
    add_track: add_track::AddTrackView,
    clip_rename: clip_rename::ClipRenameView,
    track_rename: track_rename::TrackRenameView,
    track_group: track_group::TrackGroupView,
    track_marker: track_marker::TrackMarkerView,
    track_template_save: track_template_save::TrackTemplateSaveView,
    track_group_template_save: track_group_template_save::TrackGroupTemplateSaveView,
    template_save: template_save::TemplateSaveView,
    #[cfg(all(unix, not(target_os = "macos")))]
    plugin_filter: String,
    #[cfg(all(unix, not(target_os = "macos")))]
    selected_lv2_plugins: BTreeSet<String>,
    vst3_plugin_filter: String,
    selected_vst3_plugins: BTreeSet<String>,
    clap_plugin_filter: String,
    selected_clap_plugins: BTreeSet<String>,
    plugin_format: PluginFormat,
    session_dir: Option<PathBuf>,
    pending_save_path: Option<String>,
    pending_save_tracks: std::collections::HashSet<String>,
    pending_save_clap_tracks: std::collections::HashSet<String>,
    #[cfg(target_os = "macos")]
    pending_save_vst3_states: HashSet<(String, usize)>,
    pending_save_is_template: bool,
    pending_peak_file_loads: HashMap<AudioClipKey, PathBuf>,
    pending_peak_rebuilds: HashSet<AudioClipKey>,
    pending_precomputed_peaks: HashMap<AudioClipKey, crate::state::ClipPeaks>,
    pitch_correction_undo: Vec<PitchCorrectionHistoryEntry>,
    pitch_correction_redo: Vec<PitchCorrectionHistoryEntry>,
    pending_track_freeze_restore: HashMap<String, TrackFreezeRestore>,
    pending_track_midi_editor_view_mode: HashMap<String, crate::message::MidiEditorViewMode>,
    pending_track_freeze_bounce: HashMap<String, PendingTrackFreezeBounce>,
    track_automation_runtime: HashMap<String, TrackAutomationRuntime>,
    touch_automation_overrides:
        HashMap<String, HashMap<AutomationWriteKey, TouchAutomationOverride>>,
    touch_active_keys: HashMap<String, HashSet<AutomationWriteKey>>,
    latch_automation_overrides: HashMap<String, HashMap<AutomationWriteKey, f32>>,
    #[cfg(all(unix, not(target_os = "macos")))]
    pending_add_lv2_automation_uris: HashSet<(String, String)>,
    #[cfg(all(unix, not(target_os = "macos")))]
    pending_add_lv2_automation_instances: HashSet<(String, usize)>,
    pending_add_vst3_automation_paths: HashSet<(String, String)>,
    pending_add_vst3_automation_instances: HashSet<(String, usize)>,
    pending_add_clap_automation_paths: HashSet<(String, String)>,
    pending_add_clap_automation_instances: HashSet<(String, usize)>,
    midi_clip_previews: MidiClipPreviewMap,
    pending_midi_clip_previews: HashSet<(String, usize, String)>,
    freeze_in_progress: bool,
    freeze_progress: f32,
    freeze_track_name: Option<String>,
    freeze_cancel_requested: bool,
    playing: bool,
    paused: bool,
    metronome_enabled: bool,
    transport_samples: f64,
    last_playback_tick: Option<Instant>,
    playback_rate_hz: f64,
    loop_enabled: bool,
    loop_range_samples: Option<(usize, usize)>,
    punch_enabled: bool,
    punch_range_samples: Option<(usize, usize)>,
    snap_mode: SnapMode,
    midi_snap_mode: SnapMode,
    zoom_visible_bars: f32,
    editor_scroll_origin_samples: f64,
    editor_scroll_x: f32,
    editor_scroll_y: f32,
    mixer_scroll_x: f32,
    tracks_resize_hovered: bool,
    mixer_resize_hovered: bool,
    tracks_visible: bool,
    editor_visible: bool,
    mixer_visible: bool,
    show_log_window: bool,
    hw_mixer: mixosc::app::StatusApp,
    mixer_level_edit_track: Option<String>,
    mixer_level_edit_input: String,
    record_armed: bool,
    pending_record_after_save: bool,
    recording_preview_start_sample: Option<usize>,
    recording_preview_sample: Option<usize>,
    recording_preview_peaks: HashMap<String, ClipPeaks>,
    import_in_progress: bool,
    import_current_file: usize,
    import_total_files: usize,
    import_file_progress: f32,
    import_current_filename: String,
    import_current_operation: Option<String>,
    generate_audio_model: GenerateAudioModelOption,
    generate_audio_prompt_editor: text_editor::Content,
    log_viewer_content: text_editor::Content,

    generate_audio_tags_input: String,
    generate_audio_backend: BurnBackendOption,

    generate_audio_cfg_scale_input: String,
    generate_audio_steps_input: usize,
    generate_audio_seconds_total_input: usize,
    generate_audio_in_progress: bool,
    generate_audio_progress: f32,
    generate_audio_operation: Option<String>,
    generate_audio_abort_handle: Option<tokio::task::AbortHandle>,
    #[cfg(unix)]
    generate_audio_process_id: Option<u32>,
    clip_pitch_correction_in_progress: bool,
    clip_pitch_correction_progress: f32,
    clip_pitch_correction_clip_name: String,
    clip_pitch_correction_operation: Option<String>,
    export_in_progress: bool,
    export_progress: f32,
    export_operation: Option<String>,
    export_sample_rate_hz: u32,
    export_format_wav: bool,
    export_format_mp3: bool,
    export_format_ogg: bool,
    export_format_flac: bool,
    export_bit_depth: ExportBitDepth,
    export_mp3_mode: ExportMp3Mode,
    export_mp3_bitrate_kbps: u16,
    export_ogg_quality_input: String,
    export_render_mode: ExportRenderMode,
    export_hw_out_ports: BTreeSet<usize>,
    export_realtime_fallback: bool,
    export_normalize: bool,
    export_normalize_mode: ExportNormalizeMode,
    export_normalize_dbfs_input: String,
    export_normalize_lufs_input: String,
    export_normalize_dbtp_input: String,
    export_normalize_tp_limiter: bool,
    export_master_limiter: bool,
    export_master_limiter_ceiling_input: String,
    clap_ui_host: GuiClapUiHost,
    #[cfg(all(unix, not(target_os = "macos")))]
    lv2_ui_host: GuiLv2UiHost,
    vst3_ui_host: GuiVst3UiHost,
    pending_clap_ui_open: Option<PendingClapUiOpen>,
    pending_vst3_ui_open: Option<PendingVst3UiOpen>,
    tempo_input: String,
    time_signature_num_input: String,
    time_signature_denom_input: String,
    last_sent_tempo_bpm: Option<f64>,
    last_sent_time_signature: Option<(u16, u16)>,
    selected_tempo_points: BTreeSet<usize>,
    selected_time_signature_points: BTreeSet<usize>,
    timing_selection_lane: Option<TimingSelectionLane>,
    midi_mappings_panel_open: bool,
    midi_mappings_report_lines: Vec<String>,
    has_unsaved_changes: bool,
    pending_exit_after_save: bool,
    session_restore_in_progress: bool,
    last_autosave_snapshot: Option<Instant>,
    pending_recovery_session_dir: Option<PathBuf>,
    pending_autosave_recovery: Option<PendingAutosaveRecovery>,
    pending_open_session_dir: Option<PathBuf>,
    pending_diagnostics_bundle_export: bool,
    diagnostics_bundle_wait_session_report: bool,
    diagnostics_bundle_wait_midi_report: bool,
    prefs_osc_enabled: bool,
    prefs_export_sample_rate_hz: u32,
    prefs_snap_mode: SnapMode,
    prefs_midi_snap_mode: SnapMode,
    prefs_audio_bit_depth: usize,
    prefs_default_output_device_id: Option<String>,
    prefs_default_input_device_id: Option<String>,
}

fn load_preferences() -> AppPreferences {
    let cfg = config::Config::load().unwrap_or_default();
    AppPreferences {
        osc_enabled: cfg.osc_enabled,
        default_export_sample_rate_hz: cfg.default_export_sample_rate_hz,
        default_snap_mode: cfg.default_snap_mode,
        default_midi_snap_mode: cfg.default_midi_snap_mode,
        default_audio_bit_depth: cfg.default_audio_bit_depth,
        default_output_device_id: cfg.default_output_device_id,
        default_input_device_id: cfg.default_input_device_id,
        recent_session_paths: cfg.recent_session_paths,
    }
}

fn scan_templates() -> Vec<String> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let templates_dir = format!("{}/.config/maolan/session_templates", home);

    let Ok(entries) = std::fs::read_dir(&templates_dir) else {
        return vec![];
    };

    entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.is_dir() {
                path.file_name()?.to_str().map(|s| s.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn scan_track_templates() -> Vec<String> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let templates_dir = format!("{}/.config/maolan/track_templates", home);

    let Ok(entries) = std::fs::read_dir(&templates_dir) else {
        return vec![];
    };

    entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.is_dir() {
                path.file_name()?.to_str().map(|s| s.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn scan_group_templates() -> Vec<String> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let templates_dir = format!("{}/.config/maolan/group_templates", home);

    let Ok(entries) = std::fs::read_dir(&templates_dir) else {
        return vec![];
    };

    entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.is_dir() {
                path.file_name()?.to_str().map(|s| s.to_string())
            } else {
                None
            }
        })
        .collect()
}

impl Default for Maolan {
    fn default() -> Self {
        let prefs = load_preferences();
        let mut state_data = StateData {
            available_templates: scan_templates(),
            ..StateData::default()
        };
        Self::apply_preferred_devices_to_state(&mut state_data, &prefs);
        let state = Arc::new(RwLock::new(state_data));
        if tokio::runtime::Handle::try_current().is_ok() {
            let _ = CLIENT
                .sender
                .blocking_send(EngineMessage::Request(Action::SetOscEnabled(
                    prefs.osc_enabled,
                )));
        }
        let mut menu = menu::Menu::default();
        menu.update_templates(scan_templates());
        menu.update_recent_sessions(Self::normalize_recent_session_paths(
            prefs.recent_session_paths.clone(),
        ));
        Self {
            clip: None,
            clip_preview_target_track: None,
            clip_preview_target_valid: false,
            clip_preview_snap_adjust_samples: 0.0,
            clip_snap_targets: Vec::new(),
            menu,
            size: Size::new(0.0, 0.0),
            state: state.clone(),
            toolbar: toolbar::Toolbar::new(),
            track: None,
            workspace: workspace::Workspace::new(state.clone()),
            connections: connections::canvas_host::CanvasHost::new(
                connections::tracks::Graph::new(state.clone()),
            ),
            #[cfg(all(unix, not(target_os = "macos")))]
            track_plugins: connections::canvas_host::CanvasHost::new(
                connections::plugins::Graph::new(state.clone()),
            ),
            hw: hw::HW::new(state.clone()),
            modal: None,
            add_track: add_track::AddTrackView::default(),
            clip_rename: clip_rename::ClipRenameView::new(state.clone()),
            track_rename: track_rename::TrackRenameView::new(state.clone()),
            track_group: track_group::TrackGroupView::new(state.clone()),
            track_marker: track_marker::TrackMarkerView::new(state.clone()),
            track_template_save: track_template_save::TrackTemplateSaveView::new(state.clone()),
            track_group_template_save: track_group_template_save::TrackGroupTemplateSaveView::new(
                state.clone(),
            ),
            template_save: template_save::TemplateSaveView::new(state.clone()),
            #[cfg(all(unix, not(target_os = "macos")))]
            plugin_filter: String::new(),
            #[cfg(all(unix, not(target_os = "macos")))]
            selected_lv2_plugins: BTreeSet::new(),
            vst3_plugin_filter: String::new(),
            selected_vst3_plugins: BTreeSet::new(),
            clap_plugin_filter: String::new(),
            selected_clap_plugins: BTreeSet::new(),
            plugin_format: Self::default_plugin_format(),
            session_dir: None,
            pending_save_path: None,
            pending_save_tracks: std::collections::HashSet::new(),
            pending_save_clap_tracks: std::collections::HashSet::new(),
            #[cfg(target_os = "macos")]
            pending_save_vst3_states: HashSet::new(),
            pending_save_is_template: false,
            pending_peak_file_loads: HashMap::new(),
            pending_peak_rebuilds: HashSet::new(),
            pending_precomputed_peaks: HashMap::new(),
            pitch_correction_undo: Vec::new(),
            pitch_correction_redo: Vec::new(),
            pending_track_freeze_restore: HashMap::new(),
            pending_track_midi_editor_view_mode: HashMap::new(),
            pending_track_freeze_bounce: HashMap::new(),
            track_automation_runtime: HashMap::new(),
            touch_automation_overrides: HashMap::new(),
            touch_active_keys: HashMap::new(),
            latch_automation_overrides: HashMap::new(),
            #[cfg(all(unix, not(target_os = "macos")))]
            pending_add_lv2_automation_uris: HashSet::new(),
            #[cfg(all(unix, not(target_os = "macos")))]
            pending_add_lv2_automation_instances: HashSet::new(),
            pending_add_vst3_automation_paths: HashSet::new(),
            pending_add_vst3_automation_instances: HashSet::new(),
            pending_add_clap_automation_paths: HashSet::new(),
            pending_add_clap_automation_instances: HashSet::new(),
            midi_clip_previews: HashMap::new(),
            pending_midi_clip_previews: HashSet::new(),
            freeze_in_progress: false,
            freeze_progress: 0.0,
            freeze_track_name: None,
            freeze_cancel_requested: false,
            playing: false,
            paused: false,
            metronome_enabled: false,
            transport_samples: 0.0,
            last_playback_tick: None,
            playback_rate_hz: 48_000.0,
            loop_enabled: false,
            loop_range_samples: None,
            punch_enabled: false,
            punch_range_samples: None,
            snap_mode: prefs.default_snap_mode,
            midi_snap_mode: prefs.default_midi_snap_mode,
            zoom_visible_bars: 127.0,
            editor_scroll_origin_samples: 0.0,
            editor_scroll_x: 0.0,
            editor_scroll_y: 0.0,
            mixer_scroll_x: 0.0,
            tracks_resize_hovered: false,
            mixer_resize_hovered: false,
            tracks_visible: true,
            editor_visible: true,
            mixer_visible: true,
            show_log_window: false,
            hw_mixer: mixosc::app::StatusApp::default(),
            mixer_level_edit_track: None,
            mixer_level_edit_input: String::new(),
            record_armed: false,
            pending_record_after_save: false,
            recording_preview_start_sample: None,
            recording_preview_sample: None,
            recording_preview_peaks: HashMap::new(),
            import_in_progress: false,
            import_current_file: 0,
            import_total_files: 0,
            import_file_progress: 0.0,
            import_current_filename: String::new(),
            import_current_operation: None,
            generate_audio_model: GenerateAudioModelOption::HappyNewYear,
            generate_audio_prompt_editor: text_editor::Content::new(),
            log_viewer_content: text_editor::Content::with_text(
                "[INFO] Thank you for using Maolan!",
            ),

            generate_audio_tags_input: String::new(),
            generate_audio_backend: BurnBackendOption::Vulkan,

            generate_audio_cfg_scale_input: maolan_generate::DEFAULT_CFG_SCALE.to_string(),
            generate_audio_steps_input: 10,
            generate_audio_seconds_total_input: 180_usize,
            generate_audio_in_progress: false,
            generate_audio_progress: 0.0,
            generate_audio_operation: None,
            generate_audio_abort_handle: None,
            #[cfg(unix)]
            generate_audio_process_id: None,
            clip_pitch_correction_in_progress: false,
            clip_pitch_correction_progress: 0.0,
            clip_pitch_correction_clip_name: String::new(),
            clip_pitch_correction_operation: None,
            export_in_progress: false,
            export_progress: 0.0,
            export_operation: None,
            export_sample_rate_hz: prefs.default_export_sample_rate_hz,
            export_format_wav: true,
            export_format_mp3: false,
            export_format_ogg: false,
            export_format_flac: false,
            export_bit_depth: ExportBitDepth::Int24,
            export_mp3_mode: ExportMp3Mode::Cbr,
            export_mp3_bitrate_kbps: 320,
            export_ogg_quality_input: "0.6".to_string(),
            export_render_mode: ExportRenderMode::Mixdown,
            export_hw_out_ports: [0_usize, 1].into_iter().collect(),
            export_realtime_fallback: false,
            export_normalize: false,
            export_normalize_mode: ExportNormalizeMode::Peak,
            export_normalize_dbfs_input: "0.0".to_string(),
            export_normalize_lufs_input: "-23.0".to_string(),
            export_normalize_dbtp_input: "-1.0".to_string(),
            export_normalize_tp_limiter: true,
            export_master_limiter: true,
            export_master_limiter_ceiling_input: "-1.0".to_string(),
            clap_ui_host: GuiClapUiHost::new(),
            #[cfg(all(unix, not(target_os = "macos")))]
            lv2_ui_host: GuiLv2UiHost::new(),
            vst3_ui_host: GuiVst3UiHost::new(),
            pending_clap_ui_open: None,
            pending_vst3_ui_open: None,
            tempo_input: "120".to_string(),
            time_signature_num_input: "4".to_string(),
            time_signature_denom_input: "4".to_string(),
            last_sent_tempo_bpm: Some(120.0),
            last_sent_time_signature: Some((4, 4)),
            selected_tempo_points: BTreeSet::new(),
            selected_time_signature_points: BTreeSet::new(),
            timing_selection_lane: None,
            midi_mappings_panel_open: false,
            midi_mappings_report_lines: Vec::new(),
            has_unsaved_changes: false,
            pending_exit_after_save: false,
            session_restore_in_progress: false,
            last_autosave_snapshot: None,
            pending_recovery_session_dir: None,
            pending_autosave_recovery: None,
            pending_open_session_dir: None,
            pending_diagnostics_bundle_export: false,
            diagnostics_bundle_wait_session_report: false,
            diagnostics_bundle_wait_midi_report: false,
            prefs_osc_enabled: prefs.osc_enabled,
            prefs_export_sample_rate_hz: prefs.default_export_sample_rate_hz,
            prefs_snap_mode: prefs.default_snap_mode,
            prefs_midi_snap_mode: prefs.default_midi_snap_mode,
            prefs_audio_bit_depth: prefs.default_audio_bit_depth,
            prefs_default_output_device_id: prefs.default_output_device_id,
            prefs_default_input_device_id: prefs.default_input_device_id,
        }
    }
}

impl Maolan {
    fn push_log_entry(state: &mut StateData, level: LogLevel, message: String) {
        state.message = message.clone();
        state.log_entries.push(LogEntry { level, message });
        if state.log_entries.len() > LOG_HISTORY_LIMIT {
            let drop_count = state.log_entries.len() - LOG_HISTORY_LIMIT;
            state.log_entries.drain(0..drop_count);
        }
    }

    fn refresh_log_viewer_content(&mut self) {
        let log_text = self
            .state
            .blocking_read()
            .log_entries
            .iter()
            .map(|entry| format!("[{}] {}", entry.level, entry.message))
            .collect::<Vec<_>>()
            .join("\n");
        self.log_viewer_content = text_editor::Content::with_text(&log_text);
    }

    fn sync_message_log_from_state(&mut self) {
        let mut state = self.state.blocking_write();
        let needs_append = state
            .log_entries
            .last()
            .map(|entry| entry.message.as_str() != state.message.as_str())
            .unwrap_or(true);
        if needs_append {
            let message = state.message.clone();
            Self::push_log_entry(&mut state, LogLevel::Info, message);
            drop(state);
            self.refresh_log_viewer_content();
        }
    }

    fn info(&mut self, message: impl Into<String>) {
        let message = message.into();
        tracing::info!("{message}");
        let mut state = self.state.blocking_write();
        Self::push_log_entry(&mut state, LogLevel::Info, message);
        drop(state);
        self.refresh_log_viewer_content();
    }

    fn warning(&mut self, message: impl Into<String>) {
        let message = message.into();
        tracing::warn!("{message}");
        let mut state = self.state.blocking_write();
        Self::push_log_entry(&mut state, LogLevel::Warning, message);
        drop(state);
        self.refresh_log_viewer_content();
    }

    fn error(&mut self, message: impl Into<String>) {
        let message = message.into();
        tracing::error!("{message}");
        let mut state = self.state.blocking_write();
        Self::push_log_entry(&mut state, LogLevel::Error, message);
        drop(state);
        self.refresh_log_viewer_content();
    }

    #[cfg(unix)]
    fn maolan_workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    #[cfg(unix)]
    fn resolve_maolan_burn_binary_path(
        current_exe: Option<&Path>,
        _workspace_root: &Path,
    ) -> Option<PathBuf> {
        let mut candidates = Vec::new();
        if let Some(current_exe) = current_exe
            && let Some(parent) = current_exe.parent()
        {
            candidates.push(parent.join("maolan-generate"));
        }
        candidates.into_iter().find(|path| path.exists())
    }

    #[cfg(unix)]
    fn maolan_burn_command() -> Command {
        let workspace_root = Self::maolan_workspace_root();

        if let Some(path) = Self::resolve_maolan_burn_binary_path(
            std::env::current_exe().ok().as_deref(),
            &workspace_root,
        ) {
            return Command::new(path);
        }
        Command::new("maolan-generate")
    }

    #[cfg(unix)]
    fn write_ipc_message<T: Serialize>(
        writer: &mut impl std::io::Write,
        value: &T,
    ) -> Result<(), String> {
        let payload =
            serde_json::to_vec(value).map_err(|e| format!("Failed to encode IPC request: {e}"))?;
        let len = u64::try_from(payload.len()).map_err(|_| "IPC payload too large".to_string())?;
        writer
            .write_all(&len.to_le_bytes())
            .map_err(|e| format!("Failed to write IPC request length: {e}"))?;
        writer
            .write_all(&payload)
            .map_err(|e| format!("Failed to write IPC request payload: {e}"))?;
        writer
            .flush()
            .map_err(|e| format!("Failed to flush IPC request: {e}"))?;
        Ok(())
    }

    #[cfg(unix)]
    fn read_ipc_payload(reader: &mut impl std::io::Read) -> Result<Vec<u8>, String> {
        let mut len_bytes = [0_u8; 8];
        reader
            .read_exact(&mut len_bytes)
            .map_err(|e| format!("Failed to read IPC response length: {e}"))?;
        let len = usize::try_from(u64::from_le_bytes(len_bytes))
            .map_err(|_| "IPC response too large".to_string())?;
        let mut payload = vec![0_u8; len];
        reader
            .read_exact(&mut payload)
            .map_err(|e| format!("Failed to read IPC response payload: {e}"))?;
        Ok(payload)
    }

    #[cfg(unix)]
    #[cfg(unix)]
    fn spawn_generate_process(
        request: &BurnGenerateRequest,
    ) -> Result<(u32, BurnGenerateProcessHandle), String> {
        use std::io::{BufRead, BufReader};
        use std::os::fd::OwnedFd;
        use std::os::unix::net::UnixStream;
        use std::process::Stdio;

        let (mut parent, child) =
            UnixStream::pair().map_err(|e| format!("Failed to create socketpair: {e}"))?;

        let child_read = child
            .try_clone()
            .map_err(|e| format!("Failed to clone child socket: {e}"))?;
        let mut command = Self::maolan_burn_command();
        command.env(MAOLAN_BURN_SOCKETPAIR_ENV, "1");
        let mut process = command
            .stdin(Stdio::from(OwnedFd::from(child_read)))
            .stdout(Stdio::from(OwnedFd::from(child)))
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to launch generate: {e}"))?;
        let pid = process.id();

        let stderr_log = Arc::new(Mutex::new(Vec::<String>::new()));
        let exit_status = Arc::new(Mutex::new(None));

        let stderr_log_bg = stderr_log.clone();
        let exit_status_bg = exit_status.clone();
        std::thread::spawn(move || {
            if let Some(stderr) = process.stderr.take() {
                let reader = BufReader::new(stderr);
                for line in reader.lines() {
                    match line {
                        Ok(line) => {
                            error!("maolan-generate: {line}");
                            if let Ok(mut log) = stderr_log_bg.lock() {
                                log.push(line);
                                if log.len() > 64 {
                                    let drop_count = log.len() - 64;
                                    log.drain(0..drop_count);
                                }
                            }
                        }
                        Err(err) => {
                            error!("Failed to read maolan-generate stderr: {err}");
                            break;
                        }
                    }
                }
            }
            let status = process.wait().ok();
            if let Ok(mut slot) = exit_status_bg.lock() {
                *slot = status;
            }
        });

        // Send the request
        Self::write_ipc_message(&mut parent, request)?;

        Ok((
            pid,
            BurnGenerateProcessHandle {
                socket: parent,
                stderr_log,
                exit_status,
            },
        ))
    }

    #[cfg(unix)]
    fn communicate_with_generate_process<F>(
        mut process: BurnGenerateProcessHandle,
        mut progress_callback: F,
    ) -> Result<(), String>
    where
        F: FnMut(&str, f32, &str),
    {
        use maolan_generate::GenerateError;
        use maolan_generate::GenerateProgress;
        use maolan_generate::GenerateResponseHeader;

        // Read messages until we get the header
        // Progress messages come before the header
        let header: GenerateResponseHeader;
        loop {
            let payload = match Self::read_ipc_payload(&mut process.socket) {
                Ok(payload) => payload,
                Err(err) => {
                    let stderr_tail = process
                        .stderr_log
                        .lock()
                        .ok()
                        .map(|log| log.join("\n"))
                        .unwrap_or_default();
                    let exit_status = process
                        .exit_status
                        .lock()
                        .ok()
                        .and_then(|status| *status)
                        .map(|status| status.to_string())
                        .unwrap_or_else(|| "unknown".to_string());
                    let stderr_suffix = if stderr_tail.trim().is_empty() {
                        String::new()
                    } else {
                        format!("\nChild stderr:\n{stderr_tail}")
                    };
                    return Err(format!(
                        "{err}\nmaolan-generate exited before completing IPC (status: {exit_status}){stderr_suffix}"
                    ));
                }
            };

            match serde_json::from_slice::<GenerateProgress>(&payload) {
                Ok(progress) => {
                    progress_callback(&progress.phase, progress.progress, &progress.operation);
                    continue;
                }
                Err(_) => {
                    if let Ok(error) = serde_json::from_slice::<GenerateError>(&payload) {
                        return Err(error.error);
                    }
                    header = match serde_json::from_slice::<GenerateResponseHeader>(&payload) {
                        Ok(h) => h,
                        Err(err) => {
                            return Err(format!("Failed to decode IPC response payload: {err}"));
                        }
                    };
                    break;
                }
            }
        }

        // Drop the socket to signal EOF
        drop(process.socket);
        let _ = header;
        Ok(())
    }

    #[cfg(not(unix))]
    fn spawn_generate_process(_request: &BurnGenerateRequest) -> Result<(u32, ()), String> {
        Err("Generated audio via generate is only available on Unix platforms".to_string())
    }
    fn plugin_graph_title(state: &StateData) -> String {
        if let Some(target) = state.plugin_graph_clip.as_ref() {
            if let Some(track) = state
                .tracks
                .iter()
                .find(|track| track.name == target.track_name)
                && let Some(clip) = track.audio.clips.get(target.clip_idx)
            {
                return format!("Clip Plugins: {} / {}", target.track_name, clip.name);
            }
            return format!(
                "Clip Plugins: {} / clip {}",
                target.track_name, target.clip_idx
            );
        }
        format!(
            "Track Plugins: {}",
            state
                .plugin_graph_track
                .clone()
                .unwrap_or_else(|| "(no track)".to_string())
        )
    }

    fn normalize_recent_session_paths(paths: Vec<String>) -> Vec<String> {
        let mut normalized = Vec::new();
        let mut seen = HashSet::new();
        for path in paths {
            let trimmed = path.trim();
            if trimmed.is_empty() {
                continue;
            }
            if seen.insert(trimmed.to_string()) {
                normalized.push(trimmed.to_string());
            }
            if normalized.len() >= MAX_RECENT_SESSIONS {
                break;
            }
        }
        normalized
    }

    fn remember_recent_session_path(&mut self, path: &Path) {
        let current = path.to_string_lossy().to_string();
        if current.trim().is_empty() {
            return;
        }
        let mut cfg = config::Config::load().unwrap_or_default();
        let mut recent = vec![current.clone()];
        recent.extend(
            cfg.recent_session_paths
                .into_iter()
                .filter(|p| p != &current),
        );
        let recent = Self::normalize_recent_session_paths(recent);
        cfg.recent_session_paths = recent.clone();
        if let Err(err) = cfg.save() {
            error!("Failed to save recent session paths: {err}");
        }
        self.menu.update_recent_sessions(recent);
    }

    fn default_plugin_format() -> PluginFormat {
        if platform_caps::SUPPORTS_LV2 {
            PluginFormat::Lv2
        } else {
            PluginFormat::Vst3
        }
    }

    fn supported_plugin_formats() -> Vec<PluginFormat> {
        let mut formats = Vec::new();
        if platform_caps::SUPPORTS_LV2 {
            formats.push(PluginFormat::Lv2);
        }
        formats.push(PluginFormat::Clap);
        formats.push(PluginFormat::Vst3);
        formats
    }

    fn session_display_name_from_path(path: &Path) -> Option<String> {
        let mut display_path = path;
        if let Some(parent) = path.parent()
            && parent
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n == "snapshots")
            && parent
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .is_some_and(|n| n == ".maolan_autosave")
            && let Some(session_root) = parent.parent().and_then(Path::parent)
        {
            display_path = session_root;
        }
        display_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .filter(|name| !name.is_empty())
    }

    pub fn title(&self) -> String {
        let session = self
            .session_dir
            .as_ref()
            .and_then(|path| Self::session_display_name_from_path(path))
            .unwrap_or_else(|| "<New>".to_string());
        let dirty_suffix = if self.has_unsaved_changes { " *" } else { "" };
        format!("Maolan: {session}{dirty_suffix}")
    }

    fn samples_per_beat(&self) -> f64 {
        let (tempo, denom) = {
            let state = self.state.blocking_read();
            (
                state.tempo.max(1.0) as f64,
                state.time_signature_denom.max(1) as f64,
            )
        };
        let quarter = self.playback_rate_hz * 60.0 / tempo;
        quarter * (4.0 / denom)
    }

    fn samples_per_bar(&self) -> f64 {
        let beats_per_bar = self.state.blocking_read().time_signature_num.max(1) as f64;
        self.samples_per_beat() * beats_per_bar
    }

    fn snap_sample_to_bar(&self, sample: f32) -> usize {
        self.snap_mode.snap_sample(
            sample as f64,
            self.samples_per_beat(),
            self.samples_per_bar(),
        ) as usize
    }

    fn snap_sample_to_bar_drag(&self, sample: f32, delta_samples: f32) -> usize {
        self.snap_mode.snap_sample_drag(
            sample as f64,
            delta_samples as f64,
            self.samples_per_beat(),
            self.samples_per_bar(),
        ) as usize
    }

    fn snap_interval_samples(&self) -> usize {
        self.snap_mode
            .interval_samples(self.samples_per_beat(), self.samples_per_bar()) as usize
    }

    fn create_empty_midi_clip_file(
        &self,
        track_name: &str,
        session_root: &Path,
    ) -> std::io::Result<String> {
        fs::create_dir_all(session_root.join("midi"))?;
        let stem = format!("{}_clip", Self::sanitize_peak_file_component(track_name));
        let rel = Self::unique_import_rel_path(session_root, "midi", &stem, "mid")?;
        let path = session_root.join(&rel);
        let events = vec![
            TrackEvent {
                delta: u28::new(0),
                kind: TrackEventKind::Meta(MetaMessage::Tempo(u24::new(500_000))),
            },
            TrackEvent {
                delta: u28::new(0),
                kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
            },
        ];
        let smf = Smf {
            header: Header::new(Format::SingleTrack, Timing::Metrical(u15::new(480))),
            tracks: vec![events],
        };
        let mut file = File::create(&path)?;
        smf.write_std(&mut file)
            .map_err(|e| io::Error::other(format!("Failed to write '{}': {e}", path.display())))?;
        Ok(rel)
    }

    fn tracks_width_px(&self) -> f32 {
        match self.state.blocking_read().tracks_width {
            Length::Fixed(v) => v,
            _ => 200.0,
        }
    }

    fn editor_width_px(&self) -> f32 {
        (self.size.width - self.tracks_width_px() - 3.0).max(1.0)
    }

    fn pixels_per_sample(&self) -> f32 {
        let total_samples = self.samples_per_bar() * self.zoom_visible_bars as f64;
        if total_samples <= 0.0 {
            return 1.0;
        }
        (self.editor_width_px() as f64 / total_samples) as f32
    }

    fn editor_visible_samples(&self) -> f64 {
        (self.samples_per_bar() * self.zoom_visible_bars as f64).max(1.0)
    }

    fn editor_timeline_samples(&self) -> f64 {
        let state = self.state.blocking_read();
        let max_end_samples = state
            .tracks
            .iter()
            .filter(|track| track.name != METRONOME_TRACK_ID)
            .flat_map(|track| {
                let audio = track
                    .audio
                    .clips
                    .iter()
                    .map(|clip| clip.start.saturating_add(clip.length));
                let midi = track
                    .midi
                    .clips
                    .iter()
                    .map(|clip| clip.start.saturating_add(clip.length));
                audio.chain(midi)
            })
            .max()
            .unwrap_or(0) as f64;
        let visible_samples = self.editor_visible_samples();
        let right_padding_samples = visible_samples * 0.5;
        let min_timeline_samples =
            (self.samples_per_bar() * crate::consts::workspace::MIN_TIMELINE_BARS as f64).max(1.0);
        max_end_samples
            .max(self.transport_samples.max(0.0) + right_padding_samples)
            .max(max_end_samples + right_padding_samples)
            .max(visible_samples)
            .max(min_timeline_samples)
    }

    fn editor_max_scroll_samples(&self) -> f64 {
        (self.editor_timeline_samples() - self.editor_visible_samples()).max(0.0)
    }

    fn editor_scroll_relative_x(&self) -> f32 {
        let max_scroll = self.editor_max_scroll_samples();
        if max_scroll <= 0.0 {
            0.0
        } else {
            (self.editor_scroll_origin_samples / max_scroll).clamp(0.0, 1.0) as f32
        }
    }

    fn beat_pixels(&self) -> f32 {
        (self.samples_per_beat() as f32 * self.pixels_per_sample()).max(0.01)
    }

    fn start_recording_preview(&mut self) {
        let sample = self.transport_samples.max(0.0) as usize;
        self.recording_preview_start_sample = Some(sample);
        self.recording_preview_sample = Some(sample);
        self.recording_preview_peaks.clear();
    }

    fn stop_recording_preview(&mut self) {
        self.recording_preview_start_sample = None;
        self.recording_preview_sample = None;
        self.recording_preview_peaks.clear();
    }

    fn recording_preview_bounds(&self) -> Option<(usize, usize)> {
        let start = self.recording_preview_start_sample?;
        let current = self.recording_preview_sample?;
        let (mut preview_start, mut preview_end) = if current > start {
            (start, current)
        } else {
            return None;
        };
        if self.punch_enabled
            && let Some((punch_start, punch_end)) = self.punch_range_samples
            && punch_end > punch_start
        {
            preview_start = preview_start.max(punch_start);
            preview_end = preview_end.min(punch_end);
        }
        if preview_end > preview_start {
            Some((preview_start, preview_end))
        } else {
            None
        }
    }

    fn audio_clip_key(
        track_name: &str,
        clip_name: &str,
        start: usize,
        length: usize,
        offset: usize,
    ) -> AudioClipKey {
        AudioClipKey {
            track_name: track_name.to_string(),
            clip_name: clip_name.to_string(),
            start,
            length,
            offset,
        }
    }

    fn sanitize_peak_file_component(value: &str) -> String {
        let mut out = String::with_capacity(value.len());
        for ch in value.chars() {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                out.push(ch);
            } else {
                out.push('_');
            }
        }
        if out.is_empty() {
            "clip".to_string()
        } else {
            out
        }
    }

    fn build_peak_file_rel(track_name: &str, clip_idx: usize, clip_name: &str) -> String {
        let track = Self::sanitize_peak_file_component(track_name);
        let clip = Self::sanitize_peak_file_component(clip_name);
        format!("peaks/{}_{:04}_{}.json", track, clip_idx, clip)
    }

    fn compute_audio_clip_peaks(path: &Path) -> std::io::Result<ClipPeaks> {
        let mut wav = Wav::<f32>::from_path(path).map_err(|e| {
            io::Error::other(format!("Failed to open WAV '{}': {e}", path.display()))
        })?;
        let channels = wav.n_channels().max(1) as usize;
        let samples: wavers::Samples<f32> = wav
            .read()
            .map_err(|e| io::Error::other(format!("WAV read error '{}': {e}", path.display())))?;
        let mut per_channel = vec![Vec::with_capacity(samples.len() / channels + 1); channels];
        for frame in samples.chunks(channels) {
            for (channel_idx, sample) in frame.iter().enumerate() {
                per_channel[channel_idx].push(sample.clamp(-1.0, 1.0));
            }
        }

        if per_channel.iter().all(|ch| ch.is_empty()) {
            return Ok(Arc::new(Vec::new()));
        }

        // High-resolution, but bounded to avoid huge startup stalls and massive peak files.
        let target_bins = per_channel
            .iter()
            .map(Vec::len)
            .max()
            .unwrap_or(0)
            .clamp(1024, MAX_PEAK_BINS);

        let mut peaks = vec![vec![[0.0_f32, 0.0_f32]; target_bins]; channels];
        for channel_idx in 0..channels {
            let samples = &per_channel[channel_idx];
            if samples.is_empty() {
                continue;
            }
            let mut touched = vec![false; target_bins];
            for (i, sample) in samples.iter().enumerate() {
                let bin = (i * target_bins) / samples.len();
                let idx = bin.min(target_bins - 1);
                if !touched[idx] {
                    peaks[channel_idx][idx] = [*sample, *sample];
                    touched[idx] = true;
                } else {
                    peaks[channel_idx][idx][0] = peaks[channel_idx][idx][0].min(*sample);
                    peaks[channel_idx][idx][1] = peaks[channel_idx][idx][1].max(*sample);
                }
            }
        }
        Ok(Arc::new(peaks))
    }

    fn read_clip_peaks_file(path: &Path) -> std::io::Result<ClipPeaks> {
        let file = File::open(path)?;
        let json: Value = serde_json::from_reader(BufReader::new(file))?;
        let peaks_val = json.get("peaks").cloned().unwrap_or(Value::Null);
        let Some(root) = peaks_val.as_array() else {
            return Ok(Arc::new(Vec::new()));
        };

        if root.first().is_some_and(Value::is_number) {
            let mono = root
                .iter()
                .filter_map(Value::as_f64)
                .map(|v| {
                    let a = (v as f32).abs().clamp(0.0, 1.0);
                    [-a, a]
                })
                .collect::<Vec<_>>();
            return Ok(Arc::new(if mono.is_empty() {
                Vec::new()
            } else {
                vec![mono]
            }));
        }

        let first = root.first();
        if first.is_some_and(|v| {
            v.as_array()
                .is_some_and(|a| a.len() == 2 && a[0].is_number() && a[1].is_number())
        }) {
            let mono = root
                .iter()
                .filter_map(Value::as_array)
                .filter_map(|pair| {
                    let min = pair.first()?.as_f64()? as f32;
                    let max = pair.get(1)?.as_f64()? as f32;
                    Some([min.min(max).clamp(-1.0, 1.0), min.max(max).clamp(-1.0, 1.0)])
                })
                .collect::<Vec<_>>();
            return Ok(Arc::new(if mono.is_empty() {
                Vec::new()
            } else {
                vec![mono]
            }));
        }

        let mut per_channel: Vec<Vec<[f32; 2]>> = Vec::with_capacity(root.len());
        for channel in root {
            let Some(arr) = channel.as_array() else {
                continue;
            };
            let mut ch = Vec::with_capacity(arr.len());
            for peak in arr {
                if let Some(pair) = peak.as_array()
                    && pair.len() == 2
                    && let (Some(min), Some(max)) = (
                        pair.first().and_then(Value::as_f64),
                        pair.get(1).and_then(Value::as_f64),
                    )
                {
                    let min = min as f32;
                    let max = max as f32;
                    ch.push([min.min(max).clamp(-1.0, 1.0), min.max(max).clamp(-1.0, 1.0)]);
                    continue;
                }
                if let Some(v) = peak.as_f64() {
                    let a = (v as f32).abs().clamp(0.0, 1.0);
                    ch.push([-a, a]);
                }
            }
            per_channel.push(ch);
        }
        Ok(Arc::new(per_channel))
    }

    fn stream_peak_file_to_queue(
        peaks_path: &Path,
        track_name: String,
        clip_name: String,
        start: usize,
        length: usize,
        offset: usize,
    ) -> std::io::Result<()> {
        let peaks = Self::read_clip_peaks_file(peaks_path)?;
        if peaks.is_empty() {
            if let Ok(mut queue) = AUDIO_PEAK_UPDATES.lock() {
                queue.push(AudioPeakChunkUpdate {
                    track_name,
                    clip_name,
                    start,
                    length,
                    offset,
                    channels: 0,
                    target_bins: 0,
                    bin_start: 0,
                    peaks: Vec::new(),
                    done: true,
                });
            }
            return Ok(());
        }

        let channels = peaks.len();
        let target_bins = peaks.first().map(Vec::len).unwrap_or(0);
        if target_bins == 0 {
            if let Ok(mut queue) = AUDIO_PEAK_UPDATES.lock() {
                queue.push(AudioPeakChunkUpdate {
                    track_name,
                    clip_name,
                    start,
                    length,
                    offset,
                    channels,
                    target_bins: 0,
                    bin_start: 0,
                    peaks: Vec::new(),
                    done: true,
                });
            }
            return Ok(());
        }

        let mut bin_start = 0usize;
        while bin_start < target_bins {
            let end = (bin_start + BINS_PER_UPDATE).min(target_bins);
            let mut chunk = vec![Vec::with_capacity(end - bin_start); channels];
            for (channel_idx, chunk_channel) in chunk.iter_mut().enumerate().take(channels) {
                if let Some(channel) = peaks.get(channel_idx) {
                    chunk_channel.extend_from_slice(&channel[bin_start..end]);
                }
            }
            if let Ok(mut queue) = AUDIO_PEAK_UPDATES.lock() {
                queue.push(AudioPeakChunkUpdate {
                    track_name: track_name.clone(),
                    clip_name: clip_name.clone(),
                    start,
                    length,
                    offset,
                    channels,
                    target_bins,
                    bin_start,
                    peaks: chunk,
                    done: false,
                });
            }
            bin_start = end;
        }

        if let Ok(mut queue) = AUDIO_PEAK_UPDATES.lock() {
            queue.push(AudioPeakChunkUpdate {
                track_name,
                clip_name,
                start,
                length,
                offset,
                channels,
                target_bins,
                bin_start: 0,
                peaks: Vec::new(),
                done: true,
            });
        }
        Ok(())
    }

    fn stream_audio_clip_peaks_to_queue(
        path: &Path,
        track_name: String,
        clip_name: String,
        start: usize,
        length: usize,
        offset: usize,
    ) -> std::io::Result<()> {
        let mut wav = Wav::<f32>::from_path(path).map_err(|e| {
            io::Error::other(format!("Failed to open WAV '{}': {e}", path.display()))
        })?;
        let channels = wav.n_channels().max(1) as usize;
        let total_frames = wav.n_samples() / channels.max(1);
        if total_frames == 0 {
            if let Ok(mut queue) = AUDIO_PEAK_UPDATES.lock() {
                queue.push(AudioPeakChunkUpdate {
                    track_name,
                    clip_name,
                    start,
                    length,
                    offset,
                    channels,
                    target_bins: 0,
                    bin_start: 0,
                    peaks: Vec::new(),
                    done: true,
                });
            }
            return Ok(());
        }

        let target_bins = total_frames.clamp(1024, MAX_PEAK_BINS);
        let mut accum = vec![vec![[0.0_f32, 0.0_f32]; target_bins]; channels];
        let mut touched = vec![vec![false; target_bins]; channels];
        let mut processed_frames = 0usize;
        let mut last_emitted_bin = 0usize;

        while processed_frames < total_frames {
            let frames_to_read = (total_frames - processed_frames).min(CHUNK_FRAMES);
            let samples_to_read = frames_to_read.saturating_mul(channels);
            let chunk: wavers::Samples<f32> = wav.read_samples(samples_to_read).map_err(|e| {
                io::Error::other(format!("WAV read error '{}': {e}", path.display()))
            })?;
            if chunk.is_empty() {
                break;
            }
            let frames_read = chunk.len() / channels.max(1);
            if frames_read == 0 {
                break;
            }

            for (frame_offset, frame) in chunk.chunks(channels).enumerate() {
                let frame_index = processed_frames + frame_offset;
                let bin = ((frame_index * target_bins) / total_frames).min(target_bins - 1);
                for (channel_idx, sample) in frame.iter().enumerate() {
                    let s = sample.clamp(-1.0, 1.0);
                    if !touched[channel_idx][bin] {
                        accum[channel_idx][bin] = [s, s];
                        touched[channel_idx][bin] = true;
                    } else {
                        accum[channel_idx][bin][0] = accum[channel_idx][bin][0].min(s);
                        accum[channel_idx][bin][1] = accum[channel_idx][bin][1].max(s);
                    }
                }
            }

            processed_frames = processed_frames.saturating_add(frames_read);
            let emit_end = (((processed_frames * target_bins) / total_frames) + 1).min(target_bins);
            if emit_end > last_emitted_bin {
                let mut peaks_chunk =
                    vec![Vec::with_capacity(emit_end - last_emitted_bin); channels];
                for channel_idx in 0..channels {
                    for bin in last_emitted_bin..emit_end {
                        let pair = if touched[channel_idx][bin] {
                            accum[channel_idx][bin]
                        } else {
                            [0.0_f32, 0.0_f32]
                        };
                        peaks_chunk[channel_idx].push(pair);
                    }
                }
                if let Ok(mut queue) = AUDIO_PEAK_UPDATES.lock() {
                    queue.push(AudioPeakChunkUpdate {
                        track_name: track_name.clone(),
                        clip_name: clip_name.clone(),
                        start,
                        length,
                        offset,
                        channels,
                        target_bins,
                        bin_start: last_emitted_bin,
                        peaks: peaks_chunk,
                        done: false,
                    });
                }
                last_emitted_bin = emit_end;
            }
        }

        if let Ok(mut queue) = AUDIO_PEAK_UPDATES.lock() {
            queue.push(AudioPeakChunkUpdate {
                track_name,
                clip_name,
                start,
                length,
                offset,
                channels,
                target_bins,
                bin_start: 0,
                peaks: Vec::new(),
                done: true,
            });
        }
        Ok(())
    }

    fn audio_clip_source_length(path: &Path) -> std::io::Result<usize> {
        let wav = Wav::<f32>::from_path(path).map_err(|e| {
            io::Error::other(format!("Failed to open WAV '{}': {e}", path.display()))
        })?;
        Ok(wav.n_samples() / wav.n_channels().max(1) as usize)
    }

    fn file_extension_lower(path: &Path) -> Option<String> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.trim_matches('.').to_ascii_lowercase())
    }

    fn normalize_session_media_rel(path: &str) -> Option<String> {
        let candidate = Path::new(path);
        if candidate.is_absolute() {
            return None;
        }

        let mut parts = Vec::new();
        for component in candidate.components() {
            match component {
                std::path::Component::Normal(part) => {
                    parts.push(part.to_string_lossy().to_string());
                }
                std::path::Component::CurDir => {}
                _ => return None,
            }
        }

        if parts.is_empty() {
            None
        } else {
            Some(parts.join("/"))
        }
    }

    fn is_cleanup_target_rel(path: &str) -> bool {
        if path.starts_with("audio/") || path.starts_with("midi/") {
            let rel_path = Path::new(path);
            let Some(ext) = Self::file_extension_lower(rel_path) else {
                return false;
            };
            if matches!(ext.as_str(), "wav" | "mid" | "midi") {
                return true;
            }
            if path.starts_with("audio/") && ext == "txt" {
                let stem = rel_path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("");
                return stem.contains("_pitchmap");
            }
            return false;
        }

        (path.starts_with("peaks/") || path.starts_with("pitch/"))
            && Path::new(path).extension().and_then(|ext| ext.to_str()) == Some("json")
    }

    fn insert_referenced_session_media_path(referenced: &mut HashSet<String>, path: &str) {
        let Some(rel) = Self::normalize_session_media_rel(path) else {
            return;
        };
        if Self::is_cleanup_target_rel(&rel) {
            referenced.insert(rel);
        }
    }

    fn pitch_correction_cache_rel(
        source_name: &str,
        source_offset: usize,
        source_length: usize,
    ) -> String {
        let mut hasher = DefaultHasher::new();
        source_name.hash(&mut hasher);
        source_offset.hash(&mut hasher);
        source_length.hash(&mut hasher);
        format!("pitch/{:016x}.json", hasher.finish())
    }

    fn insert_pitch_correction_cache_reference(
        referenced: &mut HashSet<String>,
        clip: &crate::state::AudioClip,
    ) {
        let Some(source_name) = clip.pitch_correction_source_name.as_deref() else {
            return;
        };
        let Some(source_rel) = Self::normalize_session_media_rel(source_name) else {
            return;
        };
        let cache_rel = Self::pitch_correction_cache_rel(
            &source_rel,
            clip.pitch_correction_source_offset.unwrap_or(clip.offset),
            clip.pitch_correction_source_length.unwrap_or(clip.length),
        );
        referenced.insert(cache_rel);
    }

    fn referenced_session_media_paths(state: &crate::state::StateData) -> HashSet<String> {
        let mut referenced = HashSet::new();

        for track in &state.tracks {
            for clip in &track.audio.clips {
                Self::insert_referenced_session_media_path(&mut referenced, &clip.name);
                if let Some(source_name) = clip.pitch_correction_source_name.as_deref() {
                    Self::insert_referenced_session_media_path(&mut referenced, source_name);
                }
                if let Some(peaks_file) = clip.peaks_file.as_deref() {
                    Self::insert_referenced_session_media_path(&mut referenced, peaks_file);
                }
                Self::insert_pitch_correction_cache_reference(&mut referenced, clip);
            }
            for clip in &track.midi.clips {
                Self::insert_referenced_session_media_path(&mut referenced, &clip.name);
            }
            for clip in &track.frozen_audio_backup {
                Self::insert_referenced_session_media_path(&mut referenced, &clip.name);
                if let Some(source_name) = clip.pitch_correction_source_name.as_deref() {
                    Self::insert_referenced_session_media_path(&mut referenced, source_name);
                }
                if let Some(peaks_file) = clip.peaks_file.as_deref() {
                    Self::insert_referenced_session_media_path(&mut referenced, peaks_file);
                }
                Self::insert_pitch_correction_cache_reference(&mut referenced, clip);
            }
            for clip in &track.frozen_midi_backup {
                Self::insert_referenced_session_media_path(&mut referenced, &clip.name);
            }
            if let Some(render_clip) = track.frozen_render_clip.as_deref() {
                Self::insert_referenced_session_media_path(&mut referenced, render_clip);
            }
        }

        referenced
    }

    fn collect_cleanup_candidate_files(
        dir: &Path,
        session_root: &Path,
        referenced: &HashSet<String>,
        out: &mut Vec<(PathBuf, String)>,
    ) -> io::Result<()> {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(err),
        };

        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                Self::collect_cleanup_candidate_files(&path, session_root, referenced, out)?;
                continue;
            }
            if !file_type.is_file() {
                continue;
            }

            let Ok(rel_path) = path.strip_prefix(session_root) else {
                continue;
            };
            let rel = rel_path.to_string_lossy().replace('\\', "/");
            if Self::is_cleanup_target_rel(&rel) && !referenced.contains(&rel) {
                out.push((path, rel));
            }
        }

        Ok(())
    }

    fn delete_unused_session_media_files(
        &self,
        session_root: &Path,
    ) -> Result<SessionMediaCleanupReport, String> {
        let referenced = {
            let state = self.state.blocking_read();
            Self::referenced_session_media_paths(&state)
        };

        let mut candidates = Vec::new();
        Self::collect_cleanup_candidate_files(
            &session_root.join("audio"),
            session_root,
            &referenced,
            &mut candidates,
        )
        .map_err(|e| format!("Failed to scan audio/: {e}"))?;
        Self::collect_cleanup_candidate_files(
            &session_root.join("midi"),
            session_root,
            &referenced,
            &mut candidates,
        )
        .map_err(|e| format!("Failed to scan midi/: {e}"))?;
        Self::collect_cleanup_candidate_files(
            &session_root.join("peaks"),
            session_root,
            &referenced,
            &mut candidates,
        )
        .map_err(|e| format!("Failed to scan peaks/: {e}"))?;
        Self::collect_cleanup_candidate_files(
            &session_root.join("pitch"),
            session_root,
            &referenced,
            &mut candidates,
        )
        .map_err(|e| format!("Failed to scan pitch/: {e}"))?;
        candidates.sort_by(|a, b| a.1.cmp(&b.1));

        let mut report = SessionMediaCleanupReport::default();
        for (path, rel) in candidates {
            match fs::remove_file(&path) {
                Ok(()) => report.deleted_files.push(rel),
                Err(e) => report.failed_files.push(format!("{rel} ({e})")),
            }
        }

        Ok(report)
    }

    fn is_import_audio_path(path: &Path) -> bool {
        matches!(
            Self::file_extension_lower(path).as_deref(),
            Some("wav" | "ogg" | "mp3" | "flac")
        )
    }

    fn is_import_midi_path(path: &Path) -> bool {
        matches!(
            Self::file_extension_lower(path).as_deref(),
            Some("mid" | "midi")
        )
    }

    fn import_track_base_name(path: &Path) -> String {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Track")
            .trim();
        let candidate = Self::sanitize_peak_file_component(stem);
        if candidate.is_empty() {
            "Track".to_string()
        } else {
            candidate
        }
    }

    fn sanitize_generated_track_base_name(prompt: &str) -> String {
        let prompt = prompt.trim();
        if prompt.is_empty() {
            return "Generated".to_string();
        }
        let shortened = prompt.chars().take(32).collect::<String>();
        let candidate = Self::sanitize_peak_file_component(&shortened);
        if candidate.is_empty() {
            "Generated".to_string()
        } else {
            candidate
        }
    }

    fn generate_audio_tags_with_timing(
        tags: &str,
        bpm: f32,
        time_signature_num: u8,
        time_signature_denom: u8,
    ) -> String {
        let mut parts = tags
            .split(',')
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        parts.push(format!("{:.0}bpm", bpm.clamp(20.0, 300.0)));
        parts.push(format!(
            "{}/{} tempo signature",
            time_signature_num.max(1),
            time_signature_denom.max(1)
        ));
        parts.join(",")
    }

    fn unique_track_name(base: &str, used_names: &mut HashSet<String>) -> String {
        if !used_names.contains(base) {
            let candidate = base.to_string();
            used_names.insert(candidate.clone());
            return candidate;
        }
        for n in 2..=9_999 {
            let candidate = format!("{base}_{n}");
            if !used_names.contains(&candidate) {
                used_names.insert(candidate.clone());
                return candidate;
            }
        }
        let fallback = format!("{base}_import");
        used_names.insert(fallback.clone());
        fallback
    }

    fn unique_import_rel_path(
        session_root: &Path,
        subdir: &str,
        stem: &str,
        ext: &str,
    ) -> std::io::Result<String> {
        fs::create_dir_all(session_root.join(subdir))?;
        let clean_stem = Self::sanitize_peak_file_component(stem);
        let clean_ext = ext.trim_matches('.').to_ascii_lowercase();
        let mut index = 0usize;
        loop {
            let file_name = if index == 0 {
                format!("{clean_stem}.{clean_ext}")
            } else {
                format!("{clean_stem}_{index:03}.{clean_ext}")
            };
            let rel = format!("{subdir}/{file_name}");
            if !session_root.join(&rel).exists() {
                return Ok(rel);
            }
            index = index.saturating_add(1);
        }
    }

    async fn decode_audio_to_f32_interleaved_with_progress<F>(
        path: &Path,
        mut progress_callback: F,
    ) -> std::io::Result<(Vec<f32>, usize, u32)>
    where
        F: FnMut(f32),
    {
        let file = File::open(path)?;
        let file_size = file.metadata()?.len();
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        let probed = symphonia::default::get_probe()
            .format(
                &hint,
                mss,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .map_err(|e| {
                io::Error::other(format!(
                    "Unsupported or unreadable audio '{}': {e}",
                    path.display()
                ))
            })?;
        let mut format = probed.format;
        let track = format.default_track().ok_or_else(|| {
            io::Error::other(format!("No decodable audio track in '{}'", path.display()))
        })?;
        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(|e| io::Error::other(format!("Failed to decode '{}': {e}", path.display())))?;
        let mut channels = track
            .codec_params
            .channels
            .map(|c| c.count())
            .unwrap_or(1usize)
            .max(1);
        let mut sample_rate = track.codec_params.sample_rate.unwrap_or(48_000u32);
        let mut samples = Vec::<f32>::new();

        let mut packets_decoded = 0usize;
        let report_interval = 100;

        loop {
            let packet = match format.next_packet() {
                Ok(packet) => packet,
                Err(SymphoniaError::IoError(e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    break;
                }
                Err(SymphoniaError::ResetRequired) => {
                    return Err(io::Error::other(format!(
                        "Decoder reset required for '{}'",
                        path.display()
                    )));
                }
                Err(e) => {
                    return Err(io::Error::other(format!(
                        "Failed reading audio packets '{}': {e}",
                        path.display()
                    )));
                }
            };
            let decoded = match decoder.decode(&packet) {
                Ok(decoded) => decoded,
                Err(SymphoniaError::DecodeError(_)) => continue,
                Err(SymphoniaError::ResetRequired) => {
                    return Err(io::Error::other(format!(
                        "Decoder reset required for '{}'",
                        path.display()
                    )));
                }
                Err(e) => {
                    return Err(io::Error::other(format!(
                        "Audio decode failed '{}': {e}",
                        path.display()
                    )));
                }
            };
            let spec = *decoded.spec();
            channels = spec.channels.count().max(1);
            sample_rate = spec.rate;
            let mut sample_buffer = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
            sample_buffer.copy_interleaved_ref(decoded);
            samples.extend_from_slice(sample_buffer.samples());

            packets_decoded += 1;
            if packets_decoded.is_multiple_of(report_interval) {
                let bytes_read = samples.len() * std::mem::size_of::<f32>();
                let progress = if file_size > 0 {
                    (bytes_read as f64 / file_size as f64).clamp(0.0, 1.0) as f32
                } else {
                    0.0
                };
                progress_callback(progress);
                tokio::task::yield_now().await;
            }
        }

        if samples.is_empty() {
            return Err(io::Error::other(format!(
                "Audio file '{}' contains no samples",
                path.display()
            )));
        }
        progress_callback(1.0);
        Ok((samples, channels, sample_rate))
    }

    async fn resample_audio_interleaved_with_progress<F>(
        samples: &[f32],
        channels: usize,
        from_rate: u32,
        to_rate: u32,
        mut progress_callback: F,
    ) -> std::io::Result<Vec<f32>>
    where
        F: FnMut(f32),
    {
        if from_rate == to_rate {
            progress_callback(1.0);
            return Ok(samples.to_vec());
        }

        use rubato::{
            Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType,
            WindowFunction,
        };

        let params = SincInterpolationParameters {
            sinc_len: 256,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: 256,
            window: WindowFunction::BlackmanHarris2,
        };

        progress_callback(0.1);

        let mut resampler = SincFixedIn::<f32>::new(
            to_rate as f64 / from_rate as f64,
            2.0,
            params,
            samples.len() / channels.max(1),
            channels.max(1),
        )
        .map_err(|e| io::Error::other(format!("Failed to create resampler: {e}")))?;

        progress_callback(0.2);
        tokio::task::yield_now().await;

        let frames = samples.len() / channels.max(1);
        let mut channel_buffers: Vec<Vec<f32>> = vec![Vec::with_capacity(frames); channels];
        for (i, &sample) in samples.iter().enumerate() {
            channel_buffers[i % channels].push(sample);
        }

        progress_callback(0.5);
        tokio::task::yield_now().await;

        let resampled = resampler
            .process(&channel_buffers, None)
            .map_err(|e| io::Error::other(format!("Resampling failed: {e}")))?;

        progress_callback(0.8);
        tokio::task::yield_now().await;

        let out_frames = resampled[0].len();
        let mut output = Vec::with_capacity(out_frames * channels);
        for frame_idx in 0..out_frames {
            for channel in resampled.iter().take(channels) {
                output.push(channel[frame_idx]);
            }
        }

        progress_callback(1.0);
        Ok(output)
    }

    async fn import_audio_to_session_wav_with_progress<F>(
        src_path: &Path,
        session_root: &Path,
        target_sample_rate: u32,
        mut progress_callback: F,
    ) -> std::io::Result<(String, usize, usize, ClipPeaks)>
    where
        F: FnMut(f32, Option<String>),
    {
        let stem = src_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("audio");
        let rel = Self::unique_import_rel_path(session_root, "audio", stem, "wav")?;
        let dst = session_root.join(&rel);

        let (samples, channels, sample_rate) =
            Self::decode_audio_to_f32_interleaved_with_progress(src_path, |decode_progress| {
                progress_callback(decode_progress * 0.6, Some("Decoding".to_string()));
            })
            .await?;

        progress_callback(0.6, None);
        tokio::task::yield_now().await;

        let final_samples = if sample_rate != target_sample_rate {
            let resample_msg = format!("Resampling {} Hz → {} Hz", sample_rate, target_sample_rate);
            Self::resample_audio_interleaved_with_progress(
                &samples,
                channels,
                sample_rate,
                target_sample_rate,
                |resample_progress| {
                    progress_callback(0.6 + resample_progress * 0.25, Some(resample_msg.clone()));
                },
            )
            .await?
        } else {
            samples
        };

        progress_callback(0.85, Some("Writing".to_string()));
        tokio::task::yield_now().await;

        wavers::write::<f32, _>(
            &dst,
            &final_samples,
            target_sample_rate as i32,
            channels as u16,
        )
        .map_err(|e| io::Error::other(format!("Failed to write '{}': {e}", dst.display())))?;

        progress_callback(0.95, Some("Calculating peaks".to_string()));
        tokio::task::yield_now().await;
        let peaks = Self::compute_audio_clip_peaks(&dst)?;

        progress_callback(1.0, None);
        let frames = final_samples.len() / channels.max(1);
        Ok((rel, channels.max(1), frames.max(1), peaks))
    }

    async fn stretch_audio_clip_with_rubberband(
        src_path: &Path,
        session_root: &Path,
        clip_name: &str,
        offset: usize,
        length: usize,
        stretch_ratio: f32,
    ) -> io::Result<(String, usize)> {
        let (samples, channels, sample_rate) =
            Self::decode_audio_to_f32_interleaved_with_progress(src_path, |_| {}).await?;
        let channels = channels.max(1);
        let total_frames = samples.len() / channels;
        let start_frame = offset.min(total_frames);
        let segment_frames = length.max(1).min(total_frames.saturating_sub(start_frame));
        if segment_frames == 0 {
            return Err(io::Error::other(format!(
                "Audio clip '{}' has no available source samples to stretch",
                clip_name
            )));
        }

        let start_idx = start_frame * channels;
        let end_idx = start_idx + segment_frames * channels;
        let segment_samples = &samples[start_idx..end_idx];
        let stem = Path::new(clip_name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("audio");
        let temp_rel =
            Self::unique_import_rel_path(session_root, "audio", &format!("{stem}_src"), "wav")?;
        let temp_path = session_root.join(&temp_rel);
        let output_rel =
            Self::unique_import_rel_path(session_root, "audio", &format!("{stem}_stretch"), "wav")?;
        let output_path = session_root.join(&output_rel);

        wavers::write::<f32, _>(
            &temp_path,
            segment_samples,
            sample_rate as i32,
            channels as u16,
        )
        .map_err(|e| {
            io::Error::other(format!(
                "Failed to write stretch source '{}': {e}",
                temp_path.display()
            ))
        })?;

        let command_result = tokio::process::Command::new("rubberband")
            .arg("--quiet")
            .arg("--fine")
            .arg("--time")
            .arg(format!("{:.6}", stretch_ratio.max(0.01)))
            .arg(&temp_path)
            .arg(&output_path)
            .output()
            .await;

        let _ = fs::remove_file(&temp_path);

        let output = command_result
            .map_err(|e| io::Error::other(format!("Failed to launch Rubber Band: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let _ = fs::remove_file(&output_path);
            return Err(io::Error::other(if stderr.is_empty() {
                "Rubber Band failed to stretch the clip".to_string()
            } else {
                format!("Rubber Band failed: {stderr}")
            }));
        }

        let output_frames = Self::audio_clip_source_length(&output_path)?;
        Ok((output_rel, output_frames.max(1)))
    }

    async fn analyze_audio_clip_pitch_correction<F>(
        src_path: &Path,
        clip_name: &str,
        offset: usize,
        length: usize,
        frame_likeness: f32,
        mut progress_callback: F,
    ) -> io::Result<PitchCorrectionData>
    where
        F: FnMut(f32, Option<String>),
    {
        let (samples, channels, sample_rate) =
            Self::decode_audio_to_f32_interleaved_with_progress(src_path, |decode_progress| {
                progress_callback(decode_progress * 0.55, Some("Decoding".to_string()));
            })
            .await?;
        let channels = channels.max(1);
        let total_frames = samples.len() / channels;
        let start_frame = offset.min(total_frames);
        let segment_frames = length.max(1).min(total_frames.saturating_sub(start_frame));
        if segment_frames < 256 {
            return Err(io::Error::other(format!(
                "Audio clip '{}' is too short for pitch analysis",
                clip_name
            )));
        }

        let mut mono = Vec::with_capacity(segment_frames);
        for frame_idx in 0..segment_frames {
            let base = (start_frame + frame_idx) * channels;
            let sample = (0..channels)
                .map(|channel| samples[base + channel])
                .sum::<f32>()
                / channels as f32;
            mono.push(sample);
        }
        progress_callback(0.60, Some("Preparing analysis".to_string()));

        let analysis_size = 2048usize.min(mono.len().next_power_of_two() / 2).max(256);
        let analysis_size = analysis_size.min(mono.len());
        let analysis_size = analysis_size
            .next_power_of_two()
            .min(mono.len().next_power_of_two());
        let analysis_size = analysis_size.min(mono.len());
        let analysis_size = analysis_size.clamp(256, 4096);
        if mono.len() < analysis_size {
            return Err(io::Error::other(format!(
                "Audio clip '{}' is too short for pitch analysis",
                clip_name
            )));
        }
        let hop_size = (analysis_size / 4).max(64);
        let padding = analysis_size / 2;
        let mut detector = McLeodDetector::new(analysis_size, padding);
        let mut detected = Vec::<PitchCorrectionPoint>::new();
        let mut cursor = 0usize;
        let total_windows = ((mono.len().saturating_sub(analysis_size)) / hop_size).max(1) + 1;
        let mut window_index = 0usize;
        while cursor + analysis_size <= mono.len() {
            let window: Vec<f64> = mono[cursor..cursor + analysis_size]
                .iter()
                .map(|sample| *sample as f64)
                .collect();
            if let Some(pitch) = detector.get_pitch(&window, sample_rate as usize, 5.0f64, 0.7f64)
                && pitch.frequency.is_finite()
                && pitch.frequency > 0.0
            {
                let midi_pitch = 69.0 + 12.0 * (pitch.frequency as f32 / 440.0).log2();
                if midi_pitch.is_finite() {
                    detected.push(PitchCorrectionPoint {
                        start_sample: cursor,
                        length_samples: hop_size.min(mono.len().saturating_sub(cursor)).max(1),
                        detected_midi_pitch: midi_pitch.clamp(0.0, f32::from(PITCH_MAX) + 0.999),
                        target_midi_pitch: midi_pitch.clamp(0.0, f32::from(PITCH_MAX) + 0.999),
                        clarity: pitch.clarity as f32,
                    });
                }
            }
            window_index = window_index.saturating_add(1);
            let analysis_progress = window_index as f32 / total_windows as f32;
            progress_callback(
                0.60 + analysis_progress.clamp(0.0, 1.0) * 0.40,
                Some("Detecting pitch".to_string()),
            );
            cursor = cursor.saturating_add(hop_size);
        }
        if detected.is_empty() {
            return Err(io::Error::other(format!(
                "No stable pitch detected in '{}'",
                clip_name
            )));
        }
        let frame_likeness = frame_likeness.clamp(0.05, 2.0);
        let raw_points = detected;
        let points =
            Self::merge_adjacent_pitch_fragments(raw_points.clone(), frame_likeness, hop_size);
        Ok(PitchCorrectionData {
            track_idx: String::new(),
            clip_index: 0,
            clip_name: clip_name.to_string(),
            clip_length_samples: length.max(1),
            frame_likeness,
            raw_points,
            points,
        })
    }

    fn regroup_pitch_correction_frames(
        pitch_correction: &mut PitchCorrectionData,
        frame_likeness: f32,
    ) {
        let clamped = frame_likeness.clamp(0.05, 2.0);
        let max_gap_samples = pitch_correction
            .raw_points
            .iter()
            .map(|point| point.length_samples)
            .min()
            .unwrap_or(1)
            .max(1);
        pitch_correction.frame_likeness = clamped;
        pitch_correction.points = Self::merge_adjacent_pitch_fragments(
            pitch_correction.raw_points.clone(),
            clamped,
            max_gap_samples,
        );
    }

    fn merge_adjacent_pitch_fragments(
        mut points: Vec<PitchCorrectionPoint>,
        max_pitch_delta: f32,
        max_gap_samples: usize,
    ) -> Vec<PitchCorrectionPoint> {
        if points.len() <= 1 {
            return points;
        }
        points.sort_by_key(|point| point.start_sample);
        let mut merged = Vec::with_capacity(points.len());
        let mut points_iter = points.into_iter();
        let Some(mut current) = points_iter.next() else {
            return merged;
        };
        let mut reference_detected = current.detected_midi_pitch;
        let mut reference_target = current.target_midi_pitch;

        for point in points_iter {
            let current_end = current.start_sample.saturating_add(current.length_samples);
            let gap = point.start_sample.saturating_sub(current_end);
            let detected_delta = (reference_detected - point.detected_midi_pitch).abs();
            let target_delta = (reference_target - point.target_midi_pitch).abs();

            if gap <= max_gap_samples
                && detected_delta <= max_pitch_delta
                && target_delta <= max_pitch_delta
            {
                let left_weight = (current.length_samples as f32) * current.clarity.max(0.05);
                let right_weight = (point.length_samples as f32) * point.clarity.max(0.05);
                let total_weight = (left_weight + right_weight).max(f32::EPSILON);
                let merged_end = point.start_sample.saturating_add(point.length_samples);
                current.length_samples = merged_end.saturating_sub(current.start_sample);
                current.detected_midi_pitch = ((current.detected_midi_pitch * left_weight)
                    + (point.detected_midi_pitch * right_weight))
                    / total_weight;
                current.target_midi_pitch = ((current.target_midi_pitch * left_weight)
                    + (point.target_midi_pitch * right_weight))
                    / total_weight;
                current.clarity = ((current.clarity * left_weight)
                    + (point.clarity * right_weight))
                    / total_weight;
            } else {
                merged.push(current);
                reference_detected = point.detected_midi_pitch;
                reference_target = point.target_midi_pitch;
                current = point;
            }
        }

        merged.push(current);
        merged
    }

    #[allow(clippy::too_many_arguments)]
    async fn render_audio_clip_pitch_correction_with_rubberband<F>(
        src_path: &Path,
        session_root: &Path,
        clip_name: &str,
        offset: usize,
        length: usize,
        points: &[PitchCorrectionPoint],
        inertia_ms: u16,
        formant_compensation: bool,
        mut progress_callback: F,
    ) -> io::Result<(String, usize, crate::state::ClipPeaks)>
    where
        F: FnMut(f32, Option<String>),
    {
        let (samples, channels, sample_rate) =
            Self::decode_audio_to_f32_interleaved_with_progress(src_path, |decode_progress| {
                progress_callback(decode_progress * 0.45, Some("Decoding".to_string()));
            })
            .await?;
        let channels = channels.max(1);
        let total_frames = samples.len() / channels;
        let start_frame = offset.min(total_frames);
        let segment_frames = length.max(1).min(total_frames.saturating_sub(start_frame));
        if segment_frames == 0 {
            return Err(io::Error::other(format!(
                "Audio clip '{}' has no available source samples for pitch correction",
                clip_name
            )));
        }
        let start_idx = start_frame * channels;
        let end_idx = start_idx + segment_frames * channels;
        let segment_samples = &samples[start_idx..end_idx];
        progress_callback(0.50, Some("Preparing source".to_string()));
        tokio::task::yield_now().await;

        let stem = Path::new(clip_name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("audio");
        let temp_rel =
            Self::unique_import_rel_path(session_root, "audio", &format!("{stem}_src"), "wav")?;
        let temp_path = session_root.join(&temp_rel);
        let pitchmap_rel = Self::unique_import_rel_path(
            session_root,
            "audio",
            &format!("{stem}_pitchmap"),
            "txt",
        )?;
        let pitchmap_path = session_root.join(&pitchmap_rel);
        let output_rel = Self::unique_import_rel_path(
            session_root,
            "audio",
            &format!("{stem}_pitch_corrected"),
            "wav",
        )?;
        let output_path = session_root.join(&output_rel);

        wavers::write::<f32, _>(
            &temp_path,
            segment_samples,
            sample_rate as i32,
            channels as u16,
        )
        .map_err(|e| {
            io::Error::other(format!(
                "Failed to write pitch-correction source '{}': {e}",
                temp_path.display()
            ))
        })?;

        let mut pitchmap = String::new();
        if points.is_empty() {
            pitchmap.push_str("0 0\n");
        } else {
            let mut sorted_points = points.to_vec();
            sorted_points.sort_by_key(|point| point.start_sample);
            let inertia_frames = ((sample_rate as u64 * inertia_ms as u64) / 1000) as usize;
            let mut previous_shift =
                sorted_points[0].target_midi_pitch - sorted_points[0].detected_midi_pitch;
            if sorted_points[0].start_sample > 0 {
                pitchmap.push_str(&format!("0 {:.6}\n", previous_shift));
            }
            for point in sorted_points {
                let start_sample = point.start_sample.min(segment_frames.saturating_sub(1));
                let target_shift = point.target_midi_pitch - point.detected_midi_pitch;
                if inertia_frames == 0 || (target_shift - previous_shift).abs() <= f32::EPSILON {
                    pitchmap.push_str(&format!("{start_sample} {:.6}\n", target_shift));
                } else {
                    pitchmap.push_str(&format!("{start_sample} {:.6}\n", previous_shift));
                    let glide_end = start_sample
                        .saturating_add(inertia_frames)
                        .min(segment_frames.saturating_sub(1));
                    if glide_end > start_sample {
                        pitchmap.push_str(&format!("{glide_end} {:.6}\n", target_shift));
                    } else {
                        pitchmap.push_str(&format!("{start_sample} {:.6}\n", target_shift));
                    }
                }
                previous_shift = target_shift;
            }
        }
        fs::write(&pitchmap_path, pitchmap).map_err(|e| {
            io::Error::other(format!(
                "Failed to write pitch map '{}': {e}",
                pitchmap_path.display()
            ))
        })?;
        progress_callback(0.60, Some("Writing pitch map".to_string()));
        tokio::task::yield_now().await;

        let command_result = tokio::process::Command::new("rubberband")
            .args({
                let mut args = vec![
                    "--quiet".to_string(),
                    "--fine".to_string(),
                    "--pitch-hq".to_string(),
                    "--pitch".to_string(),
                    "0".to_string(),
                    "--pitchmap".to_string(),
                    pitchmap_path.to_string_lossy().to_string(),
                    temp_path.to_string_lossy().to_string(),
                    output_path.to_string_lossy().to_string(),
                ];
                if formant_compensation {
                    args.insert(2, "--formant".to_string());
                }
                args
            })
            .output()
            .await;
        let _ = fs::remove_file(&temp_path);
        let _ = fs::remove_file(&pitchmap_path);
        progress_callback(0.90, Some("Rendering".to_string()));
        tokio::task::yield_now().await;

        let output = command_result
            .map_err(|e| io::Error::other(format!("Failed to launch Rubber Band: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let _ = fs::remove_file(&output_path);
            return Err(io::Error::other(if stderr.is_empty() {
                "Rubber Band failed to render pitch correction".to_string()
            } else {
                format!("Rubber Band failed: {stderr}")
            }));
        }
        let output_frames = Self::audio_clip_source_length(&output_path)?;
        progress_callback(0.95, Some("Calculating peaks".to_string()));
        tokio::task::yield_now().await;
        let peaks = Self::compute_audio_clip_peaks(&output_path)?;
        progress_callback(1.0, Some("Complete".to_string()));
        Ok((output_rel, output_frames.max(1), peaks))
    }

    fn write_wav_with_bit_depth(
        export_path: &Path,
        mixed_buffer: &[f32],
        sample_rate: i32,
        output_channels: usize,
        bit_depth: ExportBitDepth,
    ) -> io::Result<()> {
        match bit_depth {
            ExportBitDepth::Int16 => {
                let quantized: Vec<i16> = mixed_buffer
                    .iter()
                    .map(|s| {
                        (s.clamp(-1.0, 1.0) * i16::MAX as f32)
                            .round()
                            .clamp(i16::MIN as f32, i16::MAX as f32) as i16
                    })
                    .collect();
                wavers::write::<i16, _>(
                    export_path,
                    &quantized,
                    sample_rate,
                    output_channels as u16,
                )
            }
            ExportBitDepth::Int24 => wavers::write::<i24::i24, _>(
                export_path,
                &mixed_buffer
                    .iter()
                    .map(|s| {
                        i24::i24::from_i32(
                            (s.clamp(-1.0, 1.0) * 8_388_607.0)
                                .round()
                                .clamp(-8_388_608.0, 8_388_607.0)
                                as i32,
                        )
                    })
                    .collect::<Vec<i24::i24>>(),
                sample_rate,
                output_channels as u16,
            ),
            ExportBitDepth::Int32 => {
                let quantized: Vec<i32> = mixed_buffer
                    .iter()
                    .map(|s| {
                        (s.clamp(-1.0, 1.0) * i32::MAX as f32)
                            .round()
                            .clamp(i32::MIN as f32, i32::MAX as f32) as i32
                    })
                    .collect();
                wavers::write::<i32, _>(
                    export_path,
                    &quantized,
                    sample_rate,
                    output_channels as u16,
                )
            }
            ExportBitDepth::Float32 => wavers::write::<f32, _>(
                export_path,
                mixed_buffer,
                sample_rate,
                output_channels as u16,
            ),
        }
        .map_err(|e| {
            io::Error::other(format!(
                "Failed to write '{}': {}",
                export_path.display(),
                e
            ))
        })
    }

    fn quantize_samples_for_bit_depth(
        mixed_buffer: &[f32],
        bit_depth: ExportBitDepth,
    ) -> (Vec<i32>, u8) {
        let (scale, min, max, bps) = match bit_depth {
            ExportBitDepth::Int16 => (i16::MAX as f32, i16::MIN as f32, i16::MAX as f32, 16),
            ExportBitDepth::Int24 => (8_388_607.0_f32, -8_388_608.0_f32, 8_388_607.0_f32, 24),
            ExportBitDepth::Int32 => (i32::MAX as f32, i32::MIN as f32, i32::MAX as f32, 32),
            ExportBitDepth::Float32 => (8_388_607.0_f32, -8_388_608.0_f32, 8_388_607.0_f32, 24),
        };
        let samples = mixed_buffer
            .iter()
            .map(|s| (s.clamp(-1.0, 1.0) * scale).round().clamp(min, max) as i32)
            .collect();
        (samples, bps)
    }

    fn write_flac_with_bit_depth(
        export_path: &Path,
        mixed_buffer: &[f32],
        sample_rate: i32,
        output_channels: usize,
        bit_depth: ExportBitDepth,
    ) -> io::Result<()> {
        let (quantized, bits_per_sample) =
            Self::quantize_samples_for_bit_depth(mixed_buffer, bit_depth);
        let config = flacenc::config::Encoder::default()
            .into_verified()
            .map_err(|e| io::Error::other(format!("Invalid FLAC encoder config: {e:?}")))?;
        let source = flacenc::source::MemSource::from_samples(
            &quantized,
            output_channels,
            bits_per_sample as usize,
            sample_rate.max(1) as usize,
        );
        let stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size)
            .map_err(|e| io::Error::other(format!("FLAC encode failed: {e}")))?;
        let mut sink = flacenc::bitsink::ByteSink::new();
        stream
            .write(&mut sink)
            .map_err(|e| io::Error::other(format!("FLAC bitstream write failed: {e}")))?;
        fs::write(export_path, sink.as_slice()).map_err(|e| {
            io::Error::other(format!(
                "Failed to write '{}': {}",
                export_path.display(),
                e
            ))
        })
    }

    fn ffmpeg_init() -> Result<(), ffmpeg_next::Error> {
        static RESULT: std::sync::OnceLock<Result<(), ffmpeg_next::Error>> =
            std::sync::OnceLock::new();
        *RESULT.get_or_init(ffmpeg_next::init)
    }

    fn write_mp3(
        export_path: &Path,
        mixed_buffer: &[f32],
        sample_rate: i32,
        output_channels: usize,
        codec: ExportCodecSettings,
        metadata: &ExportMetadata,
    ) -> io::Result<()> {
        if output_channels != 1 && output_channels != 2 {
            return Err(io::Error::other(format!(
                "MP3 export supports only mono/stereo, got {} channels",
                output_channels
            )));
        }

        Self::ffmpeg_init().map_err(|e| io::Error::other(format!("FFmpeg init failed: {e}")))?;

        let mut octx = output(export_path.to_str().unwrap_or("output.mp3"))
            .map_err(|e| io::Error::other(format!("Failed to create output context: {e}")))?;

        let codec_id = CodecId::MP3;
        let encoder_codec = ffmpeg_next::codec::encoder::find(codec_id)
            .ok_or_else(|| io::Error::other("MP3 encoder not found"))?;

        let mut encoder = CodecContext::new_with_codec(encoder_codec)
            .encoder()
            .audio()
            .map_err(|e| io::Error::other(format!("Failed to create audio encoder: {e}")))?;

        encoder.set_rate(sample_rate);

        encoder.set_format(ffmpeg_next::format::Sample::F32(
            ffmpeg_next::format::sample::Type::Planar,
        ));
        encoder.set_channel_layout(match output_channels {
            1 => ffmpeg_next::channel_layout::ChannelLayout::MONO,
            _ => ffmpeg_next::channel_layout::ChannelLayout::STEREO,
        });

        let bitrate = (codec.mp3_bitrate_kbps as usize) * 1000;
        encoder.set_bit_rate(bitrate);

        if matches!(codec.mp3_mode, ExportMp3Mode::Vbr) {
            encoder.set_quality(2usize);
        }

        let mut metadata_dict = Dictionary::new();
        if !metadata.author.is_empty() {
            metadata_dict.set("artist", &metadata.author);
        }
        if !metadata.album.is_empty() {
            metadata_dict.set("album", &metadata.album);
        }
        if let Some(year) = metadata.year {
            metadata_dict.set("date", &year.to_string());
        }
        if let Some(track_number) = metadata.track_number {
            metadata_dict.set("track", &track_number.to_string());
        }
        if !metadata.genre.is_empty() {
            metadata_dict.set("genre", &metadata.genre);
        }

        let mut output_stream = octx
            .add_stream(encoder_codec)
            .map_err(|e| io::Error::other(format!("Failed to add stream: {e}")))?;

        // Set parameters from encoder before opening
        output_stream.set_parameters(&encoder);

        let mut encoder = encoder
            .open_as(encoder_codec)
            .map_err(|e| io::Error::other(format!("Failed to open encoder: {e}")))?;

        octx.write_header()
            .map_err(|e| io::Error::other(format!("Failed to write header: {e}")))?;

        let frame_size = 1152;

        for chunk_start in (0..mixed_buffer.len()).step_by(frame_size * output_channels) {
            let chunk_end = (chunk_start + frame_size * output_channels).min(mixed_buffer.len());
            let chunk = &mixed_buffer[chunk_start..chunk_end];
            let actual_frames = chunk.len() / output_channels;

            if actual_frames == 0 {
                continue;
            }

            let mut frame = Audio::empty();
            frame.set_format(encoder.format());
            frame.set_channel_layout(encoder.channel_layout());
            frame.set_rate(encoder.rate());
            frame.set_samples(actual_frames);

            unsafe {
                ffmpeg_next::ffi::av_frame_get_buffer(frame.as_mut_ptr(), 0);
            }

            for ch in 0..output_channels {
                let data_ptr = frame.data_mut(ch).as_mut_ptr() as *mut f32;
                for frame_idx in 0..actual_frames {
                    let src_idx = frame_idx * output_channels + ch;
                    if src_idx < chunk.len() {
                        unsafe {
                            *data_ptr.add(frame_idx) = chunk[src_idx];
                        }
                    }
                }
            }

            encoder
                .send_frame(&frame)
                .map_err(|e| io::Error::other(format!("Failed to send frame: {e}")))?;

            let mut packet = ffmpeg_next::packet::Packet::empty();
            while encoder.receive_packet(&mut packet).is_ok() {
                packet.set_stream(0);
                packet
                    .write_interleaved(&mut octx)
                    .map_err(|e| io::Error::other(format!("Failed to write packet: {e}")))?;
            }
        }

        encoder
            .send_eof()
            .map_err(|e| io::Error::other(format!("Failed to send EOF: {e}")))?;

        let mut packet = ffmpeg_next::packet::Packet::empty();
        while encoder.receive_packet(&mut packet).is_ok() {
            packet.set_stream(0);
            packet
                .write_interleaved(&mut octx)
                .map_err(|e| io::Error::other(format!("Failed to write packet: {e}")))?;
        }

        octx.write_trailer()
            .map_err(|e| io::Error::other(format!("Failed to write trailer: {e}")))?;

        Ok(())
    }

    fn write_ogg_vorbis(
        export_path: &Path,
        mixed_buffer: &[f32],
        sample_rate: i32,
        output_channels: usize,
        codec: ExportCodecSettings,
        metadata: &ExportMetadata,
    ) -> io::Result<()> {
        Self::ffmpeg_init().map_err(|e| io::Error::other(format!("FFmpeg init failed: {e}")))?;

        let mut octx = output(export_path.to_str().unwrap_or("output.ogg"))
            .map_err(|e| io::Error::other(format!("Failed to create output context: {e}")))?;

        let codec_id = CodecId::VORBIS;
        let encoder_codec = ffmpeg_next::codec::encoder::find(codec_id)
            .ok_or_else(|| io::Error::other("Vorbis encoder not found"))?;

        let mut encoder = CodecContext::new_with_codec(encoder_codec)
            .encoder()
            .audio()
            .map_err(|e| io::Error::other(format!("Failed to create audio encoder: {e}")))?;

        encoder.set_rate(sample_rate);

        encoder.set_format(ffmpeg_next::format::Sample::F32(
            ffmpeg_next::format::sample::Type::Planar,
        ));
        encoder.set_channel_layout(match output_channels {
            1 => ffmpeg_next::channel_layout::ChannelLayout::MONO,
            _ => ffmpeg_next::channel_layout::ChannelLayout::STEREO,
        });

        let quality = ((codec.ogg_quality + 0.1) * 10.0).clamp(0.0, 10.0) as i32;
        encoder.set_quality(quality as usize);

        let mut metadata_dict = Dictionary::new();
        if !metadata.author.is_empty() {
            metadata_dict.set("artist", &metadata.author);
        }
        if !metadata.album.is_empty() {
            metadata_dict.set("album", &metadata.album);
        }
        if let Some(year) = metadata.year {
            metadata_dict.set("date", &year.to_string());
        }
        if let Some(track_number) = metadata.track_number {
            metadata_dict.set("track", &track_number.to_string());
        }
        if !metadata.genre.is_empty() {
            metadata_dict.set("genre", &metadata.genre);
        }

        let mut output_stream = octx
            .add_stream(encoder_codec)
            .map_err(|e| io::Error::other(format!("Failed to add stream: {e}")))?;

        // Set parameters from encoder before opening
        output_stream.set_parameters(&encoder);

        let mut encoder = encoder
            .open_as(encoder_codec)
            .map_err(|e| io::Error::other(format!("Failed to open encoder: {e}")))?;

        octx.write_header()
            .map_err(|e| io::Error::other(format!("Failed to write header: {e}")))?;

        let frame_size = 1024;

        for chunk_start in (0..mixed_buffer.len()).step_by(frame_size * output_channels) {
            let chunk_end = (chunk_start + frame_size * output_channels).min(mixed_buffer.len());
            let chunk = &mixed_buffer[chunk_start..chunk_end];
            let actual_frames = chunk.len() / output_channels;

            if actual_frames == 0 {
                continue;
            }

            let mut frame = Audio::empty();
            frame.set_format(encoder.format());
            frame.set_channel_layout(encoder.channel_layout());
            frame.set_rate(encoder.rate());
            frame.set_samples(actual_frames);

            unsafe {
                ffmpeg_next::ffi::av_frame_get_buffer(frame.as_mut_ptr(), 0);
            }

            for ch in 0..output_channels {
                let data_ptr = frame.data_mut(ch).as_mut_ptr() as *mut f32;
                for frame_idx in 0..actual_frames {
                    let src_idx = frame_idx * output_channels + ch;
                    if src_idx < chunk.len() {
                        unsafe {
                            *data_ptr.add(frame_idx) = chunk[src_idx];
                        }
                    }
                }
            }

            encoder
                .send_frame(&frame)
                .map_err(|e| io::Error::other(format!("Failed to send frame: {e}")))?;

            let mut packet = ffmpeg_next::packet::Packet::empty();
            while encoder.receive_packet(&mut packet).is_ok() {
                packet.set_stream(0);
                packet
                    .write_interleaved(&mut octx)
                    .map_err(|e| io::Error::other(format!("Failed to write packet: {e}")))?;
            }
        }

        encoder
            .send_eof()
            .map_err(|e| io::Error::other(format!("Failed to send EOF: {e}")))?;

        let mut packet = ffmpeg_next::packet::Packet::empty();
        while encoder.receive_packet(&mut packet).is_ok() {
            packet.set_stream(0);
            packet
                .write_interleaved(&mut octx)
                .map_err(|e| io::Error::other(format!("Failed to write packet: {e}")))?;
        }

        octx.write_trailer()
            .map_err(|e| io::Error::other(format!("Failed to write trailer: {e}")))?;

        Ok(())
    }

    fn write_export_audio(req: ExportWriteRequest<'_>) -> io::Result<()> {
        match req.format {
            ExportFormat::Wav => Self::write_wav_with_bit_depth(
                req.export_path,
                req.mixed_buffer,
                req.sample_rate,
                req.output_channels,
                req.bit_depth,
            ),
            ExportFormat::Flac => Self::write_flac_with_bit_depth(
                req.export_path,
                req.mixed_buffer,
                req.sample_rate,
                req.output_channels,
                req.bit_depth,
            ),
            ExportFormat::Mp3 => Self::write_mp3(
                req.export_path,
                req.mixed_buffer,
                req.sample_rate,
                req.output_channels,
                req.codec,
                req.metadata,
            ),
            ExportFormat::Ogg => Self::write_ogg_vorbis(
                req.export_path,
                req.mixed_buffer,
                req.sample_rate,
                req.output_channels,
                req.codec,
                req.metadata,
            ),
        }
    }

    fn measure_lufs_and_true_peak(
        samples: &[f32],
        channels: usize,
        sample_rate: i32,
    ) -> io::Result<(f32, f32)> {
        let mut meter = EbuR128::new(
            channels as u32,
            sample_rate as u32,
            LoudnessMode::I | LoudnessMode::TRUE_PEAK,
        )
        .map_err(|e| io::Error::other(format!("Failed to initialize loudness meter: {e}")))?;
        meter
            .add_frames_f32(samples)
            .map_err(|e| io::Error::other(format!("Loudness analysis failed: {e}")))?;
        let lufs = meter
            .loudness_global()
            .map_err(|e| io::Error::other(format!("Failed to get integrated loudness: {e}")))?
            as f32;
        if !lufs.is_finite() {
            Err(io::Error::other("Integrated loudness is not finite"))
        } else {
            let mut tp = 0.0_f32;
            for ch in 0..channels as u32 {
                let p = meter
                    .true_peak(ch)
                    .map_err(|e| io::Error::other(format!("Failed to get true peak: {e}")))?
                    as f32;
                tp = tp.max(p);
            }
            Ok((lufs, tp))
        }
    }

    fn apply_export_normalization(
        mixed_buffer: &mut [f32],
        params: ExportNormalizeParams,
    ) -> io::Result<()> {
        match params.mode {
            ExportNormalizeMode::Peak => {
                let peak = mixed_buffer
                    .iter()
                    .fold(0.0_f32, |acc, sample| acc.max(sample.abs()));
                if peak > 0.0 {
                    let target_amp = 10.0_f32.powf(params.target_dbfs / 20.0).clamp(0.0, 1.0);
                    let gain = target_amp / peak;
                    for sample in mixed_buffer {
                        *sample *= gain;
                    }
                }
            }
            ExportNormalizeMode::Loudness => {
                let (measured_lufs, measured_tp_amp) = Self::measure_lufs_and_true_peak(
                    mixed_buffer,
                    params.output_channels,
                    params.sample_rate,
                )?;
                let gain_loudness_db = params.target_lufs - measured_lufs;
                let gain_loudness = 10.0_f32.powf(gain_loudness_db / 20.0);

                let ceiling_amp = 10.0_f32.powf(params.true_peak_dbtp / 20.0).clamp(0.0, 1.0);
                let gain_tp = if measured_tp_amp > 0.0 {
                    ceiling_amp / measured_tp_amp
                } else {
                    gain_loudness
                };

                let applied_gain = if params.tp_limiter {
                    gain_loudness
                } else {
                    gain_loudness.min(gain_tp)
                };

                for sample in mixed_buffer.iter_mut() {
                    *sample *= applied_gain;
                }

                if params.tp_limiter {
                    let predicted_tp = measured_tp_amp * applied_gain;
                    if predicted_tp > ceiling_amp && ceiling_amp > 0.0 {
                        for sample in mixed_buffer.iter_mut() {
                            *sample = sample.clamp(-ceiling_amp, ceiling_amp);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn available_export_hw_out_ports(&self) -> Vec<usize> {
        let channels = self
            .state
            .blocking_read()
            .hw_out
            .as_ref()
            .map(|hw| hw.channels)
            .unwrap_or(0);
        (0..channels).collect()
    }

    fn default_export_hw_out_ports(&self) -> BTreeSet<usize> {
        self.available_export_hw_out_ports()
            .into_iter()
            .take(2)
            .collect()
    }

    fn normalize_export_hw_out_ports(&mut self) {
        let available: BTreeSet<usize> = self.available_export_hw_out_ports().into_iter().collect();
        self.export_hw_out_ports
            .retain(|port| available.contains(port));
        if self.export_hw_out_ports.is_empty() {
            self.export_hw_out_ports = self.default_export_hw_out_ports();
        }
    }

    fn export_max_channels_for_current_settings(&self) -> usize {
        if matches!(self.export_render_mode, ExportRenderMode::Mixdown) {
            return self.export_hw_out_ports.len();
        }

        let state = self.state.blocking_read();
        state
            .tracks
            .iter()
            .filter(|track| state.selected.contains(&track.name))
            .map(|track| track.audio.outs.max(1))
            .max()
            .unwrap_or(0)
    }

    fn export_mp3_supported_for_current_settings(&self) -> bool {
        self.export_max_channels_for_current_settings() <= 2
    }

    fn mix_track_clips_to_channels(
        clips: &[crate::state::AudioClip],
        session_root: &Path,
        total_length: usize,
        output_channels: usize,
        level_db: f32,
        balance: f32,
        apply_fader: bool,
    ) -> io::Result<Vec<f32>> {
        let output_channels = output_channels.max(1);
        let mut mixed = vec![0.0_f32; total_length * output_channels];
        let channel_gains = if apply_fader {
            let level_amp = 10.0_f32.powf(level_db / 20.0);
            if output_channels == 2 {
                vec![
                    if balance <= 0.0 {
                        level_amp
                    } else {
                        level_amp * (1.0 - balance)
                    },
                    if balance >= 0.0 {
                        level_amp
                    } else {
                        level_amp * (1.0 + balance)
                    },
                ]
            } else {
                vec![level_amp; output_channels]
            }
        } else {
            vec![1.0; output_channels]
        };

        for clip in clips {
            let clip_path = if std::path::PathBuf::from(&clip.name).is_absolute() {
                std::path::PathBuf::from(&clip.name)
            } else {
                session_root.join(&clip.name)
            };
            let mut wav = Wav::<f32>::from_path(&clip_path).map_err(|e| {
                io::Error::other(format!(
                    "Failed to open WAV '{}': {}",
                    clip_path.display(),
                    e
                ))
            })?;
            let clip_channels = wav.n_channels().max(1) as usize;
            let samples: wavers::Samples<f32> = wav.read().map_err(|e| {
                io::Error::other(format!("WAV read error '{}': {}", clip_path.display(), e))
            })?;
            if samples.is_empty() {
                continue;
            }
            let clip_frames = samples.len() / clip_channels;
            let start_frame = clip.start;
            let offset_frame = clip.offset;
            let length_frames = clip.length.min(clip_frames.saturating_sub(offset_frame));

            for frame_idx in 0..length_frames {
                let src_frame = offset_frame + frame_idx;
                let dst_frame = start_frame + frame_idx;
                if dst_frame >= total_length {
                    break;
                }
                let src_idx = src_frame * clip_channels;
                if src_idx + clip_channels > samples.len() {
                    break;
                }
                let dst_idx = dst_frame * output_channels;
                for out_ch in 0..output_channels {
                    let source_sample = if clip_channels == 1 {
                        samples[src_idx]
                    } else {
                        let source_ch = out_ch.min(clip_channels.saturating_sub(1));
                        samples[src_idx + source_ch]
                    };
                    mixed[dst_idx + out_ch] += source_sample * channel_gains[out_ch];
                }
            }
        }

        Ok(mixed)
    }

    async fn export_session<F>(
        options: &ExportSessionOptions,
        mut progress_callback: F,
    ) -> std::io::Result<()>
    where
        F: FnMut(f32, Option<String>),
    {
        let export_path = options.export_path.as_path();
        let sample_rate = options.sample_rate;
        let export_formats = options.formats.clone();
        let bit_depth = options.bit_depth;
        let mp3_mode = options.mp3_mode;
        let mp3_bitrate_kbps = options.mp3_bitrate_kbps;
        let ogg_quality = options.ogg_quality;
        let realtime_fallback = options.realtime_fallback;
        let normalize = options.normalize;
        let normalize_target_dbfs = options.normalize_target_dbfs;
        let normalize_mode = options.normalize_mode;
        let normalize_target_lufs = options.normalize_target_lufs;
        let normalize_true_peak_dbtp = options.normalize_true_peak_dbtp;
        let normalize_tp_limiter = options.normalize_tp_limiter;
        let master_limiter = options.master_limiter;
        let master_limiter_ceiling_dbtp = options.master_limiter_ceiling_dbtp;
        let metadata_author = options.metadata_author.clone();
        let metadata_album = options.metadata_album.clone();
        let metadata_year = options.metadata_year;
        let metadata_track_number = options.metadata_track_number;
        let metadata_genre = options.metadata_genre.clone();
        let codec = ExportCodecSettings {
            mp3_mode,
            mp3_bitrate_kbps,
            ogg_quality,
        };
        let metadata = ExportMetadata {
            author: metadata_author,
            album: metadata_album,
            year: metadata_year,
            track_number: metadata_track_number,
            genre: metadata_genre,
        };
        let render_mode = options.render_mode;
        let selected_hw_out_ports = options.selected_hw_out_ports.clone();
        let state = options.state.clone();
        let session_root = options.session_root.as_path();

        progress_callback(0.0, Some("Analyzing tracks".to_string()));
        tokio::task::yield_now().await;

        let (mut tracks, connections, total_length, selected_tracks) = {
            let state = state.read().await;
            let mut max_length = 0_usize;
            let tracks_data: Vec<_> = state
                .tracks
                .iter()
                .map(|track| {
                    let mut track_max = 0_usize;
                    for clip in &track.audio.clips {
                        let clip_end = clip.start + clip.length;
                        track_max = track_max.max(clip_end);
                    }
                    max_length = max_length.max(track_max);
                    ExportTrackData {
                        name: track.name.clone(),
                        level: track.level,
                        balance: track.balance,
                        muted: track.muted,
                        soloed: track.soloed,
                        output_ports: track.audio.outs.max(1),
                        clips: track.audio.clips.clone(),
                    }
                })
                .collect();
            (
                tracks_data,
                state.connections.clone(),
                max_length,
                state.selected.iter().cloned().collect::<HashSet<String>>(),
            )
        };

        let corrected_clip_count = tracks
            .iter()
            .flat_map(|track| track.clips.iter())
            .filter(|clip| !clip.pitch_correction_points.is_empty())
            .count();
        if corrected_clip_count > 0 {
            let mut corrected_done = 0usize;
            for track in &mut tracks {
                for clip in &mut track.clips {
                    if clip.pitch_correction_points.is_empty() {
                        continue;
                    }
                    let source_name = clip
                        .pitch_correction_source_name
                        .clone()
                        .unwrap_or_else(|| clip.name.clone());
                    let source_path = if Path::new(&source_name).is_absolute() {
                        PathBuf::from(&source_name)
                    } else {
                        session_root.join(&source_name)
                    };
                    let progress =
                        0.1 + (corrected_done as f32 / corrected_clip_count.max(1) as f32) * 0.15;
                    progress_callback(
                        progress,
                        Some(format!("Preparing offline pitch correction: {}", clip.name)),
                    );
                    tokio::task::yield_now().await;
                    let (rendered_name, rendered_length, _) =
                        Self::render_audio_clip_pitch_correction_with_rubberband(
                            &source_path,
                            session_root,
                            &clip.name,
                            clip.pitch_correction_source_offset.unwrap_or(clip.offset),
                            clip.pitch_correction_source_length.unwrap_or(clip.length),
                            &clip.pitch_correction_points,
                            clip.pitch_correction_inertia_ms.unwrap_or(100),
                            clip.pitch_correction_formant_compensation.unwrap_or(true),
                            |_, _| {},
                        )
                        .await?;
                    clip.name = rendered_name;
                    clip.offset = 0;
                    clip.length = rendered_length.max(1);
                    clip.pitch_correction_preview_name = None;
                    clip.pitch_correction_points.clear();
                    corrected_done = corrected_done.saturating_add(1);
                }
            }
        }

        if total_length == 0 {
            return Err(io::Error::other(
                "No audio clips found. Nothing to export.".to_string(),
            ));
        }
        if export_formats.is_empty() {
            return Err(io::Error::other("Select at least one export format"));
        }

        let has_solo = tracks.iter().any(|track| track.soloed);
        if !matches!(render_mode, ExportRenderMode::Mixdown) && selected_tracks.is_empty() {
            return Err(io::Error::other(
                "Stem export requires at least one selected track",
            ));
        }
        progress_callback(0.1, Some("Loading and mixing tracks".to_string()));
        tokio::task::yield_now().await;
        if matches!(render_mode, ExportRenderMode::Mixdown) {
            let output_ports = if selected_hw_out_ports.is_empty() {
                let mut routed_ports: Vec<usize> = connections
                    .iter()
                    .filter(|conn| conn.kind == Kind::Audio && conn.to_track == "hw:out")
                    .map(|conn| conn.to_port)
                    .collect();
                routed_ports.sort_unstable();
                routed_ports.dedup();
                routed_ports
            } else {
                selected_hw_out_ports
            };
            if output_ports.is_empty() {
                return Err(io::Error::other(
                    "Mixdown export requires at least one selected hw:out port",
                ));
            }
            let output_channels = output_ports.len();
            let normalize_params = ExportNormalizeParams {
                mode: normalize_mode,
                target_dbfs: normalize_target_dbfs,
                target_lufs: normalize_target_lufs,
                true_peak_dbtp: normalize_true_peak_dbtp,
                tp_limiter: normalize_tp_limiter,
                sample_rate,
                output_channels,
            };
            let hw_out_channel_map: HashMap<usize, usize> = output_ports
                .iter()
                .enumerate()
                .map(|(channel_idx, port)| (*port, channel_idx))
                .collect();
            let mut mixed_buffer = vec![0.0_f32; total_length * output_channels];
            for (track_idx, track) in tracks.iter().enumerate() {
                if track.muted || (has_solo && !track.soloed) {
                    continue;
                }
                let track_progress_start = 0.1 + (track_idx as f32 / tracks.len() as f32) * 0.7;
                let track_progress_span = 0.7 / tracks.len() as f32;
                progress_callback(
                    track_progress_start,
                    Some(format!("Processing track: {}", track.name)),
                );
                tokio::task::yield_now().await;
                let routed_ports: Vec<(usize, usize)> = connections
                    .iter()
                    .filter(|conn| {
                        conn.kind == Kind::Audio
                            && conn.from_track == track.name
                            && conn.to_track == "hw:out"
                    })
                    .filter_map(|conn| {
                        hw_out_channel_map
                            .get(&conn.to_port)
                            .map(|dest_idx| (conn.from_port, *dest_idx))
                    })
                    .collect();
                if routed_ports.is_empty() {
                    continue;
                }
                let track_buffer = Self::mix_track_clips_to_channels(
                    &track.clips,
                    session_root,
                    total_length,
                    track.output_ports,
                    track.level,
                    track.balance,
                    true,
                )?;
                for frame in 0..total_length {
                    let track_base = frame * track.output_ports;
                    let mixed_base = frame * output_channels;
                    for (source_port, dest_channel) in &routed_ports {
                        if *source_port >= track.output_ports {
                            continue;
                        }
                        mixed_buffer[mixed_base + *dest_channel] +=
                            track_buffer[track_base + *source_port];
                    }
                }
                progress_callback(
                    track_progress_start + track_progress_span,
                    Some(format!("Finished: {}", track.name)),
                );
                tokio::task::yield_now().await;
            }
            if realtime_fallback {
                progress_callback(0.82, Some("Real-time fallback pacing".to_string()));
                let seconds = (total_length as f64 / sample_rate.max(1) as f64).max(0.0);
                tokio::time::sleep(Duration::from_secs_f64(seconds)).await;
            }

            if normalize {
                Self::apply_export_normalization(&mut mixed_buffer, normalize_params)?;
            }
            if master_limiter {
                let ceiling_amp = 10.0_f32
                    .powf(master_limiter_ceiling_dbtp / 20.0)
                    .clamp(0.0, 1.0);
                for sample in &mut mixed_buffer {
                    *sample = sample.clamp(-ceiling_amp, ceiling_amp);
                }
            }
            let base_path = Self::export_base_path(export_path.to_path_buf());
            let write_span = 0.1 / export_formats.len().max(1) as f32;
            for (format_idx, format) in export_formats.iter().enumerate() {
                let write_progress = 0.9 + write_span * format_idx as f32;
                progress_callback(
                    write_progress.clamp(0.0, 0.99),
                    Some(format!("Writing {} ({bit_depth})", format)),
                );
                tokio::task::yield_now().await;
                let out_path = base_path.with_extension(Self::export_format_extension(*format));
                Self::write_export_audio(ExportWriteRequest {
                    export_path: &out_path,
                    mixed_buffer: &mixed_buffer,
                    sample_rate,
                    output_channels,
                    bit_depth,
                    format: *format,
                    codec,
                    metadata: &metadata,
                })?;
            }
            progress_callback(1.0, Some("Complete".to_string()));
            return Ok(());
        }

        let stem_mode_label = if matches!(render_mode, ExportRenderMode::StemsPreFader) {
            "pre"
        } else {
            "post"
        };
        let export_parent = export_path.parent().unwrap_or_else(|| Path::new("."));
        let export_stem = export_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("export");
        let stem_dir = export_parent.join(format!("{export_stem}_stems"));
        fs::create_dir_all(&stem_dir)?;

        let selected_tracks: Vec<_> = tracks
            .iter()
            .filter(|track| {
                selected_tracks.contains(&track.name) && !track.muted && (!has_solo || track.soloed)
            })
            .collect();
        if selected_tracks.is_empty() {
            return Err(io::Error::other(
                "No selected tracks are eligible for stem export",
            ));
        }

        for (idx, track) in selected_tracks.iter().enumerate() {
            let start = 0.1 + (idx as f32 / selected_tracks.len() as f32) * 0.75;
            progress_callback(start, Some(format!("Rendering stem: {}", track.name)));
            tokio::task::yield_now().await;
            let output_channels = track.output_ports.max(1);
            let normalize_params = ExportNormalizeParams {
                mode: normalize_mode,
                target_dbfs: normalize_target_dbfs,
                target_lufs: normalize_target_lufs,
                true_peak_dbtp: normalize_true_peak_dbtp,
                tp_limiter: normalize_tp_limiter,
                sample_rate,
                output_channels,
            };
            let mut stem_buffer = Self::mix_track_clips_to_channels(
                &track.clips,
                session_root,
                total_length,
                output_channels,
                track.level,
                track.balance,
                matches!(render_mode, ExportRenderMode::StemsPostFader),
            )?;
            if normalize {
                Self::apply_export_normalization(&mut stem_buffer, normalize_params)?;
            }
            if master_limiter {
                let ceiling_amp = 10.0_f32
                    .powf(master_limiter_ceiling_dbtp / 20.0)
                    .clamp(0.0, 1.0);
                for sample in &mut stem_buffer {
                    *sample = sample.clamp(-ceiling_amp, ceiling_amp);
                }
            }
            for format in &export_formats {
                let stem_file = stem_dir.join(format!(
                    "{}_{}.{}",
                    Self::sanitize_export_component(&track.name),
                    stem_mode_label,
                    Self::export_format_extension(*format)
                ));
                Self::write_export_audio(ExportWriteRequest {
                    export_path: &stem_file,
                    mixed_buffer: &stem_buffer,
                    sample_rate,
                    output_channels,
                    bit_depth,
                    format: *format,
                    codec,
                    metadata: &metadata,
                })?;
            }
            if realtime_fallback {
                let seconds = (total_length as f64 / sample_rate.max(1) as f64).max(0.0);
                tokio::time::sleep(Duration::from_secs_f64(seconds)).await;
            }
        }
        progress_callback(
            1.0,
            Some(format!(
                "Complete (stems written to {})",
                stem_dir.display()
            )),
        );
        Ok(())
    }

    fn midi_length_in_samples(path: &Path, sample_rate: f64) -> std::io::Result<usize> {
        let bytes = fs::read(path)?;
        let smf = Smf::parse(&bytes).map_err(|e| {
            io::Error::other(format!("Failed to parse MIDI '{}': {e}", path.display()))
        })?;
        let Timing::Metrical(ppq) = smf.header.timing else {
            return Ok(sample_rate.max(1.0) as usize);
        };
        let ppq = u64::from(ppq.as_int().max(1));
        let mut tempo_changes: Vec<(u64, u32)> = vec![(0, 500_000)];
        let mut max_tick = 0_u64;
        for track in &smf.tracks {
            let mut tick = 0_u64;
            for event in track {
                tick = tick.saturating_add(event.delta.as_int() as u64);
                max_tick = max_tick.max(tick);
                if let TrackEventKind::Meta(MetaMessage::Tempo(us_per_q)) = event.kind {
                    tempo_changes.push((tick, us_per_q.as_int()));
                }
            }
        }
        tempo_changes.sort_by_key(|(tick, _)| *tick);
        let mut normalized_tempos: Vec<(u64, u32)> = Vec::with_capacity(tempo_changes.len());
        for (tick, tempo) in tempo_changes {
            if let Some(last) = normalized_tempos.last_mut()
                && last.0 == tick
            {
                last.1 = tempo;
            } else {
                normalized_tempos.push((tick, tempo));
            }
        }
        let ticks_to_samples = |tick: u64| -> usize {
            let mut total_us: u128 = 0;
            let mut prev_tick = 0_u64;
            let mut current_tempo_us = 500_000_u32;
            for (change_tick, tempo_us) in &normalized_tempos {
                if *change_tick > tick {
                    break;
                }
                let span = change_tick.saturating_sub(prev_tick);
                total_us = total_us.saturating_add(
                    u128::from(span).saturating_mul(u128::from(current_tempo_us)) / u128::from(ppq),
                );
                prev_tick = *change_tick;
                current_tempo_us = *tempo_us;
            }
            let rem = tick.saturating_sub(prev_tick);
            total_us = total_us.saturating_add(
                u128::from(rem).saturating_mul(u128::from(current_tempo_us)) / u128::from(ppq),
            );
            ((total_us as f64 / 1_000_000.0) * sample_rate).round() as usize
        };
        Ok(ticks_to_samples(max_tick).max(1))
    }

    fn midi_ticks_to_samples(smf: &Smf<'_>, sample_rate: f64) -> Option<MidiTickMap> {
        let Timing::Metrical(ppq) = smf.header.timing else {
            return None;
        };
        let ppq = u64::from(ppq.as_int().max(1));
        let mut tempo_changes: Vec<(u64, u32)> = vec![(0, 500_000)];
        let mut max_tick = 0_u64;
        for track in &smf.tracks {
            let mut tick = 0_u64;
            for event in track {
                tick = tick.saturating_add(event.delta.as_int() as u64);
                max_tick = max_tick.max(tick);
                if let TrackEventKind::Meta(MetaMessage::Tempo(us_per_q)) = event.kind {
                    tempo_changes.push((tick, us_per_q.as_int()));
                }
            }
        }
        tempo_changes.sort_by_key(|(tick, _)| *tick);
        let mut normalized_tempos: Vec<(u64, u32)> = Vec::with_capacity(tempo_changes.len());
        for (tick, tempo) in tempo_changes {
            if let Some(last) = normalized_tempos.last_mut()
                && last.0 == tick
            {
                last.1 = tempo;
            } else {
                normalized_tempos.push((tick, tempo));
            }
        }
        let normalized_tempos = Arc::new(normalized_tempos);
        let mapper = {
            let normalized_tempos = normalized_tempos.clone();
            move |tick: u64| -> usize {
                let mut total_us: u128 = 0;
                let mut prev_tick = 0_u64;
                let mut current_tempo_us = 500_000_u32;
                for (change_tick, tempo_us) in normalized_tempos.iter() {
                    if *change_tick > tick {
                        break;
                    }
                    let span = change_tick.saturating_sub(prev_tick);
                    total_us = total_us.saturating_add(
                        u128::from(span).saturating_mul(u128::from(current_tempo_us))
                            / u128::from(ppq),
                    );
                    prev_tick = *change_tick;
                    current_tempo_us = *tempo_us;
                }
                let rem = tick.saturating_sub(prev_tick);
                total_us = total_us.saturating_add(
                    u128::from(rem).saturating_mul(u128::from(current_tempo_us)) / u128::from(ppq),
                );
                ((total_us as f64 / 1_000_000.0) * sample_rate).round() as usize
            }
        };
        Some((Box::new(mapper), ppq, max_tick))
    }

    fn parse_midi_clip_for_piano(
        path: &Path,
        sample_rate: f64,
        clip_start: usize,
    ) -> std::io::Result<PianoParseResult> {
        let bytes = fs::read(path)?;
        let smf = Smf::parse(&bytes).map_err(|e| {
            io::Error::other(format!("Failed to parse MIDI '{}': {e}", path.display()))
        })?;
        let Some((ticks_to_samples, ppq, max_tick)) =
            Self::midi_ticks_to_samples(&smf, sample_rate)
        else {
            return Ok((vec![], vec![], vec![], sample_rate.max(1.0) as usize));
        };

        let mut notes = Vec::<PianoNote>::new();
        let mut controllers = Vec::<PianoControllerPoint>::new();
        let mut sysexes = Vec::<PianoSysExPoint>::new();
        let mut active_notes: HashMap<(u8, u8), Vec<(u64, u8)>> = HashMap::new();

        for track in &smf.tracks {
            let mut tick = 0_u64;
            for event in track {
                tick = tick.saturating_add(event.delta.as_int() as u64);
                match event.kind {
                    TrackEventKind::Midi { channel, message } => {
                        let channel = channel.as_int();
                        match message {
                            midly::MidiMessage::NoteOn { key, vel } => {
                                let pitch = key.as_int();
                                let velocity = vel.as_int();
                                if velocity == 0 {
                                    if let Some(starts) = active_notes.get_mut(&(channel, pitch))
                                        && let Some((start_tick, start_vel)) = starts.pop()
                                    {
                                        let start_sample = ticks_to_samples(start_tick);
                                        let end_sample = ticks_to_samples(tick);
                                        let length_samples =
                                            end_sample.saturating_sub(start_sample).max(1);
                                        notes.push(PianoNote {
                                            start_sample,
                                            length_samples,
                                            pitch,
                                            velocity: start_vel,
                                            channel,
                                        });
                                    }
                                } else {
                                    active_notes
                                        .entry((channel, pitch))
                                        .or_default()
                                        .push((tick, velocity));
                                }
                            }
                            midly::MidiMessage::NoteOff { key, .. } => {
                                let pitch = key.as_int();
                                if let Some(starts) = active_notes.get_mut(&(channel, pitch))
                                    && let Some((start_tick, start_vel)) = starts.pop()
                                {
                                    let start_sample = ticks_to_samples(start_tick);
                                    let end_sample = ticks_to_samples(tick);
                                    let length_samples =
                                        end_sample.saturating_sub(start_sample).max(1);
                                    notes.push(PianoNote {
                                        start_sample,
                                        length_samples,
                                        pitch,
                                        velocity: start_vel,
                                        channel,
                                    });
                                }
                            }
                            midly::MidiMessage::Controller { controller, value } => {
                                controllers.push(PianoControllerPoint {
                                    sample: ticks_to_samples(tick),
                                    controller: controller.as_int(),
                                    value: value.as_int(),
                                    channel,
                                });
                            }
                            _ => {}
                        }
                    }
                    TrackEventKind::SysEx(payload) => {
                        let mut data = Vec::with_capacity(payload.len() + 2);
                        data.push(0xF0);
                        data.extend_from_slice(payload);
                        if data.last().copied() != Some(0xF7) {
                            data.push(0xF7);
                        }
                        sysexes.push(PianoSysExPoint {
                            sample: ticks_to_samples(tick),
                            data,
                        });
                    }
                    TrackEventKind::Escape(payload) => {
                        let mut data = Vec::with_capacity(payload.len() + 1);
                        data.push(0xF7);
                        data.extend_from_slice(payload);
                        sysexes.push(PianoSysExPoint {
                            sample: ticks_to_samples(tick),
                            data,
                        });
                    }
                    _ => {}
                }
            }
        }

        for ((channel, pitch), starts) in active_notes {
            for (start_tick, velocity) in starts {
                let start_sample = ticks_to_samples(start_tick);
                let end_sample = ticks_to_samples(start_tick.saturating_add(ppq / 8));
                let length_samples = end_sample.saturating_sub(start_sample).max(1);
                notes.push(PianoNote {
                    start_sample,
                    length_samples,
                    pitch,
                    velocity,
                    channel,
                });
            }
        }

        notes.sort_by_key(|n| (n.start_sample, n.pitch));
        controllers.sort_by_key(|c| (c.sample, c.controller));
        sysexes.sort_by_key(|s| s.sample);
        let mut clip_len = notes
            .iter()
            .map(|n| n.start_sample.saturating_add(n.length_samples))
            .chain(controllers.iter().map(|c| c.sample))
            .chain(sysexes.iter().map(|s| s.sample))
            .max()
            .unwrap_or_else(|| ticks_to_samples(max_tick))
            .max(1);

        // Normalize absolute timeline timestamps to clip-relative coordinates.
        // Recorded clips used to write absolute samples; imported files are
        // already relative (start at 0).  We detect absolute files by checking
        // whether every event is at or after clip_start.
        let min_sample = notes
            .iter()
            .map(|n| n.start_sample)
            .chain(controllers.iter().map(|c| c.sample))
            .chain(sysexes.iter().map(|s| s.sample))
            .min()
            .unwrap_or(0);
        if min_sample >= clip_start && clip_start > 0 {
            for note in &mut notes {
                note.start_sample = note.start_sample.saturating_sub(clip_start);
            }
            for ctrl in &mut controllers {
                ctrl.sample = ctrl.sample.saturating_sub(clip_start);
            }
            for sysex in &mut sysexes {
                sysex.sample = sysex.sample.saturating_sub(clip_start);
            }
            clip_len = clip_len.saturating_sub(clip_start).max(1);
        }

        Ok((notes, controllers, sysexes, clip_len))
    }

    fn import_midi_to_session(
        src_path: &Path,
        session_root: &Path,
        sample_rate: f64,
    ) -> std::io::Result<(String, usize)> {
        let stem = src_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("midi");
        let ext = Self::file_extension_lower(src_path).unwrap_or_else(|| "mid".to_string());
        let rel = Self::unique_import_rel_path(session_root, "midi", stem, &ext)?;
        let dst = session_root.join(&rel);
        fs::create_dir_all(session_root.join("midi"))?;
        fs::copy(src_path, &dst).map_err(|e| {
            io::Error::other(format!(
                "Failed to copy MIDI '{}' to '{}': {e}",
                src_path.display(),
                dst.display()
            ))
        })?;
        let length = Self::midi_length_in_samples(&dst, sample_rate)?;
        Ok((rel, length.max(1)))
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn plugin_node_to_json(
        node: &maolan_engine::message::PluginGraphNode,
        id_to_index: &std::collections::HashMap<usize, usize>,
    ) -> Option<Value> {
        use maolan_engine::message::PluginGraphNode;
        match node {
            PluginGraphNode::TrackInput => Some(json!({"type":"track_input"})),
            PluginGraphNode::TrackOutput => Some(json!({"type":"track_output"})),
            PluginGraphNode::Lv2PluginInstance(id) => id_to_index
                .get(id)
                .copied()
                .map(|idx| json!({"type":"plugin","plugin_index":idx})),
            PluginGraphNode::Vst3PluginInstance(id) => id_to_index
                .get(id)
                .copied()
                .map(|idx| json!({"type":"vst3_plugin","plugin_index":idx})),
            PluginGraphNode::ClapPluginInstance(id) => id_to_index
                .get(id)
                .copied()
                .map(|idx| json!({"type":"clap_plugin","plugin_index":idx})),
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn plugin_node_from_json_with_runtime_nodes(
        v: &Value,
        runtime_nodes: &[maolan_engine::message::PluginGraphNode],
    ) -> Option<maolan_engine::message::PluginGraphNode> {
        use maolan_engine::message::PluginGraphNode;
        if let Some(node_name) = v.as_str() {
            return match node_name {
                "TrackInput" => Some(PluginGraphNode::TrackInput),
                "TrackOutput" => Some(PluginGraphNode::TrackOutput),
                _ => None,
            };
        }

        let t = v["type"].as_str()?;
        match t {
            "track_input" => Some(PluginGraphNode::TrackInput),
            "track_output" => Some(PluginGraphNode::TrackOutput),
            "plugin" => runtime_nodes
                .get(v["plugin_index"].as_u64()? as usize)
                .and_then(|node| {
                    matches!(node, PluginGraphNode::Lv2PluginInstance(_)).then(|| node.clone())
                }),
            "vst3_plugin" => runtime_nodes
                .get(v["plugin_index"].as_u64()? as usize)
                .and_then(|node| {
                    matches!(node, PluginGraphNode::Vst3PluginInstance(_)).then(|| node.clone())
                }),
            "clap_plugin" => runtime_nodes
                .get(v["plugin_index"].as_u64()? as usize)
                .and_then(|node| {
                    matches!(node, PluginGraphNode::ClapPluginInstance(_)).then(|| node.clone())
                }),
            _ => None,
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn plugin_graph_snapshot_to_json(
        previous_graph: Option<&Value>,
        plugins: &[maolan_engine::message::PluginGraphPlugin],
        connections: &[maolan_engine::message::PluginGraphConnection],
    ) -> Value {
        let id_to_index: std::collections::HashMap<usize, usize> = plugins
            .iter()
            .enumerate()
            .map(|(idx, plugin)| (plugin.instance_id, idx))
            .collect();
        let previous_plugin_states = previous_graph
            .and_then(|graph| graph.get("plugins"))
            .and_then(Value::as_array)
            .map(|plugins| {
                plugins
                    .iter()
                    .filter_map(|plugin| {
                        Some((
                            (
                                plugin.get("format")?.as_str()?.to_string(),
                                plugin.get("uri")?.as_str()?.to_string(),
                            ),
                            plugin.get("state").cloned().unwrap_or(Value::Null),
                        ))
                    })
                    .collect::<std::collections::HashMap<(String, String), Value>>()
            })
            .unwrap_or_default();
        let plugins_json = plugins
            .iter()
            .map(|plugin| {
                let state_json = plugin.state.clone().unwrap_or_else(|| {
                    previous_plugin_states
                        .get(&(plugin.format.clone(), plugin.uri.clone()))
                        .cloned()
                        .unwrap_or(Value::Null)
                });
                json!({
                    "format": plugin.format,
                    "uri": plugin.uri,
                    "state": state_json,
                })
            })
            .collect::<Vec<_>>();
        let connections_json = connections
            .iter()
            .filter_map(|connection| {
                let from_node = Self::plugin_node_to_json(&connection.from_node, &id_to_index)?;
                let to_node = Self::plugin_node_to_json(&connection.to_node, &id_to_index)?;
                Some(json!({
                    "from_node": from_node,
                    "from_port": connection.from_port,
                    "to_node": to_node,
                    "to_port": connection.to_port,
                    "kind": Self::kind_to_json(connection.kind),
                }))
            })
            .collect::<Vec<_>>();
        json!({
            "plugins": plugins_json,
            "connections": connections_json,
        })
    }

    #[cfg(all(test, unix, not(target_os = "macos")))]
    fn plugin_graph_saved_state_from_json<T: serde::de::DeserializeOwned>(
        graph: Option<&Value>,
        plugin_index: usize,
    ) -> Option<T> {
        let state = graph
            .and_then(|graph| graph.get("plugins"))
            .and_then(Value::as_array)
            .and_then(|plugins| plugins.get(plugin_index))
            .and_then(|plugin| plugin.get("state"))
            .cloned()?;
        serde_json::from_value(state).ok()
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn plugin_graph_json_with_saved_plugin_state(
        graph: Option<&Value>,
        plugin_index: usize,
        state: Value,
    ) -> Option<Value> {
        let mut graph = graph?.clone();
        let plugins = graph.get_mut("plugins")?.as_array_mut()?;
        let plugin = plugins.get_mut(plugin_index)?;
        let plugin_object = plugin.as_object_mut()?;
        plugin_object.insert("state".to_string(), state);
        Some(graph)
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn plugin_graph_plugin_from_saved_json(
        instance_id: usize,
        plugin: &Value,
        lv2_plugins: &[maolan_engine::lv2::Lv2PluginInfo],
        vst3_plugins: &[maolan_engine::vst3::Vst3PluginInfo],
        clap_plugins: &[maolan_engine::clap::ClapPluginInfo],
    ) -> Option<maolan_engine::message::PluginGraphPlugin> {
        use maolan_engine::message::{PluginGraphNode, PluginGraphPlugin};

        let uri = plugin.get("uri").and_then(Value::as_str)?.to_string();
        match plugin.get("format").and_then(Value::as_str) {
            Some(format) if format.eq_ignore_ascii_case("LV2") => {
                let info = lv2_plugins.iter().find(|info| info.uri == uri);
                Some(PluginGraphPlugin {
                    node: PluginGraphNode::Lv2PluginInstance(instance_id),
                    instance_id,
                    format: "LV2".to_string(),
                    uri: uri.clone(),
                    plugin_id: uri.clone(),
                    name: info
                        .map(|info| info.name.clone())
                        .unwrap_or_else(|| uri.clone()),
                    main_audio_inputs: info.map(|info| info.audio_inputs).unwrap_or(0),
                    main_audio_outputs: info.map(|info| info.audio_outputs).unwrap_or(0),
                    audio_inputs: info.map(|info| info.audio_inputs).unwrap_or(0),
                    audio_outputs: info.map(|info| info.audio_outputs).unwrap_or(0),
                    midi_inputs: info.map(|info| info.midi_inputs).unwrap_or(0),
                    midi_outputs: info.map(|info| info.midi_outputs).unwrap_or(0),
                    state: plugin.get("state").cloned(),
                })
            }
            Some(format) if format.eq_ignore_ascii_case("VST3") => {
                let info = vst3_plugins.iter().find(|info| info.path == uri);
                Some(PluginGraphPlugin {
                    node: PluginGraphNode::Vst3PluginInstance(instance_id),
                    instance_id,
                    format: "VST3".to_string(),
                    uri: uri.clone(),
                    plugin_id: info.map(|info| info.id.clone()).unwrap_or_default(),
                    name: info
                        .map(|info| info.name.clone())
                        .unwrap_or_else(|| uri.clone()),
                    main_audio_inputs: info.map(|info| info.audio_inputs).unwrap_or(0),
                    main_audio_outputs: info.map(|info| info.audio_outputs).unwrap_or(0),
                    audio_inputs: info.map(|info| info.audio_inputs).unwrap_or(0),
                    audio_outputs: info.map(|info| info.audio_outputs).unwrap_or(0),
                    midi_inputs: info
                        .map(|info| usize::from(info.has_midi_input))
                        .unwrap_or(0),
                    midi_outputs: info
                        .map(|info| usize::from(info.has_midi_output))
                        .unwrap_or(0),
                    state: None,
                })
            }
            Some(format) if format.eq_ignore_ascii_case("CLAP") => {
                let info = clap_plugins.iter().find(|info| info.path == uri);
                let caps = info.and_then(|info| info.capabilities.as_ref());
                Some(PluginGraphPlugin {
                    node: PluginGraphNode::ClapPluginInstance(instance_id),
                    instance_id,
                    format: "CLAP".to_string(),
                    uri: uri.clone(),
                    plugin_id: uri
                        .split_once("::")
                        .map(|(_, id)| id.to_string())
                        .unwrap_or_default(),
                    name: info
                        .map(|info| info.name.clone())
                        .unwrap_or_else(|| uri.clone()),
                    main_audio_inputs: caps.map(|caps| caps.audio_inputs).unwrap_or(0),
                    main_audio_outputs: caps.map(|caps| caps.audio_outputs).unwrap_or(0),
                    audio_inputs: caps.map(|caps| caps.audio_inputs).unwrap_or(0),
                    audio_outputs: caps.map(|caps| caps.audio_outputs).unwrap_or(0),
                    midi_inputs: caps.map(|caps| caps.midi_inputs).unwrap_or(0),
                    midi_outputs: caps.map(|caps| caps.midi_outputs).unwrap_or(0),
                    state: plugin.get("state").cloned(),
                })
            }
            _ => None,
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn plugin_graph_snapshot_from_json(
        graph: Option<&Value>,
        lv2_plugins: &[maolan_engine::lv2::Lv2PluginInfo],
        vst3_plugins: &[maolan_engine::vst3::Vst3PluginInfo],
        clap_plugins: &[maolan_engine::clap::ClapPluginInfo],
    ) -> maolan_engine::message::PluginGraphSnapshot {
        let Some(graph) = graph else {
            return (Vec::new(), Vec::new());
        };
        let mut plugins = Vec::new();
        let mut runtime_nodes = Vec::new();
        if let Some(plugin_values) = graph.get("plugins").and_then(Value::as_array) {
            for (idx, plugin) in plugin_values.iter().enumerate() {
                let Some(saved) = Self::plugin_graph_plugin_from_saved_json(
                    idx,
                    plugin,
                    lv2_plugins,
                    vst3_plugins,
                    clap_plugins,
                ) else {
                    continue;
                };
                runtime_nodes.push(saved.node.clone());
                plugins.push(saved);
            }
        }

        let mut connections = Vec::new();
        if let Some(connection_values) = graph.get("connections").and_then(Value::as_array) {
            for connection in connection_values {
                let Some(from_node) = Self::plugin_node_from_json_with_runtime_nodes(
                    &connection["from_node"],
                    &runtime_nodes,
                ) else {
                    continue;
                };
                let Some(to_node) = Self::plugin_node_from_json_with_runtime_nodes(
                    &connection["to_node"],
                    &runtime_nodes,
                ) else {
                    continue;
                };
                let Some(kind) = Self::kind_from_json(&connection["kind"]) else {
                    continue;
                };
                connections.push(maolan_engine::message::PluginGraphConnection {
                    from_node,
                    from_port: connection["from_port"].as_u64().unwrap_or(0) as usize,
                    to_node,
                    to_port: connection["to_port"].as_u64().unwrap_or(0) as usize,
                    kind,
                });
            }
        }

        (plugins, connections)
    }

    #[cfg(unix)]
    fn kind_to_json(kind: Kind) -> Value {
        match kind {
            Kind::Audio => json!("audio"),
            Kind::MIDI => json!("midi"),
        }
    }

    #[cfg(unix)]
    fn kind_from_json(v: &Value) -> Option<Kind> {
        match v.as_str()? {
            "audio" | "Audio" => Some(Kind::Audio),
            "midi" | "MIDI" => Some(Kind::MIDI),
            _ => None,
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn lv2_state_to_json(state: &maolan_engine::message::Lv2PluginState) -> Value {
        let port_values = state
            .port_values
            .iter()
            .map(|p| json!({"index": p.index, "value": p.value}))
            .collect::<Vec<_>>();
        let properties = state
            .properties
            .iter()
            .map(|p| {
                json!({
                    "key_uri": p.key_uri,
                    "type_uri": p.type_uri,
                    "flags": p.flags,
                    "value": p.value,
                })
            })
            .collect::<Vec<_>>();
        json!({
            "port_values": port_values,
            "properties": properties,
        })
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn lv2_state_from_json(v: &Value) -> Option<maolan_engine::message::Lv2PluginState> {
        let port_values = v["port_values"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        Some(maolan_engine::message::Lv2StatePortValue {
                            index: item["index"].as_u64()? as u32,
                            value: item["value"].as_f64()? as f32,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let properties = v["properties"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        Some(maolan_engine::message::Lv2StateProperty {
                            key_uri: item["key_uri"].as_str()?.to_string(),
                            type_uri: item["type_uri"].as_str()?.to_string(),
                            flags: item["flags"].as_u64().unwrap_or(0) as u32,
                            value: item["value"]
                                .as_array()?
                                .iter()
                                .map(|b| b.as_u64().unwrap_or(0) as u8)
                                .collect(),
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Some(maolan_engine::message::Lv2PluginState {
            port_values,
            properties,
        })
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn clap_state_from_json(v: &Value) -> Option<maolan_engine::clap::ClapPluginState> {
        serde_json::from_value(v.clone()).ok()
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn track_plugin_list_view(&self) -> iced::Element<'_, Message> {
        let state = self.state.blocking_read();
        let title = Self::plugin_graph_title(&state);

        let mut lv2_items = Vec::new();
        let filter = self.plugin_filter.trim().to_lowercase();
        for plugin in &state.lv2_plugins {
            if !filter.is_empty() {
                let name = plugin.name.to_lowercase();
                let uri = plugin.uri.to_lowercase();
                if !name.contains(&filter) && !uri.contains(&filter) {
                    continue;
                }
            }
            let is_selected = self.selected_lv2_plugins.contains(&plugin.uri);
            let row_content: iced::Element<'_, Message> = row![
                text(if is_selected { "[x]" } else { "[ ]" }),
                text(format!(
                    "{} (a:{}/{}, m:{}/{})",
                    plugin.name,
                    plugin.audio_inputs,
                    plugin.audio_outputs,
                    plugin.midi_inputs,
                    plugin.midi_outputs
                ))
                .width(Length::Fill),
            ]
            .spacing(8)
            .width(Length::Fill)
            .into();

            let row_button = if is_selected {
                button(row_content).style(button::primary)
            } else {
                button(row_content).style(button::text)
            };
            lv2_items.push(
                row_button
                    .width(Length::Fill)
                    .on_press(Message::SelectLv2Plugin(plugin.uri.clone()))
                    .into(),
            );
        }
        let lv2_list = column(lv2_items);

        let mut clap_items = Vec::new();
        let clap_filter = self.clap_plugin_filter.trim().to_lowercase();
        for plugin in &state.clap_plugins {
            if !clap_filter.is_empty() {
                let name = plugin.name.to_lowercase();
                let path = plugin.path.to_lowercase();
                if !name.contains(&clap_filter) && !path.contains(&clap_filter) {
                    continue;
                }
            }
            let is_selected = self.selected_clap_plugins.contains(&plugin.path);

            // Build capability indicators
            let mut capability_icons = String::new();
            if let Some(caps) = &plugin.capabilities {
                if caps.has_gui {
                    capability_icons.push_str("\u{1F5BC} "); // 🖼 Frame with Picture
                }
                if caps.has_params {
                    capability_icons.push_str("\u{2699} "); // ⚙ Gear
                }
                if caps.has_state {
                    capability_icons.push_str("\u{1F4BE} "); // 💾 Floppy Disk
                }
            }

            let row_content: iced::Element<'_, Message> = row![
                text(if is_selected { "[x]" } else { "[ ]" }),
                text(plugin.name.clone()).width(Length::Fill),
                text(capability_icons),
            ]
            .spacing(8)
            .width(Length::Fill)
            .into();
            let row_button = if is_selected {
                button(row_content).style(button::primary)
            } else {
                button(row_content).style(button::text)
            };
            clap_items.push(
                row_button
                    .width(Length::Fill)
                    .on_press(Message::SelectClapPlugin(plugin.path.clone()))
                    .into(),
            );
        }
        let clap_list = column(clap_items);

        let mut vst3_items = Vec::new();
        let vst3_filter = self.vst3_plugin_filter.trim().to_lowercase();
        for plugin in &state.vst3_plugins {
            if !vst3_filter.is_empty() {
                let name = plugin.name.to_lowercase();
                let path = plugin.path.to_lowercase();
                if !name.contains(&vst3_filter) && !path.contains(&vst3_filter) {
                    continue;
                }
            }
            let is_selected = self.selected_vst3_plugins.contains(&plugin.path);
            let row_content: iced::Element<'_, Message> = row![
                text(if is_selected { "[x]" } else { "[ ]" }),
                text(plugin.name.clone()).width(Length::Fill),
            ]
            .spacing(8)
            .width(Length::Fill)
            .into();
            let row_button = if is_selected {
                button(row_content).style(button::primary)
            } else {
                button(row_content).style(button::text)
            };
            vst3_items.push(
                row_button
                    .width(Length::Fill)
                    .on_press(Message::SelectVst3Plugin(plugin.path.clone()))
                    .into(),
            );
        }
        let vst3_list = column(vst3_items);

        let plugin_controls = match self.plugin_format {
            PluginFormat::Lv2 => {
                let load = if self.selected_lv2_plugins.is_empty() {
                    button("Load")
                } else {
                    button(text(format!("Load ({})", self.selected_lv2_plugins.len())))
                        .on_press(Message::LoadSelectedLv2Plugins)
                };
                column![
                    text_input("Filter LV2 plugins...", &self.plugin_filter)
                        .on_input(Message::FilterLv2Plugins)
                        .width(Length::Fill),
                    scrollable(lv2_list).height(Length::Fill),
                    row![
                        load,
                        pick_list(
                            Self::supported_plugin_formats(),
                            Some(self.plugin_format),
                            Message::PluginFormatSelected,
                        ),
                    ]
                    .spacing(10),
                ]
                .spacing(10)
            }
            PluginFormat::Vst3 => {
                let load = if self.selected_vst3_plugins.is_empty() {
                    button("Load")
                } else {
                    button(text(format!("Load ({})", self.selected_vst3_plugins.len())))
                        .on_press(Message::LoadSelectedVst3Plugins)
                };
                column![
                    text_input("Filter VST3 plugins...", &self.vst3_plugin_filter)
                        .on_input(Message::FilterVst3Plugins)
                        .width(Length::Fill),
                    scrollable(vst3_list).height(Length::Fill),
                    row![
                        load,
                        pick_list(
                            Self::supported_plugin_formats(),
                            Some(self.plugin_format),
                            Message::PluginFormatSelected,
                        ),
                    ]
                    .spacing(10),
                ]
                .spacing(10)
            }
            PluginFormat::Clap => {
                let load = if self.selected_clap_plugins.is_empty() {
                    button("Load")
                } else {
                    button(text(format!("Load ({})", self.selected_clap_plugins.len())))
                        .on_press(Message::LoadSelectedClapPlugins)
                };
                column![
                    text_input("Filter CLAP plugins...", &self.clap_plugin_filter)
                        .on_input(Message::FilterClapPlugin)
                        .width(Length::Fill),
                    scrollable(clap_list).height(Length::Fill),
                    row![
                        load,
                        pick_list(
                            Self::supported_plugin_formats(),
                            Some(self.plugin_format),
                            Message::PluginFormatSelected,
                        ),
                    ]
                    .spacing(10),
                ]
                .spacing(10)
            }
        };

        container(
            column![
                text(title),
                plugin_controls,
                row![
                    button("Close")
                        .on_press(Message::Cancel)
                        .style(button::secondary),
                ]
                .spacing(10),
            ]
            .spacing(10),
        )
        .style(|_theme| crate::style::app_background())
        .padding(20)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    #[cfg(target_os = "macos")]
    fn track_plugin_list_view(&self) -> iced::Element<'_, Message> {
        let state = self.state.blocking_read();
        let title = Self::plugin_graph_title(&state);
        let mut vst3_items = Vec::new();
        let filter = self.vst3_plugin_filter.trim().to_lowercase();
        for plugin in &state.vst3_plugins {
            if !filter.is_empty() {
                let name = plugin.name.to_lowercase();
                let path = plugin.path.to_lowercase();
                if !name.contains(&filter) && !path.contains(&filter) {
                    continue;
                }
            }
            let is_selected = self.selected_vst3_plugins.contains(&plugin.path);
            let row_content: iced::Element<'_, Message> = row![
                text(if is_selected { "[x]" } else { "[ ]" }),
                text(plugin.name.clone()).width(Length::Fill),
            ]
            .spacing(8)
            .width(Length::Fill)
            .into();
            let row_button = if is_selected {
                button(row_content).style(button::primary)
            } else {
                button(row_content).style(button::text)
            };
            vst3_items.push(
                row_button
                    .width(Length::Fill)
                    .on_press(Message::SelectVst3Plugin(plugin.path.clone()))
                    .into(),
            );
        }
        let vst3_list = column(vst3_items);

        let mut clap_items = Vec::new();
        let clap_filter = self.clap_plugin_filter.trim().to_lowercase();
        for plugin in &state.clap_plugins {
            if !clap_filter.is_empty() {
                let name = plugin.name.to_lowercase();
                let path = plugin.path.to_lowercase();
                if !name.contains(&clap_filter) && !path.contains(&clap_filter) {
                    continue;
                }
            }
            let is_selected = self.selected_clap_plugins.contains(&plugin.path);

            // Build capability indicators
            let mut capability_icons = String::new();
            if let Some(caps) = &plugin.capabilities {
                if caps.has_gui {
                    capability_icons.push_str("\u{1F5BC} "); // 🖼 Frame with Picture
                }
                if caps.has_params {
                    capability_icons.push_str("\u{2699} "); // ⚙ Gear
                }
                if caps.has_state {
                    capability_icons.push_str("\u{1F4BE} "); // 💾 Floppy Disk
                }
            }

            let row_content: iced::Element<'_, Message> = row![
                text(if is_selected { "[x]" } else { "[ ]" }),
                text(plugin.name.clone()).width(Length::Fill),
                text(capability_icons),
            ]
            .spacing(8)
            .width(Length::Fill)
            .into();
            let row_button = if is_selected {
                button(row_content).style(button::primary)
            } else {
                button(row_content).style(button::text)
            };
            clap_items.push(
                row_button
                    .width(Length::Fill)
                    .on_press(Message::SelectClapPlugin(plugin.path.clone()))
                    .into(),
            );
        }
        let clap_list = column(clap_items);

        let plugin_controls = if self.plugin_format == PluginFormat::Clap {
            let load = if self.selected_clap_plugins.is_empty() {
                button("Load")
            } else {
                button(text(format!("Load ({})", self.selected_clap_plugins.len())))
                    .on_press(Message::LoadSelectedClapPlugins)
            };
            column![
                text_input("Filter CLAP plugins...", &self.clap_plugin_filter)
                    .on_input(Message::FilterClapPlugin)
                    .width(Length::Fill),
                scrollable(clap_list).height(Length::Fill),
                row![
                    load,
                    pick_list(
                        Self::supported_plugin_formats(),
                        Some(self.plugin_format),
                        Message::PluginFormatSelected,
                    ),
                ]
                .spacing(10),
            ]
            .spacing(10)
        } else {
            let load = if self.selected_vst3_plugins.is_empty() {
                button("Load")
            } else {
                button(text(format!("Load ({})", self.selected_vst3_plugins.len())))
                    .on_press(Message::LoadSelectedVst3Plugins)
            };
            column![
                text_input("Filter VST3 plugins...", &self.vst3_plugin_filter)
                    .on_input(Message::FilterVst3Plugins)
                    .width(Length::Fill),
                scrollable(vst3_list).height(Length::Fill),
                row![
                    load,
                    pick_list(
                        Self::supported_plugin_formats(),
                        Some(self.plugin_format),
                        Message::PluginFormatSelected,
                    ),
                ]
                .spacing(10),
            ]
            .spacing(10)
        };

        container(
            column![
                text(title),
                plugin_controls,
                row![
                    button("Close")
                        .on_press(Message::Cancel)
                        .style(button::secondary),
                ]
                .spacing(10),
            ]
            .spacing(10),
        )
        .padding(20)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    fn send(&self, action: Action) -> Task<Message> {
        Task::perform(
            async move { CLIENT.send(EngineMessage::Request(action)).await },
            |result| match result {
                Ok(_) => Message::SendMessageFinished(Ok(())),
                Err(_) => Message::Response(Err("Channel closed".to_string())),
            },
        )
    }

    fn selected_export_formats(&self) -> Vec<ExportFormat> {
        let mut formats = Vec::new();
        if self.export_format_wav {
            formats.push(ExportFormat::Wav);
        }
        if self.export_format_mp3 {
            formats.push(ExportFormat::Mp3);
        }
        if self.export_format_ogg {
            formats.push(ExportFormat::Ogg);
        }
        if self.export_format_flac {
            formats.push(ExportFormat::Flac);
        }
        formats
    }

    fn export_bit_depth_options(formats: &[ExportFormat]) -> Vec<ExportBitDepth> {
        if formats
            .iter()
            .any(|f| matches!(f, ExportFormat::Wav | ExportFormat::Flac))
        {
            EXPORT_BIT_DEPTH_ALL.to_vec()
        } else {
            vec![ExportBitDepth::Float32]
        }
    }

    fn export_format_extension(format: ExportFormat) -> &'static str {
        match format {
            ExportFormat::Wav => "wav",
            ExportFormat::Mp3 => "mp3",
            ExportFormat::Ogg => "ogg",
            ExportFormat::Flac => "flac",
        }
    }

    fn export_base_path(path: PathBuf) -> PathBuf {
        let known_ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .is_some_and(|e| matches!(e.as_str(), "wav" | "mp3" | "ogg" | "flac"));
        if known_ext {
            path.with_extension("")
        } else {
            path
        }
    }

    fn sanitize_export_component(value: &str) -> String {
        let mut out = String::with_capacity(value.len());
        for ch in value.chars() {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                out.push(ch);
            } else {
                out.push('_');
            }
        }
        if out.is_empty() {
            "track".to_string()
        } else {
            out
        }
    }

    fn export_settings_view(&self) -> iced::Element<'_, Message> {
        let last_message = self.state.blocking_read().message.clone();
        let selected_formats = self.selected_export_formats();
        let bit_depth_options = Self::export_bit_depth_options(&selected_formats);
        let available_hw_out_ports = self.available_export_hw_out_ports();
        let mp3_supported = self.export_mp3_supported_for_current_settings();
        let hw_out_column_count = if available_hw_out_ports.len() > 16 {
            4
        } else if available_hw_out_ports.len() > 8 {
            2
        } else {
            1
        };
        let hw_out_rows_per_column = available_hw_out_ports
            .len()
            .div_ceil(hw_out_column_count.max(1));
        let selected_bit_depth = if bit_depth_options.contains(&self.export_bit_depth) {
            self.export_bit_depth
        } else {
            bit_depth_options
                .first()
                .copied()
                .unwrap_or(ExportBitDepth::Int24)
        };

        let body = container(
            scrollable(
                container(
                    column![
                        text("Export session").size(16),
                        row![
                            text("Formats:"),
                            checkbox(self.export_format_wav)
                                .label("WAV")
                                .on_toggle(Message::ExportFormatWavToggled),
                            if mp3_supported {
                                checkbox(self.export_format_mp3)
                                    .label("MP3")
                                    .on_toggle(Message::ExportFormatMp3Toggled)
                            } else {
                                checkbox(false).label("MP3 (mono/stereo only)")
                            },
                            checkbox(self.export_format_ogg)
                                .label("OGG")
                                .on_toggle(Message::ExportFormatOggToggled),
                            checkbox(self.export_format_flac)
                                .label("FLAC")
                                .on_toggle(Message::ExportFormatFlacToggled),
                        ]
                        .spacing(10)
                        .align_y(iced::Alignment::Center),
                        row![
                            text("Sample rate (Hz):"),
                            pick_list(
                                STANDARD_EXPORT_SAMPLE_RATES.to_vec(),
                                Some(self.export_sample_rate_hz),
                                Message::ExportSampleRateSelected
                            )
                            .placeholder("Choose sample rate")
                            .width(Length::Fixed(220.0)),
                        ]
                        .spacing(10)
                        .align_y(iced::Alignment::Center),
                        if selected_formats
                            .iter()
                            .any(|f| matches!(f, ExportFormat::Wav | ExportFormat::Flac))
                        {
                            row![
                                text("Bit depth:"),
                                pick_list(
                                    bit_depth_options,
                                    Some(selected_bit_depth),
                                    Message::ExportBitDepthSelected
                                )
                                .placeholder("Choose bit depth"),
                            ]
                            .spacing(10)
                            .align_y(iced::Alignment::Center)
                        } else {
                            row![text("Bit depth: codec-managed (lossy formats)")]
                                .spacing(10)
                                .align_y(iced::Alignment::Center)
                        },
                        if self.export_format_mp3 {
                            row![
                                text("MP3 mode:"),
                                pick_list(
                                    EXPORT_MP3_MODE_ALL.to_vec(),
                                    Some(self.export_mp3_mode),
                                    Message::ExportMp3ModeSelected
                                )
                                .placeholder("Choose MP3 mode")
                                .width(Length::Fixed(160.0)),
                                text("Bitrate (kbps):"),
                                pick_list(
                                    vec![96_u16, 128, 160, 192, 224, 256, 320],
                                    Some(self.export_mp3_bitrate_kbps),
                                    Message::ExportMp3BitrateSelected
                                )
                                .placeholder("Bitrate")
                                .width(Length::Fixed(140.0)),
                            ]
                            .spacing(10)
                            .align_y(iced::Alignment::Center)
                        } else {
                            row![text("")].spacing(10).align_y(iced::Alignment::Center)
                        },
                        if self.export_format_ogg {
                            row![
                                text("OGG quality (-0.1..1.0):"),
                                text_input("0.6", &self.export_ogg_quality_input)
                                    .on_input(Message::ExportOggQualityInput)
                                    .width(Length::Fixed(140.0)),
                            ]
                            .spacing(10)
                            .align_y(iced::Alignment::Center)
                        } else {
                            row![text("")].spacing(10).align_y(iced::Alignment::Center)
                        },
                        row![
                            text("Render mode:"),
                            pick_list(
                                EXPORT_RENDER_MODE_ALL.to_vec(),
                                Some(self.export_render_mode),
                                Message::ExportRenderModeSelected
                            )
                            .placeholder("Choose render mode")
                            .width(Length::Fixed(220.0)),
                        ]
                        .spacing(10)
                        .align_y(iced::Alignment::Center),
                        if matches!(self.export_render_mode, ExportRenderMode::Mixdown) {
                            container(
                                column![
                                    text("Export hw:out ports:"),
                                    if available_hw_out_ports.is_empty() {
                                        iced::Element::from(text(
                                            "No hardware output ports are available.",
                                        ))
                                    } else {
                                        iced::Element::from(row(
                                            available_hw_out_ports
                                                .chunks(hw_out_rows_per_column.max(1))
                                                .map(|ports| {
                                                    column(
                                                        ports
                                                            .iter()
                                                            .map(|port| {
                                                                checkbox(
                                                                    self.export_hw_out_ports
                                                                        .contains(port),
                                                                )
                                                                .label(format!(
                                                                    "hw:out {}",
                                                                    port + 1
                                                                ))
                                                                .on_toggle({
                                                                    let port = *port;
                                                                    move |enabled| {
                                                                        Message::ExportHwOutPortToggled(
                                                                            port, enabled,
                                                                        )
                                                                    }
                                                                })
                                                                .into()
                                                            })
                                                            .collect::<Vec<
                                                                iced::Element<'_, Message>,
                                                            >>(),
                                                    )
                                                    .spacing(6)
                                                    .width(Length::FillPortion(1))
                                                    .into()
                                                })
                                                .collect::<Vec<iced::Element<'_, Message>>>(),
                                        )
                                        .spacing(24)
                                        .width(Length::Fill))
                                    }
                                ]
                                .spacing(8),
                            )
                        } else {
                            container(text(
                                "Stem export writes one channel per track output port.",
                            ))
                        },
                        checkbox(self.export_realtime_fallback)
                            .label("Real-time fallback render")
                            .on_toggle(Message::ExportRealtimeFallbackToggled),
                        row![
                            checkbox(self.export_master_limiter)
                                .label("Master limiter")
                                .on_toggle(Message::ExportMasterLimiterToggled),
                            text("Ceiling (dBTP):"),
                            text_input("-1.0", &self.export_master_limiter_ceiling_input)
                                .on_input(Message::ExportMasterLimiterCeilingInput)
                                .width(Length::Fixed(110.0)),
                        ]
                        .spacing(10)
                        .align_y(iced::Alignment::Center),
                        checkbox(self.export_normalize)
                            .label("Normalize")
                            .on_toggle(Message::ExportNormalizeToggled),
                        if self.export_normalize {
                            container(
                                column![
                                    row![
                                        text("Mode:"),
                                        pick_list(
                                            EXPORT_NORMALIZE_MODE_ALL.to_vec(),
                                            Some(self.export_normalize_mode),
                                            Message::ExportNormalizeModeSelected
                                        )
                                        .placeholder("Choose mode")
                                        .width(Length::Fixed(180.0)),
                                    ]
                                    .spacing(10)
                                    .align_y(iced::Alignment::Center),
                                    if matches!(
                                        self.export_normalize_mode,
                                        ExportNormalizeMode::Peak
                                    ) {
                                        row![
                                            text("Target (dBFS):"),
                                            text_input("0.0", &self.export_normalize_dbfs_input)
                                                .on_input(Message::ExportNormalizeDbfsInput)
                                                .width(Length::Fixed(120.0)),
                                        ]
                                        .spacing(10)
                                        .align_y(iced::Alignment::Center)
                                    } else {
                                        row![
                                            text("Target (LUFS):"),
                                            text_input("-23.0", &self.export_normalize_lufs_input)
                                                .on_input(Message::ExportNormalizeLufsInput)
                                                .width(Length::Fixed(120.0)),
                                            text("TP ceiling (dBTP):"),
                                            text_input("-1.0", &self.export_normalize_dbtp_input)
                                                .on_input(Message::ExportNormalizeDbtpInput)
                                                .width(Length::Fixed(120.0)),
                                        ]
                                        .spacing(10)
                                        .align_y(iced::Alignment::Center)
                                    },
                                    if matches!(
                                        self.export_normalize_mode,
                                        ExportNormalizeMode::Loudness
                                    ) {
                                        checkbox(self.export_normalize_tp_limiter)
                                            .label("Use true-peak limiter")
                                            .on_toggle(Message::ExportNormalizeLimiterToggled)
                                    } else {
                                        checkbox(false).label("")
                                    }
                                ]
                                .spacing(8),
                            )
                        } else {
                            container(row![text("")])
                        },
                        row![
                            button("Export").on_press(Message::ExportSettingsConfirm),
                            button("Cancel")
                                .on_press(Message::Cancel)
                                .style(button::secondary),
                        ]
                        .spacing(10),
                        text("Use standard sample rates for broad compatibility."),
                    ]
                    .align_x(iced::Alignment::Start)
                    .spacing(12),
                )
                .width(Length::Fill),
            )
            .width(Length::Fill)
            .height(Length::Fill),
        )
        .style(|_theme| crate::style::app_background())
        .padding(20)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::Alignment::Center)
        .align_y(iced::Alignment::Center);

        column![
            body,
            container(text(format!("Last message: {}", last_message)))
                .width(Length::Fill)
                .padding([0, 20]),
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    fn preferences_auto_device_option() -> PreferencesDeviceOption {
        PreferencesDeviceOption {
            id: PREF_DEVICE_AUTO_ID.to_string(),
            label: "Auto".to_string(),
        }
    }

    fn preferences_selected_device_option(
        options: &[PreferencesDeviceOption],
        selected_id: Option<&str>,
    ) -> Option<PreferencesDeviceOption> {
        let selected_id = selected_id.unwrap_or(PREF_DEVICE_AUTO_ID);
        options.iter().find(|opt| opt.id == selected_id).cloned()
    }

    fn preferences_output_device_options(&self) -> Vec<PreferencesDeviceOption> {
        let mut options = vec![Self::preferences_auto_device_option()];
        let state = self.state.blocking_read();
        #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
        {
            options.extend(state.available_hw.iter().map(|hw| PreferencesDeviceOption {
                id: hw.id.clone(),
                label: hw.label.clone(),
            }));
        }
        #[cfg(not(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd")))]
        {
            options.extend(state.available_hw.iter().map(|hw| PreferencesDeviceOption {
                id: hw.clone(),
                label: hw.clone(),
            }));
        }
        options
    }

    fn preferences_input_device_options(&self) -> Vec<PreferencesDeviceOption> {
        let mut options = vec![Self::preferences_auto_device_option()];
        let state = self.state.blocking_read();
        #[cfg(any(target_os = "freebsd", target_os = "openbsd"))]
        {
            options.extend(state.available_hw.iter().map(|hw| PreferencesDeviceOption {
                id: hw.id.clone(),
                label: hw.label.clone(),
            }));
        }
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            options.extend(
                state
                    .available_input_hw
                    .iter()
                    .map(|hw| PreferencesDeviceOption {
                        id: hw.id.clone(),
                        label: hw.label.clone(),
                    }),
            );
        }
        options
    }

    fn apply_preferred_devices_to_state(state: &mut StateData, prefs: &AppPreferences) {
        #[cfg(any(unix, target_os = "windows"))]
        {
            let bits = if AUDIO_BIT_DEPTH_OPTIONS.contains(&prefs.default_audio_bit_depth) {
                prefs.default_audio_bit_depth
            } else {
                audio_defaults::BIT_DEPTH
            };
            state.oss_bits = bits;
        }
        if let Some(device_id) = prefs.default_output_device_id.as_deref() {
            #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
            if let Some(selected) = state
                .available_hw
                .iter()
                .find(|hw| hw.id == device_id)
                .cloned()
            {
                state.selected_hw = Some(selected);
            }
            #[cfg(not(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd")))]
            if state.available_hw.iter().any(|hw| hw == device_id) {
                state.selected_hw = Some(device_id.to_string());
            }
        }
        if let Some(device_id) = prefs.default_input_device_id.as_deref() {
            #[cfg(any(target_os = "freebsd", target_os = "openbsd"))]
            if let Some(selected) = state
                .available_hw
                .iter()
                .find(|hw| hw.id == device_id)
                .cloned()
            {
                state.selected_input_hw = Some(selected);
            }
            #[cfg(target_os = "linux")]
            if let Some(selected) = state
                .available_input_hw
                .iter()
                .find(|hw| hw.id == device_id)
                .cloned()
            {
                state.selected_input_hw = Some(selected);
            }
            #[cfg(target_os = "windows")]
            if state.available_input_hw.iter().any(|hw| hw == device_id) {
                state.selected_input_hw = Some(device_id.to_string());
            }
        }
        #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
        if let Some(selected) = state.selected_hw.as_ref()
            && let Some(bits) = selected.preferred_bits()
        {
            state.oss_bits = bits;
        }
        #[cfg(target_os = "windows")]
        if let Some(selected) = state.selected_hw.as_ref() {
            state.oss_bits = crate::state::discover_windows_output_bit_depths(selected)
                .first()
                .copied()
                .unwrap_or(state.oss_bits);
        }
    }

    fn preferences_view(&self) -> iced::Element<'_, Message> {
        let output_options = self.preferences_output_device_options();
        let selected_output = Self::preferences_selected_device_option(
            &output_options,
            self.prefs_default_output_device_id.as_deref(),
        );
        let input_options = self.preferences_input_device_options();
        let selected_input = Self::preferences_selected_device_option(
            &input_options,
            self.prefs_default_input_device_id.as_deref(),
        );
        container(
            column![
                text("Preferences").size(16),
                row![
                    checkbox(self.prefs_osc_enabled)
                        .label("Enable OSC")
                        .on_toggle(Message::PreferencesOscEnabledToggled),
                ]
                .spacing(10)
                .align_y(iced::Alignment::Center),
                row![
                    text("Default export sample rate (Hz):"),
                    pick_list(
                        STANDARD_EXPORT_SAMPLE_RATES.to_vec(),
                        Some(self.prefs_export_sample_rate_hz),
                        Message::PreferencesSampleRateSelected
                    )
                    .placeholder("Choose sample rate")
                    .width(Length::Fixed(220.0)),
                ]
                .spacing(10)
                .align_y(iced::Alignment::Center),
                row![
                    text("Default clip snap mode:"),
                    pick_list(
                        SNAP_MODE_ALL.to_vec(),
                        Some(self.prefs_snap_mode),
                        Message::PreferencesSnapModeSelected
                    )
                    .placeholder("Choose snap mode")
                    .width(Length::Fixed(220.0)),
                ]
                .spacing(10)
                .align_y(iced::Alignment::Center),
                row![
                    text("Default MIDI snap mode:"),
                    pick_list(
                        SNAP_MODE_ALL.to_vec(),
                        Some(self.prefs_midi_snap_mode),
                        Message::PreferencesMidiSnapModeSelected
                    )
                    .placeholder("Choose snap mode")
                    .width(Length::Fixed(220.0)),
                ]
                .spacing(10)
                .align_y(iced::Alignment::Center),
                #[cfg(unix)]
                row![
                    text("Default bit depth:"),
                    pick_list(
                        AUDIO_BIT_DEPTH_OPTIONS.to_vec(),
                        Some(self.prefs_audio_bit_depth),
                        Message::PreferencesBitDepthSelected
                    )
                    .placeholder("Choose bit depth")
                    .width(Length::Fixed(220.0)),
                ]
                .spacing(10)
                .align_y(iced::Alignment::Center),
                row![
                    text("Default output device:"),
                    pick_list(
                        output_options,
                        selected_output,
                        Message::PreferencesOutputDeviceSelected
                    )
                    .placeholder("Choose output device")
                    .width(Length::Fixed(320.0)),
                ]
                .spacing(10)
                .align_y(iced::Alignment::Center),
                if platform_caps::HAS_SEPARATE_AUDIO_INPUT_DEVICE {
                    row![
                        text("Default input device:"),
                        pick_list(
                            input_options,
                            selected_input,
                            Message::PreferencesInputDeviceSelected
                        )
                        .placeholder("Choose input device")
                        .width(Length::Fixed(320.0)),
                    ]
                    .spacing(10)
                    .align_y(iced::Alignment::Center)
                } else {
                    row![text("")].spacing(10).align_y(iced::Alignment::Center)
                },
                row![
                    button("Save").on_press(Message::PreferencesSave),
                    button("Cancel")
                        .on_press(Message::Cancel)
                        .style(button::secondary),
                ]
                .spacing(10),
            ]
            .align_x(iced::Alignment::Start)
            .spacing(12),
        )
        .style(|_theme| crate::style::app_background())
        .padding(20)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::Alignment::Center)
        .align_y(iced::Alignment::Center)
        .into()
    }

    fn generate_audio_view(&self) -> iced::Element<'_, Message> {
        let session_ready = self.session_dir.is_some();
        let progress_label = if self.generate_audio_in_progress {
            if let Some(operation) = self.generate_audio_operation.as_deref() {
                format!(
                    "{} ({:.0}%)",
                    operation,
                    (self.generate_audio_progress * 100.0).clamp(0.0, 100.0)
                )
            } else {
                format!(
                    "Generating ({:.0}%)",
                    (self.generate_audio_progress * 100.0).clamp(0.0, 100.0)
                )
            }
        } else {
            "Open or save a session before generating audio.".to_string()
        };
        let prompt_label = "Lyrics";
        let generate_button = if self.generate_audio_in_progress || !session_ready {
            button("Generate")
        } else {
            button("Generate").on_press(Message::GenerateAudioSubmit)
        };
        let cancel_button = if self.generate_audio_in_progress {
            button("Cancel")
                .on_press(Message::GenerateAudioCancel)
                .style(button::danger)
        } else {
            button("Close")
                .on_press(Message::Cancel)
                .style(button::secondary)
        };

        container(
            scrollable(
                column![
                    text("Generate Audio").size(16),
                    row![
                        text("Model:"),
                        pick_list(
                            GenerateAudioModelOption::ALL.to_vec(),
                            Some(self.generate_audio_model),
                            Message::GenerateAudioModelSelected
                        )
                        .placeholder("Choose model")
                        .width(Length::Fill),
                    ]
                    .spacing(10)
                    .align_y(iced::Alignment::Center),
                    text_editor(&self.generate_audio_prompt_editor)
                        .on_action(Message::GenerateAudioPromptAction)
                        .height(Length::Fixed(120.0))
                        .placeholder(prompt_label),
                    text_input("Tags (optional)", &self.generate_audio_tags_input)
                        .on_input(Message::GenerateAudioTagsInput)
                        .width(Length::Fill),
                    row![
                        text("Backend:"),
                        pick_list(
                            BurnBackendOption::ALL.to_vec(),
                            Some(self.generate_audio_backend),
                            Message::GenerateAudioBackendSelected
                        )
                        .placeholder("Choose backend")
                        .width(Length::Fill),
                    ]
                    .spacing(10)
                    .align_y(iced::Alignment::Center),
                    row![
                        text("CFG scale"),
                        number_input_f32(
                            &self.generate_audio_cfg_scale_input,
                            0.0..=20.0,
                            0.1,
                            Message::GenerateAudioCfgScaleInput
                        ),
                        text("Steps:"),
                        number_input(
                            &self.generate_audio_steps_input,
                            1..=50,
                            Message::GenerateAudioStepsInput
                        ),
                    ]
                    .spacing(10)
                    .align_y(iced::Alignment::Center),
                    row![
                        text("Seconds total:"),
                        number_input(
                            &self.generate_audio_seconds_total_input,
                            1..=600,
                            Message::GenerateAudioSecondsTotalInput
                        ),
                    ]
                    .spacing(10)
                    .align_y(iced::Alignment::Center),
                    text(progress_label),
                    if self.generate_audio_in_progress {
                        container(progress_bar(
                            0.0..=1.0,
                            self.generate_audio_progress.clamp(0.0, 1.0),
                        ))
                        .width(Length::Fill)
                    } else {
                        container(progress_bar(0.0..=1.0, 0.0)).width(Length::Fill)
                    },
                    row![generate_button, cancel_button].spacing(10),
                ]
                .align_x(iced::Alignment::Start)
                .spacing(12),
            )
            .width(Length::Fill)
            .height(Length::Fill),
        )
        .style(|_theme| crate::style::app_background())
        .padding(12)
        .width(Length::Fixed(360.0))
        .height(Length::Fill)
        .into()
    }

    fn session_metadata_view(&self) -> iced::Element<'_, Message> {
        let state = self.state.blocking_read();
        container(
            column![
                text("Session Metadata").size(16),
                row![
                    text("Author:"),
                    text_input("Author", &state.session_author)
                        .on_input(Message::SessionMetadataAuthorInput)
                        .width(Length::Fixed(260.0)),
                ]
                .spacing(10)
                .align_y(iced::Alignment::Center),
                row![
                    text("Album:"),
                    text_input("Album", &state.session_album)
                        .on_input(Message::SessionMetadataAlbumInput)
                        .width(Length::Fixed(260.0)),
                ]
                .spacing(10)
                .align_y(iced::Alignment::Center),
                row![
                    text("Year:"),
                    text_input("Year", &state.session_year)
                        .on_input(Message::SessionMetadataYearInput)
                        .width(Length::Fixed(120.0)),
                    text("Track #:"),
                    text_input("Track number", &state.session_track_number)
                        .on_input(Message::SessionMetadataTrackNumberInput)
                        .width(Length::Fixed(120.0)),
                ]
                .spacing(10)
                .align_y(iced::Alignment::Center),
                row![
                    text("Genre:"),
                    text_input("Genre", &state.session_genre)
                        .on_input(Message::SessionMetadataGenreInput)
                        .width(Length::Fixed(260.0)),
                ]
                .spacing(10)
                .align_y(iced::Alignment::Center),
                row![
                    button("Save").on_press(Message::SessionMetadataSave),
                    button("Cancel")
                        .on_press(Message::Cancel)
                        .style(button::secondary),
                ]
                .spacing(10),
            ]
            .align_x(iced::Alignment::Start)
            .spacing(12),
        )
        .style(|_theme| crate::style::app_background())
        .padding(20)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::Alignment::Center)
        .align_y(iced::Alignment::Center)
        .into()
    }

    fn autosave_recovery_view(&self) -> iced::Element<'_, Message> {
        let session_label = self
            .pending_recovery_session_dir
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "<unknown session>".to_string());
        container(
            column![
                text("Autosave Recovery").size(16),
                text(format!(
                    "A newer autosave snapshot was found for:\n{}",
                    session_label
                )),
                row![
                    button("Recover Latest").on_press(Message::RecoverAutosaveSnapshot),
                    button("Ignore").on_press(Message::RecoverAutosaveIgnore),
                ]
                .spacing(10),
            ]
            .align_x(iced::Alignment::Start)
            .spacing(12),
        )
        .style(|_theme| crate::style::app_background())
        .padding(20)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::Alignment::Center)
        .align_y(iced::Alignment::Center)
        .into()
    }

    fn unsaved_changes_view(&self) -> iced::Element<'_, Message> {
        let session_label = self
            .session_dir
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled session".to_string());
        container(
            column![
                text("Unsaved Changes").size(16),
                text(format!(
                    "Session has unsaved changes:\n{}\n\nSave before closing, discard changes, or cancel.",
                    session_label
                )),
                row![
                    button("Save").on_press(Message::ConfirmCloseSave),
                    button("Discard").on_press(Message::ConfirmCloseDiscard),
                    button("Cancel").on_press(Message::ConfirmCloseCancel),
                ]
                .spacing(10),
            ]
            .align_x(iced::Alignment::Start)
            .spacing(12),
        )
        .style(|_theme| crate::style::app_background())
        .padding(20)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::Alignment::Center)
        .align_y(iced::Alignment::Center)
        .into()
    }

    fn update_children(&mut self, message: &Message) {
        self.menu.update(message);
        self.toolbar.update(message);
        self.workspace.update(message);
        self.connections.update(message);
        #[cfg(all(unix, not(target_os = "macos")))]
        self.track_plugins.update(message);
        self.add_track.update(message);
        self.clip_rename.update(message);
        self.track_rename.update(message);
        self.track_group.update(message);
        self.track_marker.update(message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use maolan_engine::message::PluginGraphNode;
    #[cfg(unix)]
    use std::path::PathBuf;
    use std::sync::{LazyLock, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    static AUDIO_PEAK_TEST_GUARD: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    #[test]
    fn normalize_recent_session_paths_trims_deduplicates_and_limits() {
        let paths = vec![
            "  /a  ".to_string(),
            "".to_string(),
            "/b".to_string(),
            "/a".to_string(),
            "   ".to_string(),
        ];

        let normalized = Maolan::normalize_recent_session_paths(paths);
        assert_eq!(normalized, vec!["/a".to_string(), "/b".to_string()]);
        assert!(normalized.len() <= crate::consts::gui_mod::MAX_RECENT_SESSIONS);
    }

    #[test]
    #[cfg(unix)]
    fn kind_json_roundtrip_rejects_unknown_values() {
        assert_eq!(
            Maolan::kind_from_json(&Maolan::kind_to_json(Kind::Audio)),
            Some(Kind::Audio)
        );
        assert_eq!(
            Maolan::kind_from_json(&Maolan::kind_to_json(Kind::MIDI)),
            Some(Kind::MIDI)
        );
        assert_eq!(
            Maolan::kind_from_json(&Value::String("other".to_string())),
            None
        );
        assert_eq!(
            Maolan::kind_from_json(&Value::String("Audio".to_string())),
            Some(Kind::Audio)
        );
        assert_eq!(
            Maolan::kind_from_json(&Value::String("MIDI".to_string())),
            Some(Kind::MIDI)
        );
    }

    #[test]
    #[cfg(all(unix, not(target_os = "macos")))]
    fn plugin_node_from_json_with_runtime_nodes_maps_only_matching_runtime_formats() {
        let runtime_nodes = vec![
            PluginGraphNode::Lv2PluginInstance(10),
            PluginGraphNode::ClapPluginInstance(11),
        ];

        assert_eq!(
            Maolan::plugin_node_from_json_with_runtime_nodes(
                &json!({"type": "plugin", "plugin_index": 0}),
                &runtime_nodes
            ),
            Some(PluginGraphNode::Lv2PluginInstance(10))
        );
        assert_eq!(
            Maolan::plugin_node_from_json_with_runtime_nodes(
                &json!({"type": "clap_plugin", "plugin_index": 1}),
                &runtime_nodes
            ),
            Some(PluginGraphNode::ClapPluginInstance(11))
        );
        assert_eq!(
            Maolan::plugin_node_from_json_with_runtime_nodes(
                &json!({"type": "plugin", "plugin_index": 1}),
                &runtime_nodes
            ),
            None
        );
        assert_eq!(
            Maolan::plugin_node_from_json_with_runtime_nodes(
                &json!({"type": "clap_plugin", "plugin_index": 0}),
                &runtime_nodes
            ),
            None
        );
        assert_eq!(
            Maolan::plugin_node_from_json_with_runtime_nodes(&json!("TrackInput"), &runtime_nodes),
            Some(PluginGraphNode::TrackInput)
        );
        assert_eq!(
            Maolan::plugin_node_from_json_with_runtime_nodes(&json!("TrackOutput"), &runtime_nodes),
            Some(PluginGraphNode::TrackOutput)
        );
    }

    #[test]
    #[cfg(unix)]
    fn plugin_node_to_json_serializes_track_and_plugin_nodes() {
        let mut id_to_index = std::collections::HashMap::new();
        id_to_index.insert(7usize, 2usize);

        assert_eq!(
            Maolan::plugin_node_to_json(&PluginGraphNode::TrackInput, &id_to_index),
            Some(json!({"type":"track_input"}))
        );
        assert_eq!(
            Maolan::plugin_node_to_json(&PluginGraphNode::TrackOutput, &id_to_index),
            Some(json!({"type":"track_output"}))
        );
        #[cfg(all(unix, not(target_os = "macos")))]
        assert_eq!(
            Maolan::plugin_node_to_json(&PluginGraphNode::Lv2PluginInstance(7), &id_to_index),
            Some(json!({"type":"plugin","plugin_index":2}))
        );
        assert_eq!(
            Maolan::plugin_node_to_json(&PluginGraphNode::Vst3PluginInstance(7), &id_to_index),
            Some(json!({"type":"vst3_plugin","plugin_index":2}))
        );
        assert_eq!(
            Maolan::plugin_node_to_json(&PluginGraphNode::ClapPluginInstance(7), &id_to_index),
            Some(json!({"type":"clap_plugin","plugin_index":2}))
        );
    }

    #[test]
    fn zoom_slider_roundtrip_preserves_visible_bars() {
        for visible_bars in [0.25, 1.0, 2.0, 4.0, 16.0, 64.0, 256.0] {
            let slider = visible_bars_to_zoom_slider(visible_bars);
            let roundtrip = zoom_slider_to_visible_bars(slider);
            assert!((roundtrip - visible_bars).abs() < 0.001);
        }
    }

    #[test]
    fn zoom_slider_midpoint_is_geometric_midpoint() {
        let midpoint = zoom_slider_to_visible_bars(0.5);
        assert!((midpoint - 8.0).abs() < 0.001);
    }

    #[test]
    fn session_save_and_load_roundtrip_preserves_arrangement_zoom() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let session_root = std::env::temp_dir().join(format!("maolan_zoom_session_{unique}"));

        let app = Maolan {
            zoom_visible_bars: 6.5,
            ..Maolan::default()
        };
        app.save(session_root.to_string_lossy().to_string())
            .expect("save session");

        let mut restored = Maolan {
            zoom_visible_bars: 42.0,
            ..Maolan::default()
        };
        let _ = restored
            .load(session_root.to_string_lossy().to_string())
            .expect("load session");

        assert!((restored.zoom_visible_bars - 6.5).abs() < f32::EPSILON);

        fs::remove_dir_all(&session_root).expect("cleanup temp session");
    }

    #[test]
    fn session_load_without_saved_arrangement_zoom_keeps_current_zoom() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let session_root =
            std::env::temp_dir().join(format!("maolan_zoom_session_compat_{unique}"));

        let app = Maolan {
            zoom_visible_bars: 5.0,
            ..Maolan::default()
        };
        app.save(session_root.to_string_lossy().to_string())
            .expect("save session");

        let session_path = session_root.join("session.json");
        let mut session: Value =
            serde_json::from_reader(File::open(&session_path).expect("open saved session"))
                .expect("parse saved session");
        session["ui"]
            .as_object_mut()
            .expect("ui object")
            .remove("zoom_visible_bars");
        serde_json::to_writer_pretty(
            File::create(&session_path).expect("rewrite session"),
            &session,
        )
        .expect("write compatibility session");

        let mut restored = Maolan {
            zoom_visible_bars: 42.0,
            ..Maolan::default()
        };
        let _ = restored
            .load(session_root.to_string_lossy().to_string())
            .expect("load session");

        assert!((restored.zoom_visible_bars - 42.0).abs() < f32::EPSILON);

        fs::remove_dir_all(&session_root).expect("cleanup temp session");
    }

    #[test]
    fn session_save_includes_per_track_midi_editor_view_mode() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let session_root =
            std::env::temp_dir().join(format!("maolan_midi_view_mode_save_{unique}"));

        let app = Maolan {
            ..Maolan::default()
        };
        {
            let mut state = app.state.blocking_write();
            let mut track = crate::state::Track::new("Drums".to_string(), 0.0, 2, 2, 1, 1);
            track.midi.editor_view_mode = crate::message::MidiEditorViewMode::DrumGrid;
            state.tracks.push(track);
        }
        app.save(session_root.to_string_lossy().to_string())
            .expect("save session");

        let session_path = session_root.join("session.json");
        let session: serde_json::Value =
            serde_json::from_reader(File::open(&session_path).expect("open saved session"))
                .expect("parse saved session");
        let tracks = session["tracks"].as_array().expect("tracks array");
        let drum_track = tracks
            .iter()
            .find(|t| t["name"] == "Drums")
            .expect("drums track");
        assert_eq!(
            drum_track["midi"]["editor_view_mode"],
            serde_json::json!("drumgrid")
        );

        fs::remove_dir_all(&session_root).expect("cleanup temp session");
    }

    #[test]
    fn session_load_populates_pending_track_midi_editor_view_mode() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let session_root =
            std::env::temp_dir().join(format!("maolan_midi_view_mode_load_{unique}"));

        let session = serde_json::json!({
            "tracks": [{
                "name": "Drums",
                "level": 0.0,
                "balance": 0.0,
                "armed": false,
                "muted": false,
                "phase_inverted": false,
                "soloed": false,
                "input_monitor": false,
                "disk_monitor": true,
                "frozen": false,
                "height": 100.0,
                "audio": { "clips": [], "ins": 2, "outs": 2 },
                "midi": { "clips": [], "ins": 1, "outs": 1, "editor_view_mode": "drumgrid" },
                "position": { "x": 0.0, "y": 0.0 },
                "automation_mode": "read"
            }],
            "connections": [],
            "transport": {
                "loop_range_samples": null,
                "loop_enabled": false,
                "punch_range_samples": null,
                "punch_enabled": false,
                "sample_rate_hz": 48000,
                "tempo": 120.0,
                "time_signature_num": 4,
                "time_signature_denom": 4,
                "tempo_points": [{"sample": 0, "bpm": 120.0}],
                "time_signature_points": [{"sample": 0, "numerator": 4, "denominator": 4}]
            },
            "ui": {
                "tracks_width": 200.0,
                "mixer_height": 300.0,
                "zoom_visible_bars": 8.0,
                "snap_mode": "bar",
                "midi_snap_mode": "bar"
            },
            "export": {
                "sample_rate_hz": 48000,
                "format_wav": true,
                "format_mp3": false,
                "format_ogg": false,
                "format_flac": false,
                "bit_depth": "int24",
                "mp3_mode": "cbr",
                "mp3_bitrate_kbps": 320,
                "ogg_quality_input": "5",
                "render_mode": "mixdown",
                "hw_out_ports": [],
                "realtime_fallback": false,
                "normalize": false,
                "normalize_mode": "peak",
                "normalize_dbfs_input": "-1.0",
                "normalize_lufs_input": "-14.0",
                "normalize_dbtp_input": "-1.0",
                "normalize_tp_limiter": false,
                "master_limiter": false,
                "master_limiter_ceiling_input": "-1.0"
            },
            "midi_learn_global": {
                "play_pause": null,
                "stop": null,
                "record_toggle": null
            }
        });

        fs::create_dir_all(&session_root).expect("create session dir");
        let session_path = session_root.join("session.json");
        serde_json::to_writer_pretty(
            File::create(&session_path).expect("create session file"),
            &session,
        )
        .expect("write session");

        let mut restored = Maolan {
            ..Maolan::default()
        };
        let _ = restored
            .load(session_root.to_string_lossy().to_string())
            .expect("load session");

        assert!(
            matches!(
                restored.pending_track_midi_editor_view_mode.get("Drums"),
                Some(crate::message::MidiEditorViewMode::DrumGrid)
            ),
            "expected DrumGrid in pending map, got {:?}",
            restored.pending_track_midi_editor_view_mode.get("Drums")
        );

        fs::remove_dir_all(&session_root).expect("cleanup temp session");
    }

    #[test]
    fn cleanup_targets_include_pitchmap_sidecars_only() {
        assert!(Maolan::is_cleanup_target_rel("audio/clip_pitchmap.txt"));
        assert!(Maolan::is_cleanup_target_rel("audio/clip_pitchmap_001.txt"));
        assert!(Maolan::is_cleanup_target_rel("audio/song.wav"));
        assert!(Maolan::is_cleanup_target_rel("midi/pattern.midi"));
        assert!(!Maolan::is_cleanup_target_rel("audio/notes.txt"));
        assert!(!Maolan::is_cleanup_target_rel("audio/../notes.txt"));
        assert!(!Maolan::is_cleanup_target_rel("pitch/cache.txt"));
    }

    #[test]
    fn normalize_session_media_rel_rejects_unsafe_paths_and_normalizes_current_dir() {
        assert_eq!(
            Maolan::normalize_session_media_rel("./audio/clip.wav").as_deref(),
            Some("audio/clip.wav")
        );
        assert_eq!(
            Maolan::normalize_session_media_rel("peaks/clip.json").as_deref(),
            Some("peaks/clip.json")
        );
        assert!(Maolan::normalize_session_media_rel("/tmp/clip.wav").is_none());
        assert!(Maolan::normalize_session_media_rel("../audio/clip.wav").is_none());
    }

    #[test]
    fn file_extension_lower_normalizes_case_and_trims_dot() {
        assert_eq!(
            Maolan::file_extension_lower(std::path::Path::new("Song.MP3")).as_deref(),
            Some("mp3")
        );
        assert_eq!(
            Maolan::file_extension_lower(std::path::Path::new("clip..WAV")).as_deref(),
            Some("wav")
        );
        assert!(Maolan::file_extension_lower(std::path::Path::new("no_extension")).is_none());
    }

    #[test]
    fn import_path_helpers_accept_case_insensitive_extensions() {
        assert!(Maolan::is_import_audio_path(std::path::Path::new(
            "take.FLAC"
        )));
        assert!(Maolan::is_import_audio_path(std::path::Path::new(
            "take.Mp3"
        )));
        assert!(Maolan::is_import_midi_path(std::path::Path::new(
            "pattern.MID"
        )));
        assert!(Maolan::is_import_midi_path(std::path::Path::new(
            "pattern.Midi"
        )));
        assert!(!Maolan::is_import_audio_path(std::path::Path::new(
            "take.txt"
        )));
        assert!(!Maolan::is_import_midi_path(std::path::Path::new(
            "pattern.txt"
        )));
    }

    #[test]
    fn import_track_base_name_trims_and_sanitizes_file_stem() {
        assert_eq!(
            Maolan::import_track_base_name(std::path::Path::new("  Lead Vox!!.wav  ")),
            "Lead_Vox__"
        );
        assert_eq!(
            Maolan::import_track_base_name(std::path::Path::new("   .wav")),
            "clip"
        );
    }

    #[test]
    fn unique_track_name_uses_first_available_numeric_suffix() {
        let mut used = HashSet::from([
            "Track".to_string(),
            "Track_2".to_string(),
            "Track_3".to_string(),
        ]);

        let unique = Maolan::unique_track_name("Track", &mut used);

        assert_eq!(unique, "Track_4");
        assert!(used.contains("Track_4"));
    }

    #[test]
    fn unique_import_rel_path_appends_numeric_suffix_when_name_exists() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let session_root = std::env::temp_dir().join(format!("maolan_test_import_{unique}"));
        fs::create_dir_all(session_root.join("audio")).expect("create temp audio dir");
        fs::write(session_root.join("audio/clip.wav"), b"").expect("seed existing import");

        let rel = Maolan::unique_import_rel_path(&session_root, "audio", "clip", "wav").unwrap();

        assert_eq!(rel, "audio/clip_001.wav");

        fs::remove_dir_all(&session_root).expect("cleanup temp session");
    }

    #[test]
    fn referenced_session_media_paths_include_indirect_and_frozen_assets() {
        let mut state = crate::state::StateData::default();
        let mut track = crate::state::Track::new("Track".to_string(), 0.0, 1, 1, 1, 1);
        let lead_pitch_cache = Maolan::pitch_correction_cache_rel("audio/lead_src.wav", 0, 0);
        let frozen_pitch_cache = Maolan::pitch_correction_cache_rel("audio/frozen_src.wav", 0, 0);
        track.audio.clips.push(crate::state::AudioClip {
            name: "audio/lead.wav".to_string(),
            pitch_correction_source_name: Some("audio/lead_src.wav".to_string()),
            peaks_file: Some("peaks/lead.json".to_string()),
            ..Default::default()
        });
        track.audio.clips.push(crate::state::AudioClip {
            name: "/tmp/external.wav".to_string(),
            ..Default::default()
        });
        track.midi.clips.push(crate::state::MIDIClip {
            name: "midi/pattern.mid".to_string(),
            ..Default::default()
        });
        track.frozen_audio_backup.push(crate::state::AudioClip {
            name: "audio/frozen.wav".to_string(),
            pitch_correction_source_name: Some("audio/frozen_src.wav".to_string()),
            peaks_file: Some("peaks/frozen.json".to_string()),
            ..Default::default()
        });
        track.frozen_midi_backup.push(crate::state::MIDIClip {
            name: "midi/ghost.midi".to_string(),
            ..Default::default()
        });
        track.frozen_render_clip = Some("audio/render.wav".to_string());
        state.tracks.push(track);

        let referenced = Maolan::referenced_session_media_paths(&state);

        assert!(referenced.contains("audio/lead.wav"));
        assert!(referenced.contains("audio/lead_src.wav"));
        assert!(referenced.contains("peaks/lead.json"));
        assert!(referenced.contains(&lead_pitch_cache));
        assert!(referenced.contains("midi/pattern.mid"));
        assert!(referenced.contains("audio/frozen.wav"));
        assert!(referenced.contains("audio/frozen_src.wav"));
        assert!(referenced.contains("peaks/frozen.json"));
        assert!(referenced.contains(&frozen_pitch_cache));
        assert!(referenced.contains("midi/ghost.midi"));
        assert!(referenced.contains("audio/render.wav"));
        assert!(!referenced.contains("/tmp/external.wav"));
    }

    #[test]
    fn referenced_session_media_paths_use_source_offset_and_length_for_pitch_cache_keys() {
        let mut state = crate::state::StateData::default();
        let mut track = crate::state::Track::new("Track".to_string(), 0.0, 1, 1, 1, 1);
        let expected_cache = Maolan::pitch_correction_cache_rel("audio/source.wav", 128, 4096);
        track.audio.clips.push(crate::state::AudioClip {
            name: "audio/rendered.wav".to_string(),
            offset: 12,
            length: 64,
            pitch_correction_source_name: Some("audio/source.wav".to_string()),
            pitch_correction_source_offset: Some(128),
            pitch_correction_source_length: Some(4096),
            ..Default::default()
        });
        state.tracks.push(track);

        let referenced = Maolan::referenced_session_media_paths(&state);

        assert!(referenced.contains("audio/rendered.wav"));
        assert!(referenced.contains("audio/source.wav"));
        assert!(referenced.contains(&expected_cache));
    }

    #[test]
    fn insert_referenced_session_media_path_ignores_unsafe_inputs() {
        let mut referenced = HashSet::new();

        Maolan::insert_referenced_session_media_path(&mut referenced, "/tmp/external.wav");
        Maolan::insert_referenced_session_media_path(&mut referenced, "../audio/clip.wav");
        Maolan::insert_referenced_session_media_path(&mut referenced, "audio/clip.wav");

        assert_eq!(referenced, HashSet::from(["audio/clip.wav".to_string()]));
    }

    #[test]
    fn pitch_correction_cache_rel_is_stable_and_changes_with_segment() {
        let a = Maolan::pitch_correction_cache_rel("audio/source.wav", 0, 1024);
        let b = Maolan::pitch_correction_cache_rel("audio/source.wav", 0, 1024);
        let c = Maolan::pitch_correction_cache_rel("audio/source.wav", 1, 1024);
        let d = Maolan::pitch_correction_cache_rel("audio/source.wav", 0, 2048);

        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, d);
    }

    #[test]
    fn collect_cleanup_candidate_files_recurses_and_skips_referenced_media() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let session_root = std::env::temp_dir().join(format!("maolan_test_cleanup_{unique}"));
        fs::create_dir_all(session_root.join("audio/nested")).expect("create nested audio dir");
        fs::create_dir_all(session_root.join("peaks")).expect("create peaks dir");
        fs::create_dir_all(session_root.join("pitch")).expect("create pitch dir");

        fs::write(session_root.join("audio/keep.wav"), b"").expect("write keep file");
        fs::write(session_root.join("audio/nested/drop_pitchmap.txt"), b"")
            .expect("write nested pitchmap");
        fs::write(session_root.join("peaks/drop.json"), b"").expect("write peak file");
        fs::write(session_root.join("pitch/drop.json"), b"").expect("write pitch cache");
        fs::write(session_root.join("audio/ignore.txt"), b"").expect("write ignored text");

        let referenced = HashSet::from(["audio/keep.wav".to_string()]);
        let mut out = Vec::new();
        Maolan::collect_cleanup_candidate_files(
            &session_root.join("audio"),
            &session_root,
            &referenced,
            &mut out,
        )
        .expect("scan audio");
        Maolan::collect_cleanup_candidate_files(
            &session_root.join("peaks"),
            &session_root,
            &referenced,
            &mut out,
        )
        .expect("scan peaks");
        Maolan::collect_cleanup_candidate_files(
            &session_root.join("pitch"),
            &session_root,
            &referenced,
            &mut out,
        )
        .expect("scan pitch");
        out.sort_by(|a, b| a.1.cmp(&b.1));

        let rels: Vec<_> = out.into_iter().map(|(_, rel)| rel).collect();
        assert_eq!(
            rels,
            vec![
                "audio/nested/drop_pitchmap.txt".to_string(),
                "peaks/drop.json".to_string(),
                "pitch/drop.json".to_string(),
            ]
        );

        fs::remove_dir_all(&session_root).expect("cleanup temp session");
    }

    #[test]
    fn import_prepared_audio_peaks_stores_precomputed_peaks_by_clip_key() {
        let mut app = Maolan::default();
        let peaks = Arc::new(vec![vec![[0.1_f32, 0.4_f32], [-0.2_f32, 0.3_f32]]]);

        let _ = app.update(Message::ImportPreparedAudioPeaks {
            track_name: "Track".to_string(),
            clip_name: "audio/import.wav".to_string(),
            start: 0,
            length: 256,
            offset: 0,
            peaks: peaks.clone(),
        });

        let key = Maolan::audio_clip_key("Track", "audio/import.wav", 0, 256, 0);
        assert_eq!(app.pending_precomputed_peaks.get(&key), Some(&peaks));
    }

    #[test]
    fn drain_audio_peak_updates_applies_chunks_and_clears_pending_rebuilds() {
        let _guard = AUDIO_PEAK_TEST_GUARD.lock().expect("lock audio peak test");
        let mut app = Maolan::default();
        let mut track = crate::state::Track::new("Track".to_string(), 0.0, 1, 1, 0, 0);
        track.audio.clips.push(crate::state::AudioClip {
            name: "audio/import.wav".to_string(),
            start: 0,
            length: 256,
            offset: 0,
            peaks: Arc::new(Vec::new()),
            ..Default::default()
        });
        app.state.blocking_write().tracks.push(track);

        let key = Maolan::audio_clip_key("Track", "audio/import.wav", 0, 256, 0);
        app.pending_peak_rebuilds.insert(key.clone());

        if let Ok(mut queue) = AUDIO_PEAK_UPDATES.lock() {
            queue.clear();
            queue.push(AudioPeakChunkUpdate {
                track_name: "Track".to_string(),
                clip_name: "audio/import.wav".to_string(),
                start: 0,
                length: 256,
                offset: 0,
                channels: 1,
                target_bins: 4,
                bin_start: 0,
                peaks: vec![vec![
                    [-0.8_f32, 0.1_f32],
                    [-0.4_f32, 0.3_f32],
                    [-0.2_f32, 0.6_f32],
                    [-0.1_f32, 0.9_f32],
                ]],
                done: false,
            });
            queue.push(AudioPeakChunkUpdate {
                track_name: "Track".to_string(),
                clip_name: "audio/import.wav".to_string(),
                start: 0,
                length: 256,
                offset: 0,
                channels: 1,
                target_bins: 4,
                bin_start: 0,
                peaks: Vec::new(),
                done: true,
            });
        }

        let _ = app.update(Message::DrainAudioPeakUpdates);

        let state = app.state.blocking_read();
        let clip = &state.tracks[0].audio.clips[0];
        assert_eq!(clip.peaks.len(), 1);
        assert_eq!(clip.peaks[0].len(), 4);
        assert_eq!(clip.peaks[0][0], [-0.8_f32, 0.1_f32]);
        assert_eq!(clip.peaks[0][3], [-0.1_f32, 0.9_f32]);
        drop(state);

        assert!(!app.pending_peak_rebuilds.contains(&key));
    }

    #[test]
    fn drain_audio_peak_updates_accumulates_multiple_chunks() {
        let _guard = AUDIO_PEAK_TEST_GUARD.lock().expect("lock audio peak test");
        let mut app = Maolan::default();
        let mut track = crate::state::Track::new("Track".to_string(), 0.0, 1, 1, 0, 0);
        track.audio.clips.push(crate::state::AudioClip {
            name: "audio/import.wav".to_string(),
            start: 0,
            length: 256,
            offset: 0,
            peaks: Arc::new(Vec::new()),
            ..Default::default()
        });
        app.state.blocking_write().tracks.push(track);

        if let Ok(mut queue) = AUDIO_PEAK_UPDATES.lock() {
            queue.clear();
            queue.push(AudioPeakChunkUpdate {
                track_name: "Track".to_string(),
                clip_name: "audio/import.wav".to_string(),
                start: 0,
                length: 256,
                offset: 0,
                channels: 1,
                target_bins: 4,
                bin_start: 0,
                peaks: vec![vec![[-0.8_f32, 0.1_f32], [-0.4_f32, 0.3_f32]]],
                done: false,
            });
        }
        let _ = app.update(Message::DrainAudioPeakUpdates);

        if let Ok(mut queue) = AUDIO_PEAK_UPDATES.lock() {
            queue.push(AudioPeakChunkUpdate {
                track_name: "Track".to_string(),
                clip_name: "audio/import.wav".to_string(),
                start: 0,
                length: 256,
                offset: 0,
                channels: 1,
                target_bins: 4,
                bin_start: 2,
                peaks: vec![vec![[-0.2_f32, 0.6_f32], [-0.1_f32, 0.9_f32]]],
                done: false,
            });
            queue.push(AudioPeakChunkUpdate {
                track_name: "Track".to_string(),
                clip_name: "audio/import.wav".to_string(),
                start: 0,
                length: 256,
                offset: 0,
                channels: 1,
                target_bins: 4,
                bin_start: 0,
                peaks: Vec::new(),
                done: true,
            });
        }
        let _ = app.update(Message::DrainAudioPeakUpdates);

        let state = app.state.blocking_read();
        let clip = &state.tracks[0].audio.clips[0];
        assert_eq!(
            clip.peaks.as_ref(),
            &vec![vec![
                [-0.8_f32, 0.1_f32],
                [-0.4_f32, 0.3_f32],
                [-0.2_f32, 0.6_f32],
                [-0.1_f32, 0.9_f32],
            ]]
        );
    }

    #[test]
    fn import_progress_only_finishes_on_last_file_at_full_progress() {
        let mut app = Maolan {
            import_in_progress: true,
            ..Maolan::default()
        };

        let _ = app.update(Message::ImportProgress {
            file_index: 1,
            total_files: 2,
            file_progress: 1.0,
            filename: "first.wav".to_string(),
            operation: None,
        });
        assert!(app.import_in_progress);

        let _ = app.update(Message::ImportProgress {
            file_index: 2,
            total_files: 2,
            file_progress: 0.5,
            filename: "second.wav".to_string(),
            operation: Some("Decoding".to_string()),
        });
        assert!(app.import_in_progress);

        let _ = app.update(Message::ImportProgress {
            file_index: 2,
            total_files: 2,
            file_progress: 1.0,
            filename: "second.wav".to_string(),
            operation: None,
        });
        assert!(!app.import_in_progress);
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn clip_plugin_snapshot_preserves_existing_vst3_state_blob() {
        let previous = json!({
            "plugins": [{
                "format": "VST3",
                "uri": "/tmp/test.vst3",
                "state": {"bytes": [1, 2, 3]}
            }],
            "connections": []
        });
        let plugins = vec![maolan_engine::message::PluginGraphPlugin {
            node: maolan_engine::message::PluginGraphNode::Vst3PluginInstance(0),
            instance_id: 0,
            format: "VST3".to_string(),
            uri: "/tmp/test.vst3".to_string(),
            plugin_id: "plugin-id".to_string(),
            name: "Test".to_string(),
            main_audio_inputs: 2,
            main_audio_outputs: 2,
            audio_inputs: 2,
            audio_outputs: 2,
            midi_inputs: 0,
            midi_outputs: 0,
            state: None,
        }];

        let snapshot = Maolan::plugin_graph_snapshot_to_json(Some(&previous), &plugins, &[]);
        assert_eq!(snapshot["plugins"][0]["state"], json!({"bytes": [1, 2, 3]}));
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn plugin_graph_saved_state_from_json_reads_plugin_slot_by_index() {
        let graph = json!({
            "plugins": [
                {
                    "format": "VST3",
                    "uri": "/plugins/one.vst3",
                    "state": {
                        "plugin_id": "one",
                        "component_state": [1, 2],
                        "controller_state": [3]
                    }
                },
                {
                    "format": "VST3",
                    "uri": "/plugins/two.vst3",
                    "state": {
                        "plugin_id": "two",
                        "component_state": [9],
                        "controller_state": []
                    }
                }
            ],
            "connections": []
        });

        let state = Maolan::plugin_graph_saved_state_from_json::<
            maolan_engine::vst3::Vst3PluginState,
        >(Some(&graph), 1)
        .expect("saved state");

        assert_eq!(state.plugin_id, "two");
        assert_eq!(state.component_state, vec![9]);
        assert!(state.controller_state.is_empty());
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn plugin_graph_json_with_saved_plugin_state_updates_only_target_plugin() {
        let graph = json!({
            "plugins": [
                {
                    "format": "VST3",
                    "uri": "/plugins/one.vst3",
                    "state": {
                        "plugin_id": "one",
                        "component_state": [1],
                        "controller_state": []
                    }
                },
                {
                    "format": "VST3",
                    "uri": "/plugins/two.vst3",
                    "state": {
                        "plugin_id": "two",
                        "component_state": [2],
                        "controller_state": []
                    }
                }
            ],
            "connections": []
        });

        let updated = Maolan::plugin_graph_json_with_saved_plugin_state(
            Some(&graph),
            1,
            json!({
                "plugin_id": "two",
                "component_state": [7, 8],
                "controller_state": [6]
            }),
        )
        .expect("updated graph");

        assert_eq!(
            updated["plugins"][0]["state"]["component_state"],
            json!([1])
        );
        assert_eq!(
            updated["plugins"][1]["state"]["component_state"],
            json!([7, 8])
        );
        assert_eq!(
            updated["plugins"][1]["state"]["controller_state"],
            json!([6])
        );
    }

    #[test]
    fn core_toggle_transport_requires_loaded_hw() {
        let mut app = Maolan::default();
        app.state.blocking_write().hw_loaded = false;

        let _ = app.update(Message::ToggleTransport);

        assert!(!app.playing);
        assert!(!app.paused);
    }

    #[test]
    fn core_toggle_transport_stops_active_playback_and_clears_preview() {
        let mut app = Maolan {
            playing: true,
            paused: false,
            recording_preview_start_sample: Some(12),
            recording_preview_sample: Some(24),
            ..Maolan::default()
        };
        app.state.blocking_write().hw_loaded = true;
        app.track_automation_runtime
            .insert("Track".to_string(), TrackAutomationRuntime::default());
        app.touch_automation_overrides
            .insert("Track".to_string(), HashMap::new());
        app.touch_active_keys
            .insert("Track".to_string(), HashSet::new());
        app.latch_automation_overrides
            .insert("Track".to_string(), HashMap::new());

        let _ = app.update(Message::ToggleTransport);

        assert!(!app.playing);
        assert!(!app.paused);
        assert!(app.recording_preview_start_sample.is_none());
        assert!(app.track_automation_runtime.is_empty());
        assert!(app.touch_automation_overrides.is_empty());
        assert!(app.touch_active_keys.is_empty());
        assert!(app.latch_automation_overrides.is_empty());
    }

    #[test]
    fn core_toggle_transport_starts_preview_when_record_armed() {
        let mut app = Maolan {
            record_armed: true,
            transport_samples: 128.0,
            ..Maolan::default()
        };
        app.state.blocking_write().hw_loaded = true;

        let _ = app.update(Message::ToggleTransport);

        assert!(app.playing);
        assert!(!app.paused);
        assert_eq!(app.recording_preview_start_sample, Some(128));
        assert_eq!(app.recording_preview_sample, Some(128));
    }

    #[test]
    fn core_toggle_loop_and_punch_require_ranges() {
        let mut app = Maolan::default();

        let _ = app.update(Message::ToggleLoop);
        let _ = app.update(Message::TogglePunch);
        assert!(!app.loop_enabled);
        assert!(!app.punch_enabled);

        app.loop_range_samples = Some((10, 20));
        app.punch_range_samples = Some((30, 40));
        let _ = app.update(Message::ToggleLoop);
        let _ = app.update(Message::TogglePunch);

        assert!(app.loop_enabled);
        assert!(app.punch_enabled);
    }

    #[test]
    fn simple_ui_mp3_toggle_respects_channel_limits() {
        let mut app = Maolan {
            export_render_mode: ExportRenderMode::Mixdown,
            export_hw_out_ports: BTreeSet::from([0, 1, 2]),
            ..Maolan::default()
        };

        let _ = app.update(Message::ExportFormatMp3Toggled(true));
        assert!(!app.export_format_mp3);

        app.export_hw_out_ports = BTreeSet::from([0, 1]);
        let _ = app.update(Message::ExportFormatMp3Toggled(true));
        assert!(app.export_format_mp3);
    }

    #[test]
    fn simple_ui_render_mode_disables_normalize_and_clamps_mp3() {
        let mut app = Maolan {
            export_format_mp3: true,
            export_normalize: true,
            ..Maolan::default()
        };
        let surround = crate::state::Track::new("Surround".to_string(), 0.0, 1, 4, 0, 0);
        {
            let mut state = app.state.blocking_write();
            state.selected.insert("Surround".to_string());
            state.tracks.push(surround);
        }

        let _ = app.update(Message::ExportRenderModeSelected(
            ExportRenderMode::StemsPostFader,
        ));
        assert!(!app.export_normalize);
        assert!(!app.export_format_mp3);
    }

    #[test]
    fn simple_ui_hw_settings_clamp_numeric_values() {
        let mut app = Maolan::default();

        let _ = app.update(Message::HWSampleRateChanged(0));
        let _ = app.update(Message::HWPeriodFramesChanged(3));
        let _ = app.update(Message::HWNPeriodsChanged(0));
        let _ = app.update(Message::HWSyncModeToggled(true));

        let state = app.state.blocking_read();
        assert_eq!(state.hw_sample_rate_hz, 1);
        assert_eq!(state.oss_period_frames, 16);
        assert_eq!(state.oss_nperiods, 1);
        assert!(state.oss_sync_mode);
    }

    #[test]
    fn track_selection_modifier_messages_require_loaded_hw() {
        let mut app = Maolan::default();
        app.state.blocking_write().hw_loaded = false;

        let _ = app.update(Message::ShiftPressed);
        assert!(!app.state.blocking_read().shift);

        app.state.blocking_write().hw_loaded = true;
        let _ = app.update(Message::ShiftPressed);
        assert!(app.state.blocking_read().shift);
        let _ = app.update(Message::CtrlPressed);
        assert!(app.state.blocking_read().ctrl);
    }

    #[test]
    fn track_selection_select_track_ctrl_adds_to_existing_selection() {
        let mut app = Maolan::default();
        {
            let mut state = app.state.blocking_write();
            state.ctrl = true;
            state.selected.insert("A".to_string());
            state.connection_view_selection =
                crate::state::ConnectionViewSelection::Tracks(HashSet::from(["A".to_string()]));
        }

        let _ = app.update(Message::SelectTrack("B".to_string()));

        let state = app.state.blocking_read();
        assert!(state.selected.contains("A"));
        assert!(state.selected.contains("B"));
        match &state.connection_view_selection {
            crate::state::ConnectionViewSelection::Tracks(set) => {
                assert!(set.contains("A"));
                assert!(set.contains("B"));
            }
            other => panic!("unexpected selection: {other:?}"),
        }
    }

    #[test]
    fn track_selection_double_click_schedules_open_plugins() {
        let mut app = Maolan::default();
        app.state.blocking_write().connections_last_track_click =
            Some(("Track".to_string(), Instant::now()));

        let _ = app.update(Message::SelectTrack("Track".to_string()));
        assert!(
            app.state
                .blocking_read()
                .connections_last_track_click
                .is_none()
        );
    }

    #[test]
    fn session_io_save_folder_selected_none_cancels_pending_exit() {
        let mut app = Maolan {
            pending_exit_after_save: true,
            ..Maolan::default()
        };

        let _ = app.update(Message::SaveFolderSelected(None));

        assert!(!app.pending_exit_after_save);
        assert_eq!(app.state.blocking_read().message, "Close cancelled");
    }

    #[test]
    fn session_io_record_folder_selected_none_clears_pending_record() {
        let mut app = Maolan {
            pending_record_after_save: true,
            ..Maolan::default()
        };

        let _ = app.update(Message::RecordFolderSelected(None));
        assert!(!app.pending_record_after_save);
    }

    #[test]
    fn session_io_open_folder_selected_without_autosave_sets_loading_state() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("maolan_open_session_{unique}"));
        fs::create_dir_all(&path).expect("create session dir");
        let mut app = Maolan {
            recording_preview_start_sample: Some(1),
            recording_preview_sample: Some(2),
            ..Maolan::default()
        };

        let _ = app.update(Message::OpenFolderSelected(Some(path.clone())));
        assert_eq!(app.session_dir.as_ref(), Some(&path));
        assert_eq!(app.state.blocking_read().message, "Loading session...");
        assert!(app.recording_preview_start_sample.is_none());

        fs::remove_dir_all(path).expect("cleanup session dir");
    }

    #[test]
    fn transport_set_loop_and_punch_range_normalize_invalid_ranges() {
        let mut app = Maolan::default();

        let _ = app.update(Message::SetLoopRange(Some((20, 10))));
        let _ = app.update(Message::SetPunchRange(Some((40, 10))));
        assert!(app.loop_range_samples.is_none());
        assert!(app.punch_range_samples.is_none());
        assert!(!app.loop_enabled);
        assert!(!app.punch_enabled);

        let _ = app.update(Message::SetLoopRange(Some((10, 20))));
        let _ = app.update(Message::SetPunchRange(Some((30, 40))));
        assert_eq!(app.loop_range_samples, Some((10, 20)));
        assert_eq!(app.punch_range_samples, Some((30, 40)));
        assert!(app.loop_enabled);
        assert!(app.punch_enabled);
    }

    #[test]
    fn transport_playback_tick_updates_tempo_and_time_signature_inputs() {
        let mut app = Maolan {
            last_sent_tempo_bpm: None,
            last_sent_time_signature: None,
            ..Maolan::default()
        };

        let _ = app.update(Message::PlaybackTick);

        assert_eq!(app.tempo_input, "120.00");
        assert_eq!(app.time_signature_num_input, "4");
        assert_eq!(app.time_signature_denom_input, "4");
        assert_eq!(app.last_sent_tempo_bpm, Some(120.0));
        assert_eq!(app.last_sent_time_signature, Some((4, 4)));
    }

    #[test]
    fn confirm_close_cancel_clears_modal_and_resets_exit_flag() {
        let mut app = Maolan {
            modal: Some(Show::UnsavedChanges),
            pending_exit_after_save: true,
            ..Maolan::default()
        };

        let _ = app.update(Message::ConfirmCloseCancel);

        assert!(app.modal.is_none());
        assert!(!app.pending_exit_after_save);
        assert_eq!(app.state.blocking_read().message, "Close cancelled");
    }

    #[test]
    fn escape_pressed_closes_add_track_modal_and_marker_dialog() {
        let mut app = Maolan {
            modal: Some(Show::AddTrack),
            ..Maolan::default()
        };
        app.state.blocking_write().track_marker_dialog = Some(crate::state::TrackMarkerDialog {
            track_name: "Track".to_string(),
            sample: 10,
            marker_index: None,
            name: "Marker".to_string(),
        });

        let _ = app.update(Message::EscapePressed);
        assert!(app.modal.is_none());
        assert!(app.state.blocking_read().track_marker_dialog.is_some());

        let _ = app.update(Message::EscapePressed);
        assert!(app.state.blocking_read().track_marker_dialog.is_none());
    }

    #[test]
    fn set_snap_mode_updates_state() {
        let mut app = Maolan::default();

        let _ = app.update(Message::SetSnapMode(SnapMode::Sixteenth));

        assert_eq!(app.snap_mode, SnapMode::Sixteenth);
    }

    #[test]
    fn set_midi_snap_mode_updates_state() {
        let mut app = Maolan::default();

        let _ = app.update(Message::SetMidiSnapMode(SnapMode::Beat));

        assert_eq!(app.midi_snap_mode, SnapMode::Beat);
    }

    #[test]
    fn recording_preview_tick_tracks_current_sample_and_respects_punch() {
        let mut app = Maolan {
            playing: true,
            record_armed: true,
            transport_samples: 96.0,
            recording_preview_start_sample: Some(0),
            ..Maolan::default()
        };

        let _ = app.update(Message::RecordingPreviewTick);
        assert_eq!(app.recording_preview_sample, Some(96));

        app.punch_enabled = true;
        app.punch_range_samples = Some((100, 120));
        let _ = app.update(Message::RecordingPreviewTick);
        assert!(app.recording_preview_sample.is_none());
    }

    #[test]
    fn recording_preview_peaks_tick_collects_armed_track_meter_values() {
        let mut app = Maolan {
            playing: true,
            record_armed: true,
            transport_samples: 64.0,
            recording_preview_start_sample: Some(0),
            ..Maolan::default()
        };
        let mut track = crate::state::Track::new("Track".to_string(), 0.0, 1, 2, 0, 0);
        track.armed = true;
        track.meter_out_db = vec![-6.0, -90.0];
        app.state.blocking_write().tracks.push(track);

        let _ = app.update(Message::RecordingPreviewPeaksTick);

        let peaks = app
            .recording_preview_peaks
            .get("Track")
            .expect("preview peaks");
        assert_eq!(peaks.len(), 2);
        assert_eq!(peaks[0].len(), 1);
        assert_eq!(peaks[1].len(), 1);
        assert!(peaks[0][0][1] > 0.49 && peaks[0][0][1] < 0.51);
        assert_eq!(peaks[1][0], [0.0, 0.0]);
    }

    #[test]
    fn zoom_and_scroll_messages_clamp_and_update_positions() {
        let mut app = Maolan {
            size: Size::new(800.0, 600.0),
            zoom_visible_bars: 8.0,
            editor_scroll_origin_samples: 10_000.0,
            ..Maolan::default()
        };

        let _ = app.update(Message::ZoomSliderChanged(0.0));
        assert_eq!(app.zoom_visible_bars, MIN_ZOOM_VISIBLE_BARS);
        assert!(app.editor_scroll_x >= 0.0 && app.editor_scroll_x <= 1.0);

        let _ = app.update(Message::EditorScrollXChanged(2.0));
        assert_eq!(app.editor_scroll_x, 1.0);

        let _ = app.update(Message::EditorScrollYChanged(-1.0));
        assert_eq!(app.editor_scroll_y, 0.0);

        let _ = app.update(Message::MixerScrollXChanged(0.25));
        assert_eq!(app.mixer_scroll_x, 0.25);
    }

    #[test]
    fn piano_scroll_and_zoom_messages_update_state() {
        let mut app = Maolan::default();

        let _ = app.update(Message::PianoZoomXChanged(3.0));
        let _ = app.update(Message::PianoZoomYChanged(2.0));
        let _ = app.update(Message::PianoScrollChanged { x: 1.5, y: -1.0 });
        let _ = app.update(Message::PianoScrollXChanged(0.25));
        let _ = app.update(Message::PianoScrollYChanged(0.75));

        let state = app.state.blocking_read();
        assert_eq!(state.piano_zoom_x, 3.0);
        assert_eq!(state.piano_zoom_y, 2.0);
        assert_eq!(state.piano_scroll_x, 0.25);
        assert_eq!(state.piano_scroll_y, 0.75);
    }

    #[test]
    fn piano_controller_selector_messages_switch_lane_and_sysex_panel() {
        let mut app = Maolan::default();

        let _ = app.update(Message::PianoControllerLaneSelected(
            crate::message::PianoControllerLane::SysEx,
        ));
        {
            let state = app.state.blocking_read();
            assert_eq!(
                state.piano_controller_lane,
                crate::message::PianoControllerLane::SysEx
            );
            assert!(state.piano_sysex_panel_open);
        }

        let _ = app.update(Message::PianoControllerKindSelected(74));
        let _ = app.update(Message::PianoVelocityKindSelected(
            crate::message::PianoVelocityKind::ReleaseVelocity,
        ));
        let _ = app.update(Message::PianoRpnKindSelected(
            crate::message::PianoRpnKind::FineTuning,
        ));
        let _ = app.update(Message::PianoNrpnKindSelected(
            crate::message::PianoNrpnKind::VibratoDepth,
        ));

        let state = app.state.blocking_read();
        assert_eq!(
            state.piano_controller_lane,
            crate::message::PianoControllerLane::Nrpn
        );
        assert_eq!(state.piano_controller_kind, 74);
        assert_eq!(
            state.piano_velocity_kind,
            crate::message::PianoVelocityKind::ReleaseVelocity
        );
        assert_eq!(
            state.piano_rpn_kind,
            crate::message::PianoRpnKind::FineTuning
        );
        assert_eq!(
            state.piano_nrpn_kind,
            crate::message::PianoNrpnKind::VibratoDepth
        );
        assert!(!state.piano_sysex_panel_open);
    }

    #[test]
    fn transport_record_toggle_arms_when_session_exists_and_disarms_when_already_armed() {
        let mut app = Maolan {
            session_dir: Some(PathBuf::from("/tmp/session")),
            playing: true,
            ..Maolan::default()
        };

        let _ = app.update(Message::TransportRecordToggle);
        assert!(app.record_armed);
        assert_eq!(app.recording_preview_start_sample, Some(0));

        let _ = app.update(Message::TransportRecordToggle);
        assert!(!app.record_armed);
        assert!(!app.pending_record_after_save);
        assert!(app.recording_preview_start_sample.is_none());
    }

    #[test]
    fn transport_record_toggle_without_session_sets_pending_record() {
        let mut app = Maolan::default();

        let _ = app.update(Message::TransportRecordToggle);

        assert!(app.pending_record_after_save);
        assert!(!app.record_armed);
    }

    #[cfg(unix)]
    #[test]
    fn resolve_maolan_burn_binary_path_prefers_sibling_binary() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("generate-path-test-{unique}"));
        let bin_dir = root.join("target").join("debug");
        fs::create_dir_all(&bin_dir).expect("bin dir");
        let current_exe = bin_dir.join("maolan");
        fs::write(bin_dir.join("maolan-generate"), []).expect("touch sibling");

        let resolved = Maolan::resolve_maolan_burn_binary_path(Some(&current_exe), &root);

        assert_eq!(resolved, Some(bin_dir.join("maolan-generate")));
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn resolve_maolan_burn_binary_path_does_not_probe_parent_directory() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("generate-parent-test-{unique}"));
        let bin_dir = root.join("target").join("debug");
        fs::create_dir_all(&bin_dir).expect("bin dir");
        let current_exe = bin_dir.join("maolan");
        let parent_candidate = root.join("target").join("maolan-generate");
        fs::write(&parent_candidate, []).expect("touch parent candidate");

        let resolved = Maolan::resolve_maolan_burn_binary_path(Some(&current_exe), &root);

        assert_eq!(resolved, None);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn gui_generate_audio_defaults_match_generate_defaults() {
        let app = Maolan::default();
        assert_eq!(
            app.generate_audio_cfg_scale_input,
            maolan_generate::DEFAULT_CFG_SCALE.to_string()
        );
    }

    #[test]
    fn generate_audio_tags_with_timing_appends_bpm_and_time_signature() {
        assert_eq!(
            Maolan::generate_audio_tags_with_timing("rock, upbeat", 127.6, 7, 8),
            "rock,upbeat,128bpm,7/8 tempo signature"
        );
        assert_eq!(
            Maolan::generate_audio_tags_with_timing("", 120.0, 4, 4),
            "120bpm,4/4 tempo signature"
        );
    }

    #[test]
    fn generate_audio_cfg_scale_input_preserves_decimal_text() {
        let mut app = Maolan::default();
        let _ = app.update(Message::GenerateAudioCfgScaleInput("6.1".to_string()));
        assert_eq!(app.generate_audio_cfg_scale_input, "6.1");
    }

    #[test]
    fn export_format_extension_returns_correct_extensions() {
        // Extensions are returned without the leading dot
        assert_eq!(Maolan::export_format_extension(ExportFormat::Wav), "wav");
        assert_eq!(Maolan::export_format_extension(ExportFormat::Mp3), "mp3");
        assert_eq!(Maolan::export_format_extension(ExportFormat::Ogg), "ogg");
        assert_eq!(Maolan::export_format_extension(ExportFormat::Flac), "flac");
    }

    #[test]
    fn sanitize_export_component_replaces_special_chars() {
        assert_eq!(Maolan::sanitize_export_component("test/file"), "test_file");
        assert_eq!(Maolan::sanitize_export_component("test:file"), "test_file");
        assert_eq!(Maolan::sanitize_export_component("test file"), "test_file");
    }

    #[test]
    fn sanitize_peak_file_component_replaces_special_chars() {
        assert_eq!(
            Maolan::sanitize_peak_file_component("track/name"),
            "track_name"
        );
        assert_eq!(Maolan::sanitize_peak_file_component("clip.wav"), "clip_wav");
    }

    #[test]
    fn build_peak_file_rel_creates_correct_path() {
        let rel = Maolan::build_peak_file_rel("TestTrack", 5, "audio.wav");
        assert!(rel.contains("TestTrack"));
        assert!(rel.contains("5"));
        assert!(rel.ends_with(".json"));
    }

    #[test]
    fn audio_clip_key_is_deterministic() {
        let key1 = Maolan::audio_clip_key("track1", "clip.wav", 100, 500, 0);
        let key2 = Maolan::audio_clip_key("track1", "clip.wav", 100, 500, 0);
        assert_eq!(key1, key2);
    }

    #[test]
    fn audio_clip_key_differs_with_params() {
        let key1 = Maolan::audio_clip_key("track1", "clip.wav", 100, 500, 0);
        let key2 = Maolan::audio_clip_key("track1", "clip.wav", 200, 500, 0);
        assert_ne!(key1, key2);
    }

    #[test]
    fn export_base_path_removes_extension() {
        let path = PathBuf::from("/tmp/session/test.wav");
        let base = Maolan::export_base_path(path);
        assert_eq!(base, PathBuf::from("/tmp/session/test"));
    }

    #[test]
    fn sanitize_generated_track_base_name_cleans_input() {
        let sanitized = Maolan::sanitize_generated_track_base_name("My Track with spaces");
        assert!(sanitized.contains("My"));
        assert!(!sanitized.contains(' '));
    }

    #[test]
    fn samples_per_beat_calculation() {
        let app = Maolan::default();
        let spp = app.samples_per_beat();
        // At 120 BPM and 48kHz, samples per beat = 48000 * 60 / 120 = 24000
        assert!(spp > 0.0);
    }

    #[test]
    fn samples_per_bar_calculation() {
        let app = Maolan::default();
        let spb = app.samples_per_bar();
        // At 120 BPM, 4/4 time, 48kHz, samples per bar = 48000 * 60 / 120 * 4 = 96000
        assert!(spb > 0.0);
    }

    #[test]
    fn zoom_slider_visible_bars_roundtrip() {
        // Test that visible_bars_to_zoom_slider and zoom_slider_to_visible_bars are consistent
        for i in 0..=20 {
            let position = i as f32 / 20.0;
            let visible = zoom_slider_to_visible_bars(position);
            let back = visible_bars_to_zoom_slider(visible);
            assert!(
                (back - position).abs() < 0.001,
                "Roundtrip failed at position {}",
                position
            );
        }
    }

    #[test]
    fn zoom_slider_min_max() {
        assert_eq!(zoom_slider_to_visible_bars(0.0), MIN_ZOOM_VISIBLE_BARS);
        assert_eq!(zoom_slider_to_visible_bars(1.0), MAX_ZOOM_VISIBLE_BARS);
    }

    #[test]
    fn pixels_per_sample_calculation() {
        let app = Maolan::default();
        let pps = app.pixels_per_sample();
        assert!(pps > 0.0);
    }

    #[test]
    fn editor_width_px_calculation() {
        let app = Maolan::default();
        let width = app.editor_width_px();
        assert!(width >= 0.0);
    }

    #[test]
    fn tracks_width_px_calculation() {
        let app = Maolan::default();
        let width = app.tracks_width_px();
        assert!(width >= 0.0);
    }

    #[test]
    fn snap_interval_samples_returns_positive_value() {
        let app = Maolan::default();
        let interval = app.snap_interval_samples();
        // Should return a positive number
        assert!(interval > 0);
    }

    #[test]
    fn snap_sample_to_bar_returns_valid_sample() {
        let app = Maolan::default();
        let sample = app.snap_sample_to_bar(1000.0);
        assert!(sample < 10000); // Should snap to a reasonable value
    }

    #[test]
    fn beat_pixels_calculation() {
        let app = Maolan::default();
        let bp = app.beat_pixels();
        assert!(bp > 0.0);
    }
}
