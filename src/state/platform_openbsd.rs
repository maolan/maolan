use super::AudioDeviceOption;

const DEFAULT_SAMPLE_RATES: [i32; 12] = [
    8_000, 11_025, 16_000, 22_050, 32_000, 44_100, 48_000, 88_200, 96_000, 176_400, 192_000,
    384_000,
];

pub(crate) fn discover_openbsd_audio_devices() -> Vec<AudioDeviceOption> {
    let mut out = vec![AudioDeviceOption::with_supported_caps(
        "default",
        "Default (sndio)",
        vec![32, 24, 16, 8],
        DEFAULT_SAMPLE_RATES.to_vec(),
    )];

    let mut paths: Vec<String> = std::fs::read_dir("/dev")
        .map(|rd| {
            rd.filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter_map(|path| {
                    let name = path.file_name()?.to_str()?;
                    if !name.starts_with("audio") || name.starts_with("audioctl") {
                        return None;
                    }
                    if name[5..].chars().all(|c| c.is_ascii_digit()) {
                        Some(path.to_string_lossy().into_owned())
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    paths.sort();
    paths.dedup();

    for dev in paths {
        out.push(AudioDeviceOption::with_supported_caps(
            dev.clone(),
            format!("{dev} (sndio sun)"),
            vec![32, 24, 16, 8],
            DEFAULT_SAMPLE_RATES.to_vec(),
        ));
    }

    out
}
