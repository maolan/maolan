# Maolan

A modern digital audio workstation (DAW) built with Rust, designed for professional audio recording, editing, mixing, and production with comprehensive plugin support.

## Features

### Core DAW Features

- **Multi-track Audio & MIDI Recording**: Record multiple audio and MIDI tracks simultaneously with real-time preview
- **Non-linear Editing**: Full timeline editor with waveform visualization, clip trimming, moving, copying, and snap-to-grid alignment
- **Mixing Console**: Per-track level faders (-90dB to +20dB), pan/balance controls, and real-time metering with visual indicators
- **Transport Controls**: Play, pause, stop, record, loop, and punch recording with bar-snapped ranges
- **Session Management**: Save and load complete project sessions with organized file structure and relative path support
- **Export**: Render projects to WAV format with configurable sample rate and full mix-down

### Track Management

- **Audio Tracks**: Configurable inputs/outputs with real-time monitoring and per-track level/balance control
- **MIDI Tracks**: MIDI input/output routing and recording with piano roll editing
- **Track Controls**: Arm/disarm, mute, solo, input/disk monitoring toggle
- **Flexible Layout**: Drag-to-reorder tracks, adjustable track heights, multi-select operations (Shift/Ctrl+click)
- **Hardware I/O**: Route tracks to hardware outputs with master level and balance controls
- **Quick Plugin Access**: Double-click any track to open its plugin graph view

### Audio & MIDI Editing

**Audio Editing:**
- **Clip Operations**: Create, move, resize, trim, copy, and delete audio clips with precision handles
- **Visual Editing**: Waveform display with peak caching, marquee selection, drag-and-drop editing
- **Timeline**: Bar/beat ruler with BPM control, zoom (1-256 bars visible), horizontal/vertical scrolling
- **Recording**: Multi-track recording with real-time waveform preview, automatic file organization
- **Snap Modes**: No snap, Bar, Beat, 1/8, 1/16, 1/32, 1/64 for precise alignment

**MIDI Editing:**
- **Piano Roll Editor**: Visual MIDI note editing across 10-octave range (120 notes)
- **Note Visualization**: Color-coded velocity intensity and MIDI channel distinction
- **Controller Lane**: Dedicated 140px height controller editing area
- **Independent Zoom**: Separate X and Y axis zoom controls
- **Grid Overlay**: Beat/bar grid with white/black key distinction

### File Format Support

**Audio Import:**
- WAV (native read/write)
- MP3, FLAC, OGG Vorbis, AIFF
- All formats supported by Symphonia codec library
- **Automatic sample rate conversion** using high-quality Sinc interpolation (rubato)
- Batch import with progress tracking

**Audio Export:**
- **WAV Export**: Stereo mix-down with configurable sample rate
- Respects all track levels, balance, mute/solo states
- Progress reporting with per-track feedback

**MIDI:**
- Standard MIDI file import/export (.mid, .midi)
- Tempo extraction with variable tempo support
- Format 0 and Format 1 support
- Tick-to-sample conversion with tempo map

**Sessions:**
- JSON-based session format
- Organized directory structure (audio/, midi/, plugins/, peaks/)
- Portable session files with relative paths
- Automatic peak cache generation

### Plugin Support

Maolan supports three major plugin formats with platform-specific availability:

**LV2 Plugins (Linux/FreeBSD/OpenBSD):**
- Full LV2 plugin hosting via lilv library
- Native plugin UI support via X11 integration
- Multi-instance support across tracks
- Plugin state persistence (control values and properties)
- Audio and MIDI I/O routing

**VST3 Plugins (All Platforms):**
- VST3 plugin discovery and loading
- Platform-specific UI integration (Win32 on Windows, X11 on Unix)
- Multi-instance support
- Plugin state management
- Full audio and MIDI routing

**CLAP Plugins (All Platforms):**
- CLAP plugin discovery and scanning
- UI window management from main thread
- Per-track plugin management
- Instance state persistence

