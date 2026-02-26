# VST3 Hosting Implementation Plan

## Overview
Implement full VST3 plugin hosting support in Maolan DAW, including audio processing, MIDI routing, parameter control, state management, and plugin GUI embedding. Build a safe abstraction layer around the low-level `vst3` crate, mirroring the existing LV2 architecture.

## User Requirements
- **Scope**: Everything including plugin GUIs (full VST3 support matching LV2 capabilities plus embedded editor windows)
- **Approach**: Build our own safe abstraction layer around `vst3` crate
- **Platform**: FreeBSD, Linux, Windows, macOS support

## Phase 1: Foundation and Basic Audio Processing

### 1.1 Dependencies
**File**: `engine/Cargo.toml`

Add dependencies:
```toml
[dependencies]
vst3 = "0.3"  # COM bindings for VST3 API
```

Platform-specific GUI dependencies (for Phase 4):
```toml
[target.'cfg(target_os = "linux")'.dependencies]
raw-window-handle = "0.6"

[target.'cfg(target_os = "freebsd")'.dependencies]
raw-window-handle = "0.6"

[target.'cfg(target_os = "windows")'.dependencies]
raw-window-handle = "0.6"
windows = { version = "0.52", features = ["Win32_UI_WindowsAndMessaging", "Win32_Foundation"] }

[target.'cfg(target_os = "macos")'.dependencies]
raw-window-handle = "0.6"
cocoa = "0.25"
objc = "0.2"
```

### 1.2 VST3 Abstraction Layer Structure
**New File**: `engine/src/vst3/mod.rs`

Create module structure:
```
engine/src/vst3/
├── mod.rs          # Public API, re-exports
├── host.rs         # Vst3Host (plugin discovery)
├── processor.rs    # Vst3Processor (instance management)
├── interfaces.rs   # Safe wrappers for COM interfaces
├── port.rs         # Port binding types
├── midi.rs         # MIDI event conversion
├── state.rs        # State management
└── gui.rs          # Plugin GUI integration (Phase 4)
```

### 1.3 Core Types
**File**: `engine/src/vst3/port.rs`

Define port binding system (similar to LV2):
```rust
#[derive(Debug, Clone)]
pub enum PortBinding {
    AudioInput {
        bus_index: usize,
        channel_index: usize,
    },
    AudioOutput {
        bus_index: usize,
        channel_index: usize,
    },
    Parameter {
        param_id: u32,
        index: usize,  // index in scalar_values vec
    },
    EventInput {
        bus_index: usize,
    },
    EventOutput {
        bus_index: usize,
    },
}

pub struct BusInfo {
    pub index: usize,
    pub name: String,
    pub channel_count: usize,
    pub is_active: bool,
}
```

### 1.4 Plugin Discovery
**File**: `engine/src/vst3/host.rs`

Implement plugin scanner:
```rust
pub struct Vst3Host {
    // Cached plugin list
    plugins: Vec<Vst3PluginInfo>,
}

impl Vst3Host {
    pub fn new() -> Self {
        Self { plugins: Vec::new() }
    }

    pub fn list_plugins(&mut self) -> Vec<Vst3PluginInfo> {
        // Scan default_vst3_search_roots() + VST3_PATH
        // For each .vst3 bundle:
        //   - Load module factory
        //   - Query class info (IPluginFactory)
        //   - Extract name, vendor, category, version
        //   - Count audio buses and parameters
        // Return sorted by name
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vst3PluginInfo {
    pub id: String,        // FUID as string
    pub name: String,
    pub vendor: String,
    pub path: String,      // Path to .vst3 bundle
    pub category: String,
    pub version: String,
    pub audio_inputs: usize,
    pub audio_outputs: usize,
    pub has_midi_input: bool,
    pub has_midi_output: bool,
}
```

### 1.5 Safe COM Interface Wrappers
**File**: `engine/src/vst3/interfaces.rs`

