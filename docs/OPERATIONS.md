# Maolan Operations, Storage, and Recovery

Last updated: 2026-07-09

## Runtime and Platform Behavior

- Linux and FreeBSD builds run on Wayland when available and fall back to X11 (Xorg) when Wayland is unavailable.
- Plugin UI embedding on Unix still uses X11, so an X11 server must be reachable even under Wayland (for example via XWayland).
- macOS builds support CLAP and VST3, but not the Unix LV2 host path.
- Plugin discovery runs automatically on startup:
  - Linux / FreeBSD: LV2, VST3, CLAP
  - macOS: VST3, CLAP
- Plugin discovery path overrides:
  - `CLAP_PATH`: additional CLAP scan roots (path-list format, platform separator)
  - `VST3_PATH`: additional VST3 scan roots (path-list format, platform separator)
- CLAP discovery currently scans recursively and recognizes plugin binaries by extension:
  - `.clap`
  - `.so`
  - `.dylib`
  - `.dll`
- When using `CLAP_PATH`, point it at a dedicated plugin directory that only contains plugin binaries.
  - Example (Linux/FreeBSD): `CLAP_PATH=/home/user/plugins`
  - Avoid broad build output roots when possible (for example full Cargo target trees) to reduce unrelated binary probing.
- Passing `--log-level <level>` enables tracing output to stderr. Valid levels: none, info, warning, error, debug.

## Audio Device Filtering

The hardware preference dialog filters devices by direction:

- Output device lists show only devices that report output support.
- Input device lists show only devices that report input support.
- Devices that only support one direction no longer appear in the opposite list.

## Plugin Blocklist

Maolan maintains a plugin blocklist at:

`~/.config/maolan/plugin-blocklist.json`

The blocklist hides plugins from the plugin browser and causes the scanner to skip them entirely during system scans, so a known-bad plugin cannot crash `maolan-plugin-host --scan`. You can add entries manually, or the DAW can add them automatically when a single-plugin scan fails, the scanner reports warnings or errors for a plugin, or the scanner host crashes.

A blocklist entry uses the plugin path as the key:

```json
{
  "entries": [
    {
      "path": "/usr/lib/clap/CrashingPlugin.clap",
      "error": "scanner exited with code ...",
      "timestamp": "2026-06-30T11:14:01+00:00"
    }
  ]
}
```

Match rules per format:

- **CLAP** and **VST3**: match the full plugin file path (`path` field in the scan result).
- **LV2**: match either the `bundle_uri` (e.g. `file:///usr/lib/lv2/SomePlugin.lv2/`) or the plugin `uri`.

How completely the blocklist protects the scanner depends on the format:

- **CLAP** and **VST3**: the scanner skips the bundle before loading or inspecting it, so the bad code never runs.
- **LV2**: the scanner filters blocklisted plugins *after* the LV2 world has loaded and parsed the bundle. The plugin is hidden from the UI, but a crash inside the LV2 discovery layer could still happen before the filter runs.

Restart Maolan after editing the file. The status message after plugin discovery reports how many plugins were loaded and how many were blocklisted.

## Configuration File

Maolan stores UI and preference state in:

`~/.config/maolan/config.toml`

Current persisted keys:

- `font_size`
- `mixer_height`
- `track_width`
- `osc_enabled`
- `default_export_sample_rate_hz`
- `default_snap_mode`
- `default_audio_bit_depth`
- `default_output_device_id`
- `default_input_device_id`
- `recent_session_paths`

If the file does not exist, Maolan creates it on startup with defaults.

## Session Directory Layout

A saved session is a directory containing:

- `<branch>.json`: main project state for the current branch (default: `main.json`)
- `session.json`: Live Session View scene and slot data
- `audio/`: imported or rendered audio stored in-session
- `midi/`: copied MIDI files used by clips
- `peaks/`: cached waveform peak data written on save
- `pitch/`: cached pitch-analysis JSON files keyed by source clip path + source window
- `plugins/`: plugin-related session assets
- `data/`: consolidated external audio, MIDI, and plugin file references
- `.maolan_commits/<branch>/<timestamp>.json`: commit history created on every save
- `.maolan_autosave/snapshots/`: autosave snapshots