**Plugin Features:**
- **Visual Plugin Graph**: Drag-and-connect interface for routing audio and MIDI through plugins
- **Plugin Browser**: Searchable plugin list with multi-format selection
- **Format Switching**: Choose between available plugin formats (LV2/VST3/CLAP)
- **Quick Access**: Double-click plugin instances to open native or generic UI
- **Generic UI**: Automatic slider-based control interface for plugins without native UI
- **Connection Validation**: Type-safe routing (audio/MIDI cannot cross-connect)
- **Cycle Detection**: Prevents feedback loops in routing

### Advanced Routing

- **Connections View**: Visual connection matrix for routing audio and MIDI
- **Track-to-Track**: Route audio and MIDI between tracks
- **Hardware MIDI**: Connect hardware MIDI devices to tracks
- **Cycle Detection**: Prevents feedback loops in routing
- **Plugin Chains**: Build complex signal chains with multiple plugins per track
- **Color-Coded Connections**: Visual distinction between audio and MIDI routing

## Platform Support

### Linux
- **Audio Backend**: ALSA (default), JACK (available)
- **System Requirements**: ALSA development libraries, optionally JACK
- **Plugin Support**: LV2 (default), VST3, CLAP
- **GUI Backend**: X11 (preferred), Wayland (via X11 compatibility)

### FreeBSD
- **Audio Backend**: OSS (default), JACK (available)
- **System Requirements**: Base system OSS, optionally JACK
- **Plugin Support**: LV2 (default), VST3, CLAP

### OpenBSD
- **Audio Backend**: sndio (default), JACK (available)
- **System Requirements**: Base system sndio, optionally JACK
- **Plugin Support**: LV2 (default), VST3, CLAP

### macOS
- **Audio Backend**: CoreAudio
- **MIDI Backend**: CoreMIDI
- **System Requirements**: macOS with CoreAudio framework
- **Plugin Support**: VST3 (default), CLAP

### Windows
- **Audio Backend**: WASAPI (default), ASIO (available via cpal)
- **MIDI Backend**: Windows MIDI (via midir)
- **System Requirements**: Windows 7 or later
- **Plugin Support**: VST3 (default), CLAP

## Building from Source

### Prerequisites

**All Platforms:**
- Rust toolchain (2024 edition or later)

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

**macOS:**
```bash
# No additional dependencies required (CoreAudio/CoreMIDI built-in)
# Optionally install JACK for alternative audio backend
brew install jack
```

**Windows:**
No additional system dependencies required (WASAPI built-in).

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

To enable debug logging with tracing output:
```bash
cargo run --release -- --debug
```

This provides info-level logging useful for troubleshooting plugin loading and audio backend issues.

### Environment Variables

- `MAOLAN_USE_WAYLAND="1"` - Force Wayland instead of X11 on Linux/FreeBSD/OpenBSD (X11 is preferred by default)

## Usage

### Starting a New Session

1. Launch Maolan
2. Click "File" > "New Session" or press `Ctrl+N` to create a new project
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
3. Enable "Input Monitor" to hear the input in real-time
4. Click the record button in the transport controls
5. Click stop when done - a new audio clip will appear on the timeline with waveform visualization

### Recording MIDI

1. Arm a MIDI track for recording
2. Set the MIDI input device in the track settings
3. Click record in the transport controls
4. Play notes on your MIDI controller
5. Click stop - a new MIDI clip appears with notes visible in the piano roll

### Importing Audio and MIDI Files

1. Click "File" > "Import" or press `Ctrl+I` to open the file browser
2. Select one or multiple files to import:
   - **Audio formats**: WAV, MP3, FLAC, OGG, AIFF, and all Symphonia-supported formats
   - **MIDI formats**: .mid, .midi files
3. A progress dialog shows the import status with:
   - Current file being processed
   - Overall progress (file X of Y)
   - Per-file operation progress (Decoding/Resampling/Writing)
4. Audio files are automatically:
   - Converted to WAV format in the session's audio/ directory
   - Resampled to match the session sample rate if needed (high-quality Sinc interpolation)
   - Added to new tracks with the file's base name
5. MIDI files are:
   - Copied to the session's midi/ directory
   - Added to new MIDI tracks
   - Analyzed for tempo information and tempo changes

### Exporting Projects

1. Click "File" > "Export" or press `Ctrl+E`
2. Choose destination file in the save dialog
3. A progress dialog shows:
   - Track-by-track processing status
   - Overall export progress
