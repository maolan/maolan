//! View-facing message constructors.
//!
//! This is the boundary between view code (`workspace/`, `session_view/`) and
//! the engine protocol: views emit these semantic constructors instead of
//! building `maolan_engine::message::Action` values themselves, so view
//! modules do not need to import engine types.

use maolan_engine::message::{Action, TrackMidiLearnTarget};

use crate::message::Message;

pub(crate) fn track_balance(track_name: String, value: f32) -> Message {
    Message::Request(Action::TrackBalance(track_name, value))
}

pub(crate) fn track_level(track_name: String, value: f32) -> Message {
    Message::Request(Action::TrackLevel(track_name, value))
}

pub(crate) fn track_toggle_mute(track_name: String) -> Message {
    Message::Request(Action::TrackToggleMute(track_name))
}

pub(crate) fn track_toggle_solo(track_name: String) -> Message {
    Message::Request(Action::TrackToggleSolo(track_name))
}

pub(crate) fn track_toggle_arm(track_name: String) -> Message {
    Message::Request(Action::TrackToggleArm(track_name))
}

pub(crate) fn transport_position(sample: usize) -> Message {
    Message::Request(Action::TransportPosition(sample))
}

#[cfg(test)]
pub(crate) fn transport_position_sample(message: &Message) -> Option<usize> {
    match message {
        Message::Request(Action::TransportPosition(sample)) => Some(*sample),
        _ => None,
    }
}

pub(crate) fn track_midi_cc(track_name: String, channel: u8, cc: u8, value: u8) -> Message {
    Message::Request(Action::TrackMidiCc {
        track_name,
        channel,
        cc,
        value,
    })
}

pub(crate) fn track_set_clap_parameter(
    track_name: String,
    instance_id: usize,
    param_id: u32,
    value: f64,
) -> Message {
    Message::Request(Action::TrackSetClapParameter {
        track_name,
        instance_id,
        param_id,
        value,
    })
}

pub(crate) fn track_set_vst3_parameter(
    track_name: String,
    instance_id: usize,
    param_id: u32,
    value: f32,
) -> Message {
    Message::Request(Action::TrackSetVst3Parameter {
        track_name,
        instance_id,
        param_id,
        value,
    })
}

#[cfg(unix)]
pub(crate) fn track_set_lv2_control_value(
    track_name: String,
    instance_id: usize,
    index: u32,
    value: f32,
) -> Message {
    Message::Request(Action::TrackSetLv2ControlValue {
        track_name,
        instance_id,
        index,
        value,
    })
}

pub(crate) fn clip_kind_key(kind: crate::state::Kind) -> u8 {
    match kind {
        crate::state::Kind::Audio => 0,
        crate::state::Kind::MIDI => 1,
    }
}

pub(crate) fn track_toggle_master(track_name: String) -> Message {
    Message::Request(Action::TrackToggleMaster(track_name))
}

pub(crate) fn track_toggle_phase(track_name: String) -> Message {
    Message::Request(Action::TrackTogglePhase(track_name))
}

pub(crate) fn track_toggle_input_monitor(track_name: String) -> Message {
    Message::TrackToggleInputMonitor { track_name }
}

pub(crate) fn track_toggle_disk_monitor(track_name: String) -> Message {
    Message::TrackToggleDiskMonitor { track_name }
}

pub(crate) fn track_toggle_midi_input_monitor(track_name: String) -> Message {
    Message::TrackToggleMidiInputMonitor { track_name }
}

pub(crate) fn track_toggle_midi_disk_monitor(track_name: String) -> Message {
    Message::TrackToggleMidiDiskMonitor { track_name }
}

macro_rules! midi_learn_target_fns {
    ($($name:ident => $variant:ident),* $(,)?) => {
        $(
            pub(crate) fn $name(track_name: String) -> Message {
                Message::TrackMidiLearnArm {
                    track_name,
                    target: TrackMidiLearnTarget::$variant,
                }
            }
        )*
    };
}

macro_rules! midi_learn_clear_fns {
    ($($name:ident => $variant:ident),* $(,)?) => {
        $(
            pub(crate) fn $name(track_name: String) -> Message {
                Message::TrackMidiLearnClear {
                    track_name,
                    target: TrackMidiLearnTarget::$variant,
                }
            }
        )*
    };
}

midi_learn_target_fns! {
    track_midi_learn_volume => Volume,
    track_midi_learn_balance => Balance,
    track_midi_learn_mute => Mute,
    track_midi_learn_solo => Solo,
    track_midi_learn_arm => Arm,
    track_midi_learn_input_monitor => InputMonitor,
    track_midi_learn_disk_monitor => DiskMonitor,
}

midi_learn_clear_fns! {
    track_midi_learn_clear_volume => Volume,
    track_midi_learn_clear_balance => Balance,
    track_midi_learn_clear_mute => Mute,
    track_midi_learn_clear_solo => Solo,
    track_midi_learn_clear_arm => Arm,
    track_midi_learn_clear_input_monitor => InputMonitor,
    track_midi_learn_clear_disk_monitor => DiskMonitor,
}
