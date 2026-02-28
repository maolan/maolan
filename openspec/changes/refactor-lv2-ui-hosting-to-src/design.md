## Context
LV2 UI hosting logic currently lives in `engine/src/plugins/lv2.rs` and is triggered through engine actions. That couples GUI/event-loop concerns (GTK/external UI windows) with real-time engine ownership.

## Goals / Non-Goals
- Goals:
  - Move LV2 UI window lifecycle ownership to `src`
  - Align LV2 UI architecture with existing CLAP/VST3 GUI-side hosts
  - Keep engine responsibilities focused on plugin processing and state
- Non-Goals:
  - Reworking LV2 DSP processing
  - Changing plugin graph UX behavior
  - Adding new UI frameworks

## Decisions
- Create `src/plugins/lv2_ui` as the UI host boundary.
- Remove direct engine action path for LV2 UI opening.
- Reuse existing engine messages for parameter/state updates when LV2 UI interaction needs to change plugin values.

## Risks / Trade-offs
- Some LV2 UI synchronization currently tied to engine-owned state may need explicit message bridging.
- Linux-only UI hosting behavior needs careful cfg-gating to avoid regressions on non-Linux builds.

## Migration Plan
1. Introduce src-side LV2 UI host and wire GUI invocation path.
2. Remove engine action + track APIs that open LV2 windows.
3. Keep parameter update path through existing engine message actions.
4. Build-check and validate close behavior.
