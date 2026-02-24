#![cfg(target_os = "macos")]

//! End-to-end integration test for the CoreAudio backend.
//!
//! Opens the default output device, plays a short 440 Hz sine tone through it,
//! and asserts that no xruns occurred. Marked `#[ignore]` because it requires
//! real audio hardware and cannot run in headless CI.

#[cfg(test)]
mod tests {
    use maolan_engine::hw::coreaudio::device::{self, DeviceInfo};
    use maolan_engine::hw::coreaudio::driver::HwDriver;
    use maolan_engine::hw::coreaudio::ioproc::{IoProcHandle, SharedIOState};
    use std::sync::Arc;

    /// Find the first device with at least one output channel.
    fn first_output_device() -> Option<DeviceInfo> {
        device::list_devices()
            .into_iter()
            .find(|d| d.output_channels > 0)
    }

    #[test]
    #[ignore = "requires CoreAudio hardware"]
    fn test_coreaudio_sine_no_xruns() {
        // 1. Enumerate devices — there must be at least one.
        let devices = device::list_devices();
        assert!(!devices.is_empty(), "no CoreAudio devices found");

        // 2. Pick the first device with output channels.
        let dev = first_output_device().expect("no output device found");
        assert!(
            dev.output_channels > 0,
            "selected device has no output channels"
        );

        // 3. Open the driver at 44100 Hz with default options.
        let sample_rate = 44100;
        let driver =
            HwDriver::new(&dev, sample_rate).expect("failed to open CoreAudio HwDriver");

        let cycle_samples = driver.cycle_samples();
        let out_ch = driver.output_channels();
        assert!(out_ch > 0, "driver reports 0 output channels");

        // 4. Create SharedIOState and start the IOProc.
        let state = Arc::new(SharedIOState::new(
            driver.input_channels(),
            out_ch,
            cycle_samples,
            sample_rate as u32,
        ));
        let _handle =
            IoProcHandle::new(driver.device_id(), state.clone()).expect("failed to start IOProc");

        // 5. Generate ~1 second of 440 Hz sine and push it through run_cycle.
        let num_cycles = sample_rate as usize / cycle_samples; // roughly 1 second
        let freq = 440.0_f64;
        let mut phase: f64 = 0.0;
        let phase_inc = freq * 2.0 * std::f64::consts::PI / sample_rate as f64;

        // Collect output port Arcs from the driver.
        let output_ports: Vec<_> = (0..out_ch)
            .map(|i| driver.output_port(i).expect("missing output port"))
            .collect();
        let input_ports: Vec<_> = (0..driver.input_channels())
            .map(|i| driver.input_port(i).expect("missing input port"))
            .collect();

        for _cycle in 0..num_cycles {
            // Write a sine tone into every output port buffer.
            for port in &output_ports {
                let buf = port.buffer.lock();
                for i in 0..buf.len().min(cycle_samples) {
                    buf[i] = (phase + phase_inc * i as f64).sin() as f32 * 0.25;
                }
            }
            // Advance phase for the next cycle.
            phase += phase_inc * cycle_samples as f64;
            // Keep phase in [0, 2*PI) to avoid precision loss.
            phase %= 2.0 * std::f64::consts::PI;

            // Run one IOProc cycle (blocks until CoreAudio fires the callback).
            state
                .run_cycle(&input_ports, &output_ports, 1.0, 0.0)
                .expect("run_cycle failed");
        }

        // 6. Assert no xruns occurred.
        let data = state.mutex.lock().expect("failed to lock SharedIOState");
        assert_eq!(
            data.xrun_count, 0,
            "expected 0 xruns but got {}",
            data.xrun_count
        );
    }
}
