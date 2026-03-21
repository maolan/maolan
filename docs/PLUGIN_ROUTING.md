# Plugin Routing and Sidechains

Last updated: 2026-03-21

## Overview

Maolan uses a per-track plugin graph instead of a fixed insert-only chain. A track can contain:

- Track audio inputs and outputs
- Track MIDI inputs and outputs
- LV2 plugin nodes on Unix
- CLAP plugin nodes
- VST3 plugin nodes

Connections are explicit. Audio and MIDI routing are managed separately.

Audio clips on supported Unix builds can also carry their own per-clip plugin graph. That graph is separate from the parent track graph and is stored with the clip in session data.

## Default Behavior

- A new track starts with default passthrough routing from track inputs to track outputs.
- MIDI passthrough is also created for the first input/output path.
- Loading plugins does not remove existing routing automatically; the graph determines the signal path.
- Opening an audio clip plugin graph seeds a default passthrough clip graph if the clip does not already have one.

## Audio Ports

Plugin audio ports are shown as:

- Main audio ports
- Extra / auxiliary / sidechain ports

Extra audio ports, including sidechains, are rendered in orange so they are visually distinct from main audio ports.

This matters for plugins such as:

- compressors with dedicated sidechain inputs
- plugins exposing auxiliary sends/returns
- plugins with additional analysis or detector inputs

## MIDI Ports

MIDI connections are explicit in the graph as well.

- A plugin with MIDI input must be connected to receive MIDI.
- A plugin with MIDI output must be connected to a downstream node or track output for events to continue.
- VST3, CLAP, and LV2 MIDI paths are handled per plugin format and per track graph node.

## Session and Template Restore

Plugin graph state is part of save/restore workflows.

- Session save/load restores plugin graph topology.
- Session save/load also restores per-audio-clip plugin graphs.
- Track templates restore plugin order, plugin state, and graph connections.
- Mixed Unix graphs containing LV2 and CLAP plugins are restored in saved order.
- VST3 state is also preserved through the plugin restore paths used by the current host integration.

## Per-Clip FX Notes

- Per-clip plugin graphs are currently audio-only.
- They are opened by double-clicking an audio clip in the timeline.
- A grouped audio clip can have its own plugin graph, and child audio clips can also keep their own plugin graphs.
- In the current render path, child clips are rendered first, then group-level fades are applied, then the group plugin graph is processed.

## Practical Workflow

Typical sidechain setup:

1. Open the track plugin graph.
2. Load the destination plugin, for example a compressor.
3. Identify the orange sidechain input port on that plugin.
4. Connect the source track or upstream plugin output to that sidechain port.
5. Keep the main audio path connected separately to the plugin’s main input.

Typical serial insert setup:

1. Disconnect the default track-input to track-output passthrough if needed.
2. Connect `Track Input -> Plugin A -> Plugin B -> Track Output`.

Typical MIDI instrument/effect setup:

1. Connect MIDI source to the instrument/effect MIDI input.
2. Connect audio output of the instrument/effect to the next plugin or track output.
3. If the plugin emits MIDI, connect its MIDI output explicitly.

## Notes and Boundaries

- Plugin compatibility still depends on the host implementation and the plugin itself.
- Some plugins expose unusual bus layouts; the graph is the source of truth for what is connected.
- Undo/redo covers graph connection edits and plugin load/unload actions, but low-level state-restore actions are internal replay mechanisms rather than user-facing history items.
