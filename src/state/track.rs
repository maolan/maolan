use super::Clip;
use crate::message::Message;
use maolan_engine::message::{Action, TrackKind};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

fn custom_deserializer<'de, D>(deserializer: D) -> Result<TrackKind, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    match s.as_str() {
        "Audio" => Ok(TrackKind::Audio),
        "MIDI" => Ok(TrackKind::MIDI),
        _ => Err(serde::de::Error::custom(format!(
            "Unknown track type '{}'",
            s
        ))),
    }
}

fn custom_serializer<S>(kind: &TrackKind, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    s.serialize_str(match kind {
        TrackKind::Audio => "Audio",
        TrackKind::MIDI => "MIDI",
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    id: usize,
    pub name: String,
    #[serde(
        deserialize_with = "custom_deserializer",
        serialize_with = "custom_serializer"
    )]
    pub track_kind: TrackKind,
    pub level: f32,
    pub ins: usize,
    pub audio_outs: usize,
    pub midi_outs: usize,
    pub armed: bool,
    pub muted: bool,
    pub soloed: bool,
    pub clips: Vec<Clip>,
    pub height: f32,
}

impl Track {
    pub fn new(
        name: String,
        track_kind: TrackKind,
        level: f32,
        ins: usize,
        audio_outs: usize,
        midi_outs: usize,
    ) -> Self {
        Self {
            id: 0,
            name,
            track_kind,
            level,
            ins,
            audio_outs,
            midi_outs,
            armed: false,
            muted: false,
            soloed: false,
            clips: vec![Clip::new(0, "ime".to_string(), 0.0, 60.0, 0)],
            height: 60.0,
        }
    }

    pub fn add_clip(&mut self, clip: Clip) {
        self.clips.push(clip);
    }

    pub fn update(&mut self, message: Message) {
        if let Message::Response(Ok(a)) = message {
            match a {
                Action::TrackLevel(name, level) => {
                    if name == self.name {
                        self.level = level;
                    }
                }
                Action::TrackIns(name, ins) => {
                    if name == self.name {
                        self.ins = ins;
                    }
                }
                Action::TrackAudioOuts(name, outs) => {
                    if name == self.name {
                        self.audio_outs = outs;
                    }
                }
                Action::TrackMIDIOuts(name, outs) => {
                    if name == self.name {
                        self.midi_outs = outs;
                    }
                }
                Action::TrackToggleArm(name) => {
                    if name == self.name {
                        self.armed = !self.armed;
                    }
                }
                Action::TrackToggleMute(name) => {
                    if name == self.name {
                        self.muted = !self.muted;
                    }
                }
                Action::TrackToggleSolo(name) => {
                    if name == self.name {
                        self.soloed = !self.soloed;
                    }
                }
                _ => {}
            }
        }
    }
}
