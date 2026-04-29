use super::AudioDeviceOption;
use crate::consts::state_platform_freebsd::{
    AFMT_S8, AFMT_S16_BE, AFMT_S16_LE, AFMT_S24_BE, AFMT_S24_LE, AFMT_S32_BE, AFMT_S32_LE,
};
use nvtree::{Nvtree, Nvtvalue, nvtree_find, nvtree_unpack};
use std::{ffi::c_void, fs::File, os::fd::AsRawFd};

pub(crate) fn discover_freebsd_audio_devices() -> Vec<AudioDeviceOption> {
    let mut devices = discover_sndstat_dsp_devices().unwrap_or_default();
    devices.sort_by_key(|a| a.label.to_lowercase());
    devices.dedup_by(|a, b| a.id == b.id);
    devices
}

fn discover_sndstat_dsp_devices() -> Option<Vec<AudioDeviceOption>> {
    let file = File::open("/dev/sndstat").ok()?;
    let fd = file.as_raw_fd();

    unsafe {
        if sndst_refresh_devs(fd).is_err() {
            return None;
        }
    }

    let mut arg = SndstIoctlNvArg {
        nbytes: 0,
        buf: std::ptr::null_mut(),
    };
    unsafe {
        if sndst_get_devs(fd, &mut arg).is_err() {
            return None;
        }
    }
    if arg.nbytes == 0 {
        return None;
    }

    let mut buf = vec![0_u8; arg.nbytes];
    arg.buf = buf.as_mut_ptr().cast::<c_void>();
    unsafe {
        if sndst_get_devs(fd, &mut arg).is_err() {
            return None;
        }
    }
    if arg.nbytes == 0 || arg.nbytes > buf.len() {
        return None;
    }

    parse_sndstat_nvlist(&buf[..arg.nbytes])
}

fn parse_sndstat_nvlist(buf: &[u8]) -> Option<Vec<AudioDeviceOption>> {
    let root = nvtree_unpack(buf).ok()?;
    let dsps = if let Some(pair) = nvtree_find(&root, "dsps") {
        match &pair.value {
            Nvtvalue::NestedArray(arr) => arr,
            _ => return None,
        }
    } else {
        return None;
    };

    let out = dsps
        .iter()
        .filter_map(|dsp| {
            let devnode_pair = nvtree_find(dsp, "devnode")?;
            let Nvtvalue::String(devnode) = &devnode_pair.value else {
                return None;
            };

            let devpath = if devnode.starts_with('/') {
                devnode.to_string()
            } else {
                format!("/dev/{devnode}")
            };

            if !devpath.starts_with("/dev/dsp") {
                return None;
            }

            let label_prefix = nvtree_find(dsp, "desc")
                .and_then(|pair| match &pair.value {
                    Nvtvalue::String(s) if !s.is_empty() => Some(s.clone()),
                    _ => None,
                })
                .or_else(|| {
                    nvtree_find(dsp, "nameunit").and_then(|pair| match &pair.value {
                        Nvtvalue::String(s) if !s.is_empty() => Some(s.clone()),
                        _ => None,
                    })
                });
            let label = label_prefix
                .map(|prefix| format!("{prefix} ({devpath})"))
                .unwrap_or_else(|| devpath.clone());
            let mut supported_bits = decode_supported_bits_from_dsp(dsp);
            if supported_bits.is_empty() {
                supported_bits = probe_oss_supported_bits(&devpath);
            }
            let mut supported_sample_rates = decode_supported_sample_rates_from_dsp(dsp);
            if supported_sample_rates.is_empty() {
                supported_sample_rates = probe_oss_supported_sample_rates(&devpath);
            }
            Some(AudioDeviceOption::with_supported_caps(
                devpath,
                label,
                supported_bits,
                supported_sample_rates,
            ))
        })
        .collect::<Vec<_>>();

    (!out.is_empty()).then_some(out)
}

fn decode_supported_bits_from_dsp(dsp: &Nvtree) -> Vec<usize> {
    fn parse_number_text(s: &str) -> Option<u64> {
        let trimmed = s.trim();
        if let Some(hex) = trimmed
            .strip_prefix("0x")
            .or_else(|| trimmed.strip_prefix("0X"))
        {
            return u64::from_str_radix(hex, 16).ok();
        }
        trimmed.parse::<u64>().ok()
    }

    fn format_mask_from_value(value: &Nvtvalue) -> Option<u64> {
        match value {
            Nvtvalue::Number(n) => Some(*n),
            Nvtvalue::String(s) => parse_number_text(s),
            Nvtvalue::NumberArray(arr) => Some(arr.iter().copied().fold(0_u64, |acc, n| acc | n)),
            Nvtvalue::StringArray(arr) => Some(
                arr.iter()
                    .filter_map(|s| parse_number_text(s))
                    .fold(0_u64, |acc, n| acc | n),
            ),
            _ => None,
        }
    }

    fn format_mask_from_tree(tree: &Nvtree) -> Option<u64> {
        for key in crate::consts::state_platform_freebsd_lists::DIRECT_KEYS {
            if let Some(pair) = nvtree_find(tree, key)
                && let Some(mask) = format_mask_from_value(&pair.value)
            {
                return Some(mask);
            }
        }
        None
    }

    let mut mask = format_mask_from_tree(dsp).unwrap_or(0);
    for nested_name in ["play", "playback", "record", "capture"] {
        if let Some(pair) = nvtree_find(dsp, nested_name)
            && let Nvtvalue::Nested(nested) = &pair.value
        {
            mask |= format_mask_from_tree(nested).unwrap_or(0);
        }
    }

    bits_from_format_mask(mask)
}

