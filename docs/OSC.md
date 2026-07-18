# OSC

Maolan supports an OSC control surface for transport, track management,
routing, plugins, automation, clip editing, MIDI editing, MIDI learn,
audio/MIDI devices, offline bounce, and state queries.

OSC is disabled by default.

## Enabling OSC

Enable OSC from Preferences in the GUI:

1. Open `Preferences`.
2. Turn on `Enable OSC`.
3. Save preferences.

Once enabled, Maolan starts an OSC listener thread in the engine.

The preference is stored in:

`~/.config/maolan/config.toml`

## OSC Listener

The engine listens on:

`0.0.0.0:9000`

There is **no `/maolan` prefix** on any OSC address.

## Command addresses

Arguments use OSC type tags:

- `i` — integer
- `f` — float
- `s` — string
- `T` / `F` — true / false (also accepted as `i` 0/1)

### Transport

- `/transport/play`
- `/transport/stop`
- `/transport/pause`
- `/transport/start`
- `/transport/end`
- `/transport/position i` — set transport position in samples
- `/transport/position_at i i` — set position after N frames
- `/transport/session_play`
- `/transport/record i` — enable (`1`) or disable (`0`) recording
- `/transport/loop_enable i`
- `/transport/loop_range ii` — start, end in samples
- `/transport/loop_range/clear` — clear the loop range
- `/transport/punch_enable i`
- `/transport/punch_range ii` — start, end in samples
- `/transport/punch_range/clear` — clear the punch range
- `/transport/metronome i`
- `/transport/clip_playback i`
- `/transport/session_clip_playback i`
- `/transport/panic`
- `/transport/tempo f` — set tempo in BPM
- `/transport/time_signature ii` — numerator, denominator
- `/transport/tempo_map s` — JSON tempo/time-signature map
- `/step_recording i` — enable (`1`) or disable (`0`) step recording

Compatibility aliases are still accepted:

- `/transport/jump_to_start` → `/transport/start`
- `/transport/jump_to_end` → `/transport/end`
- `/transport/start_of_session` → `/transport/start`
- `/transport/end_of_session` → `/transport/end`

Tempo map JSON format:

```json
{
  "tempo_points": [
    {"sample": 0, "bpm": 120.0}
  ],
  "time_signature_points": [
    {"sample": 0, "numerator": 4, "denominator": 4}
  ]
}
```

### Session

- `/session/launch s i` — launch clip on track/scene
- `/session/stop s i` — stop clip on track/scene
- `/session/scene i` — launch every clip in the given scene
- `/session/stop_scene i` — stop every clip in the given scene
- `/session/stopall` — stop all currently playing session clips
- `/session/path s` — set session path

History is intentionally not exposed over OSC.

### Track management

- `/track/add s i i i i i` — name, audio_ins, midi_ins, audio_outs, midi_outs, folder
- `/track/remove s`
- `/track/rename s s`
- `/track/set_folder s i`
- `/track/toggle_folder s`
- `/track/set_parent s s` — empty parent string removes the parent
- `/track/add_audio_input s`
- `/track/remove_audio_input s`
- `/track/add_audio_output s`
- `/track/remove_audio_output s`

### Track mixing / state

- `/track/level s f`
- `/track/balance s f`
- `/track/automation_level s f`
- `/track/automation_balance s f`
- `/track/midi_cc s i i i` — track_name, channel, cc, value
- `/track/mute s i`
- `/track/solo s i`
- `/track/arm s i`
- `/track/phase s i`
- `/track/master s i`
- `/track/color s f f f [f]` — RGBA (alpha defaults to 1.0)
- `/track/color/clear s` — clear the track color
- `/track/frozen s i`
- `/track/midi_lane_channel s i i` — channel `-1` clears the assignment
- `/track/session_slot s i s`
- `/track/session_slot_play_enabled s i i`
- `/track/toggle_input_monitor s i`
- `/track/toggle_disk_monitor s i`
- `/track/toggle_midi_input_monitor s i`
- `/track/toggle_midi_disk_monitor s i`
- `/track/clear_default_passthrough s`
- `/track/clear_plugins s`
- `/track/connect_audio s s i s i`
- `/track/disconnect_audio s s i s i`
- `/track/connect_midi s s i s i`
- `/track/disconnect_midi s s i s i`

