use std::{
    collections::HashMap,
    ffi::{CStr, CString, c_char, c_void},
    fmt,
    sync::{Arc, Mutex},
    thread,
};

use lilv::{World, instance::ActiveInstance, node::Node, plugin::Plugin};
use lv2_raw::{LV2Feature, LV2Urid, LV2UridMap, LV2UridMapHandle, LV2_URID__MAP};

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
    Scalar(usize),
}

pub struct Lv2Processor {
    uri: String,
    instance: ActiveInstance,
    _urid_feature: UridMapFeature,
    port_bindings: Vec<PortBinding>,
    scalar_values: Arc<Mutex<Vec<f32>>>,
    port_symbol_to_index: Arc<HashMap<String, u32>>,
    audio_inputs: Vec<Vec<f32>>,
    audio_outputs: Vec<Vec<f32>>,
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
}

unsafe impl Send for UiController {}

#[repr(C)]
struct LV2FeatureRaw {
    uri: *const c_char,
    data: *mut c_void,
}

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
const GTK_WINDOW_TOPLEVEL: i32 = 0;

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
}

#[link(name = "gtk-x11-2.0")]
unsafe extern "C" {
    fn gtk_init_check(argc: *mut i32, argv: *mut *mut *mut c_char) -> i32;
    fn gtk_window_new(window_type: i32) -> *mut c_void;
    fn gtk_window_set_title(window: *mut c_void, title: *const c_char);
    fn gtk_window_set_default_size(window: *mut c_void, width: i32, height: i32);
    fn gtk_container_add(container: *mut c_void, widget: *mut c_void);
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

        let ports_count = plugin.ports_count();
        let mut port_bindings = vec![PortBinding::Scalar(0); ports_count];
        let mut scalar_values = vec![0.0_f32; ports_count.max(1)];
        let mut port_symbol_to_index = HashMap::<String, u32>::new();
        let mut audio_inputs: Vec<Vec<f32>> = vec![];
        let mut audio_outputs: Vec<Vec<f32>> = vec![];

        for port in plugin.iter_ports() {
            let index = port.index();
            if let Some(symbol) = port.symbol().and_then(|n| n.as_str().map(str::to_string)) {
                port_symbol_to_index.insert(symbol, index as u32);
            }
            let is_audio = port.is_a(&audio_port);
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
            } else {
                let default_value = port
                    .range()
                    .default
                    .and_then(|node| node.as_float())
                    .unwrap_or(0.0);
                scalar_values[index] = default_value;
                port_bindings[index] = PortBinding::Scalar(index);
            }
        }

        let mut processor = Self {
            uri: uri.to_string(),
            instance: active_instance,
            _urid_feature: urid_feature,
            port_bindings,
            scalar_values: Arc::new(Mutex::new(scalar_values)),
            port_symbol_to_index: Arc::new(port_symbol_to_index),
            audio_inputs,
            audio_outputs,
            ui_thread: None,
        };
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

        self.connect_ports();
        unsafe {
            self.instance.run(frames);
        }
        self.audio_outputs.clone()
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
                    unsafe {
                        self.instance.instance_mut().connect_port_mut(port_index, ptr);
                    }
                }
                PortBinding::AudioOutput(channel) => {
                    let ptr = self.audio_outputs[*channel].as_mut_ptr();
                    unsafe {
                        self.instance.instance_mut().connect_port_mut(port_index, ptr);
                    }
                }
                PortBinding::Scalar(index) => {
                    if let Ok(mut values) = self.scalar_values.lock() {
                        if *index < values.len() {
                            let ptr = (&mut values[*index]) as *mut f32;
                            unsafe {
                                self.instance.instance_mut().connect_port_mut(port_index, ptr);
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

        let ui_spec = resolve_preferred_ui(&self.uri)?;
        let values = Arc::clone(&self.scalar_values);
        let symbol_map = Arc::clone(&self.port_symbol_to_index);
        let thread = thread::spawn(move || {
            if let Err(e) = run_gtk_suil_ui(ui_spec, values, symbol_map) {
                eprintln!("LV2 UI failed: {e}");
            }
        });
        self.ui_thread = Some(thread);
        Ok(())
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

unsafe extern "C" fn on_gtk_destroy(_widget: *mut c_void, _data: *mut c_void) {
    unsafe { gtk_main_quit() };
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

    let candidates = [
        (&gtk_uri, LV2_UI_GTK, LV2_UI_GTK),
        (&gtk3_uri, LV2_UI_GTK3, LV2_UI_GTK3),
        (&gtk_uri, LV2_UI_GTK, LV2_UI_GTK3),
        (&x11_uri, LV2_UI_X11, LV2_UI_X11),
    ];
    for (class_node, class_uri, container_uri) in candidates {
        let host_type = CString::new(container_uri).map_err(|e| e.to_string())?;
        let class_c = CString::new(class_uri).map_err(|e| e.to_string())?;
        let quality = unsafe { suil_ui_supported(host_type.as_ptr(), class_c.as_ptr()) };
        if quality == 0 {
            continue;
        }
        for ui in uis.iter() {
            if !ui.is_a(class_node) {
                continue;
            }
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

            return Ok(UiSpec {
                plugin_uri: plugin_uri.to_string(),
                ui_uri,
                container_type_uri: container_uri.to_string(),
                ui_type_uri: class_uri.to_string(),
                ui_bundle_path,
                ui_binary_path,
            });
        }
    }

    Err(format!("No supported UI found for plugin: {plugin_uri}"))
}

fn run_gtk_suil_ui(
    ui_spec: UiSpec,
    scalar_values: Arc<Mutex<Vec<f32>>>,
    port_symbol_to_index: Arc<HashMap<String, u32>>,
) -> Result<(), String> {
    let mut argc = 0;
    let mut argv: *mut *mut c_char = std::ptr::null_mut();
    if unsafe { gtk_init_check(&mut argc, &mut argv) } == 0 {
        return Err("Failed to initialize GTK".to_string());
    }

    let controller = Box::new(UiController {
        scalar_values,
        port_symbol_to_index,
    });
    let controller_ptr = Box::into_raw(controller) as *mut c_void;

    let host = unsafe {
        suil_host_new(
            Some(suil_write_port),
            Some(suil_port_index),
            None,
            None,
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
    let feature = urid_feature.feature() as *const LV2Feature as *const LV2FeatureRaw;
    let features = [feature, std::ptr::null()];

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
            features.as_ptr(),
        )
    };
    if instance.is_null() {
        unsafe {
            suil_host_free(host);
            drop(Box::from_raw(controller_ptr as *mut UiController));
        }
        return Err("Failed to instantiate suil UI instance".to_string());
    }

    let window = unsafe { gtk_window_new(GTK_WINDOW_TOPLEVEL) };
    if window.is_null() {
        unsafe {
            suil_instance_free(instance);
            suil_host_free(host);
            drop(Box::from_raw(controller_ptr as *mut UiController));
        }
        return Err("Failed to create GTK window".to_string());
    }
    let title = CString::new(format!("LV2 UI - {}", plugin_uri.to_string_lossy()))
        .map_err(|e| e.to_string())?;
    unsafe {
        gtk_window_set_title(window, title.as_ptr());
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

    let widget = unsafe { suil_instance_get_widget(instance) };
    if widget.is_null() {
        unsafe {
            suil_instance_free(instance);
            suil_host_free(host);
            drop(Box::from_raw(controller_ptr as *mut UiController));
        }
        return Err("Suil returned null UI widget".to_string());
    }
    unsafe {
        gtk_container_add(window, widget);
        gtk_widget_show_all(window);
        gtk_main();
        suil_instance_free(instance);
        suil_host_free(host);
        drop(Box::from_raw(controller_ptr as *mut UiController));
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