The session JSON persists:

- tracks, clips, and track connections
- clip group membership as recursive `grouped_clips`
- per-audio-clip plugin graph state in `plugin_graph_json`
- clip pitch-correction settings and saved pitch-segment targets
- plugin graph topology and plugin state
- session metadata
- transport state, tempo points, and time-signature points
- export dialog settings
- global MIDI learn bindings
- selected UI sizing values

## Live Session View

The Session View (switch to it with the `Live` toolbar button or `Tab`) is a clip-launch grid modeled after Ableton Live. Each row is a track and each column is a scene. Slots reference arrangement clips by their stable clip ID, so edits to the arrangement clip are reflected in the session.

Switching between Workspace and Session view never stops playback. Transport controls hand over between the two: pressing **play** in one view while the other is playing stops the current playback and starts the pressed view's playback, while **pause** and **stop** pressed in either view stop whatever is playing.

Session data is stored in `session.json` inside the session directory and includes the scene list and slot references. The arrangement (`<branch>.json`) is the source of truth for clip content; removing an arrangement clip that a slot references moves the clip to the unused pool, and the slot keeps playing it from there until the slot is cleared or the clip is deleted via **File → Delete unused files**.

Commands:

- **Launch/Stop slot**: click a slot, or press `Return` on the selected slot. Each slot carries a play mark, a stop mark, or neither (right-click clears the mark); marked slots launch or stop their track on scene changes.
- **Launch scene**: click a scene slot in the Master column. Clicking always selects that scene: while the live session is stopped, the selected scene (shown with a blue border) is the one the play button starts; while the live session is playing, the selected scene launches when the longest currently playing clip finishes its current pass. Clicking a different scene slot simply moves the selection, replacing any pending launch — clicking the currently playing scene re-triggers it at the end of its pass. On the switch, each track follows its slot in the new scene — play-marked slots launch, stop-marked slots stop — and a slot with neither mark inherits from the same track's slot in the previously playing scene (play if that slot was play-marked — the previous scene's clip keeps playing, or starts when the track is silent — stop if it was stop-marked, and keeps doing whatever it was doing when that slot is also unmarked). The currently playing scene's slot is filled green with a green border. If the scene has a per-scene tempo set, the global tempo changes when the scene launches (immediate launches only).
- **Stop all clips**: press `Shift+Space`, click the **Stop All** button in the master column, or use the slot context menu.
- **Stop track clips**: click the stop square in the master column for that track row.
- **Move slot reference**: drag a slot that references a clip onto another slot in the grid. This moves the clip reference; the arrangement clip itself is not affected.
- **Duplicate slot**: right-click a slot that references a clip and choose **Duplicate**. This copies the clip reference to the next empty slot on the same track.
- **Copy to arrangement**: right-click a slot that references a clip and choose **Copy to arrangement**, or drag the slot onto the Workspace timeline. This creates a new arrangement clip at the current transport position with the same content as the referenced clip.
- **Assign clip to session slot**: in the workspace, select a session slot on the target track (in Session view), then right-click an arrangement clip on that track and choose **Assign to session slot**. The selected slot will reference that clip.
- **Import arrangement to session**: available from the View menu; creates a scene and fills each track's slots with references to its arrangement clips.
- **Record session to arrangement**: click the `Rec to Arr` button in the top-left of the session grid. This arms tracks that have session slots, starts playback if needed, enables recording, and writes new arrangement clips through the existing record path.
- **Record into slot**: right-click a slot and choose **Record into slot**. This arms the track, starts playback and recording if needed, and assigns the newly recorded clip to that slot when the clip is created.
- **Scene options**: right-click a scene header to rename it, set its color, set a per-scene tempo, or change its launch quantization.
- **Track strip**: the left column shows each track's name plus compact mute, solo, arm, volume, and pan controls.

