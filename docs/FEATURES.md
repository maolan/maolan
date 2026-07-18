# Maolan Features

Last updated: 2026-07-09

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
- Clip edge resize (left/right); MIDI clips can be extended past their content length (empty tail), audio clips are bounded by source length
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
  - Offline pitch-corrected freeze preparation using timestretch preview renders
- Audio warp markers with reset / half-speed / double-speed helpers

## Folder Tracks
- Folder tracks for hierarchical track organization
- Collapsible/expandable folder view in the track list
- Child tracks route through the folder track and are connectable to folder plugins
- Folder track plugin graph for processing the summed child output
- Child strips in the mixer and live view show highlighted top and bottom edges when their immediate parent folder is selected; the far-right child also gets a highlighted right edge
- Track-to-folder plugin connections and disabled folder-output feeds
- Folder track templates that save the full child subtree recursively

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
- Automation lane editing with MIDI-style gestures (right-drag to draw ramps, click to insert points)
- Automation modes:
  - Read
  - Touch
  - Latch
  - Write
- Automation writeback from manual control changes
- Plugin automation lane creation from loaded plugin parameters

## Modulators
- LFO-style modulators per session
- Modulator shapes: Sine, Triangle, Saw, Square, Sample & Hold
- Rate modes: Hz or musical divisions (beat, bar, etc.)
- Per-modulator phase offset and enable toggle
- Assign modulators to track parameters, plugin parameters, and MIDI controllers
- Per-assignment min/max range scaling via the modulator target dialog
- Modulators pane for creating, selecting, and editing modulators

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
- MIDI note snapping
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
- Metronome enable/disable with visual icon
- Clip playback enable/disable at transport level

## Step Recording
- Step recording mode in the piano roll
- Toggle step recording from the MIDI editor toolbar (`Step` button)
- Each incoming MIDI note is inserted at the step cursor and advances the cursor by the current MIDI snap interval
- Step cursor drawn in the piano roll while the mode is active

## Live Session View
- Clip-launch grid modeled after Ableton Live
- Switch between Workspace and Session view with `Tab` or the toolbar
- Each row is a track; each column is a scene
- Slots reference arrangement clips by stable clip ID
- Launch/stop slots with click or `Return`
- Launch scenes by clicking the Master column scene slots; the clicked scene is selected (while stopped, it starts on the next play; while playing, it launches when the current scene finishes)
- Per-scene tempo changes on launch
- Stop all clips with `Shift+Space` or the master stop column
- Move, duplicate, and copy slot references between slots and the arrangement
- Assign arrangement clips to session slots
- Clear a slot's clip with middle-click on the slot
- Child strips show highlighted top and bottom edges when their immediate parent folder is selected; the far-right child also gets a highlighted right edge (same cue as the mixer)
- Import the arrangement into the session grid
- Record session clips into the arrangement (`Rec to Arr`)
- Record directly into a slot
- Scene options: rename, set color, set per-scene tempo, change launch quantization
- Track strip with mute, solo, arm, volume, and pan controls

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
  - Phase invert
- Aux return creation from selected tracks
- Aux send controls:
  - Level adjust
  - Pan adjust
  - Pre/Post fader toggle
- Track audio/MIDI passthrough defaults
- Per-track plugin graph routing for:
  - Audio
  - MIDI
  - Sidechain / auxiliary plugin ports
- Plugin window lifecycle and focus management

## Plugins and Integration
- CLAP plugin scan/load/unload/state restore/UI
- VST3 plugin scan/load/unload/state restore/UI
- LV2 plugin scan/load/unload/state restore/UI (Unix)
- Plugin graph and per-track routing management
- Mixed plugin graph session/template restore
- Plugin parameter automation for loaded plugins
- Offline plugin bounce path for freeze/export workflows
- Plugin blocklist (`~/.config/maolan/plugin-blocklist.json`) to hide problematic plugins from scans

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
  - MP3 (via ffmpeg)
  - OGG (Vorbis, via ffmpeg)
  - FLAC
- Codec settings:
  - MP3 mode (CBR/VBR) + bitrate
  - OGG quality
- Metadata tagging on supported formats:
  - MP3 (ID3 fields used by current encoder path)
  - OGG Vorbis comments

## Control Surface and OSC
- mixosc integration for OSC-based mixing control
- Behringer X32 dedicated view

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

## Media Management and File References
- Collect/consolidate external media and plugin file references into the session's `data/` directory
- Delete unused session media files from `audio/`, `midi/`, `peaks/`, and `pitch/`
- Clips deleted from tracks are kept in a session-level unused pool (shown in the Clips pane) until **File → Delete unused files** removes them permanently
- CLAP and LV2 file-reference support: plugins can declare external file references and Maolan updates them to session-relative `data/` paths on consolidate
- Consolidation makes sessions safer to move or share

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

## Widgets and UI Components
- `arch_slider` with adjustable arc shape
- Slider `on_release` option for both horizontal and vertical variants
- Stretchable ticks and meters that fill available space

## Platform Notes
- Linux and FreeBSD builds support CLAP, VST3, and LV2.
- Windows builds support WSAPI backend, CLAP, and VST3.
- macOS builds refresh CLAP and VST3 plugin support paths; LV2 is Unix-only in the current codebase.
- Linux and FreeBSD builds run on Wayland when available and fall back to X11 (Xorg) when Wayland is unavailable.
- Plugin UI embedding on Unix still uses X11, so an X11 server must be reachable even under Wayland (for example via XWayland).
- FreeBSD roadmap notes still mark MIDI 2.0 as N/A.

## Known Boundaries
- Plugin compatibility is still a real-world host-interop concern, especially across unusual plugins.
- Many core behaviors are unit-tested, but editor-hosting and plugin-integration paths are still more integration-heavy than fixture-heavy.
