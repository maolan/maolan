# Maolan

[![codecov](https://codecov.io/gh/maolan/maolan/graph/badge.svg)](https://codecov.io/gh/maolan/maolan)
[![crates.io](https://img.shields.io/crates/v/maolan.svg)](https://crates.io/crates/maolan)

![Maolan](images/maolan.svg)

Maolan is a Rust DAW focused on recording, editing, routing, automation, export, and plugin hosting.

[maolan.github.io](https://maolan.github.io)

![workspace](images/workspace.gif)

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
- HeartMuLa generation through `maolan-generate`, including token generation and in-process HeartCodec decode
- Session templates, track templates, autosave recovery, and diagnostics

## Platform Notes

- Unix builds support CLAP, VST3, and LV2.
- Current keyboard handling is `Ctrl`-based across platforms.
- Plugin compatibility is host-dependent and should be treated as evolving rather than guaranteed.

## Build

### Prerequisites

- Rust toolchain (edition 2024)

For Unix plugin/audio integrations, install platform libraries as needed (for example `jack`, `lilv`, `suil`, `gtk2`, `rust`, `cargo`, `rubberband` where applicable).

### Compile and run

The repository root is a single Cargo package (not a workspace).

```bash
cargo build --release
cargo run --release
```

The `generate/` directory contains a separate standalone crate. To build
it, run `cd generate && cargo build --release`.

### Debug logging

```bash
cargo run --release -- --debug
```

## Documentation

- [Features](docs/FEATURES.md)
- [Operations, Storage, and Recovery](docs/OPERATIONS.md)
- [Shortcuts and Mouse Gestures](docs/SHORTCUTS.md)
- [Plugin Routing and Sidechains](docs/PLUGIN_ROUTING.md)
- [History Audit Notes](docs/HISTORY_AUDIT.md)

## `maolan-generate`

`maolan-generate` is the current CLI path for HeartMuLa generation in this repo.

- Model downloads use Hugging Face cache resolution through `hf-hub`.
- The current Burn repos expected by the generate path are:
  - `maolandaw/HeartMuLa-happy-new-year-burn`
  - `maolandaw/HeartCodec-oss-20260123-burn`
- The HeartMuLa repo is expected to provide:
  - `heartmula.bpk`
  - `tokenizer.json`
  - `gen_config.json`
- The HeartCodec repo is expected to provide:
  - `heartcodec.bpk`

Current CLI capabilities include:

- Prompt/lyrics generation with optional tags
- Adjustable backend, sampler, CFG scale, steps, top-k, temperature, ODE steps, and decoder seed
- `--length <int>` output length in milliseconds
- `--decode-only` with `--frames-json`
- `--model-dir <path>` override for using a local Burn export instead of Hugging Face cache resolution

## Project Notes

- Preferences are stored in `~/.config/maolan/config.toml`.
- Session templates are stored under `~/.config/maolan/session_templates/`.
- Track templates are stored under `~/.config/maolan/track_templates/`.
- Autosave snapshots are stored under `<session>/.maolan_autosave/snapshots/`.

## Status

Maolan is under active development. Behavior and UI details may evolve between commits.
