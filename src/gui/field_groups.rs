//! Owned sub-structs grouping `Maolan` fields by concern.
//!
//! This is a field-grouping refactor only: no logic moved. Each struct holds
//! fields that were previously flat members of `Maolan`. The `Default`
//! implementations preserve the exact baseline values `Maolan::default`
//! used before the grouping.

use super::*;
use crate::gui::update::AutomationTrackView;
use crate::message::TrackAutomationMode;
use maolan_widgets::iced::widget::{column, row};

pub(crate) type MeterSnapshotData = (Vec<f32>, Vec<(String, Vec<f32>)>);

/// Transport, playback, loop/punch, record-arm, and step-recording state.
#[derive(Debug)]
pub struct TransportUiState {
    pub playing: bool,
    pub paused: bool,
    pub live_session_playing: bool,
    pub meter_stop_decay: Option<MeterStopDecay>,
    pub recorded_live_session_clip_passes: HashSet<(String, usize, String, usize, usize)>,
    pub live_session_record_start_sample: Option<usize>,
    pub metronome_enabled: bool,
    pub transport_samples: f64,
    pub last_playback_tick: Option<Instant>,
    pub pending_transport_position: Option<(Instant, usize)>,
    pub playback_rate_hz: f64,
    pub loop_enabled: bool,
    pub loop_range_samples: Option<(usize, usize)>,
    pub punch_enabled: bool,
    pub punch_range_samples: Option<(usize, usize)>,
    pub step_recording_active: bool,
    pub step_recording_cursor_samples: usize,
    pub record_armed: bool,
    pub pending_record_after_save: bool,
    pub midi_clip_previews: MidiClipPreviewMap,
}

impl Default for TransportUiState {
    fn default() -> Self {
        Self {
            playing: false,
            paused: false,
            live_session_playing: false,
            meter_stop_decay: None,
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
            step_recording_active: false,
            step_recording_cursor_samples: 0,
            record_armed: false,
            pending_record_after_save: false,
            midi_clip_previews: HashMap::new(),
        }
    }
}

/// Active drag operations and clip snapping/hover state.
#[derive(Debug, Default)]
pub struct DragState {
    pub clip: Option<DraggedClip>,
    pub clip_preview_target_track: Option<String>,
    pub clip_preview_target_valid: bool,
    pub clip_preview_snap_adjust_samples: f32,
    pub clip_snap_targets: Vec<crate::state::ClipId>,
    pub dragging_session_slot: Option<(String, usize)>,
    pub dragging_session_clip: Option<crate::state::DraggedSessionClip>,
    pub dragging_pane_clip: Option<crate::state::DraggedSessionClip>,
    pub session_slot_record_target: Option<(String, usize)>,
}

/// Pending bookkeeping for save/import/export/freeze/peak and automation
/// plugin-add flows.
#[derive(Debug, Default)]
pub struct PendingOpsState {
    pub collect_to_session_operation: Option<CollectToSessionOperation>,
    pub pending_save_path: Option<String>,
    pub pending_save_tracks: std::collections::HashSet<String>,
    pub pending_save_clap_tracks: std::collections::HashSet<String>,
    pub pending_save_clap_clips: std::collections::HashSet<(String, usize, usize)>,
    pub pending_save_is_template: bool,
    pub pending_save_track_name: Option<String>,
    pub pending_peak_file_loads: HashMap<AudioClipKey, PathBuf>,
    pub pending_peak_rebuilds: HashSet<AudioClipKey>,
    pub pending_precomputed_peaks: HashMap<AudioClipKey, crate::state::ClipPeaks>,
    pub pending_source_lengths: HashMap<AudioClipKey, usize>,
    pub undo_peaks_cache: HashMap<AudioClipKey, crate::state::ClipPeaks>,
    pub undo_source_lengths_cache: HashMap<AudioClipKey, usize>,
    pub pending_track_freeze_restore: HashMap<String, TrackFreezeRestore>,
    pub pending_track_midi_editor_view_mode: HashMap<String, crate::message::MidiEditorViewMode>,
    pub pending_track_freeze_bounce: HashMap<String, PendingTrackFreezeBounce>,
    #[cfg(unix)]
    pub pending_add_lv2_automation_uris: HashSet<(String, String)>,
    #[cfg(unix)]
    pub pending_add_lv2_automation_instances: HashSet<(String, usize)>,
    pub pending_add_vst3_automation_paths: HashSet<(String, String)>,
    pub pending_add_vst3_automation_instances: HashSet<(String, usize)>,
    pub pending_add_clap_automation_paths: HashSet<(String, String)>,
    pub pending_add_clap_automation_instances: HashSet<(String, usize)>,
    pub pending_midi_clip_previews: HashSet<(String, usize, String)>,
    pub freeze_in_progress: bool,
    pub freeze_progress: f32,
    pub freeze_track_name: Option<String>,
    pub freeze_cancel_requested: bool,
    pub pending_native_ui_fallback: Option<PendingNativeUiFallback>,
}