Create safe wrappers around vst3 crate's COM interfaces:
```rust
use vst3::Steinberg::*;
use vst3::{ComPtr, ComRef};

pub struct PluginFactory {
    ptr: ComPtr<IPluginFactory>,
}

impl PluginFactory {
    pub fn from_module(module_path: &str) -> Result<Self, String> {
        // Load shared library (.so, .dll, .dylib)
        // Get GetPluginFactory entry point
        // Wrap in ComPtr
    }

    pub fn create_instance(&self, class_id: &TUID) -> Result<PluginInstance, String> {
        // IPluginFactory::createInstance
        // Return wrapped IComponent
    }
}

pub struct PluginInstance {
    component: ComPtr<IComponent>,
    audio_processor: Option<ComPtr<IAudioProcessor>>,
    edit_controller: Option<ComPtr<IEditController>>,
    plugin_view: Option<ComPtr<IPlugView>>,
}

impl PluginInstance {
    pub fn initialize(&mut self, context: &HostContext) -> Result<(), String> {
        // IComponent::initialize
        // Query interfaces: IAudioProcessor, IEditController
    }

    pub fn set_active(&mut self, active: bool) -> Result<(), String> {
        // IComponent::setActive
    }

    pub fn setup_processing(&mut self, sample_rate: f64, max_samples: i32) -> Result<(), String> {
        // ProcessSetup with kSample32 or kSample64
        // IAudioProcessor::setupProcessing
    }
}
```

### 1.6 Vst3Processor Implementation
**File**: `engine/src/vst3/processor.rs`

Replace stub with real implementation:
```rust
pub struct Vst3Processor {
    // Plugin identity
    path: String,
    name: String,
    plugin_id: String,

    // COM interfaces
    instance: Option<PluginInstance>,

    // Audio I/O (reuse existing AudioIO)
    audio_inputs: Vec<Arc<AudioIO>>,
    audio_outputs: Vec<Arc<AudioIO>>,
    input_buses: Vec<BusInfo>,
    output_buses: Vec<BusInfo>,

    // Parameters
    parameters: Vec<ParameterInfo>,
    scalar_values: Arc<Mutex<Vec<f32>>>,

    // MIDI (Phase 2)
    event_inputs: Vec<EventBuffer>,
    event_outputs: Vec<EventBuffer>,

    // Processing state
    sample_rate: f64,
    max_block_size: usize,
    is_active: bool,
}

impl Vst3Processor {
    pub fn new(
        sample_rate: f64,
        buffer_size: usize,
        plugin_path: &str,
    ) -> Result<Self, String> {
        // 1. Load plugin factory from path
        // 2. Create plugin instance
        // 3. Initialize component
        // 4. Query buses (getBusInfo for audio/event)
        // 5. Create AudioIO for each audio channel
        // 6. Query parameters (IEditController::getParameterCount)
        // 7. Setup processing with sample_rate/buffer_size
        // 8. Activate component
    }

    pub fn process_with_audio_io(&mut self, frames: usize) -> Result<(), String> {
        // 1. Process all input AudioIO ports (input.process())
        // 2. Fill ProcessData structure:
        //    - numSamples = frames
        //    - numInputs/numOutputs from buses
        //    - Create AudioBusBuffers pointing to AudioIO buffers
        // 3. Call IAudioProcessor::process(&mut process_data)
        // 4. Mark outputs as finished
    }

    pub fn audio_inputs(&self) -> &[Arc<AudioIO>] {
        &self.audio_inputs
    }

    pub fn audio_outputs(&self) -> &[Arc<AudioIO>] {
        &self.audio_outputs
    }
}

impl Drop for Vst3Processor {
    fn drop(&mut self) {
        // Deactivate and terminate component
        if let Some(ref mut instance) = self.instance {
            let _ = instance.set_active(false);
            // IComponent::terminate
        }
    }
}
```

### 1.7 ProcessData Bridge
**File**: `engine/src/vst3/processor.rs` (continued)

Helper to bridge AudioIO ↔ VST3 ProcessData:
```rust
impl Vst3Processor {
    fn prepare_process_data(&mut self, frames: usize) -> ProcessData {
        // Allocate ProcessData on stack (real-time safe)
        let mut data = ProcessData::default();
        data.numSamples = frames as i32;

        // Input buses
        let mut input_bus_buffers: Vec<AudioBusBuffers> = Vec::new();
        for (bus_idx, bus) in self.input_buses.iter().enumerate() {
            if !bus.is_active { continue; }

            let mut channel_ptrs: Vec<*mut f32> = Vec::new();
            for ch_idx in 0..bus.channel_count {
                let audio_io = &self.audio_inputs[bus_idx * 2 + ch_idx];
                let buf = audio_io.buffer.lock();
                channel_ptrs.push(buf.as_mut_ptr());
            }

            input_bus_buffers.push(AudioBusBuffers {
                numChannels: bus.channel_count as i32,
                channelBuffers32: channel_ptrs.as_mut_ptr(),
                silenceFlags: 0,
            });
        }

        // Output buses (similar)
        // ...

        data.inputs = input_bus_buffers.as_mut_ptr();
        data.numInputs = input_bus_buffers.len() as i32;
        // ... outputs

        data
    }
}
```

## Phase 2: MIDI Support

