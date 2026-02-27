use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortBinding {
    AudioInput {
        bus_index: usize,
        channel_index: usize,
    },
    AudioOutput {
        bus_index: usize,
        channel_index: usize,
    },
    Parameter {
        param_id: u32,
        index: usize, // index in scalar_values vec
    },
    EventInput {
        bus_index: usize,
    },
    EventOutput {
        bus_index: usize,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusInfo {
    pub index: usize,
    pub name: String,
    pub channel_count: usize,
    pub is_active: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParameterInfo {
    pub id: u32, // VST3 ParamID
    pub title: String,
    pub short_title: String,
    pub units: String,
    pub step_count: i32, // 0 = continuous, >0 = discrete
    pub default_value: f64,
    pub flags: i32, // ParameterFlags (read-only, etc.)
}
