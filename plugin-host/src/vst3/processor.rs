#![allow(clippy::unnecessary_cast)]

use super::interfaces::{HostPlugFrame, PluginInstance, Vst3GuiInfo, protected_call};
use super::midi::{EventBuffer, ParameterChanges};
use super::port::{BusInfo, ParameterInfo};
use super::state::{MemoryStream, Vst3PluginState, ibstream_ptr};
use crate::util::AudioPort;
use crate::util::MidiEvent;
use std::ffi::{CString, c_void};
use std::fmt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use vst3::ComPtr;
use vst3::ComWrapper;
use vst3::Steinberg::Vst::ProcessModes_::kRealtime;
use vst3::Steinberg::Vst::SymbolicSampleSizes_::kSample32;
use vst3::Steinberg::Vst::{IEditControllerTrait, ViewType};
use vst3::Steinberg::{FIDString, IPlugFrame, IPlugView, IPlugViewTrait, ViewRect, kResultOk};

#[derive(Clone, Copy, Debug, Default)]
pub struct Vst3TransportInfo {
    pub playhead_sample: i64,
    pub playing: bool,
    pub tempo: f64,
    pub tsig_num: i32,
    pub tsig_denom: i32,
}

pub struct Vst3Processor {
    path: String,
    name: String,
    plugin_id: String,

    instance: PluginInstance,

    _factory: super::interfaces::PluginFactory,

    audio_inputs: Vec<Arc<AudioPort>>,
    audio_outputs: Vec<Arc<AudioPort>>,
    midi_input_ports: usize,
    midi_output_ports: usize,
    main_audio_inputs: usize,
    main_audio_outputs: usize,
    input_buses: Vec<BusInfo>,
    output_buses: Vec<BusInfo>,

    parameters: Vec<ParameterInfo>,
    scalar_values: Arc<Mutex<Vec<f32>>>,
    previous_values: Arc<Mutex<Vec<f32>>>,

    pending_param_changes: Arc<Mutex<Vec<(u32, f64, u32)>>>,
    max_samples_per_block: usize,
    processing_started: bool,
    sample_rate: f64,
    bypassed: AtomicBool,
    transport_info: Mutex<Vst3TransportInfo>,

    gui_session: Arc<Mutex<Vst3GuiSession>>,
}

struct Vst3GuiSession {
    view: Option<ComPtr<IPlugView>>,
    plug_frame: Option<ComWrapper<HostPlugFrame>>,
    ui_should_close: bool,
    platform_type: Option<String>,
}

impl fmt::Debug for Vst3Processor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Vst3Processor")
            .field("path", &self.path)
            .field("name", &self.name)
            .field("plugin_id", &self.plugin_id)
            .field("audio_inputs", &self.audio_inputs.len())
            .field("audio_outputs", &self.audio_outputs.len())
            .field("midi_input_ports", &self.midi_input_ports)
            .field("midi_output_ports", &self.midi_output_ports)
            .field("main_audio_inputs", &self.main_audio_inputs)
            .field("main_audio_outputs", &self.main_audio_outputs)
            .field("input_buses", &self.input_buses)
            .field("output_buses", &self.output_buses)
            .field("parameters", &self.parameters)
            .field("max_samples_per_block", &self.max_samples_per_block)
            .field("processing_started", &self.processing_started)
            .finish()
    }
}