4. The exported WAV file contains:
   - Stereo mix-down of all tracks
   - Applied levels, balance, and mute/solo states
   - Session sample rate

### Working with Plugins

**Plugin Format Selection:**
- Unix (Linux/FreeBSD/OpenBSD): LV2 (default), VST3, CLAP
- macOS: VST3 (default), CLAP
- Windows: VST3 (default), CLAP
- Use the format picker in the plugin dialog to switch between formats

**Accessing Track Plugins:**
1. **Double-click on a track** in the track list to open its plugin graph view
2. Or click the "Track Plugins" view button in the toolbar, then select a track

**Adding Plugins:**
1. In the Track Plugins view, the left panel shows all available plugins for the selected format
2. Filter plugins by name or category
3. Drag a plugin from the list onto the canvas to load it
4. Plugins appear as cards showing their input/output ports

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

**Plugin State:**
- All plugin parameter values are saved with sessions
- Plugin chains are restored when loading sessions
- Multiple instances of the same plugin can be used

### Editing Audio Clips

**Moving and Copying:**
- Drag clips to move them
- Hold Shift while dragging to copy clips
- Clips snap to grid based on selected snap mode

**Resizing and Trimming:**
- Drag the left or right edge of a clip to resize (5px handle width)
- Resizing adjusts the clip offset for precise trimming
- Visual resize handles appear when hovering over clip edges

**Snap Modes:**
- Access snap mode menu in the toolbar
- Options: No snap, Bar, Beat, 1/8, 1/16, 1/32, 1/64
- Affects clip movement, resizing, and loop/punch range setting

**Selection:**
- Click to select individual clips
- Shift+click or Ctrl+click for multi-select
- Marquee selection for regions
- Delete or Backspace to remove selected clips

### Editing MIDI in Piano Roll

1. Select a MIDI clip or track
2. The piano roll displays all MIDI notes
3. Visual features:
   - 10-octave range (C-2 to G8, 120 notes)
   - Color intensity shows velocity
   - Color variation shows MIDI channel
   - White/black key visual distinction
   - Beat/bar grid overlay
4. Zoom controls:
   - Independent X-axis (time) zoom
   - Independent Y-axis (pitch) zoom
5. Controller lane at bottom for automation editing

### Routing Audio and MIDI

1. Switch to the "Connections" view from the toolbar
2. The matrix shows all available inputs and outputs:
   - Track inputs/outputs
   - Hardware audio interfaces
   - Hardware MIDI devices
   - Plugin ports (when plugins are loaded)
3. Drag from an output port to an input port to create a connection
4. Audio and MIDI connections are color-coded
5. Delete connections by selecting and pressing Delete/Backspace

### Mixing

1. Use the mixer panel on the right side of the window (or dedicated mixer view)
2. Each track has:
   - **Fader**: Adjust level from -90dB to +20dB (0.1dB steps)
   - **Balance/Pan**: Control stereo positioning (-100 to +100, center = 0)
   - **Meter**: Real-time level visualization with visual tick marks
   - **Mute**: Disable track output
   - **Solo**: Listen to only soloed tracks (takes priority over mute)
   - **Arm**: Enable recording for the track
3. Monitor levels in real-time during playback
4. Use solo to isolate tracks during mixing
5. Adjustments are saved with the session

### Transport and Looping

**Transport Controls:**
- **Play** (Space): Start playback from current position
- **Pause** (Shift+Space): Pause playback, resume from same position
- **Stop**: Stop playback and return to start
- **Record**: Arm global recording (requires tracks to be armed)
- **Rewind**: Jump to start of session
- **Fast Forward**: Jump to end of content

**Loop Recording:**
1. Enable the Loop toggle in transport controls
2. Set loop range by dragging on the ruler (bar-snapped)
3. Playback will loop within the range
4. Recording in loop mode records multiple takes

**Punch Recording:**
1. Enable the Punch toggle in transport controls
2. Set punch-in/punch-out range on the ruler
3. Recording only happens within the punch range
4. Useful for fixing specific sections

### Session Management

