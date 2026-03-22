#[cfg(unix)]
pub fn latency_ranges(
    cycle_samples: usize,
    nperiods: usize,
    sync_mode: bool,
    input_latency_frames: usize,
    output_latency_frames: usize,
) -> ((usize, usize), (usize, usize)) {
    let period = cycle_samples;
    let input = (period / 2) + input_latency_frames;
    let mut output = (period / 2) + output_latency_frames;
    output += nperiods.max(1) * period;
    if !sync_mode {
        output += period;
    }
    ((input, input), (output, output))
}

#[cfg(test)]
mod tests {
    use super::latency_ranges;

    #[test]
    fn latency_ranges_adds_io_offsets_and_non_sync_extra_period() {
        let ((in_min, in_max), (out_min, out_max)) = latency_ranges(128, 3, false, 11, 17);

        assert_eq!((in_min, in_max), (75, 75));
        assert_eq!((out_min, out_max), (593, 593));
    }

    #[test]
    fn latency_ranges_clamps_zero_periods_to_one_in_sync_mode() {
        let ((in_min, in_max), (out_min, out_max)) = latency_ranges(64, 0, true, 0, 0);

        assert_eq!((in_min, in_max), (32, 32));
        assert_eq!((out_min, out_max), (96, 96));
    }
}
