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
- NetBSD: audio(4) (JACK optional)
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
- `Ctrl+Z`: undo
- `Ctrl+Shift+Z` or `Ctrl+Y`: redo
- `Space`: play/stop
- `Shift+Space`: pause
- `Delete` / `Backspace`: delete selection
- `Q`: quantize selected MIDI notes (Piano view)
- `H`: humanize selected MIDI notes (Piano view)
- `G`: groove selected MIDI notes (Piano view)

## Phase 1 Shortcuts and Gestures

- `A+` button on track header: toggle automation lane visibility for that track
- Tempo lane:
  - left-click marker: select marker
  - `Shift+click` marker: multi-select markers
  - left-drag marker: move selected marker(s) on timeline
  - middle-click lane: add tempo/time-signature marker at cursor
  - right-click marker: marker menu (`Duplicate`, `Reset to Previous`, `Delete`)
- Comping:
  - toolbar `SEL/COMP` button: switch between normal edit and comp swipe mode
  - in `COMP` mode, left-drag across overlap area: set active take by swipe region

## Phase 1 Workflow Guide

### 1. SysEx Editing (Baseline)

1. Open a MIDI clip in Piano view.
2. Switch controller lane to `SysEx` from the lane menu.
3. Add/select/move SysEx events on the controller timeline.
4. Use the SysEx panel to `Add`, `Update`, or `Delete` event data.

### 2. Automation Lanes (Track + Plugin)

1. On a track, add automation lanes from context options (Volume/Balance/Mute) or plugin `Auto`.
2. Click lane to add/update points; right-click points to delete.
3. Use `A+` to show/hide automation lanes.
4. Choose automation mode (`Read`, `Touch`, `Latch`, `Write`) on the track.
5. During playback, automation writes/readback are applied according to mode.

### 3. Quantize / Humanize / Groove

1. In Piano view, select MIDI notes.
2. Press:
   - `Q` for quantize
   - `H` for humanize
   - `G` for groove
3. Adjust amounts in Piano controls and re-apply as needed.

### 4. Tempo / Time Signature Track Editor

1. Use tempo lane controls (top timeline strip):
   - typed BPM/TS in toolbar
   - marker add/select/move/delete/duplicate/reset
2. Tempo and time-signature points are evaluated at playback position.
3. Engine transport timing is synced so plugins receive current BPM + TS.

### 5. Comping / Take Lanes (MIDI + Audio)

1. Overlapping clips are stacked as take lanes.
2. Use clip context menu:
   - `Set Active Take`, `Next Active Take`, `Unmute All Takes`
   - `Pin/Unpin Take Lane`
   - `Lock/Unlock Take Lane`
   - `Take Lane Up/Down` (reorder)
3. Use toolbar `COMP` mode and swipe across overlaps to comp quickly.
4. Locked takes are protected from comp/resize/drag edits.

## Status

Maolan is under active development. Behavior and UI details may evolve between commits.
