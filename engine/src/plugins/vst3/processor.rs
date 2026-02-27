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
    instance: PluginInstance,
    // Keep factory/module alive for the plugin instance lifetime.
    _factory: super::interfaces::PluginFactory,

    // Audio I/O (reuse existing AudioIO)
    audio_inputs: Vec<Arc<AudioIO>>,
    audio_outputs: Vec<Arc<AudioIO>>,
    midi_input_ports: usize,
    midi_output_ports: usize,
    input_buses: Vec<BusInfo>,
    output_buses: Vec<BusInfo>,

    // Parameters
    parameters: Vec<ParameterInfo>,
    scalar_values: Arc<Mutex<Vec<f32>>>,
    previous_values: Arc<Mutex<Vec<f32>>>,
}

impl Vst3Processor {
    /// Create a new VST3 processor (simplified constructor for backward compatibility)
    pub fn new(
        sample_frames: usize,
        path: &str,
        audio_inputs: usize,
        audio_outputs: usize,
    ) -> Result<Self, String> {
        // Use default sample rate
        Self::new_with_sample_rate(44100.0, sample_frames, path, audio_inputs, audio_outputs)
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
        instance.initialize(&factory)?;

        let (plugin_input_buses, plugin_output_buses) = instance.audio_bus_counts();
        let (midi_input_ports, midi_output_ports) = instance.event_bus_counts();

        // Query buses (for now, use the provided counts)
        let input_buses = if plugin_input_buses > 0 {
            vec![BusInfo {
                index: 0,
                name: "Input".to_string(),
                channel_count: audio_inputs.max(1),
                is_active: true,
            }]
        } else {
            vec![]
        };

        let output_buses = if plugin_output_buses > 0 {
            vec![BusInfo {
                index: 0,
                name: "Output".to_string(),
                channel_count: audio_outputs.max(1),
                is_active: true,
            }]
        } else {
            vec![]
        };

        // Create AudioIO for each channel
        let mut audio_input_ios = Vec::new();
        for _ in 0..audio_inputs.max(1) {
            audio_input_ios.push(Arc::new(AudioIO::new(buffer_size)));
        }

        let mut audio_output_ios = Vec::new();
        for _ in 0..audio_outputs.max(1) {
            audio_output_ios.push(Arc::new(AudioIO::new(buffer_size)));
        }

        // Activate component before entering processing state.
        instance.set_active(true)?;

        // Setup processing after activation (VST3 lifecycle requirement).
        instance.setup_processing(
            sample_rate,
            buffer_size as i32,
            if plugin_input_buses > 0 {
                audio_inputs.max(1) as i32
            } else {
                0
            },
            if plugin_output_buses > 0 {
                audio_outputs.max(1) as i32
            } else {
                0
            },
        )?;

        // Temporary workaround: some Linux VST3 plugins (notably lsp-plugins) crash
        // inside setProcessing(1). We keep initialization stable and rely on process()
        // calls without explicitly toggling processing state.
        // Temporary workaround: querying IEditController parameters crashes on
        // some Linux VST3 plugins (e.g. lsp-plugins), so keep parameter list empty.
        let parameters = Vec::new();
        let scalar_values = Arc::new(Mutex::new(Vec::new()));
        let previous_values = Arc::new(Mutex::new(Vec::new()));
        let plugin_id = format!("{:02X?}", class_info.cid);

        Ok(Self {
            path: plugin_path.to_string(),
            name,
            plugin_id,
            instance,
            _factory: factory,
            audio_inputs: audio_input_ios,
            audio_outputs: audio_output_ios,
            midi_input_ports,
            midi_output_ports,
            input_buses,
            output_buses,
            parameters,
            scalar_values,
            previous_values,
        })
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    pub fn audio_inputs(&self) -> &[Arc<AudioIO>] {
        &self.audio_inputs
    }

    pub fn audio_outputs(&self) -> &[Arc<AudioIO>] {
        &self.audio_outputs
    }

    pub fn midi_input_count(&self) -> usize {
        self.midi_input_ports
    }

    pub fn midi_output_count(&self) -> usize {
        self.midi_output_ports
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

        // Get the audio processor
        let processor = match &self.instance.audio_processor {
            Some(proc) => proc,
            None => {
                self.process_silence();
                return;
            }
        };

        // Call real VST3 processing (no MIDI)
        if let Err(e) = self.process_vst3(processor, frames) {
            eprintln!("VST3 processing error: {e}, producing silence");
            self.process_silence();
        }
    }

    /// Process audio with MIDI events
    pub fn process_with_midi(&self, frames: usize, input_events: &[MidiEvent]) -> Vec<MidiEvent> {
        // Process all input AudioIO ports
        for input in &self.audio_inputs {
            input.process();
        }

        if !input_events.is_empty() {
            // MIDI event list wiring into VST3 ProcessData is not implemented yet.
        }

        // Get the audio processor
        let processor = match &self.instance.audio_processor {
            Some(proc) => proc,
            None => {
                self.process_silence();
                return Vec::new();
            }
        };

        // Call real VST3 processing with MIDI
        match self.process_vst3(processor, frames) {
            Ok(output_buffer) => {
                // Convert output events back to MIDI
                output_buffer.to_midi_events()
            }
            Err(e) => {
                eprintln!("VST3 processing error: {e}, producing silence");
                self.process_silence();
                Vec::new()
            }
        }
    }

    fn process_vst3(
        &self,
        processor: &vst3::ComPtr<vst3::Steinberg::Vst::IAudioProcessor>,
        frames: usize,
    ) -> Result<EventBuffer, String> {
        use vst3::Steinberg::Vst::IAudioProcessorTrait;
        use vst3::Steinberg::Vst::*;

        // Keep buffer guards alive while the plugin reads/writes through raw pointers.
        let input_guards: Vec<_> = self.audio_inputs.iter().map(|io| io.buffer.lock()).collect();
        let output_guards: Vec<_> = self.audio_outputs.iter().map(|io| io.buffer.lock()).collect();

        let mut input_channel_ptrs: Vec<*mut f32> = input_guards
            .iter()
            .map(|buf| buf.as_ptr() as *mut f32)
            .collect();
        let mut output_channel_ptrs: Vec<*mut f32> = output_guards
            .iter()
            .map(|buf| buf.as_ptr() as *mut f32)
            .collect();

        let max_input_frames = input_guards.iter().map(|buf| buf.len()).min().unwrap_or(frames);
        let max_output_frames = output_guards
            .iter()
            .map(|buf| buf.len())
            .min()
            .unwrap_or(frames);
        let num_frames = frames.min(max_input_frames).min(max_output_frames);
        if num_frames == 0 {
            return Ok(EventBuffer::new());
        }

        let mut input_buses = Vec::new();
        if !self.input_buses.is_empty() && !input_channel_ptrs.is_empty() {
            input_buses.push(AudioBusBuffers {
                numChannels: input_channel_ptrs.len() as i32,
                silenceFlags: 0,
                __field0: AudioBusBuffers__type0 {
                    channelBuffers32: input_channel_ptrs.as_mut_ptr(),
                },
            });
        }

        let mut output_buses = Vec::new();
        if !self.output_buses.is_empty() && !output_channel_ptrs.is_empty() {
            output_buses.push(AudioBusBuffers {
                numChannels: output_channel_ptrs.len() as i32,
                silenceFlags: 0,
                __field0: AudioBusBuffers__type0 {
                    channelBuffers32: output_channel_ptrs.as_mut_ptr(),
                },
            });
        }

        // Create ProcessData
        let mut process_context: ProcessContext = unsafe { std::mem::zeroed() };
        let mut process_data = ProcessData {
            processMode: kRealtime as i32,
            symbolicSampleSize: kSample32 as i32,
            numSamples: num_frames as i32,
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
            processContext: &mut process_context,
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

    fn process_silence(&self) {
        for output in &self.audio_outputs {
            let out_buf = output.buffer.lock();
            out_buf.fill(0.0);
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

    pub fn set_parameter_value(
        &mut self,
        param_id: u32,
        normalized_value: f32,
    ) -> Result<(), String> {
        let idx = self
            .parameters
            .iter()
            .position(|p| p.id == param_id)
            .ok_or("Parameter not found")?;

        self.scalar_values.lock().unwrap()[idx] = normalized_value;

        // Update controller if available
        if let Some(controller) = &self.instance.edit_controller {
            use vst3::Steinberg::Vst::IEditControllerTrait;
            unsafe {
                controller.setParamNormalized(param_id, normalized_value as f64);
            }
        }

        Ok(())
    }

    /// Snapshot the current plugin state for saving
    pub fn snapshot_state(&self) -> Result<Vst3PluginState, String> {
        use vst3::Steinberg::Vst::{IComponentTrait, IEditControllerTrait};

        let instance = &self.instance;

        // Save component state
        let mut comp_stream = MemoryStream::new();
        unsafe {
            let result = instance
                .component
                .getState(comp_stream.as_ibstream_mut() as *mut _ as *mut _);
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

        let instance = &self.instance;

        // Restore component state
        if !state.component_state.is_empty() {
            let mut comp_stream = MemoryStream::from_bytes(&state.component_state);
            unsafe {
                let result = instance
                    .component
                    .setState(comp_stream.as_ibstream_mut() as *mut _ as *mut _);
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
                    let result =
                        controller.setState(ctrl_stream.as_ibstream_mut() as *mut _ as *mut _);
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
        // Keep symmetric with load workaround: avoid calling setProcessing(0)
        // when we never called setProcessing(1).
        let _ = self.instance.set_active(false);
        let _ = self.instance.terminate();
    }
}

// Standalone function for listing plugins (backward compatibility)
pub fn list_plugins() -> Vec<super::host::Vst3PluginInfo> {
    super::host::list_plugins()
}
