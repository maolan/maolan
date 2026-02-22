use super::{Audio, MidiHub, OSSChannel};
use crate::audio::io::AudioIO;
use crate::hw::common;
use crate::hw::latency;
use crate::hw::options::HwOptions;
use crate::hw::prefill;
use std::sync::Arc;

#[derive(Debug)]
pub struct HwDriver {
    capture: Audio,
    playback: Audio,
    nperiods: usize,
    sync_mode: bool,
    input_latency_frames: usize,
    output_latency_frames: usize,
}

impl Default for HwOptions {
    fn default() -> Self {
        Self {
            exclusive: false,
            period_frames: 1024,
            nperiods: 1,
            ignore_hwbuf: false,
            sync_mode: false,
            input_latency_frames: 0,
            output_latency_frames: 0,
        }
    }
}

impl HwDriver {
    pub fn new(path: &str, rate: i32, bits: i32) -> std::io::Result<Self> {
        Self::new_with_options(path, rate, bits, HwOptions::default())
    }

    pub fn new_with_options(
        path: &str,
        rate: i32,
        bits: i32,
        options: HwOptions,
    ) -> std::io::Result<Self> {
        let capture = Audio::new(path, rate, bits, true, options)?;
        let playback = Audio::new(path, rate, bits, false, options)?;
        let mut driver = Self {
            capture,
            playback,
            nperiods: options.nperiods.max(1),
            sync_mode: options.sync_mode,
            input_latency_frames: options.input_latency_frames,
            output_latency_frames: options.output_latency_frames,
        };
        driver.apply_playback_prefill();
        Ok(driver)
    }

    pub fn input_fd(&self) -> i32 {
        self.capture.fd()
    }

    pub fn output_fd(&self) -> i32 {
        self.playback.fd()
    }

    pub fn input_channels(&self) -> usize {
        self.capture.channels.len()
    }

    pub fn output_channels(&self) -> usize {
        self.playback.channels.len()
    }

    pub fn sample_rate(&self) -> i32 {
        self.playback.rate
    }

    pub fn cycle_samples(&self) -> usize {
        self.playback.chsamples
    }

    pub fn input_port(&self, idx: usize) -> Option<Arc<AudioIO>> {
        self.capture.channels.get(idx).cloned()
    }

    pub fn output_port(&self, idx: usize) -> Option<Arc<AudioIO>> {
        self.playback.channels.get(idx).cloned()
    }

    pub fn set_output_gain_balance(&mut self, gain: f32, balance: f32) {
        self.playback.output_gain_linear = gain;
        self.playback.output_balance = balance;
    }

    pub fn output_meter_db(&self, gain: f32, balance: f32) -> Vec<f32> {
        common::output_meter_db(&self.playback.channels, gain, balance)
    }

    pub fn start_input_trigger(&self) -> std::io::Result<()> {
        self.capture.start_trigger()
    }

    pub fn start_output_trigger(&self) -> std::io::Result<()> {
        self.playback.start_trigger()
    }

    pub fn channel(&mut self) -> OSSChannel<'_> {
        OSSChannel {
            capture: &mut self.capture,
            playback: &mut self.playback,
        }
    }

    pub fn latency_ranges(&self) -> ((usize, usize), (usize, usize)) {
        latency::latency_ranges(
            self.cycle_samples(),
            self.nperiods,
            self.sync_mode,
            self.input_latency_frames,
            self.output_latency_frames,
        )
    }

    fn apply_playback_prefill(&mut self) {
        let prefill =
            prefill::playback_prefill_frames(self.cycle_samples(), self.nperiods, self.sync_mode);
        let mut sync = self
            .capture
            .duplex_sync
            .lock()
            .expect("duplex sync poisoned");
        sync.playback_prefill_frames = prefill.max(0);
    }
}

crate::impl_hw_worker_traits_for_driver!(HwDriver);
crate::impl_hw_device_for_driver!(HwDriver);
crate::impl_hw_midi_hub_traits!(MidiHub);
