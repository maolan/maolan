## ADDED Requirements

### Requirement: CoreAudio Device Enumeration
The system SHALL enumerate available CoreAudio audio devices on macOS using `kAudioHardwarePropertyDevices`. Device names SHALL be formatted as `coreaudio:<name>`. The system SHALL query channel counts via `kAudioDevicePropertyStreamConfiguration`.

#### Scenario: List available audio devices
- **WHEN** the engine initializes on macOS
- **THEN** all CoreAudio devices are enumerated with `coreaudio:` prefixed names and correct channel counts

### Requirement: CoreAudio IOProc Audio Streaming
The system SHALL register an IOProc callback via `AudioDeviceCreateIOProcID` for audio input and output. The IOProc SHALL copy audio buffers through a `SharedIOState` condvar bridge to the engine worker thread without blocking the real-time HAL thread.

#### Scenario: Audio playback through IOProc
- **WHEN** the engine starts playback on a CoreAudio device
- **THEN** audio samples are delivered to the hardware via the IOProc callback with no additional buffering beyond the HAL buffer

### Requirement: CoreAudio Latency Compensation
The system SHALL query `kAudioDevicePropertyLatency` and `kAudioDevicePropertySafetyOffset` and incorporate these values into the engine's latency compensation pipeline.

#### Scenario: Latency values reported
- **WHEN** a CoreAudio device is opened
- **THEN** the reported latency includes both the device latency and safety offset in frames

### Requirement: CoreAudio Xrun Detection
The system SHALL detect IOProc overloads via `kAudioDeviceProcessorOverload` notifications. Xrun events SHALL be counted and surfaced to the GUI. The system SHALL implement recovery logic to resume clean audio after an overload.

#### Scenario: Xrun during playback
- **WHEN** the IOProc callback misses its deadline
- **THEN** the xrun counter increments and audio resumes without requiring a manual restart

### Requirement: CoreMIDI Integration
The system SHALL enumerate CoreMIDI devices and provide MIDI input and output streams on macOS. MIDI device names SHALL follow the same `coreaudio:<name>` convention.

#### Scenario: MIDI device discovery
- **WHEN** the engine initializes on macOS with CoreMIDI available
- **THEN** all MIDI input and output endpoints are listed and available for routing

### Requirement: CoreAudio Module Wiring
The system SHALL expose the CoreAudio backend behind `#[cfg(target_os = "macos")]` guards in `hw/mod.rs`, `engine.rs`, and `config.rs`. The `CoreAudioBackend` struct SHALL be type-aliased as `HwWorker` on macOS.

#### Scenario: macOS build selects CoreAudio backend
- **WHEN** the engine is compiled on macOS
- **THEN** the CoreAudio backend is included and selected as the default hardware worker
