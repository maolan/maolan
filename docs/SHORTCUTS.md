# Maolan Shortcuts and Gestures

Last updated: 2026-03-16

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
- `Escape`: Cancel or clear the current context-dependent interaction

### Transport
- `Space`: Toggle play/stop
- `Shift+Space`: Pause

### Piano Tools
- `Q`: Quantize selected notes
- `H`: Humanize selected notes
- `G`: Groove selected notes

## Mouse Actions and Gestures
### Workspace / Track List
- `Left click track`: Select track
- `Ctrl+Left click track`: Add track to the current selection
- `Double click track`: Open track plugin view
- `Right click track`: Open track context menu
  - Track actions such as automation lanes, rename, sends/returns, MIDI learn, freeze/flatten, template save, and grouping/VCA actions depending on track state
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
  - Clip actions such as rename, take-lane controls, mute/unmute, fade toggle, and audio warp actions for audio clips

### Track Header Markers
- `Right click empty marker/header area`: Open the create-marker dialog at the snapped timeline position
- `Left drag marker`: Move marker horizontally; snapping is applied on drop
- `Right click marker`: Rename marker
- `Middle click marker`: Delete marker

### Selection Gestures
- `Left drag on empty editor`: Marquee clip selection rectangle
- `Right drag on MIDI lane`: Create empty MIDI clip

### Ruler (Top Timeline)
- `Left click`: Move transport playhead
- `Left drag`: Set loop range (snap-aware)
- `Right click`: Clear loop range

### Zoom Controls
- Main editor zoom: Bottom-right horizontal slider
- Piano roll horizontal zoom: Bottom slider in the MIDI editor
- Piano roll vertical zoom: Right-side vertical slider in the MIDI editor

### Tempo / Time Signature Lane
- `Left click marker`: Select marker
- `Shift+Left click marker`: Add/remove marker from selection
- `Left drag selected marker(s)`: Move marker(s) in time
- `Left click empty timing lane`: Clear timing selection and move the playhead
- `Left drag empty timing lane`: Clear timing selection and set punch range
- `Right click marker`: Open marker context menu
  - Duplicate
  - Reset to previous
  - Delete
- `Right click empty timing lane`: Clear timing selection and clear punch range
- `Right drag empty timing lane`: Clear timing selection and set punch range
- `Middle click` on tempo lane: Add tempo point
- `Middle click` on time-signature lane: Add time-signature point
- `Middle drag` an existing punch-range edge: Adjust punch start or end
- `Mouse wheel` over left control zone on tempo row: Adjust tempo
- `Mouse wheel` over left control zone on time-signature row, left half: Adjust numerator
- `Mouse wheel` over left control zone on time-signature row, right half: Adjust denominator

### Piano Roll (Mouse)
- `Click/drag notes`: Select and move notes
- `Drag note edge`: Resize note start/end
- `Left drag empty area`: Box-select notes
- `Right drag empty area`: Create notes
- `Middle click note`: Delete note
- `Mouse wheel over note`: Adjust note velocity
- `Controller lanes`: Left drag adjusts a point/value, middle click/drag erases, right drag draws
- `Mouse wheel over controller event`: Adjust controller value
- `SysEx lane`: Left drag moves SysEx event, double click opens SysEx editor

### Plugin Graph
- `Double click track`: Open the track plugin/routing graph
- `Drag plugin node`: Move plugin node in graph
- `Drag from port to port`: Create audio or MIDI connection
- `Select connection + delete`: Remove selected graph connection
- `Select plugin + delete`: Remove selected plugin instance

## Notes
- Current keyboard handling is `Ctrl`-based in code paths (including on macOS builds).
- Some actions are context-dependent (current view/tool/selection state).
- The main editor zoom is geometric rather than linear, so equal slider movement produces equal zoom-ratio changes.
