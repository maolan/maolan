# Maolan

[![crates.io](https://img.shields.io/crates/v/maolan.svg)](https://crates.io/crates/maolan)

![Maolan](images/maolan.svg)

Maolan is a Rust DAW focused on recording, editing, routing, automation, export, and plugin hosting.

[maolan.github.io](https://maolan.github.io)

![workspace](images/workspace.gif)

## Current Scope

Maolan currently includes:

- Audio and MIDI tracks with timeline editing
- Piano roll editing with note/controller/SysEx tools
- Track and plugin automation
- Plugin hosting for:
  - CLAP
  - VST3
  - LV2 on Unix
- Per-track plugin graph routing, including sidechains and MIDI paths
- Freeze, flatten, offline bounce, and export workflows
- Session templates, track templates, autosave recovery, and diagnostics

## Platform Notes

- Unix builds support CLAP, VST3, and LV2.
- Current keyboard handling is `Ctrl`-based across platforms.
- Plugin compatibility is host-dependent and should be treated as evolving rather than guaranteed.

## Build

### Prerequisites

- Rust toolchain (edition 2024)

For Unix plugin/audio integrations, install platform libraries as needed (for example `jack`, `lilv`, `suil`, `gtk2`, `rust`, `cargo`, `rubberband` where applicable).

### Windows

Building on Windows requires MSVC, vcpkg, and a few environment variables.

#### 1. Install dependencies

1. **Rust** — Install via [rustup](https://rustup.rs/):
   ```powershell
   winget install Rustlang.Rustup
   rustup target add x86_64-pc-windows-msvc
   ```

2. **Visual Studio 2022** — Install the *Desktop development with C++* workload. This provides:
   - MSVC compiler (`cl.exe`)
   - Windows SDK
   - `link.exe`

3. **vcpkg** — Install and bootstrap:
   ```powershell
   git clone https://github.com/microsoft/vcpkg C:\vcpkg
   C:\vcpkg\bootstrap-vcpkg.bat
   ```

4. **FFmpeg** — Install via vcpkg:
   ```powershell
   C:\vcpkg\vcpkg install ffmpeg:x64-windows
   ```

5. **LLVM** — Required by `ffmpeg-sys-next` (bindgen). Install from [llvm.org](https://releases.llvm.org/) or winget:
   ```powershell
   winget install LLVM.LLVM
   ```

6. **NSIS** — Required to build the installer. Download and extract:
   ```powershell
   # Download https://prdownloads.sourceforge.net/nsis/nsis-3.10.zip
   # Extract to C:\nsis-3.10 (or anywhere local)
   ```

#### 2. Set environment variables

```powershell
$env:FFMPEG_DIR     = 'C:\vcpkg\installed\x64-windows'
$env:VCPKG_ROOT     = 'C:\vcpkg'
$env:LIBCLANG_PATH  = 'C:\Program Files\LLVM\bin'
$env:LIB            = 'C:\vcpkg\installed\x64-windows\lib'
$env:INCLUDE        = 'C:\vcpkg\installed\x64-windows\include;C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.44.35207\include;C:\Program Files (x86)\Windows Kits\10\Include\10.0.26100.0\ucrt;C:\Program Files (x86)\Windows Kits\10\Include\10.0.26100.0\shared;C:\Program Files (x86)\Windows Kits\10\Include\10.0.26100.0\um'
```

> **Note:** Adjust `LIB` and `INCLUDE` paths to match your Visual Studio and Windows SDK versions.

#### 3. Build the binary

```powershell
cargo build --release --target x86_64-pc-windows-msvc
```

If building from a network share (e.g. Samba), use a local target directory to avoid "Access is denied" errors from build scripts:

```powershell
cargo build --release --target x86_64-pc-windows-msvc --target-dir C:\cargo-target
```

For a debug build (faster compilation, larger binary):

```powershell
cargo build --target x86_64-pc-windows-msvc
```

#### 4. Build the installer

The installer bundles the executables, FFmpeg DLLs, and the VC++ Redistributable.

1. Download the VC++ Redistributable to the repo root:
   ```powershell
   Invoke-WebRequest -Uri 'https://aka.ms/vs/17/release/vc_redist.x64.exe' -OutFile '..\vc_redist.x64.exe'
   ```

2. Compile the installer:
   ```powershell
   C:\nsis-3.10\makensis.exe installer.nsi
   ```

The output is `maolan-setup.exe` in the `daw/` directory.

### Compile and run (Unix)

The repository root is a single Cargo package (not a workspace).

```bash
cargo build --release
cargo run --release
```

### Debug logging

```bash
cargo run --release -- --debug
```

## Documentation

- [Features](docs/FEATURES.md)
- [Operations, Storage, and Recovery](docs/OPERATIONS.md)
- [Shortcuts and Mouse Gestures](docs/SHORTCUTS.md)
- [Plugin Routing and Sidechains](docs/PLUGIN_ROUTING.md)
- [History Audit Notes](docs/HISTORY_AUDIT.md)

## Project Notes

- Preferences are stored in `~/.config/maolan/config.toml`.
- Session templates are stored under `~/.config/maolan/session_templates/`.
- Track templates are stored under `~/.config/maolan/track_templates/`.
- Autosave snapshots are stored under `<session>/.maolan_autosave/snapshots/`.

## Status

Maolan is under active development. Behavior and UI details may evolve between commits.
