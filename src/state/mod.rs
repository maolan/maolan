mod clip;
mod connection;
mod track;

pub use clip::{AudioClip, MIDIClip};
pub use connection::Connection;
use iced::{Length, Point};
use maolan_engine::kind::Kind;
use std::{collections::HashSet, sync::Arc};
use tokio::sync::RwLock;
pub use track::Track;

#[derive(Debug, Clone)]
pub enum Resizing {
    Clip(Kind, usize, usize, bool, f32, f32),
    Mixer,
    Track(usize, f32, f32),
    Tracks,
}

#[derive(Debug, Clone)]
pub struct Connecting {
    pub from_track: usize,
    pub from_port: usize,
    pub kind: Kind,
    pub point: Point,
}

#[derive(Debug, Clone)]
pub struct MovingTrack {
    pub track_idx: usize,
    pub offset_x: f32,
    pub offset_y: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Hovering {
    Port {
        track_idx: usize,
        port_idx: usize,
        is_input: bool,
    },
    Track(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionViewSelection {
    Tracks(HashSet<usize>),
    Connections(HashSet<usize>),
    None,
}

#[derive(Debug, Clone)]
pub enum View {
    Workspace,
    Connections,
}

#[derive(Debug)]
pub struct StateData {
    pub shift: bool,
    pub ctrl: bool,
    pub tracks: Vec<Track>,
    pub connections: Vec<Connection>,
    pub selected: HashSet<String>,
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
}

impl Default for StateData {
    fn default() -> Self {
        Self {
            shift: false,
            ctrl: false,
            tracks: vec![],
            connections: vec![],
            selected: HashSet::new(),
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
        }
    }
}

pub type State = Arc<RwLock<StateData>>;
