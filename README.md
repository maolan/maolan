# Maolan

A modern digital audio workstation (DAW) built with Rust, designed for professional audio recording, editing, mixing, and production.

## Features

### Core DAW Features

- **Multi-track Audio & MIDI Recording**: Record multiple audio and MIDI tracks simultaneously
- **Non-linear Editing**: Full timeline editor with waveform visualization, clip trimming, moving, and copying
- **Mixing Console**: Per-track level faders (-90dB to +20dB), pan/balance controls, and real-time metering
- **Transport Controls**: Play, stop, record, loop, and punch recording with bar-snapped ranges
- **Session Management**: Save and load complete project sessions with organized file structure

### Track Management

- **Audio Tracks**: Configurable inputs/outputs with real-time monitoring
- **MIDI Tracks**: MIDI input/output routing and recording
- **Track Controls**: Arm/disarm, mute, solo, input/disk monitoring
- **Flexible Layout**: Drag-to-reorder tracks, adjustable track heights, multi-select operations
- **Hardware I/O**: Route tracks to hardware outputs with master level and balance controls

### Audio & MIDI Editing

- **Clip Operations**: Create, move, resize, trim, copy, and delete audio/MIDI clips
- **Visual Editing**: Waveform display with peak caching, marquee selection, drag-and-drop editing
- **Timeline**: Bar/beat ruler with BPM control, zoom in/out, horizontal/vertical scrolling
- **Recording**: Multi-track recording with real-time preview, automatic file organization

### File Format Support

**Audio Import/Export:**
- WAV (native read/write)
- MP3, FLAC, OGG Vorbis, AIFF
- All formats supported by Symphonia codec library
- Automatic sample rate conversion

**MIDI:**
- Standard MIDI file import/export (.mid, .midi)
- Tempo extraction and preservation
- Format 0 and Format 1 support

**Sessions:**
- JSON-based session format
- Organized directory structure (audio/, midi/, plugins/, peaks/)
- Portable session files with relative paths

### Plugin Support

- **LV2 Plugin Host**: Load and use LV2 audio and MIDI effect plugins
- **Visual Plugin Graph**: Drag-and-connect interface for routing audio and MIDI through plugins
- **Multi-instance**: Use multiple instances of plugins across tracks
- **Plugin UI**: Native plugin UI support via Suil
- **State Management**: Plugin state saved and restored with sessions

### Advanced Routing

- **Connections View**: Visual connection matrix for routing audio and MIDI
- **Track-to-Track**: Route audio and MIDI between tracks
- **Hardware MIDI**: Connect hardware MIDI devices to tracks
- **Cycle Detection**: Prevents feedback loops in routing
- **Plugin Chains**: Build complex signal chains with multiple plugins per track

## Platform Support

### Linux
- **Audio Backend**: ALSA (default), JACK (available)
- **System Requirements**: ALSA development libraries, optionally JACK
- **Plugin Support**: Full LV2 plugin support (requires lilv library)

### FreeBSD
- **Audio Backend**: OSS (default), JACK (available)
- **System Requirements**: Base system OSS, optionally JACK
- **Plugin Support**: Full LV2 plugin support (requires lilv library)

### OpenBSD
- **Audio Backend**: sndio (default), JACK (available)
- **System Requirements**: Base system sndio, optionally JACK
- **Plugin Support**: Full LV2 plugin support (requires lilv library)

### Windows
- **Audio Backend**: WASAPI
- **System Requirements**: Windows 7 or later
- **Plugin Support**: Currently no LV2 support on Windows

## Building from Source

### Prerequisites

**All Platforms:**
- Rust toolchain (1.93.1 or later, 2024 edition)

**Linux:**
```bash
# Debian/Ubuntu
sudo apt install libasound2-dev libjack-jackd2-dev liblilv-dev libsuil-dev libgtk2.0-dev

# Fedora/RHEL
sudo dnf install alsa-lib-devel jack-audio-connection-kit-devel lilv-devel suil-devel gtk2-devel

# Arch
sudo pacman -S alsa-lib jack2 lilv suil gtk2
```

**FreeBSD:**
```bash
# Install from ports or packages
pkg install jack lilv suil gtk2
```

**OpenBSD:**
```bash
# Install from ports or packages
pkg_add jack lilv suil gtk+2
```

**Windows:**
No additional system dependencies required.

### Build Instructions

```bash
# Clone the repository
git clone https://github.com/yourusername/maolan.git
cd maolan

# Build the project
cargo build --release

# Run the application
cargo run --release
```

### Debug Mode

To enable debug logging:
```bash
cargo run --release -- --debug
```

## Usage