impl Vst3Processor {
    pub fn new_with_sample_rate(
        sample_rate: f64,
        buffer_size: usize,
        plugin_path: &str,
        audio_inputs: usize,
        audio_outputs: usize,
    ) -> Result<Self, String> {
        let path_buf = Path::new(plugin_path);
        let name = path_buf
            .file_stem()
            .or_else(|| path_buf.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown VST3")
            .to_string();

        let factory = super::interfaces::PluginFactory::from_module(path_buf)?;

        let class_count = factory.count_classes();
        if class_count == 0 {
            return Err("No plugin classes found".to_string());
        }

        let mut class_info = None;
        for i in 0..class_count {
            if let Some(info) = factory.get_class_info(i)
                && info.category.contains("Audio Module")
            {
                class_info = Some(info);
                break;
            }
        }

        let class_info = class_info
            .or_else(|| factory.get_class_info(0))
            .ok_or("Failed to get class info")?;

        let mut instance = factory.create_instance(&class_info.cid)?;

        instance.initialize(&factory)?;

        let (plugin_input_buses, plugin_output_buses) = instance.audio_bus_counts();
        let (plugin_main_in_channels, plugin_main_out_channels) =
            instance.main_audio_channel_counts();
        let (midi_input_ports, midi_output_ports) = instance.event_bus_counts();

        let requested_inputs = if plugin_input_buses > 0 {
            audio_inputs
                .max(1)
                .min(plugin_main_in_channels.max(1))
                .min(i32::MAX as usize)
        } else {
            0
        };
        let requested_outputs = if plugin_output_buses > 0 {
            audio_outputs
                .max(1)
                .min(plugin_main_out_channels.max(1))
                .min(i32::MAX as usize)
        } else {
            0
        };

        let input_buses = if plugin_input_buses > 0 {
            vec![BusInfo {
                index: 0,
                name: "Input".to_string(),
                channel_count: requested_inputs.max(1),
                is_active: true,
            }]
        } else {
            vec![]
        };

        let output_buses = if plugin_output_buses > 0 {
            vec![BusInfo {
                index: 0,
                name: "Output".to_string(),
                channel_count: requested_outputs.max(1),
                is_active: true,
            }]
        } else {
            vec![]
        };

        let mut audio_input_ios = Vec::new();
        for _ in 0..requested_inputs {
            audio_input_ios.push(Arc::new(AudioPort::new(buffer_size)));
        }

        let mut audio_output_ios = Vec::new();
        for _ in 0..requested_outputs {
            audio_output_ios.push(Arc::new(AudioPort::new(buffer_size)));
        }

        instance.setup_processing(
            sample_rate,
            buffer_size as i32,
            requested_inputs as i32,
            requested_outputs as i32,
        )?;
        instance.set_active(true)?;

        let processing_started = false;

        let parameters = protected_call(|| instance.query_parameters()).unwrap_or_default();
        let scalar_values = Arc::new(Mutex::new(
            parameters.iter().map(|p| p.default_value as f32).collect(),
        ));
        let previous_values = Arc::new(Mutex::new(
            parameters.iter().map(|p| p.default_value as f32).collect(),
        ));
        let pending_param_changes = Arc::new(Mutex::new(Vec::new()));
        let plugin_id = format!("{:02X?}", class_info.cid);

        let gui_session = Arc::new(Mutex::new(Vst3GuiSession {
            view: None,
            plug_frame: None,
            ui_should_close: false,
            platform_type: None,
        }));

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
            main_audio_inputs: requested_inputs,
            main_audio_outputs: requested_outputs,
            input_buses,
            output_buses,
            parameters,
            scalar_values,
            previous_values,
            pending_param_changes,
            max_samples_per_block: buffer_size,
            processing_started,
            sample_rate,
            bypassed: AtomicBool::new(false),
            transport_info: Mutex::new(Vst3TransportInfo::default()),
            gui_session,
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

    pub fn audio_inputs(&self) -> &[Arc<AudioPort>] {
        &self.audio_inputs
    }

    pub fn audio_outputs(&self) -> &[Arc<AudioPort>] {
        &self.audio_outputs
    }

    pub fn main_audio_input_count(&self) -> usize {
        self.main_audio_inputs
    }

    pub fn main_audio_output_count(&self) -> usize {
        self.main_audio_outputs
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

    pub fn set_bypassed(&self, bypassed: bool) {
        self.bypassed.store(bypassed, Ordering::Relaxed);
    }

    pub fn is_bypassed(&self) -> bool {
        self.bypassed.load(Ordering::Relaxed)
    }

    fn bypass_copy_inputs_to_outputs(&self) {
        for (input, output) in self.audio_inputs.iter().zip(self.audio_outputs.iter()) {
            let src = input.buffer.lock();
            let dst = output.buffer.lock();
            dst.fill(0.0);
            for (d, s) in dst.iter_mut().zip(src.iter()) {
                *d = *s;
            }
            *output.finished.lock() = true;
        }
        for output in self.audio_outputs.iter().skip(self.audio_inputs.len()) {
            output.buffer.lock().fill(0.0);
            *output.finished.lock() = true;
        }
    }

    pub fn process_with_audio_io(&self, frames: usize) {
        for input in &self.audio_inputs {
            input.process();
        }
        if self.bypassed.load(Ordering::Relaxed) {
            self.bypass_copy_inputs_to_outputs();
            return;
        }

        let processor = match &self.instance.audio_processor {
            Some(proc) => proc,
            None => {
                self.process_silence();
                return;
            }
        };

        if self.process_vst3(processor, frames, &[]).is_err() {
            self.process_silence();
        }
    }

    #[allow(clippy::unnecessary_cast)]
    pub fn process_with_midi(&self, frames: usize, input_events: &[MidiEvent]) -> Vec<MidiEvent> {
        for input in &self.audio_inputs {
            input.process();
        }
        if self.bypassed.load(Ordering::Relaxed) {
            self.bypass_copy_inputs_to_outputs();
            return Vec::new();
        }

        let processor = match &self.instance.audio_processor {
            Some(proc) => proc,
            None => {
                self.process_silence();
                return Vec::new();
            }
        };

        match self.process_vst3(processor, frames, input_events) {
            Ok(output_buffer) => output_buffer.to_midi_events(),
            Err(_) => {
                self.process_silence();
                Vec::new()
            }
        }
    }

    fn process_vst3(
        &self,
        processor: &vst3::ComPtr<vst3::Steinberg::Vst::IAudioProcessor>,
        frames: usize,
        input_events: &[MidiEvent],
    ) -> Result<EventBuffer, String> {
        use vst3::Steinberg::Vst::IAudioProcessorTrait;
        use vst3::Steinberg::Vst::*;

        let input_guards: Vec<_> = self
            .audio_inputs
            .iter()
            .map(|io| io.buffer.lock())
            .collect();
        let output_guards: Vec<_> = self
            .audio_outputs
            .iter()
            .map(|io| io.buffer.lock())
            .collect();

        let mut input_channel_ptrs: Vec<*mut f32> = input_guards
            .iter()
            .map(|buf| buf.as_ptr() as *mut f32)
            .collect();
        let mut output_channel_ptrs: Vec<*mut f32> = output_guards
            .iter()
            .map(|buf| buf.as_ptr() as *mut f32)
            .collect();

        let max_input_frames = input_guards
            .iter()
            .map(|buf| buf.len())
            .min()
            .unwrap_or(frames);
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

        let transport = self
            .transport_info
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let mut process_context: ProcessContext = unsafe { std::mem::zeroed() };
        process_context.sampleRate = self.sample_rate;
        process_context.tempo = transport.tempo;
        process_context.timeSigNumerator = transport.tsig_num;
        process_context.timeSigDenominator = transport.tsig_denom;
        process_context.projectTimeSamples = transport.playhead_sample;
        #[allow(clippy::unnecessary_cast)]
        {
            let mut state = ProcessContext_::StatesAndFlags_::kTempoValid
                | ProcessContext_::StatesAndFlags_::kTimeSigValid
                | ProcessContext_::StatesAndFlags_::kContTimeValid
                | ProcessContext_::StatesAndFlags_::kSystemTimeValid;
            if transport.playing {
                state |= ProcessContext_::StatesAndFlags_::kPlaying;
            }
            process_context.state = state as u32;
        }
        let input_event_list = if self.midi_input_ports > 0 {
            Some(ComWrapper::new(EventBuffer::from_midi_events(
                input_events,
                0,
            )))
        } else {
            None
        };
        let midi_mapping = self
            .instance
            .edit_controller
            .as_ref()
            .and_then(|controller| controller.cast::<IMidiMapping>());
        let param_changes = ParameterChanges::new();
        {
            let mut pending = self.pending_param_changes.lock().unwrap();
            for (param_id, value, offset) in pending.drain(..) {
                param_changes.add_point(param_id, offset as i32, value);
            }
        }
        if let Some(mapping) = midi_mapping
            && let Some(midi_changes) =
                ParameterChanges::from_midi_events(input_events, &mapping, 0)
        {
            for queue in midi_changes.queues_ref() {
                for (offset, value) in queue.points_ref() {
                    param_changes.add_point(queue.parameter_id(), *offset, *value);
                }
            }
        }
        let input_parameter_changes = if !param_changes.queues_ref().is_empty() {
            Some(ComWrapper::new(param_changes))
        } else {
            None
        };
        let output_event_list = if self.midi_output_ports > 0 {
            Some(ComWrapper::new(EventBuffer::new()))
        } else {
            None
        };
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
            inputParameterChanges: input_parameter_changes
                .as_ref()
                .map(ParameterChanges::changes_ptr)
                .unwrap_or(std::ptr::null_mut()),
            outputParameterChanges: std::ptr::null_mut(),
            inputEvents: input_event_list
                .as_ref()
                .map(EventBuffer::event_list_ptr)
                .unwrap_or(std::ptr::null_mut()),
            outputEvents: output_event_list
                .as_ref()
                .map(EventBuffer::event_list_ptr)
                .unwrap_or(std::ptr::null_mut()),
            processContext: &mut process_context,
        };

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            processor.process(&mut process_data)
        }));

