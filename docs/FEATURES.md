# Maolan Features

Last updated: 2026-04-07

## Core DAW Workflow
- Multi-track audio + MIDI session editing
- Track selection, reordering, resizing, and rename
- Session template save/load
- Track template save/load
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
- Clip group / ungroup on same-track, same-type selections
- Nested clip groups preserved on save/load and restore on ungroup
- Clip edge resize (left/right)
- Clip fade-in/fade-out resize
- Clip mute/unmute
- Clip rename
- Clip middle-click split at cursor/snap point
- Grouped clips cannot be split until ungrouped
- MIDI clip double-click opens piano roll
- Audio clip double-click opens per-clip plugin graph on Unix
- Per-clip FX/plugin graph for audio clips:
  - Separate from the track plugin graph
  - Stored per clip in session data
  - Applies after grouped-child summing and clip fades in the current render path
  - Supports CLAP, VST3, and LV2 on supported Unix builds
- Audio clip pitch-correction editor
- Pitch-correction workflow:
  - Open from the audio clip context menu when transport is stopped
  - Analyze source material with per-session cached pitch scans for fast revisits
  - Reopen existing clip correction without re-analyzing when correction data is already saved on the clip
  - Tune detection granularity using frame-likeness (0.05–2.00) merge behavior
  - Manual pitch-segment retargeting with click, shift-click, marquee select, and grouped vertical drag editing
  - Double-click snap-to-nearest-note for one or many selected pitch segments
  - Local pitch-correction undo/redo history while editing
  - Adjustable inertia (0–1000 ms) for smoother pitch transitions between segments
  - Optional formant compensation in render path
  - Apply pitch correction from full clip or selected source offset/length window
  - Non-destructive clip workflow: edits stay local to the editor until Apply writes them back to the clip
  - Real-time pitch-shift playback after Apply
  - Offline pitch-corrected freeze preparation using Rubber Band preview renders
- Audio warp markers with reset / half-speed / double-speed helpers

## Timeline Markers and Arrangement Aids
- Per-track editor markers
- Marker create / rename / move / delete workflow
- Snap-aware marker placement and marker dragging
- Snap-to-clip start/end for clips, loop, punch, and markers
- Ruler playhead seek
- Loop range set/clear
- Punch range set/clear

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
- Configurable scale root / major-minor mode
- Configurable chord type
- Configurable groove, humanize, and velocity-shape amounts

## Timing and Transport
- Tempo and time-signature timeline editing
- Tempo/time-signature points add/move/delete/duplicate/reset
- Numeric tempo/time-signature input with validation
- Play/pause/stop/record transport control
- GUI transport shortcuts for rewind-to-start/end, session record arm toggle, and panic
- CLI transport shortcuts for rewind-to-start/end and panic
- Toolbar panic button for hardware MIDI outputs (`CC64=0`, `CC120=0`, `CC123=0`)
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

## AI Audio Generation
- HeartMuLa text-to-audio generation through `maolan-generate`
- Prompt / lyrics driven generation with optional style tags
- Burn backend options:
  - CPU
  - Vulkan
- Model options:
  - `happy-new-year`
  - `RL`
- Burn generation controls:
  - CFG scale
  - Steps
  - Top-k
  - Temperature
  - ODE steps
  - Decoder seed
  - Output `--length` in milliseconds
- Decode-only mode from a saved frames JSON
- Decode-only CPU worker override with `--decode-threads`
- Hugging Face cache-backed model resolution for the current Burn repos:
  - `maolandaw/HeartMuLa-happy-new-year-burn`
  - `maolandaw/HeartCodec-oss-20260123-burn`
- Local model directory override with `--model-dir`

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
- Preferences saved to `~/.config/maolan/config.toml`
- Recent sessions menu

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
- Linux and FreeBSD builds support CLAP, VST3, and LV2.
- macOS builds refresh CLAP and VST3 plugin support paths; LV2 is Unix-only in the current codebase.
- Linux and FreeBSD builds currently force the X11 window backend at startup.
- FreeBSD roadmap notes still mark MIDI 2.0 as N/A.

## Known Boundaries
- Plugin compatibility is still a real-world host-interop concern, especially across unusual plugins.
- Many core behaviors are unit-tested, but editor-hosting and plugin-integration paths are still more integration-heavy than fixture-heavy.