### 2.1 MIDI Event Types
**File**: `engine/src/vst3/midi.rs`

Convert between Maolan's `MidiEvent` and VST3's `IEventList`:
```rust
use crate::midi::event::MidiEvent;
use vst3::Steinberg::Vst::Event;

pub struct EventBuffer {
    events: Vec<Event>,
}

impl EventBuffer {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }

    pub fn from_midi_events(midi_events: &[MidiEvent]) -> Self {
        let mut buf = Self::new();
        for event in midi_events {
            // Convert MidiEvent to VST3 Event
            // Event::type_ = kNoteOnEvent / kNoteOffEvent / kDataEvent
            // Event::busIndex = 0
            // Event::sampleOffset = event.timestamp
            // Event::ppqPosition = calculate from tempo
            // Event::noteOn/noteOff/data based on MIDI type
            buf.events.push(vst3_event);
        }
        buf
    }

    pub fn to_midi_events(&self) -> Vec<MidiEvent> {
        // Reverse conversion for output events
    }
}
```

### 2.2 Update Vst3Processor for MIDI
**File**: `engine/src/vst3/processor.rs`

Add MIDI support to processor:
```rust
impl Vst3Processor {
    pub fn process_with_midi(
        &mut self,
        frames: usize,
        input_events: &[MidiEvent],
    ) -> Vec<MidiEvent> {
        // 1. Process audio inputs
        for input in &self.audio_inputs {
            input.process();
        }

        // 2. Convert input MIDI to EventBuffer
        let event_buf = EventBuffer::from_midi_events(input_events);

        // 3. Create ProcessData with inputEvents
        let mut data = self.prepare_process_data(frames);
        data.inputEvents = event_buf.as_ievent_list();

        // 4. Create output event list
        let mut output_events = EventBuffer::new();
        data.outputEvents = output_events.as_ievent_list_mut();

        // 5. Process
        unsafe {
            self.instance.audio_processor.process(&mut data);
        }

        // 6. Convert output events to MidiEvent
        output_events.to_midi_events()
    }
}
```

### 2.3 Track Integration for MIDI
**File**: `engine/src/track.rs`

Update VST3 processing in `Track::process()` (currently lines 227-242):
```rust
// Current stub loop:
if !self.vst3_processors.is_empty() {
    for instance in &self.vst3_processors {
        let ready = instance.processor.audio_inputs()
            .iter().all(|audio_in| audio_in.ready());
        if ready {
            instance.processor.process_with_audio_io(frames);
        }
    }
}

// Replace with MIDI-aware version (similar to LV2 topological sort):
if !self.vst3_processors.is_empty() {
    // Collect MIDI for each VST3 instance from connections
    let mut vst3_node_events = HashMap::new();

    for (idx, instance) in self.vst3_processors.iter_mut().enumerate() {
        // Check if all inputs ready
        let ready = instance.processor.audio_inputs()
            .iter().all(|audio_in| audio_in.ready());

        if !ready { continue; }

        // Get MIDI routed to this plugin
        let input_midi = self.vst3_plugin_input_events(
            instance.id,
            &track_input_midi_events,
            &vst3_node_events,
        );

        // Process with MIDI
        let output_midi = instance.processor.process_with_midi(frames, &input_midi);

        // Store output MIDI for routing to next plugin
        vst3_node_events.insert(instance.id, output_midi);
    }
}
```

## Phase 3: Parameters and State

### 3.1 Parameter Discovery
**File**: `engine/src/vst3/processor.rs`

Add parameter enumeration during initialization:
```rust
#[derive(Clone, Debug)]
pub struct ParameterInfo {
    pub id: u32,           // VST3 ParamID
    pub title: String,
    pub short_title: String,
    pub units: String,
    pub step_count: i32,   // 0 = continuous, >0 = discrete
    pub default_value: f64,
    pub flags: i32,        // ParameterFlags (read-only, etc.)
}

impl Vst3Processor {
    fn discover_parameters(&mut self) -> Result<(), String> {
        let controller = self.instance.edit_controller.as_ref()
            .ok_or("No edit controller")?;

        let param_count = unsafe {
            controller.getParameterCount()
        };

        for i in 0..param_count {
            let mut info: ParameterInfo = unsafe {
                controller.getParameterInfo(i)
            };

            self.parameters.push(ParameterInfo {
                id: info.id,
                title: String::from_utf16(&info.title)?,
                // ... extract other fields
            });

            // Initialize scalar value
            let default_normalized = unsafe {
                controller.getParamNormalized(info.id)
            };
            self.scalar_values.lock().push(default_normalized as f32);
        }

        Ok(())
    }

    pub fn get_parameter_value(&self, param_id: u32) -> Option<f32> {
        let idx = self.parameters.iter()
            .position(|p| p.id == param_id)?;
        Some(self.scalar_values.lock()[idx])
    }

    pub fn set_parameter_value(&mut self, param_id: u32, normalized_value: f32) -> Result<(), String> {
        let idx = self.parameters.iter()
            .position(|p| p.id == param_id)
            .ok_or("Parameter not found")?;

        self.scalar_values.lock()[idx] = normalized_value;

        // Update controller
        unsafe {
            self.instance.edit_controller
                .setParamNormalized(param_id, normalized_value as f64);
        }

        Ok(())
    }
}
```

