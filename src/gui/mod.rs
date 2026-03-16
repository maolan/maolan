mod platform;
mod session;
mod subscriptions;
mod update;
mod view;

#[cfg(all(unix, not(target_os = "macos")))]
use crate::plugins::lv2::GuiLv2UiHost;
use crate::{
    add_track, clip_rename, config, connections,
    consts::gui as gui_consts,
    consts::gui_mod::{
        AUDIO_BIT_DEPTH_OPTIONS, BINS_PER_UPDATE, CHUNK_FRAMES, CLIENT, MAX_PEAK_BINS,
        MAX_RECENT_SESSIONS, STANDARD_EXPORT_SAMPLE_RATES,
    },
    consts::message_lists::{
        EXPORT_BIT_DEPTH_ALL, EXPORT_MP3_MODE_ALL, EXPORT_NORMALIZE_MODE_ALL,
        EXPORT_RENDER_MODE_ALL, SNAP_MODE_ALL,
    },
    hw, menu,
    message::{
        DraggedClip, ExportBitDepth, ExportFormat, ExportMp3Mode, ExportNormalizeMode,
        ExportRenderMode, Message, PluginFormat, PreferencesDeviceOption, Show, SnapMode,
    },
    platform_caps,
    plugins::{clap::GuiClapUiHost, vst3::GuiVst3UiHost},
    state::{
        AudioClip, ClipPeaks, MIDIClip, MidiClipPreviewMap, PianoControllerPoint, PianoNote,
        PianoSysExPoint, State, StateData,
    },
    template_save, toolbar, track_group, track_marker, track_rename, track_template_save,
    workspace,
};
use ebur128::{EbuR128, Mode as LoudnessMode};
use flacenc::component::BitRepr;
use flacenc::error::Verify;
use iced::{
    Length, Size, Task,
    widget::{button, checkbox, column, container, pick_list, row, scrollable, text, text_input},
};
#[cfg(unix)]
use maolan_engine::kind::Kind;
use maolan_engine::message::{Action, Message as EngineMessage};
use midly::{
    Format, Header, MetaMessage, Smf, Timing, TrackEvent, TrackEventKind,
    num::{u15, u24, u28},
};
use mp3lame_encoder::{
    Bitrate as Mp3Bitrate, Builder as Mp3Builder, FlushNoGap, InterleavedPcm, Quality as Mp3Quality,
};
use serde_json::Value;
#[cfg(unix)]
use serde_json::json;
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs::{self, File},
    io::{self, BufReader},
    num::{NonZeroU8, NonZeroU32},
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex},
    time::{Duration, Instant},
};
use symphonia::core::{
    audio::SampleBuffer, codecs::DecoderOptions, errors::Error as SymphoniaError,
    formats::FormatOptions, io::MediaSourceStream, meta::MetadataOptions, probe::Hint,
};
use tokio::sync::RwLock;
use tracing::error;
use vorbis_rs::{VorbisBitrateManagementStrategy, VorbisEncoderBuilder};
use wavers::Wav;

pub(crate) use gui_consts::{MIN_CLIP_WIDTH_PX, PREF_DEVICE_AUTO_ID};
type TickToSampleFn = dyn Fn(u64) -> usize + Send + Sync;
type MidiTickMap = (Box<TickToSampleFn>, u64, u64);
type PianoParseResult = (
    Vec<PianoNote>,
    Vec<PianoControllerPoint>,
    Vec<PianoSysExPoint>,
    usize,
);
type TrackFreezeRestore = (Vec<AudioClip>, Vec<MIDIClip>, Option<String>);

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
    default_export_sample_rate_hz: u32,
    default_snap_mode: SnapMode,
    default_audio_bit_depth: usize,
    default_output_device_id: Option<String>,
    default_input_device_id: Option<String>,
    recent_session_paths: Vec<String>,
}

