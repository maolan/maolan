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

#[cfg(target_os = "macos")]
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

#[cfg(test)]
mod tests {
    use super::{channel_balance_gain, output_meter_linear};
    use crate::audio::io::AudioIO;
    use std::sync::Arc;

    #[test]
    fn channel_balance_gain_is_neutral_for_non_stereo() {
        assert_eq!(channel_balance_gain(1, 0, -1.0), 1.0);
        assert_eq!(channel_balance_gain(4, 2, 0.75), 1.0);
    }

    #[test]
    fn channel_balance_gain_clamps_full_left_and_right_for_stereo() {
        assert_eq!(channel_balance_gain(2, 0, -1.0), 1.0);
        assert_eq!(channel_balance_gain(2, 1, -1.0), 0.0);
        assert_eq!(channel_balance_gain(2, 0, 1.0), 0.0);
        assert_eq!(channel_balance_gain(2, 1, 1.0), 1.0);
    }

    #[test]
    fn output_meter_linear_uses_peak_absolute_sample_with_gain_and_balance() {
        let left = Arc::new(AudioIO::new(4));
        let right = Arc::new(AudioIO::new(4));
        left.buffer.lock().copy_from_slice(&[0.25, -0.75, 0.5, 0.1]);
        right.buffer.lock().copy_from_slice(&[-0.2, 0.4, -0.6, 0.3]);

        let meter = output_meter_linear(&[left, right], 2.0, 0.5);

        assert_eq!(meter.len(), 2);
        assert!((meter[0] - 0.75).abs() < 1.0e-6);
        assert!((meter[1] - 1.2).abs() < 1.0e-6);
    }

    #[test]
    fn output_meter_linear_handles_empty_outputs_and_zero_gain() {
        assert!(output_meter_linear(&[], 1.0, 0.0).is_empty());

        let mono = Arc::new(AudioIO::new(2));
        mono.buffer.lock().copy_from_slice(&[0.9, -0.4]);
        let meter = output_meter_linear(&[mono], 0.0, 9.0);
        assert_eq!(meter, vec![0.0]);
    }

    #[test]
    fn channel_balance_gain_clamps_out_of_range_balance() {
        assert_eq!(channel_balance_gain(2, 0, 2.0), 0.0);
        assert_eq!(channel_balance_gain(2, 1, 2.0), 1.0);
        assert_eq!(channel_balance_gain(2, 0, -2.0), 1.0);
        assert_eq!(channel_balance_gain(2, 1, -2.0), 0.0);
    }
}
