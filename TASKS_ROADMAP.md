# Maolan Feature Roadmap Tasks

Last updated: 2026-03-05

## Phase 1 - Core Editing and Automation

- [x] SysEx editing baseline (single-point edit, move, undo, wire pass-through)
- [ ] Automation lanes (track + plugin parameters)
  - [x] Step 1: Track automation lane data model in UI state
  - [x] Step 2: Add UI action to create first lane (Volume) per track
  - [ ] Step 3: Draw automation points/curves in editor timeline and edit points
  - [ ] Step 4: Engine playback application for track automation
  - [ ] Step 5: Plugin parameter automation targets + playback
  - [ ] Step 6: Automation write modes (Read/Touch/Latch/Write)
- [ ] Quantize/Humanize/Groove
- [ ] Tempo/Time Signature track editor
- [ ] Comping/Take lanes (MIDI + audio)

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
