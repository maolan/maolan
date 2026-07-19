mod platform;
mod session;
mod subscriptions;
mod update;
mod view;

use crate::{
    add_track, clip_rename, config,
    consts::audio_defaults,
    consts::gui as gui_consts,
    consts::gui_mod::{
        AUDIO_BIT_DEPTH_OPTIONS, BINS_PER_UPDATE, CHUNK_FRAMES, CLIENT, MAX_PEAK_BINS,
        MAX_RECENT_SESSIONS, STANDARD_EXPORT_SAMPLE_RATES,
    },
    consts::message_lists::{
        EXPORT_BIT_DEPTH_ALL, EXPORT_DITHER_ALL, EXPORT_MP3_MODE_ALL, EXPORT_NORMALIZE_MODE_ALL,
        EXPORT_RENDER_MODE_ALL, SNAP_MODE_ALL,
    },
    consts::state_ids::METRONOME_TRACK_ID,
    consts::widget_piano::PITCH_MAX,
    hw, menu,
    message::{
        BurnBackendOption, DraggedClip, ExportBitDepth, ExportDither, ExportFormat, ExportMp3Mode,
        ExportNormalizeMode, ExportRenderMode, GenerateAudioModelOption, Message,
        PreferencesDeviceOption, Show, SnapMode, TrackAutomationTarget,
    },
    platform_caps,
    state::{
        AudioClip, ClipPeaks, LOG_HISTORY_LIMIT, LogEntry, LogLevel, MIDIClip, MidiClipPreviewMap,
        PianoControllerPoint, PianoNote, PianoSysExPoint, PitchCorrectionData,
        PitchCorrectionPoint, State, StateData,
    },
    template_save, toolbar, track_marker, track_rename, track_template_save, workspace,
};
use ebur128::{EbuR128, Mode as LoudnessMode};
use ffmpeg_next::{
    Dictionary,
    codec::{Context as CodecContext, Id as CodecId},
    format::output,
    frame::Audio,
};
use iced::{
    Color, Length, Size, Task,
    advanced::text::Span,
    widget::{
        Column, button, checkbox, column, container, pick_list, progress_bar, rich_text, row,
        scrollable, span, text, text_editor, text_input,
    },
};
use iced_aw::helpers::color_picker_with_change;
use maolan_engine::kind::Kind;
use maolan_engine::message::{
    Action, ConnectableConnection, ConnectableRef, Message as EngineMessage, OfflineAutomationLane,
    OfflineAutomationPoint, OfflineAutomationTarget,
};
use maolan_widgets::numeric_input::{number_input, number_input_f32};
use midly::{
    Format, Header, MetaMessage, Smf, Timing, TrackEvent, TrackEventKind,
    num::{u15, u24, u28},
};
use pitch_detection::detector::{PitchDetector, mcleod::McLeodDetector};
use serde::Serialize;
use serde_json::Value;
#[allow(unused_imports)]
use serde_json::json;

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs::{self, File},
    hash::{DefaultHasher, Hash, Hasher},
    io::{self, BufReader, Write},
    ops::Range,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::AtomicBool,
    sync::{Arc, LazyLock, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::RwLock;

pub(crate) use gui_consts::{MIN_CLIP_WIDTH_PX, PREF_DEVICE_AUTO_ID};
type TickToSampleFn = dyn Fn(u64) -> usize + Send + Sync;
type MidiTickMap = (Box<TickToSampleFn>, u64, u64);

#[derive(Debug, Clone, PartialEq, Default)]
struct LogHighlightSettings {
    lines: Vec<Vec<(Range<usize>, LogHighlight)>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct LogHighlight {
    color: Color,
}

fn format_log_entry_for_editor(entry: &LogEntry) -> (String, Vec<(Range<usize>, LogHighlight)>) {
    let prefix = format!("[{}] ", entry.level);
    let mut line = prefix.clone();
    let mut highlights = vec![(
        0..prefix.len(),
        LogHighlight {
            color: log_level_color(entry.level),
        },
    )];

    let (message, message_highlights) = strip_ansi_and_collect_highlights(&entry.message);
    let message_offset = line.len();
    line.push_str(&message);
    highlights.extend(message_highlights.into_iter().map(|(range, highlight)| {
        (
            range.start + message_offset..range.end + message_offset,
            highlight,
        )
    }));

    (line, highlights)
}

fn last_message_status_text(message: &str) -> iced::Element<'static, Message> {
    rich_text(ansi_status_spans("Last message: ", message))
        .width(Length::Fill)
        .into()
}

fn ansi_status_spans(prefix: &'static str, message: &str) -> Vec<Span<'static, (), iced::Font>> {
    let (text, highlights) = strip_ansi_and_collect_highlights(message);
    let mut spans = Vec::with_capacity(highlights.len().saturating_mul(2).saturating_add(1));
    spans.push(span(prefix.to_string()));

    let mut cursor = 0;
    for (range, highlight) in highlights {
        if cursor < range.start {
            spans.push(span(text[cursor..range.start].to_string()));
        }
        spans.push(span(text[range.clone()].to_string()).color(highlight.color));
        cursor = range.end;
    }

    if cursor < text.len() {
        spans.push(span(text[cursor..].to_string()));
    }

    spans
}

fn strip_ansi_and_collect_highlights(message: &str) -> (String, Vec<(Range<usize>, LogHighlight)>) {
    let mut text = String::with_capacity(message.len());
    let mut highlights = Vec::new();
    let mut current_color = None;
    let mut bold = false;
    let mut segment_start = None;
    let bytes = message.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == 0x1b && bytes.get(index + 1) == Some(&b'[') {
            flush_log_highlight(
                &mut highlights,
                &mut segment_start,
                text.len(),
                current_color,
            );
            index += 2;
            let sequence_start = index;
            while index < bytes.len() && bytes[index] != b'm' {
                index += 1;
            }
            if index < bytes.len() {
                apply_sgr_codes(
                    &message[sequence_start..index],
                    &mut current_color,
                    &mut bold,
                );
                index += 1;
                continue;
            }

            text.push_str(&message[sequence_start.saturating_sub(2)..]);
            break;
        }

        if let Some(ch) = message[index..].chars().next() {
            if current_color.is_some() && segment_start.is_none() {
                segment_start = Some(text.len());
            }
            text.push(ch);
            index += ch.len_utf8();
        } else {
            break;
        }
    }

    flush_log_highlight(
        &mut highlights,
        &mut segment_start,
        text.len(),
        current_color,
    );

    (text, highlights)
}

fn flush_log_highlight(
    highlights: &mut Vec<(Range<usize>, LogHighlight)>,
    segment_start: &mut Option<usize>,
    segment_end: usize,
    current_color: Option<Color>,
) {
    if let (Some(start), Some(color)) = (segment_start.take(), current_color)
        && start < segment_end
    {
        highlights.push((start..segment_end, LogHighlight { color }));
    }
}

fn apply_sgr_codes(codes: &str, current_color: &mut Option<Color>, bold: &mut bool) {
    if codes.is_empty() {
        *current_color = None;
        *bold = false;
        return;
    }

    let mut selected_color = None;
    for code in codes.split(';').filter_map(|code| code.parse::<u16>().ok()) {
        match code {
            0 => {
                *current_color = None;
                *bold = false;
                selected_color = None;
            }
            1 => *bold = true,
            22 => *bold = false,
            30..=37 => selected_color = Some((code - 30, false)),
            39 => {
                *current_color = None;
                selected_color = None;
            }
            90..=97 => selected_color = Some((code - 90, true)),
            _ => {}
        }
    }

    if let Some((index, bright)) = selected_color {
        *current_color = ansi_color(index, bright || *bold);
    }
}

fn ansi_color(index: u16, bright: bool) -> Option<Color> {
    let (r, g, b) = match (index, bright) {
        (0, false) => (96, 96, 96),
        (1, false) => (205, 84, 84),
        (2, false) => (88, 166, 95),
        (3, false) => (205, 168, 80),
        (4, false) => (98, 148, 206),
        (5, false) => (176, 117, 197),
        (6, false) => (77, 178, 188),
        (7, false) => (210, 210, 210),
        (0, true) => (142, 142, 142),
        (1, true) => (245, 108, 108),
        (2, true) => (116, 210, 126),
        (3, true) => (244, 202, 99),
        (4, true) => (123, 176, 245),
        (5, true) => (211, 146, 235),
        (6, true) => (101, 219, 229),
        (7, true) => (242, 242, 242),
        _ => return None,
    };
    Some(Color::from_rgb8(r, g, b))
}

fn log_level_color(level: LogLevel) -> Color {
    match level {
        LogLevel::Info => Color::from_rgb8(123, 176, 245),
        LogLevel::Warning => Color::from_rgb8(244, 202, 99),
        LogLevel::Error => Color::from_rgb8(245, 108, 108),
    }
}

#[cfg(unix)]
const MAOLAN_BURN_SOCKETPAIR_ENV: &str = "MAOLAN_BURN_SOCKETPAIR";
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

struct ExportSessionOptions {
    export_path: PathBuf,
    sample_rate: i32,
    formats: Vec<ExportFormat>,
    render_mode: ExportRenderMode,
    selected_hw_out_ports: Vec<usize>,
    realtime_fallback: bool,
    bit_depth: ExportBitDepth,
    dither: ExportDither,
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
    dither: ExportDither,
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

fn build_offline_automation_lanes(
    lanes: &[crate::state::TrackAutomationLane],
) -> Vec<OfflineAutomationLane> {
    let mut automation_lanes = Vec::new();
    for lane in lanes.iter().filter(|lane| !lane.points.is_empty()) {
        let target = match lane.target {
            TrackAutomationTarget::Volume => OfflineAutomationTarget::Volume,
            TrackAutomationTarget::Balance => OfflineAutomationTarget::Balance,
            TrackAutomationTarget::MidiCc { channel, cc } => {
                OfflineAutomationTarget::MidiCc { channel, cc }
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            TrackAutomationTarget::Lv2Parameter {
                instance_id,
                index,
                min,
                max,
            } => OfflineAutomationTarget::Lv2Parameter {
                instance_id,
                index,
                min,
                max,
            },
            #[cfg(not(all(unix, not(target_os = "macos"))))]
            TrackAutomationTarget::Lv2Parameter { .. } => continue,
            TrackAutomationTarget::Vst3Parameter {
                instance_id,
                param_id,
            } => OfflineAutomationTarget::Vst3Parameter {
                instance_id,
                param_id,
            },
            TrackAutomationTarget::ClapParameter {
                instance_id,
                param_id,
                min,
                max,
            } => OfflineAutomationTarget::ClapParameter {
                instance_id,
                param_id,
                min,
                max,
            },
        };
        let points = lane
            .points
            .iter()
            .map(|p| OfflineAutomationPoint {
                sample: p.sample,
                value: p.value,
            })
            .collect::<Vec<_>>();
        automation_lanes.push(OfflineAutomationLane {
            target,
            visible: true,
            points,
        });
    }
    automation_lanes
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
    deleted_clips: Vec<String>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum AutomationWriteKey {
    Volume,
    Balance,
    MidiCc { channel: u8, cc: u8 },
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
    midi_cc: HashMap<(u8, u8), u8>,
    #[cfg(all(unix, not(target_os = "macos")))]
    lv2_params: HashMap<(usize, u32), f32>,
    vst3_params: HashMap<(usize, u32), f32>,
    clap_params: HashMap<(usize, u32), f64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum PluginInstanceRef {
    Track {
        track_name: String,
        instance_id: usize,
    },
    Clip {
        track_name: String,
        clip_idx: usize,
        instance_id: usize,
    },
}

struct CollectToSessionOperation {
    session_root: PathBuf,
    data_dir: PathBuf,
    pending_clap_refs: HashSet<PluginInstanceRef>,
    copied_files: HashMap<PathBuf, String>,
}

impl CollectToSessionOperation {
    fn collect_file(&mut self, path_str: &str) -> Option<(PathBuf, String)> {
        let p = PathBuf::from(path_str);
        let abs = if p.is_absolute() {
            p.clone()
        } else {
            self.session_root.join(&p)
        };
        if !abs.exists() || !abs.is_file() {
            return None;
        }
        if abs.starts_with(&self.data_dir) {
            return None;
        }
        if let Some(existing) = self.copied_files.get(&abs) {
            return Some((abs, existing.clone()));
        }
        let file_name = abs.file_name()?.to_str()?.to_string();
        let mut dst = self.data_dir.join(&file_name);
        let mut counter = 1;
        while dst.exists() {
            let stem = Path::new(&file_name)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("file");
            let ext = Path::new(&file_name)
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            let new_name = if ext.is_empty() {
                format!("{stem}_{counter}")
            } else {
                format!("{stem}_{counter}.{ext}")
            };
            dst = self.data_dir.join(new_name);
            counter += 1;
        }
        if std::fs::copy(&abs, &dst).is_err() {
            return None;
        }
        let rel = format!("data/{}", dst.file_name()?.to_str()?);
        self.copied_files.insert(abs.clone(), rel.clone());
        Some((abs, rel))
    }
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
    hw: hw::HW,
    modal: Option<Show>,
    add_track: add_track::AddTrackView,
    apply_template: crate::apply_template::ApplyTemplateView,
    clip_rename: clip_rename::ClipRenameView,
    track_rename: track_rename::TrackRenameView,
    scene_rename: crate::session_view::rename::SceneRenameView,
    track_marker: track_marker::MarkerView,
    modulator_target_dialog: crate::modulator_target_dialog::ModulatorTargetDialogView,
    track_template_save: track_template_save::TrackTemplateSaveView,
    template_save: template_save::TemplateSaveView,
    #[cfg(all(unix, not(target_os = "macos")))]
    selected_lv2_plugins: BTreeSet<String>,
    selected_vst3_plugins: BTreeSet<String>,
    selected_clap_plugins: BTreeSet<String>,
    plugin_list_filter: String,
    session_dir: Option<PathBuf>,
    session_branch: String,
    collect_to_session_operation: Option<CollectToSessionOperation>,
    pending_save_path: Option<String>,
    pending_save_tracks: std::collections::HashSet<String>,
    pending_save_clap_tracks: std::collections::HashSet<String>,
    pending_save_clap_clips: std::collections::HashSet<(String, usize, usize)>,
    #[cfg(target_os = "macos")]
    pending_save_vst3_states: HashSet<(String, usize)>,
    pending_save_is_template: bool,
    pending_save_track_name: Option<String>,
    pending_peak_file_loads: HashMap<AudioClipKey, PathBuf>,
    pending_peak_rebuilds: HashSet<AudioClipKey>,
    pending_precomputed_peaks: HashMap<AudioClipKey, crate::state::ClipPeaks>,
    pending_source_lengths: HashMap<AudioClipKey, usize>,
    undo_peaks_cache: HashMap<AudioClipKey, crate::state::ClipPeaks>,
    undo_source_lengths_cache: HashMap<AudioClipKey, usize>,
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
    live_session_playing: bool,
    recorded_live_session_clip_passes: HashSet<(String, usize, String, usize, usize)>,
    live_session_record_start_sample: Option<usize>,
    metronome_enabled: bool,
    transport_samples: f64,
    last_playback_tick: Option<Instant>,
    pending_transport_position: Option<(Instant, usize)>,
    playback_rate_hz: f64,
    loop_enabled: bool,
    loop_range_samples: Option<(usize, usize)>,
    punch_enabled: bool,
    punch_range_samples: Option<(usize, usize)>,
    snap_mode: SnapMode,
    midi_snap_mode: SnapMode,
    step_recording_active: bool,
    step_recording_cursor_samples: usize,
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
    toolbar_visible: bool,
    show_log_window: bool,
    shortcuts_pane_visible: bool,
    modulators_pane_visible: bool,
    clips_pane_visible: bool,
    pub modulators: Vec<crate::state::Modulator>,
    pub selected_modulator_id: Option<usize>,
    hw_mixer: mixosc::app::MixOscApp,
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
    log_viewer_highlights: LogHighlightSettings,

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
    export_cancel: Arc<AtomicBool>,
    export_pending_bounces: HashSet<String>,
    export_bounce_notify: Option<Arc<tokio::sync::Notify>>,
    export_progress: f32,
    export_operation: Option<String>,
    export_sample_rate_hz: u32,
    export_format_wav: bool,
    export_format_mp3: bool,
    export_format_ogg: bool,
    export_format_flac: bool,
    export_bit_depth: ExportBitDepth,
    export_dither: ExportDither,
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
    clap_param_values: HashMap<(String, Option<usize>, usize, u32), f64>,
    tempo_input: String,
    time_signature_num_input: String,
    time_signature_denom_input: String,
    tap_tempo_times: Vec<Instant>,
    last_sent_tempo_bpm: Option<f64>,
    last_sent_time_signature: Option<(u16, u16)>,
    selected_tempo_points: BTreeSet<usize>,
    selected_time_signature_points: BTreeSet<usize>,
    timing_selection_lane: Option<TimingSelectionLane>,
    midi_mappings_panel_open: bool,
    midi_mappings_report_lines: Vec<String>,
    has_unsaved_changes: bool,
    engine_dirty: bool,
    pending_exit_after_save: bool,
    session_restore_in_progress: bool,
    last_autosave_snapshot: Option<Instant>,
    pending_recovery_session_dir: Option<PathBuf>,
    pending_autosave_recovery: Option<PendingAutosaveRecovery>,
    pending_open_session_dir: Option<PathBuf>,
    pending_branch_input: String,
    dragging_session_slot: Option<(String, usize)>,
    dragging_session_clip: Option<crate::state::DraggedSessionClip>,
    dragging_pane_clip: Option<crate::state::DraggedSessionClip>,
    session_slot_record_target: Option<(String, usize)>,
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
            if path.is_dir() && path.join("main.json").is_file() {
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
            if path.is_dir() && path.join("track.json").is_file() {
                path.file_name()?.to_str().map(|s| s.to_string())
            } else {
                None
            }
        })
        .collect()
}

pub(crate) fn is_track_template_folder(template_name: &str) -> bool {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let path = format!(
        "{}/.config/maolan/track_templates/{}/track.json",
        home, template_name
    );
    let Ok(file) = std::fs::File::open(&path) else {
        return false;
    };
    let reader = std::io::BufReader::new(file);
    let Ok(json): Result<serde_json::Value, _> = serde_json::from_reader(reader) else {
        return false;
    };
    json.get("track")
        .and_then(|t| t.get("is_folder"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

pub(crate) fn scan_track_and_folder_templates() -> (Vec<String>, Vec<String>) {
    let all = scan_track_templates();
    let mut track_templates = Vec::new();
    let mut folder_templates = Vec::new();
    for name in all {
        if is_track_template_folder(&name) {
            folder_templates.push(name);
        } else {
            track_templates.push(name);
        }
    }
    (track_templates, folder_templates)
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
            hw: hw::HW::new(state.clone()),
            modal: None,
            add_track: add_track::AddTrackView::default(),
            apply_template: crate::apply_template::ApplyTemplateView::new(state.clone()),
            clip_rename: clip_rename::ClipRenameView::new(state.clone()),
            track_rename: track_rename::TrackRenameView::new(state.clone()),
            scene_rename: crate::session_view::rename::SceneRenameView::new(state.clone()),
            track_marker: track_marker::MarkerView::new(state.clone()),
            modulator_target_dialog: crate::modulator_target_dialog::ModulatorTargetDialogView::new(
                state.clone(),
            ),
            track_template_save: track_template_save::TrackTemplateSaveView::new(state.clone()),
            template_save: template_save::TemplateSaveView::new(state.clone()),
            #[cfg(all(unix, not(target_os = "macos")))]
            #[cfg(all(unix, not(target_os = "macos")))]
            selected_lv2_plugins: BTreeSet::new(),
            selected_vst3_plugins: BTreeSet::new(),
            selected_clap_plugins: BTreeSet::new(),
            plugin_list_filter: String::new(),
            session_dir: None,
            session_branch: "main".to_string(),
            collect_to_session_operation: None,
            pending_save_path: None,
            pending_save_tracks: std::collections::HashSet::new(),
            pending_save_clap_tracks: std::collections::HashSet::new(),
            pending_save_clap_clips: std::collections::HashSet::new(),
            #[cfg(target_os = "macos")]
            pending_save_vst3_states: HashSet::new(),
            pending_save_is_template: false,
            pending_save_track_name: None,
            pending_peak_file_loads: HashMap::new(),
            pending_peak_rebuilds: HashSet::new(),
            pending_precomputed_peaks: HashMap::new(),
            pending_source_lengths: HashMap::new(),
            undo_peaks_cache: HashMap::new(),
            undo_source_lengths_cache: HashMap::new(),
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
            live_session_playing: false,
            recorded_live_session_clip_passes: HashSet::new(),
            live_session_record_start_sample: None,
            metronome_enabled: false,
            transport_samples: 0.0,
            last_playback_tick: None,
            pending_transport_position: None,
            playback_rate_hz: 48_000.0,
            loop_enabled: false,
            loop_range_samples: None,
            punch_enabled: false,
            punch_range_samples: None,
            snap_mode: prefs.default_snap_mode,
            midi_snap_mode: prefs.default_midi_snap_mode,
            step_recording_active: false,
            step_recording_cursor_samples: 0,
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
            toolbar_visible: true,
            show_log_window: false,
            shortcuts_pane_visible: false,
            modulators_pane_visible: false,
            clips_pane_visible: false,
            modulators: vec![],
            selected_modulator_id: None,
            hw_mixer: mixosc::app::MixOscApp::default(),
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
            log_viewer_highlights: LogHighlightSettings::default(),

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
            export_cancel: Arc::new(AtomicBool::new(false)),
            export_pending_bounces: HashSet::new(),
            export_bounce_notify: None,
            export_progress: 0.0,
            export_operation: None,
            export_sample_rate_hz: prefs.default_export_sample_rate_hz,
            export_format_wav: true,
            export_format_mp3: false,
            export_format_ogg: false,
            export_format_flac: false,
            export_bit_depth: ExportBitDepth::Int24,
            export_dither: ExportDither::Triangular,
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
            clap_param_values: HashMap::new(),
            tempo_input: "120".to_string(),
            time_signature_num_input: "4".to_string(),
            time_signature_denom_input: "4".to_string(),
            tap_tempo_times: Vec::new(),
            last_sent_tempo_bpm: Some(120.0),
            last_sent_time_signature: Some((4, 4)),
            selected_tempo_points: BTreeSet::new(),
            selected_time_signature_points: BTreeSet::new(),
            timing_selection_lane: None,
            midi_mappings_panel_open: false,
            midi_mappings_report_lines: Vec::new(),
            has_unsaved_changes: false,
            engine_dirty: false,
            pending_exit_after_save: false,
            session_restore_in_progress: false,
            last_autosave_snapshot: None,
            pending_recovery_session_dir: None,
            pending_autosave_recovery: None,
            pending_open_session_dir: None,
            pending_branch_input: String::new(),
            dragging_session_slot: None,
            dragging_session_clip: None,
            dragging_pane_clip: None,
            session_slot_record_target: None,
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
    pub fn new() -> (Self, iced::Task<Message>) {
        let mut app = Self::default();
        let (hw_mixer, hw_task) = mixosc::app::new();
        app.hw_mixer = hw_mixer;
        (app, hw_task.map(Message::HwMixer))
    }

    fn is_dirty(&self) -> bool {
        self.has_unsaved_changes || self.engine_dirty
    }

    fn push_log_entry(state: &mut StateData, level: LogLevel, message: String) {
        state.message = message.clone();
        state.log_entries.push(LogEntry { level, message });
        if state.log_entries.len() > LOG_HISTORY_LIMIT {
            let drop_count = state.log_entries.len() - LOG_HISTORY_LIMIT;
            state.log_entries.drain(0..drop_count);
        }
    }

    fn refresh_log_viewer_content(&mut self) {
        let entries = self.state.blocking_read().log_entries.clone();
        let mut lines = Vec::with_capacity(entries.len());
        let mut highlights = Vec::with_capacity(entries.len());

        for entry in entries {
            let (line, line_highlights) = format_log_entry_for_editor(&entry);
            lines.push(line);
            highlights.push(line_highlights);
        }

        let log_text = lines.join("\n");
        self.log_viewer_content = text_editor::Content::with_text(&log_text);
        self.log_viewer_highlights = LogHighlightSettings { lines: highlights };
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
        let mut state = self.state.blocking_write();
        Self::push_log_entry(&mut state, LogLevel::Info, message);
        drop(state);
        self.refresh_log_viewer_content();
    }

    fn warning(&mut self, message: impl Into<String>) {
        let message = message.into();
        let mut state = self.state.blocking_write();
        Self::push_log_entry(&mut state, LogLevel::Warning, message);
        drop(state);
        self.refresh_log_viewer_content();
    }

    fn error(&mut self, message: impl Into<String>) {
        let message = message.into();
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
                            if let Ok(mut log) = stderr_log_bg.lock() {
                                log.push(line);
                                if log.len() > 64 {
                                    let drop_count = log.len() - 64;
                                    log.drain(0..drop_count);
                                }
                            }
                        }
                        Err(_err) => {
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

        drop(process.socket);
        let _ = header;
        Ok(())
    }

    #[cfg(not(unix))]
    fn spawn_generate_process(_request: &BurnGenerateRequest) -> Result<(u32, ()), String> {
        Err("Generated audio via generate is only available on Unix platforms".to_string())
    }

    #[cfg(not(unix))]
    fn communicate_with_generate_process<F>(
        _socket: (),
        _progress_callback: F,
    ) -> Result<(), String>
    where
        F: FnMut(&str, f32, &str),
    {
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
        if let Err(_err) = cfg.save() {}
        self.menu.update_recent_sessions(recent);
    }

    fn forget_recent_session_path(&mut self, path: &Path) {
        let target = path.to_string_lossy().to_string();
        if target.trim().is_empty() {
            return;
        }
        let mut cfg = config::Config::load().unwrap_or_default();
        let recent: Vec<String> = cfg
            .recent_session_paths
            .into_iter()
            .filter(|p| p != &target)
            .collect();
        let recent = Self::normalize_recent_session_paths(recent);
        cfg.recent_session_paths = recent.clone();
        if let Err(_err) = cfg.save() {}
        self.menu.update_recent_sessions(recent);
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
        let dirty_suffix = if self.is_dirty() { " *" } else { "" };
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
        let (samples, channels, _) = Self::decode_audio_to_f32_interleaved_sync(path)?;
        let mut per_channel = vec![Vec::with_capacity(samples.len() / channels + 1); channels];
        for frame in samples.chunks(channels) {
            for (channel_idx, sample) in frame.iter().enumerate() {
                per_channel[channel_idx].push(sample.clamp(-1.0, 1.0));
            }
        }

        if per_channel.iter().all(|ch| ch.is_empty()) {
            return Ok(Arc::new(Vec::new()));
        }

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
        let (samples, channels, _) = Self::decode_audio_to_f32_interleaved_sync(path)?;
        let total_frames = samples.len() / channels.max(1);
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
        let mut sample_offset = 0usize;

        while processed_frames < total_frames {
            let frames_to_read = (total_frames - processed_frames).min(CHUNK_FRAMES);
            let samples_to_read = frames_to_read.saturating_mul(channels);
            let end = (sample_offset + samples_to_read).min(samples.len());
            let chunk = &samples[sample_offset..end];
            if chunk.is_empty() {
                break;
            }
            let frames_read = chunk.len() / channels.max(1);
            if frames_read == 0 {
                break;
            }
            sample_offset = sample_offset.saturating_add(chunk.len());

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
        let (samples, channels, _) = Self::decode_audio_to_f32_interleaved_sync(path)?;
        Ok(samples.len() / channels.max(1))
    }

    #[allow(dead_code)]
    fn audio_clip_channel_count(path: &Path) -> std::io::Result<usize> {
        let (_, channels, _) = Self::decode_audio_to_f32_interleaved_sync(path)?;
        Ok(channels.max(1))
    }

    fn decode_audio_to_f32_interleaved_sync(
        path: &Path,
    ) -> std::io::Result<(Vec<f32>, usize, u32)> {
        use ffmpeg_next::{
            format::Sample as FfSample, format::sample::Type as SampleType,
            media::Type as MediaType,
        };

        Self::ffmpeg_init().map_err(|e| io::Error::other(format!("FFmpeg init failed: {e}")))?;

        let mut ictx = ffmpeg_next::format::input(path).map_err(|e| {
            io::Error::other(format!(
                "Unsupported or unreadable audio '{}': {e}",
                path.display()
            ))
        })?;
        let stream = ictx.streams().best(MediaType::Audio).ok_or_else(|| {
            io::Error::other(format!("No decodable audio track in '{}'", path.display()))
        })?;
        let stream_index = stream.index();
        let mut decoder = ffmpeg_next::codec::Context::from_parameters(stream.parameters())
            .and_then(|ctx| ctx.decoder().audio())
            .map_err(|e| io::Error::other(format!("Failed to decode '{}': {e}", path.display())))?;

        let channels = decoder.channels().max(1) as usize;
        let sample_rate = decoder.rate();
        let mut samples = Vec::<f32>::new();
        let mut raw_frame = ffmpeg_next::frame::Audio::empty();

        let append_frame = |frame: &ffmpeg_next::frame::Audio,
                            channels: usize,
                            dst: &mut Vec<f32>|
         -> io::Result<()> {
            let frame_samples = frame.samples();
            if frame_samples == 0 || channels == 0 {
                return Ok(());
            }
            match frame.format() {
                FfSample::F32(SampleType::Packed) => {
                    let data = frame.data(0);
                    let src = unsafe {
                        std::slice::from_raw_parts(
                            data.as_ptr() as *const f32,
                            frame_samples.saturating_mul(channels),
                        )
                    };
                    dst.extend_from_slice(src);
                }
                FfSample::F32(SampleType::Planar) => {
                    for i in 0..frame_samples {
                        for ch in 0..channels {
                            let data = frame.data(ch);
                            let src = unsafe {
                                std::slice::from_raw_parts(
                                    data.as_ptr() as *const f32,
                                    frame_samples,
                                )
                            };
                            dst.push(src[i]);
                        }
                    }
                }
                FfSample::I16(SampleType::Packed) => {
                    let data = frame.data(0);
                    let src = unsafe {
                        std::slice::from_raw_parts(
                            data.as_ptr() as *const i16,
                            frame_samples.saturating_mul(channels),
                        )
                    };
                    let scale = i16::MAX as f32;
                    dst.extend(src.iter().map(|s| *s as f32 / scale));
                }
                FfSample::I16(SampleType::Planar) => {
                    let scale = i16::MAX as f32;
                    for i in 0..frame_samples {
                        for ch in 0..channels {
                            let data = frame.data(ch);
                            let src = unsafe {
                                std::slice::from_raw_parts(
                                    data.as_ptr() as *const i16,
                                    frame_samples,
                                )
                            };
                            dst.push(src[i] as f32 / scale);
                        }
                    }
                }
                FfSample::I32(SampleType::Packed) => {
                    let data = frame.data(0);
                    let src = unsafe {
                        std::slice::from_raw_parts(
                            data.as_ptr() as *const i32,
                            frame_samples.saturating_mul(channels),
                        )
                    };
                    let scale = i32::MAX as f32;
                    dst.extend(src.iter().map(|s| *s as f32 / scale));
                }
                FfSample::I32(SampleType::Planar) => {
                    let scale = i32::MAX as f32;
                    for i in 0..frame_samples {
                        for ch in 0..channels {
                            let data = frame.data(ch);
                            let src = unsafe {
                                std::slice::from_raw_parts(
                                    data.as_ptr() as *const i32,
                                    frame_samples,
                                )
                            };
                            dst.push(src[i] as f32 / scale);
                        }
                    }
                }
                fmt => {
                    return Err(io::Error::other(format!(
                        "Unsupported decoded audio format '{}': {:?}",
                        path.display(),
                        fmt
                    )));
                }
            }
            Ok(())
        };

        for (stream, packet) in ictx.packets() {
            if stream.index() != stream_index {
                continue;
            }
            if decoder.send_packet(&packet).is_err() {
                continue;
            }
            while decoder.receive_frame(&mut raw_frame).is_ok() {
                append_frame(&raw_frame, channels, &mut samples)?;
            }
        }
        let _ = decoder.send_eof();
        while decoder.receive_frame(&mut raw_frame).is_ok() {
            append_frame(&raw_frame, channels, &mut samples)?;
        }

        if samples.is_empty() {
            return Err(io::Error::other(format!(
                "Audio file '{}' contains no samples",
                path.display()
            )));
        }
        Ok((samples, channels, sample_rate))
    }

    fn write_wav_f32_via_ffmpeg(
        path: &Path,
        samples: &[f32],
        channels: usize,
        sample_rate: u32,
    ) -> io::Result<()> {
        Self::write_wav_via_ffmpeg_codec(
            path,
            samples,
            channels,
            sample_rate,
            "pcm_f32le",
            ExportDither::None,
        )
    }

    fn write_wav_via_ffmpeg_codec(
        path: &Path,
        samples: &[f32],
        channels: usize,
        sample_rate: u32,
        codec: &str,
        dither: ExportDither,
    ) -> io::Result<()> {
        let bits_per_sample = match codec {
            "pcm_s16le" => 16u16,
            "pcm_s24le" => 24u16,
            "pcm_s32le" => 32u16,
            "pcm_f32le" => 32u16,
            other => {
                return Err(io::Error::other(format!("Unsupported WAV codec '{other}'")));
            }
        };
        let is_float = codec == "pcm_f32le";
        Self::write_wav_pcm(
            path,
            samples,
            channels.max(1),
            sample_rate,
            bits_per_sample,
            is_float,
            dither,
        )
    }

    fn write_wav_pcm(
        path: &Path,
        samples: &[f32],
        channels: usize,
        sample_rate: u32,
        bits_per_sample: u16,
        is_float: bool,
        dither: ExportDither,
    ) -> io::Result<()> {
        let bytes_per_sample = usize::from(bits_per_sample / 8);
        let block_align = (channels * bytes_per_sample) as u16;
        let byte_rate = sample_rate * u32::from(block_align);
        let data_size = samples
            .len()
            .checked_mul(bytes_per_sample)
            .ok_or_else(|| io::Error::other("WAV data too large"))? as u32;
        let riff_size = 36u32
            .checked_add(data_size)
            .ok_or_else(|| io::Error::other("WAV file too large"))?;

        let mut file = File::create(path)?;
        file.write_all(b"RIFF")?;
        file.write_all(&riff_size.to_le_bytes())?;
        file.write_all(b"WAVE")?;
        file.write_all(b"fmt ")?;
        file.write_all(&16u32.to_le_bytes())?;
        let audio_format: u16 = if is_float { 3 } else { 1 };
        file.write_all(&audio_format.to_le_bytes())?;
        file.write_all(&(channels as u16).to_le_bytes())?;
        file.write_all(&sample_rate.to_le_bytes())?;
        file.write_all(&byte_rate.to_le_bytes())?;
        file.write_all(&block_align.to_le_bytes())?;
        file.write_all(&bits_per_sample.to_le_bytes())?;
        file.write_all(b"data")?;
        file.write_all(&data_size.to_le_bytes())?;

        let mut rng = Self::dither_rng(0x1234_5678_9abc_defe);
        let apply_dither = !is_float
            && Self::dither_is_applicable(
                match bits_per_sample {
                    16 => ExportBitDepth::Int16,
                    24 => ExportBitDepth::Int24,
                    32 => ExportBitDepth::Int32,
                    _ => ExportBitDepth::Float32,
                },
                dither,
            );
        for &sample in samples {
            let s = sample.clamp(-1.0, 1.0);
            match (is_float, bits_per_sample) {
                (true, 32) => file.write_all(&s.to_le_bytes())?,
                (false, 16) => {
                    let scale = i16::MAX as f32;
                    let q = if apply_dither {
                        Self::quantize_with_dither(s, scale, &mut rng, dither)
                    } else {
                        s * scale
                    }
                    .round()
                    .clamp(i16::MIN as f32, i16::MAX as f32) as i16;
                    file.write_all(&q.to_le_bytes())?;
                }
                (false, 24) => {
                    let scale = 8_388_607.0;
                    let q = if apply_dither {
                        Self::quantize_with_dither(s, scale, &mut rng, dither)
                    } else {
                        s * scale
                    }
                    .round()
                    .clamp(-8_388_608.0, 8_388_607.0) as i32;
                    let b = q.to_le_bytes();
                    file.write_all(&b[..3])?;
                }
                (false, 32) => {
                    let scale = i32::MAX as f32;
                    let q = if apply_dither {
                        Self::quantize_with_dither(s, scale, &mut rng, dither)
                    } else {
                        s * scale
                    }
                    .round()
                    .clamp(i32::MIN as f32, i32::MAX as f32) as i32;
                    file.write_all(&q.to_le_bytes())?;
                }
                _ => return Err(io::Error::other("Unsupported WAV format")),
            }
        }
        Ok(())
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

    fn referenced_session_media_paths_from_tracks(
        tracks: &[crate::state::Track],
    ) -> HashSet<String> {
        let mut referenced = HashSet::new();

        for track in tracks {
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

    fn insert_referenced_unused_clip_paths(
        referenced: &mut HashSet<String>,
        audio_clips: &[crate::state::AudioClip],
        midi_clips: &[crate::state::MIDIClip],
    ) {
        for clip in audio_clips {
            Self::insert_referenced_session_media_path(referenced, &clip.name);
            if let Some(source_name) = clip.pitch_correction_source_name.as_deref() {
                Self::insert_referenced_session_media_path(referenced, source_name);
            }
            if let Some(peaks_file) = clip.peaks_file.as_deref() {
                Self::insert_referenced_session_media_path(referenced, peaks_file);
            }
            Self::insert_pitch_correction_cache_reference(referenced, clip);
        }
        for clip in midi_clips {
            Self::insert_referenced_session_media_path(referenced, &clip.name);
        }
    }

    #[cfg(test)]
    fn referenced_session_media_paths(state: &crate::state::StateData) -> HashSet<String> {
        let mut referenced = Self::referenced_session_media_paths_from_tracks(&state.tracks);
        Self::insert_referenced_unused_clip_paths(
            &mut referenced,
            &state.unused_audio_clips,
            &state.unused_midi_clips,
        );
        referenced
    }

    fn referenced_session_media_paths_from_file(path: &Path) -> io::Result<HashSet<String>> {
        #[derive(serde::Deserialize)]
        struct SessionFileTracks {
            tracks: Vec<crate::state::Track>,
            #[serde(default)]
            unused_audio_clips: Vec<crate::state::AudioClip>,
            #[serde(default)]
            unused_midi_clips: Vec<crate::state::MIDIClip>,
        }

        let file = std::fs::File::open(path)?;
        let reader = std::io::BufReader::new(file);
        let session: SessionFileTracks = serde_json::from_reader(reader)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let mut referenced = Self::referenced_session_media_paths_from_tracks(&session.tracks);
        Self::insert_referenced_unused_clip_paths(
            &mut referenced,
            &session.unused_audio_clips,
            &session.unused_midi_clips,
        );
        Ok(referenced)
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
        let report = Self::delete_unused_session_media_files_for(
            &self.state,
            &self.session_branch,
            session_root,
        )?;
        if !report.deleted_clips.is_empty() {
            let _ = CLIENT
                .sender
                .try_send(EngineMessage::Request(Action::DeleteUnusedClips {
                    clip_ids: report.deleted_clips.clone(),
                }));
        }
        Ok(report)
    }

    fn delete_unused_session_media_files_for(
        state: &crate::state::State,
        session_branch: &str,
        session_root: &Path,
    ) -> Result<SessionMediaCleanupReport, String> {
        // Track-only references from the in-memory current branch.
        let mut referenced = {
            let state = state.blocking_read();
            Self::referenced_session_media_paths_from_tracks(&state.tracks)
        };

        // Other branch files contribute track and unused-pool references. The
        // current branch file is skipped: in-memory state is authoritative.
        let current_branch_file = format!("{session_branch}.json");
        if let Ok(entries) = std::fs::read_dir(session_root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
                    continue;
                };
                if ext != "json" {
                    continue;
                }
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                    && stem.starts_with('.')
                {
                    continue;
                }
                if path.file_name().and_then(|s| s.to_str()) == Some(current_branch_file.as_str()) {
                    continue;
                }
                match Self::referenced_session_media_paths_from_file(&path) {
                    Ok(refs) => referenced.extend(refs),
                    Err(_e) => {}
                }
            }
        }

        // Unused clips whose main media path is not referenced anywhere else
        // can be deleted completely. Clips still referenced by a live-view
        // session slot are in use and must survive.
        let mut deletable: Vec<String> = Vec::new();
        {
            let state = state.blocking_read();
            let live_ids = state.session.slot_referenced_clip_ids();
            for clip in &state.unused_audio_clips {
                if live_ids.contains(&clip.id) {
                    continue;
                }
                if Self::normalize_session_media_rel(&clip.name)
                    .is_some_and(|rel| !referenced.contains(&rel))
                {
                    deletable.push(clip.id.clone());
                }
            }
            for clip in &state.unused_midi_clips {
                if live_ids.contains(&clip.id) {
                    continue;
                }
                if Self::normalize_session_media_rel(&clip.name)
                    .is_some_and(|rel| !referenced.contains(&rel))
                {
                    deletable.push(clip.id.clone());
                }
            }
        }

        let mut report = SessionMediaCleanupReport::default();
        if !deletable.is_empty() {
            {
                let mut state = state.blocking_write();
                state
                    .unused_audio_clips
                    .retain(|clip| !deletable.contains(&clip.id));
                state
                    .unused_midi_clips
                    .retain(|clip| !deletable.contains(&clip.id));
            }
            report.deleted_clips = deletable;
        }

        // Surviving unused clips keep their media referenced.
        {
            let state = state.blocking_read();
            Self::insert_referenced_unused_clip_paths(
                &mut referenced,
                &state.unused_audio_clips,
                &state.unused_midi_clips,
            );
        }

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

        for (path, rel) in candidates {
            match fs::remove_file(&path) {
                Ok(()) => report.deleted_files.push(rel),
                Err(e) => report.failed_files.push(format!("{rel} ({e})")),
            }
        }

        Ok(report)
    }

    fn collect_to_session(&mut self) -> Result<String, String> {
        let Some(session_root) = self.session_dir.clone() else {
            return Err("Collect requires an opened/saved session folder".to_string());
        };

        let data_dir = session_root.join("data");
        std::fs::create_dir_all(&data_dir)
            .map_err(|e| format!("Failed to create data directory: {e}"))?;

        let mut clap_refs: Vec<PluginInstanceRef> = Vec::new();
        let mut lv2_refs: Vec<PluginInstanceRef> = Vec::new();

        {
            let state = self.state.blocking_read();
            for (track_name, (plugins, _connections)) in &state.plugin_graphs_by_track {
                for plugin in plugins {
                    let plugin_ref = PluginInstanceRef::Track {
                        track_name: track_name.clone(),
                        instance_id: plugin.instance_id,
                    };
                    if plugin.format.eq_ignore_ascii_case("CLAP") {
                        clap_refs.push(plugin_ref);
                    } else if plugin.format.eq_ignore_ascii_case("LV2") {
                        lv2_refs.push(plugin_ref);
                    }
                }
            }
            for track in &state.tracks {
                for (clip_idx, clip) in track.audio.clips.iter().enumerate() {
                    if let Some(graph) = clip.plugin_graph_json.as_ref()
                        && let Some(plugins) = graph.get("plugins").and_then(Value::as_array)
                    {
                        for plugin in plugins {
                            let format = plugin
                                .get("format")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string();
                            let Some(instance_id) = plugin
                                .get("instance_id")
                                .and_then(Value::as_u64)
                                .map(|n| n as usize)
                            else {
                                continue;
                            };
                            let plugin_ref = PluginInstanceRef::Clip {
                                track_name: track.name.clone(),
                                clip_idx,
                                instance_id,
                            };
                            if format.eq_ignore_ascii_case("CLAP") {
                                clap_refs.push(plugin_ref);
                            } else if format.eq_ignore_ascii_case("LV2") {
                                lv2_refs.push(plugin_ref);
                            }
                        }
                    }
                }
            }
        }

        if clap_refs.is_empty() && lv2_refs.is_empty() {
            return Ok("No CLAP or LV2 plugins to collect".to_string());
        }

        let mut pending_clap_refs = HashSet::new();
        for plugin_ref in &clap_refs {
            pending_clap_refs.insert(plugin_ref.clone());
        }

        self.collect_to_session_operation = Some(CollectToSessionOperation {
            session_root,
            data_dir: data_dir.clone(),
            pending_clap_refs,
            copied_files: HashMap::new(),
        });

        for plugin_ref in &lv2_refs {
            Self::send_resource_dir_action(plugin_ref, "LV2", &data_dir);
        }
        for plugin_ref in &clap_refs {
            Self::send_resource_dir_action(plugin_ref, "CLAP", &data_dir);
            Self::send_clap_file_references_action(plugin_ref);
        }

        Ok("Collecting plugin files...".to_string())
    }

    fn send_resource_dir_action(plugin_ref: &PluginInstanceRef, format: &str, data_dir: &Path) {
        let directory = data_dir.to_string_lossy().to_string();
        let action = match plugin_ref {
            PluginInstanceRef::Track {
                track_name,
                instance_id,
            } => Action::TrackSetPluginResourceDir {
                track_name: track_name.clone(),
                instance_id: *instance_id,
                format: format.to_string(),
                directory,
            },
            PluginInstanceRef::Clip {
                track_name,
                clip_idx,
                instance_id,
            } => Action::ClipSetPluginResourceDir {
                track_name: track_name.clone(),
                clip_idx: *clip_idx,
                instance_id: *instance_id,
                format: format.to_string(),
                directory,
            },
        };
        tokio::spawn(async move {
            let _ = CLIENT.send(EngineMessage::Request(action)).await;
        });
    }

    fn send_clap_file_references_action(plugin_ref: &PluginInstanceRef) {
        let action = match plugin_ref {
            PluginInstanceRef::Track {
                track_name,
                instance_id,
            } => Action::TrackClapFileReferences {
                track_name: track_name.clone(),
                instance_id: *instance_id,
                refs: Vec::new(),
            },
            PluginInstanceRef::Clip {
                track_name,
                clip_idx,
                instance_id,
            } => Action::ClipClapFileReferences {
                track_name: track_name.clone(),
                clip_idx: *clip_idx,
                instance_id: *instance_id,
                refs: Vec::new(),
            },
        };
        tokio::spawn(async move {
            let _ = CLIENT.send(EngineMessage::Request(action)).await;
        });
    }

    fn handle_clap_file_references_response(
        &mut self,
        plugin_ref: &PluginInstanceRef,
        refs: &[(u32, String)],
    ) {
        tracing::info!(?plugin_ref, ?refs, "Received CLAP file references");
        let Some(op) = self.collect_to_session_operation.as_mut() else {
            return;
        };
        if !op.pending_clap_refs.remove(plugin_ref) {
            return;
        }

        for (index, path) in refs {
            if path.is_empty() {
                continue;
            }
            if let Some((src, dst_rel)) = op.collect_file(path) {
                op.copied_files.insert(src.clone(), dst_rel.clone());
                // Tell the plugin the absolute path so it can load the file
                // immediately. The saved session JSON is later rewritten to use
                // relative data/ paths for portability.
                let absolute = op.data_dir.join(&dst_rel["data/".len()..]);
                Self::send_clap_file_reference_update_action(
                    plugin_ref,
                    *index,
                    &absolute.to_string_lossy(),
                );
            }
        }

        if op.pending_clap_refs.is_empty() {
            self.finish_collect_to_session();
        }
    }

    fn send_clap_file_reference_update_action(
        plugin_ref: &PluginInstanceRef,
        index: u32,
        new_path: &str,
    ) {
        let action = match plugin_ref {
            PluginInstanceRef::Track {
                track_name,
                instance_id,
            } => Action::TrackUpdateClapFileReference {
                track_name: track_name.clone(),
                instance_id: *instance_id,
                index,
                path: new_path.to_string(),
            },
            PluginInstanceRef::Clip {
                track_name,
                clip_idx,
                instance_id,
            } => Action::ClipUpdateClapFileReference {
                track_name: track_name.clone(),
                clip_idx: *clip_idx,
                instance_id: *instance_id,
                index,
                path: new_path.to_string(),
            },
        };
        tokio::spawn(async move {
            let _ = CLIENT.send(EngineMessage::Request(action)).await;
        });
    }

    fn finish_collect_to_session(&mut self) {
        let Some(op) = self.collect_to_session_operation.take() else {
            return;
        };

        let copied_count = op.copied_files.len();
        let message = if copied_count == 0 {
            "No external CLAP files needed collecting".to_string()
        } else {
            format!("Collected {copied_count} plugin file(s) into data/")
        };

        if !op.copied_files.is_empty() {
            self.rewrite_collected_plugin_state_paths(&op.session_root, &op.copied_files);
        }

        if let Some(path) = self.session_dir.as_ref()
            && let Err(e) = self.save(path.to_string_lossy().to_string())
        {
            self.state.blocking_write().message = format!("{message}; failed to save session: {e}");
            return;
        }

        self.state.blocking_write().message = message;
    }

    fn read_utf8_string_from_bytes(bytes: &[u8], start: usize) -> Option<(usize, String)> {
        let mut end = start;
        while end < bytes.len() {
            match std::str::from_utf8(&bytes[end..end + 1]) {
                Ok(_)
                    if bytes[end].is_ascii_graphic()
                        || bytes[end] == b' '
                        || bytes[end] == b'\\'
                        || bytes[end] == b'/'
                        || bytes[end] == b'.'
                        || bytes[end] == b'_'
                        || bytes[end] == b'-' => {}
                _ => break,
            }
            end += 1;
        }
        if end - start < 4 {
            return None;
        }
        let s = std::str::from_utf8(&bytes[start..end]).ok()?;
        Some((end, s.to_string()))
    }

    fn rewrite_collected_plugin_state_paths(
        &mut self,
        session_root: &Path,
        copied_files: &HashMap<PathBuf, String>,
    ) {
        let mut state = self.state.blocking_write();
        for (plugins, _connections) in state.plugin_graphs_by_track.values_mut() {
            for plugin in plugins.iter_mut() {
                if let Some(ref mut plugin_state) = plugin.state {
                    *plugin_state =
                        Self::rewrite_plugin_state_paths(plugin_state, session_root, copied_files);
                }
            }
        }
        for track in state.tracks.iter_mut() {
            for clip in track.audio.clips.iter_mut() {
                if let Some(ref mut graph) = clip.plugin_graph_json
                    && let Some(plugins) = graph.get_mut("plugins").and_then(Value::as_array_mut)
                {
                    for plugin in plugins.iter_mut() {
                        if let Some(plugin_state) = plugin.get_mut("state") {
                            *plugin_state = Self::rewrite_plugin_state_paths(
                                plugin_state,
                                session_root,
                                copied_files,
                            );
                        }
                    }
                }
            }
        }
    }

    fn rewrite_plugin_state_paths(
        state: &Value,
        session_root: &Path,
        copied_files: &HashMap<PathBuf, String>,
    ) -> Value {
        match state {
            Value::String(s) => {
                let resolved = Self::resolve_session_external_path(session_root, s);
                if let Some(dst_rel) = copied_files.get(&resolved) {
                    Value::String(dst_rel.clone())
                } else {
                    state.clone()
                }
            }
            Value::Array(arr) => {
                let bytes: Vec<u8> = arr
                    .iter()
                    .filter_map(|v| v.as_u64().map(|n| n as u8))
                    .collect();
                if !bytes.is_empty() {
                    let rewritten = Self::rewrite_bytes_paths(&bytes, session_root, copied_files);
                    if rewritten != bytes {
                        return Value::Array(
                            rewritten
                                .into_iter()
                                .map(|b| Value::Number(serde_json::Number::from(b)))
                                .collect(),
                        );
                    }
                }
                Value::Array(
                    arr.iter()
                        .map(|v| Self::rewrite_plugin_state_paths(v, session_root, copied_files))
                        .collect(),
                )
            }
            Value::Object(obj) => {
                let mut new_obj = serde_json::Map::new();
                for (k, v) in obj {
                    new_obj.insert(
                        k.clone(),
                        Self::rewrite_plugin_state_paths(v, session_root, copied_files),
                    );
                }
                Value::Object(new_obj)
            }
            _ => state.clone(),
        }
    }

    fn rewrite_bytes_paths(
        bytes: &[u8],
        session_root: &Path,
        copied_files: &HashMap<PathBuf, String>,
    ) -> Vec<u8> {
        let mut result = bytes.to_vec();
        let mut offset = 0;
        while offset + 4 <= result.len() {
            if let Some((end, s)) = Self::read_utf8_string_from_bytes(&result, offset) {
                let resolved = Self::resolve_session_external_path(session_root, &s);
                if let Some(dst_rel) = copied_files.get(&resolved) {
                    let before = result.len();
                    result.splice(offset..end, dst_rel.bytes());
                    let after = result.len();
                    offset = end + after - before;
                    continue;
                }
                offset = end;
            } else {
                offset += 1;
            }
        }
        result
    }

    /// Resolves relative `data/<file>` paths inside plugin state to absolute
    /// paths so the plugin can load them after the session is opened.
    fn resolve_collected_plugin_state_paths(state: &Value, session_root: &Path) -> Value {
        Self::resolve_plugin_state_paths_impl(state, session_root)
    }

    fn resolve_plugin_state_paths_impl(value: &Value, session_root: &Path) -> Value {
        match value {
            Value::String(s) => {
                let resolved = Self::resolve_session_relative_path(session_root, s);
                Value::String(resolved.to_string_lossy().to_string())
            }
            Value::Array(arr) => {
                let bytes: Vec<u8> = arr
                    .iter()
                    .filter_map(|v| v.as_u64().map(|n| n as u8))
                    .collect();
                if !bytes.is_empty() {
                    let rewritten = Self::resolve_bytes_paths(&bytes, session_root);
                    if rewritten != bytes {
                        return Value::Array(
                            rewritten
                                .into_iter()
                                .map(|b| Value::Number(serde_json::Number::from(b)))
                                .collect(),
                        );
                    }
                }
                Value::Array(
                    arr.iter()
                        .map(|v| Self::resolve_plugin_state_paths_impl(v, session_root))
                        .collect(),
                )
            }
            Value::Object(obj) => {
                let mut new_obj = serde_json::Map::new();
                for (k, v) in obj {
                    new_obj.insert(
                        k.clone(),
                        Self::resolve_plugin_state_paths_impl(v, session_root),
                    );
                }
                Value::Object(new_obj)
            }
            _ => value.clone(),
        }
    }

    fn resolve_bytes_paths(bytes: &[u8], session_root: &Path) -> Vec<u8> {
        let mut result = bytes.to_vec();
        let mut offset = 0;
        while offset + 4 <= result.len() {
            if let Some((end, s)) = Self::read_utf8_string_from_bytes(&result, offset) {
                let resolved = Self::resolve_session_relative_path(session_root, &s);
                let resolved_str = resolved.to_string_lossy().to_string();
                if resolved_str != s {
                    let before = result.len();
                    result.splice(offset..end, resolved_str.bytes());
                    let after = result.len();
                    offset = end + after - before;
                    continue;
                }
                offset = end;
            } else {
                offset += 1;
            }
        }
        result
    }

    fn resolve_session_relative_path(session_root: &Path, path: &str) -> PathBuf {
        let p = PathBuf::from(path);
        if p.is_absolute() {
            return p;
        }
        if let Some(rest) = path.strip_prefix("data/") {
            session_root.join("data").join(rest)
        } else {
            // Only rewrite explicit data/ paths; leave other relative strings
            // (e.g. plugin-specific state markers) untouched.
            p
        }
    }

    fn resolve_session_external_path(session_root: &Path, path: &str) -> PathBuf {
        let p = PathBuf::from(path);
        if p.is_absolute() {
            p
        } else {
            session_root.join(&p)
        }
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
        use ffmpeg_next::{
            format::Sample as FfSample, format::sample::Type as SampleType,
            media::Type as MediaType,
        };
        use tokio::sync::mpsc;

        Self::ffmpeg_init().map_err(|e| io::Error::other(format!("FFmpeg init failed: {e}")))?;

        let file_size = path.metadata().map(|m| m.len()).unwrap_or(0);
        let path_owned = path.to_owned();

        let (progress_tx, mut progress_rx) = mpsc::unbounded_channel::<f32>();

        let decode_result = tokio::task::block_in_place(move || {
            let mut ictx = ffmpeg_next::format::input(&path_owned).map_err(|e| {
                io::Error::other(format!(
                    "Unsupported or unreadable audio '{}': {e}",
                    path_owned.display()
                ))
            })?;

            let stream = ictx.streams().best(MediaType::Audio).ok_or_else(|| {
                io::Error::other(format!(
                    "No decodable audio track in '{}'",
                    path_owned.display()
                ))
            })?;

            let stream_index = stream.index();
            let mut decoder = ffmpeg_next::codec::Context::from_parameters(stream.parameters())
                .and_then(|ctx| ctx.decoder().audio())
                .map_err(|e| {
                    io::Error::other(format!("Failed to decode '{}': {e}", path_owned.display()))
                })?;

            let channels = decoder.channels() as usize;
            let sample_rate = decoder.rate();

            let mut samples = Vec::<f32>::new();
            let mut packets_decoded = 0usize;
            let report_interval = 100;

            let mut raw_frame = ffmpeg_next::frame::Audio::empty();
            let append_frame_as_f32 = |frame: &ffmpeg_next::frame::Audio,
                                       channels: usize,
                                       dst: &mut Vec<f32>|
             -> io::Result<()> {
                let frame_samples = frame.samples();
                if frame_samples == 0 || channels == 0 {
                    return Ok(());
                }
                let fmt = frame.format();
                let is_planar = fmt.is_planar();
                match fmt {
                    FfSample::F32(SampleType::Packed) => {
                        let data = frame.data(0);
                        let src = unsafe {
                            std::slice::from_raw_parts(
                                data.as_ptr() as *const f32,
                                frame_samples.saturating_mul(channels),
                            )
                        };
                        dst.extend_from_slice(src);
                    }
                    FfSample::F32(SampleType::Planar) => {
                        for i in 0..frame_samples {
                            for ch in 0..channels {
                                let data = frame.data(ch);
                                let src = unsafe {
                                    std::slice::from_raw_parts(
                                        data.as_ptr() as *const f32,
                                        frame_samples,
                                    )
                                };
                                dst.push(src[i]);
                            }
                        }
                    }
                    FfSample::I16(SampleType::Packed) => {
                        let data = frame.data(0);
                        let src = unsafe {
                            std::slice::from_raw_parts(
                                data.as_ptr() as *const i16,
                                frame_samples.saturating_mul(channels),
                            )
                        };
                        let scale = i16::MAX as f32;
                        dst.extend(src.iter().map(|s| *s as f32 / scale));
                    }
                    FfSample::I16(SampleType::Planar) => {
                        let scale = i16::MAX as f32;
                        for i in 0..frame_samples {
                            for ch in 0..channels {
                                let data = frame.data(ch);
                                let src = unsafe {
                                    std::slice::from_raw_parts(
                                        data.as_ptr() as *const i16,
                                        frame_samples,
                                    )
                                };
                                dst.push(src[i] as f32 / scale);
                            }
                        }
                    }
                    FfSample::I32(SampleType::Packed) => {
                        let data = frame.data(0);
                        let src = unsafe {
                            std::slice::from_raw_parts(
                                data.as_ptr() as *const i32,
                                frame_samples.saturating_mul(channels),
                            )
                        };
                        let scale = i32::MAX as f32;
                        dst.extend(src.iter().map(|s| *s as f32 / scale));
                    }
                    FfSample::I32(SampleType::Planar) => {
                        let scale = i32::MAX as f32;
                        for i in 0..frame_samples {
                            for ch in 0..channels {
                                let data = frame.data(ch);
                                let src = unsafe {
                                    std::slice::from_raw_parts(
                                        data.as_ptr() as *const i32,
                                        frame_samples,
                                    )
                                };
                                dst.push(src[i] as f32 / scale);
                            }
                        }
                    }
                    _ => {
                        return Err(io::Error::other(format!(
                            "Unsupported decoded audio format '{}': {:?} ({})",
                            path_owned.display(),
                            fmt,
                            if is_planar { "planar" } else { "packed" }
                        )));
                    }
                }
                Ok(())
            };

            for (stream, packet) in ictx.packets() {
                if stream.index() != stream_index {
                    continue;
                }
                if decoder.send_packet(&packet).is_err() {
                    continue;
                }
                while decoder.receive_frame(&mut raw_frame).is_ok() {
                    append_frame_as_f32(&raw_frame, channels, &mut samples)?;
                }
                packets_decoded += 1;
                if packets_decoded.is_multiple_of(report_interval) {
                    let bytes_read = samples.len() * std::mem::size_of::<f32>();
                    let progress = if file_size > 0 {
                        (bytes_read as f64 / file_size as f64).clamp(0.0, 1.0) as f32
                    } else {
                        0.0
                    };
                    let _ = progress_tx.send(progress);
                }
            }

            let _ = decoder.send_eof();
            while decoder.receive_frame(&mut raw_frame).is_ok() {
                append_frame_as_f32(&raw_frame, channels, &mut samples)?;
            }

            Ok::<_, io::Error>((samples, channels, sample_rate))
        });

        while let Ok(p) = progress_rx.try_recv() {
            progress_callback(p);
        }

        let (samples, channels, sample_rate) = decode_result?;

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

        use rubato::audioadapter_buffers::direct::SequentialSliceOfVecs;
        use rubato::{
            Async, FixedAsync, Resampler, SincInterpolationParameters, SincInterpolationType,
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

        let mut resampler = Async::<f32>::new_sinc(
            to_rate as f64 / from_rate as f64,
            2.0,
            &params,
            samples.len() / channels.max(1),
            channels.max(1),
            FixedAsync::Input,
        )
        .map_err(|e| io::Error::other(format!("Failed to create resampler: {e}")))?;

        progress_callback(0.2);
        tokio::task::yield_now().await;

        let frames = samples.len() / channels.max(1);
        let mut channel_buffers: Vec<Vec<f32>> = vec![Vec::with_capacity(frames); channels];

        #[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
        unsafe {
            if channels == 2 && std::arch::is_x86_feature_detected!("sse") {
                channel_buffers[0].resize(frames, 0.0);
                channel_buffers[1].resize(frames, 0.0);
                let n = frames / 4;
                for i in 0..n {
                    let src = std::arch::x86_64::_mm_loadu_ps(samples.as_ptr().add(i * 8));
                    let src2 = std::arch::x86_64::_mm_loadu_ps(samples.as_ptr().add(i * 8 + 4));
                    let left = std::arch::x86_64::_mm_shuffle_ps(src, src2, 0b10_00_10_00);
                    let right = std::arch::x86_64::_mm_shuffle_ps(src, src2, 0b11_01_11_01);
                    std::arch::x86_64::_mm_storeu_ps(
                        channel_buffers[0].as_mut_ptr().add(i * 4),
                        left,
                    );
                    std::arch::x86_64::_mm_storeu_ps(
                        channel_buffers[1].as_mut_ptr().add(i * 4),
                        right,
                    );
                }
                for i in n * 4..frames {
                    channel_buffers[0][i] = samples[i * 2];
                    channel_buffers[1][i] = samples[i * 2 + 1];
                }
            } else {
                for (i, &sample) in samples.iter().enumerate() {
                    channel_buffers[i % channels].push(sample);
                }
            }
        }
        #[cfg(not(any(target_arch = "x86_64", target_arch = "x86")))]
        {
            for (i, &sample) in samples.iter().enumerate() {
                channel_buffers[i % channels].push(sample);
            }
        }

        progress_callback(0.5);
        tokio::task::yield_now().await;

        let input = SequentialSliceOfVecs::new(&channel_buffers, channels.max(1), frames)
            .map_err(|e| io::Error::other(format!("Resampler input error: {e}")))?;
        let resampled = resampler
            .process(&input, 0, None)
            .map_err(|e| io::Error::other(format!("Resampling failed: {e}")))?;

        progress_callback(1.0);
        Ok(resampled.take_data())
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

        Self::write_wav_f32_via_ffmpeg(&dst, &final_samples, channels, target_sample_rate)?;

        progress_callback(0.95, Some("Calculating peaks".to_string()));
        tokio::task::yield_now().await;
        let peaks = Self::compute_audio_clip_peaks(&dst)?;

        progress_callback(1.0, None);
        let frames = final_samples.len() / channels.max(1);
        Ok((rel, channels.max(1), frames.max(1), peaks))
    }

    fn timestretch_params(
        stretch_ratio: f32,
        sample_rate: u32,
        channels: usize,
        formant_compensation: bool,
    ) -> io::Result<timestretch::StretchParams> {
        if channels > 2 {
            return Err(io::Error::other(format!(
                "Time stretching supports mono or stereo clips, got {channels} channels"
            )));
        }

        let mut params = timestretch::StretchParams::new(stretch_ratio.max(0.01) as f64)
            .with_sample_rate(sample_rate)
            .with_channels(channels.max(1) as u32);
        if !formant_compensation {
            params.envelope_preservation = false;
            params.envelope_strength = 0.0;
        }
        Ok(params)
    }

    fn fit_interleaved_frames(mut samples: Vec<f32>, channels: usize, frames: usize) -> Vec<f32> {
        let target_len = frames.saturating_mul(channels.max(1));
        samples.resize(target_len, 0.0);
        samples
    }

    async fn stretch_audio_clip_with_timestretch(
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
        let output_rel =
            Self::unique_import_rel_path(session_root, "audio", &format!("{stem}_stretch"), "wav")?;
        let output_path = session_root.join(&output_rel);

        let params = Self::timestretch_params(stretch_ratio, sample_rate, channels, true)?;
        let stretched = timestretch::stretch(segment_samples, &params)
            .map_err(|e| io::Error::other(format!("Failed to stretch audio clip: {e}")))?;
        tokio::task::yield_now().await;
        Self::write_wav_f32_via_ffmpeg(&output_path, &stretched, channels, sample_rate)?;

        let output_frames = stretched.len() / channels;
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
    async fn render_audio_clip_pitch_correction_with_timestretch<F>(
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
        let output_rel = Self::unique_import_rel_path(
            session_root,
            "audio",
            &format!("{stem}_pitch_corrected"),
            "wav",
        )?;
        let output_path = session_root.join(&output_rel);

        let mut pitch_events = Vec::new();
        if points.is_empty() {
            pitch_events.push((0, 0.0));
        } else {
            let mut sorted_points = points.to_vec();
            sorted_points.sort_by_key(|point| point.start_sample);
            let inertia_frames = ((sample_rate as u64 * inertia_ms as u64) / 1000) as usize;
            let mut previous_shift =
                sorted_points[0].target_midi_pitch - sorted_points[0].detected_midi_pitch;
            if sorted_points[0].start_sample > 0 {
                pitch_events.push((0, previous_shift));
            }
            for point in sorted_points {
                let start_sample = point.start_sample.min(segment_frames.saturating_sub(1));
                let target_shift = point.target_midi_pitch - point.detected_midi_pitch;
                if inertia_frames == 0 || (target_shift - previous_shift).abs() <= f32::EPSILON {
                    pitch_events.push((start_sample, target_shift));
                } else {
                    pitch_events.push((start_sample, previous_shift));
                    let glide_end = start_sample
                        .saturating_add(inertia_frames)
                        .min(segment_frames.saturating_sub(1));
                    if glide_end > start_sample {
                        pitch_events.push((glide_end, target_shift));
                    } else {
                        pitch_events.push((start_sample, target_shift));
                    }
                }
                previous_shift = target_shift;
            }
        }

        pitch_events.sort_by_key(|(sample, _)| *sample);
        pitch_events.dedup_by_key(|(sample, _)| *sample);
        if pitch_events.first().is_none_or(|(sample, _)| *sample != 0) {
            pitch_events.insert(0, (0, 0.0));
        }
        progress_callback(0.60, Some("Rendering".to_string()));
        tokio::task::yield_now().await;

        let base_params =
            Self::timestretch_params(1.0, sample_rate, channels, formant_compensation)?;
        let mut rendered = Vec::with_capacity(segment_samples.len());
        for (index, (event_start, semitones)) in pitch_events.iter().copied().enumerate() {
            let event_end = pitch_events
                .get(index + 1)
                .map(|(sample, _)| *sample)
                .unwrap_or(segment_frames)
                .min(segment_frames);
            if event_end <= event_start {
                continue;
            }
            let chunk_start = event_start * channels;
            let chunk_frames = event_end - event_start;
            let chunk_end = chunk_start + chunk_frames * channels;
            let chunk = &segment_samples[chunk_start..chunk_end];
            let corrected = if semitones.abs() <= f32::EPSILON {
                chunk.to_vec()
            } else {
                let pitch_factor = 2.0_f64.powf(semitones as f64 / 12.0);
                timestretch::pitch_shift(chunk, &base_params, pitch_factor).map_err(|e| {
                    io::Error::other(format!("Failed to render pitch correction: {e}"))
                })?
            };
            rendered.extend(Self::fit_interleaved_frames(
                corrected,
                channels,
                chunk_frames,
            ));
        }

        Self::write_wav_f32_via_ffmpeg(&output_path, &rendered, channels, sample_rate)?;
        let output_frames = rendered.len() / channels;
        progress_callback(0.95, Some("Calculating peaks".to_string()));
        tokio::task::yield_now().await;
        let peaks = Self::compute_audio_clip_peaks(&output_path)?;
        progress_callback(1.0, Some("Complete".to_string()));
        Ok((output_rel, output_frames.max(1), peaks))
    }

    /// Tiny deterministic PRNG used for export dither.
    fn dither_rng(seed: u64) -> impl Iterator<Item = u64> {
        let mut state = seed.max(1);
        std::iter::from_fn(move || {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            state = state.wrapping_mul(0x2545_f491_4f6c_dd1d);
            Some(state)
        })
    }

    fn dither_is_applicable(bit_depth: ExportBitDepth, dither: ExportDither) -> bool {
        !matches!(dither, ExportDither::None) && !matches!(bit_depth, ExportBitDepth::Float32)
    }

    fn quantize_with_dither(
        sample: f32,
        scale: f32,
        rng: &mut impl Iterator<Item = u64>,
        dither: ExportDither,
    ) -> f32 {
        let mut uniform_half = || {
            let u = rng.next().unwrap_or(0) >> 32;
            (u as f32 / 4_294_967_296.0) - 0.5
        };
        let d = match dither {
            ExportDither::None => 0.0,
            ExportDither::Rectangular => uniform_half(),
            ExportDither::Triangular => uniform_half() + uniform_half(),
        };
        (sample + d / scale).clamp(-1.0, 1.0) * scale
    }

    fn write_wav_with_bit_depth(
        export_path: &Path,
        mixed_buffer: &[f32],
        sample_rate: i32,
        output_channels: usize,
        bit_depth: ExportBitDepth,
        dither: ExportDither,
    ) -> io::Result<()> {
        let codec = match bit_depth {
            ExportBitDepth::Int16 => "pcm_s16le",
            ExportBitDepth::Int24 => "pcm_s24le",
            ExportBitDepth::Int32 => "pcm_s32le",
            ExportBitDepth::Float32 => "pcm_f32le",
        };
        Self::write_wav_via_ffmpeg_codec(
            export_path,
            mixed_buffer,
            output_channels,
            sample_rate as u32,
            codec,
            dither,
        )
    }

    fn quantize_samples_for_bit_depth(
        mixed_buffer: &[f32],
        bit_depth: ExportBitDepth,
        dither: ExportDither,
    ) -> (Vec<i32>, u8) {
        let (scale, min, max, bps) = match bit_depth {
            ExportBitDepth::Int16 => (i16::MAX as f32, i16::MIN as f32, i16::MAX as f32, 16),
            ExportBitDepth::Int24 => (8_388_607.0_f32, -8_388_608.0_f32, 8_388_607.0_f32, 24),
            ExportBitDepth::Int32 => (i32::MAX as f32, i32::MIN as f32, i32::MAX as f32, 32),
            ExportBitDepth::Float32 => (8_388_607.0_f32, -8_388_608.0_f32, 8_388_607.0_f32, 24),
        };

        if Self::dither_is_applicable(bit_depth, dither) {
            let mut rng = Self::dither_rng(0x1234_5678_9abc_defe);
            let samples = mixed_buffer
                .iter()
                .map(|s| {
                    Self::quantize_with_dither(s.clamp(-1.0, 1.0), scale, &mut rng, dither)
                        .round()
                        .clamp(min, max) as i32
                })
                .collect();
            return (samples, bps);
        }

        #[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
        {
            use wide::{f32x4, i32x4};
            let mut quantized = Vec::with_capacity(mixed_buffer.len());
            let n = mixed_buffer.len() / 4;
            let vmin_f = f32x4::splat(min);
            let vmax_f = f32x4::splat(max);
            let scale_f = f32x4::splat(scale);
            let vmin_i = i32x4::splat(min as i32);
            let vmax_i = i32x4::splat(max as i32);
            for i in 0..n {
                let chunk = &mixed_buffer[i * 4..(i + 1) * 4];
                let v: f32x4 = [chunk[0], chunk[1], chunk[2], chunk[3]].into();
                let clamped = v.clamp(vmin_f, vmax_f);
                let scaled = clamped * scale_f;
                let rounded = scaled.round_int();
                let clamped_i = rounded.max(vmin_i).min(vmax_i);
                quantized.extend_from_slice(&clamped_i.to_array());
            }
            for s in &mixed_buffer[n * 4..] {
                quantized.push(
                    (*s).clamp(-1.0, 1.0)
                        .mul_add(scale, 0.0)
                        .round()
                        .clamp(min, max) as i32,
                );
            }
            (quantized, bps)
        }
        #[cfg(not(any(target_arch = "x86_64", target_arch = "x86")))]
        {
            let samples = mixed_buffer
                .iter()
                .map(|s| (s.clamp(-1.0, 1.0) * scale).round().clamp(min, max) as i32)
                .collect();
            (samples, bps)
        }
    }

    fn write_flac_with_bit_depth(
        export_path: &Path,
        mixed_buffer: &[f32],
        sample_rate: i32,
        output_channels: usize,
        bit_depth: ExportBitDepth,
        dither: ExportDither,
    ) -> io::Result<()> {
        let (quantized, bits_per_sample) =
            Self::quantize_samples_for_bit_depth(mixed_buffer, bit_depth, dither);
        let use_i16 = bits_per_sample <= 16;

        Self::ffmpeg_init().map_err(|e| io::Error::other(format!("FFmpeg init failed: {e}")))?;

        let mut octx = output(export_path.to_str().unwrap_or("output.flac"))
            .map_err(|e| io::Error::other(format!("Failed to create output context: {e}")))?;

        let encoder_codec = ffmpeg_next::codec::encoder::find(CodecId::FLAC)
            .ok_or_else(|| io::Error::other("FLAC encoder not found"))?;

        let mut encoder_ctx = CodecContext::new_with_codec(encoder_codec);
        // ffmpeg-next exposes no safe setter for this field; set it so 24-bit
        // sources are written as 24-bit FLAC rather than promoted to 32-bit.
        unsafe {
            (*encoder_ctx.as_mut_ptr()).bits_per_raw_sample = i32::from(bits_per_sample);
        }

        let mut encoder = encoder_ctx
            .encoder()
            .audio()
            .map_err(|e| io::Error::other(format!("Failed to create audio encoder: {e}")))?;

        encoder.set_rate(sample_rate);
        encoder.set_channel_layout(ffmpeg_next::channel_layout::ChannelLayout::default(
            output_channels as i32,
        ));
        if use_i16 {
            encoder.set_format(ffmpeg_next::format::Sample::I16(
                ffmpeg_next::format::sample::Type::Packed,
            ));
        } else {
            encoder.set_format(ffmpeg_next::format::Sample::I32(
                ffmpeg_next::format::sample::Type::Packed,
            ));
        }

        let mut output_stream = octx
            .add_stream(encoder_codec)
            .map_err(|e| io::Error::other(format!("Failed to add stream: {e}")))?;
        output_stream.set_parameters(&encoder);

        let mut encoder = encoder
            .open_as(encoder_codec)
            .map_err(|e| io::Error::other(format!("Failed to open encoder: {e}")))?;

        octx.write_header()
            .map_err(|e| io::Error::other(format!("Failed to write header: {e}")))?;

        // Packed 16-bit path needs a narrowed interleaved buffer.
        let quantized_i16: Option<Vec<i16>> = if use_i16 {
            Some(quantized.iter().map(|&s| s as i16).collect())
        } else {
            None
        };

        const FRAME_SIZE: usize = 4096;
        for chunk_start in (0..mixed_buffer.len()).step_by(FRAME_SIZE * output_channels) {
            let chunk_end = (chunk_start + FRAME_SIZE * output_channels).min(mixed_buffer.len());
            let actual_frames = (chunk_end - chunk_start) / output_channels;
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

            if let Some(ref narrow) = quantized_i16 {
                let src = &narrow[chunk_start..chunk_end];
                let dst = frame.data_mut(0).as_mut_ptr().cast::<i16>();
                unsafe {
                    std::ptr::copy_nonoverlapping(src.as_ptr(), dst, src.len());
                }
            } else {
                let src = &quantized[chunk_start..chunk_end];
                let dst = frame.data_mut(0).as_mut_ptr().cast::<i32>();
                unsafe {
                    std::ptr::copy_nonoverlapping(src.as_ptr(), dst, src.len());
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
        let export_path = req.export_path;
        let tmp_path = export_path.with_extension(format!(
            "{}.tmp",
            export_path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("export")
        ));
        let result = match req.format {
            ExportFormat::Wav => Self::write_wav_with_bit_depth(
                &tmp_path,
                req.mixed_buffer,
                req.sample_rate,
                req.output_channels,
                req.bit_depth,
                req.dither,
            ),
            ExportFormat::Flac => Self::write_flac_with_bit_depth(
                &tmp_path,
                req.mixed_buffer,
                req.sample_rate,
                req.output_channels,
                req.bit_depth,
                req.dither,
            ),
            ExportFormat::Mp3 => Self::write_mp3(
                &tmp_path,
                req.mixed_buffer,
                req.sample_rate,
                req.output_channels,
                req.codec,
                req.metadata,
            ),
            ExportFormat::Ogg => Self::write_ogg_vorbis(
                &tmp_path,
                req.mixed_buffer,
                req.sample_rate,
                req.output_channels,
                req.codec,
                req.metadata,
            ),
        };
        if result.is_ok() {
            fs::rename(&tmp_path, export_path)?;
        }
        result
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
                let peak = maolan_engine::simd::peak_abs(mixed_buffer);
                if peak > 0.0 {
                    let target_amp = 10.0_f32.powf(params.target_dbfs / 20.0).clamp(0.0, 1.0);
                    let gain = target_amp / peak;
                    maolan_engine::simd::mul_inplace(mixed_buffer, gain);
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

                maolan_engine::simd::mul_inplace(mixed_buffer, applied_gain);

                if params.tp_limiter {
                    let predicted_tp = measured_tp_amp * applied_gain;
                    if predicted_tp > ceiling_amp && ceiling_amp > 0.0 {
                        maolan_engine::simd::clamp_inplace(mixed_buffer, -ceiling_amp, ceiling_amp);
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
            let (samples, clip_channels, _) =
                Self::decode_audio_to_f32_interleaved_sync(&clip_path).map_err(|e| {
                    io::Error::other(format!(
                        "Failed to decode audio '{}': {}",
                        clip_path.display(),
                        e
                    ))
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
        cancel: &Arc<AtomicBool>,
        bounce_notify: Option<Arc<tokio::sync::Notify>>,
        mut progress_callback: F,
    ) -> std::io::Result<()>
    where
        F: FnMut(f32, Option<String>),
    {
        let export_path = options.export_path.as_path();
        let sample_rate = options.sample_rate;
        let export_formats = options.formats.clone();
        let bit_depth = options.bit_depth;
        let dither = options.dither;
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

        {
            let mut handles = Vec::new();
            for (track_idx, track) in tracks.iter().enumerate() {
                for (clip_idx, clip) in track.clips.iter().enumerate() {
                    if clip.pitch_correction_points.is_empty() {
                        continue;
                    }
                    let source_name = clip
                        .pitch_correction_source_name
                        .clone()
                        .unwrap_or_else(|| clip.name.clone());
                    let source_path = if std::path::PathBuf::from(&source_name).is_absolute() {
                        std::path::PathBuf::from(&source_name)
                    } else {
                        session_root.join(&source_name)
                    };
                    let clip_name = clip.name.clone();
                    let offset = clip.pitch_correction_source_offset.unwrap_or(clip.offset);
                    let length = clip.pitch_correction_source_length.unwrap_or(clip.length);
                    let points = clip.pitch_correction_points.clone();
                    let inertia_ms = clip.pitch_correction_inertia_ms.unwrap_or(100);
                    let formant = clip.pitch_correction_formant_compensation.unwrap_or(true);
                    let session_root = session_root.to_path_buf();
                    handles.push(tokio::spawn(async move {
                        let result = Maolan::render_audio_clip_pitch_correction_with_timestretch(
                            &source_path,
                            &session_root,
                            &clip_name,
                            offset,
                            length,
                            &points,
                            inertia_ms,
                            formant,
                            |_, _| {},
                        )
                        .await;
                        (track_idx, clip_idx, result)
                    }));
                }
            }
            if !handles.is_empty() {
                progress_callback(0.02, Some("Rendering pitch correction...".to_string()));
                tokio::task::yield_now().await;
                for handle in handles {
                    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                        return Err(io::Error::other("Export cancelled".to_string()));
                    }
                    let (track_idx, clip_idx, result) = handle.await.map_err(|e| {
                        io::Error::other(format!("Pitch correction task panicked: {e}"))
                    })?;
                    let (rendered_name, rendered_length, _) = result
                        .map_err(|e| io::Error::other(format!("Pitch correction failed: {e}")))?;
                    tracks[track_idx].clips[clip_idx].name = rendered_name;
                    tracks[track_idx].clips[clip_idx].offset = 0;
                    tracks[track_idx].clips[clip_idx].length = rendered_length.max(1);
                    tracks[track_idx].clips[clip_idx]
                        .pitch_correction_points
                        .clear();
                }
            }
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

        let mut temp_bounce_files: Vec<PathBuf> = Vec::new();
        if matches!(render_mode, ExportRenderMode::StemsPostFader) {
            let has_solo = tracks.iter().any(|track| track.soloed);
            let tracks_to_bounce: Vec<usize> = tracks
                .iter()
                .enumerate()
                .filter(|(_, track)| {
                    selected_tracks.contains(&track.name)
                        && !track.muted
                        && (!has_solo || track.soloed)
                })
                .map(|(idx, _)| idx)
                .collect();

            if !tracks_to_bounce.is_empty() {
                fs::create_dir_all(&stem_dir).map_err(|e| {
                    io::Error::other(format!(
                        "Failed to create stem directory '{}': {}",
                        stem_dir.display(),
                        e
                    ))
                })?;

                struct BounceInfo {
                    track_idx: usize,
                    temp_path: PathBuf,
                    render_length: usize,
                    automation_lanes: Vec<OfflineAutomationLane>,
                }

                let mut bounce_infos: Vec<BounceInfo> = Vec::new();
                for track_idx in &tracks_to_bounce {
                    let track = &tracks[*track_idx];
                    let render_length = track
                        .clips
                        .iter()
                        .map(|clip| clip.start.saturating_add(clip.length))
                        .max()
                        .unwrap_or(0)
                        .max(1);
                    let automation_lanes = {
                        let state_guard = state.read().await;
                        if let Some(t) = state_guard.tracks.iter().find(|t| t.name == track.name) {
                            build_offline_automation_lanes(&t.automation_lanes)
                        } else {
                            Vec::new()
                        }
                    };
                    let temp_path = stem_dir.join(format!(
                        ".maolan_bounce_{}_{}.wav",
                        std::process::id(),
                        Self::sanitize_export_component(&track.name)
                    ));
                    temp_bounce_files.push(temp_path.clone());
                    bounce_infos.push(BounceInfo {
                        track_idx: *track_idx,
                        temp_path,
                        render_length,
                        automation_lanes,
                    });
                }

                progress_callback(0.05, Some("Bouncing tracks...".to_string()));
                tokio::task::yield_now().await;
                for info in &bounce_infos {
                    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                        return Err(io::Error::other("Export cancelled".to_string()));
                    }
                    if let Err(e) = CLIENT
                        .send(EngineMessage::Request(Action::TrackOfflineBounce {
                            track_name: tracks[info.track_idx].name.clone(),
                            output_path: info.temp_path.to_string_lossy().to_string(),
                            start_sample: 0,
                            length_samples: info.render_length,
                            automation_lanes: info.automation_lanes.clone(),
                            apply_fader: true,
                        }))
                        .await
                    {
                        return Err(io::Error::other(format!(
                            "Failed to request bounce for '{}': {e}",
                            tracks[info.track_idx].name
                        )));
                    }
                }

                if let Some(notify) = bounce_notify {
                    notify.notified().await;
                    progress_callback(0.20, Some("Bounces complete".to_string()));
                    tokio::task::yield_now().await;
                }

                for info in &bounce_infos {
                    let bounced_clip = AudioClip {
                        name: info.temp_path.to_string_lossy().to_string(),
                        start: 0,
                        length: info.render_length,
                        offset: 0,
                        input_channel: 0,
                        muted: false,
                        max_length_samples: info.render_length,
                        source_length_samples: info.render_length,
                        peaks_file: None,
                        peaks: ClipPeaks::default(),
                        fade_enabled: false,
                        fade_in_samples: 0,
                        fade_out_samples: 0,
                        pitch_correction_preview_name: None,
                        pitch_correction_source_name: None,
                        pitch_correction_source_offset: None,
                        pitch_correction_source_length: None,
                        pitch_correction_points: vec![],
                        pitch_correction_frame_likeness: None,
                        pitch_correction_inertia_ms: None,
                        pitch_correction_formant_compensation: None,
                        ..Default::default()
                    };
                    tracks[info.track_idx].clips = vec![bounced_clip];
                }
            }
        }

        if total_length == 0 {
            for path in &temp_bounce_files {
                let _ = fs::remove_file(path);
            }
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
                let valid_ports: Vec<(usize, usize)> = routed_ports
                    .into_iter()
                    .filter(|(s, _)| *s < track.output_ports)
                    .collect();
                let is_identity = output_channels == track.output_ports
                    && valid_ports.len() == output_channels
                    && valid_ports.iter().all(|(s, d)| s == d)
                    && valid_ports.iter().map(|(s, _)| *s).max()
                        == Some(output_channels.saturating_sub(1));
                if is_identity {
                    maolan_engine::simd::add_inplace(&mut mixed_buffer, &track_buffer);
                } else if valid_ports.len() == 1 {
                    let (source_port, dest_channel) = valid_ports[0];
                    let mut mixed_idx = dest_channel;
                    let mut track_idx = source_port;
                    for _ in 0..total_length {
                        mixed_buffer[mixed_idx] += track_buffer[track_idx];
                        mixed_idx += output_channels;
                        track_idx += track.output_ports;
                    }
                } else {
                    let is_stereo_swap = output_channels == 2
                        && track.output_ports == 2
                        && valid_ports.len() == 2
                        && valid_ports[0] == (0, 1)
                        && valid_ports[1] == (1, 0);
                    if is_stereo_swap {
                        #[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
                        unsafe {
                            use std::arch::x86_64::*;
                            let mut frame = 0;
                            let end = total_length.saturating_sub(3);
                            while frame < end {
                                let t1 = _mm_loadu_ps(track_buffer.as_ptr().add(frame * 2));
                                let t2 = _mm_loadu_ps(track_buffer.as_ptr().add(frame * 2 + 4));
                                let shuffled1 = _mm_shuffle_ps(t1, t1, 0xB1);
                                let shuffled2 = _mm_shuffle_ps(t2, t2, 0xB1);
                                let m1 = _mm_loadu_ps(mixed_buffer.as_ptr().add(frame * 2));
                                let m2 = _mm_loadu_ps(mixed_buffer.as_ptr().add(frame * 2 + 4));
                                _mm_storeu_ps(
                                    mixed_buffer.as_mut_ptr().add(frame * 2),
                                    _mm_add_ps(m1, shuffled1),
                                );
                                _mm_storeu_ps(
                                    mixed_buffer.as_mut_ptr().add(frame * 2 + 4),
                                    _mm_add_ps(m2, shuffled2),
                                );
                                frame += 4;
                            }
                            for frame in frame..total_length {
                                let track_base = frame * 2;
                                let mixed_base = frame * 2;
                                mixed_buffer[mixed_base + 1] += track_buffer[track_base];
                                mixed_buffer[mixed_base] += track_buffer[track_base + 1];
                            }
                        }
                        #[cfg(not(any(target_arch = "x86_64", target_arch = "x86")))]
                        {
                            for frame in 0..total_length {
                                let track_base = frame * 2;
                                let mixed_base = frame * 2;
                                mixed_buffer[mixed_base + 1] += track_buffer[track_base];
                                mixed_buffer[mixed_base] += track_buffer[track_base + 1];
                            }
                        }
                    } else {
                        for (source_port, dest_channel) in &valid_ports {
                            let mut mixed_idx = *dest_channel;
                            let mut track_idx = *source_port;
                            for _ in 0..total_length {
                                mixed_buffer[mixed_idx] += track_buffer[track_idx];
                                mixed_idx += output_channels;
                                track_idx += track.output_ports;
                            }
                        }
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
                maolan_engine::simd::clamp_inplace(&mut mixed_buffer, -ceiling_amp, ceiling_amp);
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
                    dither,
                    format: *format,
                    codec,
                    metadata: &metadata,
                })?;
            }
            progress_callback(1.0, Some("Complete".to_string()));

            for path in &temp_bounce_files {
                let _ = fs::remove_file(path);
            }
            return Ok(());
        }

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
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(io::Error::other("Export cancelled".to_string()));
            }
            let start = 0.1 + (idx as f32 / selected_tracks.len() as f32) * 0.75;
            progress_callback(start, Some(format!("Rendering stem: {}", track.name)));
            tokio::task::yield_now().await;
            let output_channels = track.output_ports.max(1);

            let is_post_fader = matches!(render_mode, ExportRenderMode::StemsPostFader);
            let can_zero_copy = is_post_fader
                && export_formats.len() == 1
                && export_formats[0] == ExportFormat::Wav
                && bit_depth == ExportBitDepth::Float32
                && !normalize
                && !master_limiter;

            if can_zero_copy {
                let temp_path = track
                    .clips
                    .first()
                    .map(|c| {
                        let p = PathBuf::from(&c.name);
                        if p.is_absolute() {
                            p
                        } else {
                            session_root.join(&c.name)
                        }
                    })
                    .ok_or_else(|| {
                        io::Error::other(format!("No bounced clip for track '{}'", track.name))
                    })?;
                for format in &export_formats {
                    let stem_file = stem_dir.join(format!(
                        "{}_{}.{}",
                        Self::sanitize_export_component(&track.name),
                        stem_mode_label,
                        Self::export_format_extension(*format)
                    ));
                    fs::rename(&temp_path, &stem_file)?;
                }
                continue;
            }

            let _normalize_params = ExportNormalizeParams {
                mode: normalize_mode,
                target_dbfs: normalize_target_dbfs,
                target_lufs: normalize_target_lufs,
                true_peak_dbtp: normalize_true_peak_dbtp,
                tp_limiter: normalize_tp_limiter,
                sample_rate,
                output_channels,
            };

            let stem_buffer = if is_post_fader {
                let temp_path = track
                    .clips
                    .first()
                    .map(|c| {
                        let p = PathBuf::from(&c.name);
                        if p.is_absolute() {
                            p
                        } else {
                            session_root.join(&c.name)
                        }
                    })
                    .ok_or_else(|| {
                        io::Error::other(format!("No bounced clip for track '{}'", track.name))
                    })?;
                let (samples, wav_channels, _) =
                    Self::decode_audio_to_f32_interleaved_sync(&temp_path).map_err(|e| {
                        io::Error::other(format!(
                            "Failed to decode bounced audio '{}': {}",
                            temp_path.display(),
                            e
                        ))
                    })?;
                let wav_frames = samples.len() / wav_channels;
                let mut buf = vec![0.0_f32; total_length * output_channels];
                let copy_frames = wav_frames.min(total_length);
                if wav_channels == output_channels {
                    let copy_samples = copy_frames * output_channels;
                    buf[..copy_samples].copy_from_slice(&samples[..copy_samples]);
                } else if wav_channels == 1 {
                    for ch in 0..output_channels {
                        let dst_offset = ch;
                        for frame in 0..copy_frames {
                            buf[frame * output_channels + dst_offset] = samples[frame];
                        }
                    }
                } else {
                    for frame in 0..copy_frames {
                        for ch in 0..output_channels {
                            let src_ch = ch.min(wav_channels.saturating_sub(1));
                            buf[frame * output_channels + ch] =
                                samples[frame * wav_channels + src_ch];
                        }
                    }
                }
                buf
            } else {
                Self::mix_track_clips_to_channels(
                    &track.clips,
                    session_root,
                    total_length,
                    output_channels,
                    track.level,
                    track.balance,
                    false,
                )?
            };

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
                    dither,
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

        for path in &temp_bounce_files {
            let _ = fs::remove_file(path);
        }
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

    fn plugin_node_to_json(
        node: &maolan_engine::message::PluginGraphNode,
        id_to_index: &std::collections::HashMap<usize, usize>,
    ) -> Option<Value> {
        use maolan_engine::message::PluginGraphNode;
        match node {
            PluginGraphNode::TrackInput => Some(json!({"type":"track_input"})),
            PluginGraphNode::TrackOutput => Some(json!({"type":"track_output"})),
            #[cfg(all(unix, not(target_os = "macos")))]
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

    #[allow(dead_code)]
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
            #[cfg(all(unix, not(target_os = "macos")))]
            "plugin" => runtime_nodes
                .get(v["plugin_index"].as_u64()? as usize)
                .and_then(|node| {
                    matches!(node, PluginGraphNode::Lv2PluginInstance(_)).then(|| node.clone())
                }),
            #[cfg(not(all(unix, not(target_os = "macos"))))]
            "plugin" => None,
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

    fn connectable_ref_to_json(
        ref_: &ConnectableRef,
        id_to_index: &std::collections::HashMap<usize, usize>,
    ) -> Option<Value> {
        match ref_ {
            ConnectableRef::TrackInput => Some(json!({"type": "track_input"})),
            ConnectableRef::TrackOutput => Some(json!({"type": "track_output"})),
            ConnectableRef::ChildTrack(name) => Some(json!({"type": "child_track", "name": name})),
            ConnectableRef::ClapPlugin(id) => id_to_index.get(id).copied().map(|idx| {
                json!({
                    "type": "clap_plugin",
                    "plugin_index": idx,
                })
            }),
            ConnectableRef::Vst3Plugin(id) => id_to_index.get(id).copied().map(|idx| {
                json!({
                    "type": "vst3_plugin",
                    "plugin_index": idx,
                })
            }),
            #[cfg(all(unix, not(target_os = "macos")))]
            ConnectableRef::Lv2Plugin(id) => id_to_index.get(id).copied().map(|idx| {
                json!({
                    "type": "lv2_plugin",
                    "plugin_index": idx,
                })
            }),
        }
    }

    fn connectable_ref_from_json(
        v: &Value,
        index_to_id: &std::collections::HashMap<usize, usize>,
    ) -> Option<ConnectableRef> {
        let t = v["type"].as_str()?;
        match t {
            "track_input" => Some(ConnectableRef::TrackInput),
            "track_output" => Some(ConnectableRef::TrackOutput),
            "child_track" => Some(ConnectableRef::ChildTrack(v["name"].as_str()?.to_string())),
            "clap_plugin" => {
                let idx = v["plugin_index"].as_u64()? as usize;
                index_to_id
                    .get(&idx)
                    .copied()
                    .map(ConnectableRef::ClapPlugin)
            }
            "vst3_plugin" => {
                let idx = v["plugin_index"].as_u64()? as usize;
                index_to_id
                    .get(&idx)
                    .copied()
                    .map(ConnectableRef::Vst3Plugin)
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            "lv2_plugin" => {
                let idx = v["plugin_index"].as_u64()? as usize;
                index_to_id
                    .get(&idx)
                    .copied()
                    .map(ConnectableRef::Lv2Plugin)
            }
            _ => None,
        }
    }

    fn connectable_connection_to_json(
        conn: &ConnectableConnection,
        id_to_index: &std::collections::HashMap<usize, usize>,
    ) -> Option<Value> {
        let from = Self::connectable_ref_to_json(&conn.from, id_to_index)?;
        let to = Self::connectable_ref_to_json(&conn.to, id_to_index)?;
        Some(json!({
            "from": from,
            "from_port": conn.from_port,
            "to": to,
            "to_port": conn.to_port,
            "kind": Self::kind_to_json(conn.kind),
        }))
    }

    fn connectable_connections_from_json(
        arr: &serde_json::Value,
        index_to_id: &std::collections::HashMap<usize, usize>,
    ) -> Vec<ConnectableConnection> {
        arr.as_array()
            .map(|connections| {
                connections
                    .iter()
                    .filter_map(|c| {
                        let from = Self::connectable_ref_from_json(&c["from"], index_to_id)?;
                        let to = Self::connectable_ref_from_json(&c["to"], index_to_id)?;
                        let kind = Self::kind_from_json(&c["kind"])?;
                        let from_port = c["from_port"].as_u64()? as usize;
                        let to_port = c["to_port"].as_u64()? as usize;
                        Some(ConnectableConnection {
                            from,
                            from_port,
                            to,
                            to_port,
                            kind,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn is_user_connectable_connection(conn: &ConnectableConnection) -> bool {
        let involves_child = matches!(conn.from, ConnectableRef::ChildTrack(_))
            || matches!(conn.to, ConnectableRef::ChildTrack(_));
        let involves_plugin = {
            let plugin = matches!(
                conn.from,
                ConnectableRef::ClapPlugin(_) | ConnectableRef::Vst3Plugin(_)
            ) || matches!(
                conn.to,
                ConnectableRef::ClapPlugin(_) | ConnectableRef::Vst3Plugin(_)
            );
            #[cfg(all(unix, not(target_os = "macos")))]
            {
                plugin
                    || matches!(conn.from, ConnectableRef::Lv2Plugin(_))
                    || matches!(conn.to, ConnectableRef::Lv2Plugin(_))
            }
            #[cfg(not(all(unix, not(target_os = "macos"))))]
            {
                plugin
            }
        };
        involves_child || involves_plugin
    }

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
                    "bypassed": plugin.bypassed,
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
    #[cfg(not(all(unix, not(target_os = "macos"))))]
    fn plugin_graph_json_with_saved_plugin_state(
        _graph: Option<&Value>,
        _plugin_index: usize,
        _state: Value,
    ) -> Option<Value> {
        None
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[allow(dead_code)]
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
                    bypassed: plugin
                        .get("bypassed")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
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
                    bypassed: plugin
                        .get("bypassed")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                })
            }
            Some(format) if format.eq_ignore_ascii_case("CLAP") => {
                let info = clap_plugins
                    .iter()
                    .find(|info| info.path == uri)
                    .or_else(|| {
                        clap_plugins.iter().find(|info| {
                            info.path.split_once("::").is_some_and(|(_, id)| id == uri)
                        })
                    });
                let resolved_uri = info
                    .map(|info| info.path.clone())
                    .unwrap_or_else(|| uri.clone());
                let caps = info.and_then(|info| info.capabilities.as_ref());
                Some(PluginGraphPlugin {
                    node: PluginGraphNode::ClapPluginInstance(instance_id),
                    instance_id,
                    format: "CLAP".to_string(),
                    uri: resolved_uri.clone(),
                    plugin_id: resolved_uri
                        .split_once("::")
                        .map(|(_, id)| id.to_string())
                        .unwrap_or_else(|| uri.clone()),
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
                    bypassed: plugin
                        .get("bypassed")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                })
            }
            _ => None,
        }
    }

    #[cfg(not(all(unix, not(target_os = "macos"))))]
    #[allow(dead_code)]
    fn plugin_graph_plugin_from_saved_json(
        instance_id: usize,
        plugin: &Value,
        vst3_plugins: &[maolan_engine::vst3::Vst3PluginInfo],
        clap_plugins: &[maolan_engine::clap::ClapPluginInfo],
    ) -> Option<maolan_engine::message::PluginGraphPlugin> {
        use maolan_engine::message::{PluginGraphNode, PluginGraphPlugin};

        let uri = plugin.get("uri").and_then(Value::as_str)?.to_string();
        match plugin.get("format").and_then(Value::as_str) {
            Some(format) if format.eq_ignore_ascii_case("LV2") => None,
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
                    bypassed: plugin
                        .get("bypassed")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                })
            }
            Some(format) if format.eq_ignore_ascii_case("CLAP") => {
                let info = clap_plugins
                    .iter()
                    .find(|info| info.path == uri)
                    .or_else(|| {
                        clap_plugins.iter().find(|info| {
                            info.path.split_once("::").is_some_and(|(_, id)| id == uri)
                        })
                    });
                let resolved_uri = info
                    .map(|info| info.path.clone())
                    .unwrap_or_else(|| uri.clone());
                let caps = info.and_then(|info| info.capabilities.as_ref());
                Some(PluginGraphPlugin {
                    node: PluginGraphNode::ClapPluginInstance(instance_id),
                    instance_id,
                    format: "CLAP".to_string(),
                    uri: resolved_uri.clone(),
                    plugin_id: resolved_uri
                        .split_once("::")
                        .map(|(_, id)| id.to_string())
                        .unwrap_or_else(|| uri.clone()),
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
                    bypassed: plugin
                        .get("bypassed")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                })
            }
            _ => None,
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[allow(dead_code)]
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

    #[cfg(not(all(unix, not(target_os = "macos"))))]
    #[allow(dead_code)]
    fn plugin_graph_snapshot_from_json(
        graph: Option<&Value>,
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

    fn kind_to_json(kind: Kind) -> Value {
        match kind {
            Kind::Audio => json!("audio"),
            Kind::MIDI => json!("midi"),
        }
    }

    #[allow(dead_code)]
    fn kind_from_json(v: &Value) -> Option<Kind> {
        match v.as_str()? {
            "audio" | "Audio" => Some(Kind::Audio),
            "midi" | "MIDI" => Some(Kind::MIDI),
            _ => None,
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn lv2_state_to_json(state: &[u8]) -> Value {
        json!(state)
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn lv2_state_from_json(v: &Value) -> Option<Vec<u8>> {
        v.as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| item.as_u64().map(|n| n as u8))
                    .collect::<Vec<_>>()
            })
            .filter(|bytes| !bytes.is_empty())
    }

    fn clap_state_from_json(v: &Value) -> Option<maolan_engine::clap::ClapPluginState> {
        serde_json::from_value(v.clone()).ok()
    }

    fn clap_state_from_json_resolved(
        v: &Value,
        session_root: &Path,
    ) -> Option<maolan_engine::clap::ClapPluginState> {
        let resolved = Self::resolve_collected_plugin_state_paths(v, session_root);
        serde_json::from_value(resolved).ok()
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn track_plugin_list_view(&self) -> iced::Element<'_, Message> {
        let state = self.state.blocking_read();
        let title = Self::plugin_graph_title(&state);

        let mut lv2_items = Vec::new();
        let filter = self.plugin_list_filter.trim().to_lowercase();
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
        let clap_filter = filter.clone();
        for plugin in &state.clap_plugins {
            if !clap_filter.is_empty() {
                let name = plugin.name.to_lowercase();
                let id = plugin.id.to_lowercase();
                if !name.contains(&clap_filter) && !id.contains(&clap_filter) {
                    continue;
                }
            }
            let is_selected = self.selected_clap_plugins.contains(&plugin.id);

            let mut capability_icons = String::new();
            if let Some(caps) = &plugin.capabilities {
                if caps.has_gui {
                    capability_icons.push_str("\u{1F5BC} ");
                }
                if caps.has_params {
                    capability_icons.push_str("\u{2699} ");
                }
                if caps.has_state {
                    capability_icons.push_str("\u{1F4BE} ");
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
                    .on_press(Message::SelectClapPlugin(plugin.id.clone()))
                    .into(),
            );
        }
        let clap_list = column(clap_items);

        let mut vst3_items = Vec::new();
        let vst3_filter = filter.clone();
        for plugin in &state.vst3_plugins {
            if !vst3_filter.is_empty() {
                let name = plugin.name.to_lowercase();
                let id = plugin.id.to_lowercase();
                if !name.contains(&vst3_filter) && !id.contains(&vst3_filter) {
                    continue;
                }
            }
            let is_selected = self.selected_vst3_plugins.contains(&plugin.id);
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
                    .on_press(Message::SelectVst3Plugin(plugin.id.clone()))
                    .into(),
            );
        }
        let vst3_list = column(vst3_items);

        let lv2_column: iced::Element<'_, Message> = if state.lv2_plugins_unavailable {
            column![
                text("LV2").size(14),
                text("LV2 plugin scan is unavailable.").size(12),
            ]
            .spacing(10)
            .width(Length::FillPortion(1))
            .into()
        } else {
            column![
                text("LV2").size(14),
                scrollable(lv2_list).height(Length::Fill),
            ]
            .spacing(10)
            .width(Length::FillPortion(1))
            .into()
        };

        let clap_column: iced::Element<'_, Message> = if state.clap_plugins_unavailable {
            column![
                text("CLAP").size(14),
                text("CLAP plugin scan is unavailable.").size(12),
            ]
            .spacing(10)
            .width(Length::FillPortion(1))
            .into()
        } else {
            column![
                text("CLAP").size(14),
                scrollable(clap_list).height(Length::Fill),
            ]
            .spacing(10)
            .width(Length::FillPortion(1))
            .into()
        };

        let vst3_column: iced::Element<'_, Message> = if state.vst3_plugins_unavailable {
            column![
                text("VST3").size(14),
                text("VST3 plugin scan is unavailable.").size(12),
            ]
            .spacing(10)
            .width(Length::FillPortion(1))
            .into()
        } else {
            column![
                text("VST3").size(14),
                scrollable(vst3_list).height(Length::Fill),
            ]
            .spacing(10)
            .width(Length::FillPortion(1))
            .into()
        };

        let selected_count = self.selected_lv2_plugins.len()
            + self.selected_clap_plugins.len()
            + self.selected_vst3_plugins.len();
        let load = if selected_count == 0 {
            button("Load")
        } else {
            button(text(format!("Load ({})", selected_count)))
                .on_press(Message::LoadSelectedPlugins)
        };

        let plugin_columns = row![lv2_column, clap_column, vst3_column]
            .spacing(10)
            .width(Length::Fill)
            .height(Length::Fill);

        container(
            column![
                text(title),
                text_input("Filter plugins...", &self.plugin_list_filter)
                    .on_input(Message::FilterPluginList)
                    .width(Length::Fill),
                plugin_columns,
                row![
                    load,
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

    #[cfg(any(windows, target_os = "macos"))]
    fn track_plugin_list_view(&self) -> iced::Element<'_, Message> {
        let state = self.state.blocking_read();
        let title = Self::plugin_graph_title(&state);
        let filter = self.plugin_list_filter.trim().to_lowercase();
        let mut vst3_items = Vec::new();
        for plugin in &state.vst3_plugins {
            if !filter.is_empty() {
                let name = plugin.name.to_lowercase();
                let id = plugin.id.to_lowercase();
                if !name.contains(&filter) && !id.contains(&filter) {
                    continue;
                }
            }
            let is_selected = self.selected_vst3_plugins.contains(&plugin.id);
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
                    .on_press(Message::SelectVst3Plugin(plugin.id.clone()))
                    .into(),
            );
        }
        let vst3_list = column(vst3_items);

        let mut clap_items = Vec::new();
        let clap_filter = filter.clone();
        for plugin in &state.clap_plugins {
            if !clap_filter.is_empty() {
                let name = plugin.name.to_lowercase();
                let id = plugin.id.to_lowercase();
                if !name.contains(&clap_filter) && !id.contains(&clap_filter) {
                    continue;
                }
            }
            let is_selected = self.selected_clap_plugins.contains(&plugin.id);

            let mut capability_icons = String::new();
            if let Some(caps) = &plugin.capabilities {
                if caps.has_gui {
                    capability_icons.push_str("\u{1F5BC} ");
                }
                if caps.has_params {
                    capability_icons.push_str("\u{2699} ");
                }
                if caps.has_state {
                    capability_icons.push_str("\u{1F4BE} ");
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
                    .on_press(Message::SelectClapPlugin(plugin.id.clone()))
                    .into(),
            );
        }
        let clap_list = column(clap_items);

        let clap_column: iced::Element<'_, Message> = if state.clap_plugins_unavailable {
            column![
                text("CLAP").size(14),
                text("CLAP plugin scan is unavailable.").size(12),
            ]
            .spacing(10)
            .width(Length::FillPortion(1))
            .into()
        } else {
            column![
                text("CLAP").size(14),
                scrollable(clap_list).height(Length::Fill),
            ]
            .spacing(10)
            .width(Length::FillPortion(1))
            .into()
        };

        let vst3_column: iced::Element<'_, Message> = if state.vst3_plugins_unavailable {
            column![
                text("VST3").size(14),
                text("VST3 plugin scan is unavailable.").size(12),
            ]
            .spacing(10)
            .width(Length::FillPortion(1))
            .into()
        } else {
            column![
                text("VST3").size(14),
                scrollable(vst3_list).height(Length::Fill),
            ]
            .spacing(10)
            .width(Length::FillPortion(1))
            .into()
        };

        let selected_count = self.selected_clap_plugins.len() + self.selected_vst3_plugins.len();
        let load = if selected_count == 0 {
            button("Load")
        } else {
            button(text(format!("Load ({})", selected_count)))
                .on_press(Message::LoadSelectedPlugins)
        };

        let plugin_columns = row![clap_column, vst3_column]
            .spacing(10)
            .width(Length::Fill)
            .height(Length::Fill);

        container(
            column![
                text(title),
                text_input("Filter plugins...", &self.plugin_list_filter)
                    .on_input(Message::FilterPluginList)
                    .width(Length::Fill),
                plugin_columns,
                row![
                    load,
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

    fn send(&self, action: Action) -> Task<Message> {
        let action = self.action_with_clip_identity_fork(action);
        tracing::info!("GUI sending request: {:?}", std::mem::discriminant(&action));
        Task::perform(
            async move { CLIENT.send(EngineMessage::Request(action)).await },
            |result| match result {
                Ok(_) => Message::SendMessageFinished(Ok(())),
                Err(_) => Message::Response(Err("Channel closed".to_string())),
            },
        )
    }

    fn send_modulators_to_engine(&self) -> Task<Message> {
        let engine_modulators: Vec<maolan_engine::modulator::Modulator> =
            self.modulators.iter().map(Into::into).collect();
        self.send(Action::SetModulators(engine_modulators))
    }

    fn track_automation_lanes_action(&self, track_name: &str) -> Option<Action> {
        let state = self.state.blocking_read();
        let track = state.tracks.iter().find(|t| t.name == track_name)?;
        let lanes = serde_json::to_value(&track.automation_lanes).unwrap_or_default();
        let mode = track.automation_mode.into();
        Some(Action::SetTrackAutomationLanes {
            track_name: track_name.to_string(),
            lanes,
            mode,
        })
    }

    fn send_track_automation_lanes(&self, track_name: &str) -> Task<Message> {
        self.track_automation_lanes_action(track_name)
            .map(|action| self.send(action))
            .unwrap_or(Task::none())
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
                                text("Dither:"),
                                pick_list(
                                    EXPORT_DITHER_ALL.to_vec(),
                                    Some(self.export_dither),
                                    Message::ExportDitherSelected
                                )
                                .placeholder("Choose dither"),
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
            container(last_message_status_text(&last_message))
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
        #[cfg(any(target_os = "freebsd", target_os = "openbsd", target_os = "linux"))]
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
        #[cfg(target_os = "windows")]
        {
            options.extend(
                state
                    .available_input_hw
                    .iter()
                    .map(|hw| PreferencesDeviceOption {
                        id: hw.clone(),
                        label: hw.clone(),
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
            #[cfg(any(target_os = "freebsd", target_os = "openbsd", target_os = "linux"))]
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
        let mut preferences_column = Column::new().align_x(iced::Alignment::Start).spacing(12);
        preferences_column = preferences_column.push(text("Preferences").size(16));
        preferences_column = preferences_column.push(
            row![
                checkbox(self.prefs_osc_enabled)
                    .label("Enable OSC")
                    .on_toggle(Message::PreferencesOscEnabledToggled),
            ]
            .spacing(10)
            .align_y(iced::Alignment::Center),
        );
        preferences_column = preferences_column.push(
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
        );
        preferences_column = preferences_column.push(
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
        );
        preferences_column = preferences_column.push(
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
        );
        #[cfg(unix)]
        {
            preferences_column = preferences_column.push(
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
            );
        }
        preferences_column = preferences_column.push(
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
        );
        if platform_caps::HAS_SEPARATE_AUDIO_INPUT_DEVICE {
            preferences_column = preferences_column.push(
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
                .align_y(iced::Alignment::Center),
            );
        } else {
            preferences_column = preferences_column
                .push(row![text("")].spacing(10).align_y(iced::Alignment::Center));
        }
        preferences_column = preferences_column.push(
            row![
                button("Save").on_press(Message::PreferencesSave),
                button("Cancel")
                    .on_press(Message::Cancel)
                    .style(button::secondary),
            ]
            .spacing(10),
        );
        container(preferences_column)
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

    fn branch_manager_view(&self) -> iced::Element<'_, Message> {
        let mut branches: Vec<String> = Vec::new();
        if let Some(session_dir) = self.session_dir.as_ref() {
            if let Ok(entries) = std::fs::read_dir(session_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file()
                        && let Some(name) = path.file_stem().and_then(|s| s.to_str())
                        && let Some(ext) = path.extension().and_then(|s| s.to_str())
                        && ext == "json"
                        && name != ".maolan"
                    {
                        branches.push(name.to_string());
                    }
                }
            }
            branches.sort();
        }

        let current = self.session_branch.clone();
        let mut branch_rows: Vec<iced::Element<'_, Message>> = Vec::new();
        for branch in branches {
            if branch == current {
                branch_rows.push(
                    row![text(format!("{} (current)", branch)).width(Length::Fill),]
                        .spacing(4)
                        .align_y(iced::Alignment::Center)
                        .into(),
                );
            } else {
                branch_rows.push(
                    row![
                        text(branch.clone()).width(Length::Fill),
                        button("Switch")
                            .on_press(Message::BranchSwitch(branch.clone()))
                            .style(button::secondary),
                        button("Tracks")
                            .on_press(Message::Show(Show::BranchTrackList(branch.clone())))
                            .style(button::secondary),
                        button("Merge")
                            .on_press(Message::BranchMerge(branch.clone()))
                            .style(button::secondary),
                        button("Reset")
                            .on_press(Message::BranchResetHard(branch.clone()))
                            .style(button::danger),
                    ]
                    .spacing(4)
                    .align_y(iced::Alignment::Center)
                    .into(),
                );
            }
        }

        let content = column![
            text("Branches").size(16),
            text(format!("Current: {}", current)),
            text("Existing branches:"),
            column(branch_rows).spacing(6).width(Length::Fixed(400.0)),
            row![
                text_input("new branch name", &self.pending_branch_input)
                    .on_input(Message::BranchInput)
                    .width(Length::Fixed(200.0)),
                button("Create").on_press(Message::BranchCreate(self.pending_branch_input.clone()))
            ]
            .spacing(10)
            .align_y(iced::Alignment::Center),
            button("Close")
                .on_press(Message::Cancel)
                .style(button::secondary),
        ]
        .align_x(iced::Alignment::Start)
        .spacing(12);

        container(content)
            .style(|_theme| crate::style::app_background())
            .padding(20)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(iced::Alignment::Center)
            .align_y(iced::Alignment::Center)
            .into()
    }

    fn branch_track_list_view(&self, branch: &str) -> iced::Element<'_, Message> {
        let mut track_rows: Vec<iced::Element<'_, Message>> = Vec::new();

        if let Some(session_dir) = self.session_dir.as_ref() {
            let branch_file = session_dir.join(format!("{}.json", branch));
            if branch_file.exists()
                && let Ok(file) = std::fs::File::open(&branch_file)
            {
                let reader = std::io::BufReader::new(file);
                if let Ok(json) = serde_json::from_reader::<_, serde_json::Value>(reader)
                    && let Some(tracks) = json.get("tracks").and_then(|v| v.as_array())
                {
                    for track in tracks {
                        if let Some(name) =
                            track.get("name").and_then(|v| v.as_str()).map(String::from)
                        {
                            track_rows.push(
                                row![
                                    text(name.clone()).width(Length::Fill),
                                    button("Copy")
                                        .on_press(Message::BranchCopyTrack {
                                            branch: branch.to_string(),
                                            track_name: name,
                                        })
                                        .style(button::secondary),
                                ]
                                .spacing(10)
                                .align_y(iced::Alignment::Center)
                                .into(),
                            );
                        }
                    }
                }
            }
        }

        let content = column![
            text(format!("Tracks in '{}'", branch)).size(16),
            column(track_rows).spacing(6).width(Length::Fixed(400.0)),
            button("Back")
                .on_press(Message::Show(Show::BranchManager))
                .style(button::secondary),
        ]
        .align_x(iced::Alignment::Start)
        .spacing(12);

        container(content)
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

    fn about_view(&self) -> iced::Element<'_, Message> {
        container(
            column![
                text("Maolan DAW").size(20),
                text(format!("Version {}", env!("CARGO_PKG_VERSION"))).size(12),
                text("License: BSD 2-Clause License").size(12),
                button(text("https://maolan.rs").color(Color::from_rgb(0.36, 0.66, 0.98)))
                    .on_press(Message::OpenUrl("https://maolan.rs".to_string()))
                    .style(button::text),
                button("Close")
                    .on_press(Message::Cancel)
                    .style(button::secondary),
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

    fn track_color_view(&self, track_name: String) -> iced::Element<'_, Message> {
        let state = self.state.blocking_read();
        let track = state.tracks.iter().find(|t| t.name == track_name);
        let current_color = track
            .and_then(|t| t.color)
            .unwrap_or(Color::from_rgb(0.5, 0.5, 0.5));
        drop(state);
        let color_track_name = track_name.clone();
        container(
            column![
                text(format!("Color for {}", track_name)).size(16),
                color_picker_with_change(
                    true,
                    current_color,
                    container("").width(Length::Fill).height(Length::Fill),
                    Message::TrackColorClear(track_name),
                    move |_color| Message::Cancel,
                    move |color| Message::TrackColorChanged {
                        track_name: color_track_name.clone(),
                        color: Some(color),
                    },
                ),
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
        self.add_track.update(message);
        self.apply_template.update(message);
        self.clip_rename.update(message);
        self.track_rename.update(message);
        self.scene_rename.update(message);
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

    #[cfg(unix)]
    fn test_jack_graph() -> maolan_engine::message::JackGraphInfo {
        use maolan_engine::message::{JackConnectionInfo, JackGraphInfo, JackPortInfo};
        JackGraphInfo {
            ports: vec![
                JackPortInfo {
                    name: "system:capture_1".to_string(),
                    kind: Kind::Audio,
                    is_input: false,
                    is_output: true,
                    is_physical: true,
                    is_maolan: false,
                },
                JackPortInfo {
                    name: "maolan:in_1".to_string(),
                    kind: Kind::Audio,
                    is_input: true,
                    is_output: false,
                    is_physical: false,
                    is_maolan: true,
                },
            ],
            connections: vec![JackConnectionInfo {
                source: "system:capture_1".to_string(),
                destination: "maolan:in_1".to_string(),
            }],
        }
    }

    #[cfg(unix)]
    fn select_non_jack_backend(state: &mut crate::state::StateData) {
        if let Some(backend) = state
            .available_backends
            .iter()
            .find(|backend| !matches!(backend, crate::state::AudioBackendOption::Jack))
            .cloned()
        {
            state.selected_backend = backend;
        }
    }

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
    #[cfg(all(unix, not(target_os = "macos")))]
    fn plugin_graph_snapshot_resolves_portable_clap_plugin_id() {
        let graph = json!({
            "plugins": [{
                "format": "CLAP",
                "uri": "rs.maolan.synth",
                "state": null
            }],
            "connections": [{
                "from_node": {"type": "clap_plugin", "plugin_index": 0},
                "from_port": 0,
                "to_node": {"type": "track_output"},
                "to_port": 0,
                "kind": "audio"
            }]
        });
        let clap_plugins = vec![maolan_engine::clap::ClapPluginInfo {
            id: "rs.maolan.synth".to_string(),
            name: "Maolan Synth".to_string(),
            path: "/home/meka/.clap/Maolan.clap::rs.maolan.synth".to_string(),
            capabilities: None,
        }];

        let (plugins, connections) =
            Maolan::plugin_graph_snapshot_from_json(Some(&graph), &[], &[], &clap_plugins);

        assert_eq!(plugins.len(), 1);
        assert_eq!(
            plugins[0].uri,
            "/home/meka/.clap/Maolan.clap::rs.maolan.synth"
        );
        assert_eq!(plugins[0].plugin_id, "rs.maolan.synth");
        assert_eq!(plugins[0].name, "Maolan Synth");
        assert_eq!(connections.len(), 1);
        assert_eq!(
            connections[0].from_node,
            PluginGraphNode::ClapPluginInstance(0)
        );
        assert_eq!(connections[0].to_node, PluginGraphNode::TrackOutput);
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
    fn session_save_and_load_roundtrip_preserves_master_output_level() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let session_root = std::env::temp_dir().join(format!("maolan_master_session_{unique}"));

        let app = Maolan::default();
        {
            let mut state = app.state.blocking_write();
            state.hw_out_level = -7.5;
            state.hw_out_balance = 0.25;
        }
        app.save(session_root.to_string_lossy().to_string())
            .expect("save session");

        let session_path = session_root.join("main.json");
        let session: serde_json::Value =
            serde_json::from_reader(File::open(&session_path).expect("open saved session"))
                .expect("parse saved session");
        assert_eq!(session["transport"]["hw_out_level"].as_f64(), Some(-7.5));
        assert_eq!(session["transport"]["hw_out_balance"].as_f64(), Some(0.25));

        let mut restored = Maolan::default();
        {
            let mut state = restored.state.blocking_write();
            state.hw_out_level = 0.0;
            state.hw_out_balance = 0.0;
        }
        let _ = restored
            .load(session_root.to_string_lossy().to_string())
            .expect("load session");

        {
            let state = restored.state.blocking_read();
            assert!((state.hw_out_level - -7.5).abs() < f32::EPSILON);
            assert!((state.hw_out_balance - 0.25).abs() < f32::EPSILON);
        }

        fs::remove_dir_all(&session_root).expect("cleanup temp session");
    }

    #[test]
    fn session_save_and_load_roundtrip_preserves_track_positions() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let session_root = std::env::temp_dir().join(format!("maolan_track_pos_session_{unique}"));

        let app = Maolan::default();
        {
            let mut state = app.state.blocking_write();
            state.tracks.push(crate::state::Track::new(
                "Drums".to_string(),
                0.0,
                2,
                2,
                0,
                0,
            ));
            if let Some(track) = state.tracks.iter_mut().find(|t| t.name == "Drums") {
                track.position = iced::Point::new(123.0, 456.0);
            }
        }
        app.save(session_root.to_string_lossy().to_string())
            .expect("save session");

        let session_path = session_root.join("main.json");
        let session: serde_json::Value =
            serde_json::from_reader(File::open(&session_path).expect("open saved session"))
                .expect("parse saved session");
        let track = session["tracks"]
            .as_array()
            .expect("tracks array")
            .iter()
            .find(|t| t["name"].as_str() == Some("Drums"))
            .expect("Drums track");
        assert!((track["position"]["x"].as_f64().unwrap_or(0.0) - 123.0).abs() < f64::EPSILON);
        assert!((track["position"]["y"].as_f64().unwrap_or(0.0) - 456.0).abs() < f64::EPSILON);

        let mut restored = Maolan::default();
        let _ = restored
            .load(session_root.to_string_lossy().to_string())
            .expect("load session");
        {
            let state = restored.state.blocking_read();
            let pending = state
                .pending_track_positions
                .get("Drums")
                .copied()
                .expect("pending position for Drums");
            assert!((pending.x - 123.0).abs() < f32::EPSILON);
            assert!((pending.y - 456.0).abs() < f32::EPSILON);
        }

        let _ = restored.update(Message::Response(Ok(
            maolan_engine::message::Action::AddTrack {
                name: "Drums".to_string(),
                audio_ins: 2,
                audio_outs: 2,
                midi_ins: 0,
                midi_outs: 0,
                folder: false,
            },
        )));
        {
            let state = restored.state.blocking_read();
            let track = state
                .tracks
                .iter()
                .find(|t| t.name == "Drums")
                .expect("Drums track after response");
            assert!((track.position.x - 123.0).abs() < f32::EPSILON);
            assert!((track.position.y - 456.0).abs() < f32::EPSILON);
        }

        fs::remove_dir_all(&session_root).expect("cleanup temp session");
    }

    #[cfg(unix)]
    #[test]
    fn session_save_with_jack_writes_current_jack_routing() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let session_root = std::env::temp_dir().join(format!("maolan_jack_save_{unique}"));
        let app = Maolan::default();
        {
            let mut state = app.state.blocking_write();
            state.selected_backend = crate::state::AudioBackendOption::Jack;
            state.jack_graph = test_jack_graph();
        }

        app.save(session_root.to_string_lossy().to_string())
            .expect("save session");

        let session: serde_json::Value = serde_json::from_reader(
            File::open(session_root.join("main.json")).expect("open saved session"),
        )
        .expect("parse saved session");
        assert_eq!(
            session["jack_routing"]["connections"][0]["source"].as_str(),
            Some("system:capture_1")
        );
        assert_eq!(
            session["jack_routing"]["connections"][0]["destination"].as_str(),
            Some("maolan:in_1")
        );

        fs::remove_dir_all(&session_root).expect("cleanup temp session");
    }

    #[cfg(unix)]
    #[test]
    fn session_save_with_non_jack_preserves_loaded_jack_routing() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let session_root = std::env::temp_dir().join(format!("maolan_jack_preserve_{unique}"));
        let app = Maolan::default();
        {
            let mut state = app.state.blocking_write();
            select_non_jack_backend(&mut state);
            state.jack_session_routing = Some(test_jack_graph());
            state.jack_graph = maolan_engine::message::JackGraphInfo::default();
        }

        app.save(session_root.to_string_lossy().to_string())
            .expect("save session");

        let session: serde_json::Value = serde_json::from_reader(
            File::open(session_root.join("main.json")).expect("open saved session"),
        )
        .expect("parse saved session");
        assert_eq!(
            session["jack_routing"]["connections"][0]["source"].as_str(),
            Some("system:capture_1")
        );

        fs::remove_dir_all(&session_root).expect("cleanup temp session");
    }

    #[cfg(unix)]
    #[test]
    fn session_load_with_non_jack_keeps_but_ignores_jack_routing() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let session_root = std::env::temp_dir().join(format!("maolan_jack_ignore_{unique}"));
        let app = Maolan::default();
        app.save(session_root.to_string_lossy().to_string())
            .expect("save session");
        let session_path = session_root.join("main.json");
        let mut session: serde_json::Value =
            serde_json::from_reader(File::open(&session_path).expect("open session"))
                .expect("parse session");
        session["jack_routing"] = serde_json::to_value(test_jack_graph()).unwrap();
        serde_json::to_writer_pretty(
            File::create(&session_path).expect("rewrite session"),
            &session,
        )
        .expect("write session");

        let mut restored = Maolan::default();
        {
            let mut state = restored.state.blocking_write();
            select_non_jack_backend(&mut state);
        }
        let _ = restored
            .load(session_root.to_string_lossy().to_string())
            .expect("load session");
        let state = restored.state.blocking_read();
        assert_eq!(
            state
                .jack_session_routing
                .as_ref()
                .and_then(|routing| routing.connections.first())
                .map(|connection| connection.source.as_str()),
            Some("system:capture_1")
        );
        assert!(state.jack_graph.connections.is_empty());

        fs::remove_dir_all(&session_root).expect("cleanup temp session");
    }

    #[test]
    fn session_save_and_load_roundtrip_restores_open_automation_lanes_without_growing_height() {
        use crate::message::{TrackAutomationMode, TrackAutomationTarget};
        use crate::state::{Track, TrackAutomationLane};

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let session_root =
            std::env::temp_dir().join(format!("maolan_track_automation_session_{unique}"));

        let saved_height = 200.0_f32;
        let app = Maolan::default();
        {
            let mut state = app.state.blocking_write();
            state
                .tracks
                .push(Track::new("Drums".to_string(), 0.0, 2, 2, 0, 0));
            if let Some(track) = state.tracks.iter_mut().find(|t| t.name == "Drums") {
                track.height = saved_height;
                track.automation_mode = TrackAutomationMode::Touch;
                track.automation_lanes.push(TrackAutomationLane {
                    target: TrackAutomationTarget::Volume,
                    visible: true,
                    points: vec![],
                });
            }
        }
        app.save(session_root.to_string_lossy().to_string())
            .expect("save session");

        let mut restored = Maolan::default();
        let _ = restored
            .load(session_root.to_string_lossy().to_string())
            .expect("load session");
        {
            let state = restored.state.blocking_read();
            let (lanes, mode) = state
                .pending_track_automation
                .get("Drums")
                .expect("pending automation for Drums");
            assert_eq!(lanes.len(), 1);
            assert!(lanes[0].visible);
            assert_eq!(*mode, TrackAutomationMode::Touch);
        }

        let _ = restored.update(Message::Response(Ok(
            maolan_engine::message::Action::AddTrack {
                name: "Drums".to_string(),
                audio_ins: 2,
                audio_outs: 2,
                midi_ins: 0,
                midi_outs: 0,
                folder: false,
            },
        )));
        {
            let state = restored.state.blocking_read();
            let track = state
                .tracks
                .iter()
                .find(|t| t.name == "Drums")
                .expect("Drums track after response");
            assert_eq!(track.automation_lane_count(), 1);
            assert!(track.automation_lanes[0].visible);
            assert_eq!(track.automation_mode, TrackAutomationMode::Touch);
            // Height must equal the saved combined height, not grow by a lane step.
            assert!((track.height - saved_height).abs() < f32::EPSILON);
            assert!(!state.pending_track_automation.contains_key("Drums"));
        }

        fs::remove_dir_all(&session_root).expect("cleanup temp session");
    }

    #[test]
    fn session_save_and_load_roundtrip_preserves_plugin_positions() {
        use maolan_engine::message::{PluginGraphNode, PluginGraphPlugin};

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let session_root = std::env::temp_dir().join(format!("maolan_plugin_pos_session_{unique}"));

        let app = Maolan::default();
        {
            let mut state = app.state.blocking_write();
            state
                .clap_plugins
                .push(maolan_engine::clap::ClapPluginInfo {
                    id: "test.plugin".to_string(),
                    name: "Test Plugin".to_string(),
                    path: "/fake/path/test.clap::test.plugin".to_string(),
                    capabilities: None,
                });
        }
        let track_name = "Synth".to_string();
        let plugin = PluginGraphPlugin {
            node: PluginGraphNode::ClapPluginInstance(0),
            instance_id: 0,
            format: "CLAP".to_string(),
            uri: "test.clap".to_string(),
            plugin_id: "test.plugin".to_string(),
            name: "Test Plugin".to_string(),
            main_audio_inputs: 2,
            main_audio_outputs: 2,
            audio_inputs: 2,
            audio_outputs: 2,
            midi_inputs: 0,
            midi_outputs: 0,
            state: None,
            bypassed: false,
        };
        {
            let mut state = app.state.blocking_write();
            state
                .plugin_graphs_by_track
                .insert(track_name.clone(), (vec![plugin.clone()], vec![]));
            state
                .plugin_graph_plugin_positions
                .entry(track_name.clone())
                .or_default()
                .insert(plugin.instance_id, iced::Point::new(242.0, 410.0));
        }
        app.save(session_root.to_string_lossy().to_string())
            .expect("save session");

        let session_path = session_root.join("main.json");
        let session: serde_json::Value =
            serde_json::from_reader(File::open(&session_path).expect("open saved session"))
                .expect("parse saved session");
        let pos = session["graphs"][&track_name]["plugin_positions"]["0"]
            .as_object()
            .expect("plugin position");
        assert!((pos["x"].as_f64().unwrap_or(0.0) - 242.0).abs() < f64::EPSILON);
        assert!((pos["y"].as_f64().unwrap_or(0.0) - 410.0).abs() < f64::EPSILON);

        let mut restored = Maolan::default();
        {
            let mut state = restored.state.blocking_write();
            state
                .clap_plugins
                .push(maolan_engine::clap::ClapPluginInfo {
                    id: "test.plugin".to_string(),
                    name: "Test Plugin".to_string(),
                    path: "/fake/path/test.clap::test.plugin".to_string(),
                    capabilities: None,
                });
        }
        let _ = restored
            .load(session_root.to_string_lossy().to_string())
            .expect("load session");
        {
            let state = restored.state.blocking_read();
            let positions = state
                .plugin_graph_plugin_positions
                .get(&track_name)
                .expect("track plugin positions");
            let loaded = positions
                .get(&plugin.instance_id)
                .copied()
                .expect("plugin position");
            assert!((loaded.x - 242.0).abs() < f32::EPSILON);
            assert!((loaded.y - 410.0).abs() < f32::EPSILON);
        }

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

        let session_path = session_root.join("main.json");
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

        let session_path = session_root.join("main.json");
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
        let session_path = session_root.join("main.json");
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
    fn delete_unused_session_media_files_keeps_clip_referenced_by_other_branch() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let session_root = std::env::temp_dir().join(format!("maolan_test_cleanup_{unique}"));
        fs::create_dir_all(session_root.join("audio")).expect("create temp audio dir");
        fs::write(session_root.join("audio/keep.wav"), b"x").expect("seed media");

        let mut data = crate::state::StateData::default();
        data.unused_audio_clips.push(crate::state::AudioClip {
            id: "clip-1".to_string(),
            name: "audio/keep.wav".to_string(),
            ..Default::default()
        });
        let state: crate::state::State = std::sync::Arc::new(tokio::sync::RwLock::new(data));

        let mut other_track = crate::state::Track::new("Other".to_string(), 0.0, 1, 1, 1, 1);
        other_track.audio.clips.push(crate::state::AudioClip {
            name: "audio/keep.wav".to_string(),
            ..Default::default()
        });
        let other_branch = serde_json::json!({
            "tracks": serde_json::to_value(vec![&other_track]).expect("serialize track"),
        });
        fs::write(
            session_root.join("other.json"),
            serde_json::to_string(&other_branch).expect("serialize branch"),
        )
        .expect("write other branch");

        let report = Maolan::delete_unused_session_media_files_for(&state, "main", &session_root)
            .expect("cleanup");

        assert!(report.deleted_clips.is_empty());
        assert!(report.deleted_files.is_empty());
        assert!(session_root.join("audio/keep.wav").exists());
        assert_eq!(state.blocking_read().unused_audio_clips.len(), 1);

        fs::remove_dir_all(&session_root).expect("cleanup temp session");
    }

    #[test]
    fn delete_unused_session_media_files_removes_unreferenced_clip_and_media() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let session_root = std::env::temp_dir().join(format!("maolan_test_cleanup_{unique}"));
        fs::create_dir_all(session_root.join("audio")).expect("create temp audio dir");
        fs::write(session_root.join("audio/gone.wav"), b"x").expect("seed media");

        let mut data = crate::state::StateData::default();
        data.unused_audio_clips.push(crate::state::AudioClip {
            id: "clip-2".to_string(),
            name: "audio/gone.wav".to_string(),
            ..Default::default()
        });
        let state: crate::state::State = std::sync::Arc::new(tokio::sync::RwLock::new(data));

        let report = Maolan::delete_unused_session_media_files_for(&state, "main", &session_root)
            .expect("cleanup");

        assert_eq!(report.deleted_clips, vec!["clip-2".to_string()]);
        assert!(report.deleted_files.contains(&"audio/gone.wav".to_string()));
        assert!(!session_root.join("audio/gone.wav").exists());
        assert!(state.blocking_read().unused_audio_clips.is_empty());

        fs::remove_dir_all(&session_root).expect("cleanup temp session");
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
            bypassed: false,
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
    fn transport_play_in_session_view_starts_even_when_scene_is_empty() {
        let mut app = Maolan::default();
        app.state.blocking_write().view = crate::state::View::Session;

        let _ = app.update(Message::TransportPlay);

        assert!(app.live_session_playing);
    }

    #[test]
    fn session_scene_pressed_selects_scene_while_live_playing() {
        let mut app = Maolan {
            live_session_playing: true,
            ..Maolan::default()
        };

        let _ = app.update(Message::SessionScenePressed(0));
        assert_eq!(app.state.blocking_read().selected_scene, Some(0));

        // Pressing the selected scene again keeps the selection (the engine
        // re-triggers it at the end of the current pass).
        let _ = app.update(Message::SessionScenePressed(0));
        assert_eq!(app.state.blocking_read().selected_scene, Some(0));
    }

    #[test]
    fn session_scene_pressed_while_stopped_selects_scene() {
        let mut app = Maolan::default();
        // The default session has one scene; add a second so index 1 exists.
        app.state.blocking_write().session.add_scene();

        let _ = app.update(Message::SessionScenePressed(1));
        assert_eq!(app.state.blocking_read().selected_scene, Some(1));

        let _ = app.update(Message::SessionScenePressed(0));
        assert_eq!(app.state.blocking_read().selected_scene, Some(0));
    }

    #[test]
    fn transport_play_in_session_view_starts_selected_scene() {
        let mut app = Maolan::default();
        {
            let mut state = app.state.blocking_write();
            state.view = crate::state::View::Session;
            state.session.add_scene();
            state.tracks.push(crate::state::Track::new(
                "Track 1".to_string(),
                0.0,
                2,
                2,
                0,
                0,
            ));
            state.session.ensure_track_slots("Track 1");
            let slot = state
                .session
                .slot_mut("Track 1", 1)
                .expect("scene 1 slot exists");
            slot.clip = Some(crate::state::SlotClipRef {
                clip_id: "clip-1".to_string(),
                launch_mode: crate::state::LaunchMode::default(),
                launch_quantization: crate::state::LaunchQuantization::default(),
                loop_enabled: true,
                loop_start_samples: 0,
                loop_end_samples: 0,
            });
            slot.play_stop_icon = Some(true);
            state.selected_scene = Some(1);
        }

        let _ = app.update(Message::TransportPlay);

        assert!(app.live_session_playing);
        let state = app.state.blocking_read();
        let runtime = state
            .slot_runtimes
            .get(&("Track 1".to_string(), 1))
            .expect("scene 1 slot queued");
        assert_eq!(runtime.state, crate::state::SlotPlayState::Queued);
        assert!(
            !state
                .slot_runtimes
                .contains_key(&("Track 1".to_string(), 0)),
            "scene 0 must not start when scene 1 is selected"
        );
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
        assert_eq!(state.oss_period_frames, 4);
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
    fn track_setup_toggle_ignores_folder_tracks() {
        let mut app = Maolan::default();
        app.state
            .blocking_write()
            .tracks
            .push(crate::state::Track::new(
                "Folder".to_string(),
                0.0,
                0,
                0,
                0,
                0,
            ));
        app.state.blocking_write().tracks[0].is_folder = true;

        let _ = app.update(Message::TrackSetupToggle("Folder".to_string()));

        assert!(!app.state.blocking_read().tracks[0].setup_open);
    }

    #[test]
    fn track_setup_toggle_expands_narrow_tracks_panel() {
        let mut app = Maolan::default();
        {
            let mut state = app.state.blocking_write();
            state.tracks_width = Length::Fixed(200.0);
            state.tracks.push(crate::state::Track::new(
                "Track".to_string(),
                0.0,
                0,
                0,
                0,
                0,
            ));
        }

        let _ = app.update(Message::TrackSetupToggle("Track".to_string()));

        let state = app.state.blocking_read();
        assert!(state.tracks[0].setup_open);
        assert_eq!(state.tracks_width, Length::Fixed(338.6557));
    }

    #[test]
    fn track_setup_toggle_leaves_wide_tracks_panel_alone() {
        let mut app = Maolan::default();
        {
            let mut state = app.state.blocking_write();
            state.tracks_width = Length::Fixed(420.0);
            state.tracks.push(crate::state::Track::new(
                "Track".to_string(),
                0.0,
                0,
                0,
                0,
                0,
            ));
        }

        let _ = app.update(Message::TrackSetupToggle("Track".to_string()));

        let state = app.state.blocking_read();
        assert!(state.tracks[0].setup_open);
        assert_eq!(state.tracks_width, Length::Fixed(420.0));
    }

    #[test]
    fn track_toggle_phase_ignores_folder_tracks() {
        let mut app = Maolan::default();
        app.state
            .blocking_write()
            .tracks
            .push(crate::state::Track::new(
                "Folder".to_string(),
                0.0,
                0,
                0,
                0,
                0,
            ));
        app.state.blocking_write().tracks[0].is_folder = true;

        let _ = app.update(Message::Response(Ok(Action::TrackTogglePhase(
            "Folder".to_string(),
        ))));

        assert!(!app.state.blocking_read().tracks[0].phase_inverted);
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
        app.state.blocking_write().marker_dialog = Some(crate::state::MarkerDialog {
            sample: 10,
            marker_index: None,
            name: "Marker".to_string(),
        });

        let _ = app.update(Message::EscapePressed);
        assert!(app.modal.is_none());
        assert!(app.state.blocking_read().marker_dialog.is_some());

        let _ = app.update(Message::EscapePressed);
        assert!(app.state.blocking_read().marker_dialog.is_none());
    }

    #[test]
    fn add_track_submit_rejects_duplicate_name() {
        let mut app = Maolan::default();
        app.state
            .blocking_write()
            .tracks
            .push(crate::state::Track::new(
                "Existing".to_string(),
                0.0,
                1,
                1,
                0,
                0,
            ));
        app.modal = Some(Show::AddTrack);
        app.add_track
            .update(&Message::AddTrack(crate::message::AddTrack::Name(
                "Existing".to_string(),
            )));

        let _ = app.update(Message::AddTrack(crate::message::AddTrack::Submit));

        assert_eq!(app.state.blocking_read().tracks.len(), 1);
        assert!(
            app.state
                .blocking_read()
                .message
                .contains("Track 'Existing' already exists"),
            "expected error message, got: {}",
            app.state.blocking_read().message
        );
        assert!(app.modal.is_some());
    }

    #[test]
    fn add_folder_submit_rejects_duplicate_name() {
        let mut app = Maolan::default();
        app.state
            .blocking_write()
            .tracks
            .push(crate::state::Track::new(
                "Existing".to_string(),
                0.0,
                1,
                1,
                0,
                0,
            ));
        app.modal = Some(Show::AddFolder);
        app.add_track
            .update(&Message::AddTrack(crate::message::AddTrack::Name(
                "Existing".to_string(),
            )));
        app.add_track
            .update(&Message::AddTrack(crate::message::AddTrack::IsFolder(true)));

        let _ = app.update(Message::AddTrack(crate::message::AddTrack::Submit));

        assert_eq!(app.state.blocking_read().tracks.len(), 1);
        assert!(
            app.state
                .blocking_read()
                .tracks
                .iter()
                .all(|t| !t.is_folder),
            "existing track should not have been converted to folder"
        );
        assert!(
            app.state
                .blocking_read()
                .message
                .contains("Track 'Existing' already exists"),
            "expected error message, got: {}",
            app.state.blocking_read().message
        );
        assert!(app.modal.is_some());
    }

    #[test]
    fn live_view_add_track_submit_ensures_session_slots() {
        let mut app = Maolan::default();

        let _ = app.update(Message::Show(Show::AddTrack));
        assert!(matches!(app.modal, Some(Show::AddTrack)));
        app.add_track
            .update(&Message::AddTrack(crate::message::AddTrack::Name(
                "Live Track".to_string(),
            )));

        let _ = app.update(Message::AddTrack(crate::message::AddTrack::Submit));

        assert!(app.modal.is_none());
        assert!(
            app.state
                .blocking_read()
                .session
                .slots
                .contains_key("Live Track"),
            "expected session slots to be created for the new track"
        );
    }

    #[test]
    fn live_view_add_track_rejects_duplicate_name() {
        let mut app = Maolan::default();
        app.state
            .blocking_write()
            .tracks
            .push(crate::state::Track::new(
                "Live Track".to_string(),
                0.0,
                1,
                1,
                0,
                0,
            ));

        let _ = app.update(Message::Show(Show::AddTrack));
        app.add_track
            .update(&Message::AddTrack(crate::message::AddTrack::Name(
                "Live Track".to_string(),
            )));

        let _ = app.update(Message::AddTrack(crate::message::AddTrack::Submit));

        assert_eq!(app.state.blocking_read().tracks.len(), 1);
        assert!(
            app.state
                .blocking_read()
                .message
                .contains("Track 'Live Track' already exists"),
            "expected error message, got: {}",
            app.state.blocking_read().message
        );
        assert!(app.modal.is_some());
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
    fn session_message_switches_view_to_session() {
        let mut app = Maolan::default();
        let _ = app.update(Message::Session);
        assert_eq!(app.state.blocking_read().view, crate::state::View::Session);
    }

    #[test]
    fn workspace_message_switches_view_back_from_session() {
        let mut app = Maolan::default();
        let _ = app.update(Message::Session);
        let _ = app.update(Message::Workspace);
        assert_eq!(
            app.state.blocking_read().view,
            crate::state::View::Workspace
        );
    }

    #[test]
    fn export_format_extension_returns_correct_extensions() {
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

        assert!(spp > 0.0);
    }

    #[test]
    fn samples_per_bar_calculation() {
        let app = Maolan::default();
        let spb = app.samples_per_bar();

        assert!(spb > 0.0);
    }

    #[test]
    fn zoom_slider_visible_bars_roundtrip() {
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

        assert!(interval > 0);
    }

    #[test]
    fn snap_sample_to_bar_returns_valid_sample() {
        let app = Maolan::default();
        let sample = app.snap_sample_to_bar(1000.0);
        assert!(sample < 10000);
    }

    #[test]
    fn beat_pixels_calculation() {
        let app = Maolan::default();
        let bp = app.beat_pixels();
        assert!(bp > 0.0);
    }

    #[test]
    fn scan_track_templates_ignores_directories_without_track_json() {
        let _guard = AUDIO_PEAK_TEST_GUARD.lock().expect("lock guard");
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temp_home = std::env::temp_dir().join(format!("maolan_scan_track_templates_{unique}"));
        let track_templates = temp_home.join(".config/maolan/track_templates");
        let valid = track_templates.join("Valid");
        let invalid = track_templates.join("InvalidSession");
        fs::create_dir_all(&valid).unwrap();
        fs::create_dir_all(&invalid).unwrap();
        File::create(valid.join("track.json")).unwrap();
        File::create(invalid.join("main.json")).unwrap();

        let old_home = std::env::var("HOME").ok();
        unsafe {
            std::env::set_var("HOME", &temp_home);
        }

        let templates = scan_track_templates();

        if let Some(home) = old_home {
            unsafe {
                std::env::set_var("HOME", home);
            }
        } else {
            unsafe {
                std::env::remove_var("HOME");
            }
        }

        assert_eq!(templates, vec!["Valid".to_string()]);
        let _ = fs::remove_dir_all(&temp_home);
    }

    #[test]
    fn scan_track_and_folder_templates_splits_by_is_folder() {
        let _guard = AUDIO_PEAK_TEST_GUARD.lock().expect("lock guard");
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temp_home =
            std::env::temp_dir().join(format!("maolan_scan_track_and_folder_templates_{unique}"));
        let track_templates = temp_home.join(".config/maolan/track_templates");
        let folder = track_templates.join("Drums");
        let track = track_templates.join("Synth");
        fs::create_dir_all(&folder).unwrap();
        fs::create_dir_all(&track).unwrap();
        fs::write(
            folder.join("track.json"),
            r#"{"track":{"name":"Drums","is_folder":true,"audio":{"ins":2,"outs":2},"midi":{"ins":0,"outs":0}}}"#,
        )
        .unwrap();
        fs::write(
            track.join("track.json"),
            r#"{"track":{"name":"Synth","is_folder":false,"audio":{"ins":2,"outs":2},"midi":{"ins":0,"outs":0}}}"#,
        )
        .unwrap();

        let old_home = std::env::var("HOME").ok();
        unsafe {
            std::env::set_var("HOME", &temp_home);
        }

        let (tracks, folders) = scan_track_and_folder_templates();

        if let Some(home) = old_home {
            unsafe {
                std::env::set_var("HOME", home);
            }
        } else {
            unsafe {
                std::env::remove_var("HOME");
            }
        }

        assert_eq!(tracks, vec!["Synth".to_string()]);
        assert_eq!(folders, vec!["Drums".to_string()]);
        let _ = fs::remove_dir_all(&temp_home);
    }

    #[test]
    fn is_track_template_folder_reads_is_folder_flag() {
        let _guard = AUDIO_PEAK_TEST_GUARD.lock().expect("lock guard");
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temp_home =
            std::env::temp_dir().join(format!("maolan_is_track_template_folder_{unique}"));
        let track_templates = temp_home.join(".config/maolan/track_templates");
        let folder = track_templates.join("Drums");
        let track = track_templates.join("Synth");
        fs::create_dir_all(&folder).unwrap();
        fs::create_dir_all(&track).unwrap();
        fs::write(
            folder.join("track.json"),
            r#"{"track":{"name":"Drums","is_folder":true}}"#,
        )
        .unwrap();
        fs::write(
            track.join("track.json"),
            r#"{"track":{"name":"Synth","is_folder":false}}"#,
        )
        .unwrap();

        let old_home = std::env::var("HOME").ok();
        unsafe {
            std::env::set_var("HOME", &temp_home);
        }

        assert!(is_track_template_folder("Drums"));
        assert!(!is_track_template_folder("Synth"));

        if let Some(home) = old_home {
            unsafe {
                std::env::set_var("HOME", home);
            }
        } else {
            unsafe {
                std::env::remove_var("HOME");
            }
        }

        let _ = fs::remove_dir_all(&temp_home);
    }

    #[test]
    fn scan_templates_ignores_directories_without_main_json() {
        let _guard = AUDIO_PEAK_TEST_GUARD.lock().expect("lock guard");
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temp_home = std::env::temp_dir().join(format!("maolan_scan_templates_{unique}"));
        let session_templates = temp_home.join(".config/maolan/session_templates");
        let valid = session_templates.join("Valid");
        let invalid = session_templates.join("Invalid");
        fs::create_dir_all(&valid).unwrap();
        fs::create_dir_all(&invalid).unwrap();
        File::create(valid.join("main.json")).unwrap();
        File::create(invalid.join("session.json")).unwrap();

        let old_home = std::env::var("HOME").ok();
        unsafe {
            std::env::set_var("HOME", &temp_home);
        }

        let templates = scan_templates();

        if let Some(home) = old_home {
            unsafe {
                std::env::set_var("HOME", home);
            }
        } else {
            unsafe {
                std::env::remove_var("HOME");
            }
        }

        assert_eq!(templates, vec!["Valid".to_string()]);
        let _ = fs::remove_dir_all(&temp_home);
    }

    #[test]
    fn collect_to_session_operation_collects_external_file() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let session_root = std::env::temp_dir().join(format!("maolan_collect_test_{unique}"));
        let data_dir = session_root.join("data");
        fs::create_dir_all(&data_dir).unwrap();
        let external_file = session_root.join("external.wav");
        fs::write(&external_file, b"dummy").unwrap();

        let mut op = CollectToSessionOperation {
            session_root: session_root.clone(),
            data_dir,
            pending_clap_refs: HashSet::new(),
            copied_files: HashMap::new(),
        };

        let result = op.collect_file(external_file.to_string_lossy().as_ref());
        assert!(result.is_some());
        let (src, rel) = result.unwrap();
        assert_eq!(src, external_file);
        assert!(rel.starts_with("data/"));
        assert!(session_root.join(&rel).exists());

        let _ = fs::remove_dir_all(&session_root);
    }

    #[test]
    fn collect_to_session_operation_skips_internal_files() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let session_root = std::env::temp_dir().join(format!("maolan_collect_skip_test_{unique}"));
        let data_dir = session_root.join("data");
        fs::create_dir_all(&data_dir).unwrap();
        let internal = data_dir.join("already.wav");
        fs::write(&internal, b"dummy").unwrap();

        let mut op = CollectToSessionOperation {
            session_root: session_root.clone(),
            data_dir: data_dir.clone(),
            pending_clap_refs: HashSet::new(),
            copied_files: HashMap::new(),
        };

        assert!(op.collect_file("data/already.wav").is_none());
        assert!(op.copied_files.is_empty());
        let _ = fs::remove_dir_all(&session_root);
    }

    #[test]
    fn collect_to_session_operation_deduplicates_same_file() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let session_root = std::env::temp_dir().join(format!("maolan_collect_dedup_test_{unique}"));
        let data_dir = session_root.join("data");
        fs::create_dir_all(&data_dir).unwrap();
        let external_file = session_root.join("external.wav");
        fs::write(&external_file, b"dummy").unwrap();

        let mut op = CollectToSessionOperation {
            session_root: session_root.clone(),
            data_dir,
            pending_clap_refs: HashSet::new(),
            copied_files: HashMap::new(),
        };

        let (_, rel1) = op
            .collect_file(external_file.to_string_lossy().as_ref())
            .unwrap();
        let (_, rel2) = op
            .collect_file(external_file.to_string_lossy().as_ref())
            .unwrap();
        assert_eq!(rel1, rel2);
        assert_eq!(op.copied_files.len(), 1);

        let _ = fs::remove_dir_all(&session_root);
    }

    #[test]
    fn rewrite_plugin_state_paths_updates_absolute_path_to_relative() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let session_root = std::env::temp_dir().join(format!("maolan_rewrite_test_{unique}"));
        let data_dir = session_root.join("data");
        fs::create_dir_all(&data_dir).unwrap();
        let external = session_root.join("external.wav");
        fs::write(&external, b"dummy").unwrap();

        let mut copied = HashMap::new();
        copied.insert(external.clone(), "data/external.wav".to_string());

        let state = json!(
            session_root
                .join("external.wav")
                .to_string_lossy()
                .to_string()
        );
        let rewritten = Maolan::rewrite_plugin_state_paths(&state, &session_root, &copied);
        assert_eq!(rewritten, json!("data/external.wav"));

        let _ = fs::remove_dir_all(&session_root);
    }

    #[test]
    fn connectable_ref_to_json_maps_plugin_ids_to_indices() {
        let mut id_to_index = std::collections::HashMap::new();
        id_to_index.insert(42usize, 2usize);

        assert_eq!(
            Maolan::connectable_ref_to_json(&ConnectableRef::TrackInput, &id_to_index),
            Some(json!({"type": "track_input"}))
        );
        assert_eq!(
            Maolan::connectable_ref_to_json(
                &ConnectableRef::ChildTrack("Child".to_string()),
                &id_to_index
            ),
            Some(json!({"type": "child_track", "name": "Child"}))
        );
        assert_eq!(
            Maolan::connectable_ref_to_json(&ConnectableRef::ClapPlugin(42), &id_to_index),
            Some(json!({"type": "clap_plugin", "plugin_index": 2}))
        );
        assert_eq!(
            Maolan::connectable_ref_to_json(&ConnectableRef::ClapPlugin(99), &id_to_index),
            None
        );
    }

    #[test]
    fn connectable_ref_from_json_resolves_plugin_indices_to_ids() {
        let mut index_to_id = std::collections::HashMap::new();
        index_to_id.insert(2usize, 42usize);

        assert_eq!(
            Maolan::connectable_ref_from_json(&json!({"type": "track_input"}), &index_to_id),
            Some(ConnectableRef::TrackInput)
        );
        assert_eq!(
            Maolan::connectable_ref_from_json(
                &json!({"type": "child_track", "name": "Child"}),
                &index_to_id
            ),
            Some(ConnectableRef::ChildTrack("Child".to_string()))
        );
        assert_eq!(
            Maolan::connectable_ref_from_json(
                &json!({"type": "clap_plugin", "plugin_index": 2}),
                &index_to_id
            ),
            Some(ConnectableRef::ClapPlugin(42))
        );
        assert_eq!(
            Maolan::connectable_ref_from_json(
                &json!({"type": "clap_plugin", "plugin_index": 99}),
                &index_to_id
            ),
            None
        );
    }

    #[test]
    fn connectable_connection_to_json_skips_unknown_plugin_ids() {
        let id_to_index = std::collections::HashMap::new();
        let child_plugin = ConnectableConnection {
            from: ConnectableRef::ChildTrack("Child".to_string()),
            from_port: 0,
            to: ConnectableRef::ClapPlugin(7),
            to_port: 1,
            kind: Kind::Audio,
        };

        assert_eq!(
            Maolan::connectable_connection_to_json(&child_plugin, &id_to_index),
            None
        );
    }

    #[test]
    fn is_user_connectable_connection_detects_child_plugin_edges() {
        let child_plugin = ConnectableConnection {
            from: ConnectableRef::ChildTrack("Child".to_string()),
            from_port: 0,
            to: ConnectableRef::ClapPlugin(0),
            to_port: 0,
            kind: Kind::Audio,
        };
        let track_io = ConnectableConnection {
            from: ConnectableRef::TrackInput,
            from_port: 0,
            to: ConnectableRef::TrackOutput,
            to_port: 0,
            kind: Kind::Audio,
        };
        let track_to_plugin = ConnectableConnection {
            from: ConnectableRef::TrackInput,
            from_port: 0,
            to: ConnectableRef::ClapPlugin(0),
            to_port: 0,
            kind: Kind::Audio,
        };
        let plugin_to_track = ConnectableConnection {
            from: ConnectableRef::ClapPlugin(0),
            from_port: 0,
            to: ConnectableRef::TrackOutput,
            to_port: 0,
            kind: Kind::Audio,
        };
        let plugin_to_plugin = ConnectableConnection {
            from: ConnectableRef::ClapPlugin(0),
            from_port: 0,
            to: ConnectableRef::Vst3Plugin(1),
            to_port: 0,
            kind: Kind::Audio,
        };

        assert!(Maolan::is_user_connectable_connection(&child_plugin));
        assert!(!Maolan::is_user_connectable_connection(&track_io));
        assert!(Maolan::is_user_connectable_connection(&track_to_plugin));
        assert!(Maolan::is_user_connectable_connection(&plugin_to_track));
        assert!(Maolan::is_user_connectable_connection(&plugin_to_plugin));
    }
}
