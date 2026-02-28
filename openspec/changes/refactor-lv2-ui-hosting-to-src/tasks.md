## 1. Implementation
- [x] 1.1 Add `src/plugins/lv2_ui` host module for LV2 UI lifecycle and window/event loop
- [x] 1.2 Wire GUI message/update path to invoke src-side LV2 UI host directly
- [x] 1.3 Remove engine action `TrackShowLv2PluginUiInstance` and related handlers
- [x] 1.4 Remove/trim LV2 UI spawning APIs from `engine/src/track.rs` and `engine/src/plugins/lv2.rs`
- [x] 1.5 Keep LV2 parameter synchronization through existing engine actions

## 2. Verification
- [x] 2.1 Build-check all crates and targets that include LV2
- [ ] 2.2 Verify opening LV2 UI from plugin graph still works
- [ ] 2.3 Verify closing LV2 UI window exits cleanly without hanging background threads
