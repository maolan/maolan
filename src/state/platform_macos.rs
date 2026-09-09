use super::AudioDeviceOption;
use maolan_engine::audio_devices::AudioDeviceDescriptor;

const DEFAULT_SAMPLE_RATES: [i32; 12] = [
    8_000, 11_025, 16_000, 22_050, 32_000, 44_100, 48_000, 88_200, 96_000, 176_400, 192_000,
    384_000,
];

fn to_device_option(device: AudioDeviceDescriptor) -> AudioDeviceOption {
    let mut option = AudioDeviceOption::from(device);
    if option.supported_sample_rates.is_empty() {
        option.supported_sample_rates = DEFAULT_SAMPLE_RATES.to_vec();
    }
    option
}

pub(crate) fn discover_coreaudio_output_devices() -> Vec<AudioDeviceOption> {
    maolan_engine::audio_devices::discover_coreaudio_audio_devices()
        .into_iter()
        .map(to_device_option)
        .filter(|d| d.supports_output)
        .collect()
}

pub(crate) fn discover_coreaudio_input_devices() -> Vec<AudioDeviceOption> {
    maolan_engine::audio_devices::discover_coreaudio_audio_devices()
        .into_iter()
        .map(to_device_option)
        .filter(|d| d.supports_input)
        .collect()
}
