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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lv2GraphPlugin {
    pub instance_id: usize,
    pub uri: String,
    pub name: String,
    pub audio_inputs: usize,
    pub audio_outputs: usize,
    pub midi_inputs: usize,
    pub midi_outputs: usize,
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
        kind: Kind,
    },
    RemoveClip(usize, String, Kind),
    RemoveTrack(String),
    TrackLevel(String, f32),
    TrackToggleArm(String),
    TrackToggleMute(String),
    TrackToggleSolo(String),
    TrackLoadLv2Plugin {
        track_name: String,
        plugin_uri: String,
    },
    TrackUnloadLv2Plugin {
        track_name: String,
        plugin_uri: String,
    },
    TrackUnloadLv2PluginInstance {
        track_name: String,
        instance_id: usize,
    },
    TrackShowLv2PluginUi {
        track_name: String,
        plugin_uri: String,
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
    OpenMidiDevice(String),
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
    HWFinished,
}
