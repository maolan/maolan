use super::AudioDeviceOption;

pub(crate) fn discover_alsa_output_devices() -> Vec<AudioDeviceOption> {
    maolan_engine::audio_devices::discover_alsa_audio_devices()
        .into_iter()
        .map(AudioDeviceOption::from)
        .filter(|device| device.supports_output)
        .collect()
}

pub(crate) fn discover_alsa_input_devices() -> Vec<AudioDeviceOption> {
    maolan_engine::audio_devices::discover_alsa_audio_devices()
        .into_iter()
        .map(AudioDeviceOption::from)
        .filter(|device| device.supports_input)
        .collect()
}
