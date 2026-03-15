# maolan-engine

`maolan-engine` is the Rust audio engine that powers Maolan.

It provides:

- Audio and MIDI track processing
- Timeline-oriented recording and clip playback
- Track routing and plugin graph routing
- Offline bounce and export helpers
- Plugin hosting for CLAP and VST3, plus LV2 on Unix platforms
- Platform audio backends for Linux, macOS, and FreeBSD

This crate is under active development alongside the main Maolan application:

- Repository: <https://github.com/maolan/maolan>

Platform integrations depend on system libraries and host/plugin compatibility.