        match result {
            Ok(vst3::Steinberg::kResultOk) => {}
            Ok(err_code) => {
                return Err(format!("VST3 process failed with result: {}", err_code));
            }
            Err(_) => {
                return Err("VST3 process panicked".to_string());
            }
        }

        for output in &self.audio_outputs {
            *output.finished.lock() = true;
        }

        Ok(output_event_list
            .as_ref()
            .map(|events| EventBuffer::from_midi_events(&events.to_midi_events(), 0))
            .unwrap_or_default())
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

    pub fn set_transport_info(&self, info: Vst3TransportInfo) {
        if let Ok(mut t) = self.transport_info.lock() {
            *t = info;
        }
    }

    pub fn set_parameter_value(&self, param_id: u32, normalized_value: f32) -> Result<(), String> {
        self.set_parameter_value_at(param_id, normalized_value, 0)
    }

    pub fn set_parameter_value_at(
        &self,
        param_id: u32,
        normalized_value: f32,
        sample_offset: u32,
    ) -> Result<(), String> {
        let idx = self
            .parameters
            .iter()
            .position(|p| p.id == param_id)
            .ok_or("Parameter not found")?;

        self.scalar_values.lock().unwrap()[idx] = normalized_value;

        if let Some(controller) = &self.instance.edit_controller {
            use vst3::Steinberg::Vst::IEditControllerTrait;
            unsafe {
                controller.setParamNormalized(param_id, normalized_value as f64);
            }
        }

        self.pending_param_changes.lock().unwrap().push((
            param_id,
            normalized_value as f64,
            sample_offset,
        ));

        Ok(())
    }

