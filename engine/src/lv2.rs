use std::{
    collections::{HashMap, HashSet},
    ffi::{CStr, CString, c_char, c_uint, c_void},
    fmt,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread,
    time::Duration,
};

use lilv::{World, instance::ActiveInstance, node::Node, plugin::Plugin};
use lv2_raw::{
    LV2_ATOM__FRAMETIME, LV2_ATOM__SEQUENCE, LV2AtomSequence, LV2AtomSequenceBody, LV2Feature,
    LV2Urid, LV2UridMap, LV2UridMapHandle, LV2_URID__MAP,
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
    port_bindings: Vec<PortBinding>,
    scalar_values: Arc<Mutex<Vec<f32>>>,
    port_symbol_to_index: Arc<HashMap<String, u32>>,
    audio_inputs: Vec<Vec<f32>>,
    audio_outputs: Vec<Vec<f32>>,
    atom_inputs: Vec<Vec<u8>>,
    atom_outputs: Vec<Vec<u8>>,
    atom_sequence_urid: LV2Urid,
    atom_frame_time_urid: LV2Urid,
    skip_deactivate_on_drop: bool,
    control_ports: Vec<ControlPortInfo>,
    ui_feedback_cache: Vec<u32>,
    ui_feedback_tx: Option<Sender<UiFeedbackMessage>>,
    ui_thread: Option<thread::JoinHandle<()>>,
}

