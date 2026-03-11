# Maolan Features

Last updated: 2026-03-11

## Core DAW Workflow
- Multi-track audio + MIDI session editing
- Track selection, reordering, resizing, and rename
- Track templates and session templates
- Session save/open/save-as
- Recent session tracking
- Session metadata editing:
  - Author
  - Album
  - Year (unsigned)
  - Track number (unsigned)
  - Genre

## Clip Editing
- Audio clip and MIDI clip placement on timeline
- Clip select/multi-select and marquee selection
- Clip drag-and-drop move/copy across tracks
- Clip edge resize (left/right)
- Clip fade-in/fade-out resize
- Clip mute/unmute
- Clip rename
- Clip middle-click split at cursor/snap point
- MIDI clip double-click opens piano roll
- Audio warp markers with reset / half-speed / double-speed helpers

## Take Lanes and Comping
- Overlap-based take lane stacking
- Set active take
- Cycle to next active take
- Unmute all takes in range
- Take lane pin/unpin
- Take lane lock/unlock
- Take lane move up/down
- Dedicated Comp tool with swipe comping

## Automation
- Track automation lanes:
  - Volume
  - Balance
  - Mute
- Plugin automation lanes:
  - CLAP parameters
  - VST3 parameters
  - LV2 parameters (Unix)
- Lane point insert/delete/edit
- Automation ramp drawing
- Automation modes:
  - Read
  - Touch
  - Latch
  - Write
- Automation writeback from manual control changes
- Plugin automation lane creation from loaded plugin parameters

## Piano Roll / MIDI Tools
- Note editing and selection
- Velocity/controller editing
- SysEx point create/edit/move/delete
- Quantize
- Humanize (time + velocity)
- Groove (swing)
- Scale snap
- Chord generation
- Legato
- Velocity shaping

## Timing and Transport
- Tempo and time-signature timeline editing
- Tempo/time-signature points add/move/delete/duplicate/reset
- Numeric tempo/time-signature input with validation
- Play/pause/stop/record transport control
- Loop range set/clear
- Punch range set/clear
- Metronome enable/disable
- Clip playback enable/disable at transport level

## Freeze / Commit / Flatten
- Per-track freeze/unfreeze
- Freeze rendering with progress + cancel
- Reversible frozen backups
- Flatten frozen track (keep rendered audio, drop backups)
- Freeze guardrails around mutable track operations

## Routing and Mixing
- Track controls:
  - Arm
  - Mute
  - Solo
  - Input monitor
  - Disk monitor
- Aux return creation from selected tracks
- Aux send controls:
  - Level adjust
  - Pan adjust
  - Pre/Post fader toggle
- VCA-like master assignment/unassignment for tracks
- Track audio/MIDI passthrough defaults
- Per-track plugin graph routing for:
  - Audio
  - MIDI
  - Sidechain / auxiliary plugin ports

## Plugins and Integration
- CLAP plugin scan/load/unload/state restore/UI
- VST3 plugin scan/load/unload/state restore/UI
- LV2 plugin scan/load/unload/state restore/UI (Unix)
- Plugin graph and per-track routing management
- Mixed plugin graph session/template restore
- Plugin parameter automation for loaded plugins
- Offline plugin bounce path for freeze/export workflows

## Export and Render
- Mixdown and stem export (selected tracks)
- Stem modes:
  - Pre-fader
  - Post-fader
- Real-time fallback render mode
- Export master limiter + dBTP ceiling
- Normalization modes:
  - Peak
  - Loudness
- Multi-format export in one run:
  - WAV
  - MP3
  - OGG (Vorbis)
  - FLAC
- Codec settings:
  - MP3 mode (CBR/VBR) + bitrate
  - OGG quality
- Metadata tagging on supported formats:
  - MP3 (ID3 fields used by current encoder path)
  - OGG Vorbis comments

## Session Safety and Recovery
- Dirty-state tracking
- Close guard for unsaved changes
- Periodic autosave snapshots
- Startup autosave recovery prompt
- Recover latest or older autosave snapshot
- Recovery preview summary
- Fallback to older snapshots on recovery failure
- Startup recovery hint from last opened session

## Diagnostics and Tooling
- Session diagnostics report
- Diagnostics bundle export
- MIDI mappings report
- MIDI mappings import/export/clear-all
- History coverage audit script (`scripts/audit-history-coverage.sh`)
- Release packaging script (`scripts/package-release.sh`)

## MIDI Learn and Control Surface Features
- Track MIDI learn targets:
  - Volume
  - Balance
  - Mute
  - Solo
  - Arm
  - Input monitor
  - Disk monitor
- Global MIDI learn targets:
  - Play/Pause
  - Stop
  - Record toggle
- Mapping persistence in session
- Collision/conflict protection

## Platform Notes
- Unix builds support CLAP, VST3, and LV2.
- Windows builds support CLAP and VST3.
- FreeBSD roadmap notes still mark MIDI 2.0 as N/A.

## Known Boundaries
- Plugin compatibility is still a real-world host-interop concern, especially across unusual plugins.
- Many core behaviors are unit-tested, but editor-hosting and plugin-integration paths are still more integration-heavy than fixture-heavy.
