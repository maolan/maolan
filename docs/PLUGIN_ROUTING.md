# Plugin Routing and Sidechains

Last updated: 2026-03-11

## Overview

Maolan uses a per-track plugin graph instead of a fixed insert-only chain. A track can contain:

- Track audio inputs and outputs
- Track MIDI inputs and outputs
- LV2 plugin nodes on Unix
- CLAP plugin nodes
- VST3 plugin nodes

Connections are explicit. Audio and MIDI routing are managed separately.

## Default Behavior

- A new track starts with default passthrough routing from track inputs to track outputs.
- MIDI passthrough is also created for the first input/output path.
- Loading plugins does not remove existing routing automatically; the graph determines the signal path.

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
- Track templates restore plugin order, plugin state, and graph connections.
- Mixed Unix graphs containing LV2 and CLAP plugins are restored in saved order.
- VST3 state is also preserved through the plugin restore paths used by the current host integration.

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
