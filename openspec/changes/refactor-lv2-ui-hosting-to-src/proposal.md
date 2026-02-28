# Change: Move LV2 UI hosting from engine to src

## Why
LV2 UI window hosting currently lives inside `engine`, mixing DSP/runtime responsibilities with GUI toolkit integration and window event loops. This complicates threading behavior, platform portability, and lifecycle ownership for plugin editors.

## What Changes
- Move LV2 UI open/show/close orchestration out of `engine/src/plugins/lv2.rs` into a new GUI-side host module under `src/plugins/`
- Remove engine action handling that directly opens LV2 plugin UIs (`TrackShowLv2PluginUiInstance`)
- Route LV2 UI open requests from the GUI directly to the new src-side LV2 UI host, matching CLAP/VST3 hosting structure
- Keep engine responsibilities focused on audio processing and plugin state updates; GUI host communicates parameter edits through existing engine actions
- Preserve current user behavior: double-click LV2 node in plugin graph opens/closes plugin UI window without blocking main app flow

## Impact
- Affected specs: `plugin-ui-hosting` (new capability)
- Affected code:
  - `engine/src/plugins/lv2.rs`
  - `engine/src/track.rs`
  - `engine/src/engine.rs`
  - `engine/src/message.rs`
  - `src/connections/plugins.rs`
  - `src/gui/update.rs`
  - `src/plugins/` (new `lv2_ui` module)
