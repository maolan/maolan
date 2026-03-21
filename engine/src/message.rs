use crate::clap::{ClapParameterInfo, ClapPluginInfo};
#[cfg(all(unix, not(target_os = "macos")))]
use crate::lv2::Lv2PluginInfo;
use crate::midi::io::MidiEvent;
use crate::vst3::Vst3PluginInfo;
use crate::{kind::Kind, mutex::UnsafeMutex, track::Track};
use std::sync::{Arc, atomic::AtomicBool};
use tokio::sync::mpsc::Sender;

#[derive(Clone, Debug)]
pub struct MidiNoteData {
    pub start_sample: usize,
    pub length_samples: usize,
    pub pitch: u8,
    pub velocity: u8,
    pub channel: u8,
}

#[derive(Clone, Debug)]
pub struct MidiControllerData {
    pub sample: usize,
    pub controller: u8,
    pub value: u8,
    pub channel: u8,
}

#[derive(Debug, Clone)]
pub struct MidiRawEventData {
    pub sample: usize,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct HwMidiEvent {
    pub device: String,
    pub event: MidiEvent,
}

#[derive(Clone, Debug)]
pub struct OfflineAutomationPoint {
    pub sample: usize,
    pub value: f32,
}

#[derive(Clone, Debug)]
pub enum OfflineAutomationTarget {
    Volume,
    Balance,
    Mute,
    #[cfg(all(unix, not(target_os = "macos")))]
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

#[derive(Clone, Debug)]
pub struct OfflineAutomationLane {
    pub target: OfflineAutomationTarget,
    pub points: Vec<OfflineAutomationPoint>,
}

#[derive(Clone, Debug)]
pub struct OfflineBounceWork {
    pub state: Arc<UnsafeMutex<crate::state::State>>,
    pub track_name: String,
    pub output_path: String,
    pub start_sample: usize,
    pub length_samples: usize,
    pub tempo_bpm: f64,
    pub tsig_num: u16,
    pub tsig_denom: u16,
    pub automation_lanes: Vec<OfflineAutomationLane>,
    pub cancel: Arc<AtomicBool>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct PitchCorrectionPointData {
    pub start_sample: usize,
    pub length_samples: usize,
    pub detected_midi_pitch: f32,
    pub target_midi_pitch: f32,
    pub clarity: f32,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct AudioClipData {
    pub name: String,
    pub start: usize,
    pub length: usize,
    pub offset: usize,
    pub input_channel: usize,
    pub muted: bool,
    pub peaks_file: Option<String>,
    pub fade_enabled: bool,
    pub fade_in_samples: usize,
    pub fade_out_samples: usize,
    pub preview_name: Option<String>,
    pub source_name: Option<String>,
    pub source_offset: Option<usize>,
    pub source_length: Option<usize>,
    pub pitch_correction_points: Vec<PitchCorrectionPointData>,
    pub pitch_correction_frame_likeness: Option<f32>,
    pub pitch_correction_inertia_ms: Option<u16>,
    pub pitch_correction_formant_compensation: Option<bool>,
    pub plugin_graph_json: Option<serde_json::Value>,
    pub grouped_clips: Vec<AudioClipData>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct MidiClipData {
    pub name: String,
    pub start: usize,
    pub length: usize,
    pub offset: usize,
    pub input_channel: usize,
    pub muted: bool,
    pub grouped_clips: Vec<MidiClipData>,
}

#[derive(Clone, Debug)]
pub struct ClipMoveFrom {
    pub track_name: String,
    pub clip_index: usize,
}

#[derive(Clone, Debug)]
pub struct ClipMoveTo {
    pub track_name: String,
    pub sample_offset: usize,
    pub input_channel: usize,
}

#[cfg(unix)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PluginGraphNode {
    TrackInput,
    TrackOutput,
    Lv2PluginInstance(usize),
    Vst3PluginInstance(usize),
    ClapPluginInstance(usize),
}

#[cfg(unix)]
#[derive(Clone, Debug, PartialEq)]
pub struct PluginGraphPlugin {
    pub node: PluginGraphNode,
    pub instance_id: usize,
    pub format: String,
    pub uri: String,
    pub plugin_id: String,
    pub name: String,
    pub main_audio_inputs: usize,
    pub main_audio_outputs: usize,
    pub audio_inputs: usize,
    pub audio_outputs: usize,
    pub midi_inputs: usize,
    pub midi_outputs: usize,
    pub state: Option<Lv2PluginState>,
}

#[cfg(unix)]
#[derive(Clone, Debug, PartialEq)]
pub struct Lv2StatePortValue {
    pub index: u32,
    pub value: f32,
}

#[cfg(unix)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lv2StateProperty {
    pub key_uri: String,
    pub type_uri: String,
    pub flags: u32,
    pub value: Vec<u8>,
}

#[cfg(unix)]
#[derive(Clone, Debug, PartialEq)]
pub struct Lv2PluginState {
    pub port_values: Vec<Lv2StatePortValue>,
    pub properties: Vec<Lv2StateProperty>,
}

#[cfg(all(unix, not(target_os = "macos")))]
#[derive(Clone, Debug, PartialEq)]
pub struct Lv2ControlPortInfo {
    pub index: u32,
    pub name: String,
    pub min: f32,
    pub max: f32,
    pub value: f32,
}

#[cfg(unix)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginGraphConnection {
    pub from_node: PluginGraphNode,
    pub from_port: usize,
    pub to_node: PluginGraphNode,
    pub to_port: usize,
    pub kind: Kind,
}

#[cfg(unix)]
pub type PluginGraphSnapshot = (Vec<PluginGraphPlugin>, Vec<PluginGraphConnection>);

// VST3 graph types
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Vst3GraphNode {
    TrackInput,
    TrackOutput,
    PluginInstance(usize),
}

#[derive(Clone, Debug)]
pub struct Vst3GraphPlugin {
    pub instance_id: usize,
    pub name: String,
    pub path: String,
    pub audio_inputs: usize,
    pub audio_outputs: usize,
    pub parameters: Vec<crate::vst3::port::ParameterInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Vst3GraphConnection {
    pub from_node: Vst3GraphNode,
    pub from_port: usize,
    pub to_node: Vst3GraphNode,
    pub to_port: usize,
    pub kind: Kind,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MidiLearnBinding {
    pub device: Option<String>,
    pub channel: u8,
    pub cc: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TrackMidiLearnTarget {
    Volume,
    Balance,
    Mute,
    Solo,
    Arm,
    InputMonitor,
    DiskMonitor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum GlobalMidiLearnTarget {
    PlayPause,
    Stop,
    RecordToggle,
}

#[derive(Clone, Debug)]
pub enum Action {
    Quit,
    Play,
    Stop,
    TransportPosition(usize),
    SetLoopEnabled(bool),
    SetLoopRange(Option<(usize, usize)>),
    SetPunchEnabled(bool),
    SetPunchRange(Option<(usize, usize)>),
    SetMetronomeEnabled(bool),
    SetTempo(f64),
    SetTimeSignature {
        numerator: u16,
        denominator: u16,
    },
    SetClipPlaybackEnabled(bool),
    SetRecordEnabled(bool),
    SetSessionPath(String),
    BeginHistoryGroup,
    EndHistoryGroup,
    ClearHistory,
    BeginSessionRestore,
    EndSessionRestore,
    AddTrack {
        name: String,
        audio_ins: usize,
        midi_ins: usize,
        audio_outs: usize,
        midi_outs: usize,
    },
    TrackAddAudioInput(String),
    TrackAddAudioOutput(String),
    TrackRemoveAudioInput(String),
    TrackRemoveAudioOutput(String),
    AddClip {
        name: String,
        track_name: String,
        start: usize,
        length: usize,
        offset: usize,
        input_channel: usize,
        muted: bool,
        peaks_file: Option<String>,
        kind: Kind,
        fade_enabled: bool,
        fade_in_samples: usize,
        fade_out_samples: usize,
        source_name: Option<String>,
        source_offset: Option<usize>,
        source_length: Option<usize>,
        preview_name: Option<String>,
        pitch_correction_points: Vec<PitchCorrectionPointData>,
        pitch_correction_frame_likeness: Option<f32>,
        pitch_correction_inertia_ms: Option<u16>,
        pitch_correction_formant_compensation: Option<bool>,
        plugin_graph_json: Option<serde_json::Value>,
    },
    AddGroupedClip {
        track_name: String,
        kind: Kind,
        audio_clip: Option<AudioClipData>,
        midi_clip: Option<MidiClipData>,
    },
    RemoveClip {
        track_name: String,
        kind: Kind,
        clip_indices: Vec<usize>,
    },
    SetClipFade {
        track_name: String,
        clip_index: usize,
        kind: Kind,
        fade_enabled: bool,
        fade_in_samples: usize,
        fade_out_samples: usize,
    },
    SetClipBounds {
        track_name: String,
        clip_index: usize,
        kind: Kind,
        start: usize,
        length: usize,
        offset: usize,
    },
    SetClipMuted {
        track_name: String,
        clip_index: usize,
        kind: Kind,
        muted: bool,
    },
    SetClipPluginGraphJson {
        track_name: String,
        clip_index: usize,
        plugin_graph_json: Option<serde_json::Value>,
    },
    SetClipPitchCorrection {
        track_name: String,
        clip_index: usize,
        preview_name: Option<String>,
        source_name: Option<String>,
        source_offset: Option<usize>,
        source_length: Option<usize>,
        pitch_correction_points: Vec<PitchCorrectionPointData>,
        pitch_correction_frame_likeness: Option<f32>,
        pitch_correction_inertia_ms: Option<u16>,
        pitch_correction_formant_compensation: Option<bool>,
    },
    RenameClip {
        track_name: String,
        kind: Kind,
        clip_index: usize,
        new_name: String,
    },
    RenameTrack {
        old_name: String,
        new_name: String,
    },
    RemoveTrack(String),
    TrackLevel(String, f32),
    TrackBalance(String, f32),
    TrackAutomationLevel(String, f32),
    TrackAutomationBalance(String, f32),
    TrackAutomationMute(String, bool),
    TrackMeters {
        track_name: String,
        output_db: Vec<f32>,
    },
    RequestMeterSnapshot,
    MeterSnapshot {
        hw_out_db: Arc<Vec<f32>>,
        track_meters: Arc<Vec<(String, Vec<f32>)>>,
    },
    TrackToggleArm(String),
    TrackToggleMute(String),
    TrackToggleSolo(String),
    TrackToggleInputMonitor(String),
    TrackToggleDiskMonitor(String),
    TrackArmMidiLearn {
        track_name: String,
        target: TrackMidiLearnTarget,
    },
    GlobalArmMidiLearn {
        target: GlobalMidiLearnTarget,
    },
    TrackSetMidiLearnBinding {
        track_name: String,
        target: TrackMidiLearnTarget,
        binding: Option<MidiLearnBinding>,
    },
    SetGlobalMidiLearnBinding {
        target: GlobalMidiLearnTarget,
        binding: Option<MidiLearnBinding>,
    },
    TrackSetVcaMaster {
        track_name: String,
        master_track: Option<String>,
    },
    TrackSetMidiLaneChannel {
        track_name: String,
        lane: usize,
        channel: Option<u8>,
    },
    TrackSetFrozen {
        track_name: String,
        frozen: bool,
    },
    TrackOfflineBounce {
        track_name: String,
        output_path: String,
        start_sample: usize,
        length_samples: usize,
        automation_lanes: Vec<OfflineAutomationLane>,
    },
    TrackOfflineBounceCancel {
        track_name: String,
    },
    TrackOfflineBounceCanceled {
        track_name: String,
    },
    TrackOfflineBounceProgress {
        track_name: String,
        progress: f32,
        operation: Option<String>,
    },
    PianoKey {
        track_name: String,
        note: u8,
        velocity: u8,
        on: bool,
    },
    ModifyMidiNotes {
        track_name: String,
        clip_index: usize,
        note_indices: Vec<usize>,
        new_notes: Vec<MidiNoteData>,
        old_notes: Vec<MidiNoteData>,
    },
    ModifyMidiControllers {
        track_name: String,
        clip_index: usize,
        controller_indices: Vec<usize>,
        new_controllers: Vec<MidiControllerData>,
        old_controllers: Vec<MidiControllerData>,
    },
    DeleteMidiControllers {
        track_name: String,
        clip_index: usize,
        controller_indices: Vec<usize>,
        deleted_controllers: Vec<(usize, MidiControllerData)>,
    },
    InsertMidiControllers {
        track_name: String,
        clip_index: usize,
        controllers: Vec<(usize, MidiControllerData)>,
    },
    DeleteMidiNotes {
        track_name: String,
        clip_index: usize,
        note_indices: Vec<usize>,
        deleted_notes: Vec<(usize, MidiNoteData)>,
    },
    InsertMidiNotes {
        track_name: String,
        clip_index: usize,
        notes: Vec<(usize, MidiNoteData)>,
    },
    SetMidiSysExEvents {
        track_name: String,
        clip_index: usize,
        new_sysex_events: Vec<MidiRawEventData>,
        old_sysex_events: Vec<MidiRawEventData>,
    },
    #[cfg(all(unix, not(target_os = "macos")))]
    TrackLoadLv2Plugin {
        track_name: String,
        plugin_uri: String,
    },
    TrackClearDefaultPassthrough {
        track_name: String,
    },
    #[cfg(all(unix, not(target_os = "macos")))]
    TrackSetLv2PluginState {
        track_name: String,
        instance_id: usize,
        state: Lv2PluginState,
    },
    #[cfg(all(unix, not(target_os = "macos")))]
    ClipSetLv2PluginState {
        track_name: String,
        clip_idx: usize,
        instance_id: usize,
        state: Lv2PluginState,
    },
    #[cfg(all(unix, not(target_os = "macos")))]
    TrackUnloadLv2PluginInstance {
        track_name: String,
        instance_id: usize,
    },
    #[cfg(all(unix, not(target_os = "macos")))]
    TrackGetLv2PluginControls {
        track_name: String,
        instance_id: usize,
    },
    #[cfg(all(unix, not(target_os = "macos")))]
    ClipGetLv2PluginControls {
        track_name: String,
        clip_idx: usize,
        instance_id: usize,
    },
    #[cfg(all(unix, not(target_os = "macos")))]
    TrackLv2PluginControls {
        track_name: String,
        instance_id: usize,
        controls: Vec<Lv2ControlPortInfo>,
        instance_access_handle: Option<usize>,
    },
    #[cfg(all(unix, not(target_os = "macos")))]
    ClipLv2PluginControls {
        track_name: String,
        clip_idx: usize,
        instance_id: usize,
        controls: Vec<Lv2ControlPortInfo>,
        instance_access_handle: Option<usize>,
    },
    #[cfg(all(unix, not(target_os = "macos")))]
    TrackGetLv2Midnam {
        track_name: String,
    },
    #[cfg(all(unix, not(target_os = "macos")))]
    TrackLv2Midnam {
        track_name: String,
        note_names: std::collections::HashMap<u8, String>,
    },
    #[cfg(all(unix, not(target_os = "macos")))]
    TrackSetLv2ControlValue {
        track_name: String,
        instance_id: usize,
        index: u32,
        value: f32,
    },
    #[cfg(all(unix, not(target_os = "macos")))]
    ClipSetLv2ControlValue {
        track_name: String,
        clip_idx: usize,
        instance_id: usize,
        index: u32,
        value: f32,
    },
    #[cfg(all(unix, not(target_os = "macos")))]
    ClipLv2StateSnapshot {
        track_name: String,
        clip_idx: usize,
        instance_id: usize,
        state: Lv2PluginState,
    },
    #[cfg(unix)]
    TrackGetPluginGraph {
        track_name: String,
    },
    #[cfg(unix)]
    TrackPluginGraph {
        track_name: String,
        plugins: Vec<PluginGraphPlugin>,
        connections: Vec<PluginGraphConnection>,
    },
    #[cfg(unix)]
    TrackConnectPluginAudio {
        track_name: String,
        from_node: PluginGraphNode,
        from_port: usize,
        to_node: PluginGraphNode,
        to_port: usize,
    },
    #[cfg(unix)]
    TrackConnectPluginMidi {
        track_name: String,
        from_node: PluginGraphNode,
        from_port: usize,
        to_node: PluginGraphNode,
        to_port: usize,
    },
    #[cfg(unix)]
    TrackDisconnectPluginAudio {
        track_name: String,
        from_node: PluginGraphNode,
        from_port: usize,
        to_node: PluginGraphNode,
        to_port: usize,
    },
    #[cfg(unix)]
    TrackDisconnectPluginMidi {
        track_name: String,
        from_node: PluginGraphNode,
        from_port: usize,
        to_node: PluginGraphNode,
        to_port: usize,
    },
    #[cfg(all(unix, not(target_os = "macos")))]
    ListLv2Plugins,
    #[cfg(all(unix, not(target_os = "macos")))]
    Lv2Plugins(Vec<Lv2PluginInfo>),
    ListVst3Plugins,
    Vst3Plugins(Vec<Vst3PluginInfo>),
    ListClapPlugins,
    ListClapPluginsWithCapabilities,
    ClapPlugins(Vec<ClapPluginInfo>),
    TrackLoadClapPlugin {
        track_name: String,
        plugin_path: String,
    },
    TrackUnloadClapPlugin {
        track_name: String,
        plugin_path: String,
    },
    TrackSetClapParameter {
        track_name: String,
        instance_id: usize,
        param_id: u32,
        value: f64,
    },
    TrackSetClapParameterAt {
        track_name: String,
        instance_id: usize,
        param_id: u32,
        value: f64,
        frame: u32,
    },
    TrackBeginClapParameterEdit {
        track_name: String,
        instance_id: usize,
        param_id: u32,
        frame: u32,
    },
    TrackEndClapParameterEdit {
        track_name: String,
        instance_id: usize,
        param_id: u32,
        frame: u32,
    },
    TrackGetClapParameters {
        track_name: String,
        instance_id: usize,
    },
    TrackClapParameters {
        track_name: String,
        instance_id: usize,
        parameters: Vec<ClapParameterInfo>,
    },
    TrackClapSnapshotState {
        track_name: String,
        instance_id: usize,
    },
    ClipClapSnapshotState {
        track_name: String,
        clip_idx: usize,
        instance_id: usize,
    },
    TrackClapStateSnapshot {
        track_name: String,
        instance_id: usize,
        plugin_path: String,
        state: crate::clap::ClapPluginState,
    },
    ClipClapStateSnapshot {
        track_name: String,
        clip_idx: usize,
        instance_id: usize,
        plugin_path: String,
        state: crate::clap::ClapPluginState,
    },
    TrackClapRestoreState {
        track_name: String,
        instance_id: usize,
        state: crate::clap::ClapPluginState,
    },
    TrackSnapshotAllClapStates {
        track_name: String,
    },
    TrackLoadVst3Plugin {
        track_name: String,
        plugin_path: String,
    },
    TrackUnloadVst3PluginInstance {
        track_name: String,
        instance_id: usize,
    },
    TrackGetVst3Graph {
        track_name: String,
    },
    TrackVst3Graph {
        track_name: String,
        plugins: Vec<Vst3GraphPlugin>,
        connections: Vec<Vst3GraphConnection>,
    },
    TrackSetVst3Parameter {
        track_name: String,
        instance_id: usize,
        param_id: u32,
        value: f32,
    },
    TrackGetVst3Parameters {
        track_name: String,
        instance_id: usize,
    },
    TrackVst3Parameters {
        track_name: String,
        instance_id: usize,
        parameters: Vec<crate::vst3::port::ParameterInfo>,
    },
    TrackVst3SnapshotState {
        track_name: String,
        instance_id: usize,
    },
    ClipVst3SnapshotState {
        track_name: String,
        clip_idx: usize,
        instance_id: usize,
    },
    TrackVst3StateSnapshot {
        track_name: String,
        instance_id: usize,
        state: crate::vst3::state::Vst3PluginState,
    },
    ClipVst3StateSnapshot {
        track_name: String,
        clip_idx: usize,
        instance_id: usize,
        state: crate::vst3::state::Vst3PluginState,
    },
    TrackVst3RestoreState {
        track_name: String,
        instance_id: usize,
        state: crate::vst3::state::Vst3PluginState,
    },
    TrackConnectVst3Audio {
        track_name: String,
        from_node: Vst3GraphNode,
        from_port: usize,
        to_node: Vst3GraphNode,
        to_port: usize,
    },
    TrackDisconnectVst3Audio {
        track_name: String,
        from_node: Vst3GraphNode,
        from_port: usize,
        to_node: Vst3GraphNode,
        to_port: usize,
    },
    ClipMove {
        kind: Kind,
        from: ClipMoveFrom,
        to: ClipMoveTo,
        copy: bool,
    },
    Connect {
        from_track: String,
        from_port: usize,
        to_track: String,
        to_port: usize,
        kind: Kind,
    },
    Disconnect {
        from_track: String,
        from_port: usize,
        to_track: String,
        to_port: usize,
        kind: Kind,
    },
    OpenAudioDevice {
        device: String,
        input_device: Option<String>,
        sample_rate_hz: i32,
        bits: i32,
        exclusive: bool,
        period_frames: usize,
        nperiods: usize,
        sync_mode: bool,
    },
    JackAddAudioInputPort,
    JackRemoveAudioInputPort(usize),
    JackAddAudioOutputPort,
    JackRemoveAudioOutputPort(usize),
    OpenMidiInputDevice(String),
    OpenMidiOutputDevice(String),
    RequestSessionDiagnostics,
    RequestMidiLearnMappingsReport,
    ClearAllMidiLearnBindings,
    SessionDiagnosticsReport {
        track_count: usize,
        frozen_track_count: usize,
        audio_clip_count: usize,
        midi_clip_count: usize,
        #[cfg(all(unix, not(target_os = "macos")))]
        lv2_instance_count: usize,
        vst3_instance_count: usize,
        clap_instance_count: usize,
        pending_requests: usize,
        workers_total: usize,
        workers_ready: usize,
        pending_hw_midi_events: usize,
        playing: bool,
        transport_sample: usize,
        tempo_bpm: f64,
        sample_rate_hz: usize,
        cycle_samples: usize,
    },
    MidiLearnMappingsReport {
        lines: Vec<String>,
    },
    HWInfo {
        channels: usize,
        rate: usize,
        input: bool,
    },
    Undo,
    Redo,
    Panic,
}

#[derive(Clone, Debug)]
pub enum Message {
    Ready(usize),
    Finished {
        worker_id: usize,
        track_name: String,
        output_linear: Vec<f32>,
        process_epoch: usize,
    },
    TracksFinished,

    ProcessTrack(Arc<UnsafeMutex<Box<Track>>>),
    ProcessOfflineBounce(OfflineBounceWork),
    Channel(Sender<Self>),

    Request(Action),
    Response(Result<Action, String>),
    HWMidiEvents(Vec<HwMidiEvent>),
    HWMidiOutEvents(Vec<HwMidiEvent>),
    ClearHWMidiOutEvents,
    HWFinished,
    OfflineBounceFinished {
        result: Result<Action, String>,
    },
}

#[cfg(test)]
mod tests {
    use super::{AudioClipData, MidiClipData, PitchCorrectionPointData};
    use serde_json::json;

    #[test]
    fn audio_clip_data_serde_round_trips_nested_groups() {
        let clip = AudioClipData {
            name: "group.wav".to_string(),
            start: 12,
            length: 96,
            offset: 3,
            input_channel: 1,
            muted: true,
            peaks_file: Some("peaks/group.json".to_string()),
            fade_enabled: false,
            fade_in_samples: 10,
            fade_out_samples: 20,
            preview_name: Some("preview.wav".to_string()),
            source_name: Some("source.wav".to_string()),
            source_offset: Some(4),
            source_length: Some(88),
            pitch_correction_points: vec![PitchCorrectionPointData {
                start_sample: 7,
                length_samples: 11,
                detected_midi_pitch: 60.1,
                target_midi_pitch: 61.2,
                clarity: 0.8,
            }],
            pitch_correction_frame_likeness: Some(0.5),
            pitch_correction_inertia_ms: Some(123),
            pitch_correction_formant_compensation: Some(false),
            plugin_graph_json: Some(json!({"plugins":[],"connections":[{"kind":"Audio"}]})),
            grouped_clips: vec![AudioClipData {
                name: "child.wav".to_string(),
                start: 0,
                length: 48,
                ..AudioClipData::default()
            }],
        };

        let value = serde_json::to_value(&clip).expect("serialize");
        let restored: AudioClipData = serde_json::from_value(value).expect("deserialize");

        assert_eq!(restored.name, clip.name);
        assert_eq!(restored.preview_name, clip.preview_name);
        assert_eq!(restored.source_name, clip.source_name);
        assert_eq!(restored.plugin_graph_json, clip.plugin_graph_json);
        assert_eq!(restored.grouped_clips.len(), 1);
        assert_eq!(restored.grouped_clips[0].name, "child.wav");
        assert_eq!(restored.pitch_correction_points[0].target_midi_pitch, 61.2);
    }

    #[test]
    fn midi_clip_data_serde_round_trips_nested_groups() {
        let clip = MidiClipData {
            name: "group.mid".to_string(),
            start: 5,
            length: 64,
            offset: 2,
            input_channel: 3,
            muted: true,
            grouped_clips: vec![MidiClipData {
                name: "child.mid".to_string(),
                start: 0,
                length: 32,
                ..MidiClipData::default()
            }],
        };

        let value = serde_json::to_value(&clip).expect("serialize");
        let restored: MidiClipData = serde_json::from_value(value).expect("deserialize");

        assert_eq!(restored.name, clip.name);
        assert_eq!(restored.grouped_clips.len(), 1);
        assert_eq!(restored.grouped_clips[0].name, "child.mid");
    }

    #[test]
    fn pitch_correction_point_data_serde_round_trips() {
        let point = PitchCorrectionPointData {
            start_sample: 10,
            length_samples: 20,
            detected_midi_pitch: 57.5,
            target_midi_pitch: 58.0,
            clarity: 0.9,
        };

        let value = serde_json::to_value(&point).expect("serialize");
        let restored: PitchCorrectionPointData =
            serde_json::from_value(value).expect("deserialize");

        assert_eq!(restored.start_sample, 10);
        assert_eq!(restored.length_samples, 20);
        assert_eq!(restored.detected_midi_pitch, 57.5);
        assert_eq!(restored.target_midi_pitch, 58.0);
        assert_eq!(restored.clarity, 0.9);
    }

    #[test]
    fn audio_clip_data_deserializes_with_omitted_optional_fields() {
        let restored: AudioClipData = serde_json::from_value(json!({
            "name": "clip.wav",
            "start": 1,
            "length": 2,
            "offset": 3,
            "input_channel": 0,
            "muted": false,
            "fade_enabled": true,
            "fade_in_samples": 240,
            "fade_out_samples": 240,
            "pitch_correction_points": [],
            "grouped_clips": []
        }))
        .expect("deserialize");

        assert_eq!(restored.name, "clip.wav");
        assert!(restored.peaks_file.is_none());
        assert!(restored.preview_name.is_none());
        assert!(restored.source_name.is_none());
        assert!(restored.source_offset.is_none());
        assert!(restored.source_length.is_none());
        assert!(restored.pitch_correction_points.is_empty());
        assert!(restored.plugin_graph_json.is_none());
    }
}
