use std::{fs, path::Path, process::Command};

pub(super) fn kernel_midi_label(path: &str) -> String {
    let basename = Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .to_string();

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

fn compact_desc(desc: String) -> String {
    desc.split(',').next().unwrap_or(&desc).trim().to_string()
}
