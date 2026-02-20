use std::{
    collections::{HashMap, HashSet},
    ffi::{CStr, CString, c_char, c_uint, c_ulong, c_void},
    fmt,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread,
    time::Duration,
};

use crate::audio::io::AudioIO;
use crate::message::{Lv2PluginState, Lv2StatePortValue, Lv2StateProperty};
use crate::midi::io::MidiEvent;
use lilv::{World, instance::ActiveInstance, node::Node, plugin::Plugin};
use lv2_raw::{
    LV2_ATOM__FRAMETIME, LV2_ATOM__SEQUENCE, LV2_URID__MAP, LV2AtomSequence, LV2AtomSequenceBody,
    LV2Feature, LV2Urid, LV2UridMap, LV2UridMapHandle, lv2_atom_sequence_append_event,
    lv2_atom_sequence_begin, lv2_atom_sequence_is_end, lv2_atom_sequence_next,
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
    instance: Option<ActiveInstance>,
    _urid_feature: UridMapFeature,
    _state_path_feature: StatePathFeature,
    port_bindings: Vec<PortBinding>,
    scalar_values: Arc<Mutex<Vec<f32>>>,
    port_symbol_to_index: Arc<HashMap<String, u32>>,
    audio_inputs: Vec<Arc<AudioIO>>,
    audio_outputs: Vec<Arc<AudioIO>>,
    atom_inputs: Vec<AtomBuffer>,
    atom_outputs: Vec<AtomBuffer>,
    atom_sequence_urid: LV2Urid,
    atom_frame_time_urid: LV2Urid,
    midi_event_urid: LV2Urid,
    midi_inputs: usize,
    midi_outputs: usize,
    skip_deactivate_on_drop: bool,
    control_ports: Vec<ControlPortInfo>,
    ui_feedback_cache: Vec<u32>,
    ui_feedback_tx: Option<Sender<UiFeedbackMessage>>,
    ui_thread: Option<thread::JoinHandle<()>>,
}

struct LoadedPlugin {
    instance: ActiveInstance,
    _urid_feature: UridMapFeature,
    _state_path_feature: StatePathFeature,
}

#[derive(Default)]
struct UridMapState {
    next_urid: LV2Urid,
    by_uri: HashMap<String, LV2Urid>,
}

struct UridMapFeature {
    _uri: CString,
    feature: LV2Feature,
    _map: Box<LV2UridMap>,
    _state: Box<Mutex<UridMapState>>,
}

unsafe impl Send for UridMapFeature {}

struct UiController {
    scalar_values: Arc<Mutex<Vec<f32>>>,
    port_symbol_to_index: Arc<HashMap<String, u32>>,
    subscribed_ports: Arc<Mutex<HashSet<u32>>>,
}

unsafe impl Send for UiController {}

