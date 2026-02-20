use crate::midi::io::MidiEvent;
use crate::{kind::Kind, lv2::Lv2PluginInfo, mutex::UnsafeMutex, track::Track};
use std::sync::Arc;
use tokio::sync::mpsc::Sender;

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

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Lv2GraphNode {
    TrackInput,
    TrackOutput,
    PluginInstance(usize),
}

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

#[derive(Clone, Debug, PartialEq)]
pub struct Lv2StatePortValue {
    pub index: u32,
    pub value: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lv2StateProperty {
    pub key_uri: String,
    pub type_uri: String,
    pub flags: u32,
    pub value: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Lv2PluginState {
    pub port_values: Vec<Lv2StatePortValue>,
    pub properties: Vec<Lv2StateProperty>,
}

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
    RemoveTrack(String),
    TrackLevel(String, f32),
    TrackToggleArm(String),
    TrackToggleMute(String),
    TrackToggleSolo(String),
    TrackLoadLv2Plugin {
        track_name: String,
        plugin_uri: String,
    },
    TrackClearDefaultPassthrough {
        track_name: String,
    },
    TrackSetLv2PluginState {
        track_name: String,
        instance_id: usize,
        state: Lv2PluginState,
    },
    TrackUnloadLv2PluginInstance {
        track_name: String,
        instance_id: usize,
    },
    TrackShowLv2PluginUiInstance {
        track_name: String,
        instance_id: usize,
    },
    TrackGetLv2Graph {
        track_name: String,
    },
    TrackLv2Graph {
        track_name: String,
        plugins: Vec<Lv2GraphPlugin>,
        connections: Vec<Lv2GraphConnection>,
    },
    TrackConnectLv2Audio {
        track_name: String,
        from_node: Lv2GraphNode,
        from_port: usize,
        to_node: Lv2GraphNode,
        to_port: usize,
    },
    TrackConnectLv2Midi {
        track_name: String,
        from_node: Lv2GraphNode,
        from_port: usize,
        to_node: Lv2GraphNode,
        to_port: usize,
    },
    TrackDisconnectLv2Audio {
        track_name: String,
        from_node: Lv2GraphNode,
        from_port: usize,
        to_node: Lv2GraphNode,
        to_port: usize,
    },
    TrackDisconnectLv2Midi {
        track_name: String,
        from_node: Lv2GraphNode,
        from_port: usize,
        to_node: Lv2GraphNode,
        to_port: usize,
    },
    ListLv2Plugins,
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
    OpenAudioDevice(String),
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
    HWMidiEvents(Vec<MidiEvent>),
    HWMidiOutEvents(Vec<MidiEvent>),
    HWFinished,
}