/// Automation touch/latch runtime overrides.
#[derive(Debug, Default)]
pub struct AutomationRuntimeState {
    pub track_automation_runtime: HashMap<String, TrackAutomationRuntime>,
    pub touch_automation_overrides:
        HashMap<String, HashMap<AutomationWriteKey, TouchAutomationOverride>>,
    pub touch_active_keys: HashMap<String, HashSet<AutomationWriteKey>>,
    pub latch_automation_overrides: HashMap<String, HashMap<AutomationWriteKey, f32>>,
}

/// Plugin scanner selection/filter state.
#[derive(Debug, Default)]
pub struct PluginScanState {
    #[cfg(unix)]
    pub selected_lv2_plugins: BTreeSet<String>,
    pub selected_vst3_plugins: BTreeSet<String>,
    pub selected_clap_plugins: BTreeSet<String>,
    pub plugin_list_filter: String,
}

/// Import/export progress and format settings.
#[derive(Debug)]
pub struct TransferState {
    pub import_in_progress: bool,
    pub import_current_file: usize,
    pub import_total_files: usize,
    pub import_file_progress: f32,
    pub import_current_filename: String,
    pub import_current_operation: Option<String>,
    pub export_in_progress: bool,
    pub export_cancel: Arc<AtomicBool>,
    pub export_pending_bounces: HashSet<String>,
    pub export_bounce_notify: Option<Arc<tokio::sync::Notify>>,
    pub export_progress: f32,
    pub export_operation: Option<String>,
    pub export_sample_rate_hz: u32,
    pub export_format_wav: bool,
    pub export_format_flac: bool,
    pub export_format_mp3: bool,
    pub export_format_ogg: bool,
    pub export_bit_depth: ExportBitDepth,
    pub export_dither: ExportDither,
    pub export_render_mode: ExportRenderMode,
    pub export_hw_out_ports: BTreeSet<usize>,
    pub export_realtime_fallback: bool,
    pub export_normalize: bool,
    pub export_normalize_mode: ExportNormalizeMode,
    pub export_normalize_dbfs_input: String,
    pub export_normalize_lufs_input: String,
    pub export_normalize_dbtp_input: String,
    pub export_normalize_tp_limiter: bool,
    pub export_master_limiter: bool,
    pub export_master_limiter_ceiling_input: String,
}

impl Default for TransferState {
    fn default() -> Self {
        Self {
            import_in_progress: false,
            import_current_file: 0,
            import_total_files: 0,
            import_file_progress: 0.0,
            import_current_filename: String::new(),
            import_current_operation: None,
            export_in_progress: false,
            export_cancel: Arc::new(AtomicBool::new(false)),
            export_pending_bounces: HashSet::new(),
            export_bounce_notify: None,
            export_progress: 0.0,
            export_operation: None,
            export_sample_rate_hz: 48_000,
            export_format_wav: true,
            export_format_flac: false,
            export_format_mp3: false,
            export_format_ogg: false,
            export_bit_depth: ExportBitDepth::Int24,
            export_dither: ExportDither::Triangular,
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
        }
    }
}

/// AI audio/MIDI generation and pitch-correction progress state.
#[derive(Debug)]
pub struct GenerateState {
    pub generate_audio_model: GenerateAudioModelOption,
    pub generate_audio_acestep_lm: GenerateAudioAceStepLmOption,
    pub generate_audio_prompt_editor: text_editor::Content,
    pub generate_audio_tags_input: String,
    pub generate_audio_backend: BurnBackendOption,
    pub generate_audio_key_root: NoteName,
    pub generate_audio_key_mode: KeyMode,
    pub generate_audio_cfg_scale_input: String,
    pub generate_audio_steps_input: usize,
    pub generate_audio_seconds_total_input: usize,
    pub generate_audio_in_progress: bool,
    pub generate_audio_progress: f32,
    pub generate_audio_operation: Option<String>,
    pub generate_audio_abort_handle: Option<tokio::task::AbortHandle>,
    #[cfg(unix)]
    pub generate_audio_process_id: Option<u32>,
    pub generate_midi_model: GenerateMidiModelOption,
    pub generate_midi_prompt_editor: text_editor::Content,
    pub generate_midi_backend: BurnBackendOption,
    pub generate_midi_key_root: NoteName,
    pub generate_midi_key_mode: KeyMode,
    pub generate_midi_bpm_input: String,
    pub generate_midi_time_signature_num_input: String,
    pub generate_midi_time_signature_denom_input: String,
    pub generate_midi_length_seconds_input: String,
    pub generate_midi_max_tokens_input: String,
    pub generate_midi_top_p_input: String,
    pub generate_midi_seed_input: String,
    pub generate_midi_in_progress: bool,
    pub generate_midi_progress: f32,
    pub generate_midi_operation: Option<String>,
    pub generate_midi_abort_handle: Option<tokio::task::AbortHandle>,
    #[cfg(unix)]
    pub generate_midi_process_id: Option<u32>,
    pub clip_pitch_correction_in_progress: bool,
    pub clip_pitch_correction_progress: f32,
    pub clip_pitch_correction_clip_name: String,
    pub clip_pitch_correction_operation: Option<String>,
    pub resynth_render_in_progress: bool,
}

