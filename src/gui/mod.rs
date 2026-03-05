mod platform;
mod session;
mod subscriptions;
mod update;
mod view;

#[cfg(all(unix, not(target_os = "macos")))]
use crate::plugins::lv2::GuiLv2UiHost;
use crate::{
    add_track, clip_rename, connections, hw, menu,
    message::{
        DraggedClip, ExportBitDepth, ExportNormalizeMode, Message, PluginFormat, Show, SnapMode,
    },
    plugins::{clap::GuiClapUiHost, vst3::GuiVst3UiHost},
    state::{PianoControllerPoint, PianoNote, PianoSysExPoint, State, StateData},
    template_save, toolbar, track_rename, track_template_save, workspace,
};
use ebur128::{EbuR128, Mode as LoudnessMode};
use iced::{
    Length, Size, Task,
    widget::{button, checkbox, column, container, pick_list, row, scrollable, text, text_input},
};
#[cfg(unix)]
use maolan_engine::kind::Kind;
use maolan_engine::{
    self as engine,
    message::{Action, Message as EngineMessage},
};
use midly::{
    Format, Header, MetaMessage, Smf, Timing, TrackEvent, TrackEventKind,
    num::{u15, u24, u28},
};
use serde_json::Value;
#[cfg(unix)]
use serde_json::json;
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs::{self, File},
    io::{self, BufReader},
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
    time::Instant,
};
use symphonia::core::{
    audio::SampleBuffer, codecs::DecoderOptions, errors::Error as SymphoniaError,
    formats::FormatOptions, io::MediaSourceStream, meta::MetadataOptions, probe::Hint,
};
use tokio::sync::RwLock;
use wavers::Wav;

static CLIENT: LazyLock<engine::client::Client> = LazyLock::new(engine::client::Client::default);
const MIN_CLIP_WIDTH_PX: f32 = 12.0;
type TickToSampleFn = dyn Fn(u64) -> usize + Send + Sync;
type MidiTickMap = (Box<TickToSampleFn>, u64, u64);
type PianoParseResult = (Vec<PianoNote>, Vec<PianoControllerPoint>, Vec<PianoSysExPoint>, usize);

struct ExportSessionOptions {
    export_path: PathBuf,
    sample_rate: i32,
    bit_depth: ExportBitDepth,
    normalize: bool,
    normalize_target_dbfs: f32,
    normalize_mode: ExportNormalizeMode,
    normalize_target_lufs: f32,
    normalize_true_peak_dbtp: f32,
    normalize_tp_limiter: bool,
    state: State,
    session_root: PathBuf,
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
    Lv2 {
        instance_id: usize,
        index: u32,
    },
    Vst3 {
        instance_id: usize,
        param_id: u32,
    },
    Clap {
        instance_id: usize,
        param_id: u32,
    },
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
    lv2_params: HashMap<(usize, u32), f32>,
    vst3_params: HashMap<(usize, u32), f32>,
    clap_params: HashMap<(usize, u32), f64>,
}

