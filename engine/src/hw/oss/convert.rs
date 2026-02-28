use super::consts::*;
use crate::audio::io::AudioIO;
use nix::libc;
use std::sync::Arc;

pub(super) fn bytes_per_sample(format: u32) -> Option<usize> {
    match format {
        AFMT_S16_LE | AFMT_S16_BE => Some(2),
        AFMT_S24_LE | AFMT_S24_BE => Some(3),
        AFMT_S32_LE | AFMT_S32_BE => Some(4),
        AFMT_S8 => Some(1),
        _ => None,
    }
}

pub(super) fn supported_sample_format(format: u32) -> bool {
    matches!(
        format,
        AFMT_S16_LE | AFMT_S16_BE | AFMT_S24_LE | AFMT_S24_BE | AFMT_S32_LE | AFMT_S32_BE | AFMT_S8
    )
}

pub(super) fn cstr_fixed_prefix<const N: usize>(buf: &[libc::c_char; N]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(N);
    let bytes: Vec<u8> = buf[..len].iter().map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

pub(super) fn convert_in_to_i32_interleaved(
    format: u32,
    channels: usize,
    frames: usize,
    src: &[u8],
    dst: &mut [i32],
) {
    if format == AFMT_S32_NE {
        let samples = frames
            .saturating_mul(channels)
            .min(dst.len())
            .min(src.len() / 4);
        let bytes = samples * 4;
        unsafe {
            std::ptr::copy_nonoverlapping(src.as_ptr(), dst.as_mut_ptr().cast::<u8>(), bytes);
        }
        return;
    }
    let bps = bytes_per_sample(format).unwrap_or(4);
    let n = frames.saturating_mul(channels);
    for i in 0..n.min(dst.len()) {
        let o = i * bps;
        dst[i] = match format {
            AFMT_S16_LE => i16::from_le_bytes([src[o], src[o + 1]]) as i32 * 65536,
            AFMT_S16_BE => i16::from_be_bytes([src[o], src[o + 1]]) as i32 * 65536,
            AFMT_S24_LE => {
                let v = ((src[o + 2] as i32) << 24)
                    | ((src[o + 1] as i32) << 16)
                    | ((src[o] as i32) << 8);
                v >> 8
            }
            AFMT_S24_BE => {
                let v = ((src[o] as i32) << 24)
                    | ((src[o + 1] as i32) << 16)
                    | ((src[o + 2] as i32) << 8);
                v >> 8
            }
            AFMT_S32_LE => i32::from_le_bytes([src[o], src[o + 1], src[o + 2], src[o + 3]]),
            AFMT_S32_BE => i32::from_be_bytes([src[o], src[o + 1], src[o + 2], src[o + 3]]),
            AFMT_S8 => (src[o] as i8 as i32) << 24,
            _ => 0,
        };
    }
}

pub(super) fn convert_in_to_i32_connected(
    format: u32,
    frames: usize,
    src: &[u8],
    dst: &mut [i32],
    channels: &[Arc<AudioIO>],
) {
    if channels.iter().all(crate::hw::ports::has_audio_connections) {
        convert_in_to_i32_interleaved(format, channels.len(), frames, src, dst);
        return;
    }
    let bps = bytes_per_sample(format).unwrap_or(4);
    let channel_count = channels.len();
    for (ch, port) in channels.iter().enumerate() {
        if !crate::hw::ports::has_audio_connections(port) {
            continue;
        }
        for frame in 0..frames {
            let i = frame * channel_count + ch;
            if i >= dst.len() {
                continue;
            }
            let o = i * bps;
            dst[i] = match format {
                AFMT_S16_LE => i16::from_le_bytes([src[o], src[o + 1]]) as i32 * 65536,
                AFMT_S16_BE => i16::from_be_bytes([src[o], src[o + 1]]) as i32 * 65536,
                AFMT_S24_LE => {
                    let v = ((src[o + 2] as i32) << 24)
                        | ((src[o + 1] as i32) << 16)
                        | ((src[o] as i32) << 8);
                    v >> 8
                }
                AFMT_S24_BE => {
                    let v = ((src[o] as i32) << 24)
                        | ((src[o + 1] as i32) << 16)
                        | ((src[o + 2] as i32) << 8);
                    v >> 8
                }
                AFMT_S32_LE => i32::from_le_bytes([src[o], src[o + 1], src[o + 2], src[o + 3]]),
                AFMT_S32_BE => i32::from_be_bytes([src[o], src[o + 1], src[o + 2], src[o + 3]]),
                AFMT_S8 => (src[o] as i8 as i32) << 24,
                _ => 0,
            };
        }
    }
}

pub(super) fn convert_out_from_i32_interleaved(
    format: u32,
    channels: usize,
    frames: usize,
    src: &mut [i32],
    dst: &mut [u8],
) {
    if format == AFMT_S32_NE {
        let samples = frames
            .saturating_mul(channels)
            .min(src.len())
            .min(dst.len() / 4);
        let bytes = samples * 4;
        unsafe {
            std::ptr::copy_nonoverlapping(src.as_ptr().cast::<u8>(), dst.as_mut_ptr(), bytes);
        }
        return;
    }
    let bps = bytes_per_sample(format).unwrap_or(4);
    let n = frames.saturating_mul(channels);
    for (i, _item) in src.iter().enumerate().take(n.min(src.len())) {
        let o = i * bps;
        let s = src[i];
        match format {
            AFMT_S16_LE => {
                let v = (s >> 16) as i16;
                dst[o..o + 2].copy_from_slice(&v.to_le_bytes());
            }
            AFMT_S16_BE => {
                let v = (s >> 16) as i16;
                dst[o..o + 2].copy_from_slice(&v.to_be_bytes());
            }
            AFMT_S24_LE => {
                let v = s >> 8;
                dst[o] = v as u8;
                dst[o + 1] = (v >> 8) as u8;
                dst[o + 2] = (v >> 16) as u8;
            }
            AFMT_S24_BE => {
                let v = s >> 8;
                dst[o] = (v >> 16) as u8;
                dst[o + 1] = (v >> 8) as u8;
                dst[o + 2] = v as u8;
            }
            AFMT_S32_LE => {
                dst[o..o + 4].copy_from_slice(&s.to_le_bytes());
            }
            AFMT_S32_BE => {
                dst[o..o + 4].copy_from_slice(&s.to_be_bytes());
            }
            AFMT_S8 => {
                dst[o] = (s >> 24) as i8 as u8;
            }
            _ => {}
        }
    }
}
