mod clip;
mod track;

pub use clip::Clip;
use std::{collections::HashSet, sync::Arc};
use tokio::sync::RwLock;
pub use track::Track;

#[derive(Default, Debug)]
pub struct StateData {
    pub shift: bool,
    pub ctrl: bool,
    pub tracks: Vec<Track>,
    pub selected: HashSet<String>,
    pub message: String,
    pub resizing_track: Option<(String, f32, f32)>,
    pub resizing_clip: Option<(String, String, bool, f32, f32)>,
    pub dragging_clip: Option<(String, String, f32, f32)>, // (track_name, clip_name, initial_start, initial_mouse_x)
    pub cursor_position_y: f32,
    pub cursor_position_x: f32,
}

pub type State = Arc<RwLock<StateData>>;
