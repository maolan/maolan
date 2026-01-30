mod clip;
mod track;

pub use clip::Clip;
use std::sync::Arc;
use tokio::sync::RwLock;
pub use track::Track;

#[derive(Default, Debug)]
pub struct StateData {
    pub shift: bool,
    pub ctrl: bool,
    pub tracks: Vec<Track>,
    pub selected: Vec<String>,
}

pub type State = Arc<RwLock<StateData>>;