    pub fn snapshot_state(&self) -> Result<Vst3PluginState, String> {
        use vst3::Steinberg::Vst::{IComponentTrait, IEditControllerTrait};

        let instance = &self.instance;

        let comp_stream = vst3::ComWrapper::new(MemoryStream::new());
        unsafe {
            let result = instance
                .component
                .getState(ibstream_ptr(&comp_stream) as *mut _);
            if result != vst3::Steinberg::kResultOk {
                return Err("Failed to get component state".to_string());
            }
        }

        let ctrl_stream = vst3::ComWrapper::new(MemoryStream::new());
        if let Some(controller) = &instance.edit_controller {
            unsafe {
                controller.getState(ibstream_ptr(&ctrl_stream) as *mut _);
            }
        }

        Ok(Vst3PluginState {
            plugin_id: self.plugin_id.clone(),
            component_state: comp_stream.bytes(),
            controller_state: ctrl_stream.bytes(),
        })
    }

    pub fn restore_state(&self, state: &Vst3PluginState) -> Result<(), String> {
        use vst3::Steinberg::Vst::{IComponentTrait, IEditControllerTrait};

        if state.plugin_id != self.plugin_id {
            return Err(format!(
                "Plugin ID mismatch: expected '{}', got '{}'",
                self.plugin_id, state.plugin_id
            ));
        }

        let instance = &self.instance;

        if !state.component_state.is_empty() {
            let comp_stream =
                vst3::ComWrapper::new(MemoryStream::from_bytes(&state.component_state));
            unsafe {
                let result = instance
                    .component
                    .setState(ibstream_ptr(&comp_stream) as *mut _);
                if result != vst3::Steinberg::kResultOk {
                    return Err("Failed to set component state".to_string());
                }
            }
        }

        if !state.controller_state.is_empty()
            && let Some(controller) = &instance.edit_controller
        {
            let ctrl_stream =
                vst3::ComWrapper::new(MemoryStream::from_bytes(&state.controller_state));
            unsafe {
                controller.setState(ibstream_ptr(&ctrl_stream) as *mut _);
            }

            for (idx, param) in self.parameters.iter().enumerate() {
                let value = unsafe { controller.getParamNormalized(param.id) };
                self.scalar_values.lock().unwrap()[idx] = value as f32;
                self.previous_values.lock().unwrap()[idx] = value as f32;
            }
        }

        Ok(())
    }

