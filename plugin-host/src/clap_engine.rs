use crate::midi::io::MidiEvent;
use crate::mutex::UnsafeMutex;
#[cfg(any(
    target_os = "macos",
    target_os = "linux",
    target_os = "freebsd",
    target_os = "openbsd"
))]
use crate::plugins::paths;
use libloading::Library;
use serde::{Deserialize, Serialize};
use std::ffi::{CStr, CString, c_char, c_void};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClapGuiInfo {
    pub api: String,
    pub supports_embedded: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct ClapParamUpdate {
    pub param_id: u32,
    pub value: f64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClapPluginInfo {
    pub name: String,
    pub path: String,
    pub capabilities: Option<ClapPluginCapabilities>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClapPluginCapabilities {
    pub has_gui: bool,
    pub gui_apis: Vec<String>,
    pub supports_embedded: bool,
    pub supports_floating: bool,
    pub has_params: bool,
    pub has_state: bool,
    pub audio_inputs: usize,
    pub audio_outputs: usize,
    pub midi_inputs: usize,
    pub midi_outputs: usize,
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
    init: Option<unsafe extern "C" fn(*const ClapHost) -> bool>,
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
    get_extension: Option<unsafe extern "C" fn(*const ClapPlugin, *const c_char) -> *const c_void>,
    on_main_thread: Option<unsafe extern "C" fn(*const ClapPlugin)>,
}

#[repr(C)]
struct ClapInputEvents {
    ctx: *const c_void,
    size: Option<unsafe extern "C" fn(*const ClapInputEvents) -> u32>,
    get: Option<unsafe extern "C" fn(*const ClapInputEvents, u32) -> *const ClapEventHeader>,
}

#[repr(C)]
struct ClapOutputEvents {
    ctx: *mut c_void,
    try_push: Option<unsafe extern "C" fn(*const ClapOutputEvents, *const ClapEventHeader) -> bool>,
}

#[repr(C)]
struct ClapEventHeader {
    size: u32,
    time: u32,
    space_id: u16,
    type_: u16,
    flags: u32,
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
    get: Option<
        unsafe extern "C" fn(*const ClapPlugin, u32, bool, *mut ClapAudioPortInfoRaw) -> bool,
    >,
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
    get: Option<
        unsafe extern "C" fn(*const ClapPlugin, u32, bool, *mut ClapNotePortInfoRaw) -> bool,
    >,
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
    set_parent: Option<unsafe extern "C" fn(*const ClapPlugin, *const ClapWindow) -> bool>,
    set_transient: Option<unsafe extern "C" fn(*const ClapPlugin, *const ClapWindow) -> bool>,
    suggest_title: Option<unsafe extern "C" fn(*const ClapPlugin, *const c_char)>,
    show: Option<unsafe extern "C" fn(*const ClapPlugin) -> bool>,
    hide: Option<unsafe extern "C" fn(*const ClapPlugin) -> bool>,
}

#[repr(C)]
union ClapWindowHandle {
    x11: usize,
    native: *mut c_void,
    cocoa: *mut c_void,
}

#[repr(C)]
struct ClapWindow {
    api: *const c_char,
    handle: ClapWindowHandle,
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
struct ClapHostGui {
    resize_hints_changed: Option<unsafe extern "C" fn(*const ClapHost)>,
    request_resize: Option<unsafe extern "C" fn(*const ClapHost, u32, u32) -> bool>,
    request_show: Option<unsafe extern "C" fn(*const ClapHost) -> bool>,
    request_hide: Option<unsafe extern "C" fn(*const ClapHost) -> bool>,
    closed: Option<unsafe extern "C" fn(*const ClapHost, bool)>,
}

#[repr(C)]
struct ClapHostParams {
    rescan: Option<unsafe extern "C" fn(*const ClapHost, u32)>,
    clear: Option<unsafe extern "C" fn(*const ClapHost, u32, u32)>,
    request_flush: Option<unsafe extern "C" fn(*const ClapHost)>,
}

#[repr(C)]
struct ClapHostState {
    mark_dirty: Option<unsafe extern "C" fn(*const ClapHost)>,
}

#[repr(C)]
struct ClapHostAudioPorts {
    is_rescan_flag_supported: Option<unsafe extern "C" fn(*const ClapHost, flag: u32) -> bool>,
    rescan: Option<unsafe extern "C" fn(*const ClapHost, flags: u32)>,
}

#[repr(C)]
struct ClapHostNotePorts {
    supported_dialects: Option<unsafe extern "C" fn(*const ClapHost) -> u32>,
    rescan: Option<unsafe extern "C" fn(*const ClapHost, flags: u32)>,
}

#[repr(C)]
struct ClapHostNoteName {
    changed: Option<unsafe extern "C" fn(*const ClapHost)>,
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

#[derive(Default, Clone, Copy)]
struct HostCallbackFlags {
    restart: bool,
    process: bool,
    callback: bool,
}

#[derive(Clone, Copy)]
struct HostTimer {
    id: u32,
}

struct HostRuntimeState {
    callback_flags: UnsafeMutex<HostCallbackFlags>,
    timers: UnsafeMutex<Vec<HostTimer>>,
    ui_should_close: AtomicU32,
    ui_active: AtomicU32,
    param_flush_requested: AtomicU32,
    state_dirty_requested: AtomicU32,
    note_names_dirty: AtomicU32,
    audio_ports_rescan_requested: AtomicU32,
    note_ports_rescan_requested: AtomicU32,
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
static HOST_GUI_EXT: ClapHostGui = ClapHostGui {
    resize_hints_changed: Some(host_gui_resize_hints_changed),
    request_resize: Some(host_gui_request_resize),
    request_show: Some(host_gui_request_show),
    request_hide: Some(host_gui_request_hide),
    closed: Some(host_gui_closed),
};
static HOST_PARAMS_EXT: ClapHostParams = ClapHostParams {
    rescan: Some(host_params_rescan),
    clear: Some(host_params_clear),
    request_flush: Some(host_params_request_flush),
};
static HOST_STATE_EXT: ClapHostState = ClapHostState {
    mark_dirty: Some(host_state_mark_dirty),
};
static HOST_NOTE_NAME_EXT: ClapHostNoteName = ClapHostNoteName {
    changed: Some(host_note_name_changed),
};
static HOST_AUDIO_PORTS_EXT: ClapHostAudioPorts = ClapHostAudioPorts {
    is_rescan_flag_supported: Some(host_audio_ports_is_rescan_flag_supported),
    rescan: Some(host_audio_ports_rescan),
};
static HOST_NOTE_PORTS_EXT: ClapHostNotePorts = ClapHostNotePorts {
    supported_dialects: Some(host_note_ports_supported_dialects),
    rescan: Some(host_note_ports_rescan),
};
static NEXT_TIMER_ID: AtomicU32 = AtomicU32::new(1);

fn host_runtime_state(host: *const ClapHost) -> Option<&'static HostRuntimeState> {
    if host.is_null() {
        return None;
    }
    let state_ptr = unsafe { (*host).host_data as *const HostRuntimeState };
    if state_ptr.is_null() {
        return None;
    }
    Some(unsafe { &*state_ptr })
}

unsafe extern "C" fn host_get_extension(
    _host: *const ClapHost,
    _extension_id: *const c_char,
) -> *const c_void {
    if _extension_id.is_null() {
        return std::ptr::null();
    }

    let id = unsafe { CStr::from_ptr(_extension_id) }.to_string_lossy();
    match id.as_ref() {
        "clap.host.thread-check" => {
            (&HOST_THREAD_CHECK_EXT as *const ClapHostThreadCheck).cast::<c_void>()
        }
        "clap.host.latency" => (&HOST_LATENCY_EXT as *const ClapHostLatency).cast::<c_void>(),
        "clap.host.tail" => (&HOST_TAIL_EXT as *const ClapHostTail).cast::<c_void>(),
        "clap.host.timer-support" => {
            (&HOST_TIMER_EXT as *const ClapHostTimerSupport).cast::<c_void>()
        }
        "clap.host.gui" => host_runtime_state(_host)
            .filter(|state| state.ui_active.load(Ordering::Acquire) != 0)
            .map(|_| (&HOST_GUI_EXT as *const ClapHostGui).cast::<c_void>())
            .unwrap_or(std::ptr::null()),
        "clap.host.params" => host_runtime_state(_host)
            .filter(|state| state.ui_active.load(Ordering::Acquire) != 0)
            .map(|_| (&HOST_PARAMS_EXT as *const ClapHostParams).cast::<c_void>())
            .unwrap_or(std::ptr::null()),
        "clap.host.state" => host_runtime_state(_host)
            .filter(|state| state.ui_active.load(Ordering::Acquire) != 0)
            .map(|_| (&HOST_STATE_EXT as *const ClapHostState).cast::<c_void>())
            .unwrap_or(std::ptr::null()),
        "clap.host.note-name" => (&HOST_NOTE_NAME_EXT as *const ClapHostNoteName).cast::<c_void>(),
        "clap.host.audio-ports" => {
            (&HOST_AUDIO_PORTS_EXT as *const ClapHostAudioPorts).cast::<c_void>()
        }
        "clap.host.note-ports" => {
            (&HOST_NOTE_PORTS_EXT as *const ClapHostNotePorts).cast::<c_void>()
        }
        _ => std::ptr::null(),
    }
}

unsafe extern "C" fn host_request_process(_host: *const ClapHost) {
    if let Some(state) = host_runtime_state(_host) {
        state.callback_flags.lock().process = true;
    }
}

unsafe extern "C" fn host_request_callback(_host: *const ClapHost) {
    if let Some(state) = host_runtime_state(_host) {
        state.callback_flags.lock().callback = true;
    }
}

unsafe extern "C" fn host_request_restart(_host: *const ClapHost) {
    if let Some(state) = host_runtime_state(_host) {
        state.callback_flags.lock().restart = true;
    }
}

unsafe extern "C" fn host_audio_ports_is_rescan_flag_supported(
    _host: *const ClapHost,
    _flag: u32,
) -> bool {
    true
}

unsafe extern "C" fn host_audio_ports_rescan(_host: *const ClapHost, _flags: u32) {
    if let Some(state) = host_runtime_state(_host) {
        state
            .audio_ports_rescan_requested
            .store(1, Ordering::Release);
    }
}

unsafe extern "C" fn host_note_ports_rescan(_host: *const ClapHost, _flags: u32) {
    if let Some(state) = host_runtime_state(_host) {
        state
            .note_ports_rescan_requested
            .store(1, Ordering::Release);
    }
}

unsafe extern "C" fn host_note_ports_supported_dialects(_host: *const ClapHost) -> u32 {
    let _ = _host;
    0x1F
}

unsafe extern "C" fn host_note_name_changed(_host: *const ClapHost) {
    if let Some(state) = host_runtime_state(_host) {
        state.note_names_dirty.store(1, Ordering::Release);
    }
}

unsafe extern "C" fn host_is_main_thread(_host: *const ClapHost) -> bool {
    true
}

unsafe extern "C" fn host_is_audio_thread(_host: *const ClapHost) -> bool {
    false
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
    if let Some(state) = host_runtime_state(_host) {
        state.timers.lock().push(HostTimer { id });
    }

    unsafe {
        *timer_id = id;
    }
    true
}

unsafe extern "C" fn host_timer_unregister(_host: *const ClapHost, _timer_id: u32) -> bool {
    if let Some(state) = host_runtime_state(_host) {
        state.timers.lock().retain(|timer| timer.id != _timer_id);
    }
    true
}

unsafe extern "C" fn host_gui_resize_hints_changed(_host: *const ClapHost) {}

unsafe extern "C" fn host_gui_request_resize(
    _host: *const ClapHost,
    _width: u32,
    _height: u32,
) -> bool {
    true
}

unsafe extern "C" fn host_gui_request_show(_host: *const ClapHost) -> bool {
    true
}

unsafe extern "C" fn host_gui_request_hide(_host: *const ClapHost) -> bool {
    if let Some(state) = host_runtime_state(_host) {
        if state.ui_active.load(Ordering::Acquire) != 0 {
            state.ui_should_close.store(1, Ordering::Release);
        }
        true
    } else {
        false
    }
}

unsafe extern "C" fn host_gui_closed(_host: *const ClapHost, _was_destroyed: bool) {
    if let Some(state) = host_runtime_state(_host)
        && state.ui_active.load(Ordering::Acquire) != 0
    {
        state.ui_should_close.store(1, Ordering::Release);
    }
}

unsafe extern "C" fn host_params_rescan(_host: *const ClapHost, _flags: u32) {}

unsafe extern "C" fn host_params_clear(_host: *const ClapHost, _param_id: u32, _flags: u32) {}

unsafe extern "C" fn host_params_request_flush(_host: *const ClapHost) {
    if let Some(state) = host_runtime_state(_host) {
        state.param_flush_requested.store(1, Ordering::Release);
        state.callback_flags.lock().callback = true;
    }
}

unsafe extern "C" fn host_state_mark_dirty(_host: *const ClapHost) {
    if let Some(state) = host_runtime_state(_host) {
        state.state_dirty_requested.store(1, Ordering::Release);
        state.callback_flags.lock().callback = true;
    }
}

pub fn list_plugins() -> Vec<ClapPluginInfo> {
    list_plugins_with_capabilities(false)
}

pub fn is_supported_clap_binary(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("clap"))
}

pub fn list_plugins_with_capabilities(scan_capabilities: bool) -> Vec<ClapPluginInfo> {
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
        collect_clap_plugins(&root, &mut out, scan_capabilities);
    }

    out.sort_by_key(|a| a.name.to_lowercase());
    out.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name) && a.path.eq_ignore_ascii_case(&b.path));
    out
}

fn collect_clap_plugins(root: &Path, out: &mut Vec<ClapPluginInfo>, scan_capabilities: bool) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        if ft.is_dir() {
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    matches!(
                        name,
                        "deps" | "build" | "incremental" | ".fingerprint" | "examples"
                    )
                })
            {
                continue;
            }
            collect_clap_plugins(&path, out, scan_capabilities);
            continue;
        }

        if is_supported_clap_binary(&path) {
            let infos = scan_bundle_descriptors(&path, scan_capabilities);
            if infos.is_empty() {
                let name = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.to_string_lossy().to_string());
                out.push(ClapPluginInfo {
                    name,
                    path: path.to_string_lossy().to_string(),
                    capabilities: None,
                });
            } else {
                out.extend(infos);
            }
        }
    }
}