## Templates

Session templates live under:

`~/.config/maolan/session_templates/<name>/`

Each session template contains a `session.json` (templates are not branch-aware) plus the same supporting subdirectories used by a normal session. Session templates keep:

- track structure
- routing
- plugin graphs and plugin state
- session metadata
- export settings

Session templates intentionally do not keep:

- audio clips
- MIDI clips
- frozen render state or frozen backups

Track templates live under:

`~/.config/maolan/track_templates/<name>/`

Each track template stores `track.json` plus a `plugins/` directory. Track templates keep:

- one track's settings
- that track's plugin graph and plugin state
- connections involving that track

Folder track templates are stored in the same location. A folder template's `track.json` also contains a `children` array with the saved subtree (each child has the same `track`/`graph`/`children` shape). Folder templates keep:

- the folder track's settings
- the folder's plugin graph and plugin state
- child-to-folder-plugin connectable connections
- disabled child-to-folder-output feeds
- internal connections between folder members
- the full child track subtree, recursively

Track and folder templates intentionally do not keep:

- audio clips
- MIDI clips
- connections to tracks outside the saved subtree

## Modulators

The Modulators pane (View → Modulators or `M`) shows the session's LFO-style modulators.

- Press `+` to add a modulator.
- Each modulator has a name, enable toggle, shape, rate mode, rate value, and phase offset.
- Rate can be set in Hz or in musical divisions (bar, half, beat, eighth, … sixty-fourth).
- While a modulator is selected, mixer faders/pans and automation-lane headers show a target overlay.
- Click the overlay (or use the track context menu) to open the **Assign modulator** dialog.
- The dialog asks for a min and max value; the modulator output is scaled to that range.
- Targets include track Volume/Balance, visible automation lanes (plugin parameters, MIDI CC), and master output Volume/Balance.
- Existing assignments can be removed from the same dialog.

Modulator assignments are saved in the session and restored on load.

## Step Recording

Step recording is a MIDI input mode in the piano roll for entering notes one step at a time.

- Open a MIDI clip in the piano roll (double-click a MIDI clip).
- Toggle step recording from the toolbar (`Step` button), visible only while the MIDI editor is active.
- When enabled, the editor shows a step cursor at the current insert position.
- Played MIDI notes are inserted at the step cursor with length equal to the current MIDI snap interval, and the cursor advances automatically.
- `NoSnap` / `Clips` snap modes fall back to a sixteenth-note step length.

## Fold Mixer Strips

Folder tracks render in the mixer as collapsible strips.

- Folder mixer strips have a ▼ / ▶ toggle in their header; click it to expand or collapse the child tracks.
- When expanded, child tracks are nested to the right of the folder strip.
- When collapsed, only the folder strip is shown.
- Folder strips omit the record-arm button because folders cannot be armed.
- Double-click a folder strip to open its connections graph.

## Autosave and Recovery

- Autosave snapshots are generated every 15 seconds.
- Snapshots are stored in `<session>/.maolan_autosave/snapshots/<timestamp>/`.
- A snapshot is considered valid when it contains a `<branch>.json` file.
- Recovery prefers newer snapshots and sorts them newest-first.
- On startup or open, Maolan can detect when the newest autosave snapshot is newer than the live `<branch>.json`.
- Recovery preview currently summarizes track, audio-clip, and MIDI-clip count deltas between the live session and the selected snapshot.

Recovering an autosave snapshot loads that snapshot as the current session state and marks the session as having unsaved changes.

## Diagnostics

The UI exposes:

- Session Diagnostics
- MIDI Mappings Report
- Export Diagnostics Bundle

Exported diagnostics bundles are written to:

- `<session>/maolan_diagnostics_<unix-seconds>/` when a session is open
- `/tmp/maolan_diagnostics_<unix-seconds>/` otherwise

Current bundle contents:

- `session_diagnostics.txt`
- `midi_mappings.txt`
- `ui_summary.json`

