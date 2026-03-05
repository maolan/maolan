# Maolan Feature Roadmap Tasks

Last updated: 2026-03-05

## Phase 1 - Core Editing and Automation

- [x] SysEx editing baseline (single-point edit, move, undo, wire pass-through)
- [x] Automation lanes (track + plugin parameters)
  - [x] Step 1: Track automation lane data model in UI state
  - [x] Step 2: Add UI action to create first lane (Volume) per track
    - [x] Track context menu adds Volume/Balance/Mute automation lanes
  - [x] Step 3: Draw automation points/curves in editor timeline and edit points
    - [x] Render visible automation lanes and point markers in timeline
    - [x] `A+` toggle to show/hide automation lanes per track
    - [x] Click lane to insert/update automation point
    - [x] Right-click point to delete
    - [x] Draw connecting ramp segments between adjacent points
  - [x] Step 4: Engine playback application for track automation
    - [x] Playback tick evaluates automation value at transport sample
    - [x] Sends non-history engine actions for Volume/Balance/Mute automation
    - [x] Runtime smoothing/thresholds to avoid excessive action spam
  - [x] Step 5: Plugin parameter automation targets + playback
    - [x] CLAP: add parameter automation lanes from loaded plugin (`Auto` button)
    - [x] CLAP: playback applies parameter lanes via `TrackSetClapParameterAt`
    - [x] VST3: parameter lane creation + playback path
    - [x] LV2: parameter lane creation + playback path
  - [x] Step 6: Automation write modes (Read/Touch/Latch/Write)
    - [x] Track automation mode field + UI cycle button
    - [x] Playback honors `Write` mode by skipping readback
    - [x] Manual control changes write automation points in Touch/Latch/Write
    - [x] Touch: temporary manual override over lane readback
    - [x] Touch: per-target gesture lifecycle (begin/end + targeted release clear)
    - [x] Latch: sticky manual override until stop/mode change
- [x] Quantize/Humanize/Groove
  - [x] Quantize selected MIDI notes in piano roll
  - [x] Humanize selected MIDI notes (time + velocity)
  - [x] Groove selected MIDI notes (swing)
  - [x] Piano UI controls (buttons + amount sliders + Q/H/G shortcuts)
- [ ] Tempo/Time Signature track editor
  - [x] First pass: interactive tempo/time-signature edit from timeline header
  - [x] Second pass: bidirectional edits (increment/decrement via click + scroll hints)
  - [x] Session/template persistence for time signature
- [ ] Comping/Take lanes (MIDI + audio)
  - [x] First pass: clip mute state in engine/UI + "Set Active Take" overlap comping action

## Phase 2 - Production Workflow

- [ ] Freeze/Commit/Flatten track
- [ ] Folder/Group/VCA-like controls
- [ ] MIDI power tools (scale, chord, legato, velocity shaping)
- [ ] Audio warp/time-stretch with markers

## Phase 3 - Control and Integration

- [ ] Control surface support and MIDI learn expansion
- [ ] Session diagnostics/performance tooling
- [ ] Optional MIDI 2.0 investigation track (separate from core roadmap)

## Notes

- Prioritize features that improve day-to-day composing/mixing before deep protocol work.
- Keep each feature split into engine, state/message, UI, undo, and tests.
