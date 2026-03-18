# Maolan Operations, Storage, and Recovery

Last updated: 2026-03-18

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

## Pitch Correction Caching and Rendering

- Opening pitch correction for an audio clip requires a saved/opened session.
- Analysis results are cached per session under `pitch/` and reused when the source file modification time still matches.
- Cached analysis is keyed by source file, source offset, and source length, so clip corrections can target either the whole clip or a saved source window.
- Pressing `Apply` stores the current correction points and render settings on the clip without destructively rewriting the source audio file.
- Live transport playback applies saved pitch correction in real time.
- Freeze preparation renders pitch-corrected clips offline first so frozen audio matches the corrected result.

## Recent Sessions

Recent sessions are stored in `config.toml` via `recent_session_paths`.

- paths are normalized before display
- duplicates are removed
- invalid paths are dropped
- the list is capped to the app's configured recent-session limit