## Export Behavior

- Mixdown export renders the selected hardware output ports.
- Stem export writes one file per eligible selected track into `<base>_stems/`.
- Multiple output formats can be written in one export run.
- Opus export supports only mono or stereo output.
- Normalization, master-limiter, and dither settings are persisted in the session file.
- Dither is applied when exporting to integer PCM formats (WAV/FLAC at 16/24/32-bit). It is skipped for 32-bit float and for lossy formats (Opus).

## HeartMuLa Generation Operations

The current HeartMuLa generation path uses the `maolan-generate` crate/binary.

- The GUI launches generate through the local `maolan-generate` binary and exchanges requests over a socketpair IPC path.
- Prompt generation runs as a dedicated HeartMuLa token-generation subprocess, then decode runs in-process with HeartCodec.
- When `--model-dir <path>` is not provided, the generate path resolves models from the Hugging Face cache via `hf-hub`.
- The expected Hugging Face Burn repos are:
  - `maolandaw/HeartMuLa-happy-new-year-burn`
  - `maolandaw/HeartCodec-oss-20260123-burn`
- The generate path currently expects these repo file layouts:
  - HeartMuLa repo: `heartmula.bpk`, `tokenizer.json`, `gen_config.json`
  - HeartCodec repo: `heartcodec.bpk`
- The current CLI supports `--model <happy-new-year|RL>`.
- The current CLI uses `--length <int>` in milliseconds for output duration.
- `--decode-only` requires `--frames-json <path>`.
- `--decode-threads <int>` can be used to control decode-only CPU worker count.
- `--model-dir <path>` can be used to bypass Hugging Face cache resolution and point at a local export directly.

## Media Consolidation and File References

Maolan can collect external media files into the session directory and update plugin file references to keep sessions portable.

- **File → Consolidate** copies imported audio/MIDI files and any CLAP/LV2 plugin file references into the session's `data/` directory.
- Consolidation updates plugin file references to absolute paths immediately, and saved plugin state is rewritten to relative `data/` paths on save.
- **File → Delete unused files** scans `audio/`, `midi/`, `peaks/`, and `pitch/` and removes files not referenced by the current session or any non-hidden branch JSON. Unused clips (deleted from tracks but kept in the Clips pane's Unused section) are permanently removed first when no session slot of the current branch references them and no other branch file references their media; clips still used in the live or edit view of any session file stay in the Unused section.
- Consolidation makes it safer to move or share a session directory.

## Pitch Correction Caching and Rendering

- Opening pitch correction for an audio clip requires a saved/opened session.
- Analysis results are cached per session under `pitch/` and reused when the source file modification time still matches.
- Cached analysis is keyed by source file, source offset, and source length, so clip corrections can target either the whole clip or a saved source window.
- Pressing `Apply` stores the current correction points and render settings on the clip without destructively rewriting the source audio file.
- Live transport playback applies saved pitch correction in real time.
- Freeze preparation renders pitch-corrected clips offline first so frozen audio matches the corrected result.

## Clip Groups and Per-Clip FX

- Grouping replaces the selected top-level clips with one parent clip whose `grouped_clips` store child timing relative to the group start.
- Grouping is only available for selections of two or more clips from the same track and the same clip kind.
- Child fades are cleared during grouping; the group clip owns the audible fades after grouping.
- Ungroup restores children at their absolute timeline positions and preserves nested groups.
- Grouped clips cannot be split directly.
- Audio clips can also store an independent plugin graph in `plugin_graph_json`.
- On supported Unix builds, opening clip plugins seeds a default passthrough graph when a clip has no saved graph yet.
- In the current audio engine path, grouped audio children are summed, the group fade is applied, and the group clip plugin graph runs after that sum; leaf audio clips can also run their own plugin graphs.

## Recent Sessions

Recent sessions are stored in `config.toml` via `recent_session_paths`.

- paths are normalized before display
- duplicates are removed
- invalid paths are dropped
- the list is capped to the app's configured recent-session limit
