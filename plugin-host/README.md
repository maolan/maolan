# maolan-plugin-host

Out-of-process plugin host for the [Maolan DAW](https://maolan.github.io).

This crate provides both a library and a standalone binary that loads and runs
audio plugins inside isolated processes. The DAW communicates with each host
over shared memory and lightweight event channels, giving robust sandboxing:
a crashing plugin does not bring down the session.

## Supported formats

| Format | Status | Platforms |
|--------|--------|-----------|
| **CLAP** | Fully supported | Linux, FreeBSD, Windows |
| **VST3** | Fully supported | Linux, FreeBSD |
| **LV2**  | Fully supported | Linux, FreeBSD |

## How it works

1. The DAW spawns `maolan-plugin-host` as a separate process for every plugin
   instance.
2. Both sides open a shared-memory segment and a pair of event pipes
   (Unix) or named events (Windows).
3. Audio buffers, parameter changes, and MIDI events are exchanged through the
   shared-memory protocol defined in
   [`maolan-plugin-protocol`](https://crates.io/crates/maolan-plugin-protocol).
4. If the plugin crashes or hangs, the DAW can terminate the host process and
   recover without affecting the rest of the project.

## Binary usage

### Plugin hosting mode

The DAW normally invokes the binary automatically, but the command-line
interface looks like this:

```text
maolan-plugin-host <format> <plugin-path> <shm-name> <instance-id> <d2h-fd> <h2d-fd> [sample-rate buffer-size num-inputs num-outputs]
```

- `format` – `clap`, `vst3`, or `lv2`.
- `plugin-path` – Path to the plugin library. For CLAP you can select a
  specific plugin inside the factory with `path#plugin_id` or `path::plugin_id`.
- `shm-name` – Name of the shared-memory segment created by the DAW.
- `instance-id` – Unique identifier for this instance.
- `d2h-fd` / `h2d-fd` – File descriptors (Unix) or event names (Windows) used
  for the event channel.
- Optional VST3/LV2 arguments: `sample-rate`, `buffer-size`, `num-inputs`,
  `num-outputs`.

### Scan mode

Discover plugins and dump their metadata to JSON:

```text
maolan-plugin-host --scan --format <format> --path <plugin-path> [--output <json-path>]
```

Example:

```bash
maolan-plugin-host --scan --format clap --path /usr/lib/clap --output clap-index.json
```

The generated JSON has the following shape, with any stderr diagnostics from the
scan captured as `errors` and `warnings`. Each diagnostic includes the original
`message` and, when it can be determined, the affected `plugin_uri`,
`plugin_name`, and/or `bundle_uri`:

```json
{
  "errors": [
    {
      "message": "error: failed to open file /usr/lib/lv2/broken.lv2/manifest.ttl (No such file or directory)",
      "bundle_uri": "file:///usr/lib/lv2/broken.lv2/"
    }
  ],
  "warnings": [],
  "data": []
}
```

## Library usage

Add the crate to your `Cargo.toml`:

```toml
[dependencies]
maolan-plugin-host = "0.0.1"
```

The library re-exports `maolan_plugin_protocol` and exposes the following
modules:

- `clap` – Low-level CLAP C FFI bindings and host extension helpers.
- `host` – Shared-memory runtime that drives a plugin instance.
- `lv2` – LV2 host implementation (Unix only), built on the pure-Rust
  `maolan-lv2` crate (no C lilv dependency).
- `scan` – Plugin scanner and metadata serialization.
- `paths` – Standard plugin installation paths for each platform.
- `util` – Small helpers such as `SimpleMutex` and `AudioPort`.
- `vst3` – VST3 bindings and helpers.
- `vst3_lv2_host` – VST3/LV2 specific runtime loop and GUI support.

Most users will only need the binary; the library API is intended for the
Maolan engine and advanced integrations.

## Building

Requires **Rust 1.85+** (2024 edition).

```bash
cargo build --release
```

### Platform-specific dependencies

- **Linux / FreeBSD**
  - X11 development libraries (for VST3 GUI support)
  - LV2 support is pure Rust (via `maolan-lv2`); no `liblilv`/`lv2` system
    libraries are required.
- **Windows**
  - No extra system libraries required.

## License

BSD-2-Clause. See `Cargo.toml` for the full license identifier.