Connectable node strings: `"track_input"`, `"track_output"`,
`"child:<name>"`, `"clap_<id>"`, `"vst3_<id>"`, `"lv2_<id>"`.

### Routing

- `/connect s i s i s` — from_track, from_port, to_track, to_port, kind
- `/disconnect s i s i s`

`kind` is `"audio"` or `"midi"`.

### Clips

- `/clip/add s s i i i i i i i i s [s] [s]` — track, name, start, length,
  offset, input_channel, muted, fade_enabled, fade_in, fade_out, kind,
  optional source_name, optional preview_name
- `/clip/remove s s s` — track, kind, comma-separated indices
- `/clip/move s s i s i i i` — kind, from_track, from_index, to_track,
  to_offset, to_channel, copy
- `/clip/fade s i s i i i` — track, clip_index, kind, fade_enabled,
  fade_in_samples, fade_out_samples
- `/clip/bounds s i s i i i` — track, clip_index, kind, start, length, offset
- `/clip/mute s i s i`
- `/clip/rename s i s s`
- `/clip/source_name s i s s`
- `/clip/plugin_graph_json s i s` — empty JSON string clears the graph
- `/clip/pitch_correction s i s` — JSON pitch-correction data or empty string
- `/clip_group/add s s s s` — track, kind, audio_clip JSON, midi_clip JSON

Pitch-correction JSON format:

```json
{
  "preview_name": "preview.wav",
  "source_name": "source.wav",
  "source_offset": 0,
  "source_length": 44100,
  "frame_likeness": 0.5,
  "inertia_ms": 50,
  "formant_compensation": false,
  "points": [
    {
      "start_sample": 0,
      "length_samples": 1000,
      "detected_midi_pitch": 60.0,
      "target_midi_pitch": 61.0,
      "clarity": 0.9
    }
  ]
}
```

### MIDI editing

All MIDI editing commands take a JSON string as the last argument.

- `/midi/insert_notes s i s`
- `/midi/delete_notes s i s`
- `/midi/modify_notes s i s`
- `/midi/insert_controllers s i s`
- `/midi/delete_controllers s i s`
- `/midi/modify_controllers s i s`
- `/midi/sysex s i s`
- `/midi/step_record s i i i` — device, channel, pitch, velocity

Insert-notes JSON format:

```json
[
  {"index": 0, "start_sample": 0, "length_samples": 1000,
   "pitch": 60, "velocity": 100, "channel": 0}
]
```

Delete-notes JSON format:

```json
{
  "indices": [0, 1],
  "deleted": [
    {"index": 0, "start_sample": 0, "length_samples": 1000,
     "pitch": 60, "velocity": 100, "channel": 0}
  ]
}
```

Modify-notes JSON format:

```json
{
  "indices": [0, 1],
  "new": [{"start_sample": 0, "length_samples": 1000, "pitch": 60, ...}],
  "old": [{"start_sample": 0, "length_samples": 1000, "pitch": 59, ...}]
}
```

Controller JSON objects use `sample`, `controller`, `value`, and `channel`.
SysEx JSON format:

```json
[{"sample": 0, "data": [240, 1, 2, 247]}]
```

### Plugins

- `/plugin/load s s s` — track_name, format (`"clap"`, `"vst3"`, `"lv2"`), plugin ID
- `/plugin/unload s s s`
- `/plugin/unload_instance s s i`
- `/plugin/bypass s s i i` — track_name, format, instance_id, bypassed
- `/plugin/show_gui s s i`
- `/plugin/snapshot_state s s i`
- `/plugin/restore_state s s i s` — JSON state (`{"bytes":[...]}` for CLAP,
  `{"plugin_id":"...","component_state":[...],"controller_state":[...]}` for VST3)
- `/plugin/set_resource_dir s s i s`
- `/plugin/update_file_reference s s i i s` — track_name, format, instance_id,
  file_index, path
- `/plugin/connect_audio s s i s i`
- `/plugin/disconnect_audio s s i s i`
- `/plugin/connect_midi s s i s i`
- `/plugin/disconnect_midi s s i s i`
- `/plugin/set_param s s i i f`
- `/clip_plugin/set_param s s i i i f`
- `/clip_plugin/snapshot_state s s i i`
- `/clip_plugin/restore_state s s i i s`
- `/clip_plugin/set_resource_dir s s i i s`
- `/clip_plugin/update_file_reference s s i i i s`

