use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct MidiMappingsGlobalFile {
    pub(super) play_pause: Option<maolan_engine::message::MidiLearnBinding>,
    pub(super) stop: Option<maolan_engine::message::MidiLearnBinding>,
    pub(super) record_toggle: Option<maolan_engine::message::MidiLearnBinding>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct MidiMappingsTrackFile {
    pub(super) volume: Option<maolan_engine::message::MidiLearnBinding>,
    pub(super) balance: Option<maolan_engine::message::MidiLearnBinding>,
    pub(super) mute: Option<maolan_engine::message::MidiLearnBinding>,
    pub(super) solo: Option<maolan_engine::message::MidiLearnBinding>,
    pub(super) arm: Option<maolan_engine::message::MidiLearnBinding>,
    pub(super) input_monitor: Option<maolan_engine::message::MidiLearnBinding>,
    pub(super) disk_monitor: Option<maolan_engine::message::MidiLearnBinding>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct MidiMappingsFile {
    pub(super) global: MidiMappingsGlobalFile,
    pub(super) tracks: HashMap<String, MidiMappingsTrackFile>,
}

#[derive(Clone)]
pub(super) struct AutomationTrackView {
    pub(super) name: String,
    pub(super) automation_mode: TrackAutomationMode,
    pub(super) automation_lanes: Vec<TrackAutomationLane>,
    pub(super) frozen: bool,
}
