# Maolan Features

Last updated: 2026-03-05

## Core DAW Workflow
- Multi-track audio + MIDI session editing
- Track selection, reordering, resizing, and rename
- Track templates and session templates
- Session save/open/save-as
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

## Plugins and Integration
- CLAP plugin scan/load/unload/UI
- VST3 plugin scan/load/UI
- LV2 plugin scan/load/UI (Unix)
- Plugin graph and per-track routing management
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
- MIDI 2.0 roadmap item is marked N/A on FreeBSD in current project roadmap.
