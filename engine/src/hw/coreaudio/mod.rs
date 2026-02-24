//! CoreAudio backend for macOS.
//!
//! Follows the same structural pattern as the OSS backend:
//!   - device.rs     — AudioDeviceID enumeration
//!   - driver.rs     — buffer-size / sample-rate negotiation, HwDriver impl
//!   - convert.rs    — f32 pass-through conversion helpers
//!   - error_fmt.rs  — OSStatus → human-readable error strings
//!   - ioproc.rs     — IOProc callback + SharedIOState condvar bridge
//!   - latency.rs    — kAudioDevicePropertyLatency queries
//!   - midi_hub.rs   — CoreMIDI HwMidiHub implementation
//!   - sync.rs       — DuplexSync / Correction / FrameClock port

pub mod convert;
pub mod device;
pub mod driver;
pub mod error_fmt;
pub mod ioproc;
pub mod latency;
pub mod midi_hub;
pub mod sync;

pub use self::device::list_devices;
pub use self::driver::HwDriver;
pub use self::midi_hub::MidiHub;
pub use crate::hw::options::HwOptions;