impl Default for AppPreferences {
    fn default() -> Self {
        Self {
            default_export_sample_rate_hz: 48_000,
            default_snap_mode: SnapMode::Bar,
            default_audio_bit_depth: 32,
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
struct PendingVst3UiOpen {
    track_name: String,
    instance_id: usize,
    plugin_path: String,
    plugin_name: String,
    plugin_id: String,
    audio_inputs: usize,
    audio_outputs: usize,
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
    #[cfg(target_os = "macos")]
    pending_save_vst3_states: HashSet<(String, usize)>,
    pending_save_is_template: bool,
    pending_peak_file_loads: HashMap<AudioClipKey, PathBuf>,
    pending_peak_rebuilds: HashSet<AudioClipKey>,
    pending_track_freeze_restore: HashMap<String, TrackFreezeRestore>,
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
    zoom_visible_bars: f32,
    editor_scroll_x: f32,
    editor_scroll_y: f32,
    mixer_scroll_x: f32,
    tracks_resize_hovered: bool,
    mixer_resize_hovered: bool,
    tracks_visible: bool,
    editor_visible: bool,
    mixer_visible: bool,
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
    prefs_export_sample_rate_hz: u32,
    prefs_snap_mode: SnapMode,
    prefs_audio_bit_depth: usize,
    prefs_default_output_device_id: Option<String>,
    prefs_default_input_device_id: Option<String>,
}

fn load_preferences() -> AppPreferences {
    let cfg = config::Config::load().unwrap_or_default();
    AppPreferences {
        default_export_sample_rate_hz: cfg.default_export_sample_rate_hz,
        default_snap_mode: cfg.default_snap_mode,
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

impl Default for Maolan {
    fn default() -> Self {
        let prefs = load_preferences();
        let mut state_data = StateData {
            available_templates: scan_templates(),
            ..StateData::default()
        };
        Self::apply_preferred_devices_to_state(&mut state_data, &prefs);
        let state = Arc::new(RwLock::new(state_data));
        let mut menu = menu::Menu::default();
        menu.update_templates(scan_templates());
        menu.update_recent_sessions(Self::normalize_recent_session_paths(
            prefs.recent_session_paths.clone(),
        ));
        Self {
            clip: None,
            clip_preview_target_track: None,
            clip_preview_target_valid: false,
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
            #[cfg(target_os = "macos")]
            pending_save_vst3_states: HashSet::new(),
            pending_save_is_template: false,
            pending_peak_file_loads: HashMap::new(),
            pending_peak_rebuilds: HashSet::new(),
            pending_track_freeze_restore: HashMap::new(),
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
            zoom_visible_bars: 127.0,
            editor_scroll_x: 0.0,
            editor_scroll_y: 0.0,
            mixer_scroll_x: 0.0,
            tracks_resize_hovered: false,
            mixer_resize_hovered: false,
            tracks_visible: true,
            editor_visible: true,
            mixer_visible: true,
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
            prefs_export_sample_rate_hz: prefs.default_export_sample_rate_hz,
            prefs_snap_mode: prefs.default_snap_mode,
            prefs_audio_bit_depth: prefs.default_audio_bit_depth,
            prefs_default_output_device_id: prefs.default_output_device_id,
            prefs_default_input_device_id: prefs.default_input_device_id,
        }
    }
}

impl Maolan {
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
    ) -> std::io::Result<(String, usize, usize)>
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

        progress_callback(1.0, None);
        let frames = final_samples.len() / channels.max(1);
        Ok((rel, channels.max(1), frames.max(1)))
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
        let mut builder = Mp3Builder::new()
            .ok_or_else(|| io::Error::other("Failed to initialize MP3 encoder builder"))?;
        builder
            .set_num_channels(output_channels as u8)
            .map_err(|e| io::Error::other(format!("MP3 set channels failed: {e}")))?;
        builder
            .set_sample_rate(sample_rate.max(1) as u32)
            .map_err(|e| io::Error::other(format!("MP3 set sample rate failed: {e}")))?;
        let mp3_bitrate = match codec.mp3_bitrate_kbps {
            8 => Mp3Bitrate::Kbps8,
            16 => Mp3Bitrate::Kbps16,
            24 => Mp3Bitrate::Kbps24,
            32 => Mp3Bitrate::Kbps32,
            40 => Mp3Bitrate::Kbps40,
            48 => Mp3Bitrate::Kbps48,
            64 => Mp3Bitrate::Kbps64,
            80 => Mp3Bitrate::Kbps80,
            96 => Mp3Bitrate::Kbps96,
            112 => Mp3Bitrate::Kbps112,
            128 => Mp3Bitrate::Kbps128,
            160 => Mp3Bitrate::Kbps160,
            192 => Mp3Bitrate::Kbps192,
            224 => Mp3Bitrate::Kbps224,
            256 => Mp3Bitrate::Kbps256,
            _ => Mp3Bitrate::Kbps320,
        };
        builder
            .set_brate(mp3_bitrate)
            .map_err(|e| io::Error::other(format!("MP3 set bitrate failed: {e}")))?;
        if matches!(codec.mp3_mode, ExportMp3Mode::Vbr) {
            builder
                .set_vbr_mode(mp3lame_encoder::VbrMode::Mtrh)
                .map_err(|e| io::Error::other(format!("MP3 set VBR mode failed: {e}")))?;
            builder
                .set_vbr_quality(Mp3Quality::NearBest)
                .map_err(|e| io::Error::other(format!("MP3 set VBR quality failed: {e}")))?;
        } else {
            builder
                .set_vbr_mode(mp3lame_encoder::VbrMode::Off)
                .map_err(|e| io::Error::other(format!("MP3 set CBR mode failed: {e}")))?;
        }
        builder
            .set_quality(Mp3Quality::Best)
            .map_err(|e| io::Error::other(format!("MP3 set quality failed: {e}")))?;
        let id3_year = metadata.year.map(|v| v.to_string()).unwrap_or_default();
        let mut id3_comment = String::new();
        if let Some(track_number) = metadata.track_number {
            id3_comment.push_str(&format!("TRACKNUMBER={track_number};"));
        }
        if !metadata.genre.is_empty() {
            id3_comment.push_str(&format!("GENRE={};", metadata.genre));
        }
        let id3 = mp3lame_encoder::Id3Tag {
            title: b"",
            artist: metadata.author.as_bytes(),
            album: metadata.album.as_bytes(),
            album_art: &[],
            year: id3_year.as_bytes(),
            comment: id3_comment.as_bytes(),
        };
        builder
            .set_id3_tag(id3)
            .map_err(|e| io::Error::other(format!("Failed to set MP3 ID3 tag: {e:?}")))?;
        let mut encoder = builder
            .build()
            .map_err(|e| io::Error::other(format!("MP3 encoder build failed: {e}")))?;

        let mut out = Vec::<u8>::with_capacity(mp3lame_encoder::max_required_buffer_size(
            mixed_buffer.len(),
        ));
        let frame_chunk = 4096usize;
        for chunk in mixed_buffer.chunks(frame_chunk * output_channels.max(1)) {
            encoder
                .encode_to_vec(InterleavedPcm(chunk), &mut out)
                .map_err(|e| io::Error::other(format!("MP3 encode failed: {e}")))?;
        }
        encoder
            .flush_to_vec::<FlushNoGap>(&mut out)
            .map_err(|e| io::Error::other(format!("MP3 finalization failed: {e}")))?;
        fs::write(export_path, out).map_err(|e| {
            io::Error::other(format!(
                "Failed to write '{}': {}",
                export_path.display(),
                e
            ))
        })
    }

    fn write_ogg_vorbis(
        export_path: &Path,
        mixed_buffer: &[f32],
        sample_rate: i32,
        output_channels: usize,
        codec: ExportCodecSettings,
        metadata: &ExportMetadata,
    ) -> io::Result<()> {
        let sampling_frequency = NonZeroU32::new(sample_rate.max(1) as u32)
            .ok_or_else(|| io::Error::other("Invalid sample rate for OGG"))?;
        let channels = NonZeroU8::new(output_channels as u8)
            .ok_or_else(|| io::Error::other("Invalid channel count for OGG"))?;
        let mut out = Vec::<u8>::new();
        let mut builder = VorbisEncoderBuilder::new(sampling_frequency, channels, &mut out)
            .map_err(|e| io::Error::other(format!("OGG encoder init failed: {e}")))?;
        builder.bitrate_management_strategy(VorbisBitrateManagementStrategy::QualityVbr {
            target_quality: codec.ogg_quality.clamp(-0.1, 1.0),
        });
        if !metadata.author.is_empty() {
            builder
                .comment_tag("ARTIST", metadata.author.as_str())
                .map_err(|e| io::Error::other(format!("Failed to set OGG ARTIST tag: {e}")))?;
        }
        if !metadata.album.is_empty() {
            builder
                .comment_tag("ALBUM", metadata.album.as_str())
                .map_err(|e| io::Error::other(format!("Failed to set OGG ALBUM tag: {e}")))?;
        }
        if let Some(year) = metadata.year {
            builder
                .comment_tag("DATE", year.to_string())
                .map_err(|e| io::Error::other(format!("Failed to set OGG DATE tag: {e}")))?;
        }
        if let Some(track_number) = metadata.track_number {
            builder
                .comment_tag("TRACKNUMBER", track_number.to_string())
                .map_err(|e| io::Error::other(format!("Failed to set OGG TRACKNUMBER tag: {e}")))?;
        }
        if !metadata.genre.is_empty() {
            builder
                .comment_tag("GENRE", metadata.genre.as_str())
                .map_err(|e| io::Error::other(format!("Failed to set OGG GENRE tag: {e}")))?;
        }
        let mut encoder = builder
            .build()
            .map_err(|e| io::Error::other(format!("OGG encoder build failed: {e}")))?;

        let frame_chunk = 2048usize;
        for chunk in mixed_buffer.chunks(frame_chunk * output_channels.max(1)) {
            let frames = chunk.len() / output_channels.max(1);
            if frames == 0 {
                continue;
            }
            let mut planar = vec![vec![0.0_f32; frames]; output_channels];
            for frame in 0..frames {
                for ch in 0..output_channels {
                    planar[ch][frame] = chunk[frame * output_channels + ch];
                }
            }
            let block = planar.iter().map(Vec::as_slice).collect::<Vec<_>>();
            encoder
                .encode_audio_block(&block)
                .map_err(|e| io::Error::other(format!("OGG encode failed: {e}")))?;
        }
        encoder
            .finish()
            .map_err(|e| io::Error::other(format!("OGG finalization failed: {e}")))?;
        fs::write(export_path, out).map_err(|e| {
            io::Error::other(format!(
                "Failed to write '{}': {}",
                export_path.display(),
                e
            ))
        })
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

        let (tracks, connections, total_length, selected_tracks) = {
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
        let clip_len = notes
            .iter()
            .map(|n| n.start_sample.saturating_add(n.length_samples))
            .chain(controllers.iter().map(|c| c.sample))
            .chain(sysexes.iter().map(|s| s.sample))
            .max()
            .unwrap_or_else(|| ticks_to_samples(max_tick))
            .max(1);
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
    fn saved_unix_plugin_format(plugin: &Value, clap_paths: &[String]) -> Option<&'static str> {
        if let Some(format) = plugin.get("format").and_then(Value::as_str) {
            if format.eq_ignore_ascii_case("LV2") {
                return Some("LV2");
            }
            if format.eq_ignore_ascii_case("CLAP") {
                return Some("CLAP");
            }
        }
        let uri = plugin.get("uri").and_then(Value::as_str)?;
        if clap_paths.iter().any(|path| path.eq_ignore_ascii_case(uri)) {
            Some("CLAP")
        } else {
            Some("LV2")
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn plugin_node_from_json_with_runtime_nodes(
        v: &Value,
        runtime_nodes: &[maolan_engine::message::PluginGraphNode],
    ) -> Option<maolan_engine::message::PluginGraphNode> {
        use maolan_engine::message::PluginGraphNode;
        let t = v["type"].as_str()?;
        match t {
            "track_input" => Some(PluginGraphNode::TrackInput),
            "track_output" => Some(PluginGraphNode::TrackOutput),
            "plugin" => runtime_nodes
                .get(v["plugin_index"].as_u64()? as usize)
                .and_then(|node| {
                    matches!(node, PluginGraphNode::Lv2PluginInstance(_)).then(|| node.clone())
                }),
            "clap_plugin" => runtime_nodes
                .get(v["plugin_index"].as_u64()? as usize)
                .and_then(|node| {
                    matches!(node, PluginGraphNode::ClapPluginInstance(_)).then(|| node.clone())
                }),
            _ => None,
        }
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
            "audio" => Some(Kind::Audio),
            "midi" => Some(Kind::MIDI),
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
    fn track_plugin_list_view(&self) -> iced::Element<'_, Message> {
        let state = self.state.blocking_read();
        let title = state
            .plugin_graph_track
            .clone()
            .unwrap_or_else(|| "(no track)".to_string());

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
                text(format!("Track Plugins: {title}")),
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
        let title = state
            .plugin_graph_track
            .clone()
            .unwrap_or_else(|| "(no track)".to_string());
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
                text(format!("Track Plugins: {title}")),
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
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        {
            options.extend(state.available_hw.iter().map(|hw| PreferencesDeviceOption {
                id: hw.id.clone(),
                label: hw.label.clone(),
            }));
        }
        #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
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
        #[cfg(target_os = "freebsd")]
        {
            options.extend(state.available_hw.iter().map(|hw| PreferencesDeviceOption {
                id: hw.id.clone(),
                label: hw.label.clone(),
            }));
        }
        #[cfg(target_os = "linux")]
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
        #[cfg(unix)]
        {
            let bits = if AUDIO_BIT_DEPTH_OPTIONS.contains(&prefs.default_audio_bit_depth) {
                prefs.default_audio_bit_depth
            } else {
                32
            };
            state.oss_bits = bits;
        }
        if let Some(device_id) = prefs.default_output_device_id.as_deref() {
            #[cfg(any(target_os = "linux", target_os = "freebsd"))]
            if let Some(selected) = state
                .available_hw
                .iter()
                .find(|hw| hw.id == device_id)
                .cloned()
            {
                state.selected_hw = Some(selected);
            }
            #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
            if state.available_hw.iter().any(|hw| hw == device_id) {
                state.selected_hw = Some(device_id.to_string());
            }
        }
        if let Some(device_id) = prefs.default_input_device_id.as_deref() {
            #[cfg(target_os = "freebsd")]
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
        }
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        if let Some(selected) = state.selected_hw.as_ref()
            && let Some(bits) = selected.preferred_bits()
        {
            state.oss_bits = bits;
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
                    text("Default snap mode:"),
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
    }

    #[test]
    #[cfg(all(unix, not(target_os = "macos")))]
    fn saved_unix_plugin_format_prefers_explicit_format_and_falls_back_to_clap_paths() {
        let clap_paths = vec!["/plugins/a.clap".to_string()];

        assert_eq!(
            Maolan::saved_unix_plugin_format(
                &json!({"format": "CLAP", "uri": "ignored"}),
                &clap_paths
            ),
            Some("CLAP")
        );
        assert_eq!(
            Maolan::saved_unix_plugin_format(
                &json!({"format": "LV2", "uri": "/plugins/a.clap"}),
                &clap_paths
            ),
            Some("LV2")
        );
        assert_eq!(
            Maolan::saved_unix_plugin_format(&json!({"uri": "/plugins/a.clap"}), &clap_paths),
            Some("CLAP")
        );
        assert_eq!(
            Maolan::saved_unix_plugin_format(&json!({"uri": "urn:lv2:test"}), &clap_paths),
            Some("LV2")
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
}
