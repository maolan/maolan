# Change: Add native CoreAudio backend for macOS

## Why
macOS currently supports only JACK for audio I/O, requiring users to install and configure a third-party audio server. A native CoreAudio backend removes this dependency and provides a first-class experience on macOS with lower latency and better system integration.

## What Changes
- Add `coreaudio-sys` and `coremidi` crate dependencies (macOS-only)
- Create `engine/src/hw/coreaudio/` module implementing `HwDriver` via CoreAudio HAL IOProc callbacks
- Create `CoreAudioBackend` struct (type-aliased as `HwWorker` on macOS) mirroring the existing `OssBackend` / `AlsaBackend` pattern
- Implement `SharedIOState` with condvar bridge between the HAL real-time thread and the engine worker thread
- Implement device enumeration, buffer access, latency compensation, and xrun detection
- Add CoreMIDI-based `MidiHub` implementation for macOS
- Wire the new module into `hw/mod.rs`, `engine.rs`, and `config.rs` behind `#[cfg(target_os = "macos")]`

## Impact
- Affected specs: `hw-coreaudio` (new capability)
- Affected code: `engine/Cargo.toml`, `engine/src/hw/mod.rs`, `engine/src/hw/coreaudio/`, `engine/src/hw/config.rs`, `engine/src/engine.rs`, `engine/src/coreaudio_worker.rs`

## Epic Breakdown

### Epic 1 — Foundation
Set up dependencies, module wiring, `CoreAudioBackend` struct, and device enumeration.

### Epic 2 — Buffer Access
Implement IOProc callback, `SharedIOState` condvar bridge, and mmap-equivalent buffer exchange between the HAL thread and the worker thread.

### Epic 3 — Latency Compensation
Query hardware and safety-offset latencies from CoreAudio properties and plumb them into the engine's latency compensation pipeline.

### Epic 4 — Xrun Handling
Detect IOProc overloads via `kAudioDeviceProcessorOverload`, surface xrun counts, and implement recovery logic.

### Epic 5 — MIDI
Implement CoreMIDI-based `MidiHub` for MIDI device discovery, input, and output on macOS.

### Epic 6 — Polish
Integration testing, error reporting, sample-rate switching, default-device change notifications, and documentation.

## Key API Surface

| Struct / Trait | Location | Role |
|---|---|---|
| `CoreAudioBackend` | `engine/src/coreaudio_worker.rs` | `HwWorker` type alias on macOS |
| `HwDriver` (impl) | `engine/src/hw/coreaudio/driver.rs` | Device open/close, stream start/stop |
| `SharedIOState` | `engine/src/hw/coreaudio/io_state.rs` | Lock-free condvar bridge between HAL and worker |
| `MidiHub` (impl) | `engine/src/hw/coreaudio/midi.rs` | CoreMIDI device enumeration and I/O |
