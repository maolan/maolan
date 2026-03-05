![Maolan](images/maolan.png)

# Maolan

Maolan is a Rust DAW focused on recording, editing, routing, and plugin hosting.

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

### Package release artifact

```bash
cargo build --release
./scripts/package-release.sh
```

The script creates a `dist/maolan-<timestamp>-<target>.tar.gz` bundle with the release binary and core docs.

### Audit undo/redo coverage

```bash
./scripts/audit-history-coverage.sh
```

Prints mutating `engine::message::Action` candidates not currently included in history `should_record`.
See [docs/HISTORY_AUDIT.md](docs/HISTORY_AUDIT.md) for the latest audit notes and decisions.

## Documentation

- [Features](docs/FEATURES.md)
- [Shortcuts and Mouse Gestures](docs/SHORTCUTS.md)
- [History Audit Notes](docs/HISTORY_AUDIT.md)

## Project Notes

- Session templates are stored under `~/.config/maolan/session_templates/`.
- Track templates are stored under `~/.config/maolan/track_templates/`.

## Status

Maolan is under active development. Behavior and UI details may evolve between commits.