### 3.2 Parameter Automation in ProcessData
**File**: `engine/src/vst3/processor.rs`

Add parameter changes to process call:
```rust
impl Vst3Processor {
    fn prepare_process_data(&mut self, frames: usize) -> ProcessData {
        // ... existing audio buffer setup ...

        // Create input parameter changes
        let mut param_changes = InputParameterChanges::new();

        // For any changed parameters since last process:
        for (idx, param) in self.parameters.iter().enumerate() {
            let current = self.scalar_values.lock()[idx];
            let previous = self.previous_values.lock()[idx];

            if (current - previous).abs() > f32::EPSILON {
                param_changes.add_point(param.id, 0, current as f64);
                self.previous_values.lock()[idx] = current;
            }
        }

        data.inputParameterChanges = param_changes.as_iptr();

        // Output parameter changes (for read-back)
        let mut output_changes = OutputParameterChanges::new();
        data.outputParameterChanges = output_changes.as_iptr();

        data
    }
}
```

### 3.3 State Management
**File**: `engine/src/vst3/state.rs`

Implement state save/restore (similar to LV2):
```rust
use vst3::Steinberg::IBStream;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Vst3PluginState {
    pub plugin_id: String,
    pub component_state: Vec<u8>,
    pub controller_state: Vec<u8>,
}

impl Vst3Processor {
    pub fn snapshot_state(&self) -> Result<Vst3PluginState, String> {
        let component = self.instance.component.as_ref()
            .ok_or("No component")?;

        // Save component state
        let mut comp_stream = MemoryStream::new();
        unsafe {
            component.getState(&mut comp_stream)?;
        }

        // Save controller state
        let mut ctrl_stream = MemoryStream::new();
        if let Some(controller) = &self.instance.edit_controller {
            unsafe {
                controller.getState(&mut ctrl_stream)?;
            }
        }

        Ok(Vst3PluginState {
            plugin_id: self.plugin_id.clone(),
            component_state: comp_stream.into_bytes(),
            controller_state: ctrl_stream.into_bytes(),
        })
    }

    pub fn restore_state(&mut self, state: &Vst3PluginState) -> Result<(), String> {
        if state.plugin_id != self.plugin_id {
            return Err("Plugin ID mismatch".to_string());
        }

        // Restore component state
        let mut comp_stream = MemoryStream::from_bytes(&state.component_state);
        unsafe {
            self.instance.component.setState(&mut comp_stream)?;
        }

        // Restore controller state
        if !state.controller_state.is_empty() {
            let mut ctrl_stream = MemoryStream::from_bytes(&state.controller_state);
            unsafe {
                self.instance.edit_controller.setState(&mut ctrl_stream)?;
            }
        }

        // Re-sync parameter values
        for param in &self.parameters {
            let value = unsafe {
                self.instance.edit_controller.getParamNormalized(param.id)
            };
            let idx = self.parameters.iter().position(|p| p.id == param.id).unwrap();
            self.scalar_values.lock()[idx] = value as f32;
        }

        Ok(())
    }
}

struct MemoryStream {
    data: Vec<u8>,
    position: usize,
}

impl IBStream for MemoryStream {
    // Implement read/write/seek for VST3 state I/O
}
```

### 3.4 Track State Integration
**File**: `engine/src/track.rs`

Add methods for VST3 state (mirror LV2 pattern):
```rust
impl Track {
    pub fn vst3_snapshot_states(&self) -> Vec<(u64, Vst3PluginState)> {
        self.vst3_processors
            .iter()
            .filter_map(|instance| {
                instance.processor.snapshot_state()
                    .ok()
                    .map(|state| (instance.id, state))
            })
            .collect()
    }

    pub fn vst3_restore_states(&mut self, states: Vec<(u64, Vst3PluginState)>) -> Result<(), String> {
        for (id, state) in states {
            if let Some(instance) = self.vst3_processors.iter_mut()
                .find(|i| i.id == id) {
                instance.processor.restore_state(&state)?;
            }
        }
        Ok(())
    }
}
```

