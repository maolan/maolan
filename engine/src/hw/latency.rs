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
