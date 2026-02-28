#![cfg(not(target_os = "macos"))]

use std::{
    collections::{HashMap, HashSet},
    ffi::{CStr, CString, c_char, c_void},
    fmt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use crate::audio::io::AudioIO;
use crate::message::{Lv2ControlPortInfo, Lv2PluginState, Lv2StatePortValue, Lv2StateProperty};
use crate::midi::io::MidiEvent;
use crate::mutex::UnsafeMutex;
use lilv::{World, instance::ActiveInstance, node::Node, plugin::Plugin};
use lv2_raw::{
    LV2_ATOM__DOUBLE, LV2_ATOM__FRAMETIME, LV2_ATOM__INT, LV2_ATOM__LONG, LV2_ATOM__OBJECT,
    LV2_ATOM__SEQUENCE, LV2_URID__MAP, LV2_URID__UNMAP, LV2Atom, LV2AtomDouble, LV2AtomEvent,
    LV2AtomLong, LV2AtomObjectBody, LV2AtomPropertyBody, LV2AtomSequence, LV2AtomSequenceBody,
    LV2Feature, LV2Urid, LV2UridMap, LV2UridMapHandle, lv2_atom_pad_size,
    lv2_atom_sequence_append_event, lv2_atom_sequence_begin, lv2_atom_sequence_is_end,
    lv2_atom_sequence_next,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lv2PluginInfo {
    pub uri: String,
    pub name: String,
    pub class_label: String,
    pub bundle_uri: String,
    pub required_features: Vec<String>,
    pub audio_inputs: usize,
    pub audio_outputs: usize,
    pub midi_inputs: usize,
    pub midi_outputs: usize,
}

pub struct Lv2Host {
    world: World,
    sample_rate: f64,
    loaded_plugins: HashMap<String, LoadedPlugin>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Lv2TransportInfo {
    pub transport_sample: usize,
    pub playing: bool,
    pub bpm: f64,
    pub tsig_num: u32,
    pub tsig_denom: u32,
}

type Lv2WorkerStatus = u32;
const LV2_WORKER_SUCCESS: Lv2WorkerStatus = 0;
const LV2_WORKER_ERR_UNKNOWN: Lv2WorkerStatus = 1;
const LV2_WORKER__SCHEDULE: &str = "http://lv2plug.in/ns/ext/worker#schedule";
const LV2_WORKER__INTERFACE: &str = "http://lv2plug.in/ns/ext/worker#interface";

#[repr(C)]
struct Lv2WorkerSchedule {
    handle: *mut c_void,
    schedule_work:
        Option<unsafe extern "C" fn(handle: *mut c_void, size: u32, data: *const c_void) -> u32>,
}

type Lv2WorkerRespondFunc =
    Option<unsafe extern "C" fn(handle: *mut c_void, size: u32, data: *const c_void) -> u32>;

#[repr(C)]
struct Lv2WorkerInterface {
    work: Option<
        unsafe extern "C" fn(
            handle: *mut c_void,
            respond: Lv2WorkerRespondFunc,
            respond_handle: *mut c_void,
            size: u32,
            data: *const c_void,
        ) -> u32,
    >,
    work_response:
        Option<unsafe extern "C" fn(handle: *mut c_void, size: u32, data: *const c_void) -> u32>,
    end_run: Option<unsafe extern "C" fn(handle: *mut c_void)>,
}

struct WorkerScheduleState {
    jobs: UnsafeMutex<Vec<Vec<u8>>>,
    responses: UnsafeMutex<Vec<Vec<u8>>>,
}

struct WorkerFeature {
    _uri: CString,
    _schedule: Box<Lv2WorkerSchedule>,
    feature: LV2Feature,
    state: Box<WorkerScheduleState>,
}

unsafe impl Send for WorkerFeature {}

#[derive(Clone, Copy)]
enum PortBinding {
    AudioInput(usize),
    AudioOutput(usize),
    AtomInput(usize),
    AtomOutput(usize),
    Scalar(usize),
}

pub struct Lv2Processor {
    uri: String,
    plugin_name: String,
    sample_rate: f64,
    instance: Option<ActiveInstance>,
    _world: World,
    _urid_feature: UridMapFeature,
    _state_path_feature: StatePathFeature,
    _instantiate_features: InstantiateFeatureSet,
    port_bindings: Vec<PortBinding>,
    scalar_values: Arc<Mutex<Vec<f32>>>,
    audio_inputs: Vec<Arc<AudioIO>>,
    audio_outputs: Vec<Arc<AudioIO>>,
    atom_inputs: Vec<AtomBuffer>,
    atom_outputs: Vec<AtomBuffer>,
    atom_sequence_urid: LV2Urid,
    atom_object_urid: LV2Urid,
    atom_long_urid: LV2Urid,
    atom_double_urid: LV2Urid,
    atom_frame_time_urid: LV2Urid,
    midi_event_urid: LV2Urid,
    time_position_urid: LV2Urid,
    time_frame_urid: LV2Urid,
    time_speed_urid: LV2Urid,
    time_bpm_urid: LV2Urid,
    time_bar_urid: LV2Urid,
    time_bar_beat_urid: LV2Urid,
    time_beats_per_bar_urid: LV2Urid,
    time_beat_unit_urid: LV2Urid,
    midi_inputs: usize,
    midi_outputs: usize,
    has_worker_interface: bool,
    control_ports: Vec<ControlPortInfo>,
}

struct LoadedPlugin {
    instance: ActiveInstance,
    _urid_feature: UridMapFeature,
    _state_path_feature: StatePathFeature,
    _instantiate_features: InstantiateFeatureSet,
}

#[derive(Default)]
struct UridMapState {
    next_urid: LV2Urid,
    by_uri: HashMap<String, LV2Urid>,
    by_urid: HashMap<LV2Urid, CString>,
}

struct UridMapFeature {
    _map_uri: CString,
    _unmap_uri: CString,
    map_feature: LV2Feature,
    unmap_feature: LV2Feature,
    _map: Box<LV2UridMap>,
    _unmap: Box<LV2UridUnmap>,
    _state: Box<Mutex<UridMapState>>,
}

unsafe impl Send for UridMapFeature {}

#[repr(C)]
struct LV2UridUnmap {
    handle: LV2UridMapHandle,
    unmap: extern "C" fn(handle: LV2UridMapHandle, urid: LV2Urid) -> *const c_char,
}

struct InstantiateFeatureSet {
    _feature_uris: Vec<CString>,
    features: Vec<LV2Feature>,
    _worker_feature: WorkerFeature,
    _option_values: Vec<u32>,
    _options: Vec<LV2OptionsOption>,
    _flag_feature_data: Box<u8>,
}

unsafe impl Send for InstantiateFeatureSet {}

#[repr(C)]
#[derive(Clone, Copy)]
struct LV2OptionsOption {
    context: u32,
    subject: u32,
    key: u32,
    size: u32,
    type_: u32,
    value: *const c_void,
}

#[derive(Debug, Clone)]
struct ControlPortInfo {
    index: u32,
    name: String,
    min: f32,
    max: f32,
}

struct AtomBuffer {
    words: Vec<u64>,
}

impl AtomBuffer {
    fn new(len_bytes: usize) -> Self {
        let words = len_bytes.div_ceil(std::mem::size_of::<u64>()).max(1);
        Self {
            words: vec![0; words],
        }
    }

    fn bytes_mut(&mut self) -> &mut [u8] {
        let full_len = self.words.len() * std::mem::size_of::<u64>();
        let raw = self.words.as_mut_ptr().cast::<u8>();
        unsafe { std::slice::from_raw_parts_mut(raw, full_len) }
    }

    fn ptr_mut(&mut self) -> *mut u8 {
        self.words.as_mut_ptr().cast::<u8>()
    }
}

#[derive(Debug)]
struct RawStateProperty {
    key: u32,
    type_: u32,
    flags: u32,
    value: Vec<u8>,
}

struct StateSaveContext {
    properties: Vec<RawStateProperty>,
}

struct StateRestoreContext {
    properties: Vec<RawStateProperty>,
    by_key: HashMap<u32, usize>,
}

type Lv2Handle = *mut c_void;
type Lv2StateHandle = *mut c_void;
type Lv2StateStatus = u32;
type Lv2StateStoreFn = Option<
    unsafe extern "C" fn(
        handle: Lv2StateHandle,
        key: u32,
        value: *const c_void,
        size: usize,
        type_: u32,
        flags: u32,
    ) -> Lv2StateStatus,
>;
type Lv2StateRetrieveFn = Option<
    unsafe extern "C" fn(
        handle: Lv2StateHandle,
        key: u32,
        size: *mut usize,
        type_: *mut u32,
        flags: *mut u32,
    ) -> *const c_void,
>;
const LV2_STATE_STATUS_SUCCESS: Lv2StateStatus = 0;
const LV2_STATE_STATUS_ERR_NO_PROPERTY: Lv2StateStatus = 5;

#[repr(C)]
struct Lv2StateInterface {
    save: Option<
        unsafe extern "C" fn(
            instance: Lv2Handle,
            store: Lv2StateStoreFn,
            handle: Lv2StateHandle,
            flags: u32,
            features: *const *const LV2Feature,
        ) -> Lv2StateStatus,
    >,
    restore: Option<
        unsafe extern "C" fn(
            instance: Lv2Handle,
            retrieve: Lv2StateRetrieveFn,
            handle: Lv2StateHandle,
            flags: u32,
            features: *const *const LV2Feature,
        ) -> Lv2StateStatus,
    >,
}

#[repr(C)]
struct Lv2StateMapPath {
    handle: *mut c_void,
    abstract_path: Option<extern "C" fn(*mut c_void, *const c_char) -> *mut c_char>,
    absolute_path: Option<extern "C" fn(*mut c_void, *const c_char) -> *mut c_char>,
}

#[repr(C)]
struct Lv2StateMakePath {
    handle: *mut c_void,
    path: Option<extern "C" fn(*mut c_void, *const c_char) -> *mut c_char>,
}

#[repr(C)]
struct Lv2StateFreePath {
    handle: *mut c_void,
    free_path: Option<extern "C" fn(*mut c_void, *mut c_char)>,
}

#[derive(Default)]
struct StatePathContext {
    base_dir: PathBuf,
    copy_counter: u64,
}

struct StatePathFeature {
    _map_uri: CString,
    _make_uri: CString,
    _free_uri: CString,
    _map: Box<Lv2StateMapPath>,
    _make: Box<Lv2StateMakePath>,
    _free: Box<Lv2StateFreePath>,
    map_feature: LV2Feature,
    make_feature: LV2Feature,
    free_feature: LV2Feature,
    _context: Box<Mutex<StatePathContext>>,
}

unsafe impl Send for StatePathFeature {}

const LV2_STATE_INTERFACE_URI: &str = "http://lv2plug.in/ns/ext/state#interface";
const LV2_STATE_MAP_PATH_URI: &str = "http://lv2plug.in/ns/ext/state#mapPath";
const LV2_STATE_MAKE_PATH_URI: &str = "http://lv2plug.in/ns/ext/state#makePath";
const LV2_STATE_FREE_PATH_URI: &str = "http://lv2plug.in/ns/ext/state#freePath";
const LV2_OPTIONS__OPTIONS: &str = "http://lv2plug.in/ns/ext/options#options";
const LV2_BUF_SIZE__BOUNDED_BLOCK_LENGTH: &str =
    "http://lv2plug.in/ns/ext/buf-size#boundedBlockLength";
const LV2_BUF_SIZE__MIN_BLOCK_LENGTH: &str = "http://lv2plug.in/ns/ext/buf-size#minBlockLength";
const LV2_BUF_SIZE__MAX_BLOCK_LENGTH: &str = "http://lv2plug.in/ns/ext/buf-size#maxBlockLength";
const LV2_BUF_SIZE__NOMINAL_BLOCK_LENGTH: &str =
    "http://lv2plug.in/ns/ext/buf-size#nominalBlockLength";
const LV2_URID__MAP_URI_TYPO_COMPAT: &str = "http://lv2plug.in/ns//ext/urid#map";

const LV2_ATOM_BUFFER_BYTES: usize = 8192;

impl fmt::Debug for Lv2Processor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Lv2Processor")
            .field("uri", &self.uri)
            .field("audio_inputs", &self.audio_inputs.len())
            .field("audio_outputs", &self.audio_outputs.len())
            .finish()
    }
}

impl Lv2Processor {
    pub fn new(sample_rate: f64, buffer_size: usize, uri: &str) -> Result<Self, String> {
        let world = World::new();
        world.load_all();

        let uri_node = world.new_uri(uri);
        let plugin = world
            .plugins()
            .plugin(&uri_node)
            .ok_or_else(|| format!("Plugin not found for URI: {uri}"))?;
        if !plugin.verify() {
            return Err(format!("Plugin failed verification: {uri}"));
        }

        let mut urid_feature = UridMapFeature::new()?;
        let mut state_path_feature = StatePathFeature::new(default_state_base_dir());
        let (instance, instantiate_features) = instantiate_plugin(
            &plugin,
            sample_rate,
            uri,
            &mut urid_feature,
            &mut state_path_feature,
        )?;
        let active_instance = unsafe { instance.activate() };

        let input_port = world.new_uri("http://lv2plug.in/ns/lv2core#InputPort");
        let output_port = world.new_uri("http://lv2plug.in/ns/lv2core#OutputPort");
        let audio_port = world.new_uri("http://lv2plug.in/ns/lv2core#AudioPort");
        let control_port = world.new_uri("http://lv2plug.in/ns/lv2core#ControlPort");
        let atom_port = world.new_uri("http://lv2plug.in/ns/ext/atom#AtomPort");
        let event_port = world.new_uri("http://lv2plug.in/ns/ext/event#EventPort");

        let ports_count = plugin.ports_count();
        let mut port_bindings = vec![PortBinding::Scalar(0); ports_count];
        let mut scalar_values = vec![0.0_f32; ports_count.max(1)];
        let mut audio_inputs: Vec<Arc<AudioIO>> = vec![];
        let mut audio_outputs: Vec<Arc<AudioIO>> = vec![];
        let mut atom_inputs: Vec<AtomBuffer> = vec![];
        let mut atom_outputs: Vec<AtomBuffer> = vec![];
        let mut midi_inputs = 0_usize;
        let mut midi_outputs = 0_usize;
        let mut control_ports = vec![];
        let has_worker_interface = plugin.has_extension_data(&world.new_uri(LV2_WORKER__INTERFACE));
        let atom_sequence_urid = urid_feature.map_uri(LV2_ATOM__SEQUENCE);
        let atom_object_urid = urid_feature.map_uri(LV2_ATOM__OBJECT);
        let atom_long_urid = urid_feature.map_uri(LV2_ATOM__LONG);
        let atom_double_urid = urid_feature.map_uri(LV2_ATOM__DOUBLE);
        let atom_frame_time_urid = urid_feature.map_uri(LV2_ATOM__FRAMETIME);
        let midi_event_urid = urid_feature.map_uri(lv2_raw::LV2_MIDI__MIDIEVENT);
        let time_position_urid = urid_feature.map_uri(lv2_raw::LV2_TIME__POSITION);
        let time_frame_urid = urid_feature.map_uri(lv2_raw::LV2_TIME__FRAME);
        let time_speed_urid = urid_feature.map_uri(lv2_raw::LV2_TIME__SPEED);
        let time_bpm_urid = urid_feature.map_uri(lv2_raw::LV2_TIME__BEATSPERMINUTE);
        let time_bar_urid = urid_feature.map_uri(lv2_raw::LV2_TIME__BAR);
        let time_bar_beat_urid = urid_feature.map_uri(lv2_raw::LV2_TIME__BARBEAT);
        let time_beats_per_bar_urid = urid_feature.map_uri(lv2_raw::LV2_TIME__BEATSPERBAR);
        let time_beat_unit_urid = urid_feature.map_uri(lv2_raw::LV2_TIME__BEATUNIT);

        for port in plugin.iter_ports() {
            let index = port.index();
            let is_audio = port.is_a(&audio_port);
            let is_control = port.is_a(&control_port);
            let is_atom = port.is_a(&atom_port) || port.is_a(&event_port);
            let is_input = port.is_a(&input_port);
            let is_output = port.is_a(&output_port);

            if is_audio && is_input {
                let channel = audio_inputs.len();
                audio_inputs.push(Arc::new(AudioIO::new(buffer_size)));
                port_bindings[index] = PortBinding::AudioInput(channel);
            } else if is_audio && is_output {
                let channel = audio_outputs.len();
                audio_outputs.push(Arc::new(AudioIO::new(buffer_size)));
                port_bindings[index] = PortBinding::AudioOutput(channel);
            } else if is_atom && is_input {
                midi_inputs += 1;
                let atom_idx = atom_inputs.len();
                let mut buffer = AtomBuffer::new(LV2_ATOM_BUFFER_BYTES);
                prepare_empty_atom_sequence(
                    buffer.bytes_mut(),
                    atom_sequence_urid,
                    atom_frame_time_urid,
                );
                atom_inputs.push(buffer);
                port_bindings[index] = PortBinding::AtomInput(atom_idx);
            } else if is_atom && is_output {
                midi_outputs += 1;
                let atom_idx = atom_outputs.len();
                let mut buffer = AtomBuffer::new(LV2_ATOM_BUFFER_BYTES);
                prepare_output_atom_sequence(
                    buffer.bytes_mut(),
                    atom_sequence_urid,
                    atom_frame_time_urid,
                );
                atom_outputs.push(buffer);
                port_bindings[index] = PortBinding::AtomOutput(atom_idx);
            } else {
                let default_value = port
                    .range()
                    .default
                    .and_then(|node| node.as_float())
                    .unwrap_or(0.0);
                scalar_values[index] = default_value;
                port_bindings[index] = PortBinding::Scalar(index);

                if is_control && is_input {
                    let range = port.range();
                    let mut min = range
                        .minimum
                        .and_then(|node| node.as_float())
                        .unwrap_or(0.0);
                    let mut max = range
                        .maximum
                        .and_then(|node| node.as_float())
                        .unwrap_or(1.0);
                    if !matches!(min.partial_cmp(&max), Some(std::cmp::Ordering::Less)) {
                        min = default_value - 1.0;
                        max = default_value + 1.0;
                    }
                    let fallback_name = format!("Port {}", index);
                    let name = port
                        .name()
                        .and_then(|node| node.as_str().map(str::to_string))
                        .or_else(|| {
                            port.symbol()
                                .and_then(|node| node.as_str().map(str::to_string))
                        })
                        .unwrap_or(fallback_name);
                    control_ports.push(ControlPortInfo {
                        index: index as u32,
                        name,
                        min,
                        max,
                    });
                }
            }
        }

        let mut processor = Self {
            uri: uri.to_string(),
            plugin_name: plugin
                .name()
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| uri.to_string()),
            sample_rate,
            instance: Some(active_instance),
            _world: world,
            _urid_feature: urid_feature,
            _state_path_feature: state_path_feature,
            _instantiate_features: instantiate_features,
            port_bindings,
            scalar_values: Arc::new(Mutex::new(scalar_values)),
            audio_inputs,
            audio_outputs,
            atom_inputs,
            atom_outputs,
            atom_sequence_urid,
            atom_object_urid,
            atom_long_urid,
            atom_double_urid,
            atom_frame_time_urid,
            midi_event_urid,
            time_position_urid,
            time_frame_urid,
            time_speed_urid,
            time_bpm_urid,
            time_bar_urid,
            time_bar_beat_urid,
            time_beats_per_bar_urid,
            time_beat_unit_urid,
            midi_inputs,
            midi_outputs,
            has_worker_interface,
            control_ports,
        };
        processor.connect_ports();
        Ok(processor)
    }

    pub fn uri(&self) -> &str {
        &self.uri
    }

    pub fn name(&self) -> &str {
        &self.plugin_name
    }

    pub fn audio_inputs(&self) -> &[Arc<AudioIO>] {
        &self.audio_inputs
    }

    pub fn audio_outputs(&self) -> &[Arc<AudioIO>] {
        &self.audio_outputs
    }

    pub fn audio_input_count(&self) -> usize {
        self.audio_inputs.len()
    }

    pub fn audio_output_count(&self) -> usize {
        self.audio_outputs.len()
    }

    pub fn midi_input_count(&self) -> usize {
        self.midi_inputs
    }

    pub fn midi_output_count(&self) -> usize {
        self.midi_outputs
    }

    pub fn setup_audio_ports(&self) {
        for io in &self.audio_inputs {
            io.setup();
        }
        for io in &self.audio_outputs {
            io.setup();
        }
    }

    pub fn process(&mut self, input_channels: &[Vec<f32>], frames: usize) -> Vec<Vec<f32>> {
        if let Ok(mut values) = self.scalar_values.lock() {
            if values.is_empty() {
                values.push(0.0);
            }
        }

        for (channel, io) in self.audio_inputs.iter_mut().enumerate() {
            let buffer = io.buffer.lock();
            buffer.fill(0.0);
            if let Some(input) = input_channels.get(channel) {
                let copy_len = input.len().min(frames);
                buffer[..copy_len].copy_from_slice(&input[..copy_len]);
            }
        }
        for io in &self.audio_outputs {
            let buffer = io.buffer.lock();
            buffer.fill(0.0);
        }
        for buffer in &mut self.atom_inputs {
            prepare_empty_atom_sequence(
                buffer.bytes_mut(),
                self.atom_sequence_urid,
                self.atom_frame_time_urid,
            );
        }
        for buffer in &mut self.atom_outputs {
            prepare_output_atom_sequence(
                buffer.bytes_mut(),
                self.atom_sequence_urid,
                self.atom_frame_time_urid,
            );
        }

        self.connect_ports();
        if let Some(instance) = self.instance.as_mut() {
            unsafe {
                instance.run(frames);
            }
        }
        self.audio_outputs
            .iter()
            .map(|io| io.buffer.lock().as_ref().to_vec())
            .collect()
    }

    pub fn process_with_audio_io(
        &mut self,
        frames: usize,
        midi_inputs: &[Vec<MidiEvent>],
        transport: Lv2TransportInfo,
    ) -> Vec<Vec<MidiEvent>> {
        if let Ok(mut values) = self.scalar_values.lock() {
            if values.is_empty() {
                values.push(0.0);
            }
        }

        for io in &self.audio_outputs {
            let buffer = io.buffer.lock();
            buffer.fill(0.0);
            *io.finished.lock() = false;
        }
        for buffer in &mut self.atom_inputs {
            prepare_empty_atom_sequence(
                buffer.bytes_mut(),
                self.atom_sequence_urid,
                self.atom_frame_time_urid,
            );
        }
        for buffer in &mut self.atom_outputs {
            prepare_output_atom_sequence(
                buffer.bytes_mut(),
                self.atom_sequence_urid,
                self.atom_frame_time_urid,
            );
        }
        for port in 0..self.atom_inputs.len() {
            self.write_transport_event(port, transport);
        }
        for (port, events) in midi_inputs.iter().enumerate() {
            self.write_midi_input_events(port, events);
        }

        self.connect_ports();
        if let Some(instance) = self.instance.as_mut() {
            unsafe {
                instance.run(frames);
            }
        }
        self.run_worker_cycle();

        for io in &self.audio_outputs {
            *io.finished.lock() = true;
        }
        let mut midi_outputs = vec![];
        for port in 0..self.midi_outputs {
            midi_outputs.push(self.read_midi_output_events(port));
        }
        midi_outputs
    }

    fn run_worker_cycle(&mut self) {
        if !self.has_worker_interface {
            return;
        }
        let Some(worker_iface) = self.worker_interface() else {
            return;
        };
        let (work_fn, work_response_fn, end_run_fn) = (
            worker_iface.work,
            worker_iface.work_response,
            worker_iface.end_run,
        );
        let Some(work_fn) = work_fn else {
            return;
        };
        let instance_handle = self.instance_handle();
        if instance_handle.is_null() {
            return;
        }

        let worker_state = &self._instantiate_features._worker_feature.state;
        let mut jobs = std::mem::take(worker_state.jobs.lock());
        for job in jobs.drain(..) {
            if job.len() > (u32::MAX as usize) {
                continue;
            }
            unsafe {
                work_fn(
                    instance_handle,
                    Some(lv2_worker_respond_callback),
                    &**worker_state as *const WorkerScheduleState as *mut c_void,
                    job.len() as u32,
                    job.as_ptr().cast::<c_void>(),
                );
            }
        }
        *worker_state.jobs.lock() = jobs;

        if let Some(work_response_fn) = work_response_fn {
            let mut responses = std::mem::take(worker_state.responses.lock());
            for response in responses.drain(..) {
                if response.len() > (u32::MAX as usize) {
                    continue;
                }
                unsafe {
                    work_response_fn(
                        instance_handle,
                        response.len() as u32,
                        response.as_ptr().cast::<c_void>(),
                    );
                }
            }
            *worker_state.responses.lock() = responses;
        }

        if let Some(end_run_fn) = end_run_fn {
            unsafe {
                end_run_fn(instance_handle);
            }
        }
    }

    fn connect_ports(&mut self) {
        for (port_index, binding) in self.port_bindings.iter().enumerate() {
            match binding {
                PortBinding::AudioInput(channel) => {
                    let ptr = self.audio_inputs[*channel].buffer.lock().as_mut_ptr();
                    if let Some(instance) = self.instance.as_mut() {
                        unsafe {
                            instance.instance_mut().connect_port_mut(port_index, ptr);
                        }
                    }
                }
                PortBinding::AudioOutput(channel) => {
                    let ptr = self.audio_outputs[*channel].buffer.lock().as_mut_ptr();
                    if let Some(instance) = self.instance.as_mut() {
                        unsafe {
                            instance.instance_mut().connect_port_mut(port_index, ptr);
                        }
                    }
                }
                PortBinding::AtomInput(atom_index) => {
                    let ptr = self.atom_inputs[*atom_index].ptr_mut();
                    if let Some(instance) = self.instance.as_mut() {
                        unsafe {
                            instance.instance_mut().connect_port_mut(port_index, ptr);
                        }
                    }
                }
                PortBinding::AtomOutput(atom_index) => {
                    let ptr = self.atom_outputs[*atom_index].ptr_mut();
                    if let Some(instance) = self.instance.as_mut() {
                        unsafe {
                            instance.instance_mut().connect_port_mut(port_index, ptr);
                        }
                    }
                }
                PortBinding::Scalar(index) => {
                    if let Ok(mut values) = self.scalar_values.lock() {
                        if *index < values.len() {
                            let ptr = (&mut values[*index]) as *mut f32;
                            if let Some(instance) = self.instance.as_mut() {
                                unsafe {
                                    instance.instance_mut().connect_port_mut(port_index, ptr);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn snapshot_state(&self) -> Lv2PluginState {
        let mut state = Lv2PluginState {
            port_values: self.control_port_values(),
            properties: vec![],
        };
        let Some(interface) = self.state_interface() else {
            return state;
        };
        let Some(save_fn) = interface.save else {
            return state;
        };

        let mut ctx = StateSaveContext { properties: vec![] };
        let features = self.state_feature_ptrs();
        let status = unsafe {
            save_fn(
                self.instance_handle(),
                Some(lv2_state_store_callback),
                (&mut ctx as *mut StateSaveContext).cast::<c_void>(),
                0,
                features.as_ptr(),
            )
        };
        if status != LV2_STATE_STATUS_SUCCESS {
            return state;
        }

        state.properties = ctx
            .properties
            .into_iter()
            .filter_map(|p| {
                let key_uri = self._urid_feature.unmap_urid(p.key)?;
                let type_uri = self._urid_feature.unmap_urid(p.type_)?;
                Some(Lv2StateProperty {
                    key_uri,
                    type_uri,
                    flags: p.flags,
                    value: p.value,
                })
            })
            .collect();
        state
    }

    pub fn snapshot_port_state(&self) -> Lv2PluginState {
        Lv2PluginState {
            port_values: self.control_port_values(),
            properties: vec![],
        }
    }

    pub fn restore_state(&mut self, state: &Lv2PluginState) -> Result<(), String> {
        self.set_control_port_values(&state.port_values);
        if state.properties.is_empty() {
            return Ok(());
        }
        let Some(interface) = self.state_interface() else {
            return Ok(());
        };
        let Some(restore_fn) = interface.restore else {
            return Ok(());
        };

        let mut properties: Vec<RawStateProperty> = vec![];
        let mut by_key: HashMap<u32, usize> = HashMap::new();
        for prop in &state.properties {
            let key = self._urid_feature.map_uri(prop.key_uri.as_bytes());
            let type_ = self._urid_feature.map_uri(prop.type_uri.as_bytes());
            if key == 0 || type_ == 0 {
                continue;
            }
            let idx = properties.len();
            properties.push(RawStateProperty {
                key,
                type_,
                flags: prop.flags,
                value: prop.value.clone(),
            });
            by_key.insert(key, idx);
        }
        let mut ctx = StateRestoreContext { properties, by_key };
        let features = self.state_feature_ptrs();

        let status = unsafe {
            restore_fn(
                self.instance_handle(),
                Some(lv2_state_retrieve_callback),
                (&mut ctx as *mut StateRestoreContext).cast::<c_void>(),
                0,
                features.as_ptr(),
            )
        };
        if status == LV2_STATE_STATUS_SUCCESS {
            Ok(())
        } else {
            Err(format!(
                "LV2 state restore failed for '{}': status {}",
                self.uri, status
            ))
        }
    }

    fn state_interface(&self) -> Option<&Lv2StateInterface> {
        let instance = self.instance.as_ref()?;
        let ptr = unsafe {
            instance
                .instance()
                .extension_data::<Lv2StateInterface>(LV2_STATE_INTERFACE_URI)?
        };
        Some(unsafe { ptr.as_ref() })
    }

    fn worker_interface(&self) -> Option<&Lv2WorkerInterface> {
        let instance = self.instance.as_ref()?;
        let ptr = unsafe {
            instance
                .instance()
                .extension_data::<Lv2WorkerInterface>(LV2_WORKER__INTERFACE)?
        };
        Some(unsafe { ptr.as_ref() })
    }

    fn instance_handle(&self) -> Lv2Handle {
        self.instance
            .as_ref()
            .map(|i| i.instance().handle() as Lv2Handle)
            .unwrap_or(std::ptr::null_mut())
    }

    pub fn instance_access_handle(&self) -> Option<usize> {
        let handle = self.instance_handle();
        if handle.is_null() {
            None
        } else {
            Some(handle as usize)
        }
    }

    fn state_feature_ptrs(&self) -> [*const LV2Feature; 6] {
        let sp = self._state_path_feature.feature_ptrs();
        [
            self._urid_feature.map_feature() as *const LV2Feature,
            self._urid_feature.unmap_feature() as *const LV2Feature,
            sp[0],
            sp[1],
            sp[2],
            std::ptr::null(),
        ]
    }

    fn control_port_values(&self) -> Vec<Lv2StatePortValue> {
        let Ok(values) = self.scalar_values.lock() else {
            return vec![];
        };
        self.control_ports
            .iter()
            .filter_map(|port| {
                values.get(port.index as usize).map(|v| Lv2StatePortValue {
                    index: port.index,
                    value: *v,
                })
            })
            .collect()
    }

    fn set_control_port_values(&mut self, port_values: &[Lv2StatePortValue]) {
        let Ok(mut values) = self.scalar_values.lock() else {
            return;
        };
        for port in port_values {
            if let Some(slot) = values.get_mut(port.index as usize) {
                *slot = port.value;
            }
        }
    }

    pub fn set_state_base_dir(&mut self, base_dir: PathBuf) {
        self._state_path_feature.set_base_dir(base_dir);
    }

    pub fn control_ports_with_values(&self) -> Vec<Lv2ControlPortInfo> {
        let Ok(values) = self.scalar_values.lock() else {
            return Vec::new();
        };
        self.control_ports
            .iter()
            .map(|port| Lv2ControlPortInfo {
                index: port.index,
                name: port.name.clone(),
                min: port.min,
                max: port.max,
                value: values.get(port.index as usize).copied().unwrap_or(0.0),
            })
            .collect()
    }

    pub fn set_control_value(&mut self, index: u32, value: f32) -> Result<(), String> {
        let Some(port) = self.control_ports.iter().find(|port| port.index == index) else {
            return Err(format!("Unknown LV2 control port index: {index}"));
        };
        let clamped = value.clamp(port.min, port.max);
        let Ok(mut values) = self.scalar_values.lock() else {
            return Err("Failed to lock LV2 control values".to_string());
        };
        let Some(slot) = values.get_mut(index as usize) else {
            return Err(format!("LV2 control port index out of range: {index}"));
        };
        *slot = clamped;
        Ok(())
    }

    fn write_midi_input_events(&mut self, port: usize, events: &[MidiEvent]) {
        let Some(buffer) = self.atom_inputs.get_mut(port) else {
            return;
        };
        let bytes = buffer.bytes_mut();
        if bytes.len() < std::mem::size_of::<LV2AtomSequence>() {
            return;
        }
        let seq = bytes.as_mut_ptr() as *mut LV2AtomSequence;
        let capacity = bytes
            .len()
            .saturating_sub(std::mem::size_of::<lv2_raw::LV2Atom>()) as u32;
        for event in events {
            if event.data.is_empty() {
                continue;
            }
            let mut raw =
                vec![0_u8; std::mem::size_of::<lv2_raw::LV2AtomEvent>() + event.data.len()];
            let raw_event = raw.as_mut_ptr() as *mut lv2_raw::LV2AtomEvent;
            unsafe {
                (*raw_event).time_in_frames = event.frame as i64;
                (*raw_event).body.mytype = self.midi_event_urid;
                (*raw_event).body.size = event.data.len() as u32;
                let data_ptr =
                    (raw_event as *mut u8).add(std::mem::size_of::<lv2_raw::LV2AtomEvent>());
                std::ptr::copy_nonoverlapping(event.data.as_ptr(), data_ptr, event.data.len());
                if lv2_atom_sequence_append_event(seq, capacity, raw_event).is_null() {
                    break;
                }
            }
        }
    }

    fn write_transport_event(&mut self, port: usize, transport: Lv2TransportInfo) {
        let Some(buffer) = self.atom_inputs.get_mut(port) else {
            return;
        };
        let bytes = buffer.bytes_mut();
        if bytes.len() < std::mem::size_of::<LV2AtomSequence>() {
            return;
        }
        let seq = bytes.as_mut_ptr() as *mut LV2AtomSequence;
        let capacity = bytes
            .len()
            .saturating_sub(std::mem::size_of::<lv2_raw::LV2Atom>()) as u32;

        let beats_per_bar = if transport.tsig_num == 0 {
            4.0
        } else {
            transport.tsig_num as f64
        };
        let beat_unit = if transport.tsig_denom == 0 {
            4_i64
        } else {
            transport.tsig_denom as i64
        };
        let bpm = if transport.bpm > 0.0 {
            transport.bpm
        } else {
            120.0
        };
        let speed = if transport.playing { 1.0 } else { 0.0 };
        let sample = transport.transport_sample as i64;
        let seconds = (transport.transport_sample as f64) / self.sample_rate.max(1.0);
        let absolute_beats = seconds * bpm / 60.0;
        let bar = (absolute_beats / beats_per_bar).floor().max(0.0) as i64;
        let bar_beat = absolute_beats - (bar as f64 * beats_per_bar);

        let mut payload =
            Vec::<u8>::with_capacity(std::mem::size_of::<LV2AtomObjectBody>() + (7 * 32));
        let object_body = LV2AtomObjectBody {
            id: 0,
            otype: self.time_position_urid,
        };
        let object_body_bytes = unsafe {
            std::slice::from_raw_parts(
                (&object_body as *const LV2AtomObjectBody).cast::<u8>(),
                std::mem::size_of::<LV2AtomObjectBody>(),
            )
        };
        payload.extend_from_slice(object_body_bytes);

        append_object_long_property(
            &mut payload,
            self.time_frame_urid,
            self.atom_long_urid,
            sample,
        );
        append_object_double_property(
            &mut payload,
            self.time_speed_urid,
            self.atom_double_urid,
            speed,
        );
        append_object_double_property(&mut payload, self.time_bpm_urid, self.atom_double_urid, bpm);
        append_object_long_property(&mut payload, self.time_bar_urid, self.atom_long_urid, bar);
        append_object_double_property(
            &mut payload,
            self.time_bar_beat_urid,
            self.atom_double_urid,
            bar_beat,
        );
        append_object_double_property(
            &mut payload,
            self.time_beats_per_bar_urid,
            self.atom_double_urid,
            beats_per_bar,
        );
        append_object_long_property(
            &mut payload,
            self.time_beat_unit_urid,
            self.atom_long_urid,
            beat_unit,
        );

        if payload.len() > (u32::MAX as usize) {
            return;
        }

        let mut raw = vec![0_u8; std::mem::size_of::<LV2AtomEvent>() + payload.len()];
        let raw_event = raw.as_mut_ptr() as *mut LV2AtomEvent;
        unsafe {
            (*raw_event).time_in_frames = 0;
            (*raw_event).body.mytype = self.atom_object_urid;
            (*raw_event).body.size = payload.len() as u32;
            let data_ptr = (raw_event as *mut u8).add(std::mem::size_of::<LV2AtomEvent>());
            std::ptr::copy_nonoverlapping(payload.as_ptr(), data_ptr, payload.len());
            let _ = lv2_atom_sequence_append_event(seq, capacity, raw_event);
        }
    }

    fn read_midi_output_events(&mut self, port: usize) -> Vec<MidiEvent> {
        let Some(buffer) = self.atom_outputs.get_mut(port) else {
            return vec![];
        };
        let bytes = buffer.bytes_mut();
        if bytes.len() < std::mem::size_of::<LV2AtomSequence>() {
            return vec![];
        }

        let mut result = Vec::new();
        let seq = bytes.as_mut_ptr() as *mut LV2AtomSequence;
        unsafe {
            let body = &(*seq).body as *const LV2AtomSequenceBody;
            let size = (*seq).atom.size;
            let mut it = lv2_atom_sequence_begin(body);
            while !lv2_atom_sequence_is_end(body, size, it) {
                let event = &*it;
                if event.body.mytype == self.midi_event_urid && event.body.size > 0 {
                    let data_ptr =
                        (it as *const u8).add(std::mem::size_of::<lv2_raw::LV2AtomEvent>());
                    let data_len = event.body.size as usize;
                    let data = std::slice::from_raw_parts(data_ptr, data_len).to_vec();
                    result.push(MidiEvent::new(event.time_in_frames.max(0) as u32, data));
                }
                it = lv2_atom_sequence_next(it);
            }
        }
        result
    }
}

impl Drop for Lv2Processor {
    fn drop(&mut self) {
        let Some(instance) = self.instance.take() else {
            return;
        };
        drop(instance);
    }
}

impl Lv2Host {
    pub fn new(sample_rate: f64) -> Self {
        let world = World::new();
        world.load_all();
        Self {
            world,
            sample_rate,
            loaded_plugins: HashMap::new(),
        }
    }

    pub fn list_plugins(&self) -> Vec<Lv2PluginInfo> {
        let input_port = self.world.new_uri("http://lv2plug.in/ns/lv2core#InputPort");
        let output_port = self
            .world
            .new_uri("http://lv2plug.in/ns/lv2core#OutputPort");
        let audio_port = self.world.new_uri("http://lv2plug.in/ns/lv2core#AudioPort");
        let atom_port = self.world.new_uri("http://lv2plug.in/ns/ext/atom#AtomPort");
        let event_port = self
            .world
            .new_uri("http://lv2plug.in/ns/ext/event#EventPort");
        let midi_event = self
            .world
            .new_uri("http://lv2plug.in/ns/ext/midi#MidiEvent");

        let mut plugins = self
            .world
            .plugins()
            .iter()
            .filter(|plugin| plugin.verify())
            .filter_map(|plugin| {
                let uri = plugin.uri().as_uri()?.to_string();
                let name = plugin.name().as_str().unwrap_or(&uri).to_string();
                let class_label = plugin
                    .class()
                    .label()
                    .as_str()
                    .unwrap_or("Unknown")
                    .to_string();
                let bundle_uri = plugin.bundle_uri().as_uri().unwrap_or("").to_string();
                let required_features = plugin_feature_uris(&plugin);
                let (audio_inputs, audio_outputs, midi_inputs, midi_outputs) = plugin_port_counts(
                    &plugin,
                    &input_port,
                    &output_port,
                    &audio_port,
                    &atom_port,
                    &event_port,
                    &midi_event,
                );

                Some(Lv2PluginInfo {
                    uri,
                    name,
                    class_label,
                    bundle_uri,
                    required_features,
                    audio_inputs,
                    audio_outputs,
                    midi_inputs,
                    midi_outputs,
                })
            })
            .collect::<Vec<_>>();

        plugins.sort_by(|left, right| left.name.cmp(&right.name));
        plugins
    }

    pub fn load_plugin(&mut self, uri: &str) -> Result<(), String> {
        if self.loaded_plugins.contains_key(uri) {
            return Err(format!("Plugin is already loaded: {uri}"));
        }

        let plugin = self
            .plugin_by_uri(uri)
            .ok_or_else(|| format!("Plugin not found for URI: {uri}"))?;

        let mut urid_feature = UridMapFeature::new()?;
        let mut state_path_feature = StatePathFeature::new(default_state_base_dir());
        let (instance, instantiate_features) = instantiate_plugin(
            &plugin,
            self.sample_rate,
            uri,
            &mut urid_feature,
            &mut state_path_feature,
        )?;
        let active_instance = unsafe { instance.activate() };
        self.loaded_plugins.insert(
            uri.to_string(),
            LoadedPlugin {
                instance: active_instance,
                _urid_feature: urid_feature,
                _state_path_feature: state_path_feature,
                _instantiate_features: instantiate_features,
            },
        );
        Ok(())
    }

    pub fn unload_plugin(&mut self, uri: &str) -> Result<(), String> {
        let loaded_plugin = self
            .loaded_plugins
            .remove(uri)
            .ok_or_else(|| format!("Plugin is not currently loaded: {uri}"))?;
        let _ = unsafe { loaded_plugin.instance.deactivate() };
        Ok(())
    }

    pub fn unload_all(&mut self) {
        let uris = self.loaded_plugins();
        for uri in uris {
            let _ = self.unload_plugin(&uri);
        }
    }

    pub fn loaded_plugins(&self) -> Vec<String> {
        let mut uris = self.loaded_plugins.keys().cloned().collect::<Vec<_>>();
        uris.sort();
        uris
    }

    pub fn loaded_count(&self) -> usize {
        self.loaded_plugins.len()
    }

    fn plugin_by_uri(&self, uri: &str) -> Option<Plugin> {
        let uri_node = self.world.new_uri(uri);
        self.world.plugins().plugin(&uri_node)
    }
}

impl Drop for Lv2Host {
    fn drop(&mut self) {
        self.unload_all();
    }
}

fn plugin_feature_uris(plugin: &Plugin) -> Vec<String> {
    plugin
        .required_features()
        .iter()
        .filter_map(|feature| {
            feature
                .as_uri()
                .map(str::to_string)
                .or_else(|| feature.as_str().map(str::to_string))
        })
        .collect()
}

fn instantiate_plugin(
    plugin: &Plugin,
    sample_rate: f64,
    uri: &str,
    urid_feature: &mut UridMapFeature,
    state_path_feature: &mut StatePathFeature,
) -> Result<(lilv::instance::Instance, InstantiateFeatureSet), String> {
    let required_features = plugin_feature_uris(plugin);
    let feature_set =
        build_instantiate_features(&required_features, urid_feature, state_path_feature)?;
    let feature_refs: Vec<&LV2Feature> = feature_set.features.iter().collect();
    let instance = unsafe { plugin.instantiate(sample_rate, feature_refs) }.ok_or_else(|| {
        if required_features.is_empty() {
            format!(
                "Failed to instantiate '{uri}'. It likely requires LV2 host features that are not wired yet."
            )
        } else {
            format!(
                "Failed to instantiate '{uri}'. Required features: {}",
                required_features.join(", ")
            )
        }
    })?;
    Ok((instance, feature_set))
}

fn build_instantiate_features(
    required_features: &[String],
    urid_feature: &UridMapFeature,
    state_path_feature: &StatePathFeature,
) -> Result<InstantiateFeatureSet, String> {
    let mut seen = HashSet::<String>::new();
    let mut feature_uris = Vec::<CString>::new();
    let mut features = Vec::<LV2Feature>::new();

    let mut push_feature =
        |uri: &str, data: *mut c_void, allow_duplicate: bool| -> Result<(), String> {
            if !allow_duplicate && !seen.insert(uri.to_string()) {
                return Ok(());
            }
            let c_uri = CString::new(uri)
                .map_err(|e| format!("Invalid LV2 feature URI '{uri}' for instantiate: {e}"))?;
            let feature = LV2Feature {
                uri: c_uri.as_ptr(),
                data,
            };
            feature_uris.push(c_uri);
            features.push(feature);
            Ok(())
        };

    push_feature(LV2_URID__MAP, urid_feature.map_feature().data, false)?;
    push_feature(LV2_URID__UNMAP, urid_feature.unmap_feature().data, false)?;
    let worker_feature = WorkerFeature::new()?;
    push_feature(LV2_WORKER__SCHEDULE, worker_feature.feature.data, false)?;

    let state_features = state_path_feature.feature_ptrs();
    for feature_ptr in state_features {
        if feature_ptr.is_null() {
            continue;
        }
        let feature = unsafe { &*feature_ptr };
        let uri = unsafe { CStr::from_ptr(feature.uri) }
            .to_str()
            .map_err(|e| format!("Invalid LV2 feature URI from state path feature: {e}"))?;
        push_feature(uri, feature.data, false)?;
    }

    let option_values = vec![1_u32, 8192_u32, 1024_u32];
    let int_type = urid_feature.map_uri(LV2_ATOM__INT);
    let min_key = urid_feature.map_uri(LV2_BUF_SIZE__MIN_BLOCK_LENGTH.as_bytes());
    let max_key = urid_feature.map_uri(LV2_BUF_SIZE__MAX_BLOCK_LENGTH.as_bytes());
    let nominal_key = urid_feature.map_uri(LV2_BUF_SIZE__NOMINAL_BLOCK_LENGTH.as_bytes());
    let mut options = vec![
        LV2OptionsOption {
            context: 0,
            subject: 0,
            key: min_key,
            size: std::mem::size_of::<u32>() as u32,
            type_: int_type,
            value: (&option_values[0] as *const u32).cast::<c_void>(),
        },
        LV2OptionsOption {
            context: 0,
            subject: 0,
            key: max_key,
            size: std::mem::size_of::<u32>() as u32,
            type_: int_type,
            value: (&option_values[1] as *const u32).cast::<c_void>(),
        },
        LV2OptionsOption {
            context: 0,
            subject: 0,
            key: nominal_key,
            size: std::mem::size_of::<u32>() as u32,
            type_: int_type,
            value: (&option_values[2] as *const u32).cast::<c_void>(),
        },
        LV2OptionsOption {
            context: 0,
            subject: 0,
            key: 0,
            size: 0,
            type_: 0,
            value: std::ptr::null(),
        },
    ];
    push_feature(
        LV2_OPTIONS__OPTIONS,
        options.as_mut_ptr().cast::<c_void>(),
        false,
    )?;

    let flag_feature_data = Box::new(0_u8);

    for required in required_features {
        let data = match required.as_str() {
            LV2_OPTIONS__OPTIONS => options.as_mut_ptr().cast::<c_void>(),
            LV2_BUF_SIZE__BOUNDED_BLOCK_LENGTH => (&*flag_feature_data as *const u8)
                .cast_mut()
                .cast::<c_void>(),
            LV2_URID__MAP_URI_TYPO_COMPAT => urid_feature.map_feature().data,
            LV2_WORKER__SCHEDULE => worker_feature.feature.data,
            _ => (&*flag_feature_data as *const u8)
                .cast_mut()
                .cast::<c_void>(),
        };
        push_feature(required, data, false)?;
    }

    Ok(InstantiateFeatureSet {
        _feature_uris: feature_uris,
        features,
        _worker_feature: worker_feature,
        _option_values: option_values,
        _options: options,
        _flag_feature_data: flag_feature_data,
    })
}

impl WorkerFeature {
    fn new() -> Result<Self, String> {
        let mut schedule = Box::new(Lv2WorkerSchedule {
            handle: std::ptr::null_mut(),
            schedule_work: Some(lv2_worker_schedule_work_callback),
        });
        let state = Box::new(WorkerScheduleState {
            jobs: UnsafeMutex::new(vec![]),
            responses: UnsafeMutex::new(vec![]),
        });
        schedule.handle = &*state as *const WorkerScheduleState as *mut c_void;
        let uri =
            CString::new(LV2_WORKER__SCHEDULE).map_err(|e| format!("Invalid worker URI: {e}"))?;
        let feature = LV2Feature {
            uri: uri.as_ptr(),
            data: (&mut *schedule as *mut Lv2WorkerSchedule).cast::<c_void>(),
        };
        Ok(Self {
            _uri: uri,
            _schedule: schedule,
            feature,
            state,
        })
    }
}

unsafe extern "C" fn lv2_worker_schedule_work_callback(
    handle: *mut c_void,
    size: u32,
    data: *const c_void,
) -> u32 {
    if handle.is_null() || (size > 0 && data.is_null()) {
        return LV2_WORKER_ERR_UNKNOWN;
    }
    let state = unsafe { &*(handle as *const WorkerScheduleState) };
    let bytes = if size == 0 {
        vec![]
    } else {
        unsafe { std::slice::from_raw_parts(data.cast::<u8>(), size as usize).to_vec() }
    };
    state.jobs.lock().push(bytes);
    LV2_WORKER_SUCCESS
}

unsafe extern "C" fn lv2_worker_respond_callback(
    handle: *mut c_void,
    size: u32,
    data: *const c_void,
) -> u32 {
    if handle.is_null() || (size > 0 && data.is_null()) {
        return LV2_WORKER_ERR_UNKNOWN;
    }
    let state = unsafe { &*(handle as *const WorkerScheduleState) };
    let bytes = if size == 0 {
        vec![]
    } else {
        unsafe { std::slice::from_raw_parts(data.cast::<u8>(), size as usize).to_vec() }
    };
    state.responses.lock().push(bytes);
    LV2_WORKER_SUCCESS
}

fn append_object_long_property(buffer: &mut Vec<u8>, key: LV2Urid, atom_type: LV2Urid, value: i64) {
    let prop = LV2AtomPropertyBody {
        key,
        context: 0,
        value: LV2Atom {
            size: std::mem::size_of::<i64>() as u32,
            mytype: atom_type,
        },
    };
    let prop_size = std::mem::size_of::<LV2AtomPropertyBody>();
    let prop_bytes = unsafe {
        std::slice::from_raw_parts(
            (&prop as *const LV2AtomPropertyBody).cast::<u8>(),
            prop_size,
        )
    };
    buffer.extend_from_slice(prop_bytes);
    let atom = LV2AtomLong {
        atom: LV2Atom {
            size: std::mem::size_of::<i64>() as u32,
            mytype: atom_type,
        },
        body: value,
    };
    let value_bytes = unsafe {
        std::slice::from_raw_parts(
            (&atom.body as *const i64).cast::<u8>(),
            std::mem::size_of::<i64>(),
        )
    };
    buffer.extend_from_slice(value_bytes);
    let written = (prop_size + std::mem::size_of::<i64>()) as u32;
    let padded = lv2_atom_pad_size(written) as usize;
    if padded > (prop_size + std::mem::size_of::<i64>()) {
        buffer.resize(
            buffer.len() + (padded - prop_size - std::mem::size_of::<i64>()),
            0,
        );
    }
}

fn append_object_double_property(
    buffer: &mut Vec<u8>,
    key: LV2Urid,
    atom_type: LV2Urid,
    value: f64,
) {
    let prop = LV2AtomPropertyBody {
        key,
        context: 0,
        value: LV2Atom {
            size: std::mem::size_of::<f64>() as u32,
            mytype: atom_type,
        },
    };
    let prop_size = std::mem::size_of::<LV2AtomPropertyBody>();
    let prop_bytes = unsafe {
        std::slice::from_raw_parts(
            (&prop as *const LV2AtomPropertyBody).cast::<u8>(),
            prop_size,
        )
    };
    buffer.extend_from_slice(prop_bytes);
    let atom = LV2AtomDouble {
        atom: LV2Atom {
            size: std::mem::size_of::<f64>() as u32,
            mytype: atom_type,
        },
        body: value,
    };
    let value_bytes = unsafe {
        std::slice::from_raw_parts(
            (&atom.body as *const f64).cast::<u8>(),
            std::mem::size_of::<f64>(),
        )
    };
    buffer.extend_from_slice(value_bytes);
    let written = (prop_size + std::mem::size_of::<f64>()) as u32;
    let padded = lv2_atom_pad_size(written) as usize;
    if padded > (prop_size + std::mem::size_of::<f64>()) {
        buffer.resize(
            buffer.len() + (padded - prop_size - std::mem::size_of::<f64>()),
            0,
        );
    }
}

impl UridMapFeature {
    fn new() -> Result<Self, String> {
        let mut map = Box::new(LV2UridMap {
            handle: std::ptr::null_mut(),
            map: urid_map_callback,
        });
        let mut unmap = Box::new(LV2UridUnmap {
            handle: std::ptr::null_mut(),
            unmap: urid_unmap_callback,
        });
        let state = Box::new(Mutex::new(UridMapState {
            next_urid: 1,
            by_uri: HashMap::new(),
            by_urid: HashMap::new(),
        }));
        map.handle = (&*state as *const Mutex<UridMapState>) as *mut c_void;
        unmap.handle = (&*state as *const Mutex<UridMapState>) as *mut c_void;

        let map_uri =
            CString::new(LV2_URID__MAP).map_err(|e| format!("Invalid URID feature URI: {e}"))?;
        let map_feature = LV2Feature {
            uri: map_uri.as_ptr(),
            data: (&mut *map as *mut LV2UridMap).cast::<c_void>(),
        };
        let unmap_uri =
            CString::new(LV2_URID__UNMAP).map_err(|e| format!("Invalid URID feature URI: {e}"))?;
        let unmap_feature = LV2Feature {
            uri: unmap_uri.as_ptr(),
            data: (&mut *unmap as *mut LV2UridUnmap).cast::<c_void>(),
        };

        Ok(Self {
            _map_uri: map_uri,
            _unmap_uri: unmap_uri,
            map_feature,
            unmap_feature,
            _map: map,
            _unmap: unmap,
            _state: state,
        })
    }

    fn map_feature(&self) -> &LV2Feature {
        &self.map_feature
    }

    fn unmap_feature(&self) -> &LV2Feature {
        &self.unmap_feature
    }

    fn map_uri(&self, uri: &[u8]) -> LV2Urid {
        let Ok(uri_str) = std::str::from_utf8(uri) else {
            return 0;
        };
        let uri_str = uri_str.trim_end_matches('\0');
        let Ok(mut state) = self._state.lock() else {
            return 0;
        };
        if let Some(existing) = state.by_uri.get(uri_str).copied() {
            return existing;
        }
        let mapped = state.next_urid;
        state.next_urid = state.next_urid.saturating_add(1);
        state.by_uri.insert(uri_str.to_string(), mapped);
        if let Ok(uri_c) = CString::new(uri_str) {
            state.by_urid.insert(mapped, uri_c);
        }
        mapped
    }

    fn unmap_urid(&self, urid: LV2Urid) -> Option<String> {
        let Ok(state) = self._state.lock() else {
            return None;
        };
        state
            .by_urid
            .get(&urid)
            .and_then(|uri| uri.to_str().ok().map(str::to_string))
    }
}

fn default_state_base_dir() -> PathBuf {
    std::env::temp_dir().join("maolan-lv2-state")
}

impl StatePathFeature {
    fn new(base_dir: PathBuf) -> Self {
        let context = Box::new(Mutex::new(StatePathContext {
            base_dir,
            copy_counter: 0,
        }));
        let handle = (&*context as *const Mutex<StatePathContext>) as *mut c_void;

        let map = Box::new(Lv2StateMapPath {
            handle,
            abstract_path: Some(lv2_state_abstract_path_callback),
            absolute_path: Some(lv2_state_absolute_path_callback),
        });
        let make = Box::new(Lv2StateMakePath {
            handle,
            path: Some(lv2_state_make_path_callback),
        });
        let free = Box::new(Lv2StateFreePath {
            handle,
            free_path: Some(lv2_state_free_path_callback),
        });

        let map_uri = CString::new(LV2_STATE_MAP_PATH_URI).expect("valid LV2 state mapPath URI");
        let make_uri = CString::new(LV2_STATE_MAKE_PATH_URI).expect("valid LV2 state makePath URI");
        let free_uri = CString::new(LV2_STATE_FREE_PATH_URI).expect("valid LV2 state freePath URI");

        let map_feature = LV2Feature {
            uri: map_uri.as_ptr(),
            data: (&*map as *const Lv2StateMapPath)
                .cast_mut()
                .cast::<c_void>(),
        };
        let make_feature = LV2Feature {
            uri: make_uri.as_ptr(),
            data: (&*make as *const Lv2StateMakePath)
                .cast_mut()
                .cast::<c_void>(),
        };
        let free_feature = LV2Feature {
            uri: free_uri.as_ptr(),
            data: (&*free as *const Lv2StateFreePath)
                .cast_mut()
                .cast::<c_void>(),
        };

        let instance = Self {
            _map_uri: map_uri,
            _make_uri: make_uri,
            _free_uri: free_uri,
            _map: map,
            _make: make,
            _free: free,
            map_feature,
            make_feature,
            free_feature,
            _context: context,
        };
        instance.ensure_base_dir();
        instance
    }

    fn ensure_base_dir(&self) {
        if let Ok(ctx) = self._context.lock() {
            let _ = std::fs::create_dir_all(&ctx.base_dir);
        }
    }

    fn set_base_dir(&self, base_dir: PathBuf) {
        if let Ok(mut ctx) = self._context.lock() {
            ctx.base_dir = base_dir;
            let _ = std::fs::create_dir_all(&ctx.base_dir);
        }
    }

    fn feature_ptrs(&self) -> [*const LV2Feature; 3] {
        [
            &self.map_feature as *const LV2Feature,
            &self.make_feature as *const LV2Feature,
            &self.free_feature as *const LV2Feature,
        ]
    }
}

extern "C" fn lv2_state_free_path_callback(_handle: *mut c_void, path: *mut c_char) {
    if path.is_null() {
        return;
    }
    unsafe {
        let _ = CString::from_raw(path);
    }
}

fn state_ctx_from_handle(handle: *mut c_void) -> Option<&'static Mutex<StatePathContext>> {
    if handle.is_null() {
        return None;
    }
    Some(unsafe { &*(handle as *const Mutex<StatePathContext>) })
}

fn copy_into_state_assets(ctx: &mut StatePathContext, src: &Path) -> Option<String> {
    let file_name = src.file_name()?.to_str()?.to_string();
    let assets_dir = ctx.base_dir.join("assets");
    let _ = std::fs::create_dir_all(&assets_dir);
    ctx.copy_counter = ctx.copy_counter.saturating_add(1);
    let dst_name = format!("{}-{}", ctx.copy_counter, file_name);
    let dst = assets_dir.join(&dst_name);
    std::fs::copy(src, &dst).ok()?;
    Some(format!("assets/{dst_name}"))
}

extern "C" fn lv2_state_abstract_path_callback(
    handle: *mut c_void,
    absolute_path: *const c_char,
) -> *mut c_char {
    let Some(ctx_lock) = state_ctx_from_handle(handle) else {
        return std::ptr::null_mut();
    };
    if absolute_path.is_null() {
        return std::ptr::null_mut();
    }
    let Some(path_str) = (unsafe { CStr::from_ptr(absolute_path) }).to_str().ok() else {
        return std::ptr::null_mut();
    };
    let path = PathBuf::from(path_str);
    let mut mapped = None;
    if let Ok(mut ctx) = ctx_lock.lock() {
        if let Ok(rel) = path.strip_prefix(&ctx.base_dir) {
            mapped = Some(rel.to_string_lossy().to_string());
        } else if path.exists() {
            mapped = copy_into_state_assets(&mut ctx, &path);
        }
    }
    let out = mapped.unwrap_or_else(|| path_str.to_string());
    CString::new(out)
        .ok()
        .map(CString::into_raw)
        .unwrap_or(std::ptr::null_mut())
}

extern "C" fn lv2_state_absolute_path_callback(
    handle: *mut c_void,
    abstract_path: *const c_char,
) -> *mut c_char {
    let Some(ctx_lock) = state_ctx_from_handle(handle) else {
        return std::ptr::null_mut();
    };
    if abstract_path.is_null() {
        return std::ptr::null_mut();
    }
    let Some(path_str) = (unsafe { CStr::from_ptr(abstract_path) }).to_str().ok() else {
        return std::ptr::null_mut();
    };
    let output = if Path::new(path_str).is_absolute() {
        path_str.to_string()
    } else if let Ok(ctx) = ctx_lock.lock() {
        ctx.base_dir.join(path_str).to_string_lossy().to_string()
    } else {
        path_str.to_string()
    };
    CString::new(output)
        .ok()
        .map(CString::into_raw)
        .unwrap_or(std::ptr::null_mut())
}

extern "C" fn lv2_state_make_path_callback(
    handle: *mut c_void,
    requested: *const c_char,
) -> *mut c_char {
    let Some(ctx_lock) = state_ctx_from_handle(handle) else {
        return std::ptr::null_mut();
    };

    let requested_name = if requested.is_null() {
        "state.bin".to_string()
    } else {
        (unsafe { CStr::from_ptr(requested) })
            .to_str()
            .ok()
            .filter(|s| !s.is_empty())
            .map(|s| s.replace("..", "_"))
            .unwrap_or_else(|| "state.bin".to_string())
    };

    let output = if let Ok(mut ctx) = ctx_lock.lock() {
        ctx.copy_counter = ctx.copy_counter.saturating_add(1);
        let file_name = format!("generated-{}-{}", ctx.copy_counter, requested_name);
        let path = ctx.base_dir.join("generated").join(file_name);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        path.to_string_lossy().to_string()
    } else {
        requested_name
    };

    CString::new(output)
        .ok()
        .map(CString::into_raw)
        .unwrap_or(std::ptr::null_mut())
}

extern "C" fn lv2_state_store_callback(
    handle: Lv2StateHandle,
    key: u32,
    value: *const c_void,
    size: usize,
    type_: u32,
    flags: u32,
) -> Lv2StateStatus {
    if handle.is_null() || value.is_null() || size == 0 {
        return LV2_STATE_STATUS_ERR_NO_PROPERTY;
    }
    let ctx = unsafe { &mut *(handle as *mut StateSaveContext) };
    let bytes = unsafe { std::slice::from_raw_parts(value.cast::<u8>(), size) };
    ctx.properties.push(RawStateProperty {
        key,
        type_,
        flags,
        value: bytes.to_vec(),
    });
    LV2_STATE_STATUS_SUCCESS
}

extern "C" fn lv2_state_retrieve_callback(
    handle: Lv2StateHandle,
    key: u32,
    size: *mut usize,
    type_: *mut u32,
    flags: *mut u32,
) -> *const c_void {
    if handle.is_null() {
        return std::ptr::null();
    }
    let ctx = unsafe { &mut *(handle as *mut StateRestoreContext) };
    let Some(idx) = ctx.by_key.get(&key).copied() else {
        return std::ptr::null();
    };
    let Some(prop) = ctx.properties.get(idx) else {
        return std::ptr::null();
    };
    if !size.is_null() {
        unsafe {
            *size = prop.value.len();
        }
    }
    if !type_.is_null() {
        unsafe {
            *type_ = prop.type_;
        }
    }
    if !flags.is_null() {
        unsafe {
            *flags = prop.flags;
        }
    }
    prop.value.as_ptr().cast::<c_void>()
}

fn prepare_empty_atom_sequence(
    buffer: &mut [u8],
    sequence_urid: LV2Urid,
    frame_time_urid: LV2Urid,
) {
    buffer.fill(0);
    if buffer.len() < std::mem::size_of::<LV2AtomSequence>() {
        return;
    }
    let seq = buffer.as_mut_ptr() as *mut LV2AtomSequence;
    unsafe {
        (*seq).atom.mytype = sequence_urid;
        (*seq).atom.size = std::mem::size_of::<LV2AtomSequenceBody>() as u32;
        (*seq).body.unit = frame_time_urid;
        (*seq).body.pad = 0;
    }
}

fn prepare_output_atom_sequence(
    buffer: &mut [u8],
    sequence_urid: LV2Urid,
    frame_time_urid: LV2Urid,
) {
    buffer.fill(0);
    if buffer.len() < std::mem::size_of::<LV2AtomSequence>() {
        return;
    }
    let seq = buffer.as_mut_ptr() as *mut LV2AtomSequence;
    let body_capacity = buffer
        .len()
        .saturating_sub(std::mem::size_of::<lv2_raw::LV2Atom>()) as u32;
    unsafe {
        (*seq).atom.mytype = sequence_urid;
        (*seq).atom.size = body_capacity;
        (*seq).body.unit = frame_time_urid;
        (*seq).body.pad = 0;
    }
}

extern "C" fn urid_map_callback(handle: LV2UridMapHandle, uri: *const c_char) -> LV2Urid {
    if handle.is_null() || uri.is_null() {
        return 0;
    }
    let Some(uri_str) = unsafe { CStr::from_ptr(uri) }.to_str().ok() else {
        return 0;
    };

    let state_mutex = unsafe { &*(handle as *const Mutex<UridMapState>) };
    let Ok(mut state) = state_mutex.lock() else {
        return 0;
    };

    if let Some(existing) = state.by_uri.get(uri_str).copied() {
        return existing;
    }

    let mapped = state.next_urid;
    state.next_urid = state.next_urid.saturating_add(1);
    state.by_uri.insert(uri_str.to_string(), mapped);
    if let Ok(uri_c) = CString::new(uri_str) {
        state.by_urid.insert(mapped, uri_c);
    }
    mapped
}

extern "C" fn urid_unmap_callback(handle: LV2UridMapHandle, urid: LV2Urid) -> *const c_char {
    if handle.is_null() || urid == 0 {
        return std::ptr::null();
    }
    let state_mutex = unsafe { &*(handle as *const Mutex<UridMapState>) };
    let Ok(state) = state_mutex.lock() else {
        return std::ptr::null();
    };
    state
        .by_urid
        .get(&urid)
        .map(|uri| uri.as_ptr())
        .unwrap_or(std::ptr::null())
}

fn plugin_port_counts(
    plugin: &Plugin,
    input_port: &Node,
    output_port: &Node,
    audio_port: &Node,
    atom_port: &Node,
    event_port: &Node,
    midi_event: &Node,
) -> (usize, usize, usize, usize) {
    let mut audio_inputs = 0;
    let mut audio_outputs = 0;
    let mut midi_inputs = 0;
    let mut midi_outputs = 0;

    for port in plugin.iter_ports() {
        let is_input = port.is_a(input_port);
        let is_output = port.is_a(output_port);

        if port.is_a(audio_port) {
            if is_input {
                audio_inputs += 1;
            }
            if is_output {
                audio_outputs += 1;
            }
        }

        let is_event_or_atom = port.is_a(atom_port) || port.is_a(event_port);
        let is_midi = is_event_or_atom && port.supports_event(midi_event);
        if is_midi {
            if is_input {
                midi_inputs += 1;
            }
            if is_output {
                midi_outputs += 1;
            }
        }
    }

    (audio_inputs, audio_outputs, midi_inputs, midi_outputs)
}
