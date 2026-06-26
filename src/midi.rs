pub fn standard_cc_name(cc: u8) -> &'static str {
    match cc {
        0 => "Bank Select",
        1 => "Modulation Wheel",
        2 => "Breath Controller",
        4 => "Foot Controller",
        5 => "Portamento Time",
        6 => "Data Entry MSB",
        7 => "Volume",
        8 => "Balance",
        10 => "Pan",
        11 => "Expression",
        12 => "Effect Control 1",
        13 => "Effect Control 2",
        16 => "General Purpose 1",
        17 => "General Purpose 2",
        18 => "General Purpose 3",
        19 => "General Purpose 4",
        32 => "Bank Select LSB",
        33 => "Modulation Wheel LSB",
        34 => "Breath Controller LSB",
        36 => "Foot Controller LSB",
        37 => "Portamento Time LSB",
        38 => "Data Entry LSB",
        39 => "Volume LSB",
        40 => "Balance LSB",
        42 => "Pan LSB",
        43 => "Expression LSB",
        64 => "Sustain",
        65 => "Portamento",
        66 => "Sostenuto",
        67 => "Soft Pedal",
        68 => "Legato Footswitch",
        69 => "Hold 2",
        70 => "Sound Variation",
        71 => "Resonance",
        72 => "Release Time",
        73 => "Attack Time",
        74 => "Brightness",
        75 => "Decay Time",
        76 => "Vibrato Rate",
        77 => "Vibrato Depth",
        78 => "Vibrato Delay",
        80 => "General Purpose 5",
        81 => "General Purpose 6",
        82 => "General Purpose 7",
        83 => "General Purpose 8",
        84 => "Portamento Control",
        91 => "Reverb Send",
        92 => "Tremolo Depth",
        93 => "Chorus Send",
        94 => "Celeste Depth",
        95 => "Phaser Depth",
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
        124 => "Omni Off",
        125 => "Omni On",
        126 => "Mono On",
        127 => "Poly On",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_cc_name_returns_known_names() {
        assert_eq!(standard_cc_name(7), "Volume");
        assert_eq!(standard_cc_name(10), "Pan");
        assert_eq!(standard_cc_name(64), "Sustain");
    }

    #[test]
    fn standard_cc_name_returns_empty_for_unknown() {
        assert_eq!(standard_cc_name(3), "");
        assert_eq!(standard_cc_name(69), "Hold 2");
    }
}
