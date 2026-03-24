use crate::midi::{
    MIDI_CHANNELS, PIANO_NRPN_KIND_ALL, PIANO_RPN_KIND_ALL, PianoControllerLane,
    PianoControllerPoint, PianoNrpnKind, PianoRpnKind,
};
use iced::Color;
use std::collections::HashSet;

pub fn controller_color(controller: u8, channel: u8) -> Color {
    let h = (controller as f32 / 127.0).clamp(0.0, 1.0);
    let c = (channel as f32 / 15.0).clamp(0.0, 1.0);
    Color {
        r: 0.3 + 0.5 * h,
        g: 0.85 - 0.45 * h,
        b: 0.25 + 0.45 * (1.0 - c),
        a: 0.85,
    }
}

pub fn controller_lane_line_count(lane: PianoControllerLane) -> usize {
    match lane {
        PianoControllerLane::Controller => 128,
        PianoControllerLane::Velocity => 128,
        PianoControllerLane::Rpn => PIANO_RPN_KIND_ALL.len(),
        PianoControllerLane::Nrpn => PIANO_NRPN_KIND_ALL.len(),
        PianoControllerLane::SysEx => 1,
    }
}

pub fn controller_row_for_lane(lane: PianoControllerLane, controller: u8) -> Option<usize> {
    match lane {
        PianoControllerLane::Controller => Some(usize::from(127_u8.saturating_sub(controller))),
        PianoControllerLane::Velocity => None,
        PianoControllerLane::Rpn => match controller {
            101 => Some(0),
            100 => Some(1),
            6 | 38 | 96 | 97 => Some(2),
            _ => None,
        },
        PianoControllerLane::Nrpn => match controller {
            99 => Some(0),
            98 => Some(1),
            6 | 38 | 96 | 97 => Some(2),
            _ => None,
        },
        PianoControllerLane::SysEx => None,
    }
}

pub fn rpn_param(kind: PianoRpnKind) -> (u8, u8) {
    match kind {
        PianoRpnKind::PitchBendSensitivity => (0, 0),
        PianoRpnKind::FineTuning => (0, 1),
        PianoRpnKind::CoarseTuning => (0, 2),
    }
}

pub fn nrpn_param(kind: PianoNrpnKind) -> (u8, u8) {
    match kind {
        PianoNrpnKind::Brightness => (1, 8),
        PianoNrpnKind::VibratoRate => (1, 9),
        PianoNrpnKind::VibratoDepth => (1, 10),
    }
}

pub fn rpn_row_for_param(msb: u8, lsb: u8) -> Option<usize> {
    PIANO_RPN_KIND_ALL
        .iter()
        .position(|kind| rpn_param(*kind) == (msb, lsb))
}

pub fn nrpn_row_for_param(msb: u8, lsb: u8) -> Option<usize> {
    PIANO_NRPN_KIND_ALL
        .iter()
        .position(|kind| nrpn_param(*kind) == (msb, lsb))
}

pub fn sysex_preview(data: &[u8]) -> String {
    let mut parts = data
        .iter()
        .take(6)
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>();
    if data.len() > 6 {
        parts.push("...".to_string());
    }
    parts.join(" ")
}

pub fn lane_controller_events(
    lane: PianoControllerLane,
    controllers: &[PianoControllerPoint],
) -> Vec<(usize, usize)> {
    match lane {
        PianoControllerLane::Controller => controllers
            .iter()
            .enumerate()
            .filter_map(|(idx, ctrl)| {
                controller_row_for_lane(lane, ctrl.controller).map(|row| (idx, row))
            })
            .collect(),
        PianoControllerLane::Velocity => vec![],
        PianoControllerLane::SysEx => vec![],
        PianoControllerLane::Rpn => {
            let mut ordered: Vec<usize> = (0..controllers.len()).collect();
            ordered.sort_unstable_by_key(|idx| (controllers[*idx].sample, *idx));
            let mut current_msb: [Option<u8>; MIDI_CHANNELS] = [None; MIDI_CHANNELS];
            let mut current_lsb: [Option<u8>; MIDI_CHANNELS] = [None; MIDI_CHANNELS];
            let mut out = Vec::new();
            for idx in ordered {
                let ctrl = &controllers[idx];
                let channel = usize::from(ctrl.channel.min((MIDI_CHANNELS - 1) as u8));
                match ctrl.controller {
                    101 => current_msb[channel] = Some(ctrl.value),
                    100 => current_lsb[channel] = Some(ctrl.value),
                    6 => {
                        if let (Some(msb), Some(lsb)) = (current_msb[channel], current_lsb[channel])
                            && let Some(row) = rpn_row_for_param(msb, lsb)
                        {
                            out.push((idx, row));
                        }
                    }
                    _ => {}
                }
            }
            out
        }
        PianoControllerLane::Nrpn => {
            let mut ordered: Vec<usize> = (0..controllers.len()).collect();
            ordered.sort_unstable_by_key(|idx| (controllers[*idx].sample, *idx));
            let mut current_msb: [Option<u8>; MIDI_CHANNELS] = [None; MIDI_CHANNELS];
            let mut current_lsb: [Option<u8>; MIDI_CHANNELS] = [None; MIDI_CHANNELS];
            let mut out = Vec::new();
            for idx in ordered {
                let ctrl = &controllers[idx];
                let channel = usize::from(ctrl.channel.min((MIDI_CHANNELS - 1) as u8));
                match ctrl.controller {
                    99 => current_msb[channel] = Some(ctrl.value),
                    98 => current_lsb[channel] = Some(ctrl.value),
                    6 => {
                        if let (Some(msb), Some(lsb)) = (current_msb[channel], current_lsb[channel])
                            && let Some(row) = nrpn_row_for_param(msb, lsb)
                        {
                            out.push((idx, row));
                        }
                    }
                    _ => {}
                }
            }
            out
        }
    }
}

