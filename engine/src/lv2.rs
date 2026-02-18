use std::{
    collections::HashMap,
    ffi::{CStr, CString, c_char, c_void},
    fmt,
    sync::Mutex,
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
    scalar_values: Vec<f32>,
    audio_inputs: Vec<Vec<f32>>,
    audio_outputs: Vec<Vec<f32>>,
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
        let mut audio_inputs: Vec<Vec<f32>> = vec![];
        let mut audio_outputs: Vec<Vec<f32>> = vec![];

        for port in plugin.iter_ports() {
            let index = port.index();
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
            scalar_values,
            audio_inputs,
            audio_outputs,
        };
        processor.connect_ports();
        Ok(processor)
    }

    pub fn uri(&self) -> &str {
        &self.uri
    }

    pub fn process(&mut self, input_channels: &[Vec<f32>], frames: usize) -> Vec<Vec<f32>> {
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
                    let ptr = &mut self.scalar_values[*index] as *mut f32;
                    unsafe {
                        self.instance.instance_mut().connect_port_mut(port_index, ptr);
                    }
                }
            }
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