## Phase 4: Plugin GUI Integration

### 4.1 Platform Window Abstraction
**File**: `engine/src/vst3/gui.rs`

Create platform-specific parent window:
```rust
use raw_window_handle::{RawWindowHandle, HasRawWindowHandle};

pub struct PluginWindow {
    #[cfg(target_os = "windows")]
    hwnd: windows::Win32::Foundation::HWND,

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    x11_window: u64,  // X11 Window ID

    #[cfg(target_os = "macos")]
    nsview: *mut objc::runtime::Object,  // NSView*

    width: u32,
    height: u32,
}

impl PluginWindow {
    pub fn new() -> Result<Self, String> {
        #[cfg(target_os = "windows")]
        {
            // CreateWindowEx with WS_CHILD style
        }

        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        {
            // Create X11 window or use existing from iced
        }

        #[cfg(target_os = "macos")]
        {
            // Create NSView
        }
    }

    pub fn raw_handle(&self) -> RawWindowHandle {
        #[cfg(target_os = "windows")]
        return RawWindowHandle::Win32(Win32WindowHandle {
            hwnd: self.hwnd.0 as *mut _,
            ..Default::default()
        });

        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        return RawWindowHandle::Xlib(XlibWindowHandle {
            window: self.x11_window,
            ..Default::default()
        });

        // ... macOS
    }
}
```

### 4.2 IPlugView Integration
**File**: `engine/src/vst3/gui.rs` (continued)

Wrap VST3's IPlugView:
```rust
pub struct PluginEditor {
    view: ComPtr<IPlugView>,
    window: PluginWindow,
    is_attached: bool,
}

impl PluginEditor {
    pub fn new(processor: &mut Vst3Processor) -> Result<Self, String> {
        let controller = processor.instance.edit_controller.as_ref()
            .ok_or("No edit controller")?;

        // Create view
        let view = unsafe {
            controller.createView(ViewType::kEditor)?
        };

        // Query preferred size
        let mut rect = ViewRect::default();
        unsafe {
            view.getSize(&mut rect)?;
        }

        let window = PluginWindow::new()?;
        window.resize(rect.right - rect.left, rect.bottom - rect.top)?;

        Ok(Self {
            view,
            window,
            is_attached: false,
        })
    }

    pub fn attach(&mut self) -> Result<(), String> {
        if self.is_attached {
            return Ok(());
        }

        let handle = self.window.raw_handle();

        #[cfg(target_os = "windows")]
        let parent = handle.hwnd;

        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        let parent = handle.window as *mut _;

        unsafe {
            self.view.attached(parent, ViewType::kEditor)?;
        }

        self.is_attached = true;
        Ok(())
    }

    pub fn detach(&mut self) -> Result<(), String> {
        if !self.is_attached {
            return Ok(());
        }

        unsafe {
            self.view.removed()?;
        }

        self.is_attached = false;
        Ok(())
    }
}

impl Drop for PluginEditor {
    fn drop(&mut self) {
        let _ = self.detach();
    }
}
```

### 4.3 Vst3Processor GUI Methods
**File**: `engine/src/vst3/processor.rs`

Add editor creation:
```rust
impl Vst3Processor {
    pub fn has_editor(&self) -> bool {
        self.instance.edit_controller
            .as_ref()
            .map(|c| unsafe { c.createView(ViewType::kEditor).is_ok() })
            .unwrap_or(false)
    }

    pub fn create_editor(&mut self) -> Result<PluginEditor, String> {
        PluginEditor::new(self)
    }
}
```

### 4.4 GUI Integration with Iced
**File**: `src/gui/mod.rs` or new `src/plugin_editor_widget.rs`

Create iced widget for embedding VST3 GUIs:
```rust
use iced::{widget, Element};

pub struct PluginEditorWidget {
    editor: Option<PluginEditor>,
    track_name: String,
    instance_id: u64,
}

impl PluginEditorWidget {
    pub fn new(track_name: String, instance_id: u64) -> Self {
        Self {
            editor: None,
            track_name,
            instance_id,
        }
    }

    pub fn view(&self) -> Element<Message> {
        if let Some(editor) = &self.editor {
            // Create native widget container
            widget::container(
                widget::text("VST3 Plugin Editor")
            )
            .width(editor.width())
            .height(editor.height())
            .into()
        } else {
            widget::text("Loading editor...").into()
        }
    }
}

// Message handling in Maolan::update
impl Maolan {
    fn handle_open_vst3_editor(&mut self, track_name: String, instance_id: u64) -> Task<Message> {
        // Send Action::TrackGetVst3Editor to engine
        // On response, create PluginEditorWidget and attach
        self.send(Action::TrackGetVst3Editor { track_name, instance_id })
    }
}
```