struct LoadedPlugin {
    instance: ActiveInstance,
    _urid_feature: UridMapFeature,
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

#[repr(C)]
struct LV2FeatureRaw {
    uri: *const c_char,
    data: *mut c_void,
}

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

#[derive(Debug, Clone)]
struct UiSpec {
    plugin_uri: String,
    ui_uri: String,
    container_type_uri: String,
    ui_type_uri: String,
    ui_bundle_path: String,
    ui_binary_path: String,
}

#[link(name = "suil-0")]
unsafe extern "C" {
    fn suil_host_new(
        write_func: Option<SuilPortWriteFunc>,
        index_func: Option<SuilPortIndexFunc>,
        subscribe_func: Option<extern "C" fn(SuilController, u32, u32, *const *const LV2FeatureRaw) -> u32>,
        unsubscribe_func: Option<extern "C" fn(SuilController, u32, u32, *const *const LV2FeatureRaw) -> u32>,
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
    fn suil_instance_extension_data(instance: *mut SuilInstance, uri: *const c_char)
    -> *const c_void;
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

impl Lv2Processor {
    pub fn new(sample_rate: f64, uri: &str) -> Result<Self, String> {
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
        let instance = instantiate_plugin(&plugin, sample_rate, uri, &mut urid_feature)?;
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
        let mut audio_inputs: Vec<Vec<f32>> = vec![];
        let mut audio_outputs: Vec<Vec<f32>> = vec![];
        let mut atom_inputs: Vec<Vec<u8>> = vec![];
        let mut atom_outputs: Vec<Vec<u8>> = vec![];
        let mut control_ports = vec![];
        let atom_sequence_urid = urid_feature.map_uri(LV2_ATOM__SEQUENCE);
        let atom_frame_time_urid = urid_feature.map_uri(LV2_ATOM__FRAMETIME);
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
                audio_inputs.push(vec![0.0]);
                port_bindings[index] = PortBinding::AudioInput(channel);
            } else if is_audio && is_output {
                let channel = audio_outputs.len();
                audio_outputs.push(vec![0.0]);
                port_bindings[index] = PortBinding::AudioOutput(channel);
            } else if is_atom && is_input {
                has_atom_ports = true;
                let atom_idx = atom_inputs.len();
                let mut buffer = vec![0_u8; LV2_ATOM_BUFFER_BYTES];
                prepare_empty_atom_sequence(&mut buffer, atom_sequence_urid, atom_frame_time_urid);
                atom_inputs.push(buffer);
                port_bindings[index] = PortBinding::AtomInput(atom_idx);
            } else if is_atom && is_output {
                has_atom_ports = true;
                let atom_idx = atom_outputs.len();
                let mut buffer = vec![0_u8; LV2_ATOM_BUFFER_BYTES];
                prepare_empty_atom_sequence(&mut buffer, atom_sequence_urid, atom_frame_time_urid);
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
                    let mut min = range.minimum.and_then(|node| node.as_float()).unwrap_or(0.0);
                    let mut max = range.maximum.and_then(|node| node.as_float()).unwrap_or(1.0);
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
            port_bindings,
            scalar_values: Arc::new(Mutex::new(scalar_values)),
            port_symbol_to_index: Arc::new(port_symbol_to_index),
            audio_inputs,
            audio_outputs,
            atom_inputs,
            atom_outputs,
            atom_sequence_urid,
            atom_frame_time_urid,
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

    pub fn process(&mut self, input_channels: &[Vec<f32>], frames: usize) -> Vec<Vec<f32>> {
        if let Ok(mut values) = self.scalar_values.lock() {
            if values.is_empty() {
                values.push(0.0);
            }
        }
        self.resize_buffers(frames);

        for (channel, buffer) in self.audio_inputs.iter_mut().enumerate() {
            buffer.fill(0.0);
            if let Some(input) = input_channels.get(channel) {
                let copy_len = input.len().min(frames);
                buffer[..copy_len].copy_from_slice(&input[..copy_len]);
            }
        }
        for buffer in &mut self.audio_outputs {
            buffer.fill(0.0);
        }
        for buffer in &mut self.atom_inputs {
            prepare_empty_atom_sequence(buffer, self.atom_sequence_urid, self.atom_frame_time_urid);
        }
        for buffer in &mut self.atom_outputs {
            prepare_empty_atom_sequence(buffer, self.atom_sequence_urid, self.atom_frame_time_urid);
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
        self.audio_outputs.clone()
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

    fn resize_buffers(&mut self, frames: usize) {
        let frames = frames.max(1);
        for buffer in &mut self.audio_inputs {
            if buffer.len() != frames {
                buffer.resize(frames, 0.0);
            }
        }
        for buffer in &mut self.audio_outputs {
            if buffer.len() != frames {
                buffer.resize(frames, 0.0);
            }
        }
    }

    fn connect_ports(&mut self) {
        for (port_index, binding) in self.port_bindings.iter().enumerate() {
            match binding {
                PortBinding::AudioInput(channel) => {
                    let ptr = self.audio_inputs[*channel].as_mut_ptr();
                    if let Some(instance) = self.instance.as_mut() {
                        unsafe {
                            instance.instance_mut().connect_port_mut(port_index, ptr);
                        }
                    }
                }
                PortBinding::AudioOutput(channel) => {
                    let ptr = self.audio_outputs[*channel].as_mut_ptr();
                    if let Some(instance) = self.instance.as_mut() {
                        unsafe {
                            instance.instance_mut().connect_port_mut(port_index, ptr);
                        }
                    }
                }
                PortBinding::AtomInput(atom_index) => {
                    let ptr = self.atom_inputs[*atom_index].as_mut_ptr();
                    if let Some(instance) = self.instance.as_mut() {
                        unsafe {
                            instance.instance_mut().connect_port_mut(port_index, ptr);
                        }
                    }
                }
                PortBinding::AtomOutput(atom_index) => {
                    let ptr = self.atom_outputs[*atom_index].as_mut_ptr();
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
        let values = Arc::clone(&self.scalar_values);
        let symbol_map = Arc::clone(&self.port_symbol_to_index);
        let control_ports = self.control_ports.clone();
        let plugin_title = self.plugin_name.clone();
        let (tx, rx) = mpsc::channel::<UiFeedbackMessage>();
        self.ui_feedback_tx = Some(tx);
        let thread = thread::spawn(move || {
            if let Err(e) =
                run_gtk_plugin_ui(ui_spec.ok(), plugin_title, values, symbol_map, control_ports, rx)
            {
                eprintln!("LV2 UI failed: {e}");
            }
        });
        self.ui_thread = Some(thread);
        Ok(())
    }
}

impl Drop for Lv2Processor {
    fn drop(&mut self) {
        let Some(instance) = self.instance.take() else {
            return;
        };
        if self.skip_deactivate_on_drop {
            // Some plugins exposing atom/event ports crash inside deactivate.
            // Leak the active instance as a process-lifetime fallback to avoid SIGSEGV on quit.
            std::mem::forget(instance);
        } else {
            drop(instance);
        }
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
        let output_port = self.world.new_uri("http://lv2plug.in/ns/lv2core#OutputPort");
        let audio_port = self.world.new_uri("http://lv2plug.in/ns/lv2core#AudioPort");
        let atom_port = self.world.new_uri("http://lv2plug.in/ns/ext/atom#AtomPort");
        let event_port = self.world.new_uri("http://lv2plug.in/ns/ext/event#EventPort");
        let midi_event = self.world.new_uri("http://lv2plug.in/ns/ext/midi#MidiEvent");

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
                let (audio_inputs, audio_outputs, midi_inputs, midi_outputs) =
                    plugin_port_counts(
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
        let instance = instantiate_plugin(&plugin, self.sample_rate, uri, &mut urid_feature)?;
        let active_instance = unsafe { instance.activate() };
        self.loaded_plugins.insert(
            uri.to_string(),
            LoadedPlugin {
                instance: active_instance,
                _urid_feature: urid_feature,
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
    let has_more = state.pending.lock().map(|queue| !queue.is_empty()).unwrap_or(false);
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

    let host_containers = [LV2_UI_GTK3, LV2_UI_GTK, LV2_UI_X11];
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

    let mut best: Option<(u32, UiSpec)> = None;
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

        for (class_node, class_uri) in ui_classes {
            if !ui.is_a(class_node) {
                continue;
            }
            let class_c = CString::new(class_uri).map_err(|e| e.to_string())?;
            for container_uri in host_containers {
                let host_type = CString::new(container_uri).map_err(|e| e.to_string())?;
                let quality = unsafe { suil_ui_supported(host_type.as_ptr(), class_c.as_ptr()) };
                if quality == 0 {
                    continue;
                }
                let spec = UiSpec {
                    plugin_uri: plugin_uri.to_string(),
                    ui_uri: ui_uri.clone(),
                    container_type_uri: container_uri.to_string(),
                    ui_type_uri: class_uri.to_string(),
                    ui_bundle_path: ui_bundle_path.clone(),
                    ui_binary_path: ui_binary_path.clone(),
                };
                if best.as_ref().map(|(q, _)| quality > *q).unwrap_or(true) {
                    best = Some((quality, spec));
                }
            }
        }
    }

    best.map(|(_, spec)| spec)
        .ok_or_else(|| format!("No supported UI found for plugin: {plugin_uri}"))
}

fn run_gtk_plugin_ui(
    ui_spec: Option<UiSpec>,
    plugin_name: String,
    scalar_values: Arc<Mutex<Vec<f32>>>,
    port_symbol_to_index: Arc<HashMap<String, u32>>,
    control_ports: Vec<ControlPortInfo>,
    feedback_rx: Receiver<UiFeedbackMessage>,
) -> Result<(), String> {
    let mut argc = 0;
    let mut argv: *mut *mut c_char = std::ptr::null_mut();
    if unsafe { gtk_init_check(&mut argc, &mut argv) } == 0 {
        return Err("Failed to initialize GTK".to_string());
    }

    if let Some(spec) = ui_spec {
        return run_gtk_suil_ui(spec, scalar_values, port_symbol_to_index, feedback_rx);
    }
    run_generic_parameter_ui(plugin_name, scalar_values, control_ports, feedback_rx)
}

fn spawn_feedback_forwarder(
    dispatch_state: Arc<UiDispatchState>,
    feedback_rx: Receiver<UiFeedbackMessage>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || loop {
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
    })
}

fn run_gtk_suil_ui(
    ui_spec: UiSpec,
    scalar_values: Arc<Mutex<Vec<f32>>>,
    port_symbol_to_index: Arc<HashMap<String, u32>>,
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

    let container_type_uri = CString::new(ui_spec.container_type_uri).map_err(|e| e.to_string())?;
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
        data: window,
    };
    let resize_raw = LV2FeatureRaw {
        uri: resize_uri.as_ptr(),
        data: (&mut resize_feature as *mut LV2UiResize).cast::<c_void>(),
    };
    let feature_ptrs: [*const LV2FeatureRaw; 4] =
        [&urid_raw as *const LV2FeatureRaw, &parent_raw, &resize_raw, std::ptr::null()];

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
        gtk_container_add(window, widget);
        gtk_widget_show_all(window);
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

        let uri = CString::new(LV2_URID__MAP).map_err(|e| format!("Invalid URID feature URI: {e}"))?;
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
}

fn prepare_empty_atom_sequence(buffer: &mut [u8], sequence_urid: LV2Urid, frame_time_urid: LV2Urid) {
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