**Session File Structure:**
```
session_directory/
├── session.json          # Main session file (tracks, routing, settings)
├── audio/                # Audio clips (WAV format)
├── midi/                 # MIDI clips (.mid files)
├── peaks/                # Waveform peak cache (512 bins per clip)
└── plugins/              # Plugin state (reserved for future use)
```

**Saving Sessions:**
- `Ctrl+S` - Save current session (updates existing file)
- `Ctrl+Shift+S` - Save As (create new session file in new location)
- Sessions include:
  - All tracks with configurations
  - Audio/MIDI clips with positions and offsets
  - Track levels, balance, mute/solo/arm states
  - Plugin chains and connections
  - Track routing connections
  - Loop/punch ranges
  - UI layout preferences (zoom, scroll positions, panel sizes)

**Loading Sessions:**
- `Ctrl+O` - Open existing session
- All files are referenced with relative paths for portability
- Hardware state is validated and restored
- Peak caches are automatically regenerated if missing

**Creating New Sessions:**
- `Ctrl+N` - Create fresh session
- Automatically creates directory structure
- Prompts for session name and location

## Keyboard Shortcuts

### File Operations
| Shortcut          | Action                        |
| ----------------- | ----------------------------- |
| `Ctrl+N`          | Create a new session          |
| `Ctrl+O`          | Open an existing session      |
| `Ctrl+S`          | Save the current session      |
| `Ctrl+Shift+S`    | Save the current session as   |
| `Ctrl+I`          | Import audio or MIDI files    |
| `Ctrl+E`          | Export project to WAV         |

### Transport Controls
| Shortcut          | Action                        |
| ----------------- | ----------------------------- |
| `Space`           | Toggle playback (play/stop)   |
| `Shift+Space`     | Pause playback                |

### Editing Operations
| Shortcut          | Action                        |
| ----------------- | ----------------------------- |
| `Delete`          | Delete selected item(s)       |
| `Backspace`       | Delete selected item(s)       |
| `Shift` (hold)    | Multi-select or copy modifier |
| `Ctrl` (hold)     | Multi-select modifier         |

## Audio Configuration

Audio backend settings can be configured through the settings interface:

**General Settings:**
- **Backend Selection**: Choose audio system (ALSA/JACK/OSS/sndio/WASAPI/ASIO/CoreAudio)
- **Device Selection**: Choose your audio interface from available devices
- **Bit Depth**: 8, 16, 24, or 32-bit audio (Unix platforms)
- **Buffer Size (Period Frames)**: 64-8192 samples (affects latency, auto-normalized to power of 2)
- **Number of Periods**: Buffer count configuration
- **Sync Mode**: Enable/disable synchronization

**Platform-Specific:**
- **Exclusive Mode** (OSS only): Exclusive device access for lower latency
- **ASIO** (Windows): Optional ASIO driver support for pro audio interfaces

**Backend Options by Platform:**
- Linux: ALSA (default), JACK
- FreeBSD: OSS (default), JACK
- OpenBSD: sndio (default), JACK
- macOS: CoreAudio
- Windows: WASAPI (default), ASIO

## Architecture

### Technology Stack

**Core:**
- **Language**: Rust (2024 edition, memory-safe, high-performance)
- **GUI Framework**: Iced 0.14.0 (reactive, immediate-mode rendering)
- **Async Runtime**: Tokio (non-blocking operations)

**Audio Engine:**
- Custom engine with async I/O (maolan-engine crate)
- Platform-specific backend implementations
- Lock-free message passing

**Plugin Hosting:**
- LV2 support via lilv library (Unix)
- VST3 support via vst3 crate (all platforms)
- CLAP support via libloading (all platforms)

**File I/O:**
- Symphonia (multi-format audio decoding)
- Wavers (WAV file read/write)
- Midly (MIDI parsing)
- Rubato (high-quality sample rate conversion with Sinc interpolation)

**GUI Components:**
- Iced core widgets
- Iced_aw (additional widgets)
- Iced_drop (drag-and-drop support)
- Iced_fonts (Lucide icon font)

**Serialization:**
- Serde/serde_json for session files

## Codebase Structure

The `maolan` GUI codebase is organized into several modules within the `src` directory. This structure separates concerns and makes the project easier to navigate and maintain.

