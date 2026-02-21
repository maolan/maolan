use crate::midi::io::MidiEvent;
use crate::audio::io::AudioIO;
use std::sync::Arc;

pub trait HwWorkerDriver {
    fn cycle_samples(&self) -> usize;
    fn sample_rate(&self) -> i32;
    fn run_cycle_for_worker(&mut self) -> Result<(), String>;
    fn run_assist_step_for_worker(&mut self) -> Result<bool, String>;
}

pub trait HwMidiHub {
    fn read_events_into(&mut self, out: &mut Vec<MidiEvent>);
    fn write_events(&mut self, events: &[MidiEvent]);
}

#[allow(dead_code)]
pub trait HwDevice {
    fn input_channels(&self) -> usize;
    fn output_channels(&self) -> usize;
    fn sample_rate(&self) -> i32;
    fn cycle_samples(&self) -> usize;
    fn input_port(&self, idx: usize) -> Option<Arc<AudioIO>>;
    fn output_port(&self, idx: usize) -> Option<Arc<AudioIO>>;
    fn set_output_gain_balance(&mut self, gain: f32, balance: f32);
    fn output_meter_db(&self, gain: f32, balance: f32) -> Vec<f32>;
    fn latency_ranges(&self) -> ((usize, usize), (usize, usize));
}

#[macro_export]
macro_rules! impl_hw_worker_traits_for_driver {
    ($driver:ty) => {
        impl crate::hw::traits::HwWorkerDriver for $driver {
            fn cycle_samples(&self) -> usize {
                self.cycle_samples()
            }

            fn sample_rate(&self) -> i32 {
                self.sample_rate()
            }

            fn run_cycle_for_worker(&mut self) -> Result<(), String> {
                self.channel().run_cycle().map_err(|e| e.to_string())
            }

            fn run_assist_step_for_worker(&mut self) -> Result<bool, String> {
                self.channel().run_assist_step().map_err(|e| e.to_string())
            }
        }
    };
}

#[macro_export]
macro_rules! impl_hw_device_for_driver {
    ($driver:ty) => {
        impl crate::hw::traits::HwDevice for $driver {
            fn input_channels(&self) -> usize {
                self.input_channels()
            }

            fn output_channels(&self) -> usize {
                self.output_channels()
            }

            fn sample_rate(&self) -> i32 {
                self.sample_rate()
            }

            fn cycle_samples(&self) -> usize {
                self.cycle_samples()
            }

            fn input_port(
                &self,
                idx: usize,
            ) -> Option<std::sync::Arc<crate::audio::io::AudioIO>> {
                self.input_port(idx)
            }

            fn output_port(
                &self,
                idx: usize,
            ) -> Option<std::sync::Arc<crate::audio::io::AudioIO>> {
                self.output_port(idx)
            }

            fn set_output_gain_balance(&mut self, gain: f32, balance: f32) {
                self.set_output_gain_balance(gain, balance);
            }

            fn output_meter_db(&self, gain: f32, balance: f32) -> Vec<f32> {
                self.output_meter_db(gain, balance)
            }

            fn latency_ranges(&self) -> ((usize, usize), (usize, usize)) {
                self.latency_ranges()
            }
        }
    };
}

#[macro_export]
macro_rules! impl_hw_midi_hub_traits {
    ($hub:ty) => {
        impl crate::hw::traits::HwMidiHub for $hub {
            fn read_events_into(&mut self, out: &mut Vec<crate::midi::io::MidiEvent>) {
                <$hub>::read_events_into(self, out);
            }

            fn write_events(&mut self, events: &[crate::midi::io::MidiEvent]) {
                <$hub>::write_events(self, events);
            }
        }
    };
}
