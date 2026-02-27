use crate::audio::io::AudioIO;
use crate::midi::io::MidiEvent;
use crate::mutex::UnsafeMutex;
use libloading::Library;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_void};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub struct ClapParameterInfo {
    pub id: u32,
    pub name: String,
    pub module: String,
    pub min_value: f64,
    pub max_value: f64,
    pub default_value: f64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClapPluginState {
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClapMidiOutputEvent {
    pub port: usize,
    pub event: MidiEvent,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ClapTransportInfo {
    pub transport_sample: usize,
    pub playing: bool,
    pub loop_enabled: bool,
    pub loop_range_samples: Option<(usize, usize)>,
    pub bpm: f64,
    pub tsig_num: u16,
    pub tsig_denom: u16,
}

#[derive(Clone, Copy, Debug)]
struct PendingParamValue {
    param_id: u32,
    value: f64,
}

#[derive(Clone, Copy, Debug)]
enum PendingParamEvent {
    Value { param_id: u32, value: f64, frame: u32 },
    GestureBegin { param_id: u32, frame: u32 },
    GestureEnd { param_id: u32, frame: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClapPluginInfo {
    pub name: String,
    pub path: String,
}

#[derive(Clone)]
pub struct ClapProcessor {
    path: String,
    name: String,
    sample_rate: f64,
    audio_inputs: Vec<Arc<AudioIO>>,
    audio_outputs: Vec<Arc<AudioIO>>,
    midi_input_ports: usize,
    midi_output_ports: usize,
    #[allow(dead_code)]
    host_runtime: Arc<HostRuntime>,
    plugin_handle: Arc<PluginHandle>,
    param_infos: Arc<Vec<ClapParameterInfo>>,
    param_values: Arc<UnsafeMutex<HashMap<u32, f64>>>,
    pending_param_events: Arc<UnsafeMutex<Vec<PendingParamEvent>>>,
}

impl fmt::Debug for ClapProcessor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClapProcessor")
            .field("path", &self.path)
            .field("name", &self.name)
            .field("audio_inputs", &self.audio_inputs.len())
            .field("audio_outputs", &self.audio_outputs.len())
            .field("midi_input_ports", &self.midi_input_ports)
            .field("midi_output_ports", &self.midi_output_ports)
            .finish()
    }
}

impl ClapProcessor {
    pub fn new(
        sample_rate: f64,
        buffer_size: usize,
        plugin_spec: &str,
        input_count: usize,
        output_count: usize,
    ) -> Result<Self, String> {
        let (plugin_path, plugin_id) = split_plugin_spec(plugin_spec);
        let name = Path::new(plugin_path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| plugin_spec.to_string());
        let host_runtime = Arc::new(HostRuntime::new()?);
        let plugin_handle = Arc::new(PluginHandle::load(
            plugin_path,
            plugin_id,
            host_runtime.clone(),
            sample_rate,
            buffer_size as u32,
        )?);
        let (discovered_inputs, discovered_outputs) = plugin_handle.audio_port_layout();
        let (discovered_midi_inputs, discovered_midi_outputs) = plugin_handle.note_port_layout();
        let resolved_inputs = discovered_inputs.unwrap_or(input_count).max(1);
        let resolved_outputs = discovered_outputs.unwrap_or(output_count).max(1);
        let audio_inputs = (0..resolved_inputs)
            .map(|_| Arc::new(AudioIO::new(buffer_size)))
            .collect();
        let audio_outputs = (0..resolved_outputs)
            .map(|_| Arc::new(AudioIO::new(buffer_size)))
            .collect();
        let param_infos = Arc::new(plugin_handle.parameter_infos());
        let param_values = Arc::new(UnsafeMutex::new(plugin_handle.parameter_values(&param_infos)));
        Ok(Self {
            path: plugin_spec.to_string(),
            name,
            sample_rate,
            audio_inputs,
            audio_outputs,
            midi_input_ports: discovered_midi_inputs.unwrap_or(1).max(1),
            midi_output_ports: discovered_midi_outputs.unwrap_or(1).max(1),
            host_runtime,
            plugin_handle,
            param_infos,
            param_values,
            pending_param_events: Arc::new(UnsafeMutex::new(Vec::new())),
        })
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
        let _ = self.process_with_midi(frames, &[], ClapTransportInfo::default());
    }

    pub fn process_with_midi(
        &self,
        frames: usize,
        midi_in: &[MidiEvent],
        transport: ClapTransportInfo,
    ) -> Vec<ClapMidiOutputEvent> {
        for port in &self.audio_inputs {
            if port.ready() {
                port.process();
            }
        }
        let (processed, processed_midi) = match self.process_native(frames, midi_in, transport) {
            Ok(v) => v,
            Err(err) => {
                eprintln!("CLAP processing error: {err}, producing silence");
                (false, Vec::new())
            }
        };
        if !processed {
            for out in &self.audio_outputs {
                let out_buf = out.buffer.lock();
                out_buf.fill(0.0);
                *out.finished.lock() = true;
            }
        }
        processed_midi
    }

    pub fn parameter_infos(&self) -> Vec<ClapParameterInfo> {
        self.param_infos.as_ref().clone()
    }

    pub fn parameter_values(&self) -> HashMap<u32, f64> {
        self.param_values.lock().clone()
    }

    pub fn set_parameter(&self, param_id: u32, value: f64) -> Result<(), String> {
        self.set_parameter_at(param_id, value, 0)
    }

    pub fn set_parameter_at(&self, param_id: u32, value: f64, frame: u32) -> Result<(), String> {
        let Some(info) = self.param_infos.iter().find(|p| p.id == param_id) else {
            return Err(format!("Unknown CLAP parameter id: {param_id}"));
        };
        let clamped = value.clamp(info.min_value, info.max_value);
        self.pending_param_events
            .lock()
            .push(PendingParamEvent::Value {
            param_id,
            value: clamped,
            frame,
        });
        self.param_values.lock().insert(param_id, clamped);
        Ok(())
    }

    pub fn begin_parameter_edit(&self, param_id: u32) -> Result<(), String> {
        self.begin_parameter_edit_at(param_id, 0)
    }

    pub fn begin_parameter_edit_at(&self, param_id: u32, frame: u32) -> Result<(), String> {
        if !self.param_infos.iter().any(|p| p.id == param_id) {
            return Err(format!("Unknown CLAP parameter id: {param_id}"));
        }
        self.pending_param_events
            .lock()
            .push(PendingParamEvent::GestureBegin { param_id, frame });
        Ok(())
    }

    pub fn end_parameter_edit(&self, param_id: u32) -> Result<(), String> {
        self.end_parameter_edit_at(param_id, 0)
    }

    pub fn end_parameter_edit_at(&self, param_id: u32, frame: u32) -> Result<(), String> {
        if !self.param_infos.iter().any(|p| p.id == param_id) {
            return Err(format!("Unknown CLAP parameter id: {param_id}"));
        }
        self.pending_param_events
            .lock()
            .push(PendingParamEvent::GestureEnd { param_id, frame });
        Ok(())
    }

    pub fn show_ui(&self) -> Result<(), String> {
        self.plugin_handle.show_ui()
    }

    pub fn snapshot_state(&self) -> Result<ClapPluginState, String> {
        self.plugin_handle.snapshot_state()
    }

    pub fn restore_state(&self, state: &ClapPluginState) -> Result<(), String> {
        self.plugin_handle.restore_state(state)
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

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    fn process_native(
        &self,
        frames: usize,
        midi_in: &[MidiEvent],
        transport: ClapTransportInfo,
    ) -> Result<(bool, Vec<ClapMidiOutputEvent>), String> {
        if frames == 0 {
            return Ok((true, Vec::new()));
        }

        let mut in_channel_ptrs: Vec<Vec<*mut f32>> = Vec::with_capacity(self.audio_inputs.len());
        let mut out_channel_ptrs: Vec<Vec<*mut f32>> = Vec::with_capacity(self.audio_outputs.len());
        let mut in_buffers = Vec::with_capacity(self.audio_inputs.len());
        let mut out_buffers = Vec::with_capacity(self.audio_outputs.len());

        for input in &self.audio_inputs {
            let buf = input.buffer.lock();
            in_channel_ptrs.push(vec![buf.as_ptr() as *mut f32]);
            in_buffers.push(buf);
        }
        for output in &self.audio_outputs {
            let buf = output.buffer.lock();
            out_channel_ptrs.push(vec![buf.as_ptr() as *mut f32]);
            out_buffers.push(buf);
        }

        let mut in_audio = Vec::with_capacity(self.audio_inputs.len());
        let mut out_audio = Vec::with_capacity(self.audio_outputs.len());

        for ptrs in &mut in_channel_ptrs {
            in_audio.push(ClapAudioBuffer {
                data32: ptrs.as_mut_ptr(),
                data64: std::ptr::null_mut(),
                channel_count: 1,
                latency: 0,
                constant_mask: 0,
            });
        }
        for ptrs in &mut out_channel_ptrs {
            out_audio.push(ClapAudioBuffer {
                data32: ptrs.as_mut_ptr(),
                data64: std::ptr::null_mut(),
                channel_count: 1,
                latency: 0,
                constant_mask: 0,
            });
        }

        let pending_params = std::mem::take(self.pending_param_events.lock());
        let (in_events, in_ctx) = input_events_from(
            midi_in,
            &pending_params,
            self.sample_rate,
            transport,
        );
        let out_cap = midi_in
            .len()
            .saturating_add(self.midi_output_ports.saturating_mul(64));
        let (mut out_events, mut out_ctx) = output_events_ctx(out_cap);

        let mut process = ClapProcess {
            steady_time: -1,
            frames_count: frames as u32,
            transport: std::ptr::null(),
            audio_inputs: in_audio.as_mut_ptr(),
            audio_outputs: out_audio.as_mut_ptr(),
            audio_inputs_count: in_audio.len() as u32,
            audio_outputs_count: out_audio.len() as u32,
            in_events: &in_events,
            out_events: &mut out_events,
        };

        let result = self.plugin_handle.process(&mut process);
        drop(in_ctx);
        for output in &self.audio_outputs {
            *output.finished.lock() = true;
        }
        let processed = result?;
        let host_flags = self.host_runtime.take_callback_flags();
        if host_flags.restart {
            self.plugin_handle.reset();
        }
        if host_flags.callback {
            self.plugin_handle.on_main_thread();
        }
        if host_flags.process {
            // Host already continuously schedules process blocks.
        }
        if processed {
            for update in &out_ctx.param_values {
                self.param_values
                    .lock()
                    .insert(update.param_id, update.value);
            }
            Ok((true, std::mem::take(&mut out_ctx.midi_events)))
        } else {
            Ok((false, Vec::new()))
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ClapVersion {
    major: u32,
    minor: u32,
    revision: u32,
}

const CLAP_VERSION: ClapVersion = ClapVersion {
    major: 1,
    minor: 2,
    revision: 0,
};

#[repr(C)]
struct ClapHost {
    clap_version: ClapVersion,
    host_data: *mut c_void,
    name: *const c_char,
    vendor: *const c_char,
    url: *const c_char,
    version: *const c_char,
    get_extension: Option<unsafe extern "C" fn(*const ClapHost, *const c_char) -> *const c_void>,
    request_restart: Option<unsafe extern "C" fn(*const ClapHost)>,
    request_process: Option<unsafe extern "C" fn(*const ClapHost)>,
    request_callback: Option<unsafe extern "C" fn(*const ClapHost)>,
}

#[repr(C)]
struct ClapPluginEntry {
    clap_version: ClapVersion,
    init: Option<unsafe extern "C" fn(*const c_char) -> bool>,
    deinit: Option<unsafe extern "C" fn()>,
    get_factory: Option<unsafe extern "C" fn(*const c_char) -> *const c_void>,
}

#[repr(C)]
struct ClapPluginFactory {
    get_plugin_count: Option<unsafe extern "C" fn(*const ClapPluginFactory) -> u32>,
    get_plugin_descriptor:
        Option<unsafe extern "C" fn(*const ClapPluginFactory, u32) -> *const ClapPluginDescriptor>,
    create_plugin: Option<
        unsafe extern "C" fn(
            *const ClapPluginFactory,
            *const ClapHost,
            *const c_char,
        ) -> *const ClapPlugin,
    >,
}

#[repr(C)]
struct ClapPluginDescriptor {
    clap_version: ClapVersion,
    id: *const c_char,
    name: *const c_char,
    vendor: *const c_char,
    url: *const c_char,
    manual_url: *const c_char,
    support_url: *const c_char,
    version: *const c_char,
    description: *const c_char,
    features: *const *const c_char,
}

#[repr(C)]
struct ClapPlugin {
    desc: *const ClapPluginDescriptor,
    plugin_data: *mut c_void,
    init: Option<unsafe extern "C" fn(*const ClapPlugin) -> bool>,
    destroy: Option<unsafe extern "C" fn(*const ClapPlugin)>,
    activate: Option<unsafe extern "C" fn(*const ClapPlugin, f64, u32, u32) -> bool>,
    deactivate: Option<unsafe extern "C" fn(*const ClapPlugin)>,
    start_processing: Option<unsafe extern "C" fn(*const ClapPlugin) -> bool>,
    stop_processing: Option<unsafe extern "C" fn(*const ClapPlugin)>,
    reset: Option<unsafe extern "C" fn(*const ClapPlugin)>,
    process: Option<unsafe extern "C" fn(*const ClapPlugin, *const ClapProcess) -> i32>,
    get_extension:
        Option<unsafe extern "C" fn(*const ClapPlugin, *const c_char) -> *const c_void>,
    on_main_thread: Option<unsafe extern "C" fn(*const ClapPlugin)>,
}

#[repr(C)]
struct ClapInputEvents {
    ctx: *const c_void,
    size: Option<unsafe extern "C" fn(*const ClapInputEvents) -> u32>,
    get: Option<
        unsafe extern "C" fn(*const ClapInputEvents, u32) -> *const ClapEventHeader,
    >,
}

#[repr(C)]
struct ClapOutputEvents {
    ctx: *mut c_void,
    try_push:
        Option<unsafe extern "C" fn(*const ClapOutputEvents, *const ClapEventHeader) -> bool>,
}

#[repr(C)]
struct ClapEventHeader {
    size: u32,
    time: u32,
    space_id: u16,
    type_: u16,
    flags: u32,
}

const CLAP_CORE_EVENT_SPACE_ID: u16 = 0;
const CLAP_EVENT_MIDI: u16 = 10;
const CLAP_EVENT_PARAM_VALUE: u16 = 5;
const CLAP_EVENT_PARAM_GESTURE_BEGIN: u16 = 6;
const CLAP_EVENT_PARAM_GESTURE_END: u16 = 7;
const CLAP_EVENT_TRANSPORT: u16 = 9;
const CLAP_TRANSPORT_HAS_TEMPO: u32 = 1 << 0;
const CLAP_TRANSPORT_HAS_BEATS_TIMELINE: u32 = 1 << 1;
const CLAP_TRANSPORT_HAS_SECONDS_TIMELINE: u32 = 1 << 2;
const CLAP_TRANSPORT_HAS_TIME_SIGNATURE: u32 = 1 << 3;
const CLAP_TRANSPORT_IS_PLAYING: u32 = 1 << 4;
const CLAP_TRANSPORT_IS_LOOP_ACTIVE: u32 = 1 << 6;
const CLAP_BEATTIME_FACTOR: i64 = 1_i64 << 31;
const CLAP_SECTIME_FACTOR: i64 = 1_i64 << 31;

#[repr(C)]
struct ClapEventMidi {
    header: ClapEventHeader,
    port_index: u16,
    data: [u8; 3],
}

#[repr(C)]
struct ClapEventParamValue {
    header: ClapEventHeader,
    param_id: u32,
    cookie: *mut c_void,
    note_id: i32,
    port_index: i16,
    channel: i16,
    key: i16,
    value: f64,
}

#[repr(C)]
struct ClapEventParamGesture {
    header: ClapEventHeader,
    param_id: u32,
}

#[repr(C)]
struct ClapEventTransport {
    header: ClapEventHeader,
    flags: u32,
    song_pos_beats: i64,
    song_pos_seconds: i64,
    tempo: f64,
    tempo_inc: f64,
    loop_start_beats: i64,
    loop_end_beats: i64,
    loop_start_seconds: i64,
    loop_end_seconds: i64,
    bar_start: i64,
    bar_number: i32,
    tsig_num: u16,
    tsig_denom: u16,
}

#[repr(C)]
struct ClapParamInfoRaw {
    id: u32,
    flags: u32,
    cookie: *mut c_void,
    name: [c_char; 256],
    module: [c_char; 1024],
    min_value: f64,
    max_value: f64,
    default_value: f64,
}

#[repr(C)]
struct ClapPluginParams {
    count: Option<unsafe extern "C" fn(*const ClapPlugin) -> u32>,
    get_info: Option<unsafe extern "C" fn(*const ClapPlugin, u32, *mut ClapParamInfoRaw) -> bool>,
    get_value: Option<unsafe extern "C" fn(*const ClapPlugin, u32, *mut f64) -> bool>,
    value_to_text:
        Option<unsafe extern "C" fn(*const ClapPlugin, u32, f64, *mut c_char, u32) -> bool>,
    text_to_value:
        Option<unsafe extern "C" fn(*const ClapPlugin, u32, *const c_char, *mut f64) -> bool>,
    flush:
        Option<unsafe extern "C" fn(*const ClapPlugin, *const ClapInputEvents, *const ClapOutputEvents)>,
}

#[repr(C)]
struct ClapPluginStateExt {
    save: Option<unsafe extern "C" fn(*const ClapPlugin, *const ClapOStream) -> bool>,
    load: Option<unsafe extern "C" fn(*const ClapPlugin, *const ClapIStream) -> bool>,
}

#[repr(C)]
struct ClapAudioPortInfoRaw {
    id: u32,
    name: [c_char; 256],
    flags: u32,
    channel_count: u32,
    port_type: *const c_char,
    in_place_pair: u32,
}

#[repr(C)]
struct ClapPluginAudioPorts {
    count: Option<unsafe extern "C" fn(*const ClapPlugin, bool) -> u32>,
    get: Option<unsafe extern "C" fn(*const ClapPlugin, u32, bool, *mut ClapAudioPortInfoRaw) -> bool>,
}

#[repr(C)]
struct ClapNotePortInfoRaw {
    id: u16,
    supported_dialects: u32,
    preferred_dialect: u32,
    name: [c_char; 256],
}

#[repr(C)]
struct ClapPluginNotePorts {
    count: Option<unsafe extern "C" fn(*const ClapPlugin, bool) -> u32>,
    get: Option<unsafe extern "C" fn(*const ClapPlugin, u32, bool, *mut ClapNotePortInfoRaw) -> bool>,
}

#[repr(C)]
struct ClapPluginGui {
    is_api_supported: Option<unsafe extern "C" fn(*const ClapPlugin, *const c_char, bool) -> bool>,
    get_preferred_api:
        Option<unsafe extern "C" fn(*const ClapPlugin, *mut *const c_char, *mut bool) -> bool>,
    create: Option<unsafe extern "C" fn(*const ClapPlugin, *const c_char, bool) -> bool>,
    destroy: Option<unsafe extern "C" fn(*const ClapPlugin)>,
    set_scale: Option<unsafe extern "C" fn(*const ClapPlugin, f64) -> bool>,
    get_size: Option<unsafe extern "C" fn(*const ClapPlugin, *mut u32, *mut u32) -> bool>,
    can_resize: Option<unsafe extern "C" fn(*const ClapPlugin) -> bool>,
    get_resize_hints: Option<unsafe extern "C" fn(*const ClapPlugin, *mut c_void) -> bool>,
    adjust_size: Option<unsafe extern "C" fn(*const ClapPlugin, *mut u32, *mut u32) -> bool>,
    set_size: Option<unsafe extern "C" fn(*const ClapPlugin, u32, u32) -> bool>,
    set_parent: Option<unsafe extern "C" fn(*const ClapPlugin, *const c_void) -> bool>,
    set_transient: Option<unsafe extern "C" fn(*const ClapPlugin, *const c_void) -> bool>,
    suggest_title: Option<unsafe extern "C" fn(*const ClapPlugin, *const c_char)>,
    show: Option<unsafe extern "C" fn(*const ClapPlugin) -> bool>,
    hide: Option<unsafe extern "C" fn(*const ClapPlugin) -> bool>,
}

#[repr(C)]
struct ClapHostThreadCheck {
    is_main_thread: Option<unsafe extern "C" fn(*const ClapHost) -> bool>,
    is_audio_thread: Option<unsafe extern "C" fn(*const ClapHost) -> bool>,
}

#[repr(C)]
struct ClapHostLatency {
    changed: Option<unsafe extern "C" fn(*const ClapHost)>,
}

#[repr(C)]
struct ClapHostTail {
    changed: Option<unsafe extern "C" fn(*const ClapHost)>,
}

#[repr(C)]
struct ClapHostTimerSupport {
    register_timer: Option<unsafe extern "C" fn(*const ClapHost, u32, *mut u32) -> bool>,
    unregister_timer: Option<unsafe extern "C" fn(*const ClapHost, u32) -> bool>,
}

#[repr(C)]
struct ClapOStream {
    ctx: *mut c_void,
    write: Option<unsafe extern "C" fn(*const ClapOStream, *const c_void, u64) -> i64>,
}

#[repr(C)]
struct ClapIStream {
    ctx: *mut c_void,
    read: Option<unsafe extern "C" fn(*const ClapIStream, *mut c_void, u64) -> i64>,
}

#[repr(C)]
struct ClapAudioBuffer {
    data32: *mut *mut f32,
    data64: *mut *mut f64,
    channel_count: u32,
    latency: u32,
    constant_mask: u64,
}

#[repr(C)]
struct ClapProcess {
    steady_time: i64,
    frames_count: u32,
    transport: *const c_void,
    audio_inputs: *mut ClapAudioBuffer,
    audio_outputs: *mut ClapAudioBuffer,
    audio_inputs_count: u32,
    audio_outputs_count: u32,
    in_events: *const ClapInputEvents,
    out_events: *mut ClapOutputEvents,
}

enum ClapInputEvent {
    Midi(ClapEventMidi),
    ParamValue(ClapEventParamValue),
    ParamGesture(ClapEventParamGesture),
    Transport(ClapEventTransport),
}

impl ClapInputEvent {
    fn header_ptr(&self) -> *const ClapEventHeader {
        match self {
            Self::Midi(e) => &e.header as *const ClapEventHeader,
            Self::ParamValue(e) => &e.header as *const ClapEventHeader,
            Self::ParamGesture(e) => &e.header as *const ClapEventHeader,
            Self::Transport(e) => &e.header as *const ClapEventHeader,
        }
    }
}

struct ClapInputEventsCtx {
    events: Vec<ClapInputEvent>,
}

struct ClapOutputEventsCtx {
    midi_events: Vec<ClapMidiOutputEvent>,
    param_values: Vec<PendingParamValue>,
}

struct ClapIStreamCtx<'a> {
    bytes: &'a [u8],
    offset: usize,
}

struct HostRuntime {
    #[allow(dead_code)]
    name: CString,
    #[allow(dead_code)]
    vendor: CString,
    #[allow(dead_code)]
    url: CString,
    #[allow(dead_code)]
    version: CString,
    callback_flags: Box<UnsafeMutex<HostCallbackFlags>>,
    host: ClapHost,
}

#[derive(Default, Clone, Copy)]
struct HostCallbackFlags {
    restart: bool,
    process: bool,
    callback: bool,
}

impl HostRuntime {
    fn new() -> Result<Self, String> {
        let name = CString::new("Maolan").map_err(|e| e.to_string())?;
        let vendor = CString::new("Maolan").map_err(|e| e.to_string())?;
        let url = CString::new("https://example.invalid").map_err(|e| e.to_string())?;
        let version = CString::new("0.0.1").map_err(|e| e.to_string())?;
        let mut callback_flags = Box::new(UnsafeMutex::new(HostCallbackFlags::default()));
        let host = ClapHost {
            clap_version: CLAP_VERSION,
            host_data: (&mut *callback_flags as *mut UnsafeMutex<HostCallbackFlags>).cast::<c_void>(),
            name: name.as_ptr(),
            vendor: vendor.as_ptr(),
            url: url.as_ptr(),
            version: version.as_ptr(),
            get_extension: Some(host_get_extension),
            request_restart: Some(host_request_restart),
            request_process: Some(host_request_process),
            request_callback: Some(host_request_callback),
        };
        Ok(Self {
            name,
            vendor,
            url,
            version,
            callback_flags,
            host,
        })
    }

    fn take_callback_flags(&self) -> HostCallbackFlags {
        let flags = self.callback_flags.lock();
        let out = *flags;
        *flags = HostCallbackFlags::default();
        out
    }
}

// SAFETY: HostRuntime owns stable CString storage and a CLAP host struct that
// contains raw pointers into that owned storage. The data is immutable after
// construction and safe to share/move across threads.
unsafe impl Send for HostRuntime {}
// SAFETY: See Send rationale above; HostRuntime has no interior mutation.
unsafe impl Sync for HostRuntime {}

struct PluginHandle {
    _library: Library,
    entry: *const ClapPluginEntry,
    plugin: *const ClapPlugin,
    gui_created: UnsafeMutex<bool>,
}

// SAFETY: PluginHandle only stores pointers/libraries managed by the CLAP ABI.
// Access to plugin processing is synchronized by the engine track scheduling.
unsafe impl Send for PluginHandle {}
// SAFETY: Shared references do not mutate PluginHandle fields directly.
unsafe impl Sync for PluginHandle {}

impl PluginHandle {
    fn load(
        plugin_path: &str,
        plugin_id: Option<&str>,
        host_runtime: Arc<HostRuntime>,
        sample_rate: f64,
        frames: u32,
    ) -> Result<Self, String> {
        let c_path = CString::new(plugin_path).map_err(|e| e.to_string())?;
        let factory_id = c"clap.plugin-factory";

        // SAFETY: We keep `library` alive for at least as long as plugin and entry pointers.
        let library = unsafe { Library::new(plugin_path) }.map_err(|e| e.to_string())?;
        // SAFETY: Symbol name and type follow CLAP ABI (`clap_entry` global variable).
        let entry_ptr = unsafe {
            let sym = library
                .get::<*const ClapPluginEntry>(b"clap_entry\0")
                .map_err(|e| e.to_string())?;
            *sym
        };
        if entry_ptr.is_null() {
            return Err("CLAP entry symbol is null".to_string());
        }
        // SAFETY: entry pointer comes from validated CLAP symbol.
        let entry = unsafe { &*entry_ptr };
        let init = entry
            .init
            .ok_or_else(|| "CLAP entry missing init()".to_string())?;
        // SAFETY: Valid C string path for plugin bundle.
        if unsafe { !init(c_path.as_ptr()) } {
            return Err(format!("CLAP entry init failed for {plugin_path}"));
        }
        let get_factory = entry
            .get_factory
            .ok_or_else(|| "CLAP entry missing get_factory()".to_string())?;
        // SAFETY: Factory id is a static NUL-terminated C string.
        let factory = unsafe { get_factory(factory_id.as_ptr()) } as *const ClapPluginFactory;
        if factory.is_null() {
            return Err("CLAP plugin factory not found".to_string());
        }
        // SAFETY: factory pointer was validated above.
        let factory_ref = unsafe { &*factory };
        let get_count = factory_ref
            .get_plugin_count
            .ok_or_else(|| "CLAP factory missing get_plugin_count()".to_string())?;
        let get_desc = factory_ref
            .get_plugin_descriptor
            .ok_or_else(|| "CLAP factory missing get_plugin_descriptor()".to_string())?;
        let create = factory_ref
            .create_plugin
            .ok_or_else(|| "CLAP factory missing create_plugin()".to_string())?;

        // SAFETY: factory function pointers are valid CLAP ABI function pointers.
        let count = unsafe { get_count(factory) };
        if count == 0 {
            return Err("CLAP factory returned zero plugins".to_string());
        }
        let mut selected_id = None::<CString>;
        for i in 0..count {
            // SAFETY: i < count.
            let desc = unsafe { get_desc(factory, i) };
            if desc.is_null() {
                continue;
            }
            // SAFETY: descriptor pointer comes from factory.
            let desc = unsafe { &*desc };
            if desc.id.is_null() {
                continue;
            }
            // SAFETY: descriptor id is NUL-terminated per CLAP ABI.
            let id = unsafe { CStr::from_ptr(desc.id) };
            let id_str = id.to_string_lossy();
            if plugin_id.is_none() || plugin_id == Some(id_str.as_ref()) {
                selected_id = Some(
                    CString::new(id_str.as_ref()).map_err(|e| format!("Invalid plugin id: {e}"))?,
                );
                break;
            }
        }
        let selected_id = selected_id.ok_or_else(|| {
            if let Some(id) = plugin_id {
                format!("CLAP descriptor id not found in bundle: {id}")
            } else {
                "CLAP descriptor not found".to_string()
            }
        })?;
        // SAFETY: valid host pointer and plugin id.
        let plugin = unsafe { create(factory, &host_runtime.host, selected_id.as_ptr()) };
        if plugin.is_null() {
            return Err("CLAP factory create_plugin failed".to_string());
        }
        // SAFETY: plugin pointer validated above.
        let plugin_ref = unsafe { &*plugin };
        let plugin_init = plugin_ref
            .init
            .ok_or_else(|| "CLAP plugin missing init()".to_string())?;
        // SAFETY: plugin pointer and function pointer follow CLAP ABI.
        if unsafe { !plugin_init(plugin) } {
            return Err("CLAP plugin init() failed".to_string());
        }
        if let Some(activate) = plugin_ref.activate {
            // SAFETY: plugin pointer and arguments are valid for current engine buffer config.
            if unsafe { !activate(plugin, sample_rate, frames.max(1), frames.max(1)) } {
                return Err("CLAP plugin activate() failed".to_string());
            }
        }
        if let Some(start_processing) = plugin_ref.start_processing {
            // SAFETY: plugin activated above.
            if unsafe { !start_processing(plugin) } {
                return Err("CLAP plugin start_processing() failed".to_string());
            }
        }
        Ok(Self {
            _library: library,
            entry: entry_ptr,
            plugin,
            gui_created: UnsafeMutex::new(false),
        })
    }

    fn process(&self, process: &mut ClapProcess) -> Result<bool, String> {
        // SAFETY: plugin pointer is valid for lifetime of self.
        let plugin = unsafe { &*self.plugin };
        let Some(process_fn) = plugin.process else {
            return Ok(false);
        };
        // SAFETY: process struct references live buffers for the duration of call.
        let _status = unsafe { process_fn(self.plugin, process as *const _) };
        Ok(true)
    }

    fn reset(&self) {
        // SAFETY: plugin pointer valid during self lifetime.
        let plugin = unsafe { &*self.plugin };
        if let Some(reset) = plugin.reset {
            // SAFETY: function pointer follows CLAP ABI.
            unsafe { reset(self.plugin) };
        }
    }

    fn on_main_thread(&self) {
        // SAFETY: plugin pointer valid during self lifetime.
        let plugin = unsafe { &*self.plugin };
        if let Some(on_main_thread) = plugin.on_main_thread {
            // SAFETY: function pointer follows CLAP ABI.
            unsafe { on_main_thread(self.plugin) };
        }
    }

    fn params_ext(&self) -> Option<&ClapPluginParams> {
        let ext_id = c"clap.params";
        // SAFETY: plugin pointer is valid while self is alive.
        let plugin = unsafe { &*self.plugin };
        let get_extension = plugin.get_extension?;
        // SAFETY: extension id is a valid static C string.
        let ext_ptr = unsafe { get_extension(self.plugin, ext_id.as_ptr()) };
        if ext_ptr.is_null() {
            return None;
        }
        // SAFETY: CLAP guarantees extension pointer layout for requested extension id.
        Some(unsafe { &*(ext_ptr as *const ClapPluginParams) })
    }

    fn state_ext(&self) -> Option<&ClapPluginStateExt> {
        let ext_id = c"clap.state";
        // SAFETY: plugin pointer is valid while self is alive.
        let plugin = unsafe { &*self.plugin };
        let get_extension = plugin.get_extension?;
        // SAFETY: extension id is valid static C string.
        let ext_ptr = unsafe { get_extension(self.plugin, ext_id.as_ptr()) };
        if ext_ptr.is_null() {
            return None;
        }
        // SAFETY: extension pointer layout follows clap.state ABI.
        Some(unsafe { &*(ext_ptr as *const ClapPluginStateExt) })
    }

    fn gui_ext(&self) -> Option<&ClapPluginGui> {
        let ext_id = c"clap.gui";
        // SAFETY: plugin pointer is valid while self is alive.
        let plugin = unsafe { &*self.plugin };
        let get_extension = plugin.get_extension?;
        // SAFETY: extension id is valid static C string.
        let ext_ptr = unsafe { get_extension(self.plugin, ext_id.as_ptr()) };
        if ext_ptr.is_null() {
            return None;
        }
        // SAFETY: extension pointer layout follows clap.gui ABI.
        Some(unsafe { &*(ext_ptr as *const ClapPluginGui) })
    }

    fn audio_ports_ext(&self) -> Option<&ClapPluginAudioPorts> {
        let ext_id = c"clap.audio-ports";
        // SAFETY: plugin pointer is valid while self is alive.
        let plugin = unsafe { &*self.plugin };
        let get_extension = plugin.get_extension?;
        // SAFETY: extension id is valid static C string.
        let ext_ptr = unsafe { get_extension(self.plugin, ext_id.as_ptr()) };
        if ext_ptr.is_null() {
            return None;
        }
        // SAFETY: extension pointer layout follows clap.audio-ports ABI.
        Some(unsafe { &*(ext_ptr as *const ClapPluginAudioPorts) })
    }

    fn note_ports_ext(&self) -> Option<&ClapPluginNotePorts> {
        let ext_id = c"clap.note-ports";
        // SAFETY: plugin pointer is valid while self is alive.
        let plugin = unsafe { &*self.plugin };
        let get_extension = plugin.get_extension?;
        // SAFETY: extension id is valid static C string.
        let ext_ptr = unsafe { get_extension(self.plugin, ext_id.as_ptr()) };
        if ext_ptr.is_null() {
            return None;
        }
        // SAFETY: extension pointer layout follows clap.note-ports ABI.
        Some(unsafe { &*(ext_ptr as *const ClapPluginNotePorts) })
    }

    fn parameter_infos(&self) -> Vec<ClapParameterInfo> {
        let Some(params) = self.params_ext() else {
            return Vec::new();
        };
        let Some(count_fn) = params.count else {
            return Vec::new();
        };
        let Some(get_info_fn) = params.get_info else {
            return Vec::new();
        };
        // SAFETY: function pointers come from plugin extension table.
        let count = unsafe { count_fn(self.plugin) };
        let mut out = Vec::with_capacity(count as usize);
        for idx in 0..count {
            let mut info = ClapParamInfoRaw {
                id: 0,
                flags: 0,
                cookie: std::ptr::null_mut(),
                name: [0; 256],
                module: [0; 1024],
                min_value: 0.0,
                max_value: 1.0,
                default_value: 0.0,
            };
            // SAFETY: info points to valid writable struct.
            if unsafe { !get_info_fn(self.plugin, idx, &mut info as *mut _) } {
                continue;
            }
            out.push(ClapParameterInfo {
                id: info.id,
                name: c_char_buf_to_string(&info.name),
                module: c_char_buf_to_string(&info.module),
                min_value: info.min_value,
                max_value: info.max_value,
                default_value: info.default_value,
            });
        }
        out
    }

    fn parameter_values(&self, infos: &[ClapParameterInfo]) -> HashMap<u32, f64> {
        let mut out = HashMap::new();
        let Some(params) = self.params_ext() else {
            for info in infos {
                out.insert(info.id, info.default_value);
            }
            return out;
        };
        let Some(get_value_fn) = params.get_value else {
            for info in infos {
                out.insert(info.id, info.default_value);
            }
            return out;
        };
        for info in infos {
            let mut value = info.default_value;
            // SAFETY: pointer to stack `value` is valid and param id belongs to plugin metadata.
            if unsafe { !get_value_fn(self.plugin, info.id, &mut value as *mut _) } {
                value = info.default_value;
            }
            out.insert(info.id, value);
        }
        out
    }

    fn snapshot_state(&self) -> Result<ClapPluginState, String> {
        let Some(state_ext) = self.state_ext() else {
            return Ok(ClapPluginState { bytes: Vec::new() });
        };
        let Some(save_fn) = state_ext.save else {
            return Ok(ClapPluginState { bytes: Vec::new() });
        };
        let mut bytes = Vec::<u8>::new();
        let mut stream = ClapOStream {
            ctx: (&mut bytes as *mut Vec<u8>).cast::<c_void>(),
            write: Some(clap_ostream_write),
        };
        // SAFETY: stream callbacks reference `bytes` for duration of call.
        if unsafe { !save_fn(self.plugin, &mut stream as *mut ClapOStream as *const ClapOStream) } {
            return Err("CLAP state save failed".to_string());
        }
        Ok(ClapPluginState { bytes })
    }

    fn restore_state(&self, state: &ClapPluginState) -> Result<(), String> {
        let Some(state_ext) = self.state_ext() else {
            return Ok(());
        };
        let Some(load_fn) = state_ext.load else {
            return Ok(());
        };
        let mut ctx = ClapIStreamCtx {
            bytes: &state.bytes,
            offset: 0,
        };
        let mut stream = ClapIStream {
            ctx: (&mut ctx as *mut ClapIStreamCtx).cast::<c_void>(),
            read: Some(clap_istream_read),
        };
        // SAFETY: stream callbacks reference `ctx` for duration of call.
        if unsafe { !load_fn(self.plugin, &mut stream as *mut ClapIStream as *const ClapIStream) } {
            return Err("CLAP state load failed".to_string());
        }
        Ok(())
    }

    fn show_ui(&self) -> Result<(), String> {
        let Some(gui) = self.gui_ext() else {
            return Err("CLAP plugin does not expose clap.gui".to_string());
        };
        let Some(show) = gui.show else {
            return Err("CLAP gui.show is unavailable".to_string());
        };
        let created = *self.gui_created.lock();
        if !created {
            let Some(create) = gui.create else {
                return Err("CLAP gui.create is unavailable".to_string());
            };
            let api_candidates = platform_gui_apis();
            let mut chosen: Option<CString> = None;
            if let Some(get_preferred_api) = gui.get_preferred_api {
                let mut api_ptr: *const c_char = std::ptr::null();
                let mut floating = true;
                // SAFETY: plugin pointer valid, out pointers initialized.
                let ok = unsafe { get_preferred_api(self.plugin, &mut api_ptr, &mut floating) };
                if ok && floating && !api_ptr.is_null() {
                    // SAFETY: returned API id is NUL-terminated.
                    let pref = unsafe { CStr::from_ptr(api_ptr) }.to_string_lossy().to_string();
                    if api_candidates.iter().any(|c| c == &pref) {
                        chosen = CString::new(pref).ok();
                    }
                }
            }
            if chosen.is_none() {
                if let Some(is_api_supported) = gui.is_api_supported {
                    for candidate in api_candidates {
                        let c = CString::new(candidate).map_err(|e| e.to_string())?;
                        // SAFETY: plugin pointer and static c-string are valid.
                        if unsafe { is_api_supported(self.plugin, c.as_ptr(), true) } {
                            chosen = Some(c);
                            break;
                        }
                    }
                }
            }
            let Some(api) = chosen else {
                return Err("No supported floating CLAP GUI API found".to_string());
            };
            // SAFETY: plugin pointer valid; api c-string lives across call.
            if unsafe { !create(self.plugin, api.as_ptr(), true) } {
                return Err("CLAP gui.create failed".to_string());
            }
            *self.gui_created.lock() = true;
        }
        // SAFETY: plugin pointer valid.
        if unsafe { !show(self.plugin) } {
            return Err("CLAP gui.show failed".to_string());
        }
        Ok(())
    }

    fn audio_port_layout(&self) -> (Option<usize>, Option<usize>) {
        let Some(ext) = self.audio_ports_ext() else {
            return (None, None);
        };
        let Some(count_fn) = ext.count else {
            return (None, None);
        };
        // SAFETY: function pointer comes from plugin extension table.
        let in_count = unsafe { count_fn(self.plugin, true) } as usize;
        // SAFETY: function pointer comes from plugin extension table.
        let out_count = unsafe { count_fn(self.plugin, false) } as usize;
        (Some(in_count.max(1)), Some(out_count.max(1)))
    }

    fn note_port_layout(&self) -> (Option<usize>, Option<usize>) {
        let Some(ext) = self.note_ports_ext() else {
            return (None, None);
        };
        let Some(count_fn) = ext.count else {
            return (None, None);
        };
        // SAFETY: function pointer comes from plugin extension table.
        let in_count = unsafe { count_fn(self.plugin, true) } as usize;
        // SAFETY: function pointer comes from plugin extension table.
        let out_count = unsafe { count_fn(self.plugin, false) } as usize;
        (Some(in_count.max(1)), Some(out_count.max(1)))
    }
}

impl Drop for PluginHandle {
    fn drop(&mut self) {
        // SAFETY: pointers were obtained from valid CLAP entry and plugin factory.
        unsafe {
            if !self.plugin.is_null() {
                let plugin = &*self.plugin;
                if *self.gui_created.lock()
                    && let Some(gui_ext) = self.gui_ext()
                {
                    if let Some(hide) = gui_ext.hide {
                        hide(self.plugin);
                    }
                    if let Some(destroy_gui) = gui_ext.destroy {
                        destroy_gui(self.plugin);
                    }
                }
                if let Some(stop_processing) = plugin.stop_processing {
                    stop_processing(self.plugin);
                }
                if let Some(deactivate) = plugin.deactivate {
                    deactivate(self.plugin);
                }
                if let Some(destroy) = plugin.destroy {
                    destroy(self.plugin);
                }
            }
            if !self.entry.is_null() {
                let entry = &*self.entry;
                if let Some(deinit) = entry.deinit {
                    deinit();
                }
            }
        }
    }
}

static HOST_THREAD_CHECK_EXT: ClapHostThreadCheck = ClapHostThreadCheck {
    is_main_thread: Some(host_is_main_thread),
    is_audio_thread: Some(host_is_audio_thread),
};
static HOST_LATENCY_EXT: ClapHostLatency = ClapHostLatency {
    changed: Some(host_latency_changed),
};
static HOST_TAIL_EXT: ClapHostTail = ClapHostTail {
    changed: Some(host_tail_changed),
};
static HOST_TIMER_EXT: ClapHostTimerSupport = ClapHostTimerSupport {
    register_timer: Some(host_timer_register),
    unregister_timer: Some(host_timer_unregister),
};
static NEXT_TIMER_ID: AtomicU32 = AtomicU32::new(1);

unsafe extern "C" fn host_get_extension(
    _host: *const ClapHost,
    _extension_id: *const c_char,
) -> *const c_void {
    if _extension_id.is_null() {
        return std::ptr::null();
    }
    // SAFETY: extension id is expected to be a valid NUL-terminated string.
    let id = unsafe { CStr::from_ptr(_extension_id) }.to_string_lossy();
    match id.as_ref() {
        "clap.host.thread-check" => (&HOST_THREAD_CHECK_EXT as *const ClapHostThreadCheck)
            .cast::<c_void>(),
        "clap.host.latency" => (&HOST_LATENCY_EXT as *const ClapHostLatency).cast::<c_void>(),
        "clap.host.tail" => (&HOST_TAIL_EXT as *const ClapHostTail).cast::<c_void>(),
        "clap.host.timer-support" => {
            (&HOST_TIMER_EXT as *const ClapHostTimerSupport).cast::<c_void>()
        }
        _ => std::ptr::null(),
    }
}

unsafe extern "C" fn host_request_process(_host: *const ClapHost) {
    if _host.is_null() {
        return;
    }
    // SAFETY: host_data was initialized to point to HostCallbackFlags storage.
    let flags_ptr = unsafe { (*_host).host_data as *mut UnsafeMutex<HostCallbackFlags> };
    if flags_ptr.is_null() {
        return;
    }
    // SAFETY: flags_ptr is valid for plugin lifetime.
    unsafe {
        (*flags_ptr).lock().process = true;
    }
}

unsafe extern "C" fn host_request_callback(_host: *const ClapHost) {
    if _host.is_null() {
        return;
    }
    // SAFETY: host_data was initialized to point to HostCallbackFlags storage.
    let flags_ptr = unsafe { (*_host).host_data as *mut UnsafeMutex<HostCallbackFlags> };
    if flags_ptr.is_null() {
        return;
    }
    // SAFETY: flags_ptr is valid for plugin lifetime.
    unsafe {
        (*flags_ptr).lock().callback = true;
    }
}

unsafe extern "C" fn host_request_restart(_host: *const ClapHost) {
    if _host.is_null() {
        return;
    }
    // SAFETY: host_data was initialized to point to HostCallbackFlags storage.
    let flags_ptr = unsafe { (*_host).host_data as *mut UnsafeMutex<HostCallbackFlags> };
    if flags_ptr.is_null() {
        return;
    }
    // SAFETY: flags_ptr is valid for plugin lifetime.
    unsafe {
        (*flags_ptr).lock().restart = true;
    }
}

unsafe extern "C" fn host_is_main_thread(_host: *const ClapHost) -> bool {
    true
}

unsafe extern "C" fn host_is_audio_thread(_host: *const ClapHost) -> bool {
    true
}

unsafe extern "C" fn host_latency_changed(_host: *const ClapHost) {}

unsafe extern "C" fn host_tail_changed(_host: *const ClapHost) {}

unsafe extern "C" fn host_timer_register(
    _host: *const ClapHost,
    _period_ms: u32,
    timer_id: *mut u32,
) -> bool {
    if timer_id.is_null() {
        return false;
    }
    let id = NEXT_TIMER_ID.fetch_add(1, Ordering::Relaxed);
    // SAFETY: timer_id points to writable u32 provided by plugin.
    unsafe {
        *timer_id = id;
    }
    true
}

unsafe extern "C" fn host_timer_unregister(_host: *const ClapHost, _timer_id: u32) -> bool {
    true
}

unsafe extern "C" fn input_events_size(_list: *const ClapInputEvents) -> u32 {
    if _list.is_null() {
        return 0;
    }
    // SAFETY: ctx points to ClapInputEventsCtx owned by process_native.
    let ctx = unsafe { (*_list).ctx as *const ClapInputEventsCtx };
    if ctx.is_null() {
        return 0;
    }
    // SAFETY: ctx is valid during process callback lifetime.
    unsafe { (*ctx).events.len() as u32 }
}

unsafe extern "C" fn input_events_get(
    _list: *const ClapInputEvents,
    _index: u32,
) -> *const ClapEventHeader {
    if _list.is_null() {
        return std::ptr::null();
    }
    // SAFETY: ctx points to ClapInputEventsCtx owned by process_native.
    let ctx = unsafe { (*_list).ctx as *const ClapInputEventsCtx };
    if ctx.is_null() {
        return std::ptr::null();
    }
    // SAFETY: ctx is valid during process callback lifetime.
    let events = unsafe { &(*ctx).events };
    let Some(event) = events.get(_index as usize) else {
        return std::ptr::null();
    };
    event.header_ptr()
}

unsafe extern "C" fn output_events_try_push(
    _list: *const ClapOutputEvents,
    _event: *const ClapEventHeader,
) -> bool {
    if _list.is_null() || _event.is_null() {
        return false;
    }
    // SAFETY: ctx points to ClapOutputEventsCtx owned by process_native.
    let ctx = unsafe { (*_list).ctx as *mut ClapOutputEventsCtx };
    if ctx.is_null() {
        return false;
    }
    // SAFETY: event pointer is valid for callback lifetime.
    let header = unsafe { &*_event };
    if header.space_id != CLAP_CORE_EVENT_SPACE_ID {
        return false;
    }
    match header.type_ {
        CLAP_EVENT_MIDI => {
            if (header.size as usize) < std::mem::size_of::<ClapEventMidi>() {
                return false;
            }
            // SAFETY: validated type/size above.
            let midi = unsafe { &*(_event as *const ClapEventMidi) };
            // SAFETY: ctx pointer is valid and uniquely owned during processing.
            unsafe {
                (*ctx).midi_events.push(ClapMidiOutputEvent {
                    port: midi.port_index as usize,
                    event: MidiEvent::new(header.time, midi.data.to_vec()),
                });
            }
            true
        }
        CLAP_EVENT_PARAM_VALUE => {
            if (header.size as usize) < std::mem::size_of::<ClapEventParamValue>() {
                return false;
            }
            // SAFETY: validated type/size above.
            let param = unsafe { &*(_event as *const ClapEventParamValue) };
            // SAFETY: ctx pointer is valid and uniquely owned during processing.
            unsafe {
                (*ctx).param_values.push(PendingParamValue {
                    param_id: param.param_id,
                    value: param.value,
                });
            }
            true
        }
        _ => false,
    }
}

fn input_events_from(
    midi_events: &[MidiEvent],
    param_events: &[PendingParamEvent],
    sample_rate: f64,
    transport: ClapTransportInfo,
) -> (ClapInputEvents, Box<ClapInputEventsCtx>) {
    let mut events = Vec::with_capacity(midi_events.len() + param_events.len() + 1);
    let bpm = transport.bpm.max(1.0);
    let sample_rate = sample_rate.max(1.0);
    let seconds = transport.transport_sample as f64 / sample_rate;
    let song_pos_seconds = (seconds * CLAP_SECTIME_FACTOR as f64) as i64;
    let beats = seconds * (bpm / 60.0);
    let song_pos_beats = (beats * CLAP_BEATTIME_FACTOR as f64) as i64;
    let mut flags = CLAP_TRANSPORT_HAS_TEMPO
        | CLAP_TRANSPORT_HAS_BEATS_TIMELINE
        | CLAP_TRANSPORT_HAS_SECONDS_TIMELINE
        | CLAP_TRANSPORT_HAS_TIME_SIGNATURE;
    if transport.playing {
        flags |= CLAP_TRANSPORT_IS_PLAYING;
    }
    let (loop_start_seconds, loop_end_seconds, loop_start_beats, loop_end_beats) =
        if transport.loop_enabled {
            if let Some((loop_start, loop_end)) = transport.loop_range_samples {
                flags |= CLAP_TRANSPORT_IS_LOOP_ACTIVE;
                let ls_sec = loop_start as f64 / sample_rate;
                let le_sec = loop_end as f64 / sample_rate;
                let ls_beats = ls_sec * (bpm / 60.0);
                let le_beats = le_sec * (bpm / 60.0);
                (
                    (ls_sec * CLAP_SECTIME_FACTOR as f64) as i64,
                    (le_sec * CLAP_SECTIME_FACTOR as f64) as i64,
                    (ls_beats * CLAP_BEATTIME_FACTOR as f64) as i64,
                    (le_beats * CLAP_BEATTIME_FACTOR as f64) as i64,
                )
            } else {
                (0, 0, 0, 0)
            }
        } else {
            (0, 0, 0, 0)
        };
    let ts_num = transport.tsig_num.max(1);
    let ts_denom = transport.tsig_denom.max(1);
    let beats_per_bar = ts_num as f64 * (4.0 / ts_denom as f64);
    let bar_number = if beats_per_bar > 0.0 {
        (beats / beats_per_bar).floor().max(0.0) as i32
    } else {
        0
    };
    let bar_start_beats = (bar_number as f64 * beats_per_bar * CLAP_BEATTIME_FACTOR as f64) as i64;
    events.push(ClapInputEvent::Transport(ClapEventTransport {
        header: ClapEventHeader {
            size: std::mem::size_of::<ClapEventTransport>() as u32,
            time: 0,
            space_id: CLAP_CORE_EVENT_SPACE_ID,
            type_: CLAP_EVENT_TRANSPORT,
            flags: 0,
        },
        flags,
        song_pos_beats,
        song_pos_seconds,
        tempo: bpm,
        tempo_inc: 0.0,
        loop_start_beats,
        loop_end_beats,
        loop_start_seconds,
        loop_end_seconds,
        bar_start: bar_start_beats,
        bar_number,
        tsig_num: ts_num,
        tsig_denom: ts_denom,
    }));
    for event in midi_events {
        if event.data.is_empty() {
            continue;
        }
        let mut data = [0_u8; 3];
        let bytes = event.data.len().min(3);
        data[..bytes].copy_from_slice(&event.data[..bytes]);
        events.push(ClapInputEvent::Midi(ClapEventMidi {
            header: ClapEventHeader {
                size: std::mem::size_of::<ClapEventMidi>() as u32,
                time: event.frame,
                space_id: CLAP_CORE_EVENT_SPACE_ID,
                type_: CLAP_EVENT_MIDI,
                flags: 0,
            },
            port_index: 0,
            data,
        }));
    }
    for param in param_events {
        match *param {
            PendingParamEvent::Value {
                param_id,
                value,
                frame,
            } => events.push(ClapInputEvent::ParamValue(ClapEventParamValue {
                header: ClapEventHeader {
                    size: std::mem::size_of::<ClapEventParamValue>() as u32,
                    time: frame,
                    space_id: CLAP_CORE_EVENT_SPACE_ID,
                    type_: CLAP_EVENT_PARAM_VALUE,
                    flags: 0,
                },
                param_id,
                cookie: std::ptr::null_mut(),
                note_id: -1,
                port_index: -1,
                channel: -1,
                key: -1,
                value,
            })),
            PendingParamEvent::GestureBegin { param_id, frame } => {
                events.push(ClapInputEvent::ParamGesture(ClapEventParamGesture {
                    header: ClapEventHeader {
                        size: std::mem::size_of::<ClapEventParamGesture>() as u32,
                        time: frame,
                        space_id: CLAP_CORE_EVENT_SPACE_ID,
                        type_: CLAP_EVENT_PARAM_GESTURE_BEGIN,
                        flags: 0,
                    },
                    param_id,
                }))
            }
            PendingParamEvent::GestureEnd { param_id, frame } => {
                events.push(ClapInputEvent::ParamGesture(ClapEventParamGesture {
                    header: ClapEventHeader {
                        size: std::mem::size_of::<ClapEventParamGesture>() as u32,
                        time: frame,
                        space_id: CLAP_CORE_EVENT_SPACE_ID,
                        type_: CLAP_EVENT_PARAM_GESTURE_END,
                        flags: 0,
                    },
                    param_id,
                }))
            }
        }
    }
    events.sort_by_key(|event| match event {
        ClapInputEvent::Midi(e) => e.header.time,
        ClapInputEvent::ParamValue(e) => e.header.time,
        ClapInputEvent::ParamGesture(e) => e.header.time,
        ClapInputEvent::Transport(e) => e.header.time,
    });
    for event in &mut events {
        if let ClapInputEvent::ParamValue(p) = event {
            p.header.time = p.header.time.min(u32::MAX);
        }
    }
    let mut ctx = Box::new(ClapInputEventsCtx { events });
    let list = ClapInputEvents {
        ctx: (&mut *ctx as *mut ClapInputEventsCtx).cast::<c_void>(),
        size: Some(input_events_size),
        get: Some(input_events_get),
    };
    (list, ctx)
}

fn output_events_ctx(capacity: usize) -> (ClapOutputEvents, Box<ClapOutputEventsCtx>) {
    let mut ctx = Box::new(ClapOutputEventsCtx {
        midi_events: Vec::with_capacity(capacity),
        param_values: Vec::with_capacity(capacity / 2),
    });
    let list = ClapOutputEvents {
        ctx: (&mut *ctx as *mut ClapOutputEventsCtx).cast::<c_void>(),
        try_push: Some(output_events_try_push),
    };
    (list, ctx)
}

fn c_char_buf_to_string<const N: usize>(buf: &[c_char; N]) -> String {
    let bytes = buf
        .iter()
        .take_while(|&&b| b != 0)
        .map(|&b| b as u8)
        .collect::<Vec<u8>>();
    String::from_utf8_lossy(&bytes).to_string()
}

fn split_plugin_spec(spec: &str) -> (&str, Option<&str>) {
    if let Some((path, id)) = spec.split_once("::") {
        if !id.trim().is_empty() {
            return (path, Some(id.trim()));
        }
    }
    (spec, None)
}

unsafe extern "C" fn clap_ostream_write(
    stream: *const ClapOStream,
    buffer: *const c_void,
    size: u64,
) -> i64 {
    if stream.is_null() || buffer.is_null() {
        return -1;
    }
    // SAFETY: ctx is initialized by snapshot_state and valid during callback.
    let ctx = unsafe { (*stream).ctx as *mut Vec<u8> };
    if ctx.is_null() {
        return -1;
    }
    let n = (size as usize).min(isize::MAX as usize);
    // SAFETY: source pointer is valid for `n` bytes per caller contract.
    let src = unsafe { std::slice::from_raw_parts(buffer.cast::<u8>(), n) };
    // SAFETY: ctx points to writable Vec<u8>.
    unsafe {
        (*ctx).extend_from_slice(src);
    }
    n as i64
}

unsafe extern "C" fn clap_istream_read(
    stream: *const ClapIStream,
    buffer: *mut c_void,
    size: u64,
) -> i64 {
    if stream.is_null() || buffer.is_null() {
        return -1;
    }
    // SAFETY: ctx is initialized by restore_state and valid during callback.
    let ctx = unsafe { (*stream).ctx as *mut ClapIStreamCtx<'_> };
    if ctx.is_null() {
        return -1;
    }
    // SAFETY: ctx points to valid read context.
    let ctx = unsafe { &mut *ctx };
    let remaining = ctx.bytes.len().saturating_sub(ctx.offset);
    if remaining == 0 {
        return 0;
    }
    let n = remaining.min(size as usize);
    // SAFETY: destination pointer is valid for `n` bytes per caller contract.
    let dst = unsafe { std::slice::from_raw_parts_mut(buffer.cast::<u8>(), n) };
    dst.copy_from_slice(&ctx.bytes[ctx.offset..ctx.offset + n]);
    ctx.offset += n;
    n as i64
}

pub fn list_plugins() -> Vec<ClapPluginInfo> {
    let mut roots = default_clap_search_roots();

    if let Ok(extra) = std::env::var("CLAP_PATH") {
        for p in std::env::split_paths(&extra) {
            if !p.as_os_str().is_empty() {
                roots.push(p);
            }
        }
    }

    let mut out = Vec::new();
    for root in roots {
        collect_clap_plugins(&root, &mut out);
    }

    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out.dedup_by(|a, b| a.path.eq_ignore_ascii_case(&b.path));
    out
}

fn collect_clap_plugins(root: &Path, out: &mut Vec<ClapPluginInfo>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else {
            continue;
        };

        if ft.is_dir() {
            collect_clap_plugins(&path, out);
            continue;
        }

        if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("clap"))
        {
            let infos = scan_bundle_descriptors(&path);
            if infos.is_empty() {
                let name = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.to_string_lossy().to_string());
                out.push(ClapPluginInfo {
                    name,
                    path: path.to_string_lossy().to_string(),
                });
            } else {
                out.extend(infos);
            }
        }
    }
}

fn scan_bundle_descriptors(path: &Path) -> Vec<ClapPluginInfo> {
    let path_str = path.to_string_lossy().to_string();
    let c_path = match CString::new(path_str.clone()) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let factory_id = c"clap.plugin-factory";
    // SAFETY: path points to plugin module file.
    let library = match unsafe { Library::new(path) } {
        Ok(lib) => lib,
        Err(_) => return Vec::new(),
    };
    // SAFETY: symbol is CLAP entry pointer.
    let entry_ptr = unsafe {
        match library.get::<*const ClapPluginEntry>(b"clap_entry\0") {
            Ok(sym) => *sym,
            Err(_) => return Vec::new(),
        }
    };
    if entry_ptr.is_null() {
        return Vec::new();
    }
    // SAFETY: entry pointer validated above.
    let entry = unsafe { &*entry_ptr };
    let Some(init) = entry.init else {
        return Vec::new();
    };
    // SAFETY: valid path c string.
    if unsafe { !init(c_path.as_ptr()) } {
        return Vec::new();
    }
    let mut out = Vec::new();
    if let Some(get_factory) = entry.get_factory {
        // SAFETY: static factory id.
        let factory = unsafe { get_factory(factory_id.as_ptr()) } as *const ClapPluginFactory;
        if !factory.is_null() {
            // SAFETY: factory pointer validated above.
            let factory_ref = unsafe { &*factory };
            if let (Some(get_count), Some(get_desc)) =
                (factory_ref.get_plugin_count, factory_ref.get_plugin_descriptor)
            {
                // SAFETY: function pointer from plugin.
                let count = unsafe { get_count(factory) };
                for i in 0..count {
                    // SAFETY: i < count.
                    let desc = unsafe { get_desc(factory, i) };
                    if desc.is_null() {
                        continue;
                    }
                    // SAFETY: descriptor pointer from plugin factory.
                    let desc = unsafe { &*desc };
                    if desc.id.is_null() || desc.name.is_null() {
                        continue;
                    }
                    // SAFETY: CLAP descriptor strings are NUL-terminated.
                    let id = unsafe { CStr::from_ptr(desc.id) }.to_string_lossy().to_string();
                    // SAFETY: CLAP descriptor strings are NUL-terminated.
                    let name = unsafe { CStr::from_ptr(desc.name) }
                        .to_string_lossy()
                        .to_string();
                    out.push(ClapPluginInfo {
                        name,
                        path: format!("{path_str}::{id}"),
                    });
                }
            }
        }
    }
    // SAFETY: deinit belongs to entry and is valid after init.
    if let Some(deinit) = entry.deinit {
        unsafe { deinit() };
    }
    out
}

fn platform_gui_apis() -> Vec<&'static str> {
    let mut apis = Vec::new();
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
    {
        apis.push("x11");
        apis.push("wayland");
    }
    #[cfg(target_os = "windows")]
    {
        apis.push("win32");
    }
    #[cfg(target_os = "macos")]
    {
        apis.push("cocoa");
    }
    apis
}

fn default_clap_search_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    #[cfg(target_os = "windows")]
    {
        roots.push(PathBuf::from(r"C:\Program Files\Common Files\CLAP"));
        roots.push(PathBuf::from(r"C:\Program Files (x86)\Common Files\CLAP"));
    }

    #[cfg(target_os = "macos")]
    {
        roots.push(PathBuf::from("/Library/Audio/Plug-Ins/CLAP"));
        roots.push(PathBuf::from(format!(
            "{}/Library/Audio/Plug-Ins/CLAP",
            home_dir()
        )));
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
    {
        roots.push(PathBuf::from("/usr/lib/clap"));
        roots.push(PathBuf::from("/usr/local/lib/clap"));
        roots.push(PathBuf::from(format!("{}/.clap", home_dir())));
        roots.push(PathBuf::from(format!("{}/.local/lib/clap", home_dir())));
    }

    roots
}

fn home_dir() -> String {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default()
}