fn scan_bundle_descriptors(path: &Path, scan_capabilities: bool) -> Vec<ClapPluginInfo> {
    let path_str = path.to_string_lossy().to_string();
    let factory_id = c"clap.plugin-factory";
    let mut host_runtime_state = Box::new(HostRuntimeState {
        callback_flags: UnsafeMutex::new(HostCallbackFlags::default()),
        timers: UnsafeMutex::new(Vec::new()),
        ui_should_close: AtomicU32::new(0),
        ui_active: AtomicU32::new(0),
        param_flush_requested: AtomicU32::new(0),
        state_dirty_requested: AtomicU32::new(0),
        note_names_dirty: AtomicU32::new(0),
        audio_ports_rescan_requested: AtomicU32::new(0),
        note_ports_rescan_requested: AtomicU32::new(0),
    });
    let host_runtime = ClapHost {
        clap_version: CLAP_VERSION,
        host_data: (&mut *host_runtime_state as *mut HostRuntimeState).cast::<c_void>(),
        name: c"Maolan".as_ptr(),
        vendor: c"Maolan".as_ptr(),
        url: c"https://example.invalid".as_ptr(),
        version: c"0.0.1".as_ptr(),
        get_extension: Some(host_get_extension),
        request_restart: Some(host_request_restart),
        request_process: Some(host_request_process),
        request_callback: Some(host_request_callback),
    };

    let library = match unsafe { Library::new(path) } {
        Ok(lib) => lib,
        Err(_) => return Vec::new(),
    };

    let entry_ptr = unsafe {
        match library.get::<*const ClapPluginEntry>(b"clap_entry\0") {
            Ok(sym) => *sym,
            Err(_) => return Vec::new(),
        }
    };
    if entry_ptr.is_null() {
        return Vec::new();
    }

    let entry = unsafe { &*entry_ptr };
    let Some(init) = entry.init else {
        return Vec::new();
    };
    let host_ptr = &host_runtime;

    if unsafe { !init(host_ptr) } {
        return Vec::new();
    }
    let mut out = Vec::new();
    if let Some(get_factory) = entry.get_factory {
        let factory = unsafe { get_factory(factory_id.as_ptr()) } as *const ClapPluginFactory;
        if !factory.is_null() {
            let factory_ref = unsafe { &*factory };
            if let (Some(get_count), Some(get_desc)) = (
                factory_ref.get_plugin_count,
                factory_ref.get_plugin_descriptor,
            ) {
                let count = unsafe { get_count(factory) };
                for i in 0..count {
                    let desc = unsafe { get_desc(factory, i) };
                    if desc.is_null() {
                        continue;
                    }

                    let desc = unsafe { &*desc };
                    if desc.id.is_null() || desc.name.is_null() {
                        continue;
                    }

                    let id = unsafe { CStr::from_ptr(desc.id) }
                        .to_string_lossy()
                        .to_string();

                    let name = unsafe { CStr::from_ptr(desc.name) }
                        .to_string_lossy()
                        .to_string();

                    let capabilities = if scan_capabilities {
                        scan_plugin_capabilities(factory_ref, factory, &host_runtime, &id)
                    } else {
                        None
                    };

                    out.push(ClapPluginInfo {
                        name,
                        path: format!("{path_str}::{id}"),
                        capabilities,
                    });
                }
            }
        }
    }

    if let Some(deinit) = entry.deinit {
        unsafe { deinit() };
    }
    out
}

