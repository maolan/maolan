mod clip;
mod connection;
mod track;

pub use clip::{AudioClip, MIDIClip};
pub use connection::Connection;
use iced::{Length, Point};
use maolan_engine::kind::Kind;
use maolan_engine::lv2::Lv2PluginInfo;
use maolan_engine::message::{Lv2GraphConnection, Lv2GraphNode, Lv2GraphPlugin};
use std::{
    collections::{HashMap, HashSet},
    fs::read_dir,
    sync::Arc,
    time::Instant,
};
use tokio::sync::RwLock;
pub use track::Track;

pub const HW_IN_ID: &str = "hw:in";
pub const HW_OUT_ID: &str = "hw:out";
pub const MIDI_HW_IN_ID: &str = "midi:hw:in";
pub const MIDI_HW_OUT_ID: &str = "midi:hw:out";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClipId {
    pub track_idx: String,
    pub clip_idx: usize,
    pub kind: Kind,
}

#[derive(Debug, Clone)]
pub enum Resizing {
    Clip(Kind, String, usize, bool, f32, f32, f32),
    Mixer(f32, f32),
    Track(String, f32, f32),
    Tracks(f32, f32),
}

#[derive(Debug, Clone)]
pub struct Connecting {
    pub from_track: String,
    pub from_port: usize,
    pub kind: Kind,
    pub point: Point,
    pub is_input: bool,
}

#[derive(Debug, Clone)]
pub struct MovingTrack {
    pub track_idx: String,
    pub offset_x: f32,
    pub offset_y: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Hovering {
    Port {
        track_idx: String,
        port_idx: usize,
        is_input: bool,
    },
    Track(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionViewSelection {
    Tracks(HashSet<String>),
    Connections(HashSet<usize>),
    None,
}

#[derive(Debug, Clone)]
pub enum View {
    Workspace,
    Connections,
    TrackPlugins,
}

#[derive(Debug, Clone)]
pub struct HW {
    pub channels: usize,
}

#[derive(Debug, Clone)]
pub struct Lv2Connecting {
    pub from_node: Lv2GraphNode,
    pub from_port: usize,
    pub kind: Kind,
    pub point: Point,
    pub is_input: bool,
}

#[derive(Debug, Clone)]
pub struct MovingPlugin {
    pub instance_id: usize,
    pub offset_x: f32,
    pub offset_y: f32,
}

#[derive(Debug)]
pub struct StateData {
    pub shift: bool,
    pub ctrl: bool,
    pub tracks: Vec<Track>,
    pub connections: Vec<Connection>,
    pub selected: HashSet<String>,
    pub selected_clips: HashSet<ClipId>,
    pub message: String,
    pub resizing: Option<Resizing>,
    pub connecting: Option<Connecting>,
    pub moving_track: Option<MovingTrack>,
    pub hovering: Option<Hovering>,
    pub connection_view_selection: ConnectionViewSelection,
    pub cursor: Point,
    pub mixer_height: Length,
    pub tracks_width: Length,
    pub view: View,
    pub pending_track_positions: HashMap<String, Point>,
    pub pending_track_heights: HashMap<String, f32>,
    pub hovered_track_resize_handle: Option<String>,
    pub hw_loaded: bool,
    pub available_hw: Vec<String>,
    pub selected_hw: Option<String>,
    pub opened_midi_in_hw: Vec<String>,
    pub opened_midi_out_hw: Vec<String>,
    pub midi_hw_labels: HashMap<String, String>,
    pub midi_hw_in_positions: HashMap<String, Point>,
    pub midi_hw_out_positions: HashMap<String, Point>,
    pub hw_in: Option<HW>,
    pub hw_out: Option<HW>,
    pub hw_out_level: f32,
    pub hw_out_balance: f32,
    pub hw_out_muted: bool,
    pub hw_out_meter_db: Vec<f32>,
    pub lv2_plugins: Vec<Lv2PluginInfo>,
    pub lv2_graph_track: Option<String>,
    pub lv2_graph_plugins: Vec<Lv2GraphPlugin>,
    pub lv2_graph_connections: Vec<Lv2GraphConnection>,
    pub lv2_graphs_by_track: HashMap<String, (Vec<Lv2GraphPlugin>, Vec<Lv2GraphConnection>)>,
    pub lv2_graph_selected_connections: std::collections::HashSet<usize>,
    pub lv2_graph_selected_plugin: Option<usize>,
    pub lv2_graph_plugin_positions: HashMap<usize, Point>,
    pub lv2_graph_connecting: Option<Lv2Connecting>,
    pub lv2_graph_moving_plugin: Option<MovingPlugin>,
    pub lv2_graph_last_plugin_click: Option<(usize, Instant)>,
    pub connections_last_track_click: Option<(String, Instant)>,
}

impl Default for StateData {
    fn default() -> Self {
        let mut hw: Vec<String> = read_dir("/dev")
            .map(|rd| {
                rd.filter_map(Result::ok)
                    .map(|e| e.path())
                    .filter_map(|path| {
                        let name = path.file_name()?.to_str()?;
                        if name.starts_with("dsp") {
                            Some(path.to_string_lossy().into_owned())
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_else(|_| vec![]);
        hw.sort();
        hw.push("jack".to_string());
        hw.sort();
        hw.dedup();
        Self {
            shift: false,
            ctrl: false,
            tracks: vec![],
            connections: vec![],
            selected: HashSet::new(),
            selected_clips: HashSet::new(),
            message: "Thank you for using Maolan!".to_string(),
            resizing: None,
            connecting: None,
            moving_track: None,
            hovering: None,
            connection_view_selection: ConnectionViewSelection::None,
            cursor: Point::new(0.0, 0.0),
            mixer_height: Length::Fixed(300.0),
            tracks_width: Length::Fixed(200.0),
            view: View::Workspace,
            pending_track_positions: HashMap::new(),
            pending_track_heights: HashMap::new(),
            hovered_track_resize_handle: None,
            hw_loaded: false,
            available_hw: hw,
            selected_hw: None,
            opened_midi_in_hw: vec![],
            opened_midi_out_hw: vec![],
            midi_hw_labels: HashMap::new(),
            midi_hw_in_positions: HashMap::new(),
            midi_hw_out_positions: HashMap::new(),
            hw_in: None,
            hw_out: None,
            hw_out_level: 0.0,
            hw_out_balance: 0.0,
            hw_out_muted: false,
            hw_out_meter_db: vec![],
            lv2_plugins: vec![],
            lv2_graph_track: None,
            lv2_graph_plugins: vec![],
            lv2_graph_connections: vec![],
            lv2_graphs_by_track: HashMap::new(),
            lv2_graph_selected_connections: HashSet::new(),
            lv2_graph_selected_plugin: None,
            lv2_graph_plugin_positions: HashMap::new(),
            lv2_graph_connecting: None,
            lv2_graph_moving_plugin: None,
            lv2_graph_last_plugin_click: None,
            connections_last_track_click: None,
        }
    }
}

pub type State = Arc<RwLock<StateData>>;