impl Default for GenerateState {
    fn default() -> Self {
        Self {
            generate_audio_model: GenerateAudioModelOption::HappyNewYear,
            generate_audio_acestep_lm: GenerateAudioAceStepLmOption::default(),
            generate_audio_prompt_editor: text_editor::Content::new(),
            generate_audio_tags_input: String::new(),
            generate_audio_backend: BurnBackendOption::Vulkan,
            generate_audio_key_root: NoteName::C,
            generate_audio_key_mode: KeyMode::Major,
            generate_audio_cfg_scale_input: maolan_generate::DEFAULT_CFG_SCALE.to_string(),
            generate_audio_steps_input: 10,
            generate_audio_seconds_total_input: 180_usize,
            generate_audio_in_progress: false,
            generate_audio_progress: 0.0,
            generate_audio_operation: None,
            generate_audio_abort_handle: None,
            #[cfg(unix)]
            generate_audio_process_id: None,
            generate_midi_model: GenerateMidiModelOption::TextToMidi,
            generate_midi_prompt_editor: text_editor::Content::new(),
            generate_midi_backend: BurnBackendOption::Vulkan,
            generate_midi_key_root: NoteName::C,
            generate_midi_key_mode: KeyMode::Major,
            generate_midi_bpm_input: "120".to_string(),
            generate_midi_time_signature_num_input: "4".to_string(),
            generate_midi_time_signature_denom_input: "4".to_string(),
            generate_midi_length_seconds_input: "10".to_string(),
            generate_midi_max_tokens_input: "1024".to_string(),
            generate_midi_top_p_input: "0.98".to_string(),
            generate_midi_seed_input: "0".to_string(),
            generate_midi_in_progress: false,
            generate_midi_progress: 0.0,
            generate_midi_operation: None,
            generate_midi_abort_handle: None,
            #[cfg(unix)]
            generate_midi_process_id: None,
            clip_pitch_correction_in_progress: false,
            clip_pitch_correction_progress: 0.0,
            clip_pitch_correction_clip_name: String::new(),
            clip_pitch_correction_operation: None,
            resynth_render_in_progress: false,
        }
    }
}

/// Pane visibility, scroll/zoom, and misc layout flags.
#[derive(Debug)]
pub struct UiState {
    pub zoom_visible_bars: f32,
    pub editor_scroll_origin_samples: f64,
    pub editor_scroll_x: f32,
    pub editor_scroll_y: f32,
    pub mixer_scroll_x: f32,
    pub tracks_resize_hovered: bool,
    pub tracks_filter: String,
    pub mixer_resize_hovered: bool,
    pub tracks_visible: bool,
    pub editor_visible: bool,
    pub mixer_visible: bool,
    pub toolbar_visible: bool,
    pub show_log_window: bool,
    pub shortcuts_pane_visible: bool,
    pub shortcut_capture_action: Option<ShortcutAction>,
    pub modulators_pane_visible: bool,
    pub clips_pane_visible: bool,
    pub midi_mappings_panel_open: bool,
    pub midi_mappings_report_lines: Vec<String>,
    pub log_viewer_content: text_editor::Content,
    pub log_viewer_highlights: LogHighlightSettings,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            zoom_visible_bars: 127.0,
            editor_scroll_origin_samples: 0.0,
            editor_scroll_x: 0.0,
            editor_scroll_y: 0.0,
            mixer_scroll_x: 0.0,
            tracks_resize_hovered: false,
            tracks_filter: String::new(),
            mixer_resize_hovered: false,
            tracks_visible: true,
            editor_visible: true,
            mixer_visible: true,
            toolbar_visible: true,
            show_log_window: false,
            shortcuts_pane_visible: false,
            shortcut_capture_action: None,
            modulators_pane_visible: false,
            clips_pane_visible: false,
            midi_mappings_panel_open: false,
            midi_mappings_report_lines: Vec::new(),
            log_viewer_content: text_editor::Content::with_text(
                "[INFO] Thank you for using Maolan!",
            ),
            log_viewer_highlights: LogHighlightSettings::default(),
        }
    }
}

/// In-progress recording preview overlay state.
#[derive(Debug, Default)]
pub struct RecordingPreviewState {
    pub recording_preview_start_sample: Option<usize>,
    pub recording_preview_sample: Option<usize>,
    pub recording_preview_peaks: HashMap<String, ClipPeaks>,
}

/// Tempo/time-signature inputs, selection, and snap modes.
#[derive(Debug)]
pub struct TimingState {
    pub tempo_input: String,
    pub time_signature_num_input: String,
    pub time_signature_denom_input: String,
    pub tap_tempo_times: Vec<Instant>,
    pub last_sent_tempo_bpm: Option<f64>,
    pub last_sent_time_signature: Option<(u16, u16)>,
    pub selected_tempo_points: BTreeSet<usize>,
    pub selected_time_signature_points: BTreeSet<usize>,
    pub timing_selection_lane: Option<TimingSelectionLane>,
    pub snap_mode: SnapMode,
    pub midi_snap_mode: SnapMode,
}