fn scan_plugin_capabilities(
    factory: &ClapPluginFactory,
    factory_ptr: *const ClapPluginFactory,
    host: &ClapHost,
    plugin_id: &str,
) -> Option<ClapPluginCapabilities> {
    let create = factory.create_plugin?;

    let id_cstring = CString::new(plugin_id).ok()?;

    let plugin = unsafe { create(factory_ptr, host, id_cstring.as_ptr()) };
    if plugin.is_null() {
        return None;
    }

    let plugin_ref = unsafe { &*plugin };
    let plugin_init = plugin_ref.init?;

    if unsafe { !plugin_init(plugin) } {
        return None;
    }

    let mut capabilities = ClapPluginCapabilities {
        has_gui: false,
        gui_apis: Vec::new(),
        supports_embedded: false,
        supports_floating: false,
        has_params: false,
        has_state: false,
        audio_inputs: 0,
        audio_outputs: 0,
        midi_inputs: 0,
        midi_outputs: 0,
    };

    if let Some(get_extension) = plugin_ref.get_extension {
        let gui_ext_id = c"clap.gui";

        let gui_ptr = unsafe { get_extension(plugin, gui_ext_id.as_ptr()) };
        if !gui_ptr.is_null() {
            capabilities.has_gui = true;

            let gui = unsafe { &*(gui_ptr as *const ClapPluginGui) };

            if let Some(is_api_supported) = gui.is_api_supported {
                for api in ["x11", "cocoa"] {
                    if let Ok(api_cstr) = CString::new(api) {
                        if unsafe { is_api_supported(plugin, api_cstr.as_ptr(), false) } {
                            capabilities.gui_apis.push(format!("{} (embedded)", api));
                            capabilities.supports_embedded = true;
                        }

                        if unsafe { is_api_supported(plugin, api_cstr.as_ptr(), true) } {
                            if !capabilities.supports_embedded {
                                capabilities.gui_apis.push(format!("{} (floating)", api));
                            }
                            capabilities.supports_floating = true;
                        }
                    }
                }
            }
        }

        let params_ext_id = c"clap.params";

        let params_ptr = unsafe { get_extension(plugin, params_ext_id.as_ptr()) };
        capabilities.has_params = !params_ptr.is_null();

        let state_ext_id = c"clap.state";

        let state_ptr = unsafe { get_extension(plugin, state_ext_id.as_ptr()) };
        capabilities.has_state = !state_ptr.is_null();

        let audio_ports_ext_id = c"clap.audio-ports";

        let audio_ports_ptr = unsafe { get_extension(plugin, audio_ports_ext_id.as_ptr()) };
        if !audio_ports_ptr.is_null() {
            let audio_ports = unsafe { &*(audio_ports_ptr as *const ClapPluginAudioPorts) };
            if let Some(count_fn) = audio_ports.count {
                capabilities.audio_inputs = unsafe { count_fn(plugin, true) } as usize;

                capabilities.audio_outputs = unsafe { count_fn(plugin, false) } as usize;
            }
        }

        let note_ports_ext_id = c"clap.note-ports";

        let note_ports_ptr = unsafe { get_extension(plugin, note_ports_ext_id.as_ptr()) };
        if !note_ports_ptr.is_null() {
            let note_ports = unsafe { &*(note_ports_ptr as *const ClapPluginNotePorts) };
            if let Some(count_fn) = note_ports.count {
                capabilities.midi_inputs = unsafe { count_fn(plugin, true) } as usize;

                capabilities.midi_outputs = unsafe { count_fn(plugin, false) } as usize;
            }
        }
    }

    if let Some(destroy) = plugin_ref.destroy {
        unsafe { destroy(plugin) };
    }

    Some(capabilities)
}

