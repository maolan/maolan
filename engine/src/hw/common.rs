use crate::audio::io::AudioIO;
use std::sync::Arc;

pub fn channel_balance_gain(ch_count: usize, channel_idx: usize, balance: f32) -> f32 {
    if ch_count == 2 {
        let b = balance.clamp(-1.0, 1.0);
        if channel_idx == 0 {
            (1.0 - b).clamp(0.0, 1.0)
        } else {
            (1.0 + b).clamp(0.0, 1.0)
        }
    } else {
        1.0
    }
}

pub fn output_meter_linear(audio_outs: &[Arc<AudioIO>], gain: f32, balance: f32) -> Vec<f32> {
    let ch_count = audio_outs.len();
    let mut out = Vec::with_capacity(ch_count);
    for (channel_idx, channel) in audio_outs.iter().enumerate() {
        let balance_gain = channel_balance_gain(ch_count, channel_idx, balance);
        let buf = channel.buffer.lock();
        let mut peak = 0.0_f32;
        for &sample in buf.iter() {
            let v = sample.abs();
            if v > peak {
                peak = v;
            }
        }
        out.push(peak * gain * balance_gain);
    }
    out
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub fn output_meter_db(audio_outs: &[Arc<AudioIO>], gain: f32, balance: f32) -> Vec<f32> {
    output_meter_linear(audio_outs, gain, balance)
        .into_iter()
        .map(|peak| {
            if peak <= 1.0e-6 {
                -90.0
            } else {
                (20.0 * peak.log10()).clamp(-90.0, 20.0)
            }
        })
        .collect()
}
