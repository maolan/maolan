# VST3 Hosting Implementation Status

## Current Status: Phase 1 - Infrastructure Complete ✅

The VST3 hosting infrastructure has been successfully implemented and compiles without errors. However, **real VST3 plugin loading is not yet functional** - the current implementation uses stub placeholders.

## What's Implemented ✅

### Module Structure
- **`engine/src/vst3/mod.rs`** - Module root with exports and helper functions
- **`engine/src/vst3/port.rs`** - Port binding types (`PortBinding`, `BusInfo`, `ParameterInfo`)
- **`engine/src/vst3/interfaces.rs`** - Placeholder COM interface wrappers
- **`engine/src/vst3/host.rs`** - Plugin discovery infrastructure
- **`engine/src/vst3/processor.rs`** - VST3 processor with fallback to stub

### Core Features
1. **Type System** - All VST3 data structures defined and ready
2. **API Compatibility** - Maintains backward compatibility with existing `vst3.rs` stub
3. **Discovery Infrastructure** - Plugin scanning system (currently returns empty due to stub)
4. **Processor Framework** - Complete audio processing pipeline with passthrough fallback
5. **Parameter System** - Framework for parameter discovery and control (stub)
6. **FreeBSD Support** - VST3 search paths include FreeBSD locations

### Dependencies Added
- `vst3 = "0.3"` - VST3 COM bindings
- `serde` + `serde_json` - Serialization for plugin state
- `libloading = "0.8"` - Dynamic library loading

## What's NOT Implemented ❌

### Phase 1 Gaps
- **Real COM Interface Access** - The `vst3` crate v0.3 is designed for *creating* plugins, not hosting them
- **Plugin Loading** - `PluginFactory::from_module()` returns error
- **Audio Processing** - Currently just passthrough, no actual VST3 process() calls
- **Parameter Control** - Stub methods return empty values
- **State Management** - Not yet implemented

### Future Phases
- **Phase 2: MIDI Support** - Not started
- **Phase 3: Parameters & State** - Not started
- **Phase 4: Plugin GUIs** - Not started
- **Phase 5: Engine Integration** - Not started

## Why Stubs?

The `vst3` crate v0.3 has design limitations for hosting:
- Many types (`IPluginFactory`, `PClassInfo`, `TUID`) are **private**
- The crate uses trait-based API primarily for *plugin creation*
- Direct COM interface access requires using unsafe trait methods
- No high-level hosting API provided

## Next Steps (Options)

### Option 1: Complete vst3 Crate Integration (Recommended)
Continue with the implementation plan using the `vst3` crate's trait-based API:
1. Implement COM interface wrappers using `ComWrapper` and traits
2. Use `IPluginFactoryTrait`, `IComponentTrait`, etc.
3. Handle unsafe COM calls properly
4. Estimated effort: 2-3 weeks for Phase 1 completion

### Option 2: Use vst3-sys (Lower Level)
Switch to `vst3-sys` for raw VST3 SDK bindings:
- More control but significantly more unsafe code
- Direct C++ API access
- Estimated effort: 3-4 weeks

### Option 3: Wait for Mature Library
Monitor Rust audio ecosystem for:
- `rusty-daw-plugin-host` (currently v0.0.0)
- Future VST3 hosting crates
- Community developments

### Option 4: Manual COM Implementation
Write custom COM interfaces from scratch:
- Maximum control
- Most work (~4-6 weeks)
- Highest maintenance burden

## Current Behavior

### What Works
- ✅ Code compiles successfully (with warnings)
- ✅ Plugin scanning won't crash (returns empty list)
- ✅ VST3 processors can be created (fallback to stub)
- ✅ Audio passes through unchanged (passthrough mode)
- ✅ Track integration works as before
- ✅ FreeBSD VST3 paths configured

### What Doesn't Work
- ❌ Actual VST3 plugin discovery
- ❌ Real plugin instantiation
- ❌ VST3 audio processing
- ❌ Parameter discovery/control
- ❌ MIDI routing
- ❌ State save/restore
- ❌ Plugin GUIs

## Testing

To test the current implementation:

```bash
cd /home/meka/repos/maolan
cargo build
cargo run
```

The application will:
1. Compile without errors
2. Run normally with LV2 support
3. VST3 plugin list will be empty
4. Any VST3 processors created will passthrough audio

## Files Modified

### New Files
- `engine/src/vst3/mod.rs`
- `engine/src/vst3/port.rs`
- `engine/src/vst3/interfaces.rs`
- `engine/src/vst3/host.rs`
- `engine/src/vst3/processor.rs`

### Modified Files
- `engine/Cargo.toml` - Added dependencies
- `engine/src/vst3_old.rs` - Renamed from `vst3.rs` (backup)

### Unchanged
- `engine/src/lib.rs` - Already had `pub mod vst3`
- Track processing logic - Works with stubs
- LV2 plugin hosting - Unaffected

## Implementation Plan

See `/home/meka/repos/maolan/VST3_IMPLEMENTATION_PLAN.md` for the complete 5-phase implementation strategy.

## Resources

### Documentation
- [vst3 crate](https://lib.rs/crates/vst3)
- [IPluginFactory docs](https://coupler.rs/vst3-rs/vst3/Steinberg/struct.IPluginFactory.html)
- [VST3 SDK](https://steinbergmedia.github.io/vst3_doc/)
- [A Robust VST3 Host for Rust](https://renauddenis.com/case-studies/rust-vst)

### Community
- Rust Audio Discord - #vst3 channel
- Coupler Zulip - #vst3 channel

---

**Last Updated:** 2026-02-26
**Status:** Infrastructure complete, awaiting real COM implementation
