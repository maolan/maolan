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

pub fn output_meter_db(channels: &[Arc<AudioIO>], gain: f32, balance: f32) -> Vec<f32> {
    let ch_count = channels.len();
    let mut out = Vec::with_capacity(ch_count);
    for (channel_idx, channel) in channels.iter().enumerate() {
        let balance_gain = channel_balance_gain(ch_count, channel_idx, balance);
        let buf = channel.buffer.lock();
        let mut peak = 0.0_f32;
        for &sample in buf.iter() {
            let v = if sample >= 0.0 { sample } else { -sample };
            if v > peak {
                peak = v;
            }
        }
        let peak = peak * gain * balance_gain;
        let meter = if peak <= 1.0e-6 {
            -90.0
        } else {
            (20.0 * peak.log10()).clamp(-90.0, 20.0)
        };
        out.push(meter);
    }
    out
}