    pub fn gui_info(&self) -> Result<Vst3GuiInfo, String> {
        let controller = self
            .instance
            .edit_controller
            .as_ref()
            .ok_or("No edit controller available")?;
        let view = unsafe { controller.createView(ViewType::kEditor) };
        if view.is_null() {
            return Ok(Vst3GuiInfo {
                has_gui: false,
                size: None,
            });
        }

        unsafe {
            let _ = ComPtr::<IPlugView>::from_raw(view);
        }
        Ok(Vst3GuiInfo {
            has_gui: true,
            size: None,
        })
    }

    pub fn gui_create(&self, platform_type: &str) -> Result<(), String> {
        let mut session = self.gui_session.lock().unwrap();
        if session.view.is_some() {
            return Ok(());
        }
        let controller = self
            .instance
            .edit_controller
            .as_ref()
            .ok_or("No edit controller available")?;
        let view = unsafe { controller.createView(ViewType::kEditor) };
        if view.is_null() {
            return Err("Plugin does not provide an editor view".to_string());
        }
        let view =
            unsafe { ComPtr::<IPlugView>::from_raw(view) }.ok_or("Failed to wrap IPlugView")?;

        let platform_cstr =
            CString::new(platform_type).map_err(|e| format!("Invalid platform type: {e}"))?;
        let supported =
            unsafe { view.isPlatformTypeSupported(platform_cstr.as_ptr() as FIDString) };
        if supported != kResultOk {
            return Err(format!("Platform type '{}' not supported", platform_type));
        }

        session.view = Some(view);
        session.platform_type = Some(platform_type.to_string());
        Ok(())
    }