enum UiFeedbackMessage {
    ScalarChanges(Vec<(u32, f32)>),
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

enum UiDispatchTarget {
    Suil(*mut SuilInstance),
    Generic(HashMap<u32, *mut c_void>),
}

struct UiDispatchState {
    target: UiDispatchTarget,
    pending: Mutex<Vec<(u32, f32)>>,
    dispatch_scheduled: AtomicBool,
    alive: AtomicBool,
    subscribed_ports: Arc<Mutex<HashSet<u32>>>,
}

unsafe impl Send for UiDispatchState {}
unsafe impl Sync for UiDispatchState {}

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

#[repr(C)]
struct LV2FeatureRaw {
    uri: *const c_char,
    data: *mut c_void,
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

#[repr(C)]
struct LV2UiResize {
    handle: *mut c_void,
    ui_resize: Option<extern "C" fn(*mut c_void, i32, i32) -> i32>,
}

#[repr(C)]
struct LV2UiIdleInterface {
    idle: Option<extern "C" fn(*mut c_void) -> i32>,
}

#[repr(C)]
struct LV2UiShowInterface {
    show: Option<extern "C" fn(*mut c_void) -> i32>,
}

#[repr(C)]
struct LV2UiHideInterface {
    hide: Option<extern "C" fn(*mut c_void) -> i32>,
}

struct UiIdleData {
    interface: *const LV2UiIdleInterface,
    handle: *mut c_void,
}

unsafe impl Send for UiIdleData {}

#[allow(non_camel_case_types)]
type SuilHost = c_void;
#[allow(non_camel_case_types)]
type SuilInstance = c_void;
type SuilController = *mut c_void;
type SuilPortWriteFunc = extern "C" fn(SuilController, u32, u32, u32, *const c_void);
type SuilPortIndexFunc = extern "C" fn(SuilController, *const c_char) -> u32;

const LV2_UI_GTK3: &str = "http://lv2plug.in/ns/extensions/ui#Gtk3UI";
const LV2_UI_GTK: &str = "http://lv2plug.in/ns/extensions/ui#GtkUI";
const LV2_UI_X11: &str = "http://lv2plug.in/ns/extensions/ui#X11UI";
const LV2_UI_QT4: &str = "http://lv2plug.in/ns/extensions/ui#Qt4UI";
const LV2_UI_QT5: &str = "http://lv2plug.in/ns/extensions/ui#Qt5UI";
const LV2_UI_QT6: &str = "http://lv2plug.in/ns/extensions/ui#Qt6UI";
const LV2_UI_EXTERNAL: &str = "http://lv2plug.in/ns/extensions/ui#external";
const LV2_EXT_UI_WIDGET: &str = "http://kxstudio.sf.net/ns/lv2ext/external-ui#Widget";
const LV2_UI_PARENT: &str = "http://lv2plug.in/ns/extensions/ui#parent";
const LV2_UI_RESIZE: &str = "http://lv2plug.in/ns/extensions/ui#resize";
const LV2_UI_IDLE_INTERFACE: &str = "http://lv2plug.in/ns/extensions/ui#idleInterface";
const LV2_UI_SHOW_INTERFACE: &str = "http://lv2plug.in/ns/extensions/ui#showInterface";
const LV2_UI_HIDE_INTERFACE: &str = "http://lv2plug.in/ns/extensions/ui#hideInterface";
const LV2_INSTANCE_ACCESS: &str = "http://lv2plug.in/ns/ext/instance-access";
const LV2_EXTERNAL_UI_HOST_KX: &str = "http://kxstudio.sf.net/ns/lv2ext/external-ui#Host";
const LV2_STATE_INTERFACE_URI: &str = "http://lv2plug.in/ns/ext/state#interface";
const LV2_STATE_MAP_PATH_URI: &str = "http://lv2plug.in/ns/ext/state#mapPath";
const LV2_STATE_MAKE_PATH_URI: &str = "http://lv2plug.in/ns/ext/state#makePath";
const LV2_STATE_FREE_PATH_URI: &str = "http://lv2plug.in/ns/ext/state#freePath";
// RTLD_NOW | RTLD_GLOBAL on FreeBSD (dlfcn.h: RTLD_NOW=2, RTLD_GLOBAL=0x100)
const RTLD_NOW_GLOBAL: i32 = 0x102;
const GTK_WINDOW_TOPLEVEL: i32 = 0;
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

/// Spec for plugins whose UI type is `LV2_UI_EXTERNAL` or `LV2_EXT_UI_WIDGET`.
/// These are instantiated directly via dlopen rather than through suil.
#[derive(Debug, Clone)]
struct ExternalUiSpec {
    binary_path: String,
    bundle_path: String,
    plugin_uri: String,
    ui_uri: String,
}

#[derive(Debug, Clone)]
struct UiSpec {
    plugin_uri: String,
    ui_uri: String,
    container_type_uri: String,
    ui_type_uri: String,
    ui_bundle_path: String,
    ui_binary_path: String,
}

/// Raw LV2 UI descriptor as defined in lv2/ui.h.
#[repr(C)]
struct LV2UiDescriptor {
    uri: *const c_char,
    instantiate: unsafe extern "C" fn(
        descriptor: *const LV2UiDescriptor,
        plugin_uri: *const c_char,
        bundle_path: *const c_char,
        write_function: SuilPortWriteFunc,
        controller: *mut c_void,
        widget: *mut *mut c_void,
        features: *const *const LV2FeatureRaw,
    ) -> *mut c_void,
    cleanup: unsafe extern "C" fn(*mut c_void),
    extension_data: unsafe extern "C" fn(*const c_char) -> *const c_void,
}

/// External UI widget vtable (kxstudio / lv2plug.in external-ui spec).
#[repr(C)]
struct LV2ExternalUiWidget {
    run: unsafe extern "C" fn(*mut LV2ExternalUiWidget),
    show: unsafe extern "C" fn(*mut LV2ExternalUiWidget),
    hide: unsafe extern "C" fn(*mut LV2ExternalUiWidget),
}

unsafe impl Send for LV2ExternalUiWidget {}

/// Host descriptor passed as the `LV2_EXTERNAL_UI_HOST_KX` feature data.
#[repr(C)]
struct LV2ExternalUiHost {
    ui_closed: Option<unsafe extern "C" fn(*mut c_void)>,
    plugin_human_id: *const c_char,
}

unsafe impl Send for LV2ExternalUiHost {}

/// Controller for external UIs: handles parameter writes and close signals.
struct ExternalUiController {
    scalar_values: Arc<Mutex<Vec<f32>>>,
    closed: AtomicBool,
}

unsafe impl Send for ExternalUiController {}

type LV2UiDescriptorFn = unsafe extern "C" fn(u32) -> *const LV2UiDescriptor;

#[link(name = "suil-0")]
unsafe extern "C" {
    fn suil_host_new(
        write_func: Option<SuilPortWriteFunc>,
        index_func: Option<SuilPortIndexFunc>,
        subscribe_func: Option<
            extern "C" fn(SuilController, u32, u32, *const *const LV2FeatureRaw) -> u32,
        >,
        unsubscribe_func: Option<
            extern "C" fn(SuilController, u32, u32, *const *const LV2FeatureRaw) -> u32,
        >,
    ) -> *mut SuilHost;
    fn suil_host_free(host: *mut SuilHost);
    fn suil_ui_supported(host_type_uri: *const c_char, ui_type_uri: *const c_char) -> u32;
    fn suil_instance_new(
        host: *mut SuilHost,
        controller: SuilController,
        container_type_uri: *const c_char,
        plugin_uri: *const c_char,
        ui_uri: *const c_char,
        ui_type_uri: *const c_char,
        ui_bundle_path: *const c_char,
        ui_binary_path: *const c_char,
        features: *const *const LV2FeatureRaw,
    ) -> *mut SuilInstance;
    fn suil_instance_free(instance: *mut SuilInstance);
    fn suil_instance_get_widget(instance: *mut SuilInstance) -> *mut c_void;
    fn suil_instance_get_handle(instance: *mut SuilInstance) -> *mut c_void;
    fn suil_instance_extension_data(
        instance: *mut SuilInstance,
        uri: *const c_char,
    ) -> *const c_void;
    fn suil_instance_port_event(
        instance: *mut SuilInstance,
        port_index: u32,
        buffer_size: u32,
        protocol: u32,
        buffer: *const c_void,
    );
}

#[link(name = "gtk-x11-2.0")]
unsafe extern "C" {
    fn gtk_init_check(argc: *mut i32, argv: *mut *mut *mut c_char) -> i32;
    fn gtk_window_new(window_type: i32) -> *mut c_void;
    fn gtk_window_set_title(window: *mut c_void, title: *const c_char);
    fn gtk_window_set_default_size(window: *mut c_void, width: i32, height: i32);
    fn gtk_window_resize(window: *mut c_void, width: i32, height: i32);
    fn gtk_container_add(container: *mut c_void, widget: *mut c_void);
    fn gtk_vbox_new(homogeneous: i32, spacing: i32) -> *mut c_void;
    fn gtk_hbox_new(homogeneous: i32, spacing: i32) -> *mut c_void;
    fn gtk_label_new(text: *const c_char) -> *mut c_void;
    fn gtk_hscale_new_with_range(min: f64, max: f64, step: f64) -> *mut c_void;
    fn gtk_range_set_value(range: *mut c_void, value: f64);
    fn gtk_range_get_value(range: *mut c_void) -> f64;
    fn gtk_widget_set_size_request(widget: *mut c_void, width: i32, height: i32);
    fn gtk_box_pack_start(
        boxed: *mut c_void,
        child: *mut c_void,
        expand: i32,
        fill: i32,
        padding: u32,
    );
    fn gtk_widget_show_all(widget: *mut c_void);
    fn gtk_widget_realize(widget: *mut c_void);
    fn gtk_socket_new() -> *mut c_void;
    fn gtk_socket_get_id(socket: *mut c_void) -> c_ulong;
    fn gtk_main();
    fn gtk_main_quit();
}

#[link(name = "gobject-2.0")]
unsafe extern "C" {
    fn g_signal_connect_data(
        instance: *mut c_void,
        detailed_signal: *const c_char,
        c_handler: Option<unsafe extern "C" fn(*mut c_void, *mut c_void)>,
        data: *mut c_void,
        destroy_data: Option<unsafe extern "C" fn(*mut c_void, *mut c_void)>,
        connect_flags: u32,
    ) -> u64;
}

#[link(name = "glib-2.0")]
unsafe extern "C" {
    fn g_idle_add(function: Option<extern "C" fn(*mut c_void) -> i32>, data: *mut c_void)
    -> c_uint;
    fn g_timeout_add(
        interval: c_uint,
        function: Option<extern "C" fn(*mut c_void) -> i32>,
        data: *mut c_void,
    ) -> c_uint;
    fn g_source_remove(tag: c_uint) -> i32;
}

// dlopen/dlsym/dlclose are in FreeBSD's libc (no separate -ldl needed).
unsafe extern "C" {
    fn dlopen(filename: *const c_char, flags: i32) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> i32;
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
        let instance = instantiate_plugin(
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
        let mut port_symbol_to_index = HashMap::<String, u32>::new();
        let mut audio_inputs: Vec<Arc<AudioIO>> = vec![];
        let mut audio_outputs: Vec<Arc<AudioIO>> = vec![];
        let mut atom_inputs: Vec<AtomBuffer> = vec![];
        let mut atom_outputs: Vec<AtomBuffer> = vec![];
        let mut midi_inputs = 0_usize;
        let mut midi_outputs = 0_usize;
        let mut control_ports = vec![];
        let atom_sequence_urid = urid_feature.map_uri(LV2_ATOM__SEQUENCE);
        let atom_frame_time_urid = urid_feature.map_uri(LV2_ATOM__FRAMETIME);
        let midi_event_urid = urid_feature.map_uri(lv2_raw::LV2_MIDI__MIDIEVENT);
        let mut has_atom_ports = false;

        for port in plugin.iter_ports() {
            let index = port.index();
            if let Some(symbol) = port.symbol().and_then(|n| n.as_str().map(str::to_string)) {
                port_symbol_to_index.insert(symbol, index as u32);
            }
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
                has_atom_ports = true;
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
                has_atom_ports = true;
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
            instance: Some(active_instance),
            _urid_feature: urid_feature,
            _state_path_feature: state_path_feature,
            port_bindings,
            scalar_values: Arc::new(Mutex::new(scalar_values)),
            port_symbol_to_index: Arc::new(port_symbol_to_index),
            audio_inputs,
            audio_outputs,
            atom_inputs,
            atom_outputs,
            atom_sequence_urid,
            atom_frame_time_urid,
            midi_event_urid,
            midi_inputs,
            midi_outputs,
            skip_deactivate_on_drop: has_atom_ports,
            control_ports,
            ui_feedback_cache: vec![],
            ui_feedback_tx: None,
            ui_thread: None,
        };
        if let Ok(values) = processor.scalar_values.lock() {
            processor.ui_feedback_cache = values.iter().map(|v| v.to_bits()).collect();
        }
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
        let changes = self.collect_scalar_changes();
        if !changes.is_empty()
            && self
                .ui_feedback_tx
                .as_ref()
                .is_some_and(|tx| tx.send(UiFeedbackMessage::ScalarChanges(changes)).is_err())
        {
            self.ui_feedback_tx = None;
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
        for (port, events) in midi_inputs.iter().enumerate() {
            self.write_midi_input_events(port, events);
        }

        self.connect_ports();
        if let Some(instance) = self.instance.as_mut() {
            unsafe {
                instance.run(frames);
            }
        }

        for io in &self.audio_outputs {
            *io.finished.lock() = true;
        }
        let mut midi_outputs = vec![];
        for port in 0..self.midi_outputs {
            midi_outputs.push(self.read_midi_output_events(port));
        }
        let changes = self.collect_scalar_changes();
        if !changes.is_empty()
            && self
                .ui_feedback_tx
                .as_ref()
                .is_some_and(|tx| tx.send(UiFeedbackMessage::ScalarChanges(changes)).is_err())
        {
            self.ui_feedback_tx = None;
        }
        midi_outputs
    }

    fn collect_scalar_changes(&mut self) -> Vec<(u32, f32)> {
        let Ok(values) = self.scalar_values.lock() else {
            return vec![];
        };
        if self.ui_feedback_cache.len() != values.len() {
            self.ui_feedback_cache.resize(values.len(), 0);
        }
        let mut changes = Vec::new();
        for (idx, value) in values.iter().enumerate() {
            let bits = value.to_bits();
            if self.ui_feedback_cache[idx] != bits {
                self.ui_feedback_cache[idx] = bits;
                changes.push((idx as u32, *value));
            }
        }
        changes
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

    pub fn show_ui(&mut self) -> Result<(), String> {
        if let Some(handle) = &self.ui_thread
            && !handle.is_finished()
        {
            return Ok(());
        }

        let ui_spec = resolve_preferred_ui(&self.uri);
        // Fall back to external UI only when no suil-supported UI is found.
        let ext_spec = if ui_spec.is_err() {
            resolve_external_ui(&self.uri)
        } else {
            None
        };
        let values = Arc::clone(&self.scalar_values);
        let symbol_map = Arc::clone(&self.port_symbol_to_index);
        let control_ports = self.control_ports.clone();
        let plugin_title = self.plugin_name.clone();
        // Transmit as usize so the raw pointer can cross the thread boundary.
        let lv2_handle: Option<usize> = self
            .instance
            .as_ref()
            .map(|i| i.instance().handle() as usize)
            .filter(|&h| h != 0);
        let (tx, rx) = mpsc::channel::<UiFeedbackMessage>();
        self.ui_feedback_tx = Some(tx);
        let thread = thread::spawn(move || {
            if let Some(ext) = ext_spec {
                if let Err(e) = run_external_ui(ext, values, lv2_handle) {
                    eprintln!("LV2 external UI failed: {e}");
                }
            } else if let Err(e) = run_gtk_plugin_ui(
                ui_spec.ok(),
                plugin_title,
                values,
                symbol_map,
                control_ports,
                lv2_handle,
                rx,
            ) {
                eprintln!("LV2 UI failed: {e}");
            }
        });
        self.ui_thread = Some(thread);
        Ok(())
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

    fn instance_handle(&self) -> Lv2Handle {
        self.instance
            .as_ref()
            .map(|i| i.instance().handle() as Lv2Handle)
            .unwrap_or(std::ptr::null_mut())
    }

    fn state_feature_ptrs(&self) -> [*const LV2Feature; 5] {
        let sp = self._state_path_feature.feature_ptrs();
        [
            self._urid_feature.feature() as *const LV2Feature,
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
        if self.skip_deactivate_on_drop {
            if let Some(cleanup) = instance.instance().descriptor().map(|d| d.cleanup) {
                cleanup(instance.instance().handle());
            }
            std::mem::forget(instance);
            return;
        }
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
        let instance = instantiate_plugin(
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

extern "C" fn suil_write_port(
    controller: SuilController,
    port_index: u32,
    buffer_size: u32,
    protocol: u32,
    buffer: *const c_void,
) {
    if controller.is_null() || buffer.is_null() || protocol != 0 || buffer_size != 4 {
        return;
    }
    let controller = unsafe { &*(controller as *const UiController) };
    let value = unsafe { *(buffer as *const f32) };
    if let Ok(mut values) = controller.scalar_values.lock()
        && (port_index as usize) < values.len()
    {
        values[port_index as usize] = value;
    }
}

extern "C" fn suil_port_index(controller: SuilController, port_symbol: *const c_char) -> u32 {
    if controller.is_null() || port_symbol.is_null() {
        return u32::MAX;
    }
    let controller = unsafe { &*(controller as *const UiController) };
    let Some(symbol) = unsafe { CStr::from_ptr(port_symbol) }.to_str().ok() else {
        return u32::MAX;
    };
    controller
        .port_symbol_to_index
        .get(symbol)
        .copied()
        .unwrap_or(u32::MAX)
}

extern "C" fn suil_subscribe_port(
    controller: SuilController,
    port_index: u32,
    _protocol: u32,
    _features: *const *const LV2FeatureRaw,
) -> u32 {
    if controller.is_null() {
        return 1;
    }
    let controller = unsafe { &*(controller as *const UiController) };
    if let Ok(mut subscribed) = controller.subscribed_ports.lock() {
        subscribed.insert(port_index);
    }
    0
}

extern "C" fn suil_unsubscribe_port(
    controller: SuilController,
    port_index: u32,
    _protocol: u32,
    _features: *const *const LV2FeatureRaw,
) -> u32 {
    if controller.is_null() {
        return 1;
    }
    let controller = unsafe { &*(controller as *const UiController) };
    if let Ok(mut subscribed) = controller.subscribed_ports.lock() {
        subscribed.remove(&port_index);
    }
    0
}

unsafe extern "C" fn on_gtk_destroy(_widget: *mut c_void, _data: *mut c_void) {
    unsafe { gtk_main_quit() };
}

extern "C" fn host_ui_resize(handle: *mut c_void, width: i32, height: i32) -> i32 {
    if handle.is_null() || width <= 0 || height <= 0 {
        return 1;
    }
    unsafe {
        gtk_window_resize(handle, width, height);
    }
    0
}

extern "C" fn ui_idle_tick(data: *mut c_void) -> i32 {
    if data.is_null() {
        return 0;
    }
    let idle_data = unsafe { &*(data as *const UiIdleData) };
    if idle_data.interface.is_null() {
        return 0;
    }
    let interface = unsafe { &*idle_data.interface };
    let Some(idle_fn) = interface.idle else {
        return 0;
    };
    idle_fn(idle_data.handle)
}

fn schedule_ui_feedback_flush(state: &Arc<UiDispatchState>) {
    if state.dispatch_scheduled.swap(true, Ordering::SeqCst) {
        return;
    }
    let state_ptr = Arc::into_raw(Arc::clone(state)) as *mut c_void;
    unsafe {
        g_idle_add(Some(flush_ui_feedback), state_ptr);
    }
}

extern "C" fn flush_ui_feedback(data: *mut c_void) -> i32 {
    if data.is_null() {
        return 0;
    }
    let state = unsafe { Arc::from_raw(data as *const UiDispatchState) };
    if !state.alive.load(Ordering::SeqCst) {
        state.dispatch_scheduled.store(false, Ordering::SeqCst);
        return 0;
    }
    let pending = if let Ok(mut queue) = state.pending.lock() {
        std::mem::take(&mut *queue)
    } else {
        vec![]
    };
    let subscribed = state
        .subscribed_ports
        .lock()
        .map(|ports| ports.clone())
        .unwrap_or_default();
    for (port_index, value) in pending {
        if !subscribed.is_empty() && !subscribed.contains(&port_index) {
            continue;
        }
        match &state.target {
            UiDispatchTarget::Suil(instance) => unsafe {
                suil_instance_port_event(
                    *instance,
                    port_index,
                    std::mem::size_of::<f32>() as u32,
                    0,
                    (&value as *const f32).cast::<c_void>(),
                );
            },
            UiDispatchTarget::Generic(sliders) => {
                if let Some(slider) = sliders.get(&port_index) {
                    unsafe { gtk_range_set_value(*slider, value as f64) };
                }
            }
        }
    }
    state.dispatch_scheduled.store(false, Ordering::SeqCst);
    let has_more = state
        .pending
        .lock()
        .map(|queue| !queue.is_empty())
        .unwrap_or(false);
    if has_more && state.alive.load(Ordering::SeqCst) {
        schedule_ui_feedback_flush(&state);
    }
    0
}

fn resolve_preferred_ui(plugin_uri: &str) -> Result<UiSpec, String> {
    let world = World::new();
    world.load_all();
    let uri_node = world.new_uri(plugin_uri);
    let plugin = world
        .plugins()
        .plugin(&uri_node)
        .ok_or_else(|| format!("Plugin not found for URI: {plugin_uri}"))?;
    let uis = plugin
        .uis()
        .ok_or_else(|| format!("Plugin has no UI: {plugin_uri}"))?;

    let gtk3_uri = world.new_uri(LV2_UI_GTK3);
    let gtk_uri = world.new_uri(LV2_UI_GTK);
    let x11_uri = world.new_uri(LV2_UI_X11);
    let qt4_uri = world.new_uri(LV2_UI_QT4);
    let qt5_uri = world.new_uri(LV2_UI_QT5);
    let qt6_uri = world.new_uri(LV2_UI_QT6);
    let external_uri = world.new_uri(LV2_UI_EXTERNAL);
    let extui_widget_uri = world.new_uri(LV2_EXT_UI_WIDGET);

    // Only GTK2 and X11 are supported as host containers.  GTK3 is intentionally
    // omitted: the host creates GTK2 windows, so a GTK3-wrapped suil widget
    // (returned by libsuil_qt5_in_gtk3 or libsuil_x11_in_gtk3) would be added
    // to a GTK2 container – a toolkit conflict that crashes at runtime.
    // Qt plugins will therefore be wrapped by libsuil_qt5_in_gtk2 instead.
    let host_containers = [LV2_UI_GTK, LV2_UI_X11];
    let ui_classes = [
        (&gtk3_uri, LV2_UI_GTK3),
        (&gtk_uri, LV2_UI_GTK),
        (&x11_uri, LV2_UI_X11),
        (&qt6_uri, LV2_UI_QT6),
        (&qt5_uri, LV2_UI_QT5),
        (&qt4_uri, LV2_UI_QT4),
        (&external_uri, LV2_UI_EXTERNAL),
        (&extui_widget_uri, LV2_EXT_UI_WIDGET),
    ];

    let mut best: Option<(usize, usize, u32, UiSpec)> = None;
    for ui in uis.iter() {
        let ui_uri = ui
            .uri()
            .as_uri()
            .ok_or_else(|| "UI URI is invalid".to_string())?
            .to_string();
        let bundle_uri = ui
            .bundle_uri()
            .ok_or_else(|| "UI bundle URI missing".to_string())?;
        let binary_uri = ui
            .binary_uri()
            .ok_or_else(|| "UI binary URI missing".to_string())?;
        let (_, ui_bundle_path) = bundle_uri
            .path()
            .ok_or_else(|| "Failed to resolve UI bundle path".to_string())?;
        let (_, ui_binary_path) = binary_uri
            .path()
            .ok_or_else(|| "Failed to resolve UI binary path".to_string())?;

        for (class_rank, (class_node, class_uri)) in ui_classes.iter().enumerate() {
            if !ui.is_a(class_node) {
                continue;
            }
            let class_c = CString::new(*class_uri).map_err(|e| e.to_string())?;
            for container_uri in host_containers {
                let host_type = CString::new(container_uri).map_err(|e| e.to_string())?;
                let quality = unsafe { suil_ui_supported(host_type.as_ptr(), class_c.as_ptr()) };
                if quality == 0 {
                    continue;
                }
                // Prefer GTK host container for all UI classes (including X11UI),
                // matching jalv.gtk's embedding model and avoiding direct X11-only
                // parenting paths that can produce blank shells for some plugins.
                let container_rank = usize::from(container_uri != LV2_UI_GTK);
                let spec = UiSpec {
                    plugin_uri: plugin_uri.to_string(),
                    ui_uri: ui_uri.clone(),
                    container_type_uri: container_uri.to_string(),
                    ui_type_uri: (*class_uri).to_string(),
                    ui_bundle_path: ui_bundle_path.clone(),
                    ui_binary_path: ui_binary_path.clone(),
                };
                let is_better = match &best {
                    None => true,
                    Some((best_class_rank, best_container_rank, best_quality, _)) => {
                        class_rank < *best_class_rank
                            || (class_rank == *best_class_rank
                                && (container_rank < *best_container_rank
                                    || (container_rank == *best_container_rank
                                        && quality > *best_quality)))
                    }
                };
                if is_better {
                    best = Some((class_rank, container_rank, quality, spec));
                }
            }
        }
    }

    best.map(|(_, _, _, spec)| spec)
        .ok_or_else(|| format!("No supported UI found for plugin: {plugin_uri}"))
}

fn run_gtk_plugin_ui(
    ui_spec: Option<UiSpec>,
    plugin_name: String,
    scalar_values: Arc<Mutex<Vec<f32>>>,
    port_symbol_to_index: Arc<HashMap<String, u32>>,
    control_ports: Vec<ControlPortInfo>,
    lv2_handle: Option<usize>,
    feedback_rx: Receiver<UiFeedbackMessage>,
) -> Result<(), String> {
    let mut argc = 0;
    let mut argv: *mut *mut c_char = std::ptr::null_mut();
    if unsafe { gtk_init_check(&mut argc, &mut argv) } == 0 {
        return Err("Failed to initialize GTK".to_string());
    }

    if let Some(spec) = ui_spec {
        return run_gtk_suil_ui(
            spec,
            scalar_values,
            port_symbol_to_index,
            lv2_handle,
            feedback_rx,
        );
    }
    run_generic_parameter_ui(plugin_name, scalar_values, control_ports, feedback_rx)
}

fn spawn_feedback_forwarder(
    dispatch_state: Arc<UiDispatchState>,
    feedback_rx: Receiver<UiFeedbackMessage>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        loop {
            match feedback_rx.recv_timeout(Duration::from_millis(50)) {
                Ok(UiFeedbackMessage::ScalarChanges(changes)) => {
                    if !dispatch_state.alive.load(Ordering::SeqCst) {
                        return;
                    }
                    if changes.is_empty() {
                        continue;
                    }
                    if let Ok(mut queue) = dispatch_state.pending.lock() {
                        queue.extend(changes);
                    }
                    schedule_ui_feedback_flush(&dispatch_state);
                }
                Err(RecvTimeoutError::Timeout) => {
                    if !dispatch_state.alive.load(Ordering::SeqCst) {
                        return;
                    }
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return;
                }
            }
        }
    })
}

fn run_gtk_suil_ui(
    ui_spec: UiSpec,
    scalar_values: Arc<Mutex<Vec<f32>>>,
    port_symbol_to_index: Arc<HashMap<String, u32>>,
    lv2_handle: Option<usize>,
    feedback_rx: Receiver<UiFeedbackMessage>,
) -> Result<(), String> {
    let mut argc = 0;
    let mut argv: *mut *mut c_char = std::ptr::null_mut();
    if unsafe { gtk_init_check(&mut argc, &mut argv) } == 0 {
        return Err("Failed to initialize GTK".to_string());
    }

    let window = unsafe { gtk_window_new(GTK_WINDOW_TOPLEVEL) };
    if window.is_null() {
        return Err("Failed to create GTK window".to_string());
    }
    unsafe {
        gtk_window_set_default_size(window, 780, 520);
        g_signal_connect_data(
            window,
            b"destroy\0".as_ptr() as *const c_char,
            Some(on_gtk_destroy),
            std::ptr::null_mut(),
            None,
            0,
        );
    }

    // Use a GtkSocket/XID parent whenever suil's effective container is X11.
    // This is required not only for X11-native UIs, but also for wrapped UIs
    // (e.g. Qt) when suil chooses an X11 container.
    let effective_container = ui_spec.container_type_uri.clone();
    let use_x11_parent = effective_container == LV2_UI_X11;
    let parent_data: *mut c_void = if use_x11_parent {
        let socket = unsafe { gtk_socket_new() };
        if socket.is_null() {
            return Err("Failed to create GtkSocket for X11 UI embedding".to_string());
        }
        unsafe {
            gtk_container_add(window, socket);
            gtk_widget_show_all(window);
            gtk_widget_realize(socket);
        }
        let xid = unsafe { gtk_socket_get_id(socket) };
        xid as *mut c_void
    } else {
        window
    };

    let subscribed_ports = Arc::new(Mutex::new(HashSet::new()));
    let controller = Box::new(UiController {
        scalar_values,
        port_symbol_to_index,
        subscribed_ports: Arc::clone(&subscribed_ports),
    });
    let controller_ptr = Box::into_raw(controller) as *mut c_void;

    let host = unsafe {
        suil_host_new(
            Some(suil_write_port),
            Some(suil_port_index),
            Some(suil_subscribe_port),
            Some(suil_unsubscribe_port),
        )
    };
    if host.is_null() {
        unsafe { drop(Box::from_raw(controller_ptr as *mut UiController)) };
        return Err("Failed to create suil host".to_string());
    }

    let container_type_uri = CString::new(effective_container).map_err(|e| e.to_string())?;
    let plugin_uri = CString::new(ui_spec.plugin_uri).map_err(|e| e.to_string())?;
    let ui_uri = CString::new(ui_spec.ui_uri).map_err(|e| e.to_string())?;
    let ui_type_uri = CString::new(ui_spec.ui_type_uri).map_err(|e| e.to_string())?;
    let ui_bundle_path = CString::new(ui_spec.ui_bundle_path).map_err(|e| e.to_string())?;
    let ui_binary_path = CString::new(ui_spec.ui_binary_path).map_err(|e| e.to_string())?;
    let mut urid_feature = UridMapFeature::new()?;
    let parent_uri = CString::new(LV2_UI_PARENT).map_err(|e| e.to_string())?;
    let resize_uri = CString::new(LV2_UI_RESIZE).map_err(|e| e.to_string())?;
    let mut resize_feature = LV2UiResize {
        handle: window,
        ui_resize: Some(host_ui_resize),
    };
    let urid_raw = LV2FeatureRaw {
        uri: urid_feature.feature().uri,
        data: urid_feature.feature().data,
    };
    let parent_raw = LV2FeatureRaw {
        uri: parent_uri.as_ptr(),
        data: parent_data,
    };
    let resize_raw = LV2FeatureRaw {
        uri: resize_uri.as_ptr(),
        data: (&mut resize_feature as *mut LV2UiResize).cast::<c_void>(),
    };
    let instance_access_uri = CString::new(LV2_INSTANCE_ACCESS).map_err(|e| e.to_string())?;
    let instance_access_raw = lv2_handle.map(|h| LV2FeatureRaw {
        uri: instance_access_uri.as_ptr(),
        data: h as *mut c_void,
    });
    let mut feature_ptrs: Vec<*const LV2FeatureRaw> = vec![&urid_raw, &parent_raw, &resize_raw];
    if let Some(ref raw) = instance_access_raw {
        feature_ptrs.push(raw as *const LV2FeatureRaw);
    }
    feature_ptrs.push(std::ptr::null());

    let instance = unsafe {
        suil_instance_new(
            host,
            controller_ptr,
            container_type_uri.as_ptr(),
            plugin_uri.as_ptr(),
            ui_uri.as_ptr(),
            ui_type_uri.as_ptr(),
            ui_bundle_path.as_ptr(),
            ui_binary_path.as_ptr(),
            feature_ptrs.as_ptr(),
        )
    };
    if instance.is_null() {
        unsafe {
            suil_host_free(host);
            drop(Box::from_raw(controller_ptr as *mut UiController));
        }
        return Err("Failed to instantiate suil UI instance".to_string());
    }
    let title = CString::new(format!("LV2 UI - {}", plugin_uri.to_string_lossy()))
        .map_err(|e| e.to_string())?;
    unsafe {
        gtk_window_set_title(window, title.as_ptr());
    }

    let widget = unsafe { suil_instance_get_widget(instance) };
    if widget.is_null() {
        unsafe {
            suil_instance_free(instance);
            suil_host_free(host);
            drop(Box::from_raw(controller_ptr as *mut UiController));
        }
        return Err("Suil returned null UI widget".to_string());
    }

    let ui_handle = unsafe { suil_instance_get_handle(instance) };
    let idle_iface_uri = CString::new(LV2_UI_IDLE_INTERFACE).map_err(|e| e.to_string())?;
    let show_iface_uri = CString::new(LV2_UI_SHOW_INTERFACE).map_err(|e| e.to_string())?;
    let hide_iface_uri = CString::new(LV2_UI_HIDE_INTERFACE).map_err(|e| e.to_string())?;
    let idle_iface_ptr = unsafe {
        suil_instance_extension_data(instance, idle_iface_uri.as_ptr()) as *const LV2UiIdleInterface
    };
    let show_iface_ptr = unsafe {
        suil_instance_extension_data(instance, show_iface_uri.as_ptr()) as *const LV2UiShowInterface
    };
    let hide_iface_ptr = unsafe {
        suil_instance_extension_data(instance, hide_iface_uri.as_ptr()) as *const LV2UiHideInterface
    };
    let mut idle_source = 0;
    let mut idle_data_ptr: *mut UiIdleData = std::ptr::null_mut();
    if !idle_iface_ptr.is_null() {
        let idle_data = Box::new(UiIdleData {
            interface: idle_iface_ptr,
            handle: ui_handle,
        });
        idle_data_ptr = Box::into_raw(idle_data);
        unsafe {
            idle_source = g_timeout_add(16, Some(ui_idle_tick), idle_data_ptr.cast::<c_void>());
        }
    }

    let dispatch_state = Arc::new(UiDispatchState {
        target: UiDispatchTarget::Suil(instance),
        pending: Mutex::new(vec![]),
        dispatch_scheduled: AtomicBool::new(false),
        alive: AtomicBool::new(true),
        subscribed_ports: Arc::clone(&subscribed_ports),
    });
    let forwarder = spawn_feedback_forwarder(Arc::clone(&dispatch_state), feedback_rx);
    if !show_iface_ptr.is_null()
        && let Some(show) = unsafe { (*show_iface_ptr).show }
    {
        let _ = show(ui_handle);
    }
    unsafe {
        // For X11-container UIs the window was already shown before suil
        // instantiation so the socket could be realized and its XID obtained.
        // The plugin embeds directly into that XID, so there is no GTK widget
        // to pack here. For GTK-container UIs, add suil's widget normally.
        if !use_x11_parent {
            gtk_container_add(window, widget);
            gtk_widget_show_all(window);
        }
        gtk_main();
    }
    if !hide_iface_ptr.is_null()
        && let Some(hide) = unsafe { (*hide_iface_ptr).hide }
    {
        let _ = hide(ui_handle);
    }
    if idle_source != 0 {
        unsafe {
            g_source_remove(idle_source);
        }
    }
    if !idle_data_ptr.is_null() {
        unsafe {
            drop(Box::from_raw(idle_data_ptr));
        }
    }
    unsafe {
        dispatch_state.alive.store(false, Ordering::SeqCst);
        let _ = forwarder.join();
        suil_instance_free(instance);
        suil_host_free(host);
        drop(Box::from_raw(controller_ptr as *mut UiController));
    }
    let _ = &mut urid_feature;
    Ok(())
}

struct GenericSliderData {
    scalar_values: Arc<Mutex<Vec<f32>>>,
    port_index: u32,
}

unsafe impl Send for GenericSliderData {}

unsafe extern "C" fn on_slider_changed(range: *mut c_void, data: *mut c_void) {
    if range.is_null() || data.is_null() {
        return;
    }
    let data = unsafe { &*(data as *const GenericSliderData) };
    let value = unsafe { gtk_range_get_value(range) as f32 };
    if let Ok(mut values) = data.scalar_values.lock()
        && (data.port_index as usize) < values.len()
    {
        values[data.port_index as usize] = value;
    }
}

fn run_generic_parameter_ui(
    plugin_name: String,
    scalar_values: Arc<Mutex<Vec<f32>>>,
    control_ports: Vec<ControlPortInfo>,
    feedback_rx: Receiver<UiFeedbackMessage>,
) -> Result<(), String> {
    let window = unsafe { gtk_window_new(GTK_WINDOW_TOPLEVEL) };
    if window.is_null() {
        return Err("Failed to create generic parameter UI window".to_string());
    }
    let title =
        CString::new(format!("LV2 Generic UI - {plugin_name}")).map_err(|e| e.to_string())?;
    unsafe {
        gtk_window_set_title(window, title.as_ptr());
        gtk_window_set_default_size(window, 720, 480);
        g_signal_connect_data(
            window,
            b"destroy\0".as_ptr() as *const c_char,
            Some(on_gtk_destroy),
            std::ptr::null_mut(),
            None,
            0,
        );
    }

    let root = unsafe { gtk_vbox_new(0, 8) };
    if root.is_null() {
        return Err("Failed to create generic parameter UI root".to_string());
    }

    let mut sliders = HashMap::<u32, *mut c_void>::new();
    let mut slider_data = vec![];
    for port in control_ports {
        let row = unsafe { gtk_hbox_new(0, 8) };
        if row.is_null() {
            continue;
        }
        let label_txt = CString::new(port.name).map_err(|e| e.to_string())?;
        let label = unsafe { gtk_label_new(label_txt.as_ptr()) };
        let step = ((port.max - port.min).abs() / 200.0).max(0.0001) as f64;
        let slider = unsafe { gtk_hscale_new_with_range(port.min as f64, port.max as f64, step) };
        if slider.is_null() {
            continue;
        }
        unsafe {
            gtk_widget_set_size_request(slider, 420, -1);
        }
        if let Ok(values) = scalar_values.lock()
            && let Some(value) = values.get(port.index as usize)
        {
            unsafe {
                gtk_range_set_value(slider, *value as f64);
            }
        }

        let data = Box::new(GenericSliderData {
            scalar_values: Arc::clone(&scalar_values),
            port_index: port.index,
        });
        let data_ptr = Box::into_raw(data);
        slider_data.push(data_ptr);
        unsafe {
            g_signal_connect_data(
                slider,
                b"value-changed\0".as_ptr() as *const c_char,
                Some(on_slider_changed),
                data_ptr.cast::<c_void>(),
                None,
                0,
            );
            gtk_box_pack_start(row, label, 0, 0, 0);
            gtk_box_pack_start(row, slider, 1, 1, 0);
            gtk_box_pack_start(root, row, 0, 0, 0);
        }
        sliders.insert(port.index, slider);
    }

    let dispatch_state = Arc::new(UiDispatchState {
        target: UiDispatchTarget::Generic(sliders),
        pending: Mutex::new(vec![]),
        dispatch_scheduled: AtomicBool::new(false),
        alive: AtomicBool::new(true),
        subscribed_ports: Arc::new(Mutex::new(HashSet::new())),
    });
    let forwarder = spawn_feedback_forwarder(Arc::clone(&dispatch_state), feedback_rx);
    unsafe {
        gtk_container_add(window, root);
        gtk_widget_show_all(window);
        gtk_main();
    }
    dispatch_state.alive.store(false, Ordering::SeqCst);
    let _ = forwarder.join();
    for data_ptr in slider_data {
        unsafe {
            drop(Box::from_raw(data_ptr));
        }
    }
    Ok(())
}

/// Returns an `ExternalUiSpec` if the plugin has an external UI
/// (`LV2_UI_EXTERNAL` or `LV2_EXT_UI_WIDGET`) that we can load directly.
fn resolve_external_ui(plugin_uri: &str) -> Option<ExternalUiSpec> {
    let world = World::new();
    world.load_all();
    let uri_node = world.new_uri(plugin_uri);
    let plugin = world.plugins().plugin(&uri_node)?;
    let uis = plugin.uis()?;
    let external_node = world.new_uri(LV2_UI_EXTERNAL);
    let kx_node = world.new_uri(LV2_EXT_UI_WIDGET);
    for ui in uis.iter() {
        if !ui.is_a(&external_node) && !ui.is_a(&kx_node) {
            continue;
        }
        let ui_uri = ui.uri().as_uri()?.to_string();
        let (_, bundle_path) = ui.bundle_uri()?.path()?;
        let (_, binary_path) = ui.binary_uri()?.path()?;
        return Some(ExternalUiSpec {
            binary_path,
            bundle_path,
            plugin_uri: plugin_uri.to_string(),
            ui_uri,
        });
    }
    None
}

/// Write-function callback forwarded from external UI → audio thread.
extern "C" fn external_write_port(
    controller: *mut c_void,
    port_index: u32,
    buffer_size: u32,
    protocol: u32,
    buffer: *const c_void,
) {
    if controller.is_null() || buffer.is_null() || protocol != 0 || buffer_size != 4 {
        return;
    }
    let ctrl = unsafe { &*(controller as *const ExternalUiController) };
    let value = unsafe { *(buffer as *const f32) };
    if let Ok(mut values) = ctrl.scalar_values.lock() {
        if (port_index as usize) < values.len() {
            values[port_index as usize] = value;
        }
    }
}

/// Called by the external UI when the user closes its window.
unsafe extern "C" fn external_ui_closed(controller: *mut c_void) {
    if !controller.is_null() {
        let ctrl = unsafe { &*(controller as *const ExternalUiController) };
        ctrl.closed.store(true, Ordering::SeqCst);
    }
}

/// Run an external UI plugin (LV2_UI_EXTERNAL / LV2_EXT_UI_WIDGET) without GTK.
/// The binary is loaded with dlopen, instantiated directly, then shown and driven
/// via the widget's own run/show/hide vtable in a tight ~30 fps loop.
fn run_external_ui(
    spec: ExternalUiSpec,
    scalar_values: Arc<Mutex<Vec<f32>>>,
    lv2_handle: Option<usize>,
) -> Result<(), String> {
    let binary_cstr = CString::new(spec.binary_path.as_str()).map_err(|e| e.to_string())?;
    let lib = unsafe { dlopen(binary_cstr.as_ptr(), RTLD_NOW_GLOBAL) };
    if lib.is_null() {
        return Err(format!("dlopen failed for {}", spec.binary_path));
    }

    let sym = unsafe { dlsym(lib, b"lv2ui_descriptor\0".as_ptr() as *const c_char) };
    if sym.is_null() {
        unsafe { dlclose(lib) };
        return Err("lv2ui_descriptor symbol not found".to_string());
    }
    let descriptor_fn: LV2UiDescriptorFn = unsafe { std::mem::transmute(sym) };

    // Walk the descriptor table to find the one matching our UI URI.
    let descriptor: *const LV2UiDescriptor = unsafe {
        let mut idx = 0u32;
        loop {
            let d = descriptor_fn(idx);
            if d.is_null() {
                break std::ptr::null();
            }
            if CStr::from_ptr((*d).uri).to_str().unwrap_or("") == spec.ui_uri {
                break d;
            }
            idx += 1;
        }
    };
    if descriptor.is_null() {
        unsafe { dlclose(lib) };
        return Err(format!("no descriptor for UI URI {}", spec.ui_uri));
    }

    let mut urid_feature = UridMapFeature::new().map_err(|e| {
        unsafe { dlclose(lib) };
        e
    })?;
    let urid_raw = LV2FeatureRaw {
        uri: urid_feature.feature().uri,
        data: urid_feature.feature().data,
    };
    let instance_access_uri = CString::new(LV2_INSTANCE_ACCESS).map_err(|e| e.to_string())?;
    let instance_access_raw = lv2_handle.map(|h| LV2FeatureRaw {
        uri: instance_access_uri.as_ptr(),
        data: h as *mut c_void,
    });
    let ext_host_uri = CString::new(LV2_EXTERNAL_UI_HOST_KX).map_err(|e| e.to_string())?;
    let plugin_id = CString::new(spec.plugin_uri.as_str()).map_err(|e| e.to_string())?;

    let controller = Box::new(ExternalUiController {
        scalar_values,
        closed: AtomicBool::new(false),
    });
    let controller_ptr = Box::into_raw(controller);

    let mut ext_ui_host = LV2ExternalUiHost {
        ui_closed: Some(external_ui_closed),
        plugin_human_id: plugin_id.as_ptr(),
    };
    let ext_host_raw = LV2FeatureRaw {
        uri: ext_host_uri.as_ptr(),
        data: (&mut ext_ui_host as *mut LV2ExternalUiHost).cast::<c_void>(),
    };

    let mut feature_ptrs: Vec<*const LV2FeatureRaw> = vec![&urid_raw, &ext_host_raw];
    if let Some(ref raw) = instance_access_raw {
        feature_ptrs.push(raw as *const LV2FeatureRaw);
    }
    feature_ptrs.push(std::ptr::null());

    let plugin_uri_cstr = CString::new(spec.plugin_uri.as_str()).map_err(|e| e.to_string())?;
    let bundle_cstr = CString::new(spec.bundle_path.as_str()).map_err(|e| e.to_string())?;

    let mut widget_ptr: *mut c_void = std::ptr::null_mut();
    let ui_handle = unsafe {
        ((*descriptor).instantiate)(
            descriptor,
            plugin_uri_cstr.as_ptr(),
            bundle_cstr.as_ptr(),
            external_write_port,
            controller_ptr as *mut c_void,
            &mut widget_ptr,
            feature_ptrs.as_ptr(),
        )
    };
    if ui_handle.is_null() {
        unsafe {
            drop(Box::from_raw(controller_ptr));
            dlclose(lib);
        }
        return Err("External UI instantiate() returned null".to_string());
    }

    let widget = widget_ptr as *mut LV2ExternalUiWidget;
    if widget.is_null() {
        unsafe {
            ((*descriptor).cleanup)(ui_handle);
            drop(Box::from_raw(controller_ptr));
            dlclose(lib);
        }
        return Err("External UI widget pointer is null".to_string());
    }

    unsafe { ((*widget).show)(widget) };

    loop {
        unsafe { ((*widget).run)(widget) };
        if unsafe { (*controller_ptr).closed.load(Ordering::SeqCst) } {
            break;
        }
        thread::sleep(Duration::from_millis(33));
    }

    unsafe {
        ((*widget).hide)(widget);
        ((*descriptor).cleanup)(ui_handle);
        drop(Box::from_raw(controller_ptr));
        dlclose(lib);
    }
    let _ = &mut urid_feature;
    Ok(())
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
    _state_path_feature: &mut StatePathFeature,
) -> Result<lilv::instance::Instance, String> {
    let required_features = plugin_feature_uris(plugin);
    let features = [urid_feature.feature()];
    unsafe { plugin.instantiate(sample_rate, features) }.ok_or_else(|| {
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
    })
}

impl UridMapFeature {
    fn new() -> Result<Self, String> {
        let mut map = Box::new(LV2UridMap {
            handle: std::ptr::null_mut(),
            map: urid_map_callback,
        });
        let state = Box::new(Mutex::new(UridMapState {
            next_urid: 1,
            by_uri: HashMap::new(),
        }));
        map.handle = (&*state as *const Mutex<UridMapState>) as *mut c_void;

        let uri =
            CString::new(LV2_URID__MAP).map_err(|e| format!("Invalid URID feature URI: {e}"))?;
        let feature = LV2Feature {
            uri: uri.as_ptr(),
            data: (&mut *map as *mut LV2UridMap).cast::<c_void>(),
        };

        Ok(Self {
            _uri: uri,
            feature,
            _map: map,
            _state: state,
        })
    }

    fn feature(&self) -> &LV2Feature {
        &self.feature
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
        mapped
    }

    fn unmap_urid(&self, urid: LV2Urid) -> Option<String> {
        let Ok(state) = self._state.lock() else {
            return None;
        };
        state
            .by_uri
            .iter()
            .find_map(|(uri, mapped)| (*mapped == urid).then(|| uri.clone()))
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
        let make_uri =
            CString::new(LV2_STATE_MAKE_PATH_URI).expect("valid LV2 state makePath URI");
        let free_uri =
            CString::new(LV2_STATE_FREE_PATH_URI).expect("valid LV2 state freePath URI");

        let map_feature = LV2Feature {
            uri: map_uri.as_ptr(),
            data: (&*map as *const Lv2StateMapPath).cast_mut().cast::<c_void>(),
        };
        let make_feature = LV2Feature {
            uri: make_uri.as_ptr(),
            data: (&*make as *const Lv2StateMakePath).cast_mut().cast::<c_void>(),
        };
        let free_feature = LV2Feature {
            uri: free_uri.as_ptr(),
            data: (&*free as *const Lv2StateFreePath).cast_mut().cast::<c_void>(),
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
    mapped
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