pub fn populated_controller_ccs(controllers: &[PianoControllerPoint]) -> HashSet<u8> {
    controllers.iter().map(|ctrl| ctrl.controller).collect()
}

pub fn populated_controller_rows(
    lane: PianoControllerLane,
    controllers: &[PianoControllerPoint],
) -> HashSet<usize> {
    lane_controller_events(lane, controllers)
        .into_iter()
        .map(|(_, row)| row)
        .collect()
}

pub fn cc_name(cc: u8) -> &'static str {
    match cc {
        0 => "Bank Select",
        1 => "Modulation Wheel",
        2 => "Breath Controller",
        4 => "Foot Controller",
        5 => "Portamento Time",
        6 => "Data Entry MSB",
        7 => "Channel Volume",
        8 => "Balance",
        10 => "Pan",
        11 => "Expression Controller",
        12 => "Effect Control 1",
        13 => "Effect Control 2",
        16 => "General Purpose Controller 1",
        17 => "General Purpose Controller 2",
        18 => "General Purpose Controller 3",
        19 => "General Purpose Controller 4",
        32 => "Bank Select LSB",
        33 => "Modulation Wheel LSB",
        34 => "Breath Controller LSB",
        36 => "Foot Controller LSB",
        37 => "Portamento Time LSB",
        38 => "Data Entry LSB",
        39 => "Channel Volume LSB",
        40 => "Balance LSB",
        42 => "Pan LSB",
        43 => "Expression Controller LSB",
        44 => "Effect Control 1 LSB",
        45 => "Effect Control 2 LSB",
        48 => "General Purpose Controller 1 LSB",
        49 => "General Purpose Controller 2 LSB",
        50 => "General Purpose Controller 3 LSB",
        51 => "General Purpose Controller 4 LSB",
        64 => "Sustain Pedal",
        65 => "Portamento",
        66 => "Sostenuto",
        67 => "Soft Pedal",
        68 => "Legato Footswitch",
        69 => "Hold 2",
        70 => "Sound Controller 1",
        71 => "Sound Controller 2",
        72 => "Sound Controller 3",
        73 => "Sound Controller 4",
        74 => "Sound Controller 5",
        75 => "Sound Controller 6",
        76 => "Sound Controller 7",
        77 => "Sound Controller 8",
        78 => "Sound Controller 9",
        79 => "Sound Controller 10",
        80 => "General Purpose Controller 5",
        81 => "General Purpose Controller 6",
        82 => "General Purpose Controller 7",
        83 => "General Purpose Controller 8",
        84 => "Portamento Control",
        91 => "Effects 1 Depth",
        92 => "Effects 2 Depth",
        93 => "Effects 3 Depth",
        94 => "Effects 4 Depth",
        95 => "Effects 5 Depth",
        96 => "Data Increment",
        97 => "Data Decrement",
        98 => "NRPN LSB",
        99 => "NRPN MSB",
        100 => "RPN LSB",
        101 => "RPN MSB",
        120 => "All Sound Off",
        121 => "Reset All Controllers",
        122 => "Local Control",
        123 => "All Notes Off",
        124 => "Omni Mode Off",
        125 => "Omni Mode On",
        126 => "Mono Mode On",
        127 => "Poly Mode On",
        _ => "Undefined",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControllerKindOption(pub u8);

impl std::fmt::Display for ControllerKindOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CC{:03} {}", self.0, cc_name(self.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sysex_preview_truncates_long_messages() {
        assert_eq!(
            sysex_preview(&[0xF0, 0x7E, 0x7F, 0x09, 0x01, 0xF7, 0x00]),
            "F0 7E 7F 09 01 F7 ..."
        );
    }

    #[test]
    fn populated_rows_follow_rpn_mapping() {
        let controllers = vec![
            PianoControllerPoint {
                sample: 0,
                controller: 101,
                value: 0,
                channel: 0,
            },
            PianoControllerPoint {
                sample: 0,
                controller: 100,
                value: 1,
                channel: 0,
            },
            PianoControllerPoint {
                sample: 1,
                controller: 6,
                value: 64,
                channel: 0,
            },
        ];

        let rows = populated_controller_rows(PianoControllerLane::Rpn, &controllers);
        assert!(rows.contains(&1));
    }
}