impl Default for TimingState {
    fn default() -> Self {
        Self {
            tempo_input: "120".to_string(),
            time_signature_num_input: "4".to_string(),
            time_signature_denom_input: "4".to_string(),
            tap_tempo_times: Vec::new(),
            last_sent_tempo_bpm: Some(120.0),
            last_sent_time_signature: Some((4, 4)),
            selected_tempo_points: BTreeSet::new(),
            selected_time_signature_points: BTreeSet::new(),
            timing_selection_lane: None,
            snap_mode: SnapMode::default(),
            midi_snap_mode: SnapMode::default(),
        }
    }
}

/// Session open/save/autosave bookkeeping flags.
#[derive(Debug, Default)]
pub struct SessionOpsState {
    pub has_unsaved_changes: bool,
    pub engine_dirty: bool,
    pub pending_exit_after_save: bool,
    pub session_restore_in_progress: bool,
    pub last_autosave_snapshot: Option<Instant>,
    pub pending_recovery_session_dir: Option<PathBuf>,
    pub pending_autosave_recovery: Option<PendingAutosaveRecovery>,
    pub pending_open_session_dir: Option<PathBuf>,
    pub pending_branch_input: String,
}

/// Cached plugin parameter values for CLAP and generic plugins.
#[derive(Debug, Default)]
pub struct PluginParamState {
    pub clap_param_values: HashMap<(String, Option<usize>, usize, u32), f64>,
    pub generic_plugin_param_values: HashMap<(String, Option<usize>, usize, u32), f64>,
}

impl TransportUiState {
    pub(crate) fn samples_per_beat(&self, state: &State) -> f64 {
        let state = state.read().expect("state lock poisoned");
        self.samples_per_beat_from(&state)
    }

    fn samples_per_beat_from(&self, state: &StateData) -> f64 {
        let tempo = state.tempo.max(1.0) as f64;
        let denom = state.time_signature_denom.max(1) as f64;
        let quarter = self.playback_rate_hz * 60.0 / tempo;
        quarter * (4.0 / denom)
    }

    pub(crate) fn samples_per_bar(&self, state: &State) -> f64 {
        let state = state.read().expect("state lock poisoned");
        self.samples_per_bar_from(&state)
    }

    fn samples_per_bar_from(&self, state: &StateData) -> f64 {
        let beats_per_bar = state.time_signature_num.max(1) as f64;
        self.samples_per_beat_from(state) * beats_per_bar
    }

    pub(crate) fn editor_timeline_samples(&self, state: &State, visible_samples: f64) -> f64 {
        let state = state.read().expect("state lock poisoned");
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
        let right_padding_samples = visible_samples * 0.5;
        let min_timeline_samples = (self.samples_per_bar_from(&state)
            * crate::consts::workspace::MIN_TIMELINE_BARS as f64)
            .max(1.0);
        max_end_samples
            .max(self.transport_samples.max(0.0) + right_padding_samples)
            .max(max_end_samples + right_padding_samples)
            .max(visible_samples)
            .max(min_timeline_samples)
    }

    pub(crate) fn start_meter_stop_decay(&mut self, state: &State) {
        if self.meter_stop_decay.is_some() {
            return;
        }
        let state = state.read().expect("state lock poisoned");
        let hw_out_db = state.hw_out_meter_db.clone();
        let track_meters = state
            .tracks
            .iter()
            .map(|track| (track.name.clone(), track.meter_out_db.clone()))
            .collect();
        drop(state);
        self.meter_stop_decay = Some(MeterStopDecay {
            started_at: Instant::now(),
            hw_out_db,
            track_meters,
        });
    }

    pub(crate) fn stop_meter_stop_decay(&mut self) {
        self.meter_stop_decay = None;
    }

    pub(crate) fn meter_stop_decay_snapshot(&mut self) -> Option<MeterSnapshotData> {
        let decay = self.meter_stop_decay.as_ref()?;
        let elapsed = decay.started_at.elapsed();
        let fraction = (elapsed.as_secs_f32() / Duration::from_secs(1).as_secs_f32()).min(1.0);
        let target = |db: f32| db + ((-90.0 - db) * fraction);
        let snapshot = (
            decay.hw_out_db.iter().copied().map(target).collect(),
            decay
                .track_meters
                .iter()
                .map(|(name, meters)| (name.clone(), meters.iter().copied().map(target).collect()))
                .collect(),
        );
        if fraction >= 1.0 {
            self.meter_stop_decay = None;
        }
        Some(snapshot)
    }

