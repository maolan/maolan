use super::interfaces::PluginInstance;
use super::midi::EventBuffer;
use super::port::{BusInfo, ParameterInfo};
use super::state::{MemoryStream, Vst3PluginState};
use crate::audio::io::AudioIO;
use crate::midi::io::MidiEvent;
use std::path::Path;
use std::sync::{Arc, Mutex};
use vst3::Steinberg::Vst::ProcessModes_::kRealtime;
use vst3::Steinberg::Vst::SymbolicSampleSizes_::kSample32;

#[derive(Debug)]
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
    previous_values: Arc<Mutex<Vec<f32>>>,

    // Processing state
    _sample_rate: f64,
    _max_block_size: usize,
    _is_active: bool,
}

impl Vst3Processor {
    /// Create a new VST3 processor (simplified constructor for backward compatibility)
    pub fn new(
        sample_frames: usize,
        path: &str,
        audio_inputs: usize,
        audio_outputs: usize,
    ) -> Self {
        // Use default sample rate
        Self::new_with_sample_rate(44100.0, sample_frames, path, audio_inputs, audio_outputs)
            .unwrap_or_else(|_| {
                // Fallback to stub implementation if loading fails
                Self::new_stub(sample_frames, path, audio_inputs, audio_outputs)
            })
    }

