# OSC

Maolan supports a small OSC transport control surface for basic session
navigation and playback control.

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

The current OSC support is intentionally basic and transport-focused.

## Supported OSC Commands

These OSC addresses are accepted by the engine:

- `/transport/play`
- `/transport/stop`
- `/transport/pause`
- `/transport/start`
- `/transport/end`

The engine also accepts these compatibility addresses:

- `/transport/jump_to_start`
- `/transport/jump_to_end`
- `/transport/start_of_session`
- `/transport/end_of_session`

## `maolan-osc` Helper

The repository includes a small helper binary:

`maolan-osc`

It accepts exactly one argument:

- `play`
- `stop`
- `pause`
- `start`
- `end`

### Examples

```bash
cargo run --bin maolan-osc -- play
cargo run --bin maolan-osc -- stop
cargo run --bin maolan-osc -- pause
cargo run --bin maolan-osc -- start
cargo run --bin maolan-osc -- end
```

### Command Mapping

- `play` sends `/transport/play`
- `stop` sends `/transport/stop`
- `pause` sends `/transport/pause`
- `start` sends `/transport/start`
- `end` sends `/transport/end`

## Notes

- OSC only starts after it is enabled in preferences and the setting has
  been saved.
- If OSC is disabled, `maolan-osc` can still send packets, but the engine
  will not be listening for them.