    pub(crate) fn record_automation_point(
        &mut self,
        state: &State,
        track_name: &str,
        target: TrackAutomationTarget,
        value: f32,
    ) {
        if !self.playing || self.paused {
            return;
        }
        let sample = self.transport_samples.max(0.0) as usize;
        let mut state = state.write().expect("state lock poisoned");
        let Some(track) = state.tracks.iter_mut().find(|t| t.name == track_name) else {
            return;
        };
        if track.automation_mode == crate::message::TrackAutomationMode::Read {
            return;
        }
        let previous_lane_height = track
            .lane_layout()
            .representative_height()
            .max(crate::consts::state_track::TRACK_SUBTRACK_MIN_HEIGHT);
        let previously_visible = track.automation_lane_count();
        if let Some(lane) = track
            .automation_lanes
            .iter_mut()
            .find(|lane| lane.target == target)
        {
            if let Some(existing) = lane.points.iter_mut().find(|p| p.sample == sample) {
                existing.value = value.clamp(0.0, 1.0);
            } else {
                lane.points.push(crate::state::TrackAutomationPoint {
                    sample,
                    value: value.clamp(0.0, 1.0),
                });
                lane.points.sort_unstable_by_key(|p| p.sample);
            }
            lane.visible = true;
        } else {
            track
                .automation_lanes
                .push(crate::state::TrackAutomationLane {
                    target,
                    visible: true,
                    points: vec![crate::state::TrackAutomationPoint {
                        sample,
                        value: value.clamp(0.0, 1.0),
                    }],
                });
        }
        let lanes_delta = track.automation_lane_count() as isize - previously_visible as isize;
        track.adjust_height_for_automation_lanes(previous_lane_height, lanes_delta);
    }
}

impl TimingState {
    pub(crate) fn snap_sample_to_bar(
        &self,
        sample: f32,
        samples_per_beat: f64,
        samples_per_bar: f64,
    ) -> usize {
        self.snap_mode
            .snap_sample(sample as f64, samples_per_beat, samples_per_bar) as usize
    }

    pub(crate) fn snap_sample_to_bar_drag(
        &self,
        sample: f32,
        delta_samples: f32,
        samples_per_beat: f64,
        samples_per_bar: f64,
    ) -> usize {
        self.snap_mode.snap_sample_drag(
            sample as f64,
            delta_samples as f64,
            samples_per_beat,
            samples_per_bar,
        ) as usize
    }

    pub(crate) fn snap_interval_samples(
        &self,
        samples_per_beat: f64,
        samples_per_bar: f64,
    ) -> usize {
        self.snap_mode
            .interval_samples(samples_per_beat, samples_per_bar) as usize
    }
    pub(crate) fn sync_timing_inputs_from_selection(&mut self, state: &State) {
        let state = state.read().expect("state lock poisoned");
        if let Some(sample) = self.selected_tempo_points.iter().next().copied()
            && let Some(point) = state.tempo_points.iter().find(|p| p.sample == sample)
        {
            self.tempo_input = format!("{:.2}", point.bpm);
        }
        if let Some(sample) = self.selected_time_signature_points.iter().next().copied()
            && let Some(point) = state
                .time_signature_points
                .iter()
                .find(|p| p.sample == sample)
        {
            self.time_signature_num_input = point.numerator.to_string();
            self.time_signature_denom_input = point.denominator.to_string();
        }
    }
    pub(crate) fn selected_samples(&self) -> Vec<usize> {
        let mut samples = self
            .selected_tempo_points
            .iter()
            .chain(self.selected_time_signature_points.iter())
            .copied()
            .collect::<Vec<_>>();
        samples.sort_unstable();
        samples.dedup();
        samples
    }
    pub(crate) fn set_selection(
        &mut self,
        samples: impl IntoIterator<Item = usize>,
        lane: Option<TimingSelectionLane>,
    ) {
        let mut samples = samples.into_iter().collect::<Vec<_>>();
        samples.sort_unstable();
        samples.dedup();
        self.selected_tempo_points.clear();
        self.selected_tempo_points.extend(samples.iter().copied());
        self.selected_time_signature_points.clear();
        self.selected_time_signature_points
            .extend(samples.iter().copied());
        self.timing_selection_lane = if samples.is_empty() { None } else { lane };
    }
    pub(crate) fn clip_edge_snap_enabled(&self) -> bool {
        matches!(self.snap_mode, crate::message::SnapMode::Clips)
    }
}

impl PendingOpsState {
    pub(crate) fn save_ready(&self) -> bool {
        self.pending_save_tracks.is_empty()
            && self.pending_save_clap_tracks.is_empty()
            && self.pending_save_clap_clips.is_empty()
            && self.vst3_save_ready()
    }

