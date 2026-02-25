use crate::hw::convert_policy::{F32_FROM_F32, F32_TO_F32};

pub fn deinterleave_f32(src: &[f32], channels: usize, frames: usize, dst: &mut [Vec<f32>]) {
    for ch in 0..channels {
        let offset = ch * frames;
        let channel_dst = &mut dst[ch];
        channel_dst.resize(frames, 0.0);
        for i in 0..frames {
            channel_dst[i] = src[offset + i] * F32_FROM_F32;
        }
    }
}

pub fn interleave_f32(src: &[Vec<f32>], channels: usize, frames: usize, dst: &mut [f32]) {
    for ch in 0..channels {
        let offset = ch * frames;
        let channel_src = &src[ch];
        for i in 0..frames {
            dst[offset + i] = channel_src[i] * F32_TO_F32;
        }
    }
}
