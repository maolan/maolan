## 1. Epic 1 — Foundation
- [ ] 1.1 Add coreaudio-sys + coremidi deps to engine/Cargo.toml
- [ ] 1.2 Wire coreaudio module into hw/mod.rs, engine.rs, config.rs
- [ ] 1.3 Create CoreAudioBackend struct in coreaudio_worker.rs
- [ ] 1.4 Implement AudioDeviceID enumeration in hw/coreaudio/device.rs

## 2. Epic 2 — Buffer Access
- [ ] 2.1 Implement IOProc callback registration and deregistration
- [ ] 2.2 Create SharedIOState condvar bridge
- [ ] 2.3 Implement buffer copy between IOProc and worker thread

## 3. Epic 3 — Latency Compensation
- [ ] 3.1 Query kAudioDevicePropertyLatency and safety offset
- [ ] 3.2 Plumb latency values into engine compensation pipeline

## 4. Epic 4 — Xrun Handling
- [ ] 4.1 Register kAudioDeviceProcessorOverload listener
- [ ] 4.2 Surface xrun count and implement recovery

## 5. Epic 5 — MIDI
- [ ] 5.1 Implement CoreMIDI device enumeration
- [ ] 5.2 Implement MIDI input and output streams

## 6. Epic 6 — Polish
- [ ] 6.1 Integration testing on macOS hardware
- [ ] 6.2 Sample-rate switching and default-device notifications
- [ ] 6.3 Error reporting and documentation
