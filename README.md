![Maolan](images/maolan.png)

# Maolan

Maolan is a Rust DAW focused on recording, editing, routing, and plugin hosting.

## What It Does

- Multi-track audio and MIDI timeline editing
- Recording, playback, loop/punch workflow, and WAV export
- Track mixer controls (level, pan/balance, mute, solo, arm)
- Audio and MIDI import
- Plugin graph per track with native/generic plugin UI
- Session save/load with portable project folders

## Plugin Support

- `LV2` on Unix-like systems
- `VST3` and `CLAP` on supported platforms

The plugin graph supports drag-connect routing with connection validation and cycle prevention.

## Platform Notes

- Linux: ALSA (JACK optional)
- FreeBSD: OSS (JACK optional)
- OpenBSD: sndio (JACK optional)
- macOS: CoreAudio
- Windows: WASAPI (ASIO via backend support)

## Build

### Prerequisites

- Rust toolchain (edition 2024)

For Unix plugin/audio integrations, install platform libraries as needed (for example `jack`, `lilv`, `suil`, `gtk2` where applicable).

### Compile and run

```bash
cargo build --release
cargo run --release
```

### Debug logging

```bash
cargo run --release -- --debug
```

## Quick Start

1. Create or open a session.
2. Add tracks and set inputs/outputs.
3. Arm tracks and record audio or MIDI.
4. Double-click a track to open its plugin graph.
5. Export the mix to WAV when done.

## Templates

- Save the current project structure with `File -> Save as template`.
- Create a new project from `File -> New -> <Template Name>`.
- Templates are stored at `~/.config/maolan/session_templates/`.
- Saved templates keep track layout and routing, but clear audio and MIDI clips.

## Common Shortcuts

- `Ctrl+N`: new session
- `Ctrl+O`: open session
- `Ctrl+S`: save session
- `Ctrl+Shift+S`: save as
- `Ctrl+I`: import
- `Ctrl+E`: export WAV
- `Ctrl+T`: new track
- `Space`: play/stop
- `Shift+Space`: pause
- `Delete` / `Backspace`: delete selection

## Status

Maolan is under active development. Behavior and UI details may evolve between commits.
