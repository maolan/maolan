use crate::clap::{ClapParameterInfo, ClapPluginInfo};
#[cfg(all(unix, not(target_os = "macos")))]
use crate::lv2::Lv2PluginInfo;
use crate::midi::io::MidiEvent;
use crate::vst3::Vst3PluginInfo;
use crate::{kind::Kind, mutex::UnsafeMutex, track::Track};
use std::sync::Arc;
use tokio::sync::mpsc::Sender;

#[derive(Clone, Debug)]
pub struct HwMidiEvent {
    pub device: String,
    pub event: MidiEvent,
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

#[cfg(any(unix, target_os = "windows"))]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PluginGraphNode {
    TrackInput,
    TrackOutput,
    Lv2PluginInstance(usize),
    Vst3PluginInstance(usize),
    ClapPluginInstance(usize),
}

#[cfg(any(unix, target_os = "windows"))]
#[derive(Clone, Debug, PartialEq)]
pub struct PluginGraphPlugin {
    pub node: PluginGraphNode,
    pub instance_id: usize,
    pub format: String,
    pub uri: String,
    pub plugin_id: String,
    pub name: String,
    pub audio_inputs: usize,
    pub audio_outputs: usize,
    pub midi_inputs: usize,
    pub midi_outputs: usize,
    pub state: Option<Lv2PluginState>,
}

#[cfg(any(unix, target_os = "windows"))]
#[derive(Clone, Debug, PartialEq)]
pub struct Lv2StatePortValue {
    pub index: u32,
    pub value: f32,
}

#[cfg(any(unix, target_os = "windows"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lv2StateProperty {
    pub key_uri: String,
    pub type_uri: String,
    pub flags: u32,
    pub value: Vec<u8>,
}

#[cfg(any(unix, target_os = "windows"))]
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

#[cfg(any(unix, target_os = "windows"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginGraphConnection {
    pub from_node: PluginGraphNode,
    pub from_port: usize,
    pub to_node: PluginGraphNode,
    pub to_port: usize,
    pub kind: Kind,
}

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
    SetClipPlaybackEnabled(bool),
    SetRecordEnabled(bool),
    SetSessionPath(String),
    AddTrack {
        name: String,
        audio_ins: usize,
        midi_ins: usize,
        audio_outs: usize,
        midi_outs: usize,
    },
    AddClip {
        name: String,
        track_name: String,
        start: usize,
        length: usize,
        offset: usize,
        input_channel: usize,
        kind: Kind,
    },
    RemoveClip {
        track_name: String,
        kind: Kind,
        clip_indices: Vec<usize>,
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
    TrackMeters {
        track_name: String,
        output_db: Vec<f32>,
    },
    TrackToggleArm(String),
    TrackToggleMute(String),
    TrackToggleSolo(String),
    TrackToggleInputMonitor(String),
    TrackToggleDiskMonitor(String),
    PianoKey {
        track_name: String,
        note: u8,
        velocity: u8,
        on: bool,
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
    TrackLv2PluginControls {
        track_name: String,
        instance_id: usize,
        controls: Vec<Lv2ControlPortInfo>,
        instance_access_handle: Option<usize>,
    },
    #[cfg(all(unix, not(target_os = "macos")))]
    TrackSetLv2ControlValue {
        track_name: String,
        instance_id: usize,
        index: u32,
        value: f32,
    },
    #[cfg(any(unix, target_os = "windows"))]
    TrackGetPluginGraph {
        track_name: String,
    },
    #[cfg(any(unix, target_os = "windows"))]
    TrackPluginGraph {
        track_name: String,
        plugins: Vec<PluginGraphPlugin>,
        connections: Vec<PluginGraphConnection>,
    },
    #[cfg(any(unix, target_os = "windows"))]
    TrackConnectPluginAudio {
        track_name: String,
        from_node: PluginGraphNode,
        from_port: usize,
        to_node: PluginGraphNode,
        to_port: usize,
    },
    #[cfg(any(unix, target_os = "windows"))]
    TrackConnectPluginMidi {
        track_name: String,
        from_node: PluginGraphNode,
        from_port: usize,
        to_node: PluginGraphNode,
        to_port: usize,
    },
    #[cfg(any(unix, target_os = "windows"))]
    TrackDisconnectPluginAudio {
        track_name: String,
        from_node: PluginGraphNode,
        from_port: usize,
        to_node: PluginGraphNode,
        to_port: usize,
    },
    #[cfg(any(unix, target_os = "windows"))]
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
    TrackClapStateSnapshot {
        track_name: String,
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
    #[cfg(target_os = "windows")]
    TrackOpenVst3Editor {
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
    TrackVst3StateSnapshot {
        track_name: String,
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
        bits: i32,
        exclusive: bool,
        period_frames: usize,
        nperiods: usize,
        sync_mode: bool,
    },
    OpenMidiInputDevice(String),
    OpenMidiOutputDevice(String),
    HWInfo {
        channels: usize,
        rate: usize,
        input: bool,
    },
}

#[derive(Clone, Debug)]
pub enum Message {
    Ready(usize),
    Finished(usize),
    TracksFinished,

    ProcessTrack(Arc<UnsafeMutex<Box<Track>>>),
    Channel(Sender<Self>),

    Request(Action),
    Response(Result<Action, String>),
    HWMidiEvents(Vec<HwMidiEvent>),
    HWMidiOutEvents(Vec<HwMidiEvent>),
    HWFinished,
}