fn bits_from_format_mask(mask: u64) -> Vec<usize> {
    let mut bits = Vec::with_capacity(4);
    if (mask & (AFMT_S32_LE | AFMT_S32_BE)) != 0 {
        bits.push(32);
    }
    if (mask & (AFMT_S24_LE | AFMT_S24_BE)) != 0 {
        bits.push(24);
    }
    if (mask & (AFMT_S16_LE | AFMT_S16_BE)) != 0 {
        bits.push(16);
    }
    if (mask & AFMT_S8) != 0 {
        bits.push(8);
    }
    bits
}

fn decode_supported_sample_rates_from_dsp(dsp: &Nvtree) -> Vec<i32> {
    use std::collections::BTreeSet;

    fn parse_number_text(s: &str) -> Option<i32> {
        s.trim().parse::<i32>().ok().filter(|v| *v > 0)
    }

    fn collect_rates_from_value(value: &Nvtvalue, rates: &mut BTreeSet<i32>) {
        match value {
            Nvtvalue::Number(n) => {
                if let Ok(rate) = i32::try_from(*n)
                    && rate > 0
                {
                    rates.insert(rate);
                }
            }
            Nvtvalue::String(s) => {
                if let Some(rate) = parse_number_text(s) {
                    rates.insert(rate);
                }
            }
            Nvtvalue::NumberArray(arr) => {
                for rate in arr.iter().copied().filter_map(|n| i32::try_from(n).ok()) {
                    if rate > 0 {
                        rates.insert(rate);
                    }
                }
            }
            Nvtvalue::StringArray(arr) => {
                for s in arr {
                    if let Some(rate) = parse_number_text(s) {
                        rates.insert(rate);
                    }
                }
            }
            _ => {}
        }
    }

    fn collect_rates_from_tree(tree: &Nvtree, rates: &mut BTreeSet<i32>) {
        for key in crate::consts::state_platform_freebsd_lists::RATE_KEYS {
            if let Some(pair) = nvtree_find(tree, key) {
                collect_rates_from_value(&pair.value, rates);
            }
        }
    }

    let mut rates = BTreeSet::new();
    collect_rates_from_tree(dsp, &mut rates);
    for nested_name in ["play", "playback", "record", "capture"] {
        if let Some(pair) = nvtree_find(dsp, nested_name)
            && let Nvtvalue::Nested(nested) = &pair.value
        {
            collect_rates_from_tree(nested, &mut rates);
        }
    }
    rates.into_iter().collect()
}

fn probe_oss_supported_bits(devpath: &str) -> Vec<usize> {
    let Ok(file) = std::fs::OpenOptions::new().read(true).open(devpath) else {
        return Vec::new();
    };
    let fd = file.as_raw_fd();
    let mut formats = 0_i32;
    let ok = unsafe { oss_get_formats(fd, &mut formats).is_ok() };
    if !ok {
        return Vec::new();
    }
    bits_from_format_mask(formats as u64)
}

fn probe_oss_supported_sample_rates(devpath: &str) -> Vec<i32> {
    let Ok(file) = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(devpath)
    else {
        return Vec::new();
    };
    let fd = file.as_raw_fd();
    let mut supported = std::collections::BTreeSet::new();
    for candidate in crate::consts::state_platform_freebsd_lists::SAMPLE_RATE_CANDIDATES {
        let mut rate = candidate;
        let ok = unsafe { oss_set_speed(fd, &mut rate).is_ok() };
        if ok && rate > 0 {
            supported.insert(rate);
        }
    }
    supported.into_iter().collect()
}

#[repr(C)]
struct SndstIoctlNvArg {
    nbytes: usize,
    buf: *mut c_void,
}

nix::ioctl_none!(sndst_refresh_devs, b'D', 100);
nix::ioctl_readwrite!(sndst_get_devs, b'D', 101, SndstIoctlNvArg);
nix::ioctl_read!(oss_get_formats, b'P', 11, i32);
nix::ioctl_readwrite!(oss_set_speed, b'P', 2, i32);
