# Maolan Project Memory

## Key Files
- `engine/src/hw/alsa.rs` — ALSA audio backend (Linux)
- `engine/src/hw/oss/` — OSS audio backend (FreeBSD), multi-file
- `engine/src/engine.rs` — Main engine loop, calls `driver.set_playing()`
- `engine/src/hw_worker.rs` — Generic HW worker thread infrastructure
- `engine/src/hw/traits.rs` — `HwWorkerDriver`, `HwDevice`, `HwMidiHub` traits + macros

## Architecture Notes
- Engine calls `driver.lock().set_playing(bool)` directly on HwDriver (not via trait)
- ALSA uses blocking `readi`/`writei`; OSS uses non-blocking I/O with FrameClock/DuplexSync
- OSS backend is far more sophisticated (timing, prefill, xrun detection); ALSA is simpler

## ALSA vs OSS Parity (done 2026-02-25)
Brought ALSA on par with OSS by adding to `engine/src/hw/alsa.rs`:
- `set_playing(bool)` + `playing: Arc<AtomicBool>` — writes silence when not playing
- `prefill_playback()` — writes `nperiods * period_frames` silence at startup
- Xrun logging: `tracing::warn!` + `xrun_count: u64` counter; capture xruns zero buffer and continue
- Connected-only optimization: `!all_in_connected` passed to `fill_ports_from_interleaved`
- Playback: zeros unconnected channels before `write_interleaved_from_ports`
- `new()` shorthand constructor
