mod clip;
mod connection;
mod track;

pub use clip::{AudioClip, MIDIClip};
pub use connection::Connection;
use iced::{Length, Point};
use maolan_engine::kind::Kind;
use std::{
    collections::{HashMap, HashSet},
    fs::read_dir,
    sync::Arc,
};
use tokio::sync::RwLock;
pub use track::Track;

pub const HW_IN_ID: &str = "hw:in";
pub const HW_OUT_ID: &str = "hw:out";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClipId {
    pub track_idx: String,
    pub clip_idx: usize,
    pub kind: Kind,
}

#[derive(Debug, Clone)]
pub enum Resizing {
    Clip(Kind, String, usize, bool, f32, f32),
    Mixer,
    Track(String, f32, f32),
    Tracks,
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
}

#[derive(Debug, Clone)]
pub struct HW {
    pub channels: usize,
    pub rate: usize,
    pub input: bool,
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
    pub hw_loaded: bool,
    pub available_hw: Vec<String>,
    pub selected_hw: Option<String>,
    pub hw_in: Option<HW>,
    pub hw_out: Option<HW>,
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
            mixer_height: Length::Shrink,
            tracks_width: Length::Fixed(200.0),
            view: View::Workspace,
            pending_track_positions: HashMap::new(),
            pending_track_heights: HashMap::new(),
            hw_loaded: false,
            available_hw: hw,
            selected_hw: None,
            hw_in: None,
            hw_out: None,
        }
    }
}

pub type State = Arc<RwLock<StateData>>;
