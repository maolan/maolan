# Maolan Shortcuts and Gestures

Last updated: 2026-03-15

## Keyboard Shortcuts
### Global / Session
- `Ctrl+N`: New session
- `Ctrl+O`: Open session
- `Ctrl+S`: Save session
- `Ctrl+Shift+S`: Save session as
- `Ctrl+I`: Import files
- `Ctrl+E`: Open export dialog
- `Ctrl+T`: Add track
- `Ctrl+Z`: Undo
- `Ctrl+Shift+Z`: Redo
- `Ctrl+Y`: Redo
- `Delete` or `Backspace`: Remove selected item(s)

### Transport
- `Space`: Toggle play/pause
- `Shift+Space`: Pause

### Piano Tools
- `Q`: Quantize selected notes
- `H`: Humanize selected notes
- `G`: Groove selected notes

## Mouse Actions and Gestures
### Workspace / Track List
- `Left click track`: Select track
- `Double click track`: Open track plugin view
- `Right click track`: Open track context menu
  - Automation lane add
  - Rename
  - Freeze/Unfreeze/Flatten
  - Save as template
  - VCA assignment
  - Aux send controls
  - MIDI learn arm/clear
- `Drag track` (grab track body): Reorder track
- `Drag bottom track edge`: Resize track height

### Timeline Clips
- `Left click clip`: Select clip
- `Left click empty editor`: Deselect clips
- `Left drag clip`: Drag/move clip (or group if multi-selected)
- `Ctrl + drag clip`: Copy clip while dragging
- `Drag clip left/right edge`: Resize clip bounds
- `Drag clip fade handles`: Resize fade-in/fade-out
- `Middle click clip`: Split clip at current cursor/snap position
- `Double click MIDI clip`: Open MIDI piano roll
- `Right click clip`: Open clip context menu
  - Rename
  - Take lane commands
  - Mute/unmute
  - Fade enable/disable
  - Audio warp actions (audio clips)

### Track Header Markers
- `Right click empty marker/header area`: Open the create-marker dialog at the snapped timeline position
- `Left drag marker`: Move marker horizontally; snapping is applied on drop
- `Right click marker`: Rename marker
- `Middle click marker`: Delete marker

### Selection Gestures
- `Left drag on empty editor`: Marquee clip selection rectangle
- `Right drag on MIDI lane`: Create empty MIDI clip

### Comp Tool
- In Comp tool mode:
  - `Left drag across takes`: Swipe comp (promote active take/mute others in range)

### Ruler (Top Timeline)
- `Left click`: Move transport playhead
- `Left drag`: Set loop range (snap-aware)
- `Right click`: Clear loop range

### Tempo / Time Signature Lane
- `Left click marker`: Select marker
- `Shift+Left click marker`: Add/remove marker from selection
- `Left drag selected marker(s)`: Move marker(s) in time
- `Left click empty timing lane`: Clear timing selection / begin punch drag
- `Left drag empty timing lane`: Set punch range
- `Right click`: Decrement tempo/TS controls (left control zone) or open marker context menu
- `Middle click` on tempo lane: Add tempo point
- `Middle click` on time-signature lane: Add time-signature point
- `Mouse wheel` over left control zone: Adjust tempo or time-signature values

### Piano Roll (Mouse)
- `Click/drag notes`: Select and move notes
- `Drag note edge`: Resize note start/end
- `Drag rectangle`: Box-select notes
- `Drag in empty area`: Create notes
- `Controller/SysEx interactions`: Insert/edit/move/delete according to active lane/tools

### Plugin Graph
- `Double click track`: Open the track plugin/routing graph
- `Drag plugin node`: Move plugin node in graph
- `Drag from port to port`: Create audio or MIDI connection
- `Select connection + delete`: Remove selected graph connection
- Sidechain / auxiliary audio ports are rendered separately from main ports

## Notes
- Current keyboard handling is `Ctrl`-based in code paths (including on macOS builds).
- Some actions are context-dependent (current view/tool/selection state).
