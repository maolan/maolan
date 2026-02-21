pub fn playback_prefill_frames(cycle_samples: usize, nperiods: usize, sync_mode: bool) -> i64 {
    let period = cycle_samples as i64;
    let mut prefill = (nperiods.max(1) as i64).saturating_mul(period);
    if !sync_mode {
        prefill = prefill.saturating_add(period);
    }
    prefill.max(0)
}