## Phase 5: Engine Messages and UI Integration

### 5.1 New Engine Actions
**File**: `engine/src/message.rs`

Add VST3-specific actions (mirror LV2 actions at lines 155+):
```rust
pub enum Action {
    // ... existing actions ...

    // VST3 plugin management
    TrackLoadVst3Plugin {
        track_name: String,
        plugin_path: String,
    },
    TrackUnloadVst3PluginInstance {
        track_name: String,
        instance_id: u64,
    },
    TrackGetVst3Graph {
        track_name: String,
    },
    TrackVst3Graph {
        track_name: String,
        plugins: Vec<Vst3GraphPlugin>,
        connections: Vec<Vst3GraphConnection>,
    },

    // VST3 parameters
    TrackSetVst3Parameter {
        track_name: String,
        instance_id: u64,
        param_id: u32,
        value: f32,
    },
    TrackGetVst3Parameters {
        track_name: String,
        instance_id: u64,
    },
    TrackVst3Parameters {
        track_name: String,
        instance_id: u64,
        parameters: Vec<(u32, String, f32)>,  // (id, name, value)
    },

    // VST3 state
    TrackVst3SnapshotState {
        track_name: String,
        instance_id: u64,
    },
    TrackVst3RestoreState {
        track_name: String,
        instance_id: u64,
        state: Vst3PluginState,
    },

    // VST3 GUI
    TrackOpenVst3Editor {
        track_name: String,
        instance_id: u64,
    },
    TrackCloseVst3Editor {
        track_name: String,
        instance_id: u64,
    },
}
```

### 5.2 Engine Message Handlers
**File**: `engine/src/engine.rs`

Add handlers (similar to LV2 at lines 1450-1545):
```rust
impl Engine {
    async fn handle_action(&mut self, action: Action) {
        match action {
            Action::TrackLoadVst3Plugin { track_name, plugin_path } => {
                if let Some(track) = self.state.lock().tracks.get(&track_name) {
                    let result = track.lock().load_vst3_plugin(&plugin_path);
                    self.notify_clients(result.map(|_| action)).await;
                }
            }

            Action::TrackUnloadVst3PluginInstance { track_name, instance_id } => {
                if let Some(track) = self.state.lock().tracks.get(&track_name) {
                    let result = track.lock().unload_vst3_plugin_instance(instance_id);
                    self.notify_clients(result.map(|_| action)).await;
                }
            }

            Action::TrackGetVst3Graph { track_name } => {
                if let Some(track) = self.state.lock().tracks.get(&track_name) {
                    let t = track.lock();
                    let plugins = t.vst3_graph_plugins();
                    let connections = t.vst3_graph_connections();
                    self.notify_clients(Ok(Action::TrackVst3Graph {
                        track_name,
                        plugins,
                        connections,
                    })).await;
                }
            }

            Action::TrackSetVst3Parameter { track_name, instance_id, param_id, value } => {
                if let Some(track) = self.state.lock().tracks.get(&track_name) {
                    let result = track.lock().set_vst3_parameter(instance_id, param_id, value);
                    self.notify_clients(result.map(|_| action)).await;
                }
            }

            // ... other VST3 handlers
        }
    }
}
```

### 5.3 Track VST3 Methods
**File**: `engine/src/track.rs`