pub struct Maolan {
    clip: Option<DraggedClip>,
    clip_preview_target_track: Option<String>,
    menu: menu::Menu,
    size: Size,
    state: State,
    toolbar: toolbar::Toolbar,
    track: Option<String>,
    workspace: workspace::Workspace,
    connections: connections::canvas_host::CanvasHost<connections::tracks::Graph>,
    #[cfg(any(target_os = "windows", all(unix, not(target_os = "macos"))))]
    track_plugins: connections::canvas_host::CanvasHost<connections::plugins::Graph>,
    hw: hw::HW,
    modal: Option<Show>,
    add_track: add_track::AddTrackView,
    clip_rename: clip_rename::ClipRenameView,
    track_rename: track_rename::TrackRenameView,
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
    pending_save_is_template: bool,
    pending_audio_peaks: HashMap<AudioClipKey, Vec<Vec<f32>>>,
    track_automation_runtime: HashMap<String, TrackAutomationRuntime>,
    touch_automation_overrides: HashMap<String, HashMap<AutomationWriteKey, TouchAutomationOverride>>,
    touch_active_keys: HashMap<String, HashSet<AutomationWriteKey>>,
    latch_automation_overrides: HashMap<String, HashMap<AutomationWriteKey, f32>>,
    pending_add_lv2_automation_uris: HashSet<(String, String)>,
    pending_add_lv2_automation_instances: HashSet<(String, usize)>,
    pending_add_vst3_automation_paths: HashSet<(String, String)>,
    pending_add_vst3_automation_instances: HashSet<(String, usize)>,
    pending_add_clap_automation_paths: HashSet<(String, String)>,
    pending_add_clap_automation_instances: HashSet<(String, usize)>,
    playing: bool,
    paused: bool,
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
    tracks_resize_hovered: bool,
    mixer_resize_hovered: bool,
    record_armed: bool,
    pending_record_after_save: bool,
    recording_preview_start_sample: Option<usize>,
    recording_preview_sample: Option<usize>,
    recording_preview_peaks: HashMap<String, Vec<Vec<f32>>>,
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
    export_bit_depth: ExportBitDepth,
    export_normalize: bool,
    export_normalize_mode: ExportNormalizeMode,
    export_normalize_dbfs_input: String,
    export_normalize_lufs_input: String,
    export_normalize_dbtp_input: String,
    export_normalize_tp_limiter: bool,
    clap_ui_host: GuiClapUiHost,
    #[cfg(all(unix, not(target_os = "macos")))]
    lv2_ui_host: GuiLv2UiHost,
    vst3_ui_host: GuiVst3UiHost,
    pending_vst3_ui_open: Option<PendingVst3UiOpen>,
    scan_clap_capabilities: bool,
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
        let state_data = StateData {
            available_templates: scan_templates(),
            ..StateData::default()
        };
        let state = Arc::new(RwLock::new(state_data));
        let mut menu = menu::Menu::default();
        menu.update_templates(scan_templates());
        Self {
            clip: None,
            clip_preview_target_track: None,
            menu,
            size: Size::new(0.0, 0.0),
            state: state.clone(),
            toolbar: toolbar::Toolbar::new(),
            track: None,
            workspace: workspace::Workspace::new(state.clone()),
            connections: connections::canvas_host::CanvasHost::new(
                connections::tracks::Graph::new(state.clone()),
            ),
            #[cfg(any(target_os = "windows", all(unix, not(target_os = "macos"))))]
            track_plugins: connections::canvas_host::CanvasHost::new(
                connections::plugins::Graph::new(state.clone()),
            ),
            hw: hw::HW::new(state.clone()),
            modal: None,
            add_track: add_track::AddTrackView::default(),
            clip_rename: clip_rename::ClipRenameView::new(state.clone()),
            track_rename: track_rename::TrackRenameView::new(state.clone()),
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
            #[cfg(all(unix, not(target_os = "macos")))]
            plugin_format: PluginFormat::Lv2,
            #[cfg(any(target_os = "windows", target_os = "macos"))]
            plugin_format: PluginFormat::Vst3,
            session_dir: None,
            pending_save_path: None,
            pending_save_tracks: std::collections::HashSet::new(),
            pending_save_is_template: false,
            pending_audio_peaks: HashMap::new(),
            track_automation_runtime: HashMap::new(),
            touch_automation_overrides: HashMap::new(),
            touch_active_keys: HashMap::new(),
            latch_automation_overrides: HashMap::new(),
            pending_add_lv2_automation_uris: HashSet::new(),
            pending_add_lv2_automation_instances: HashSet::new(),
            pending_add_vst3_automation_paths: HashSet::new(),
            pending_add_vst3_automation_instances: HashSet::new(),
            pending_add_clap_automation_paths: HashSet::new(),
            pending_add_clap_automation_instances: HashSet::new(),
            playing: false,
            paused: false,
            transport_samples: 0.0,
            last_playback_tick: None,
            playback_rate_hz: 48_000.0,
            loop_enabled: false,
            loop_range_samples: None,
            punch_enabled: false,
            punch_range_samples: None,
            snap_mode: SnapMode::Bar,
            zoom_visible_bars: 127.0,
            editor_scroll_x: 0.0,
            tracks_resize_hovered: false,
            mixer_resize_hovered: false,
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
            export_sample_rate_hz: 48_000,
            export_bit_depth: ExportBitDepth::Int24,
            export_normalize: false,
            export_normalize_mode: ExportNormalizeMode::Peak,
            export_normalize_dbfs_input: "0.0".to_string(),
            export_normalize_lufs_input: "-23.0".to_string(),
            export_normalize_dbtp_input: "-1.0".to_string(),
            export_normalize_tp_limiter: true,
            clap_ui_host: GuiClapUiHost::new(),
            #[cfg(all(unix, not(target_os = "macos")))]
            lv2_ui_host: GuiLv2UiHost::new(),
            vst3_ui_host: GuiVst3UiHost::new(),
            pending_vst3_ui_open: None,
            scan_clap_capabilities: false,
        }
    }
}