fn default_clap_search_roots() -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let mut roots = Vec::new();
        paths::push_macos_audio_plugin_roots(&mut roots, "CLAP");
        roots
    }
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
    {
        let mut roots = Vec::new();
        paths::push_unix_plugin_roots(&mut roots, "clap");
        roots
    }
    #[cfg(not(any(
        target_os = "macos",
        target_os = "linux",
        target_os = "freebsd",
        target_os = "openbsd"
    )))]
    {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use super::collect_clap_plugins;
    #[cfg(unix)]
    use std::fs;
    #[cfg(unix)]
    use std::path::PathBuf;
    #[cfg(unix)]
    use std::time::{SystemTime, UNIX_EPOCH};

    #[cfg(unix)]
    fn make_symlink(src: &PathBuf, dst: &PathBuf) {
        std::os::unix::fs::symlink(src, dst).expect("should create symlink");
    }

    #[cfg(unix)]
    #[test]
    fn collect_clap_plugins_includes_symlinked_clap_files() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be valid")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "maolan-clap-symlink-test-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("should create temp dir");

        let target_file = root.join("librural_modeler.so");
        fs::write(&target_file, b"not a real clap binary").expect("should create target file");
        let clap_link = root.join("RuralModeler.clap");
        make_symlink(&PathBuf::from("librural_modeler.so"), &clap_link);

        let mut out = Vec::new();
        collect_clap_plugins(&root, &mut out, false);

        assert!(
            out.iter()
                .any(|info| info.path == clap_link.to_string_lossy()),
            "scanner should include symlinked .clap files"
        );

        fs::remove_dir_all(&root).expect("should remove temp dir");
    }
}