Implement track-level VST3 management (mirror LV2 at lines 641-656):
```rust
impl Track {
    pub fn load_vst3_plugin(&mut self, plugin_path: &str) -> Result<(), String> {
        let processor = Vst3Processor::new(
            self.sample_rate,
            self.buffer_size,
            plugin_path,
        )?;

        let id = self.next_vst3_instance_id;
        self.next_vst3_instance_id = self.next_vst3_instance_id.saturating_add(1);

        self.vst3_processors.push(Vst3Instance {
            id,
            processor,
        });

        // Auto-wire into graph (existing method)
        self.rewire_vst3_default_audio_graph();

        Ok(())
    }

    pub fn unload_vst3_plugin_instance(&mut self, instance_id: u64) -> Result<(), String> {
        let idx = self.vst3_processors.iter()
            .position(|i| i.id == instance_id)
            .ok_or("Instance not found")?;

        self.vst3_processors.remove(idx);
        self.rewire_vst3_default_audio_graph();

        Ok(())
    }

    pub fn set_vst3_parameter(
        &mut self,
        instance_id: u64,
        param_id: u32,
        value: f32,
    ) -> Result<(), String> {
        let instance = self.vst3_processors.iter_mut()
            .find(|i| i.id == instance_id)
            .ok_or("Instance not found")?;

        instance.processor.set_parameter_value(param_id, value)
    }

    pub fn vst3_graph_plugins(&self) -> Vec<Vst3GraphPlugin> {
        self.vst3_processors.iter().map(|i| {
            Vst3GraphPlugin {
                id: i.id,
                name: i.processor.name().to_string(),
                parameters: i.processor.parameters().clone(),
            }
        }).collect()
    }

    pub fn vst3_graph_connections(&self) -> Vec<Vst3GraphConnection> {
        // Similar to lv2_graph_connections (track.rs:1332)
        // Build list of audio/MIDI connections between:
        // - Track inputs → Plugin inputs
        // - Plugin outputs → Plugin inputs
        // - Plugin outputs → Track outputs
    }
}
```

### 5.4 GUI Update Handlers
**File**: `src/gui/update.rs`

Add VST3 message handlers (mirror LV2 at lines 2277+):
```rust
impl Maolan {
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::LoadVst3Plugin { track_name, plugin_path } => {
                self.send(Action::TrackLoadVst3Plugin {
                    track_name,
                    plugin_path,
                })
            }

            Message::UnloadVst3Plugin { track_name, instance_id } => {
                self.send(Action::TrackUnloadVst3PluginInstance {
                    track_name,
                    instance_id,
                })
            }

            Message::Vst3ParameterChanged { track_name, instance_id, param_id, value } => {
                self.send(Action::TrackSetVst3Parameter {
                    track_name,
                    instance_id,
                    param_id,
                    value,
                })
            }

            Message::OpenVst3Editor { track_name, instance_id } => {
                // Create PluginEditorWidget
                // Attach to iced window
                self.send(Action::TrackOpenVst3Editor {
                    track_name,
                    instance_id,
                })
            }

            // ... other VST3 messages
        }
    }
}
```

## Implementation Order

### Phase 1: Core Audio (Weeks 1-2)
1. Add `vst3` dependency to `engine/Cargo.toml`
2. Create module structure: `engine/src/vst3/{mod,host,interfaces,processor,port}.rs`
3. Implement `PluginFactory` and `PluginInstance` wrappers (COM safety)
4. Implement `Vst3Host::list_plugins()` for discovery
5. Implement `Vst3Processor::new()` with full initialization
6. Implement `process_with_audio_io()` with real VST3 processing
7. Test with simple audio plugins (gain, EQ)

### Phase 2: MIDI (Week 3)
1. Create `engine/src/vst3/midi.rs` with `EventBuffer`
2. Implement MIDI event conversion (MidiEvent ↔ VST3 Event)
3. Update `Vst3Processor::process_with_midi()`
4. Update `Track::process()` for VST3 MIDI routing
5. Test with synthesizer and MIDI FX plugins

### Phase 3: Parameters & State (Week 4)
1. Implement parameter discovery in `Vst3Processor`
2. Add parameter get/set methods
3. Implement `InputParameterChanges` for automation
4. Create `engine/src/vst3/state.rs` with `MemoryStream`
5. Implement `snapshot_state()` and `restore_state()`
6. Add track-level state methods
7. Test state save/restore in sessions

### Phase 4: GUI (Week 5)
1. Add platform dependencies to `engine/Cargo.toml`
2. Create `engine/src/vst3/gui.rs`
3. Implement `PluginWindow` for each platform
4. Implement `PluginEditor` with IPlugView
5. Create iced widget for embedding editors
6. Test on each platform (Linux/FreeBSD X11, Windows, macOS)

### Phase 5: Integration (Week 6)
1. Add all `Action` variants to `engine/src/message.rs`
2. Implement engine handlers in `engine/src/engine.rs`
3. Add track methods in `engine/src/track.rs`
4. Update GUI message handlers in `src/gui/update.rs`
5. Add VST3 plugin browser to UI
6. Add VST3 graph view (connections canvas)
7. Integration testing with multiple plugins

## Critical Files to Modify