impl Maolan {
    #[cfg(all(unix, not(target_os = "macos")))]
    fn supported_plugin_formats() -> Vec<PluginFormat> {
        vec![PluginFormat::Lv2, PluginFormat::Clap, PluginFormat::Vst3]
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    fn supported_plugin_formats() -> Vec<PluginFormat> {
        vec![PluginFormat::Clap, PluginFormat::Vst3]
    }

    pub fn title(&self) -> String {
        let session = self
            .session_dir
            .as_ref()
            .and_then(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().to_string())
            })
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "<New>".to_string());
        format!("Maolan: {session}")
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
        match self.snap_mode {
            SnapMode::NoSnap => sample.max(0.0) as usize,
            SnapMode::Bar => {
                let interval = self.samples_per_bar().max(1.0);
                ((sample.max(0.0) as f64 / interval).round() * interval) as usize
            }
            SnapMode::Beat => {
                let interval = self.samples_per_beat().max(1.0);
                ((sample.max(0.0) as f64 / interval).round() * interval) as usize
            }
            SnapMode::Eighth => {
                let interval = (self.samples_per_beat() / 2.0).max(1.0);
                ((sample.max(0.0) as f64 / interval).round() * interval) as usize
            }
            SnapMode::Sixteenth => {
                let interval = (self.samples_per_beat() / 4.0).max(1.0);
                ((sample.max(0.0) as f64 / interval).round() * interval) as usize
            }
            SnapMode::ThirtySecond => {
                let interval = (self.samples_per_beat() / 8.0).max(1.0);
                ((sample.max(0.0) as f64 / interval).round() * interval) as usize
            }
            SnapMode::SixtyFourth => {
                let interval = (self.samples_per_beat() / 16.0).max(1.0);
                ((sample.max(0.0) as f64 / interval).round() * interval) as usize
            }
        }
    }

    fn snap_interval_samples(&self) -> usize {
        match self.snap_mode {
            SnapMode::NoSnap => 1,
            SnapMode::Bar => self.samples_per_bar().max(1.0) as usize,
            SnapMode::Beat => self.samples_per_beat().max(1.0) as usize,
            SnapMode::Eighth => (self.samples_per_beat() / 2.0).max(1.0) as usize,
            SnapMode::Sixteenth => (self.samples_per_beat() / 4.0).max(1.0) as usize,
            SnapMode::ThirtySecond => (self.samples_per_beat() / 8.0).max(1.0) as usize,
            SnapMode::SixtyFourth => (self.samples_per_beat() / 16.0).max(1.0) as usize,
        }
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

    fn read_clip_peaks_file(path: &Path) -> std::io::Result<Vec<Vec<f32>>> {
        let file = File::open(path)?;
        let json: Value = serde_json::from_reader(BufReader::new(file))?;
        let peaks_val = &json["peaks"];

        if peaks_val
            .as_array()
            .is_some_and(|arr| arr.first().is_some_and(|first| first.is_number()))
        {
            let mono = peaks_val
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(Value::as_f64)
                        .map(|v| v as f32)
                        .collect::<Vec<f32>>()
                })
                .unwrap_or_default();
            return Ok(if mono.is_empty() { vec![] } else { vec![mono] });
        }

        let per_channel = peaks_val
            .as_array()
            .map(|channels| {
                channels
                    .iter()
                    .map(|channel| {
                        channel
                            .as_array()
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(Value::as_f64)
                                    .map(|v| v as f32)
                                    .collect::<Vec<f32>>()
                            })
                            .unwrap_or_default()
                    })
                    .collect::<Vec<Vec<f32>>>()
            })
            .unwrap_or_default();
        Ok(per_channel)
    }

    fn compute_audio_clip_peaks(path: &Path, bins: usize) -> std::io::Result<Vec<Vec<f32>>> {
        let mut wav = Wav::<f32>::from_path(path).map_err(|e| {
            io::Error::other(format!("Failed to open WAV '{}': {e}", path.display()))
        })?;
        let channels = wav.n_channels().max(1) as usize;
        let samples: wavers::Samples<f32> = wav
            .read()
            .map_err(|e| io::Error::other(format!("WAV read error '{}': {e}", path.display())))?;
        let mut per_channel_abs = vec![Vec::with_capacity(samples.len() / channels + 1); channels];
        for frame in samples.chunks(channels) {
            for (channel_idx, sample) in frame.iter().enumerate() {
                per_channel_abs[channel_idx].push(sample.abs());
            }
        }

        if per_channel_abs.iter().all(|ch| ch.is_empty()) {
            return Ok(vec![]);
        }
        let target_bins = bins.max(16);
        let mut peaks = vec![vec![0.0_f32; target_bins]; channels];
        for channel_idx in 0..channels {
            let samples = &per_channel_abs[channel_idx];
            if samples.is_empty() {
                continue;
            }
            for (i, peak) in samples.iter().enumerate() {
                let bin = (i * target_bins) / samples.len();
                let idx = bin.min(target_bins - 1);
                peaks[channel_idx][idx] = peaks[channel_idx][idx].max(*peak);
            }
        }
        Ok(peaks)
    }

    fn audio_clip_source_length(path: &Path) -> std::io::Result<usize> {
        let mut wav = Wav::<f32>::from_path(path).map_err(|e| {
            io::Error::other(format!("Failed to open WAV '{}': {e}", path.display()))
        })?;
        let channels = wav.n_channels().max(1) as usize;
        let samples: wavers::Samples<f32> = wav
            .read()
            .map_err(|e| io::Error::other(format!("WAV read error '{}': {e}", path.display())))?;
        Ok(samples.len() / channels.max(1))
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

    async fn export_session<F>(
        options: &ExportSessionOptions,
        mut progress_callback: F,
    ) -> std::io::Result<()>
    where
        F: FnMut(f32, Option<String>),
    {
        let export_path = options.export_path.as_path();
        let sample_rate = options.sample_rate;
        let bit_depth = options.bit_depth;
        let normalize = options.normalize;
        let normalize_target_dbfs = options.normalize_target_dbfs;
        let normalize_mode = options.normalize_mode;
        let normalize_target_lufs = options.normalize_target_lufs;
        let normalize_true_peak_dbtp = options.normalize_true_peak_dbtp;
        let normalize_tp_limiter = options.normalize_tp_limiter;
        let state = options.state.clone();
        let session_root = options.session_root.as_path();

        progress_callback(0.0, Some("Analyzing tracks".to_string()));
        tokio::task::yield_now().await;

        let (tracks, total_length) = {
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
                    (
                        track.name.clone(),
                        track.level,
                        track.balance,
                        track.muted,
                        track.soloed,
                        track.audio.clips.clone(),
                    )
                })
                .collect();
            (tracks_data, max_length)
        };

        if total_length == 0 {
            return Err(io::Error::other(
                "No audio clips found. Nothing to export.".to_string(),
            ));
        }

        let has_solo = tracks.iter().any(|(_, _, _, _, soloed, _)| *soloed);
        let output_channels = 2;
        let mut mixed_buffer = vec![0.0_f32; total_length * output_channels];

        progress_callback(0.1, Some("Loading and mixing tracks".to_string()));
        tokio::task::yield_now().await;

        for (track_idx, (track_name, level, balance, muted, soloed, clips)) in
            tracks.iter().enumerate()
        {
            if *muted || (has_solo && !soloed) {
                continue;
            }

            let track_progress_start = 0.1 + (track_idx as f32 / tracks.len() as f32) * 0.7;
            let track_progress_span = 0.7 / tracks.len() as f32;

            progress_callback(
                track_progress_start,
                Some(format!("Processing track: {}", track_name)),
            );
            tokio::task::yield_now().await;

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

                let level_amp = 10.0_f32.powf(*level / 20.0);
                let left_gain = if *balance <= 0.0 {
                    level_amp
                } else {
                    level_amp * (1.0 - *balance)
                };
                let right_gain = if *balance >= 0.0 {
                    level_amp
                } else {
                    level_amp * (1.0 + *balance)
                };

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

                    let left_sample = samples[src_idx];

                    let right_sample = if clip_channels == 1 {
                        samples[src_idx]
                    } else if clip_channels >= 2 {
                        samples[src_idx + 1]
                    } else {
                        left_sample
                    };

                    let dst_idx = dst_frame * output_channels;
                    mixed_buffer[dst_idx] += left_sample * left_gain;
                    mixed_buffer[dst_idx + 1] += right_sample * right_gain;
                }
            }

            progress_callback(
                track_progress_start + track_progress_span,
                Some(format!("Finished: {}", track_name)),
            );
            tokio::task::yield_now().await;
        }

        if normalize {
            match normalize_mode {
                ExportNormalizeMode::Peak => {
                    progress_callback(
                        0.85,
                        Some(format!(
                            "Normalizing peak to {:.1} dBFS",
                            normalize_target_dbfs
                        )),
                    );
                    tokio::task::yield_now().await;
                    let peak = mixed_buffer
                        .iter()
                        .fold(0.0_f32, |acc, sample| acc.max(sample.abs()));
                    if peak > 0.0 {
                        let target_amp =
                            10.0_f32.powf(normalize_target_dbfs / 20.0).clamp(0.0, 1.0);
                        let gain = target_amp / peak;
                        for sample in &mut mixed_buffer {
                            *sample *= gain;
                        }
                    }
                }
                ExportNormalizeMode::Loudness => {
                    progress_callback(
                        0.85,
                        Some(format!(
                            "Normalizing loudness to {:.1} LUFS (TP {:.1} dBTP)",
                            normalize_target_lufs, normalize_true_peak_dbtp
                        )),
                    );
                    tokio::task::yield_now().await;
                    let (measured_lufs, measured_tp_amp) = Self::measure_lufs_and_true_peak(
                        &mixed_buffer,
                        output_channels,
                        sample_rate,
                    )?;
                    let gain_loudness_db = normalize_target_lufs - measured_lufs;
                    let gain_loudness = 10.0_f32.powf(gain_loudness_db / 20.0);

                    let ceiling_amp = 10.0_f32
                        .powf(normalize_true_peak_dbtp / 20.0)
                        .clamp(0.0, 1.0);
                    let gain_tp = if measured_tp_amp > 0.0 {
                        ceiling_amp / measured_tp_amp
                    } else {
                        gain_loudness
                    };

                    // Ardour-like behavior:
                    // - limiter enabled: hit loudness target, then constrain true-peak with limiter
                    // - limiter disabled: reduce gain so true-peak ceiling is not exceeded
                    let applied_gain = if normalize_tp_limiter {
                        gain_loudness
                    } else {
                        gain_loudness.min(gain_tp)
                    };

                    for sample in &mut mixed_buffer {
                        *sample *= applied_gain;
                    }

                    if normalize_tp_limiter {
                        let predicted_tp = measured_tp_amp * applied_gain;
                        if predicted_tp > ceiling_amp && ceiling_amp > 0.0 {
                            for sample in &mut mixed_buffer {
                                *sample = sample.clamp(-ceiling_amp, ceiling_amp);
                            }
                        }
                    }
                }
            }
        }

        progress_callback(0.9, Some(format!("Writing WAV ({bit_depth})")));
        tokio::task::yield_now().await;

        Self::write_wav_with_bit_depth(
            export_path,
            &mixed_buffer,
            sample_rate,
            output_channels,
            bit_depth,
        )?;

        progress_callback(1.0, Some("Complete".to_string()));
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

    fn parse_midi_clip_for_piano(path: &Path, sample_rate: f64) -> std::io::Result<PianoParseResult> {
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
            PluginGraphNode::Vst3PluginInstance(_) | PluginGraphNode::ClapPluginInstance(_) => None,
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn plugin_node_from_json(v: &Value) -> Option<maolan_engine::message::PluginGraphNode> {
        use maolan_engine::message::PluginGraphNode;
        let t = v["type"].as_str()?;
        match t {
            "track_input" => Some(PluginGraphNode::TrackInput),
            "track_output" => Some(PluginGraphNode::TrackOutput),
            "plugin" => Some(PluginGraphNode::Lv2PluginInstance(
                v["plugin_index"].as_u64()? as usize,
            )),
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
                    checkbox(self.scan_clap_capabilities)
                        .label("Scan capabilities (GUI, params, etc.)")
                        .on_toggle(Message::ToggleClapCapabilityScanning),
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

        let loaded_vst3_section: iced::Element<'_, Message> = {
            #[cfg(any(target_os = "windows", all(unix, not(target_os = "macos"))))]
            {
                let loaded_vst3 = state
                    .plugin_graphs_by_track
                    .get(&title)
                    .map(|(plugins, _)| {
                        plugins
                            .iter()
                            .filter(|plugin| plugin.format.eq_ignore_ascii_case("VST3"))
                            .map(|plugin| {
                                (
                                    plugin.name.clone(),
                                    plugin.uri.clone(),
                                    plugin.instance_id,
                                )
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let mut loaded_vst3_items = Vec::new();
                for (name, path, _) in loaded_vst3 {
                    loaded_vst3_items.push(
                        row![
                            text(name).width(Length::Fill),
                            button("Auto").on_press(Message::TrackAutomationAddVst3Lanes {
                                track_name: title.clone(),
                                plugin_path: path,
                            }),
                        ]
                        .spacing(8)
                        .into(),
                    );
                }
                column![
                    text("Loaded VST3"),
                    scrollable(column(loaded_vst3_items)).height(Length::Fixed(72.0)),
                ]
                .spacing(6)
                .into()
            }
            #[cfg(not(any(target_os = "windows", all(unix, not(target_os = "macos")))))]
            {
                container("").into()
            }
        };
        let loaded_lv2_section: iced::Element<'_, Message> = {
            #[cfg(all(unix, not(target_os = "macos")))]
            {
                let loaded_lv2 = state
                    .plugin_graphs_by_track
                    .get(&title)
                    .map(|(plugins, _)| {
                        plugins
                            .iter()
                            .filter(|plugin| plugin.format.eq_ignore_ascii_case("LV2"))
                            .map(|plugin| (plugin.name.clone(), plugin.uri.clone()))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let mut loaded_lv2_items = Vec::new();
                for (name, uri) in loaded_lv2 {
                    loaded_lv2_items.push(
                        row![
                            text(name).width(Length::Fill),
                            button("Auto").on_press(Message::TrackAutomationAddLv2Lanes {
                                track_name: title.clone(),
                                plugin_uri: uri,
                            }),
                        ]
                        .spacing(8)
                        .into(),
                    );
                }
                column![
                    text("Loaded LV2"),
                    scrollable(column(loaded_lv2_items)).height(Length::Fixed(72.0)),
                ]
                .spacing(6)
                .into()
            }
            #[cfg(not(all(unix, not(target_os = "macos"))))]
            {
                container("").into()
            }
        };

        let loaded_clap = state
            .clap_plugins_by_track
            .get(&title)
            .cloned()
            .unwrap_or_default();
        let mut loaded_clap_items = Vec::new();
        for path in loaded_clap {
            let name = std::path::Path::new(&path)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| path.clone());
            loaded_clap_items.push(
                row![
                    text(name).width(Length::Fill),
                    button("Auto").on_press(Message::TrackAutomationAddClapLanes {
                        track_name: title.clone(),
                        plugin_path: path.clone(),
                    }),
                    button("UI").on_press(Message::ShowClapPluginUi(path.clone())),
                    button("Unload").on_press(Message::UnloadClapPlugin(path)),
                ]
                .spacing(8)
                .into(),
            );
        }
        let loaded_clap_list = column(loaded_clap_items);

        container(
            column![
                text(format!("Track Plugins: {title}")),
                plugin_controls,
                loaded_lv2_section,
                loaded_vst3_section,
                text("Loaded CLAP"),
                scrollable(loaded_clap_list).height(Length::Fixed(100.0)),
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

    #[cfg(any(target_os = "windows", target_os = "macos"))]
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
                checkbox(self.scan_clap_capabilities)
                    .label("Scan capabilities (GUI, params, etc.)")
                    .on_toggle(Message::ToggleClapCapabilityScanning),
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

        let loaded_clap = state
            .clap_plugins_by_track
            .get(&title)
            .cloned()
            .unwrap_or_default();
        let mut loaded_clap_items = Vec::new();
        for path in loaded_clap {
            let name = std::path::Path::new(&path)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| path.clone());
            loaded_clap_items.push(
                row![
                    text(name).width(Length::Fill),
                    button("Auto").on_press(Message::TrackAutomationAddClapLanes {
                        track_name: title.clone(),
                        plugin_path: path.clone(),
                    }),
                    button("UI").on_press(Message::ShowClapPluginUi(path.clone())),
                    button("Unload").on_press(Message::UnloadClapPlugin(path)),
                ]
                .spacing(8)
                .into(),
            );
        }
        let loaded_clap_list = column(loaded_clap_items);
        container(
            column![
                text(format!("Track Plugins: {title}")),
                plugin_controls,
                loaded_vst3_section,
                text("Loaded CLAP"),
                scrollable(loaded_clap_list).height(Length::Fixed(100.0)),
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

    const STANDARD_EXPORT_SAMPLE_RATES: [u32; 12] = [
        8000, 11025, 16000, 22050, 32000, 44100, 48000, 88200, 96000, 176400, 192000, 384000,
    ];

    fn export_bit_depth_options() -> Vec<ExportBitDepth> {
        ExportBitDepth::ALL.to_vec()
    }

    fn ensure_wav_extension(path: PathBuf) -> PathBuf {
        let has_matching_ext = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("wav"));
        if has_matching_ext {
            path
        } else {
            path.with_extension("wav")
        }
    }

    fn export_settings_view(&self) -> iced::Element<'_, Message> {
        let bit_depth_options = Self::export_bit_depth_options();
        let selected_bit_depth = if bit_depth_options.contains(&self.export_bit_depth) {
            self.export_bit_depth
        } else {
            bit_depth_options
                .first()
                .copied()
                .unwrap_or(ExportBitDepth::Int24)
        };

        container(
            column![
                text("Export session").size(16),
                row![text("Format: WAV"),]
                    .spacing(10)
                    .align_y(iced::Alignment::Center),
                row![
                    text("Sample rate (Hz):"),
                    pick_list(
                        Self::STANDARD_EXPORT_SAMPLE_RATES.to_vec(),
                        Some(self.export_sample_rate_hz),
                        Message::ExportSampleRateSelected
                    )
                    .placeholder("Choose sample rate")
                    .width(Length::Fixed(220.0)),
                ]
                .spacing(10)
                .align_y(iced::Alignment::Center),
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
                                    ExportNormalizeMode::ALL.to_vec(),
                                    Some(self.export_normalize_mode),
                                    Message::ExportNormalizeModeSelected
                                )
                                .placeholder("Choose mode")
                                .width(Length::Fixed(180.0)),
                            ]
                            .spacing(10)
                            .align_y(iced::Alignment::Center),
                            if matches!(self.export_normalize_mode, ExportNormalizeMode::Peak) {
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
                            if matches!(self.export_normalize_mode, ExportNormalizeMode::Loudness) {
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
        .padding(20)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::Alignment::Center)
        .align_y(iced::Alignment::Center)
        .into()
    }

    fn update_children(&mut self, message: &Message) {
        self.menu.update(message.clone());
        self.toolbar.update(message.clone());
        self.workspace.update(message.clone());
        self.connections.update(message.clone());
        #[cfg(any(target_os = "windows", all(unix, not(target_os = "macos"))))]
        self.track_plugins.update(message.clone());
        self.add_track.update(message.clone());
        self.clip_rename.update(message.clone());
        self.track_rename.update(message.clone());
        for track in &mut self.state.blocking_write().tracks {
            track.update(message.clone());
        }
    }
}
