#[cfg(target_os = "windows")]
use cpal::traits::{DeviceTrait, HostTrait};

#[cfg(target_os = "windows")]
enum WindowsDeviceDirection {
    Input,
    Output,
}

#[cfg(target_os = "windows")]
fn discover_windows_devices(direction: WindowsDeviceDirection) -> Vec<String> {
    let mut out = vec!["wasapi:default".to_string(), "asio:default".to_string()];
    for (host_id, prefix) in [
        (cpal::HostId::Wasapi, "wasapi"),
        (cpal::HostId::Asio, "asio"),
    ] {
        let Ok(host) = cpal::host_from_id(host_id) else {
            continue;
        };
        let devices = match direction {
            WindowsDeviceDirection::Input => host.input_devices(),
            WindowsDeviceDirection::Output => host.output_devices(),
        };
        let Ok(devices) = devices else {
            continue;
        };
        for dev in devices {
            if let Ok(name) = dev.name() {
                out.push(format!("{prefix}:{name}"));
            }
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

    let (host_id, requested_name) = if let Some(name) = device_id.strip_prefix("wasapi:") {
        (cpal::HostId::Wasapi, name)
    } else if let Some(name) = device_id.strip_prefix("asio:") {
        (cpal::HostId::Asio, name)
    } else {
        return fallback_sample_rates;
    };

    let Ok(host) = cpal::host_from_id(host_id) else {
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
