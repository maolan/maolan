use crate::midi::io::MidiEvent;
use crate::{kind::Kind, mutex::UnsafeMutex, track::Track};
#[cfg(not(target_os = "macos"))]
use crate::lv2::Lv2PluginInfo;
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
}

#[cfg(not(target_os = "macos"))]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Lv2GraphNode {
    TrackInput,
    TrackOutput,
    PluginInstance(usize),
}

#[cfg(not(target_os = "macos"))]
#[derive(Clone, Debug, PartialEq)]
pub struct Lv2GraphPlugin {
    pub instance_id: usize,
    pub uri: String,
    pub name: String,
    pub audio_inputs: usize,
    pub audio_outputs: usize,
    pub midi_inputs: usize,
    pub midi_outputs: usize,
    pub state: Lv2PluginState,
}

#[cfg(not(target_os = "macos"))]
#[derive(Clone, Debug, PartialEq)]
pub struct Lv2StatePortValue {
    pub index: u32,
    pub value: f32,
}

#[cfg(not(target_os = "macos"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lv2StateProperty {
    pub key_uri: String,
    pub type_uri: String,
    pub flags: u32,
    pub value: Vec<u8>,
}

#[cfg(not(target_os = "macos"))]
#[derive(Clone, Debug, PartialEq)]
pub struct Lv2PluginState {
    pub port_values: Vec<Lv2StatePortValue>,
    pub properties: Vec<Lv2StateProperty>,
}

#[cfg(not(target_os = "macos"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lv2GraphConnection {
    pub from_node: Lv2GraphNode,
    pub from_port: usize,
    pub to_node: Lv2GraphNode,
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
        kind: Kind,
    },
    RemoveClip {
        track_name: String,
        kind: Kind,
        clip_indices: Vec<usize>,
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
    #[cfg(not(target_os = "macos"))]
    TrackLoadLv2Plugin {
        track_name: String,
        plugin_uri: String,
    },
    TrackClearDefaultPassthrough {
        track_name: String,
    },
    #[cfg(not(target_os = "macos"))]
    TrackSetLv2PluginState {
        track_name: String,
        instance_id: usize,
        state: Lv2PluginState,
    },
    #[cfg(not(target_os = "macos"))]
    TrackUnloadLv2PluginInstance {
        track_name: String,
        instance_id: usize,
    },
    #[cfg(not(target_os = "macos"))]
    TrackShowLv2PluginUiInstance {
        track_name: String,
        instance_id: usize,
    },
    #[cfg(not(target_os = "macos"))]
    TrackGetLv2Graph {
        track_name: String,
    },
    #[cfg(not(target_os = "macos"))]
    TrackLv2Graph {
        track_name: String,
        plugins: Vec<Lv2GraphPlugin>,
        connections: Vec<Lv2GraphConnection>,
    },
    #[cfg(not(target_os = "macos"))]
    TrackConnectLv2Audio {
        track_name: String,
        from_node: Lv2GraphNode,
        from_port: usize,
        to_node: Lv2GraphNode,
        to_port: usize,
    },
    #[cfg(not(target_os = "macos"))]
    TrackConnectLv2Midi {
        track_name: String,
        from_node: Lv2GraphNode,
        from_port: usize,
        to_node: Lv2GraphNode,
        to_port: usize,
    },
    #[cfg(not(target_os = "macos"))]
    TrackDisconnectLv2Audio {
        track_name: String,
        from_node: Lv2GraphNode,
        from_port: usize,
        to_node: Lv2GraphNode,
        to_port: usize,
    },
    #[cfg(not(target_os = "macos"))]
    TrackDisconnectLv2Midi {
        track_name: String,
        from_node: Lv2GraphNode,
        from_port: usize,
        to_node: Lv2GraphNode,
        to_port: usize,
    },
    #[cfg(not(target_os = "macos"))]
    ListLv2Plugins,
    #[cfg(not(target_os = "macos"))]
    Lv2Plugins(Vec<Lv2PluginInfo>),
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