### Engine Files
1. **`engine/Cargo.toml`** - Add `vst3` and platform dependencies
2. **`engine/src/lib.rs`** - Export vst3 module
3. **`engine/src/vst3.rs`** - DELETE and replace with `vst3/` directory
4. **`engine/src/vst3/mod.rs`** - NEW: Module root, re-exports
5. **`engine/src/vst3/host.rs`** - NEW: Plugin discovery
6. **`engine/src/vst3/interfaces.rs`** - NEW: Safe COM wrappers
7. **`engine/src/vst3/processor.rs`** - NEW: Plugin instance management
8. **`engine/src/vst3/port.rs`** - NEW: Port binding types
9. **`engine/src/vst3/midi.rs`** - NEW: MIDI event conversion
10. **`engine/src/vst3/state.rs`** - NEW: State save/restore
11. **`engine/src/vst3/gui.rs`** - NEW: Plugin GUI embedding
12. **`engine/src/track.rs`** - Add VST3 methods, update `process()`
13. **`engine/src/message.rs`** - Add VST3 actions
14. **`engine/src/engine.rs`** - Add VST3 action handlers

### GUI Files
15. **`src/gui/update.rs`** - Add VST3 message handlers
16. **`src/gui/mod.rs`** - Update for VST3 plugin browser
17. **`src/state/mod.rs`** - Add VST3 state serialization
18. **`src/plugin_editor_widget.rs`** - NEW: iced widget for plugin GUIs

### Session Format
19. **Session JSON** - Add VST3 processor states, connections

## Testing Strategy

### Unit Tests
- COM interface wrapper safety
- MIDI event conversion accuracy
- Parameter value mapping
- State serialization roundtrip

### Integration Tests
1. **Load plugin** - Verify initialization succeeds
2. **Process audio** - Compare output to expected (sine → gain → verify level)
3. **MIDI processing** - Send note events, verify synthesis output
4. **Parameter automation** - Change value, verify audio change
5. **State persistence** - Save, unload, reload, verify identical state
6. **Multi-plugin chains** - Verify serial routing works
7. **GUI attachment** - Verify editor window appears and responds

### Platform Testing
- **Linux**: Test with common VST3 plugins (Surge XT, TAL-NoiseMaker)
- **FreeBSD**: Verify same plugins work (crucial for this user)
- **Windows**: Test with commercial plugins
- **macOS**: Test with AU-converted plugins

## Risk Mitigation

### High Risk Areas
1. **COM interface safety** - Extensive unsafe code in interfaces.rs
   - Mitigation: Thorough wrapper layer, document all unsafe blocks

2. **Real-time safety** - Memory allocation in audio thread
   - Mitigation: Pre-allocate all buffers, use stack for ProcessData

3. **Platform differences** - GUI embedding varies wildly
   - Mitigation: Conditional compilation, separate implementations

4. **Plugin crashes** - VST3 plugins can segfault
   - Mitigation: Consider plugin sandboxing (future work)

### Medium Risk Areas
1. **MIDI timing** - Sample-accurate event placement
   - Mitigation: Test with MIDI monitor plugins

2. **Parameter threading** - UI thread vs audio thread updates
   - Mitigation: Use atomic values or locks as needed

3. **State compatibility** - VST3 version changes
   - Mitigation: Store plugin version in state, handle errors gracefully

## Success Criteria

Phase 1 complete when:
- ✓ VST3 plugins load without errors
- ✓ Audio processes through plugin chain
- ✓ No crashes with 10+ plugins
- ✓ Latency equivalent to LV2 path

Phase 2 complete when:
- ✓ MIDI events route to VST3 instruments
- ✓ Synthesizers produce correct notes
- ✓ MIDI FX transform events correctly

Phase 3 complete when:
- ✓ All parameters visible in UI
- ✓ Parameter changes affect audio
- ✓ Session save/restore preserves plugin state
- ✓ Presets load correctly

Phase 4 complete when:
- ✓ Plugin GUIs appear in Maolan window
- ✓ GUI controls functional on all platforms
- ✓ GUI updates reflect parameter changes
- ✓ No GUI crashes or freezes

Phase 5 complete when:
- ✓ VST3 matches LV2 feature parity
- ✓ User can create complex VST3 graphs
- ✓ VST3 and LV2 plugins can coexist
- ✓ Documentation complete

## Future Enhancements (Post-Implementation)

1. **Preset management** - VST3 preset browser
2. **MIDI CC mapping** - Map hardware controllers to parameters
3. **Plugin sandboxing** - Run plugins in separate processes
4. **Performance profiling** - Per-plugin CPU usage display
5. **Plugin delay compensation** - Automatic latency alignment
6. **VST3 Note Expression** - Support per-note modulation
7. **Multi-channel routing** - Beyond stereo (5.1, etc.)
8. **Plugin blacklist** - Skip known-crashy plugins

---

**End of Implementation Plan**
