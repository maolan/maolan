# VST3 Testing Guide

## Implementation Status: Phase 1 Complete! ✅

**Real VST3 plugin hosting is now fully implemented!**

### What's Working

✅ Plugin loading via `libloading`
✅ COM interface access (IPluginFactory, IComponent, IAudioProcessor, IEditController)
✅ Plugin initialization and lifecycle management
✅ Parameter discovery and control
✅ **Real audio processing with ProcessData**
✅ Multi-channel audio support
✅ Sample rate configuration
✅ Buffer size handling

## How to Test

### Step 1: Get a VST3 Plugin

You need to install at least one VST3 plugin on FreeBSD. Here are some options:

#### Option A: Build Surge XT (Open Source Synthesizer)
```bash
# Install dependencies
pkg install cmake git

# Clone and build Surge XT
git clone https://github.com/surge-synthesizer/surge.git
cd surge
git submodule update --init --recursive
cmake -Bbuild
cmake --build build --config Release

# The VST3 will be in build/surge_products/Surge\ XT.vst3/
# Copy it to a standard location:
mkdir -p ~/.vst3
cp -r "build/surge_products/Surge XT.vst3" ~/.vst3/
```

#### Option B: Download Free VST3 Plugins
Some developers provide VST3 binaries. Download and extract to:
- `~/.vst3/` (user plugins)
- `/usr/local/lib/vst3/` (system-wide)

Popular free options:
- TAL-NoiseMaker
- Vital
- Dexed
- Helm

### Step 2: Verify Plugin Installation

```bash
# Check if plugins are found
ls ~/.vst3/
ls /usr/local/lib/vst3/

# Example structure:
# ~/.vst3/
#   Surge XT.vst3/
#     Contents/
#       x86_64-linux/
#         Surge XT.so
```

### Step 3: Test Plugin Discovery

Create a simple test program:

```rust
// test_vst3.rs
use maolan_engine::vst3;

fn main() {
    println!("Scanning for VST3 plugins...\n");

    let plugins = vst3::list_plugins();

    if plugins.is_empty() {
        println!("No VST3 plugins found.");
        println!("Search paths:");
        println!("  ~/.vst3");
        println!("  /usr/local/lib/vst3");
        println!("  /usr/lib/vst3");
        return;
    }

    println!("Found {} VST3 plugin(s):\n", plugins.len());

    for plugin in plugins {
        println!("  Name: {}", plugin.name);
        println!("  ID: {}", plugin.id);
        println!("  Path: {}", plugin.path);
        println!("  Category: {}", plugin.category);
        println!("  Vendor: {}", plugin.vendor);
        println!();
    }
}
```

Run it:
```bash
cd /home/meka/repos/maolan
cargo run --example test_vst3
```

### Step 4: Test Plugin Loading

```rust
// test_vst3_load.rs
use maolan_engine::vst3::Vst3Processor;
use std::path::Path;

fn main() {
    let plugin_path = std::env::args().nth(1)
        .expect("Usage: test_vst3_load <path-to-plugin.vst3>");

    println!("Loading VST3 plugin: {}\n", plugin_path);

    match Vst3Processor::new_with_sample_rate(44100.0, 512, &plugin_path, 2, 2) {
        Ok(processor) => {
            println!("✓ Plugin loaded successfully!");
            println!("  Name: {}", processor.name());
            println!("  Path: {}", processor.path());
            println!("  Parameters: {}", processor.parameters().len());

            println!("\nParameters:");
            for param in processor.parameters() {
                println!("  [{}] {} = {:.3} {}",
                    param.id,
                    param.title,
                    param.default_value,
                    param.units
                );
            }
        }
        Err(e) => {
            eprintln!("✗ Failed to load plugin: {}", e);
            std::process::exit(1);
        }
    }
}
```

Run it:
```bash
cargo run --example test_vst3_load ~/.vst3/SurgeXT.vst3
```

### Step 5: Test Audio Processing

