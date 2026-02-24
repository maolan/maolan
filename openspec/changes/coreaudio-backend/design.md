## Context
Maolan's engine abstracts audio hardware through platform-specific backends (ALSA, OSS, sndio, WASAPI). macOS currently only supports JACK, which is an optional third-party server. CoreAudio is the native audio subsystem on macOS and provides low-latency, callback-driven I/O through its HAL (Hardware Abstraction Layer) IOProc mechanism.

## Goals / Non-Goals
- Goals:
  - Provide zero-configuration audio I/O on macOS
  - Match or beat JACK round-trip latency using IOProc callbacks
  - Follow the existing HwDriver / HwWorker abstraction pattern
  - Support device hot-plug and sample-rate changes
- Non-Goals:
  - Replacing JACK support on macOS (JACK remains available)
  - Supporting AudioUnit plugin hosting (separate from hardware I/O)
  - iOS / tvOS / visionOS support

## Decisions
- **IOProc callback model**: Use `AudioDeviceCreateIOProcID` / `AudioDeviceStart` rather than the higher-level AudioQueue API. IOProc provides the lowest latency path and direct buffer access, analogous to ALSA mmap mode.
  - Alternatives considered: AudioQueue (higher latency, extra buffering), AVAudioEngine (Objective-C dependency, unnecessary abstraction layer).

- **SharedIOState condvar bridge**: The HAL IOProc runs on a real-time thread that must not block. A `SharedIOState` struct uses a lock-free ring buffer for sample data and a condvar to wake the engine worker thread. The IOProc writes into the ring buffer and signals the condvar; the worker thread waits on the condvar and reads from the ring buffer.
  - Alternatives considered: Channel-based (mpsc adds allocation), direct processing in IOProc (violates engine threading model).

- **coreaudio-sys crate**: Use the `coreaudio-sys` crate for raw FFI bindings rather than hand-written bindings. This crate is well-maintained and covers the full CoreAudio C API surface.

- **CoreMIDI via coremidi crate**: Use the `coremidi` Rust crate for MIDI I/O rather than raw FFI, as it provides a safe, idiomatic wrapper.

## Risks / Trade-offs
- HAL IOProc runs on a real-time thread with strict timing constraints. Any lock contention in `SharedIOState` could cause audio glitches. Mitigation: lock-free ring buffer for the hot path; condvar only for waking the worker.
- `coreaudio-sys` tracks Apple SDK versions; breaking changes in new macOS releases could require crate updates. Mitigation: pin crate version, test on latest macOS in CI.
- Device removal during playback can crash if not handled. Mitigation: register `kAudioDevicePropertyDeviceIsAlive` listener and gracefully stop the stream.

## Open Questions
- Exact ring buffer sizing strategy (fixed frames vs. adaptive based on IOProc buffer size).
- Whether to support aggregate devices or only physical devices initially.
