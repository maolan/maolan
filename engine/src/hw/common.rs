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