**Top-Level Files:**
-   `main.rs`: The entry point of the application. Initializes the application, sets up the GUI (X11 preference on Unix), and handles the main event loop.
-   `add_track.rs`: Contains the logic for the "Add Track" dialog and the process of adding new audio or MIDI tracks to the session.
-   `hw.rs`: Provides an abstraction layer for hardware communication, delegating to the appropriate backend in `maolan-engine`.
-   `menu.rs`: Defines the structure and content of the main application menu (File, Edit, Track).
-   `message.rs`: Defines the message enums used for communication between different parts of the application, following the Elm architecture pattern used by Iced.
-   `toolbar.rs`: Defines the main toolbar, including its buttons (New, Open, Save, Import, Export, views) and layout.
-   `ui_timing.rs`: Utilities for managing UI-related timing and animations.

**Modules:**

-   `connections/`: Implements the visual routing matrix where users can connect audio and MIDI ports between tracks, plugins, and hardware.
-   `gui/`: Core GUI logic, including the main application state, message updates, and view rendering.
    -   `update.rs`: Handles all incoming messages and updates the application state accordingly.
    -   `view.rs`: Renders the main application window and all its components.
    -   `subscriptions.rs`: Manages subscriptions to external events, like MIDI input, engine messages, and keyboard events.
    -   `session.rs`: Logic for session management (new, load, save, save as, import, export).
-   `plugins/`: Plugin host implementations for different formats.
    -   `lv2/`: LV2 plugin support (Unix only)
    -   `vst3/`: VST3 plugin support (all platforms)
    -   `clap/`: CLAP plugin support (all platforms)
-   `state/`: Defines the data structures that represent the application's state, such as tracks, clips, connections, and settings.
-   `style/`: Contains styling rules for Iced widgets, ensuring a consistent look and feel. This includes custom styles for buttons, sliders, and other UI elements.
-   `widget/`: Custom Iced widgets developed specifically for Maolan:
    -   `piano.rs`: Piano roll editor with 10-octave range and controller lane
    -   Custom faders, meters, and sliders
-   `workspace/`: Implements the different "pages" or "views" of the application:
    -   Timeline editor with waveform display
    -   Mixer view with channel strips
    -   Connections matrix
    -   Track plugins graph view
    -   Piano roll editor

**Engine (`engine/src/`):**
-   `hw/`: Platform-specific audio backend implementations (ALSA, OSS, sndio, JACK, WASAPI, ASIO, CoreAudio)
-   `plugins/`: Plugin host engine logic for LV2, VST3, and CLAP
-   Core audio processing and routing logic

## Current Status

Maolan is under active development. Core DAW features are functional including:

**Implemented:**
- Multi-track audio and MIDI recording and playback
- Audio and MIDI editing with visual editors (waveform, piano roll)
- Mixing with real-time metering and solo/mute
- Multi-format plugin support (LV2, VST3, CLAP)
- Visual plugin graph editor with routing
- Session save/load with organized file structure
- Audio import (WAV, MP3, FLAC, OGG, AIFF, etc.) with automatic resampling
- MIDI import with tempo extraction
- WAV export with mix-down
- Advanced routing with connections matrix
- Snap-to-grid editing (multiple snap modes)
- Loop and punch recording
- Cross-platform support (Linux, FreeBSD, OpenBSD, macOS, Windows)

**Recent Improvements:**
- WAV export functionality (commit 7be72ba)
- Windows VST3 UI integration (commit 2edf370)
- Snap-to-grid editing (commit b7f0238)
- CLAP UI improvements with main-thread handling (commit 25d3422)
- Wayland compatibility (commit 0c2f3ae)
- Waveform rendering fixes for clip resize (commit 7785c0c)
- Window close handling improvements (commit 7d43535)

## License

See LICENSE file for details.

## Contributing

Contributions are welcome. Please ensure your code follows Rust best practices and includes appropriate tests.

### Development Guidelines

- Follow Rust 2024 edition conventions
- Use `cargo clippy` to check for common issues
- Test on multiple platforms when possible
- Document new features in the README
- Keep commit messages clear and descriptive
