#[cfg(unix)]
use crate::audio::io::AudioIO;
#[cfg(unix)]
use std::sync::Arc;

#[cfg(unix)]
pub fn has_audio_connections(port: &Arc<AudioIO>) -> bool {
    port.connection_count
        .load(std::sync::atomic::Ordering::Relaxed)
        > 0
}

#[cfg(unix)]
pub fn fill_ports_from_interleaved(
    ports: &[Arc<AudioIO>],
    frames: usize,
    connected_only: bool,
    mut sample_at: impl FnMut(usize, usize) -> f32,
) {
    for (ch_idx, io_port) in ports.iter().enumerate() {
        if connected_only && !has_audio_connections(io_port) {
            *io_port.finished.lock() = true;
            continue;
        }
        let channel_buf_lock = io_port.buffer.lock();
        let channel_samples = channel_buf_lock.as_mut();
        for (frame, sample) in channel_samples.iter_mut().enumerate().take(frames) {
            *sample = sample_at(ch_idx, frame);
        }
        *io_port.finished.lock() = true;
    }
}

#[cfg(unix)]
pub fn write_interleaved_from_ports(
    ports: &[Arc<AudioIO>],
    frames: usize,
    gain: f32,
    balance: f32,
    connected_only: bool,
    mut write_sample: impl FnMut(usize, usize, f32),
) {
    let ch_count = ports.len();
    for (ch_idx, io_port) in ports.iter().enumerate() {
        if connected_only && !has_audio_connections(io_port) {
            continue;
        }
        io_port.process();
        let channel_buf_lock = io_port.buffer.lock();
        let channel_samples = channel_buf_lock.as_ref();
        let balance_gain = crate::hw::common::channel_balance_gain(ch_count, ch_idx, balance);
        for (frame, &sample) in channel_samples.iter().enumerate().take(frames) {
            write_sample(ch_idx, frame, sample * gain * balance_gain);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::io::AudioIO;
    use std::sync::Arc;

    #[test]
    fn fill_ports_from_interleaved_skips_unconnected_ports_when_requested() {
        let connected = Arc::new(AudioIO::new(4));
        let disconnected = Arc::new(AudioIO::new(4));
        connected
            .connection_count
            .store(1, std::sync::atomic::Ordering::Relaxed);

        fill_ports_from_interleaved(
            &[connected.clone(), disconnected.clone()],
            3,
            true,
            |ch, frame| (ch * 10 + frame) as f32,
        );

        assert_eq!(connected.buffer.lock().as_ref()[..3], [0.0, 1.0, 2.0]);
        assert_eq!(disconnected.buffer.lock().as_ref()[..3], [0.0, 0.0, 0.0]);
        assert!(*connected.finished.lock());
        assert!(*disconnected.finished.lock());
    }

    #[test]
    fn write_interleaved_from_ports_applies_gain_and_stereo_balance() {
        let left_src = Arc::new(AudioIO::new(3));
        let right_src = Arc::new(AudioIO::new(3));
        let left = Arc::new(AudioIO::new(3));
        let right = Arc::new(AudioIO::new(3));
        AudioIO::connect(&left_src, &left);
        AudioIO::connect(&right_src, &right);
        left_src.buffer.lock().as_mut()[..3].copy_from_slice(&[1.0, 0.5, -1.0]);
        right_src.buffer.lock().as_mut()[..3].copy_from_slice(&[0.25, -0.25, 0.75]);

        let mut written = vec![vec![0.0_f32; 3]; 2];
        write_interleaved_from_ports(&[left, right], 3, 2.0, 0.5, true, |ch, frame, sample| {
            written[ch][frame] = sample;
        });

        assert_eq!(written[0], vec![1.0, 0.5, -1.0]);
        assert_eq!(written[1], vec![0.5, -0.5, 1.5]);
    }
}
