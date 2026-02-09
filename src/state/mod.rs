mod clip;
mod track;

pub use clip::{AudioClip, MIDIClip};
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
pub enum View {
    Workspace,
    Connections,
}

#[derive(Debug)]
pub struct StateData {
    pub shift: bool,
    pub ctrl: bool,
    pub tracks: Vec<Track>,
    pub selected: HashSet<String>,
    pub message: String,
    pub resizing: Option<Resizing>,
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
            selected: HashSet::new(),
            message: "Thank you for using Maolan!".to_string(),
            resizing: None,
            cursor: Point::new(0.0, 0.0),
            mixer_height: Length::Shrink,
            tracks_width: Length::Fixed(200.0),
            view: View::Workspace,
        }
    }
}

pub type State = Arc<RwLock<StateData>>;