### Starting a New Session

1. Launch Maolan
2. Click "File" > "New Session" to create a new project
3. Add tracks using "Track" > "Add Track" or the toolbar button
4. Configure track inputs/outputs in the track controls

### Working with Tracks

**Adding Tracks:**
- Use "Track" > "Add Track" menu or toolbar button
- Enter a track name when prompted
- Configure audio/MIDI inputs and outputs

**Managing Tracks:**
- Drag tracks up/down to reorder them
- Adjust track height by dragging the track divider
- Multi-select tracks using Shift or Ctrl+click
- Delete tracks via the Track menu

**Quick Access to Track Plugins:**
- **Double-click any track** to instantly open its plugin graph view
- This provides quick access to add, remove, and configure plugins for that specific track

### Recording Audio

1. Arm a track by clicking the record button on the track
2. Set the input source in the track settings
3. Enable "Input Monitor" to hear the input
4. Click the record button in the transport controls
5. Click stop when done - a new audio clip will appear on the timeline

### Importing Audio and MIDI Files

1. Click "File" > "Import" to open the file browser
2. Select one or multiple files to import:
   - **Audio formats**: WAV, MP3, FLAC, OGG, AIFF, and all Symphonia-supported formats
   - **MIDI formats**: .mid, .midi files
3. A progress dialog shows the import status with:
   - Current file being processed
   - Overall progress (file X of Y)
   - Per-file operation progress (decoding, resampling, writing)
4. Audio files are automatically:
   - Converted to WAV format in the session's audio/ directory
   - Resampled to match the session sample rate if needed
   - Added to new tracks with the file's base name
5. MIDI files are:
   - Copied to the session's midi/ directory
   - Added to new MIDI tracks
   - Analyzed for tempo information

### Working with Plugins (Unix platforms)

**Accessing Track Plugins:**
1. **Double-click on a track** in the track list to open its plugin graph view
2. Or click the "Track Plugins" view button in the toolbar, then select a track

**Adding Plugins:**
1. In the Track Plugins view, the left panel shows all available LV2 plugins
2. Drag a plugin from the list onto the canvas to load it
3. Plugins appear as cards showing their input/output ports

**Connecting Plugins:**
1. Drag from an output port (right side of a plugin) to an input port (left side)
2. Audio and MIDI connections are shown in different colors
3. The routing validates port types (audio/MIDI cannot cross-connect)
4. Cycle detection prevents feedback loops

**Opening Plugin UIs:**
1. **Double-click on a plugin instance** in the graph view
2. If the plugin has a native UI, it opens in a separate window
3. If no native UI exists, a generic control interface is shown with sliders for all control parameters
4. UI windows can be closed without removing the plugin from the track

### Routing Audio and MIDI

1. Switch to the "Connections" view from the toolbar
2. Drag from an output port to an input port to create a connection
3. Audio and MIDI connections are shown in different colors
4. Delete connections by selecting and pressing delete

### Mixing

1. Use the mixer panel on the right side of the window
2. Adjust track levels with the fader (-90dB to +20dB)
3. Pan tracks left or right with the balance control
4. Monitor levels in real-time with the meter display
5. Use Mute/Solo for isolating tracks during mixing

### Session Management

**Saving:**
- "File" > "Save Session" - saves the current session
- Sessions include all track settings, clips, plugin states, and connections

**Loading:**
- "File" > "Load Session" - opens an existing session
- All files are referenced relative to the session directory

## Audio Configuration

Audio backend settings can be configured through the settings interface:

- **Device Selection**: Choose your audio interface
- **Bit Depth**: 8, 16, 24, or 32-bit audio
- **Buffer Size**: Period frames (affects latency)
- **Number of Periods**: Buffer count
- **Sync Mode**: Enable/disable synchronization
- **Exclusive Mode**: (OSS only) Exclusive device access

## Architecture

- **Language**: Rust (memory-safe, high-performance)
- **GUI Framework**: Iced (reactive, immediate-mode rendering)
- **Audio Engine**: Custom engine with async I/O (maolan-engine crate)
- **Threading**: Tokio async runtime for non-blocking operations
- **Plugin Hosting**: LV2 support via lilv library
- **File I/O**: Symphonia (audio codecs), midly (MIDI)

## Current Status

Maolan is under active development. Core DAW features are functional including:
- Multi-track recording and playback
- Audio and MIDI editing
- Mixing with real-time metering
- LV2 plugin support (Unix platforms)
- Session save/load
- File import/export

## License

See LICENSE file for details.

## Contributing

Contributions are welcome. Please ensure your code follows Rust best practices and includes appropriate tests.
