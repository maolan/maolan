pub fn playback_prefill_frames(cycle_samples: usize, nperiods: usize, sync_mode: bool) -> i64 {
    let period = cycle_samples as i64;
    let mut prefill = (nperiods.max(1) as i64).saturating_mul(period);
    if !sync_mode {
        prefill = prefill.saturating_add(period);
    }
    prefill.max(0)
}

#[cfg(test)]
mod tests {
    use super::playback_prefill_frames;

    #[test]
    fn playback_prefill_frames_adds_extra_cycle_when_not_syncing() {
        assert_eq!(playback_prefill_frames(128, 3, false), 512);
        assert_eq!(playback_prefill_frames(128, 3, true), 384);
    }

    #[test]
    fn playback_prefill_frames_clamps_zero_periods_to_one() {
        assert_eq!(playback_prefill_frames(64, 0, true), 64);
        assert_eq!(playback_prefill_frames(64, 0, false), 128);
    }
}