Plugin graph node strings:

- `"track_input"`
- `"track_output"`
- `"clap_<instance_id>"`
- `"vst3_<instance_id>"`
- `"lv2_<instance_id>"`

### Automation

Automation targets supported in OSC:

- `"volume"`
- `"balance"`
- `"midi_cc_<channel>_<cc>"` — channel is 1–16

Addresses:

- `/automation/mode s s` — track_name, mode (`"read"`, `"touch"`, `"latch"`, `"write"`)
- `/automation/toggle_lane s s`
- `/automation/insert_point s s i f` — track_name, target, sample, value
- `/automation/delete_point s s i` — track_name, target, sample

### MIDI learn

Track targets: `volume`, `balance`, `mute`, `solo`, `arm`, `input_monitor`,
`disk_monitor`.

Global targets: `play_pause`, `stop`, `record_toggle`.

Session target strings:

- `slot:<track>:<scene>`
- `scene:<scene>`
- `stop_track:<track>`
- `stop_all`

Binding JSON (empty string clears):

```json
{"device": "X-Touch", "channel": 1, "cc": 7}
```

Addresses:

- `/midi_learn/arm_track s s`
- `/midi_learn/arm_global s`
- `/midi_learn/arm_session s`
- `/midi_learn/bind_track s s s`
- `/midi_learn/bind_global s s`
- `/midi_learn/bind_session s s`
- `/midi_learn/clear`

### Modulators / Devices

- `/modulators s` — JSON array of modulators
- `/device/audio_open s` — JSON audio-device configuration
- `/device/midi_in_open s`
- `/device/midi_out_open s`
- `/device/jack/add_audio_in`
- `/device/jack/remove_audio_in i`
- `/device/jack/add_audio_out`
- `/device/jack/remove_audio_out i`

Audio-device JSON format:

```json
{
  "device": "hw:0",
  "input_device": "hw:0",
  "sample_rate_hz": 48000,
  "bits": 32,
  "exclusive": false,
  "period_frames": 256,
  "nperiods": 2,
  "sync_mode": false,
  "actual_period_frames": 256,
  "input_channels": 2,
  "output_channels": 2,
  "bytes_per_frame": 8
}
```

### Offline bounce

- `/bounce/start s s i i s i` — track, output_path, start_sample,
  length_samples, automation_lanes JSON, apply_fader
- `/bounce/cancel s`
- `/bounce/cancel_all`

### Piano key

- `/piano_key s i i i` — track_name, note, velocity, on

## Query addresses

Queries return replies to the sender.

- `/query/tracks` → `/response/tracks s...`
- `/query/transport` → `/response/transport i i f i i`
  (sample, playing, tempo, time_sig_num, time_sig_denom)
- `/query/meters` → `/response/meters ...`
- `/query/plugins s` → `/response/plugins s i (i s s s i)*`
  (track_name, plugin_count, instance_id, format, uri, name, bypassed)
- `/query/plugin_parameters s s i i` → `/response/plugin_parameters s i s s`
  (track_name, instance_id, format, parameters_json)
- `/query/clip_plugin_parameters s s i i i` → clip plugin parameters reply
- `/query/clap_plugins` → `/response/clap_plugins s...`
- `/query/clap_plugins_with_capabilities` → list with I/O capabilities
- `/query/vst3_plugins` → `/response/vst3_plugins s...`
- `/query/lv2_plugins` → `/response/lv2_plugins s...`
- `/query/clap_note_names s` → `/response/clap_note_names s...`
- `/query/lv2_midnam s` → `/response/lv2_midnam s ...` (LV2 midnam MIDI note names; Unix only)
- `/query/vst3_graph s` → `/response/vst3_graph s...`
- `/query/diagnostics` → `/response/diagnostics s...`
- `/query/midi_learn_report` → `/response/midi_learn_report s...`

Plugin-list replies contain strings in the form `id|name` for CLAP and VST3,
`uri|name` for LV2.

Errors are reported as:

- `/error s`

## `maolan-osc` Helper

The repository includes a command-line helper:

`maolan-osc`

### Global options

```bash
maolan-osc --target 127.0.0.1:9000 play
maolan-osc --host 192.168.1.10 --port 9000 stop
maolan-osc --file commands.txt
```