    pub fn gui_get_size(&self) -> Result<(i32, i32), String> {
        let session = self.gui_session.lock().unwrap();
        let view = session.view.as_ref().ok_or("No GUI view created")?;
        let mut rect = ViewRect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        let result = unsafe { view.getSize(&mut rect) };
        if result != kResultOk {
            return Err("Failed to get GUI size".to_string());
        }
        Ok((rect.right - rect.left, rect.bottom - rect.top))
    }

    pub fn gui_set_parent(&self, window: usize, platform_type: &str) -> Result<(), String> {
        let mut session = self.gui_session.lock().unwrap();
        let view = session.view.as_ref().ok_or("No GUI view created")?;

        let plug_frame = ComWrapper::new(HostPlugFrame::new());
        if let Some(frame_ptr) = plug_frame.to_com_ptr::<IPlugFrame>() {
            unsafe {
                let _ = view.setFrame(frame_ptr.into_raw());
            }
        }

        let platform_cstr =
            CString::new(platform_type).map_err(|e| format!("Invalid platform type: {e}"))?;
        let result =
            unsafe { view.attached(window as *mut c_void, platform_cstr.as_ptr() as FIDString) };
        if result != kResultOk {
            return Err(format!("Failed to attach GUI view: {:#x}", result));
        }

        session.plug_frame = Some(plug_frame);
        Ok(())
    }

    pub fn gui_on_size(&self, width: i32, height: i32) -> Result<(), String> {
        let session = self.gui_session.lock().unwrap();
        let view = session.view.as_ref().ok_or("No GUI view created")?;
        let mut rect = ViewRect {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        };
        unsafe {
            let _ = view.onSize(&mut rect);
        }
        Ok(())
    }

    pub fn gui_show(&self) -> Result<(), String> {
        let session = self.gui_session.lock().unwrap();
        let view = session.view.as_ref().ok_or("No GUI view created")?;
        unsafe {
            let _ = view.onFocus(1);
        }
        Ok(())
    }

    pub fn gui_hide(&self) {
        if let Ok(session) = self.gui_session.lock()
            && let Some(view) = session.view.as_ref()
        {
            unsafe {
                let _ = view.onFocus(0);
            }
        }
    }

    pub fn gui_destroy(&self) {
        if let Ok(mut session) = self.gui_session.lock() {
            if let Some(view) = session.view.take() {
                unsafe {
                    let _ = view.setFrame(std::ptr::null_mut());
                    let _ = view.removed();
                }
            }
            session.plug_frame.take();
            session.platform_type.take();
        }
    }

    pub fn ui_begin_session(&self) {
        if let Ok(mut session) = self.gui_session.lock() {
            session.ui_should_close = false;
        }
    }

    pub fn ui_end_session(&self) {
        if let Ok(mut session) = self.gui_session.lock() {
            session.ui_should_close = false;
        }
    }

    pub fn ui_should_close(&self) -> bool {
        if let Ok(session) = self.gui_session.lock() {
            return session.ui_should_close;
        }
        false
    }

    pub fn ui_take_param_updates(&self) -> Vec<(u32, f64)> {
        let mut changes = Vec::new();
        if let Ok(mut param_changes) = self.instance.parameter_changes.lock() {
            std::mem::swap(&mut changes, &mut *param_changes);
        }
        changes
    }

    pub fn gui_check_resize(&self) -> Option<(i32, i32)> {
        if let Ok(session) = self.gui_session.lock()
            && let Some(ref frame) = session.plug_frame
            && frame.resize_requested.swap(false, Ordering::Relaxed)
            && let Ok(size) = frame.requested_size.lock()
        {
            return *size;
        }
        None
    }

    pub fn gui_on_main_thread(&self) {
        super::interfaces::pump_host_run_loop();
    }
}

impl Drop for Vst3Processor {
    fn drop(&mut self) {
        if self.processing_started {
            self.instance.stop_processing();
        }
        let _ = self.instance.set_active(false);
        let _ = self.instance.terminate();
    }
}

pub fn list_plugins() -> Vec<super::host::Vst3PluginInfo> {
    super::host::list_plugins()
}
