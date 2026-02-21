use crate::audio::io::AudioIO;
use std::sync::Arc;

pub fn has_audio_connections(port: &Arc<AudioIO>) -> bool {
    port.connection_count
        .load(std::sync::atomic::Ordering::Relaxed)
        > 0
}

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
