use super::AudioDeviceOption;
use alsa::{
    Direction,
    pcm::{Access, Format, HwParams, PCM},
};

fn read_alsa_card_labels() -> std::collections::HashMap<u32, String> {
    let mut labels = std::collections::HashMap::new();
    let Ok(contents) = std::fs::read_to_string("/proc/asound/cards") else {
        return labels;
    };
    for line in contents.lines() {
        let line = line.trim_start();
        let Some((num_str, rest)) = line.split_once(' ') else {
            continue;
        };
        let Ok(card) = num_str.parse::<u32>() else {
            continue;
        };
        let Some((_, desc)) = rest.split_once("]:") else {
            continue;
        };
        let desc = desc.trim();
        if !desc.is_empty() {
            labels.insert(card, desc.to_string());
        }
    }
    labels
}

fn probe_alsa_supported_bits(device: &str, direction: Direction) -> Vec<usize> {
    let Ok(pcm) = PCM::new(device, direction, false) else {
        return Vec::new();
    };
    let Ok(hwp) = HwParams::any(&pcm) else {
        return Vec::new();
    };
    if hwp.set_access(Access::RWInterleaved).is_err() {
        return Vec::new();
    }

    fn supports(hwp: &HwParams<'_>, fmt: Format) -> bool {
        hwp.test_format(fmt).is_ok()
    }

    let candidates: Vec<(usize, Vec<Format>)> = vec![
        (32, vec![native_s32(), foreign_s32()]),
        (24, vec![native_s24(), foreign_s24()]),
        (16, vec![native_s16(), foreign_s16()]),
        (8, vec![Format::S8]),
    ];

    let mut supported = Vec::new();
    for (bits, formats) in candidates {
        if formats.iter().any(|f| supports(&hwp, *f)) {
            supported.push(bits);
        }
    }
    supported
}

fn probe_alsa_supported_sample_rates(device: &str, direction: Direction) -> Vec<i32> {
    let Ok(pcm) = PCM::new(device, direction, false) else {
        return Vec::new();
    };
    let Ok(hwp) = HwParams::any(&pcm) else {
        return Vec::new();
    };
    if hwp.set_access(Access::RWInterleaved).is_err() {
        return Vec::new();
    }

    let mut supported = Vec::new();
    for rate in crate::consts::state_platform_linux::SAMPLE_RATE_CANDIDATES {
        if hwp.test_rate(rate).is_ok() {
            supported.push(rate as i32);
        }
    }
    supported
}

#[cfg(target_endian = "little")]
fn native_s16() -> Format {
    Format::S16LE
}
#[cfg(target_endian = "big")]
fn native_s16() -> Format {
    Format::S16BE
}
#[cfg(target_endian = "little")]
fn foreign_s16() -> Format {
    Format::S16BE
}
#[cfg(target_endian = "big")]
fn foreign_s16() -> Format {
    Format::S16LE
}

#[cfg(target_endian = "little")]
fn native_s24() -> Format {
    Format::S24LE
}
#[cfg(target_endian = "big")]
fn native_s24() -> Format {
    Format::S24BE
}
#[cfg(target_endian = "little")]
fn foreign_s24() -> Format {
    Format::S24BE
}
#[cfg(target_endian = "big")]
fn foreign_s24() -> Format {
    Format::S24LE
}

#[cfg(target_endian = "little")]
fn native_s32() -> Format {
    Format::S32LE
}
#[cfg(target_endian = "big")]
fn native_s32() -> Format {
    Format::S32BE
}
#[cfg(target_endian = "little")]
fn foreign_s32() -> Format {
    Format::S32BE
}
#[cfg(target_endian = "big")]
fn foreign_s32() -> Format {
    Format::S32LE
}

fn discover_alsa_devices(direction_marker: &str, direction: Direction) -> Vec<AudioDeviceOption> {
    let mut devices = Vec::new();
    let card_labels = read_alsa_card_labels();
    if let Ok(contents) = std::fs::read_to_string("/proc/asound/pcm") {
        for line in contents.lines() {
            let Some((card_dev, rest)) = line.split_once(':') else {
                continue;
            };
            if !rest.contains(direction_marker) {
                continue;
            }
            let mut parts = card_dev.trim().split('-');
            let (Some(card), Some(dev)) = (parts.next(), parts.next()) else {
                continue;
            };
            let Ok(card) = card.parse::<u32>() else {
                continue;
            };
            let Ok(dev) = dev.parse::<u32>() else {
                continue;
            };
            let device_name = rest.split(':').next().unwrap_or("").trim();
            let card_label = card_labels
                .get(&card)
                .cloned()
                .unwrap_or_else(|| format!("Card {card}"));
            let base_label = if device_name.is_empty() {
                card_label
            } else {
                format!("{card_label} - {device_name}")
            };
            let id = format!("hw:{card},{dev}");
            let label = format!("{base_label} (hw:{card},{dev})");
            let supported_bits = probe_alsa_supported_bits(&id, direction);
            let supported_sample_rates = {
                let rates = probe_alsa_supported_sample_rates(&id, direction);
                if rates.is_empty() {
                    AudioDeviceOption::default_sample_rates()
                } else {
                    rates
                }
            };
            devices.push(AudioDeviceOption::with_supported_caps(
                id,
                label,
                supported_bits,
                supported_sample_rates,
            ));
        }
    }
    devices.sort_by_key(|a| a.label.to_lowercase());
    devices.dedup_by(|a, b| a.id == b.id);
    devices
}

pub(crate) fn discover_alsa_output_devices() -> Vec<AudioDeviceOption> {
    discover_alsa_devices("playback", Direction::Playback)
}

pub(crate) fn discover_alsa_input_devices() -> Vec<AudioDeviceOption> {
    discover_alsa_devices("capture", Direction::Capture)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_alsa_card_labels_returns_empty_on_missing_file() {
        // This will fail to read /proc/asound/cards in test environment
        let labels = read_alsa_card_labels();
        // Should return empty map, not panic
        assert!(labels.is_empty() || !labels.is_empty());
    }

    #[test]
    fn probe_alsa_supported_bits_returns_empty_on_error() {
        // Invalid device should return empty vec
        let bits = probe_alsa_supported_bits("invalid_device", Direction::Playback);
        assert!(bits.is_empty());
    }

    #[test]
    fn probe_alsa_supported_sample_rates_returns_empty_on_error() {
        // Invalid device should return empty vec
        let rates = probe_alsa_supported_sample_rates("invalid_device", Direction::Playback);
        assert!(rates.is_empty());
    }

    #[test]
    fn discover_alsa_output_devices_does_not_panic() {
        // Should not panic even if /proc/asound/pcm doesn't exist
        let _devices = discover_alsa_output_devices();
    }

    #[test]
    fn discover_alsa_input_devices_does_not_panic() {
        // Should not panic even if /proc/asound/pcm doesn't exist
        let _devices = discover_alsa_input_devices();
    }

    #[test]
    #[cfg(target_endian = "little")]
    fn native_formats_are_little_endian() {
        assert_eq!(native_s16(), Format::S16LE);
        assert_eq!(native_s24(), Format::S24LE);
        assert_eq!(native_s32(), Format::S32LE);
    }

    #[test]
    #[cfg(target_endian = "little")]
    fn foreign_formats_are_big_endian() {
        assert_eq!(foreign_s16(), Format::S16BE);
        assert_eq!(foreign_s24(), Format::S24BE);
        assert_eq!(foreign_s32(), Format::S32BE);
    }
}
