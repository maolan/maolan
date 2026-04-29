# Maolan Operations, Storage, and Recovery

Last updated: 2026-04-30

## Runtime and Platform Behavior

- Linux and FreeBSD builds force the X11 backend at startup by unsetting `WAYLAND_DISPLAY` and `WAYLAND_SOCKET`.
- macOS builds support CLAP and VST3, but not the Unix LV2 host path.
- Plugin discovery runs automatically on startup:
  - Linux / FreeBSD: LV2, VST3, CLAP
  - macOS: VST3, CLAP
- Passing `--debug` enables tracing output to stdout.

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

- `session.json`: main project state
- `audio/`: imported or rendered audio stored in-session
- `midi/`: copied MIDI files used by clips
- `peaks/`: cached waveform peak data written on save
- `pitch/`: cached pitch-analysis JSON files keyed by source clip path + source window
- `plugins/`: plugin-related session assets
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

## Templates

Session templates live under:

`~/.config/maolan/session_templates/<name>/`

Each session template contains a `session.json` plus the same supporting subdirectories used by a normal session. Session templates keep:

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

Track templates intentionally do not keep:

- audio clips
- MIDI clips

Group templates live under:

`~/.config/maolan/group_templates/<name>/`

Each group template stores `group.json` plus a `plugins/` directory. Group templates keep:

- all tracks that share the same VCA master (group)
- each track's settings and plugin graph
- connections between tracks in the group

When a group template is loaded from the Add Track dialog:

- the base name you enter becomes the new group name (VCA master)
- each track in the group is created with the base name as a prefix
  - single-track groups use the base name directly
  - multi-track groups use `"<base> <original>"`
- plugin graphs are restored per track
- intra-group connections are remapped to the new track names
- all tracks in the group are automatically assigned to the new VCA master

Group templates intentionally do not keep:

- audio clips
- MIDI clips
- frozen render state or frozen backups
- connections to tracks outside the group

## Autosave and Recovery

- Autosave snapshots are generated every 15 seconds.
- Snapshots are stored in `<session>/.maolan_autosave/snapshots/<timestamp>/`.
- A snapshot is considered valid when it contains `session.json`.
- Recovery prefers newer snapshots and sorts them newest-first.
- On startup or open, Maolan can detect when the newest autosave snapshot is newer than the live `session.json`.
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
- MP3 export supports only mono or stereo output.
- Normalization and master-limiter settings are persisted in the session file.

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

## Project Structure

The codebase is split across multiple repositories:

- `daw/` — Main application and GUI
- `engine/` — Audio engine (moved to its own repository)
- `widgets/` — Reusable iced widgets (moved to its own repository)
- `generate/` — AI audio generation via `maolan-generate` (moved to its own repository)
- `mixosc/` — OSC mixing control integration

## Build and Test

- Code coverage is tracked and reported.
- Unit test coverage has been expanded across the codebase.
- Cleanup and dead-code removal passes are performed regularly.

## Recent Fixes

- Fixed note names display in the piano roll.
- Fixed GitHub Actions workflow configuration.
