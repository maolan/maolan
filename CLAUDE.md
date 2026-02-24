<!-- OPENSPEC:START -->
# OpenSpec Instructions

These instructions are for AI assistants working in this project.

Always open `@/openspec/AGENTS.md` when the request:
- Mentions planning or proposals (words like proposal, spec, change, plan)
- Introduces new capabilities, breaking changes, architecture shifts, or big performance/security work
- Sounds ambiguous and you need the authoritative spec before coding

Use `@/openspec/AGENTS.md` to learn:
- How to create and apply change proposals
- Spec format and conventions
- Project structure and guidelines

Keep this managed block so 'openspec update' can refresh the instructions.

<!-- OPENSPEC:END -->

# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is Maolan

Maolan is a digital audio workstation (DAW) built in Rust. It is a Cargo workspace with two crates:

- **`maolan`** (`src/`) — the GUI application using the [iced](https://github.com/iced-rs/iced) framework
- **`maolan-engine`** (`engine/src/`) — the real-time audio/MIDI engine that runs on a separate async task

## Build & Run

```sh
# Build
cargo build

# Run
cargo run

# Run with debug logging
cargo run -- --debug

# Release build
cargo build --release
```

There are no automated tests in the repository.

## Platform-Specific Dependencies

Audio backend support is conditional on the target OS:

| OS | Audio backends |
|----|---------------|
| Linux | ALSA, JACK, lilv (LV2) |
| FreeBSD | OSS, JACK, lilv (LV2) |
| OpenBSD | sndio, JACK, lilv (LV2) |
| Windows | WASAPI, CPAL, midir |
| macOS | JACK only (no native backend) |

`lilv` (LV2 plugin host) is only compiled on Unix targets.

## Architecture

### Engine / GUI split

The engine (`maolan-engine`) runs as a long-lived Tokio task. The GUI spawns it via `maolan_engine::init()` which returns an `mpsc::Sender<engine::message::Message>`. The GUI communicates with the engine exclusively through this channel using `Action` variants (defined in `engine/src/message.rs`). The engine sends responses back through the same channel as `Message::Response(Result<Action, String>)`.

A static `CLIENT: LazyLock<engine::client::Client>` in `src/gui/mod.rs` wraps the sender and is used by `Maolan::send(action)` to dispatch all engine requests as iced `Task`s.

### GUI architecture (iced)

The root widget is `Maolan` (in `src/gui/mod.rs`), following the iced Elm-style architecture:
- `Maolan::update` — handles `Message` variants; defined in `src/gui/update.rs`
- `Maolan::view` — builds the widget tree; defined in `src/gui/view.rs`
- `Maolan::subscription` — subscribes to keyboard/mouse/timer events; defined in `src/gui/subscriptions.rs`

Shared UI state is held in `Arc<RwLock<StateData>>` (see `src/state/mod.rs`). Child widgets (`Workspace`, `connections::canvas_host::CanvasHost`, `hw::HW`, etc.) hold a clone of this `Arc` and read/write it directly. All child widgets receive a broadcast of every `Message` via `Maolan::update_children`.

### Views

Three top-level views (toggled by `src/state::View`):
- **Workspace** (`src/workspace/`) — timeline editor with tracks, clip editor, mixer, ruler, tempo
- **Connections** (`src/connections/`) — node-graph view for routing audio/MIDI between tracks and hardware
- **TrackPlugins** — LV2 plugin chain graph for a single track (rendered inline in `src/gui/mod.rs`)

### Engine internals

- `engine/src/engine.rs` — main engine loop; owns `Track` instances and dispatches processing to `Worker` tasks
- `engine/src/track.rs` — per-track state (clips, LV2 plugin chain, routing)
- `engine/src/audio/` and `engine/src/midi/` — clip data structures and I/O
- `engine/src/hw/` — hardware abstraction layer (ALSA/OSS/sndio/WASAPI/JACK); platform implementations are conditionally compiled
- `engine/src/lv2.rs` — LV2 plugin discovery and hosting via `lilv`
- `engine/src/routing.rs` — audio signal routing between tracks and hardware

### Session format

Sessions are directories. A session contains:
- `session.json` — track list, clip positions, connections, LV2 graphs, transport settings
- `audio/` — WAV files (imported or recorded)
- `midi/` — MIDI files
- `peaks/` — pre-computed peak cache files (`<track>_<idx>_<clip>.json`) used for waveform drawing

Audio peak files use the format `{"peaks": [[ch0_values...], [ch1_values...]]}`. Legacy single-channel files store a flat array `{"peaks": [...]}` and are read back-compatibly.

### Message flow summary

```
User action → iced Message → Maolan::update → Maolan::send(Action)
                                            → ENGINE (async task)
                                            → Message::Response(Action)
                                            → Maolan::update
```