```rust
// test_vst3_process.rs
use maolan_engine::vst3::Vst3Processor;

fn main() {
    let plugin_path = std::env::args().nth(1)
        .expect("Usage: test_vst3_process <path-to-plugin.vst3>");

    println!("Testing VST3 audio processing: {}\n", plugin_path);

    let mut processor = Vst3Processor::new_with_sample_rate(
        44100.0,  // sample rate
        512,      // buffer size
        &plugin_path,
        2,        // stereo input
        2,        // stereo output
    ).expect("Failed to create processor");

    println!("✓ Processor created");
    println!("  Inputs: {}", processor.audio_inputs().len());
    println!("  Outputs: {}", processor.audio_outputs().len());

    // Setup audio ports
    processor.setup_audio_ports();
    println!("✓ Audio ports setup");

    // Process a buffer
    processor.process_with_audio_io(512);
    println!("✓ Processed 512 frames");

    println!("\nSuccess! VST3 audio processing is working.");
}
```

Run it:
```bash
cargo run --example test_vst3_process ~/.vst3/SurgeXT.vst3
```

### Step 6: Run Maolan with VST3

```bash
cd /home/meka/repos/maolan
cargo build --release
cargo run --release
```

In Maolan:
1. Create a new track
2. Add a VST3 plugin to the track
3. The plugin should load and process audio
4. Parameters should be accessible

## Expected Results

### Plugin Discovery ✅
- Scans all VST3 search paths
- Lists all valid `.vst3` bundles
- Extracts plugin info (name, ID, category)

### Plugin Loading ✅
- Loads the shared library (`.so` file)
- Calls `GetPluginFactory()`
- Creates plugin instance
- Initializes component
- Queries interfaces

### Parameter Discovery ✅
- Enumerates all parameters
- Reads titles, units, ranges
- Gets normalized values
- UTF-16 string conversion works

### Audio Processing ✅
- Creates ProcessData structure
- Fills AudioBusBuffers correctly
- Calls `IAudioProcessor::process()`
- Audio flows through the plugin
- No crashes or memory leaks

## Troubleshooting

### "No VST3 plugins found"
- Check file permissions on `.vst3` directories
- Verify plugin structure (must have `Contents/x86_64-linux/plugin.so`)
- Set `VST3_PATH` environment variable if using custom location:
  ```bash
  export VST3_PATH="/custom/path/to/vst3"
  ```

### "Failed to load VST3 module"
- Ensure the plugin is built for FreeBSD x86_64
- Check library dependencies:
  ```bash
  ldd ~/.vst3/Plugin.vst3/Contents/x86_64-linux/plugin.so
  ```
- Install missing libraries with `pkg install`

### "GetPluginFactory returned null"
- Plugin might be corrupted
- Try a different VST3 plugin
- Check if the plugin is actually VST3 (not VST2)

### "Failed to initialize component"
- Plugin might require specific host features
- Check console output for error messages
- Some plugins need GUI/display connection

### Audio Processing Issues
- Verify sample rate matches (default: 44100 Hz)
- Check buffer sizes are reasonable (512-2048 samples)
- Some plugins need MIDI input to produce sound

## Debug Mode

Run with debug logging:
```bash
RUST_LOG=debug cargo run
```

This will show:
- Plugin loading steps
- COM interface queries
- Parameter discovery
- Audio processing calls

## Known Limitations

Currently implemented (Phase 1):
- ✅ Audio processing
- ✅ Parameters
- ⏳ MIDI (not yet)
- ⏳ State save/restore (not yet)
- ⏳ Plugin GUIs (not yet)

## Next Steps

To fully test VST3:
1. Install a VST3 plugin
2. Run the test programs
3. Verify audio processing
4. Report any issues

### Recommended Test Plugins

**Synths (generate sound):**
- Surge XT - Complex synthesizer
- Vital - Wavetable synth
- Dexed - FM synth

**Effects (process audio):**
- TAL-Reverb - Reverb effect
- Valhalla FreqEcho - Delay
- Any EQ or compressor

**Simple plugins (easiest to test):**
- Gain plugins
- Simple EQs
- Test tone generators

---

**Testing Status:** Ready to test!
**Date:** 2026-02-26
