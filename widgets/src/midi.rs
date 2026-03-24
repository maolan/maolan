use std::fmt;

pub const MIDI_CHANNELS: usize = 16;
pub const KEYS_SCROLL_ID: &str = "piano.keys.scroll";
pub const NOTES_SCROLL_ID: &str = "piano.notes.scroll";
pub const CTRL_SCROLL_ID: &str = "piano.ctrl.scroll";
pub const SYSEX_SCROLL_ID: &str = "piano.sysex.scroll";

pub const KEYBOARD_WIDTH: f32 = 128.0;
pub const RIGHT_SCROLL_GUTTER_WIDTH: f32 = 16.0;
pub const TOOLS_STRIP_WIDTH: f32 = 248.0;
pub const MAIN_SPLIT_SPACING: f32 = 3.0;
pub const H_ZOOM_MIN: f32 = 1.0;
pub const H_ZOOM_MAX: f32 = 127.0;
pub const MIDI_NOTE_COUNT: usize = 128;
pub const OCTAVES: usize = 11;
pub const WHITE_KEYS_PER_OCTAVE: usize = 7;
pub const NOTES_PER_OCTAVE: usize = 12;
pub const PITCH_MAX: u8 = (MIDI_NOTE_COUNT as u8) - 1;
pub const WHITE_KEY_HEIGHT: f32 = 14.0;
pub const MAX_RPN_NRPN_POINTS: usize = 4096;
pub const MIDI_DIN_BYTES_PER_SEC: f64 = 3125.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PianoControllerLane {
    Controller,
    Velocity,
    Rpn,
    Nrpn,
    SysEx,
}

impl fmt::Display for PianoControllerLane {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Controller => write!(f, "Controller"),
            Self::Velocity => write!(f, "Velocity"),
            Self::Rpn => write!(f, "RPN"),
            Self::Nrpn => write!(f, "NRPN"),
            Self::SysEx => write!(f, "SysEx"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PianoRpnKind {
    PitchBendSensitivity,
    FineTuning,
    CoarseTuning,
}

impl fmt::Display for PianoRpnKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PitchBendSensitivity => write!(f, "Pitch Bend Sensitivity"),
            Self::FineTuning => write!(f, "Fine Tuning"),
            Self::CoarseTuning => write!(f, "Coarse Tuning"),
        }
    }
}

pub const PIANO_RPN_KIND_ALL: [PianoRpnKind; 3] = [
    PianoRpnKind::PitchBendSensitivity,
    PianoRpnKind::FineTuning,
    PianoRpnKind::CoarseTuning,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PianoNrpnKind {
    Brightness,
    VibratoRate,
    VibratoDepth,
}

impl fmt::Display for PianoNrpnKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Brightness => write!(f, "Brightness"),
            Self::VibratoRate => write!(f, "Vibrato Rate"),
            Self::VibratoDepth => write!(f, "Vibrato Depth"),
        }
    }
}

pub const PIANO_NRPN_KIND_ALL: [PianoNrpnKind; 3] = [
    PianoNrpnKind::Brightness,
    PianoNrpnKind::VibratoRate,
    PianoNrpnKind::VibratoDepth,
];

#[derive(Debug, Clone)]
pub struct PianoNote {
    pub start_sample: usize,
    pub length_samples: usize,
    pub pitch: u8,
    pub velocity: u8,
    pub channel: u8,
}

#[derive(Debug, Clone)]
pub struct PianoControllerPoint {
    pub sample: usize,
    pub controller: u8,
    pub value: u8,
    pub channel: u8,
}

#[derive(Debug, Clone)]
pub struct PianoSysExPoint {
    pub sample: usize,
    pub data: Vec<u8>,
}
