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
    sync::Arc,
    time::Instant,
};
#[cfg(target_os = "freebsd")]
use std::fs::read_dir;
use tokio::sync::RwLock;
pub use track::Track;

pub const HW_IN_ID: &str = "hw:in";
pub const HW_OUT_ID: &str = "hw:out";
pub const MIDI_HW_IN_ID: &str = "midi:hw:in";
pub const MIDI_HW_OUT_ID: &str = "midi:hw:out";

#[cfg(target_os = "linux")]
#[derive(Clone, Debug)]
pub struct AudioDeviceOption {
    pub id: String,
    pub label: String,
}

#[cfg(target_os = "linux")]
impl AudioDeviceOption {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
        }
    }
}

#[cfg(target_os = "linux")]
impl std::fmt::Display for AudioDeviceOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

#[cfg(target_os = "linux")]
impl PartialEq for AudioDeviceOption {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

#[cfg(target_os = "linux")]
impl Eq for AudioDeviceOption {}

#[cfg(target_os = "linux")]
impl std::hash::Hash for AudioDeviceOption {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

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
    #[cfg(target_os = "linux")]
    pub available_hw: Vec<AudioDeviceOption>,
    #[cfg(target_os = "linux")]
    pub selected_hw: Option<AudioDeviceOption>,
    #[cfg(not(target_os = "linux"))]
    pub available_hw: Vec<String>,
    #[cfg(not(target_os = "linux"))]
    pub selected_hw: Option<String>,
    pub oss_exclusive: bool,
    pub oss_period_frames: usize,
    pub oss_nperiods: usize,
    pub oss_sync_mode: bool,
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
        #[cfg(target_os = "freebsd")]
        let hw: Vec<String> = {
            let mut devices: Vec<String> = read_dir("/dev")
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
            devices.push("jack".to_string());
            devices.sort_by_key(|a| a.to_lowercase());
            devices.dedup();
            devices
        };
        #[cfg(target_os = "linux")]
        let hw: Vec<AudioDeviceOption> = {
            let mut devices = discover_alsa_devices();
            devices.push(AudioDeviceOption::new("jack", "JACK"));
            devices.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
            devices.dedup_by(|a, b| a.id == b.id);
            devices
        };
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
            oss_exclusive: false,
            oss_period_frames: 1024,
            oss_nperiods: 1,
            oss_sync_mode: false,
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

#[cfg(target_os = "linux")]
fn read_alsa_card_labels() -> std::collections::HashMap<u32, String> {
    let mut labels = std::collections::HashMap::new();
    let Ok(contents) = std::fs::read_to_string("/proc/asound/cards") else {
        return labels;
    };
    for line in contents.lines() {
        let line = line.trim_start();
        let Some((num_str, rest)) = line.split_once(' ') else {
            continue;
        };
        let Ok(card) = num_str.parse::<u32>() else {
            continue;
        };
        let Some((_, desc)) = rest.split_once("]:") else {
            continue;
        };
        let desc = desc.trim();
        if !desc.is_empty() {
            labels.insert(card, desc.to_string());
        }
    }
    labels
}

#[cfg(target_os = "linux")]
fn discover_alsa_devices() -> Vec<AudioDeviceOption> {
    let mut devices = Vec::new();
    let card_labels = read_alsa_card_labels();
    if let Ok(contents) = std::fs::read_to_string("/proc/asound/pcm") {
        for line in contents.lines() {
            let Some((card_dev, rest)) = line.split_once(':') else {
                continue;
            };
            if !rest.contains("playback") && !rest.contains("capture") {
                continue;
            }
            let mut parts = card_dev.trim().split('-');
            let (Some(card), Some(dev)) = (parts.next(), parts.next()) else {
                continue;
            };
            let Ok(card) = card.parse::<u32>() else {
                continue;
            };
            let Ok(dev) = dev.parse::<u32>() else {
                continue;
            };
            let device_name = rest.split(':').next().unwrap_or("").trim();
            let card_label = card_labels
                .get(&card)
                .cloned()
                .unwrap_or_else(|| format!("Card {card}"));
            let base_label = if device_name.is_empty() {
                card_label
            } else {
                format!("{card_label} - {device_name}")
            };
            devices.push(AudioDeviceOption::new(
                format!("hw:{card},{dev}"),
                format!("{base_label} (hw:{card},{dev})"),
            ));
        }
    }
    devices.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
    devices.dedup_by(|a, b| a.id == b.id);
    devices
}
