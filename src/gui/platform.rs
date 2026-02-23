use std::{fs, path::Path, process::Command};

pub(super) fn kernel_midi_label(path: &str) -> String {
    let basename = Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .to_string();

    if let Some(label) = linux_alsa_label(&basename) {
        return label;
    }

    fn sysctl_value(key: &str) -> Option<String> {
        let output = Command::new("sysctl").arg("-n").arg(key).output().ok()?;
        if !output.status.success() {
            return None;
        }
        let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
        (!value.is_empty()).then_some(value)
    }

    // FreeBSD maps umidi nodes through uaudio units on many systems.
    let dev_id: String = basename
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if !dev_id.is_empty() {
        if basename.starts_with("umidi")
            && let Some(desc) = sysctl_value(&format!("dev.uaudio.{dev_id}.%desc"))
        {
            return compact_desc(desc);
        }
        if basename.starts_with("midi")
            && let Some(desc) = sysctl_value(&format!("dev.midi.{dev_id}.%desc"))
        {
            return compact_desc(desc);
        }
    }

    let probe_keys = {
        let short = basename.split('.').next().unwrap_or(&basename).to_string();
        if short == basename {
            vec![basename.clone()]
        } else {
            vec![basename.clone(), short]
        }
    };

    if let Ok(sndstat) = fs::read_to_string("/dev/sndstat") {
        for line in sndstat.lines() {
            if !probe_keys.iter().any(|key| line.contains(key)) {
                continue;
            }
            if let (Some(start), Some(end)) = (line.find('<'), line.rfind('>'))
                && start < end
            {
                let label = line[start + 1..end].trim();
                if !label.is_empty() {
                    return label.to_string();
                }
            }
            let compact = line.trim();
            if !compact.is_empty() {
                return compact.to_string();
            }
        }
    }

    basename
}

fn linux_alsa_label(basename: &str) -> Option<String> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    if !basename.starts_with("midiC") {
        return None;
    }
    let suffix = basename.strip_prefix("midiC")?;
    let (card_str, dev_str) = suffix.split_once('D')?;
    let card: usize = card_str.parse().ok()?;
    let dev: usize = dev_str.parse().ok()?;

    let read_trimmed = |path: String| -> Option<String> {
        let value = fs::read_to_string(path).ok()?.trim().to_string();
        (!value.is_empty()).then_some(value)
    };

    let card_dir = format!("/proc/asound/card{card}");
    let card_label = read_trimmed(format!("{card_dir}/longname"))
        .or_else(|| read_trimmed(format!("{card_dir}/id")))
        .or_else(|| read_trimmed(format!("{card_dir}/shortname")));

    let midi_info = read_trimmed(format!("{card_dir}/midi{dev}/info"));
    let midi_name = midi_info.and_then(|info| {
        info.lines()
            .find_map(|line| line.strip_prefix("name:"))
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty())
    });

    match (card_label, midi_name) {
        (Some(card_label), Some(midi_name)) => {
            if midi_name.contains(&card_label) {
                Some(midi_name)
            } else {
                Some(format!("{card_label}: {midi_name}"))
            }
        }
        (Some(card_label), None) => Some(card_label),
        (None, Some(midi_name)) => Some(midi_name),
        (None, None) => None,
    }
}

fn compact_desc(desc: String) -> String {
    desc.split(',').next().unwrap_or(&desc).trim().to_string()
}