    /// Create a new VST3 processor with explicit sample rate
    pub fn new_with_sample_rate(
        sample_rate: f64,
        buffer_size: usize,
        plugin_path: &str,
        audio_inputs: usize,
        audio_outputs: usize,
    ) -> Result<Self, String> {
        let path_buf = Path::new(plugin_path);

        // Extract name from path
        let name = path_buf
            .file_stem()
            .or_else(|| path_buf.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown VST3")
            .to_string();

        // Load plugin factory and create instance
        let factory = super::interfaces::PluginFactory::from_module(path_buf)?;

        let class_count = factory.count_classes();
        if class_count == 0 {
            return Err("No plugin classes found".to_string());
        }

        // Get first class and create instance
        let class_info = factory
            .get_class_info(0)
            .ok_or("Failed to get class info")?;

        let mut instance = factory.create_instance(&class_info.cid)?;

        // Initialize the plugin
        instance.initialize()?;

        // Setup processing
        instance.setup_processing(sample_rate, buffer_size as i32)?;

        // Query buses (for now, use the provided counts)
        let input_buses = vec![BusInfo {
            index: 0,
            name: "Input".to_string(),
            channel_count: audio_inputs.max(1),
            is_active: true,
        }];

        let output_buses = vec![BusInfo {
            index: 0,
            name: "Output".to_string(),
            channel_count: audio_outputs.max(1),
            is_active: true,
        }];

        // Create AudioIO for each channel
        let mut audio_input_ios = Vec::new();
        for _ in 0..audio_inputs.max(1) {
            audio_input_ios.push(Arc::new(AudioIO::new(buffer_size)));
        }

        let mut audio_output_ios = Vec::new();
        for _ in 0..audio_outputs.max(1) {
            audio_output_ios.push(Arc::new(AudioIO::new(buffer_size)));
        }

        // Discover parameters
        let (parameters, scalar_values) = discover_parameters(&instance)?;
        let previous_values = Arc::new(Mutex::new(scalar_values.lock().unwrap().clone()));

        // Activate the component
        instance.set_active(true)?;

        let plugin_id = format!("{:02X?}", class_info.cid);

        Ok(Self {
            path: plugin_path.to_string(),
            name,
            plugin_id,
            instance: Some(instance),
            audio_inputs: audio_input_ios,
            audio_outputs: audio_output_ios,
            input_buses,
            output_buses,
            parameters,
            scalar_values,
            previous_values,
            _sample_rate: sample_rate,
            _max_block_size: buffer_size,
            _is_active: true,
        })
    }

    /// Create a stub processor (fallback when real loading fails)
    fn new_stub(
        sample_frames: usize,
        path: &str,
        audio_inputs: usize,
        audio_outputs: usize,
    ) -> Self {
        let in_count = audio_inputs.max(1);
        let out_count = audio_outputs.max(1);
        let name = Path::new(path)
            .file_stem()
            .or_else(|| Path::new(path).file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown VST3")
            .to_string();

        Self {
            path: path.to_string(),
            name,
            plugin_id: String::new(),
            instance: None,
            audio_inputs: (0..in_count)
                .map(|_| Arc::new(AudioIO::new(sample_frames)))
                .collect(),
            audio_outputs: (0..out_count)
                .map(|_| Arc::new(AudioIO::new(sample_frames)))
                .collect(),
            input_buses: vec![],
            output_buses: vec![],
            parameters: vec![],
            scalar_values: Arc::new(Mutex::new(vec![])),
            previous_values: Arc::new(Mutex::new(vec![])),
            _sample_rate: 44100.0,
            _max_block_size: sample_frames,
            _is_active: false,
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn audio_inputs(&self) -> &[Arc<AudioIO>] {
        &self.audio_inputs
    }

    pub fn audio_outputs(&self) -> &[Arc<AudioIO>] {
        &self.audio_outputs
    }

    pub fn setup_audio_ports(&self) {
        for port in &self.audio_inputs {
            port.setup();
        }
        for port in &self.audio_outputs {
            port.setup();
        }
    }

    pub fn process_with_audio_io(&self, frames: usize) {
        // Process all input AudioIO ports
        for input in &self.audio_inputs {
            input.process();
        }

        // If we don't have a real instance, do passthrough
        if self.instance.is_none() {
            self.process_passthrough(frames);
            return;
        }

        // Get the audio processor
        let processor = match &self.instance.as_ref().unwrap().audio_processor {
            Some(proc) => proc,
            None => {
                // No processor available, use passthrough
                self.process_passthrough(frames);
                return;
            }
        };

        // Call real VST3 processing (no MIDI)
        if let Err(e) = self.process_vst3(processor, frames, None) {
            // If VST3 processing fails, fallback to passthrough
            eprintln!("VST3 processing error: {}, falling back to passthrough", e);
            self.process_passthrough(frames);
        }
    }

    /// Process audio with MIDI events
    pub fn process_with_midi(
        &self,
        frames: usize,
        input_events: &[MidiEvent],
    ) -> Vec<MidiEvent> {
        // Process all input AudioIO ports
        for input in &self.audio_inputs {
            input.process();
        }

        // If we don't have a real instance, do passthrough with no MIDI output
        if self.instance.is_none() {
            self.process_passthrough(frames);
            return Vec::new();
        }

        // Get the audio processor
        let processor = match &self.instance.as_ref().unwrap().audio_processor {
            Some(proc) => proc,
            None => {
                // No processor available, use passthrough
                self.process_passthrough(frames);
                return Vec::new();
            }
        };

        // Convert input MIDI events to VST3 format
        let input_event_buffer = EventBuffer::from_midi_events(input_events, 0);

        // Call real VST3 processing with MIDI
        match self.process_vst3(processor, frames, Some(&input_event_buffer)) {
            Ok(output_buffer) => {
                // Convert output events back to MIDI
                output_buffer.to_midi_events()
            }
            Err(e) => {
                // If VST3 processing fails, fallback to passthrough
                eprintln!("VST3 processing error: {}, falling back to passthrough", e);
                self.process_passthrough(frames);
                Vec::new()
            }
        }
    }

    fn process_vst3(
        &self,
        processor: &vst3::ComPtr<vst3::Steinberg::Vst::IAudioProcessor>,
        frames: usize,
        _input_events: Option<&EventBuffer>,
    ) -> Result<EventBuffer, String> {
        use vst3::Steinberg::Vst::*;
        use vst3::Steinberg::Vst::IAudioProcessorTrait;

        // Prepare input bus buffers
        let mut input_channel_ptrs: Vec<*mut f32> = Vec::new();
        for input_io in &self.audio_inputs {
            let buf = input_io.buffer.lock();
            input_channel_ptrs.push(buf.as_ptr() as *mut f32);
        }

        // Prepare output bus buffers
        let mut output_channel_ptrs: Vec<*mut f32> = Vec::new();
        for output_io in &self.audio_outputs {
            let buf = output_io.buffer.lock();
            output_channel_ptrs.push(buf.as_ptr() as *mut f32);
        }

        // Create audio bus buffers using unsafe initialization
        // AudioBusBuffers has opaque bindgen fields, so we set them via pointer manipulation
        let mut input_buses = Vec::new();
        if !self.input_buses.is_empty() && !input_channel_ptrs.is_empty() {
            let mut bus: AudioBusBuffers = unsafe { std::mem::zeroed() };
            unsafe {
                let bus_ptr = &mut bus as *mut AudioBusBuffers as *mut u8;
                // numChannels is first field (i32)
                *(bus_ptr as *mut i32) = self.input_buses[0].channel_count as i32;
                // silenceFlags is second field (u64) at offset 8
                *(bus_ptr.add(8) as *mut u64) = 0;
                // channelBuffers32 is in union at offset 16
                *(bus_ptr.add(16) as *mut *mut *mut f32) = input_channel_ptrs.as_mut_ptr();
            }
            input_buses.push(bus);
        }

        let mut output_buses = Vec::new();
        if !self.output_buses.is_empty() && !output_channel_ptrs.is_empty() {
            let mut bus: AudioBusBuffers = unsafe { std::mem::zeroed() };
            unsafe {
                let bus_ptr = &mut bus as *mut AudioBusBuffers as *mut u8;
                *(bus_ptr as *mut i32) = self.output_buses[0].channel_count as i32;
                *(bus_ptr.add(8) as *mut u64) = 0;
                *(bus_ptr.add(16) as *mut *mut *mut f32) = output_channel_ptrs.as_mut_ptr();
            }
            output_buses.push(bus);
        }

        // Create ProcessData
        let mut process_data = ProcessData {
            processMode: kRealtime as i32,
            symbolicSampleSize: kSample32 as i32,
            numSamples: frames as i32,
            numInputs: input_buses.len() as i32,
            numOutputs: output_buses.len() as i32,
            inputs: if input_buses.is_empty() {
                std::ptr::null_mut()
            } else {
                input_buses.as_mut_ptr()
            },
            outputs: if output_buses.is_empty() {
                std::ptr::null_mut()
            } else {
                output_buses.as_mut_ptr()
            },
            inputParameterChanges: std::ptr::null_mut(),
            outputParameterChanges: std::ptr::null_mut(),
            inputEvents: std::ptr::null_mut(),
            outputEvents: std::ptr::null_mut(),
            processContext: std::ptr::null_mut(),
        };

        // Call VST3 process
        let result = unsafe { processor.process(&mut process_data) };

        if result != vst3::Steinberg::kResultOk {
            return Err(format!("VST3 process failed with result: {}", result));
        }

        // Mark outputs as finished
        for output in &self.audio_outputs {
            *output.finished.lock() = true;
        }

        // For now, return empty output events
        // Full IEventList implementation would go here
        let output_events = EventBuffer::new();

        Ok(output_events)
    }

    fn process_passthrough(&self, frames: usize) {
        for (out_idx, output) in self.audio_outputs.iter().enumerate() {
            let out_buf = output.buffer.lock();
            out_buf.fill(0.0);

            if self.audio_inputs.is_empty() {
                *output.finished.lock() = true;
                continue;
            }

            let input = &self.audio_inputs[out_idx % self.audio_inputs.len()];
            let in_buf = input.buffer.lock();
            for (o, i) in out_buf.iter_mut().zip(in_buf.iter()).take(frames) {
                *o = *i;
            }
            *output.finished.lock() = true;
        }
    }

    pub fn parameters(&self) -> &[ParameterInfo] {
        &self.parameters
    }

    pub fn get_parameter_value(&self, param_id: u32) -> Option<f32> {
        let idx = self.parameters.iter().position(|p| p.id == param_id)?;
        Some(self.scalar_values.lock().unwrap()[idx])
    }

    pub fn set_parameter_value(&mut self, param_id: u32, normalized_value: f32) -> Result<(), String> {
        let idx = self
            .parameters
            .iter()
            .position(|p| p.id == param_id)
            .ok_or("Parameter not found")?;

        self.scalar_values.lock().unwrap()[idx] = normalized_value;

        // Update controller if available
        if let Some(instance) = &self.instance {
            if let Some(controller) = &instance.edit_controller {
                use vst3::Steinberg::Vst::IEditControllerTrait;
                unsafe {
                    controller.setParamNormalized(param_id, normalized_value as f64);
                }
            }
        }

        Ok(())
    }

    /// Snapshot the current plugin state for saving
    pub fn snapshot_state(&self) -> Result<Vst3PluginState, String> {
        use vst3::Steinberg::Vst::{IComponentTrait, IEditControllerTrait};

        let instance = self.instance.as_ref().ok_or("No plugin instance")?;

        // Save component state
        let mut comp_stream = MemoryStream::new();
        unsafe {
            let result = instance.component.getState(comp_stream.as_ibstream_mut() as *mut _ as *mut _);
            if result != vst3::Steinberg::kResultOk {
                return Err("Failed to get component state".to_string());
            }
        }

        // Save controller state (if available)
        let mut ctrl_stream = MemoryStream::new();
        if let Some(controller) = &instance.edit_controller {
            unsafe {
                let result = controller.getState(ctrl_stream.as_ibstream_mut() as *mut _ as *mut _);
                if result != vst3::Steinberg::kResultOk {
                    // Controller state is optional, so just log warning
                    eprintln!("Warning: Failed to get controller state");
                }
            }
        }

        Ok(Vst3PluginState {
            plugin_id: self.plugin_id.clone(),
            component_state: comp_stream.into_bytes(),
            controller_state: ctrl_stream.into_bytes(),
        })
    }

    /// Restore plugin state from a snapshot
    pub fn restore_state(&mut self, state: &Vst3PluginState) -> Result<(), String> {
        use vst3::Steinberg::Vst::{IComponentTrait, IEditControllerTrait};

        if state.plugin_id != self.plugin_id {
            return Err(format!(
                "Plugin ID mismatch: expected '{}', got '{}'",
                self.plugin_id, state.plugin_id
            ));
        }

        let instance = self.instance.as_ref().ok_or("No plugin instance")?;

        // Restore component state
        if !state.component_state.is_empty() {
            let mut comp_stream = MemoryStream::from_bytes(&state.component_state);
            unsafe {
                let result = instance.component.setState(comp_stream.as_ibstream_mut() as *mut _ as *mut _);
                if result != vst3::Steinberg::kResultOk {
                    return Err("Failed to set component state".to_string());
                }
            }
        }

        // Restore controller state (if available)
        if !state.controller_state.is_empty() {
            if let Some(controller) = &instance.edit_controller {
                let mut ctrl_stream = MemoryStream::from_bytes(&state.controller_state);
                unsafe {
                    let result = controller.setState(ctrl_stream.as_ibstream_mut() as *mut _ as *mut _);
                    if result != vst3::Steinberg::kResultOk {
                        eprintln!("Warning: Failed to set controller state");
                    }
                }

                // Re-sync parameter values after restoring state
                for (idx, param) in self.parameters.iter().enumerate() {
                    let value = unsafe { controller.getParamNormalized(param.id) };
                    self.scalar_values.lock().unwrap()[idx] = value as f32;
                    self.previous_values.lock().unwrap()[idx] = value as f32;
                }
            }
        }

        Ok(())
    }
}

impl Drop for Vst3Processor {
    fn drop(&mut self) {
        if let Some(ref mut instance) = self.instance {
            let _ = instance.set_active(false);
            let _ = instance.terminate();
        }
    }
}

fn discover_parameters(instance: &PluginInstance) -> Result<(Vec<ParameterInfo>, Arc<Mutex<Vec<f32>>>), String> {
    use vst3::Steinberg::Vst::IEditControllerTrait;

    let controller = instance
        .edit_controller
        .as_ref()
        .ok_or("No edit controller available")?;

    let param_count = unsafe { controller.getParameterCount() };

    let mut parameters = Vec::new();
    let mut values = Vec::new();

    for i in 0..param_count {
        let mut info = vst3::Steinberg::Vst::ParameterInfo {
            id: 0,
            title: [0; 128],
            shortTitle: [0; 128],
            units: [0; 128],
            stepCount: 0,
            defaultNormalizedValue: 0.0,
            unitId: 0,
            flags: 0,
        };

        let result = unsafe { controller.getParameterInfo(i, &mut info) };

        if result != vst3::Steinberg::kResultOk {
            continue;
        }

        // Extract strings from VST3 TChar arrays (UTF-16)
        let title = extract_tchar_string(&info.title);
        let short_title = extract_tchar_string(&info.shortTitle);
        let units = extract_tchar_string(&info.units);

        parameters.push(ParameterInfo {
            id: info.id,
            title,
            short_title,
            units,
            step_count: info.stepCount,
            default_value: info.defaultNormalizedValue,
            flags: info.flags,
        });

        // Get current normalized value
        let value = unsafe { controller.getParamNormalized(info.id) };
        values.push(value as f32);
    }

    Ok((parameters, Arc::new(Mutex::new(values))))
}

fn extract_tchar_string(tchar: &[u16]) -> String {
    // Find null terminator
    let len = tchar.iter().position(|&c| c == 0).unwrap_or(tchar.len());
    String::from_utf16_lossy(&tchar[..len])
}

// Standalone function for listing plugins (backward compatibility)
pub fn list_plugins() -> Vec<super::host::Vst3PluginInfo> {
    super::host::list_plugins()
}