    fn vst3_save_ready(&self) -> bool {
        !crate::platform_caps::REQUIRE_VST3_STATE_FOR_SAVE
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn schedule_audio_peak_rebuild(
        &mut self,
        track_name: &str,
        clip_name: &str,
        start: usize,
        length: usize,
        offset: usize,
        wav_path: std::path::PathBuf,
    ) -> Option<Task<Message>> {
        let key = Maolan::audio_clip_key(track_name, clip_name, start, length, offset);
        if !self.pending_peak_rebuilds.insert(key) {
            return None;
        }

        let track_name = track_name.to_string();
        let clip_name = clip_name.to_string();
        std::thread::spawn(move || {
            if Maolan::stream_audio_clip_peaks_to_queue(
                &wav_path,
                track_name.clone(),
                clip_name.clone(),
                start,
                length,
                offset,
            )
            .is_err()
                && let Ok(mut queue) = AUDIO_PEAK_UPDATES.lock()
            {
                queue.push(super::AudioPeakChunkUpdate {
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
        });
        Some(Task::none())
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn schedule_audio_peak_file_load(
        &mut self,
        track_name: &str,
        clip_name: &str,
        start: usize,
        length: usize,
        offset: usize,
        peaks_path: std::path::PathBuf,
    ) -> Option<Task<Message>> {
        let key = Maolan::audio_clip_key(track_name, clip_name, start, length, offset);
        if !self.pending_peak_rebuilds.insert(key) {
            return None;
        }
        let track_name = track_name.to_string();
        let clip_name = clip_name.to_string();
        std::thread::spawn(move || {
            if Maolan::stream_peak_file_to_queue(
                &peaks_path,
                track_name.clone(),
                clip_name.clone(),
                start,
                length,
                offset,
            )
            .is_err()
                && let Ok(mut queue) = AUDIO_PEAK_UPDATES.lock()
            {
                queue.push(super::AudioPeakChunkUpdate {
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
        });
        Some(Task::none())
    }
}

impl AutomationRuntimeState {
    pub(crate) fn end_touch_gesture(&mut self, track_name: &str, key: AutomationWriteKey) {
        if let Some(active) = self.touch_active_keys.get_mut(track_name) {
            active.remove(&key);
            if active.is_empty() {
                self.touch_active_keys.remove(track_name);
            }
        }
        if let Some(values) = self.touch_automation_overrides.get_mut(track_name) {
            values.remove(&key);
            if values.is_empty() {
                self.touch_automation_overrides.remove(track_name);
            }
        }
    }

    pub(crate) fn collect_track_automation_actions(
        &mut self,
        sample: usize,
        tracks: &[AutomationTrackView],
    ) -> Vec<Action> {
        let now = Instant::now();
        for (track_name, active_keys) in self.touch_active_keys.iter_mut() {
            let values = self.touch_automation_overrides.get(track_name);
            active_keys.retain(|key| {
                if Maolan::key_has_explicit_gesture_lifecycle(*key) {
                    true
                } else {
                    values.and_then(|map| map.get(key)).is_some_and(|entry| {
                        now.duration_since(entry.updated_at) <= Duration::from_millis(220)
                    })
                }
            });
        }
        self.touch_active_keys.retain(|_, keys| !keys.is_empty());
        for (track_name, values) in self.touch_automation_overrides.iter_mut() {
            let active = self.touch_active_keys.get(track_name);
            values.retain(|key, entry| {
                active.is_some_and(|set| set.contains(key))
                    || now.duration_since(entry.updated_at) <= Duration::from_millis(220)
            });
        }
        self.touch_automation_overrides
            .retain(|_, values| !values.is_empty());

        let mut actions = Vec::new();
        for track in tracks {
            if track.automation_mode == TrackAutomationMode::Write {
                continue;
            }
            let mut vol = None;
            let mut bal = None;
            let mut midi_cc_updates: Vec<(u8, u8, u8)> = Vec::new();
            let runtime = self
                .track_automation_runtime
                .entry(track.name.clone())
                .or_default();
            for lane in &track.automation_lanes {
                let Some(key) = Maolan::automation_key(&lane.target) else {
                    continue;
                };
                let override_value = match track.automation_mode {
                    TrackAutomationMode::Touch => self
                        .touch_automation_overrides
                        .get(&track.name)
                        .and_then(|values| values.get(&key))
                        .and_then(|entry| {
                            let active = self
                                .touch_active_keys
                                .get(&track.name)
                                .is_some_and(|set| set.contains(&key));
                            let fresh =
                                now.duration_since(entry.updated_at) <= Duration::from_millis(220);
                            (active || fresh).then_some(entry.value)
                        }),
                    TrackAutomationMode::Latch => self
                        .latch_automation_overrides
                        .get(&track.name)
                        .and_then(|values| values.get(&key))
                        .copied(),
                    _ => None,
                };
                let value = override_value
                    .or_else(|| Maolan::automation_lane_value_at(&lane.points, sample));
                match &lane.target {
                    TrackAutomationTarget::Volume => vol = value,
                    TrackAutomationTarget::Balance => bal = value,
                    TrackAutomationTarget::MidiCc { channel, cc } => {
                        if let Some(v) = value {
                            let cc_value = (v * 127.0).round() as u8;
                            midi_cc_updates.push((*channel, *cc, cc_value));
                        }
                    }
                    #[cfg(unix)]
                    TrackAutomationTarget::Lv2Parameter {
                        instance_id,
                        index,
                        min,
                        max,
                    } => {
                        if track.frozen {
                            continue;
                        }
                        #[cfg(unix)]
                        if let Some(v) = value {
                            let lo = min.min(*max);
                            let hi = max.max(*min);
                            let param_value = (lo + v * (hi - lo)).clamp(lo, hi);
                            let key = (*instance_id, *index);
                            if runtime
                                .lv2_params
                                .get(&key)
                                .is_none_or(|current| (current - param_value).abs() >= 0.0005)
                            {
                                runtime.lv2_params.insert(key, param_value);
                                actions.push(Action::TrackSetLv2ControlValue {
                                    track_name: track.name.clone(),
                                    instance_id: *instance_id,
                                    index: *index,
                                    value: param_value,
                                });
                            }
                        }
                    }
                    #[cfg(not(unix))]
                    TrackAutomationTarget::Lv2Parameter { .. } => {}
                    TrackAutomationTarget::Vst3Parameter {
                        instance_id,
                        param_id,
                    } => {
                        if track.frozen {
                            continue;
                        }
                        if let Some(v) = value {
                            let param_value = v.clamp(0.0, 1.0);
                            let key = (*instance_id, *param_id);
                            if runtime
                                .vst3_params
                                .get(&key)
                                .is_none_or(|current| (current - param_value).abs() >= 0.0005)
                            {
                                runtime.vst3_params.insert(key, param_value);
                                actions.push(Action::TrackSetVst3Parameter {
                                    track_name: track.name.clone(),
                                    instance_id: *instance_id,
                                    param_id: *param_id,
                                    value: param_value,
                                });
                            }
                        }
                    }
                    TrackAutomationTarget::ClapParameter {
                        instance_id,
                        param_id,
                        min,
                        max,
                    } => {
                        if track.frozen {
                            continue;
                        }
                        if let Some(v) = value {
                            let lo = min.min(*max);
                            let hi = max.max(*min);
                            let param_value = (lo + v as f64 * (hi - lo)).clamp(lo, hi);
                            let key = (*instance_id, *param_id);
                            if runtime
                                .clap_params
                                .get(&key)
                                .is_none_or(|current| (current - param_value).abs() >= 0.0005)
                            {
                                runtime.clap_params.insert(key, param_value);
                                actions.push(Action::TrackSetClapParameterAt {
                                    track_name: track.name.clone(),
                                    instance_id: *instance_id,
                                    param_id: *param_id,
                                    value: param_value,
                                    frame: 0,
                                });
                            }
                        }
                    }
                    TrackAutomationTarget::MixOsc { .. } => {}
                }
            }

            if let Some(v) = vol {
                let level_db = (-90.0 + v * 110.0).clamp(-90.0, 20.0);
                if runtime
                    .level_db
                    .is_none_or(|current| (current - level_db).abs() >= 0.1)
                {
                    runtime.level_db = Some(level_db);
                    actions.push(Action::TrackAutomationLevel(track.name.clone(), level_db));
                }
            }
            if let Some(v) = bal {
                let balance = (v * 2.0 - 1.0).clamp(-1.0, 1.0);
                if runtime
                    .balance
                    .is_none_or(|current| (current - balance).abs() >= 0.01)
                {
                    runtime.balance = Some(balance);
                    actions.push(Action::TrackAutomationBalance(track.name.clone(), balance));
                }
            }
            for (channel, cc, value) in midi_cc_updates {
                let key = (channel, cc);
                if runtime
                    .midi_cc
                    .get(&key)
                    .is_none_or(|current| *current != value)
                {
                    runtime.midi_cc.insert(key, value);
                    actions.push(Action::TrackMidiCc {
                        track_name: track.name.clone(),
                        channel,
                        cc,
                        value,
                    });
                }
            }
        }
        actions
    }
}

impl RecordingPreviewState {
    pub(crate) fn stop_recording_preview(&mut self) {
        self.recording_preview_start_sample = None;
        self.recording_preview_sample = None;
        self.recording_preview_peaks.clear();
    }
}

impl UiState {
    pub(crate) fn refresh_log_viewer_content(&mut self, state: &State) {
        let entries = state
            .read()
            .expect("state lock poisoned")
            .log_entries
            .clone();
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

    pub(crate) fn should_drive_playback_ui(&self, state: &State) -> bool {
        let state = state.read().expect("state lock poisoned");
        match state.view {
            crate::state::View::Workspace => {
                self.toolbar_visible
                    || self.tracks_visible
                    || self.editor_visible
                    || self.mixer_visible
            }
            crate::state::View::Piano
            | crate::state::View::PitchCorrection
            | crate::state::View::AudioEditor => true,
            _ => false,
        }
    }
    pub(crate) fn rebuild_midi_mappings_report_lines_from_state(&mut self, state: &State) {
        let state = state.read().expect("state lock poisoned");
        let mut lines = Vec::<String>::new();
        let mut push_binding =
            |scope: String, label: &str, binding: &maolan_engine::message::MidiLearnBinding| {
                lines.push(format!(
                    "{scope} {label}: CH{} CC{}",
                    binding.channel + 1,
                    binding.cc
                ));
            };

        if let Some(binding) = state.global_midi_learn_play_pause.as_ref() {
            push_binding("Global".to_string(), "Play/Pause", binding);
        }
        if let Some(binding) = state.global_midi_learn_stop.as_ref() {
            push_binding("Global".to_string(), "Stop", binding);
        }
        if let Some(binding) = state.global_midi_learn_record_toggle.as_ref() {
            push_binding("Global".to_string(), "Record Toggle", binding);
        }

        for track in &state.tracks {
            if let Some(binding) = track.midi_learn_volume.as_ref() {
                push_binding(format!("Track '{}'", track.name), "Volume", binding);
            }
            if let Some(binding) = track.midi_learn_balance.as_ref() {
                push_binding(format!("Track '{}'", track.name), "Balance", binding);
            }
            if let Some(binding) = track.midi_learn_mute.as_ref() {
                push_binding(format!("Track '{}'", track.name), "Mute", binding);
            }
            if let Some(binding) = track.midi_learn_solo.as_ref() {
                push_binding(format!("Track '{}'", track.name), "Solo", binding);
            }
            if let Some(binding) = track.midi_learn_arm.as_ref() {
                push_binding(format!("Track '{}'", track.name), "Arm", binding);
            }
            if let Some(binding) = track.midi_learn_input_monitor.as_ref() {
                push_binding(format!("Track '{}'", track.name), "Input Monitor", binding);
            }
            if let Some(binding) = track.midi_learn_disk_monitor.as_ref() {
                push_binding(format!("Track '{}'", track.name), "Disk Monitor", binding);
            }
        }
        for ((track_name, scene_index), binding) in &state.session_midi_learn_slots {
            push_binding(
                format!("Track '{}'", track_name),
                &format!("Slot {}", scene_index + 1),
                binding,
            );
        }
        for (scene_index, binding) in &state.session_midi_learn_scenes {
            push_binding(
                "Session".to_string(),
                &format!("Scene {}", scene_index + 1),
                binding,
            );
        }
        for (track_name, binding) in &state.session_midi_learn_stop_track {
            push_binding(format!("Track '{}'", track_name), "Stop Track", binding);
        }
        if let Some(binding) = state.session_midi_learn_stop_all.as_ref() {
            push_binding("Session".to_string(), "Stop All Clips", binding);
        }

        if lines.is_empty() {
            lines.push("No MIDI learn bindings".to_string());
        }
        self.midi_mappings_report_lines = lines;
    }
}

impl SessionOpsState {
    pub(crate) fn is_dirty(&self) -> bool {
        self.has_unsaved_changes || self.engine_dirty
    }
}

impl PluginScanState {
    #[cfg(unix)]
    pub(crate) fn track_plugin_list_view(
        &self,
        state: &State,
    ) -> maolan_widgets::iced::Element<'_, Message> {
        let state = state.read().expect("state lock poisoned");
        let title = Maolan::plugin_graph_title(&state);

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
            let row_content: maolan_widgets::iced::Element<'_, Message> = row![
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

            let row_content: maolan_widgets::iced::Element<'_, Message> = row![
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
            let row_content: maolan_widgets::iced::Element<'_, Message> = row![
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

        let lv2_column: maolan_widgets::iced::Element<'_, Message> =
            if state.lv2_plugins_unavailable {
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

        let clap_column: maolan_widgets::iced::Element<'_, Message> =
            if state.clap_plugins_unavailable {
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

        let vst3_column: maolan_widgets::iced::Element<'_, Message> =
            if state.vst3_plugins_unavailable {
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

    #[cfg(not(unix))]
    pub(crate) fn track_plugin_list_view(
        &self,
        state: &State,
    ) -> maolan_widgets::iced::Element<'_, Message> {
        let state = state.read().expect("state lock poisoned");
        let title = Maolan::plugin_graph_title(&state);
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
            let row_content: maolan_widgets::iced::Element<'_, Message> = row![
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

            let row_content: maolan_widgets::iced::Element<'_, Message> = row![
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

        let clap_column: maolan_widgets::iced::Element<'_, Message> =
            if state.clap_plugins_unavailable {
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

        let vst3_column: maolan_widgets::iced::Element<'_, Message> =
            if state.vst3_plugins_unavailable {
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
}

impl TransferState {
    pub(crate) fn selected_formats(&self) -> Vec<ExportFormat> {
        let mut formats = Vec::new();
        if self.export_format_wav {
            formats.push(ExportFormat::Wav);
        }
        if self.export_format_flac {
            formats.push(ExportFormat::Flac);
        }
        if self.export_format_mp3 {
            formats.push(ExportFormat::Mp3);
        }
        if self.export_format_ogg {
            formats.push(ExportFormat::Ogg);
        }
        formats
    }

    pub(crate) fn available_hw_out_ports(state: &State) -> Vec<usize> {
        let channels = state
            .read()
            .expect("state lock poisoned")
            .hw_out
            .as_ref()
            .map(|hw| hw.channels)
            .unwrap_or(0);
        (0..channels).collect()
    }

    pub(crate) fn default_hw_out_ports(state: &State) -> BTreeSet<usize> {
        Self::available_hw_out_ports(state)
            .into_iter()
            .take(2)
            .collect()
    }

    pub(crate) fn normalize_hw_out_ports(&mut self, state: &State) {
        let available: BTreeSet<usize> = Self::available_hw_out_ports(state).into_iter().collect();
        self.export_hw_out_ports
            .retain(|port| available.contains(port));
        if self.export_hw_out_ports.is_empty() {
            self.export_hw_out_ports = Self::default_hw_out_ports(state);
        }
    }

    pub(crate) fn adjust_bit_depth_if_needed(&mut self) {
        let selected = self.selected_formats();
        let valid_bit_depths = Maolan::export_bit_depth_options(&selected);
        if !valid_bit_depths.contains(&self.export_bit_depth)
            && let Some(first) = valid_bit_depths.first().copied()
        {
            self.export_bit_depth = first;
        }
    }
}
