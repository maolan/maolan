use maolan_engine::kind::Kind;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(remote = "Kind")]
enum KindDef {
    Audio,
    MIDI,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub from_track: String,
    pub from_port: usize,
    pub to_track: String,
    pub to_port: usize,
    #[serde(with = "KindDef")]
    pub kind: Kind,
}