`--file <path>` reads commands from a file, one per line, using the same
syntax as the command line. Blank lines and lines starting with `#` are
ignored, and single or double quotes group arguments that contain
whitespace (track names, JSON payloads). The whole file is parsed first
and every error is reported with its line number; packets are sent only
when the file parses without errors. An example command file is available
at `docs/commands.txt`.

### Examples

```bash
cargo run --bin maolan-osc -- play
cargo run --bin maolan-osc -- stop
cargo run --bin maolan-osc -- pause
cargo run --bin maolan-osc -- start
cargo run --bin maolan-osc -- end

cargo run --bin maolan-osc -- position 44100
cargo run --bin maolan-osc -- tempo 128.5
cargo run --bin maolan-osc -- record 1

cargo run --bin maolan-osc -- track add "Vocals" 2 0 2 0
cargo run --bin maolan-osc -- track remove "Vocals"
cargo run --bin maolan-osc -- track rename "Vocals" "Lead Vocals"
cargo run --bin maolan-osc -- track folder "Buses" 1
cargo run --bin maolan-osc -- track parent "Kick" "Drums"

cargo run --bin maolan-osc -- track level "Drums" -6.0
cargo run --bin maolan-osc -- track mute "Drums" 1
cargo run --bin maolan-osc -- track solo "Drums" 1
cargo run --bin maolan-osc -- track arm "Vocals" 1

cargo run --bin maolan-osc -- connect "Kick" 0 "Drums" 0 audio
cargo run --bin maolan-osc -- disconnect "Kick" 0 "Drums" 0 audio

cargo run --bin maolan-osc -- plugin load "Drums" clap "rs.maolan.widener"
cargo run --bin maolan-osc -- plugin bypass "Drums" clap 0 1
cargo run --bin maolan-osc -- plugin connect_audio "Drums" track_input 0 clap_0 0
cargo run --bin maolan-osc -- plugin set_param "Drums" clap 0 0 0.75

cargo run --bin maolan-osc -- track connect_audio "Drums" child:"Sub" 0 track_output 0

cargo run --bin maolan-osc -- clip add "Vocals" "Take 1" 0 44100 0 0 0 1 100 100 audio

cargo run --bin maolan-osc -- midi insert_notes "Piano" 0 '[{"index":0,"start_sample":0,"length_samples":1000,"pitch":60,"velocity":100,"channel":0}]'

cargo run --bin maolan-osc -- automation mode "Drums" touch
cargo run --bin maolan-osc -- automation point "Drums" midi_cc_1_7 44100 64.0

cargo run --bin maolan-osc -- midi_learn bind_track "Drums" volume '{"device":"X-Touch","channel":1,"cc":7}'

cargo run --bin maolan-osc -- query tracks
cargo run --bin maolan-osc -- query transport
cargo run --bin maolan-osc -- query meters
cargo run --bin maolan-osc -- query plugins "Drums"
cargo run --bin maolan-osc -- query plugin_parameters "Drums" clap 0
cargo run --bin maolan-osc -- query clap_plugins
cargo run --bin maolan-osc -- query clap_plugins_with_capabilities
cargo run --bin maolan-osc -- query diagnostics

cargo run --bin maolan-osc -- session stop_scene 2
cargo run --bin maolan-osc -- track automation_level "Drums" -6.0
cargo run --bin maolan-osc -- track midi_cc "Drums" 1 7 64
cargo run --bin maolan-osc -- step_recording 1

cargo run --bin maolan-osc -- plugin show_gui "Drums" clap 0
cargo run --bin maolan-osc -- plugin snapshot_state "Drums" clap 0
cargo run --bin maolan-osc -- plugin restore_state "Drums" clap 0 '{"bytes":[1,2,3]}'
cargo run --bin maolan-osc -- plugin update_file_reference "Drums" clap 0 0 "/tmp/sample.wav"

cargo run --bin maolan-osc -- clip_plugin snapshot_state "Drums" clap 0 0
```

Run `maolan-osc --help` for the full command list.

## Notes

- OSC only starts after it is enabled in preferences and the setting has
  been saved.
- If OSC is disabled, `maolan-osc` can still send packets, but the engine
  will not be listening for them.
- Query replies are sent to the source address of the request.
- Complex structured data (clip data, MIDI notes, controllers, tempo maps,
  modulators, audio-device settings, automation lanes) is sent as JSON
  strings because the OSC implementation uses flat typed arguments rather
  than bundles.
