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
- [x] Tempo/Time Signature track editor
  - [x] First pass: interactive tempo/time-signature edit from timeline header
  - [x] Second pass: bidirectional edits (increment/decrement via click + scroll hints)
  - [x] Session/template persistence for time signature
  - [x] Engine transport timing sync (plugins receive current BPM + time signature)
  - [x] Tempo/time-signature change events on timeline (not just global current values)
  - [x] Direct numeric editing UI (typed BPM/TS with validation)
- [x] Comping/Take lanes (MIDI + audio)
  - [x] First pass: clip mute state in engine/UI + "Set Active Take" overlap comping action
  - [x] Second pass: visual take-lane stacking for overlapping clips in timeline
  - [x] Third pass: overlap-group actions (`Next Active Take`, `Unmute All Takes`)
  - [x] Explicit take lane controls (pin/lock lane, reorder lanes)
  - [x] Dedicated comp tool/edit mode for swipe comping

## Phase 2 - Production Workflow

- [x] Freeze/Commit/Flatten track
  - [x] First pass: reversible per-track freeze state (UI + engine + undo + session restore)
  - [x] Freeze guardrails for plugin/routing/arm operations
  - [x] Freeze rendering workflow (audio render + reversible clip swap)
  - [x] Flatten frozen track workflow (keep rendered audio, discard backups, unfreeze)
  - [x] Commit/freeze rendering including plugin-processed track offline bounce
  - [x] Freeze bounce automation bake + progress reporting + in-flight cancel
- [x] Folder/Group/VCA-like controls
  - [x] Per-track VCA master assignment/unassignment in track context menu
  - [x] VCA master propagates level/mute/solo changes to assigned tracks
  - [x] VCA assignments persist through rename/remove and session restore
- [x] MIDI power tools (scale, chord, legato, velocity shaping)
  - [x] Scale snap selected notes (root + major/minor)
  - [x] Chord generation from selected notes (triad/7th presets)
  - [x] Legato selected notes to next note start
  - [x] Velocity shaping over selected note range
- [x] Audio warp/time-stretch with markers
  - [x] Per-audio-clip warp marker data model + session persistence
  - [x] Engine playback applies piecewise warp mapping for time-stretched clip reads
  - [x] First-pass clip context actions (reset, half-speed, double-speed, add marker)

## Phase 3 - Control and Integration

- [x] Control surface support and MIDI learn expansion
  - [x] First-pass MIDI CC learn for track volume/balance from hardware input
  - [x] Expanded MIDI CC learn targets for track mute/solo/arm
  - [x] Expanded MIDI CC learn targets for input monitor/disk monitor
  - [x] Global MIDI CC learn for transport play/pause/stop/record-toggle
  - [x] MIDI mappings report command to inspect active learned bindings
  - [x] MIDI mappings side panel (toggle + refresh + clear-all shortcuts)
  - [x] MIDI learn conflict detection (reject duplicate CC binding collisions)
  - [x] MIDI mappings import/export (`midi_mappings.json`)
  - [x] Clear-all MIDI mappings command
  - [x] Per-track learn arm/clear controls in track context menu
  - [x] Session persistence + restore for learned mappings
- [x] Session diagnostics/performance tooling
  - [x] Engine diagnostics snapshot action (tracks/clips/plugins/workers/MIDI queue/transport/audio cycle)
  - [x] UI trigger via menu (`Edit -> Session Diagnostics`)
  - [x] Diagnostics report surfaced in status area
- [x] Optional MIDI 2.0 investigation track (separate from core roadmap) - N/A on FreeBSD (platform support unavailable)

## Phase 4 - Product Completion (Except MIDI 2.0)

- [x] Session safety baseline
  - [x] Dirty-state tracking in GUI (window title `*` marker)
  - [x] Safe close guard for unsaved sessions (confirm-on-second-close)
  - [x] Clear dirty flag on successful save/session restore
- [x] Crash recovery and autosave journal
  - [x] Periodic autosave snapshot
  - [x] Recovery prompt on next launch
  - [x] Autosave retention and cleanup policy
  - [x] Startup recovery hint from last opened session
  - [x] Multi-generation autosave snapshots + older snapshot selection
  - [x] Recovery preview summary before apply
  - [x] Corruption fallback (attempt older snapshots automatically)
- [x] Export/render completeness
  - [x] Stem export (selected tracks, pre/post-fader)
  - [x] Real-time export fallback for offline-incompatible plugins
- [x] Routing/mix completeness
  - [x] Aux sends/returns workflow (first-pass helper: create aux return from selected tracks)
  - [x] Master chain metering/limiter slot polish (export master limiter + dBTP ceiling control)
- [x] Undo/redo audit completion
  - [x] Full action coverage audit for recent features
  - [x] Deterministic grouping for complex multi-step actions
    - [x] Grouped MIDI mappings import into a single undo/redo history step
    - [x] New Session now runs as a restore transaction (no history spam)
  - [x] Added automated history coverage audit script (`scripts/audit-history-coverage.sh`)
- [x] Productization
  - [x] Preferences screen for audio/MIDI/session defaults
  - [x] Packaging/release artifacts
  - [x] Diagnostics bundle export

## Notes

- Prioritize features that improve day-to-day composing/mixing before deep protocol work.
- Keep each feature split into engine, state/message, UI, undo, and tests.
