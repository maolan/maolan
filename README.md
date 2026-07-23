# Maolan

[![crates.io](https://img.shields.io/crates/v/maolan.svg)](https://crates.io/crates/maolan)

![Maolan](assets/images/maolan.svg)

Maolan is a Rust DAW focused on recording, editing, routing, automation, export, and plugin hosting.

[maolan.github.io](https://maolan.github.io)

![workspace](assets/images/workspace.gif)

## Current Scope

Maolan currently includes:

- Audio and MIDI tracks with timeline editing
- Piano roll editing with note/controller/SysEx tools
- Track and plugin automation
- Plugin hosting for:
  - CLAP
  - VST3
  - LV2 on Unix
- Per-track plugin graph routing, including sidechains and MIDI paths
- Freeze, flatten, offline bounce, and export workflows
- Session templates, track templates, autosave recovery, and diagnostics

## Platform Notes

- Unix builds support CLAP, VST3, and LV2.
- Current keyboard handling is `Ctrl`-based across platforms.
- Plugin compatibility is host-dependent and should be treated as evolving rather than guaranteed.

## Build

### Prerequisites

- Rust toolchain (edition 2024)

For Unix audio integrations, install platform libraries as needed (for example `jack`, `alsa`, `rust`, and `cargo` where applicable).

### Windows

In the Windows environment execute the following:
`powershell -ExecutionPolicy Bypass -File "\\172.16.0.254\repos\maolan\maolan\scripts\build.ps1"`

### Compile and run (Unix)

`maolan/` is a Cargo workspace. Build from that directory:

```bash
cd maolan
cargo build --workspace --release
cargo run --release
```

### Debug logging

```bash
cd maolan
cargo run --release -- --log-level debug
```

## Documentation

- [Features](../doc/src/features.md)
- [Workflow](../doc/src/workflow.md)
- [Shortcuts and Mouse Gestures](../doc/src/shortcuts.md)
- [Plugin Routing and Sidechains](../doc/src/routing.md)
- [History Audit Notes](../doc/src/history-audit.md)

## Project Notes

- Preferences are stored in `~/.config/maolan/config.toml`.
- Session templates are stored under `~/.config/maolan/session_templates/`.
- Track templates are stored under `~/.config/maolan/track_templates/`.
- Autosave snapshots are stored under `<session>/.maolan_autosave/snapshots/`.

## Status

Maolan is under active development. Behavior and UI details may evolve between commits.
