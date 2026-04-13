#[cfg(target_os = "windows")]
use cpal::traits::{DeviceTrait, HostTrait};
#[cfg(target_os = "windows")]
use cpal::{SampleFormat, SupportedStreamConfigRange};

#[cfg(target_os = "windows")]
enum WindowsDeviceDirection {
    Input,
    Output,
}

#[cfg(target_os = "windows")]
fn discover_windows_devices(direction: WindowsDeviceDirection) -> Vec<String> {
    let mut out = vec!["wasapi:default".to_string()];
    let Ok(host) = cpal::host_from_id(cpal::HostId::Wasapi) else {
        return out;
    };
    let devices = match direction {
        WindowsDeviceDirection::Input => host.input_devices(),
        WindowsDeviceDirection::Output => host.output_devices(),
    };
    let Ok(devices) = devices else {
        return out;
    };
    for dev in devices {
        if let Ok(name) = dev.name() {
            out.push(format!("wasapi:{name}"));
        }
    }
    out.sort();
    out.dedup();
    out
}

#[cfg(target_os = "windows")]
pub(crate) fn discover_windows_audio_devices() -> Vec<String> {
    discover_windows_devices(WindowsDeviceDirection::Output)
}

#[cfg(target_os = "windows")]
pub(crate) fn discover_windows_input_devices() -> Vec<String> {
    discover_windows_devices(WindowsDeviceDirection::Input)
}

#[cfg(target_os = "windows")]
pub(crate) fn discover_windows_output_sample_rates(device_id: &str) -> Vec<i32> {
    let fallback_sample_rates = vec![
        8_000, 11_025, 16_000, 22_050, 32_000, 44_100, 48_000, 88_200, 96_000, 176_400, 192_000,
        384_000,
    ];
    let requested_name = if let Some(name) = device_id.strip_prefix("wasapi:") {
        name
    } else {
        return fallback_sample_rates;
    };
    let Ok(host) = cpal::host_from_id(cpal::HostId::Wasapi) else {
        return fallback_sample_rates;
    };
    let device = if requested_name == "default" {
        host.default_output_device()
    } else {
        let Ok(mut devices) = host.output_devices() else {
            return fallback_sample_rates;
        };
        devices.find(|dev| dev.name().is_ok_and(|name| name == requested_name))
    };
    let Some(device) = device else {
        return fallback_sample_rates;
    };
    let Ok(configs) = device.supported_output_configs() else {
        return fallback_sample_rates;
    };
    let mut rates = Vec::new();
    for cfg in configs {
        let min_hz = cfg.min_sample_rate().0 as i32;
        let max_hz = cfg.max_sample_rate().0 as i32;
        rates.extend(
            fallback_sample_rates
                .iter()
                .copied()
                .filter(|rate| *rate >= min_hz && *rate <= max_hz),
        );
    }
    rates.sort_unstable();
    rates.dedup();
    if rates.is_empty() {
        fallback_sample_rates
    } else {
        rates
    }
}

#[cfg(target_os = "windows")]
fn bit_depth_from_sample_format(format: SampleFormat) -> Option<usize> {
    match format {
        SampleFormat::I8 | SampleFormat::U8 => Some(8),
        SampleFormat::I16 | SampleFormat::U16 => Some(16),
        SampleFormat::I32
        | SampleFormat::U32
        | SampleFormat::F32
        | SampleFormat::I64
        | SampleFormat::U64
        | SampleFormat::F64 => Some(32),
        _ => None,
    }
}

#[cfg(target_os = "windows")]
fn collect_supported_output_bit_depths(
    configs: impl Iterator<Item = SupportedStreamConfigRange>,
) -> Vec<usize> {
    let mut bits = Vec::new();
    for cfg in configs {
        if let Some(depth) = bit_depth_from_sample_format(cfg.sample_format()) {
            bits.push(depth);
        }
    }
    bits.sort_by(|a, b| b.cmp(a));
    bits.dedup();
    bits
}

#[cfg(target_os = "windows")]
pub(crate) fn discover_windows_output_bit_depths(device_id: &str) -> Vec<usize> {
    let fallback_bits = vec![32, 24, 16, 8];
    let requested_name = if let Some(name) = device_id.strip_prefix("wasapi:") {
        name
    } else {
        return fallback_bits;
    };
    let Ok(host) = cpal::host_from_id(cpal::HostId::Wasapi) else {
        return fallback_bits;
    };
    let device = if requested_name == "default" {
        host.default_output_device()
    } else {
        let Ok(mut devices) = host.output_devices() else {
            return fallback_bits;
        };
        devices.find(|dev| dev.name().is_ok_and(|name| name == requested_name))
    };
    let Some(device) = device else {
        return fallback_bits;
    };
    let Ok(configs) = device.supported_output_configs() else {
        return fallback_bits;
    };
    let bits = collect_supported_output_bit_depths(configs);
    if bits.is_empty() { fallback_bits } else { bits }
}
