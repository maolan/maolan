use std::cell::Cell;
use std::ffi::{CStr, CString, c_char, c_ulong, c_void};
use std::path::Path;
use std::ptr;
use std::sync::{
    Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq)]
pub enum ThreadType {
    MainThread,
    AudioThread,
    AudioThreadPool,
}

thread_local! {
    static CURRENT_THREAD: Cell<ThreadType> = const { Cell::new(ThreadType::MainThread) };
}

pub fn set_thread_type(ty: ThreadType) {
    CURRENT_THREAD.with(|t| t.set(ty));
}

pub fn current_thread_type() -> ThreadType {
    CURRENT_THREAD.with(|t| t.get())
}

pub struct HostTimer {
    pub id: u32,
    pub period_ms: u32,
    pub deadline: Instant,
}

pub struct HostFd {
    pub fd: i32,
    pub flags: u32,
}

pub fn host_timers() -> &'static Mutex<Vec<HostTimer>> {
    static TIMERS: OnceLock<Mutex<Vec<HostTimer>>> = OnceLock::new();
    TIMERS.get_or_init(|| Mutex::new(Vec::new()))
}

pub fn host_fds() -> &'static Mutex<Vec<HostFd>> {
    static FDS: OnceLock<Mutex<Vec<HostFd>>> = OnceLock::new();
    FDS.get_or_init(|| Mutex::new(Vec::new()))
}

static HOST_GUI_CLOSED_REQUESTED: AtomicBool = AtomicBool::new(false);

pub fn take_host_gui_closed_requested() -> bool {
    HOST_GUI_CLOSED_REQUESTED.swap(false, Ordering::AcqRel)
}

pub fn next_timer_id() -> u32 {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT: AtomicU32 = AtomicU32::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ClapVersion {
    pub major: u32,
    pub minor: u32,
    pub revision: u32,
}

pub const CLAP_VERSION: ClapVersion = ClapVersion {
    major: 1,
    minor: 2,
    revision: 5,
};

#[repr(C)]
pub struct ClapHost {
    pub clap_version: ClapVersion,
    pub host_data: *mut c_void,
    pub name: *const c_char,
    pub vendor: *const c_char,
    pub url: *const c_char,
    pub version: *const c_char,
    pub get_extension:
        Option<unsafe extern "C" fn(*const ClapHost, *const c_char) -> *const c_void>,
    pub request_restart: Option<unsafe extern "C" fn(*const ClapHost)>,
    pub request_process: Option<unsafe extern "C" fn(*const ClapHost)>,
    pub request_callback: Option<unsafe extern "C" fn(*const ClapHost)>,
}

#[repr(C)]
pub struct ClapPluginEntry {
    pub clap_version: ClapVersion,
    pub init: Option<unsafe extern "C" fn(*const c_char) -> bool>,
    pub deinit: Option<unsafe extern "C" fn()>,
    pub get_factory: Option<unsafe extern "C" fn(*const c_char) -> *const c_void>,
}

#[repr(C)]
pub struct ClapPluginFactory {
    pub get_plugin_count: Option<unsafe extern "C" fn(*const ClapPluginFactory) -> u32>,
    pub get_plugin_descriptor:
        Option<unsafe extern "C" fn(*const ClapPluginFactory, u32) -> *const ClapPluginDescriptor>,
    pub create_plugin: Option<
        unsafe extern "C" fn(
            *const ClapPluginFactory,
            *const ClapHost,
            *const c_char,
        ) -> *const ClapPlugin,
    >,
}

#[repr(C)]
pub struct ClapPluginDescriptor {
    pub clap_version: ClapVersion,
    pub id: *const c_char,
    pub name: *const c_char,
    pub vendor: *const c_char,
    pub url: *const c_char,
    pub manual_url: *const c_char,
    pub support_url: *const c_char,
    pub version: *const c_char,
    pub description: *const c_char,
    pub features: *const *const c_char,
}

#[repr(C)]
pub struct ClapPlugin {
    pub desc: *const ClapPluginDescriptor,
    pub plugin_data: *mut c_void,
    pub init: Option<unsafe extern "C" fn(*const ClapPlugin) -> bool>,
    pub destroy: Option<unsafe extern "C" fn(*const ClapPlugin)>,
    pub activate: Option<unsafe extern "C" fn(*const ClapPlugin, f64, u32, u32) -> bool>,
    pub deactivate: Option<unsafe extern "C" fn(*const ClapPlugin)>,
    pub start_processing: Option<unsafe extern "C" fn(*const ClapPlugin) -> bool>,
    pub stop_processing: Option<unsafe extern "C" fn(*const ClapPlugin)>,
    pub reset: Option<unsafe extern "C" fn(*const ClapPlugin)>,
    pub process: Option<unsafe extern "C" fn(*const ClapPlugin, *const ClapProcess) -> i32>,
    pub get_extension:
        Option<unsafe extern "C" fn(*const ClapPlugin, *const c_char) -> *const c_void>,
    pub on_main_thread: Option<unsafe extern "C" fn(*const ClapPlugin)>,
}

#[repr(C)]
pub struct ClapProcess {
    pub steady_time: i64,
    pub frames_count: u32,
    pub transport: *const c_void,
    pub audio_inputs: *const ClapAudioBuffer,
    pub audio_outputs: *mut ClapAudioBuffer,
    pub audio_inputs_count: u32,
    pub audio_outputs_count: u32,
    pub in_events: *const ClapInputEvents,
    pub out_events: *const ClapOutputEvents,
}

#[repr(C)]
pub struct ClapAudioBuffer {
    pub data32: *mut *mut f32,
    pub data64: *mut *mut f64,
    pub channel_count: u32,
    pub latency: u32,
    pub constant_mask: u64,
}

#[repr(C)]
pub struct ClapInputEvents {
    pub ctx: *const c_void,
    pub size: Option<unsafe extern "C" fn(*const ClapInputEvents) -> u32>,
    pub get: Option<unsafe extern "C" fn(*const ClapInputEvents, u32) -> *const ClapEventHeader>,
}

#[repr(C)]
pub struct ClapOutputEvents {
    pub ctx: *mut c_void,
    pub try_push:
        Option<unsafe extern "C" fn(*const ClapOutputEvents, *const ClapEventHeader) -> bool>,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ClapEventHeader {
    pub size: u32,
    pub time: u32,
    pub space_id: u16,
    pub type_: u16,
    pub flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ClapEventParamValue {
    pub header: ClapEventHeader,
    pub param_id: u32,
    pub cookie: *mut c_void,
    pub note_id: i32,
    pub port_index: i16,
    pub channel: i16,
    pub key: i16,
    pub value: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ClapEventParamMod {
    pub header: ClapEventHeader,
    pub param_id: u32,
    pub cookie: *mut c_void,
    pub note_id: i32,
    pub port_index: i16,
    pub channel: i16,
    pub key: i16,
    pub amount: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ClapEventParamGesture {
    pub header: ClapEventHeader,
    pub param_id: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ClapEventMidi {
    pub header: ClapEventHeader,
    pub port_index: u16,
    pub data: [u8; 3],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ClapEventNote {
    pub header: ClapEventHeader,
    pub note_id: i32,
    pub port_index: i16,
    pub channel: i16,
    pub key: i16,
    pub velocity: f64,
}

pub const CLAP_CORE_EVENT_SPACE_ID: u16 = 0;

pub const CLAP_EVENT_NOTE_ON: u16 = 0;
pub const CLAP_EVENT_NOTE_OFF: u16 = 1;
pub const CLAP_EVENT_NOTE_CHOKE: u16 = 2;
pub const CLAP_EVENT_NOTE_END: u16 = 3;
pub const CLAP_EVENT_NOTE_EXPRESSION: u16 = 4;
pub const CLAP_EVENT_PARAM_VALUE: u16 = 5;
pub const CLAP_EVENT_PARAM_MOD: u16 = 6;
pub const CLAP_EVENT_PARAM_GESTURE_BEGIN: u16 = 7;
pub const CLAP_EVENT_PARAM_GESTURE_END: u16 = 8;
pub const CLAP_EVENT_TRANSPORT: u16 = 9;
pub const CLAP_EVENT_MIDI: u16 = 10;
pub const CLAP_EVENT_MIDI_SYSEX: u16 = 11;
pub const CLAP_EVENT_MIDI2: u16 = 12;

#[repr(C)]
pub struct ClapHostParams {
    pub resize: Option<unsafe extern "C" fn(*const ClapHost, u32) -> bool>,
    pub clear: Option<unsafe extern "C" fn(*const ClapHost, u32, u32)>,
    pub request_flush: Option<unsafe extern "C" fn(*const ClapHost)>,
}

#[repr(C)]
pub struct ClapHostAudioPorts {
    pub is_rescan_flag_supported: Option<unsafe extern "C" fn(*const ClapHost, u32) -> bool>,
    pub rescan: Option<unsafe extern "C" fn(*const ClapHost, u32)>,
}

#[repr(C)]
pub struct ClapHostLatency {
    pub changed: Option<unsafe extern "C" fn(*const ClapHost)>,
}

#[repr(C)]
pub struct ClapHostThreadPool {
    pub request_exec: Option<unsafe extern "C" fn(*const ClapHost, u32) -> bool>,
}

#[repr(C)]
pub struct ClapHostGui {
    pub resize_hints_changed: Option<unsafe extern "C" fn(*const ClapHost)>,
    pub request_resize: Option<unsafe extern "C" fn(*const ClapHost, u32, u32) -> bool>,
    pub request_show: Option<unsafe extern "C" fn(*const ClapHost) -> bool>,
    pub request_hide: Option<unsafe extern "C" fn(*const ClapHost) -> bool>,
    pub closed: Option<unsafe extern "C" fn(*const ClapHost, bool)>,
}

#[repr(C)]
pub struct ClapHostThreadCheck {
    pub is_main_thread: Option<unsafe extern "C" fn(*const ClapHost) -> bool>,
    pub is_audio_thread: Option<unsafe extern "C" fn(*const ClapHost) -> bool>,
}

#[repr(C)]
pub struct ClapHostLog {
    pub log: Option<unsafe extern "C" fn(*const ClapHost, u32, *const c_char)>,
}

#[repr(C)]
pub struct ClapHostTimerSupport {
    pub register_timer: Option<unsafe extern "C" fn(*const ClapHost, u32, *mut u32) -> bool>,
    pub unregister_timer: Option<unsafe extern "C" fn(*const ClapHost, u32) -> bool>,
}

#[repr(C)]
pub struct ClapHostPosixFdSupport {
    pub register_fd: Option<unsafe extern "C" fn(*const ClapHost, i32, u32) -> bool>,
    pub modify_fd: Option<unsafe extern "C" fn(*const ClapHost, i32, u32) -> bool>,
    pub unregister_fd: Option<unsafe extern "C" fn(*const ClapHost, i32) -> bool>,
}

#[repr(C)]
pub struct ClapOStream {
    pub ctx: *mut c_void,
    pub write: Option<unsafe extern "C" fn(*const ClapOStream, *const c_void, u64) -> i64>,
}

#[repr(C)]
pub struct ClapIStream {
    pub ctx: *mut c_void,
    pub read: Option<unsafe extern "C" fn(*const ClapIStream, *mut c_void, u64) -> i64>,
}

#[repr(C)]
pub struct ClapPluginState {
    pub save: Option<unsafe extern "C" fn(*const ClapPlugin, *const ClapOStream) -> bool>,
    pub load: Option<unsafe extern "C" fn(*const ClapPlugin, *const ClapIStream) -> bool>,
}

#[repr(C)]
pub struct ClapPluginParams {
    pub count: Option<unsafe extern "C" fn(*const ClapPlugin) -> u32>,
    pub get_info: Option<unsafe extern "C" fn(*const ClapPlugin, u32, *mut ClapParamInfo) -> bool>,
    pub get_value: Option<unsafe extern "C" fn(*const ClapPlugin, u32, *mut f64) -> bool>,
    pub value_to_text:
        Option<unsafe extern "C" fn(*const ClapPlugin, u32, f64, *mut c_char, u32) -> bool>,
    pub text_to_value:
        Option<unsafe extern "C" fn(*const ClapPlugin, u32, *const c_char, *mut f64) -> bool>,
    pub flush: Option<
        unsafe extern "C" fn(*const ClapPlugin, *const ClapInputEvents, *const ClapOutputEvents),
    >,
}

#[repr(C)]
pub struct ClapParamInfo {
    pub id: u32,
    pub flags: u32,
    pub cookie: *mut c_void,
    pub name: [c_char; 256],
    pub module: [c_char; 1024],
    pub min_value: f64,
    pub max_value: f64,
    pub default_value: f64,
}

#[repr(C)]
pub struct ClapPluginAudioPorts {
    pub count: Option<unsafe extern "C" fn(*const ClapPlugin, bool) -> u32>,
    pub get:
        Option<unsafe extern "C" fn(*const ClapPlugin, u32, bool, *mut ClapAudioPortInfo) -> bool>,
}

#[repr(C)]
pub struct ClapAudioPortInfo {
    pub id: u32,
    pub name: [c_char; 256],
    pub flags: u32,
    pub channel_count: u32,
    pub port_type: *const c_char,
    pub in_place_pair: u32,
}

#[repr(C)]
pub struct ClapPluginGui {
    pub is_api_supported:
        Option<unsafe extern "C" fn(*const ClapPlugin, *const c_char, bool) -> bool>,
    pub get_preferred_api:
        Option<unsafe extern "C" fn(*const ClapPlugin, *mut *const c_char, *mut bool) -> bool>,
    pub create: Option<unsafe extern "C" fn(*const ClapPlugin, *const c_char, bool) -> bool>,
    pub destroy: Option<unsafe extern "C" fn(*const ClapPlugin)>,
    pub set_scale: Option<unsafe extern "C" fn(*const ClapPlugin, f64) -> bool>,
    pub get_size: Option<unsafe extern "C" fn(*const ClapPlugin, *mut u32, *mut u32) -> bool>,
    pub can_resize: Option<unsafe extern "C" fn(*const ClapPlugin) -> bool>,
    pub get_resize_hints:
        Option<unsafe extern "C" fn(*const ClapPlugin, *mut ClapGuiResizeHints) -> bool>,
    pub adjust_size: Option<unsafe extern "C" fn(*const ClapPlugin, *mut u32, *mut u32) -> bool>,
    pub set_size: Option<unsafe extern "C" fn(*const ClapPlugin, u32, u32) -> bool>,
    pub set_parent: Option<unsafe extern "C" fn(*const ClapPlugin, *const ClapWindow) -> bool>,
    pub set_transient: Option<unsafe extern "C" fn(*const ClapPlugin, *const ClapWindow) -> bool>,
    pub suggest_title: Option<unsafe extern "C" fn(*const ClapPlugin, *const c_char)>,
    pub show: Option<unsafe extern "C" fn(*const ClapPlugin) -> bool>,
    pub hide: Option<unsafe extern "C" fn(*const ClapPlugin) -> bool>,
}

#[repr(C)]
pub struct ClapGuiResizeHints {
    pub can_resize_horizontally: bool,
    pub can_resize_vertically: bool,
    pub preserve_aspect_ratio: bool,
    pub aspect_ratio_width: u32,
    pub aspect_ratio_height: u32,
}

#[repr(C)]
pub struct ClapWindow {
    pub api: *const c_char,
    pub clap_window__: ClapWindowUnion,
}

#[repr(C)]
pub union ClapWindowUnion {
    pub x11: c_ulong,
    pub cocoa: *mut c_void,
    pub win32: *mut c_void,
}

pub const CLAP_EXT_PARAMS: &CStr = c"clap.params";
pub const CLAP_EXT_AUDIO_PORTS: &CStr = c"clap.audio-ports";
pub const CLAP_EXT_NOTE_PORTS: &CStr = c"clap.note-ports";
pub const CLAP_EXT_NOTE_NAME: &CStr = c"clap.note-name";
pub const CLAP_EXT_GUI: &CStr = c"clap.gui";
pub const CLAP_EXT_STATE: &CStr = c"clap.state";
pub const CLAP_EXT_FILE_REFERENCE: &CStr = c"clap.file-reference";
pub const CLAP_EXT_THREAD_POOL: &CStr = c"clap.thread-pool";
pub const CLAP_EXT_LATENCY: &CStr = c"clap.latency";
pub const CLAP_EXT_TAIL: &CStr = c"clap.tail";
pub const CLAP_EXT_TIMER_SUPPORT: &CStr = c"clap.timer-support";
pub const CLAP_EXT_THREAD_CHECK: &CStr = c"clap.thread-check";
pub const CLAP_EXT_LOG: &CStr = c"clap.log";
pub const CLAP_EXT_POSIX_FD_SUPPORT: &CStr = c"clap.posix-fd-support";
pub const CLAP_PORT_MONO: &str = "clap.mono";
pub const CLAP_PORT_STEREO: &str = "clap.stereo";

#[repr(C)]
pub struct ClapPluginThreadPool {
    pub exec: Option<unsafe extern "C" fn(*const ClapPlugin, u32)>,
}

#[repr(C)]
pub struct ClapPluginTimerSupport {
    pub on_timer: Option<unsafe extern "C" fn(*const ClapPlugin, u32)>,
}

#[repr(C)]
pub struct ClapPluginPosixFdSupport {
    pub on_fd: Option<unsafe extern "C" fn(*const ClapPlugin, i32, u32)>,
}

#[repr(C)]
pub struct ClapPluginNotePorts {
    pub count: Option<unsafe extern "C" fn(*const ClapPlugin, bool) -> u32>,
    pub get:
        Option<unsafe extern "C" fn(*const ClapPlugin, u32, bool, *mut ClapNotePortInfo) -> bool>,
}

#[repr(C)]
pub struct ClapNotePortInfo {
    pub id: u32,
    pub name: [c_char; 256],
    pub flags: u32,
    pub supported_dialects: u16,
    pub preferred_dialect: u16,
}

pub const CLAP_NAME_SIZE: usize = 256;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ClapNoteName {
    pub name: [c_char; CLAP_NAME_SIZE],
    pub port: i16,
    pub key: i16,
    pub channel: i16,
}

#[repr(C)]
pub struct ClapPluginNoteName {
    pub count: Option<unsafe extern "C" fn(*const ClapPlugin) -> u32>,
    pub get: Option<unsafe extern "C" fn(*const ClapPlugin, u32, *mut ClapNoteName) -> bool>,
}

#[repr(C)]
pub struct ClapHostState {
    pub mark_dirty: Option<unsafe extern "C" fn(*const ClapHost)>,
}

#[repr(C)]
pub struct ClapPluginFileReference {
    pub count: Option<unsafe extern "C" fn(*const ClapPlugin) -> u32>,
    pub get: Option<
        unsafe extern "C" fn(
            *const ClapPlugin,
            index: u32,
            path: *mut c_char,
            path_size: u32,
        ) -> bool,
    >,
    pub get_hash:
        Option<unsafe extern "C" fn(*const ClapPlugin, index: u32, hash: *mut u8) -> bool>,
    pub update_path:
        Option<unsafe extern "C" fn(*const ClapPlugin, index: u32, path: *const c_char) -> bool>,
}

#[repr(C)]
pub struct ClapHostFileReference {
    pub changed: Option<unsafe extern "C" fn(*const ClapHost)>,
    pub set_path: Option<unsafe extern "C" fn(*const ClapHost, index: u32, path: *const c_char)>,
    pub set_copy_path:
        Option<unsafe extern "C" fn(*const ClapHost, index: u32, path: *const c_char)>,
}

#[repr(C)]
pub struct HostData {
    pub host: *mut ClapHost,
    pub plugin: *const ClapPlugin,
    pub header: *mut maolan_plugin_protocol::protocol::ShmHeader,
}

use libloading::Library;

#[derive(Clone, Debug)]
pub struct ParamInfo {
    pub id: u32,
    pub name: String,
    pub module: String,
    pub min_value: f64,
    pub max_value: f64,
    pub default_value: f64,
}

pub struct PluginInstance {
    _library: Library,
    entry: *const ClapPluginEntry,
    plugin: *const ClapPlugin,
    host: Box<ClapHost>,
    param_count: u32,
    gui: Option<*const ClapPluginGui>,
    gui_created: bool,
}

unsafe impl Send for PluginInstance {}

impl PluginInstance {
    pub fn new(plugin_path: &str, plugin_id: &str) -> Result<Self, String> {
        let path = Path::new(plugin_path);
        if !path.exists() {
            return Err(format!("plugin path does not exist: {plugin_path}"));
        }

        let library =
            unsafe { Library::new(path) }.map_err(|e| format!("failed to load library: {e}"))?;

        let entry: libloading::Symbol<*const ClapPluginEntry> = unsafe {
            library
                .get(b"clap_entry\0")
                .map_err(|e| format!("clap_entry not found: {e}"))?
        };

        let entry = unsafe { &**entry };

        if let Some(init) = entry.init {
            let plugin_path_c = CString::new(plugin_path).map_err(|e| e.to_string())?;
            if !unsafe { init(plugin_path_c.as_ptr()) } {
                return Err("clap_entry.init() failed".to_string());
            }
        }

        let factory = if let Some(get_factory) = entry.get_factory {
            let factory_id = CString::new("clap.plugin-factory").unwrap();
            let factory_ptr = unsafe { get_factory(factory_id.as_ptr()) };
            if factory_ptr.is_null() {
                return Err("clap.plugin-factory not found".to_string());
            }
            unsafe { &*(factory_ptr as *const ClapPluginFactory) }
        } else {
            return Err("clap_entry.get_factory is null".to_string());
        };

        let descriptor = if plugin_id.is_empty() {
            let count = factory
                .get_plugin_count
                .map(|f| unsafe { f(factory) })
                .unwrap_or(0);
            if count == 0 {
                return Err("plugin factory is empty".to_string());
            }
            factory
                .get_plugin_descriptor
                .and_then(|f| {
                    let d = unsafe { f(factory, 0) };
                    if d.is_null() { None } else { Some(d) }
                })
                .ok_or("get_plugin_descriptor returned null")?
        } else {
            let count = factory
                .get_plugin_count
                .map(|f| unsafe { f(factory) })
                .unwrap_or(0);
            let mut found = None;
            for i in 0..count {
                if let Some(desc) = factory
                    .get_plugin_descriptor
                    .map(|f| unsafe { f(factory, i) })
                {
                    if desc.is_null() {
                        continue;
                    }
                    let id = unsafe { CStr::from_ptr((*desc).id) };
                    if id.to_bytes() == plugin_id.as_bytes() {
                        found = Some(desc);
                        break;
                    }
                }
            }
            found.ok_or(format!("plugin id '{}' not found", plugin_id))?
        };

        let actual_id = unsafe { CStr::from_ptr((*descriptor).id) }
            .to_str()
            .map_err(|e| e.to_string())?;
        let plugin_id_c = CString::new(actual_id).map_err(|e| e.to_string())?;

        let mut host = Box::new(ClapHost {
            clap_version: CLAP_VERSION,
            host_data: ptr::null_mut(),
            name: c"maolan-plugin-host".as_ptr(),
            vendor: c"Maolan".as_ptr(),
            url: c"https://maolan.github.io".as_ptr(),
            version: c"0.1.0".as_ptr(),
            get_extension: Some(host_get_extension),
            request_restart: Some(host_request_restart),
            request_process: Some(host_request_process),
            request_callback: Some(host_request_callback),
        });

        let plugin = factory.create_plugin.ok_or("create_plugin is null")?;
        let plugin = unsafe { plugin(factory, &*host, plugin_id_c.as_ptr()) };
        if plugin.is_null() {
            if let Some(deinit) = entry.deinit {
                unsafe { deinit() };
            }
            return Err("create_plugin returned null".to_string());
        }

        let host_data = Box::into_raw(Box::new(HostData {
            host: &mut *host,
            plugin,
            header: std::ptr::null_mut(),
        }));
        host.host_data = host_data.cast::<c_void>();

        let init = unsafe { (*plugin).init }.ok_or("plugin.init is null")?;
        if !unsafe { init(plugin) } {
            unsafe {
                if let Some(destroy) = (*plugin).destroy {
                    destroy(plugin);
                }
            }
            if let Some(deinit) = entry.deinit {
                unsafe { deinit() };
            }
            unsafe {
                let _ = Box::from_raw(host_data);
            }
            return Err("plugin.init() returned false".to_string());
        }

        let param_count = unsafe {
            let params_ext = (*plugin)
                .get_extension
                .map(|f| f(plugin, CLAP_EXT_PARAMS.as_ptr()));
            if let Some(ext) = params_ext {
                if !ext.is_null() {
                    let params = &*(ext as *const ClapPluginParams);
                    params.count.map(|f| f(plugin)).unwrap_or(0)
                } else {
                    0
                }
            } else {
                0
            }
        };

        let gui = unsafe {
            let gui_ext = (*plugin)
                .get_extension
                .map(|f| f(plugin, CLAP_EXT_GUI.as_ptr()));
            gui_ext
                .filter(|p| !p.is_null())
                .map(|ext| ext as *const ClapPluginGui)
        };

        Ok(Self {
            _library: library,
            entry: entry as *const ClapPluginEntry,
            plugin,
            host,
            param_count,
            gui,
            gui_created: false,
        })
    }

    pub fn name(&self) -> String {
        unsafe {
            if let Some(desc) = (*self.plugin).desc.as_ref() {
                CStr::from_ptr(desc.name).to_string_lossy().into_owned()
            } else {
                "unknown".to_string()
            }
        }
    }

    pub fn plugin_ptr(&self) -> *const ClapPlugin {
        self.plugin
    }

    pub fn host_ptr(&self) -> *const ClapHost {
        self.host.as_ref() as *const _
    }

    pub fn activate(
        &self,
        sample_rate: f64,
        min_frames: u32,
        max_frames: u32,
    ) -> Result<(), String> {
        let activate = unsafe { (*self.plugin).activate }.ok_or("activate is null")?;
        if unsafe { activate(self.plugin, sample_rate, min_frames, max_frames) } {
            Ok(())
        } else {
            Err("plugin.activate() returned false".to_string())
        }
    }

    pub fn deactivate(&self) {
        if let Some(deactivate) = unsafe { (*self.plugin).deactivate } {
            unsafe { deactivate(self.plugin) };
        }
    }

    pub fn start_processing(&self) -> Result<(), String> {
        let start = unsafe { (*self.plugin).start_processing }.ok_or("start_processing is null")?;
        if unsafe { start(self.plugin) } {
            Ok(())
        } else {
            Err("plugin.start_processing() returned false".to_string())
        }
    }

    pub fn stop_processing(&self) {
        if let Some(stop) = unsafe { (*self.plugin).stop_processing } {
            unsafe { stop(self.plugin) };
        }
    }

    pub fn reset(&self) {
        if let Some(reset) = unsafe { (*self.plugin).reset } {
            unsafe { reset(self.plugin) };
        }
    }

    fn state_extension(&self) -> Result<*const ClapPluginState, String> {
        let ext = unsafe {
            (*self.plugin)
                .get_extension
                .map(|f| f(self.plugin, CLAP_EXT_STATE.as_ptr()))
        };
        match ext {
            Some(ptr) if !ptr.is_null() => Ok(ptr as *const ClapPluginState),
            _ => Err("Plugin does not support clap.state".to_string()),
        }
    }

    pub fn save_state(&self) -> Result<Vec<u8>, String> {
        let state = self.state_extension()?;
        let save = unsafe { (*state).save }.ok_or("clap.state.save is null")?;
        let mut bytes = Vec::new();
        let stream = ClapOStream {
            ctx: (&mut bytes as *mut Vec<u8>).cast(),
            write: Some(clap_ostream_write),
        };
        if unsafe { save(self.plugin, &stream) } {
            Ok(bytes)
        } else {
            Err("plugin clap.state.save returned false".to_string())
        }
    }

    pub fn load_state(&self, bytes: &[u8]) -> Result<(), String> {
        let state = self.state_extension()?;
        let load = unsafe { (*state).load }.ok_or("clap.state.load is null")?;
        let mut reader = ClapIStreamReader { bytes, offset: 0 };
        let stream = ClapIStream {
            ctx: (&mut reader as *mut ClapIStreamReader).cast(),
            read: Some(clap_istream_read),
        };
        if unsafe { load(self.plugin, &stream) } {
            Ok(())
        } else {
            Err("plugin clap.state.load returned false".to_string())
        }
    }

    fn file_reference_extension(&self) -> Result<*const ClapPluginFileReference, String> {
        let ext = unsafe {
            (*self.plugin)
                .get_extension
                .map(|f| f(self.plugin, CLAP_EXT_FILE_REFERENCE.as_ptr()))
        };
        match ext {
            Some(ptr) if !ptr.is_null() => {
                tracing::info!("Plugin supports clap.file-reference");
                Ok(ptr as *const ClapPluginFileReference)
            }
            _ => {
                tracing::info!("Plugin does not support clap.file-reference");
                Err("Plugin does not support clap.file-reference".to_string())
            }
        }
    }

    pub fn file_reference_count(&self) -> Result<u32, String> {
        let ext = self.file_reference_extension()?;
        let count = unsafe { (*ext).count }.ok_or("clap.file-reference.count is null")?;
        let n = unsafe { count(self.plugin) };
        tracing::info!(count = n, "clap.file-reference count");
        Ok(n)
    }

    pub fn file_references(
        &self,
    ) -> Result<Vec<maolan_plugin_protocol::protocol::FileReference>, String> {
        let ext = self.file_reference_extension()?;
        let count_fn = unsafe { (*ext).count }.ok_or("clap.file-reference.count is null")?;
        let get_fn = unsafe { (*ext).get }.ok_or("clap.file-reference.get is null")?;
        let count = unsafe { count_fn(self.plugin) };
        tracing::info!(count, "Enumerating clap.file-reference entries");
        let mut refs = Vec::with_capacity(count as usize);
        for index in 0..count {
            let mut buffer = vec![0i8; 2048];
            if unsafe { get_fn(self.plugin, index, buffer.as_mut_ptr(), buffer.len() as u32) } {
                let cstr = unsafe { CStr::from_ptr(buffer.as_ptr()) };
                if let Ok(s) = cstr.to_str() {
                    tracing::info!(index, path = %s, "Got clap.file-reference path");
                    refs.push((index, s.to_string()));
                }
            } else {
                tracing::warn!(index, "clap.file-reference get() returned false");
            }
        }
        Ok(refs)
    }

    pub fn update_file_reference_path(&self, index: u32, path: &str) -> Result<(), String> {
        let ext = self.file_reference_extension()?;
        let update =
            unsafe { (*ext).update_path }.ok_or("clap.file-reference.update_path is null")?;
        let path_c = CString::new(path).map_err(|e| e.to_string())?;
        if unsafe { update(self.plugin, index, path_c.as_ptr()) } {
            Ok(())
        } else {
            Err(format!(
                "plugin rejected file-reference path update for index {index}"
            ))
        }
    }

    pub fn process(&self, process: &ClapProcess) -> Result<(), String> {
        let process_fn = unsafe { (*self.plugin).process }.ok_or("process is null")?;
        let status = unsafe { process_fn(self.plugin, process) };

        if status == 4 {
            Err("plugin.process() returned CLAP_PROCESS_ERROR".to_string())
        } else {
            Ok(())
        }
    }

    pub fn param_count(&self) -> u32 {
        self.param_count
    }

    pub fn parameter_infos(&self) -> Vec<ParamInfo> {
        let mut infos = Vec::new();
        unsafe {
            let ext = (*self.plugin)
                .get_extension
                .map(|f| f(self.plugin, CLAP_EXT_PARAMS.as_ptr()));
            let Some(ptr) = ext.filter(|p| !p.is_null()) else {
                return infos;
            };
            let params = &*(ptr as *const ClapPluginParams);
            let count = params.count.map(|f| f(self.plugin)).unwrap_or(0);
            for pi in 0..count {
                let mut info = ClapParamInfo {
                    id: 0,
                    flags: 0,
                    cookie: ptr::null_mut(),
                    name: [0; 256],
                    module: [0; 1024],
                    min_value: 0.0,
                    max_value: 0.0,
                    default_value: 0.0,
                };
                if params
                    .get_info
                    .map(|f| f(self.plugin, pi, &mut info))
                    .unwrap_or(false)
                {
                    let name = CStr::from_ptr(info.name.as_ptr())
                        .to_string_lossy()
                        .into_owned();
                    let module = CStr::from_ptr(info.module.as_ptr())
                        .to_string_lossy()
                        .into_owned();
                    infos.push(ParamInfo {
                        id: info.id,
                        name,
                        module,
                        min_value: info.min_value,
                        max_value: info.max_value,
                        default_value: info.default_value,
                    });
                }
            }
        }
        infos
    }

    pub fn note_names(&self) -> std::collections::HashMap<u8, String> {
        let mut result = std::collections::HashMap::new();
        unsafe {
            let ext = (*self.plugin)
                .get_extension
                .map(|f| f(self.plugin, CLAP_EXT_NOTE_NAME.as_ptr()));
            let Some(ptr) = ext.filter(|p| !p.is_null()) else {
                return result;
            };
            let note_name = &*(ptr as *const ClapPluginNoteName);
            let count = note_name.count.map(|f| f(self.plugin)).unwrap_or(0);
            for ni in 0..count {
                let mut info = ClapNoteName {
                    name: [0; CLAP_NAME_SIZE],
                    port: -1,
                    key: -1,
                    channel: -1,
                };
                let ok = note_name
                    .get
                    .map(|f| f(self.plugin, ni, &mut info))
                    .unwrap_or(false);
                if !ok {
                    continue;
                }
                if !(0..=127).contains(&info.key) {
                    continue;
                }
                let name = CStr::from_ptr(info.name.as_ptr())
                    .to_string_lossy()
                    .into_owned();
                if name.is_empty() {
                    continue;
                }
                result.insert(info.key as u8, name);
            }
        }
        result
    }

    pub fn gui_is_supported(&self) -> bool {
        self.gui.is_some()
    }

    pub fn gui_is_api_supported(&self, api: &str, is_floating: bool) -> bool {
        let Some(gui) = self.gui else {
            return false;
        };
        let Some(is_api_supported) = (unsafe { (*gui).is_api_supported }) else {
            return false;
        };
        let Ok(api_c) = CString::new(api) else {
            return false;
        };
        unsafe { is_api_supported(self.plugin, api_c.as_ptr(), is_floating) }
    }

    pub fn gui_preferred_api(&self) -> Option<(String, bool)> {
        let gui = self.gui?;
        let get_preferred_api = unsafe { (*gui).get_preferred_api }?;
        let mut api_ptr = ptr::null();
        let mut is_floating = false;
        if unsafe { get_preferred_api(self.plugin, &mut api_ptr, &mut is_floating) }
            && !api_ptr.is_null()
        {
            let api = unsafe { CStr::from_ptr(api_ptr) }
                .to_str()
                .unwrap_or("x11")
                .to_string();
            Some((api, is_floating))
        } else {
            None
        }
    }

    pub fn gui_create(&mut self, api: &str, is_floating: bool) -> Result<(), String> {
        let gui = self.gui.ok_or("GUI extension not available")?;
        let create = unsafe { (*gui).create }.ok_or("gui.create is null")?;
        let api_c = CString::new(api).map_err(|e| e.to_string())?;
        if unsafe { create(self.plugin, api_c.as_ptr(), is_floating) } {
            self.gui_created = true;
            Ok(())
        } else {
            Err("plugin.gui.create() returned false".to_string())
        }
    }

    pub fn gui_set_scale(&self, scale: f64) -> Result<(), String> {
        let gui = self.gui.ok_or("GUI extension not available")?;
        let set_scale = unsafe { (*gui).set_scale }.ok_or("gui.set_scale is null")?;
        if unsafe { set_scale(self.plugin, scale) } {
            Ok(())
        } else {
            Err("plugin.gui.set_scale() returned false".to_string())
        }
    }

    pub fn gui_get_size(&self) -> Result<(u32, u32), String> {
        let gui = self.gui.ok_or("GUI extension not available")?;
        let get_size = unsafe { (*gui).get_size }.ok_or("gui.get_size is null")?;
        let mut width = 0u32;
        let mut height = 0u32;
        if unsafe { get_size(self.plugin, &mut width, &mut height) } {
            Ok((width, height))
        } else {
            Err("plugin.gui.get_size() returned false".to_string())
        }
    }

    pub fn gui_set_size(&self, width: u32, height: u32) -> Result<(), String> {
        let gui = self.gui.ok_or("GUI extension not available")?;
        let set_size = unsafe { (*gui).set_size }.ok_or("gui.set_size is null")?;
        if unsafe { set_size(self.plugin, width, height) } {
            Ok(())
        } else {
            Err("plugin.gui.set_size() returned false".to_string())
        }
    }

    pub fn gui_set_parent(&self, window_id: u64) -> Result<(), String> {
        let gui = self.gui.ok_or("GUI extension not available")?;
        let set_parent = unsafe { (*gui).set_parent }.ok_or("gui.set_parent is null")?;

        #[cfg(windows)]
        let window = {
            let api = c"win32".as_ptr();
            ClapWindow {
                api,
                clap_window__: ClapWindowUnion {
                    win32: window_id as *mut c_void,
                },
            }
        };

        #[cfg(all(unix, not(target_os = "macos")))]
        let window = {
            let api = c"x11".as_ptr();
            ClapWindow {
                api,
                clap_window__: ClapWindowUnion {
                    x11: window_id as c_ulong,
                },
            }
        };

        #[cfg(target_os = "macos")]
        let window = {
            let api = c"cocoa".as_ptr();
            ClapWindow {
                api,
                clap_window__: ClapWindowUnion {
                    cocoa: window_id as *mut c_void,
                },
            }
        };

        if unsafe { set_parent(self.plugin, &window) } {
            Ok(())
        } else {
            Err("plugin.gui.set_parent() returned false".to_string())
        }
    }

    pub fn gui_set_transient(&self, window_id: u64) -> Result<(), String> {
        let gui = self.gui.ok_or("GUI extension not available")?;
        let set_transient = unsafe { (*gui).set_transient }.ok_or("gui.set_transient is null")?;

        #[cfg(windows)]
        let window = {
            let api = c"win32".as_ptr();
            ClapWindow {
                api,
                clap_window__: ClapWindowUnion {
                    win32: window_id as *mut c_void,
                },
            }
        };

        #[cfg(all(unix, not(target_os = "macos")))]
        let window = {
            let api = c"x11".as_ptr();
            ClapWindow {
                api,
                clap_window__: ClapWindowUnion {
                    x11: window_id as c_ulong,
                },
            }
        };

        #[cfg(target_os = "macos")]
        let window = {
            let api = c"cocoa".as_ptr();
            ClapWindow {
                api,
                clap_window__: ClapWindowUnion {
                    cocoa: window_id as *mut c_void,
                },
            }
        };

        if unsafe { set_transient(self.plugin, &window) } {
            Ok(())
        } else {
            Err("plugin.gui.set_transient() returned false".to_string())
        }
    }

    pub fn gui_show(&self) -> Result<(), String> {
        let gui = self.gui.ok_or("GUI extension not available")?;
        let show = unsafe { (*gui).show }.ok_or("gui.show is null")?;
        if unsafe { show(self.plugin) } {
            Ok(())
        } else {
            Err("plugin.gui.show() returned false".to_string())
        }
    }

    pub fn gui_hide(&self) -> Result<(), String> {
        let gui = self.gui.ok_or("GUI extension not available")?;
        let hide = unsafe { (*gui).hide }.ok_or("gui.hide is null")?;
        if unsafe { hide(self.plugin) } {
            Ok(())
        } else {
            Err("plugin.gui.hide() returned false".to_string())
        }
    }

    pub fn gui_created(&self) -> bool {
        self.gui_created
    }

    pub fn gui_destroy(&mut self) {
        if self.gui_created {
            if let Some(gui) = self.gui
                && let Some(destroy) = unsafe { (*gui).destroy }
            {
                unsafe { destroy(self.plugin) };
            }
            self.gui_created = false;
        }
    }
}

struct ClapIStreamReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

unsafe extern "C" fn clap_ostream_write(
    stream: *const ClapOStream,
    buffer: *const c_void,
    size: u64,
) -> i64 {
    if stream.is_null() || buffer.is_null() {
        return -1;
    }
    let Some(size) = usize::try_from(size).ok() else {
        return -1;
    };
    let bytes = unsafe { &mut *((*stream).ctx as *mut Vec<u8>) };
    let src = unsafe { std::slice::from_raw_parts(buffer.cast::<u8>(), size) };
    bytes.extend_from_slice(src);
    size as i64
}

unsafe extern "C" fn clap_istream_read(
    stream: *const ClapIStream,
    buffer: *mut c_void,
    size: u64,
) -> i64 {
    if stream.is_null() || buffer.is_null() {
        return -1;
    }
    let Some(size) = usize::try_from(size).ok() else {
        return -1;
    };
    let reader = unsafe { &mut *((*stream).ctx as *mut ClapIStreamReader<'_>) };
    let available = reader.bytes.len().saturating_sub(reader.offset);
    let count = available.min(size);
    if count > 0 {
        let src = unsafe { reader.bytes.as_ptr().add(reader.offset) };
        unsafe { std::ptr::copy_nonoverlapping(src, buffer.cast::<u8>(), count) };
        reader.offset += count;
    }
    count as i64
}

impl Drop for PluginInstance {
    fn drop(&mut self) {
        self.gui_destroy();
        if let Some(destroy) = unsafe { (*self.plugin).destroy } {
            unsafe { destroy(self.plugin) };
        }

        if !self.host.host_data.is_null() {
            unsafe {
                let _ = Box::from_raw(self.host.host_data as *mut HostData);
            }
        }

        if let Some(deinit) = unsafe { (*self.entry).deinit } {
            unsafe { deinit() };
        }
    }
}

unsafe extern "C" fn host_get_extension(
    _host: *const ClapHost,
    id: *const c_char,
) -> *const c_void {
    let id = unsafe { CStr::from_ptr(id).to_bytes() };
    match id {
        b"clap.params" => &CLAP_HOST_PARAMS as *const _ as *const c_void,
        b"clap.audio-ports" => &CLAP_HOST_AUDIO_PORTS as *const _ as *const c_void,
        b"clap.latency" => &CLAP_HOST_LATENCY as *const _ as *const c_void,
        b"clap.thread-pool" => &CLAP_HOST_THREAD_POOL as *const _ as *const c_void,
        b"clap.host.gui" => &CLAP_HOST_GUI as *const _ as *const c_void,
        b"clap.thread-check" => &CLAP_HOST_THREAD_CHECK as *const _ as *const c_void,
        b"clap.log" => &CLAP_HOST_LOG as *const _ as *const c_void,
        b"clap.timer-support" => &CLAP_HOST_TIMER_SUPPORT as *const _ as *const c_void,
        b"clap.posix-fd-support" => &CLAP_HOST_POSIX_FD_SUPPORT as *const _ as *const c_void,
        b"clap.state" => &CLAP_HOST_STATE as *const _ as *const c_void,
        b"clap.file-reference" => &CLAP_HOST_FILE_REFERENCE as *const _ as *const c_void,
        _ => ptr::null(),
    }
}

unsafe extern "C" fn host_request_restart(_host: *const ClapHost) {}
unsafe extern "C" fn host_request_process(_host: *const ClapHost) {}
unsafe extern "C" fn host_request_callback(_host: *const ClapHost) {}

static CLAP_HOST_PARAMS: ClapHostParams = ClapHostParams {
    resize: Some(host_params_resize),
    clear: Some(host_params_clear),
    request_flush: Some(host_params_request_flush),
};

static CLAP_HOST_AUDIO_PORTS: ClapHostAudioPorts = ClapHostAudioPorts {
    is_rescan_flag_supported: Some(host_audio_ports_is_rescan_flag_supported),
    rescan: Some(host_audio_ports_rescan),
};

static CLAP_HOST_LATENCY: ClapHostLatency = ClapHostLatency {
    changed: Some(host_latency_changed),
};

static CLAP_HOST_THREAD_POOL: ClapHostThreadPool = ClapHostThreadPool {
    request_exec: Some(host_thread_pool_request_exec),
};

static CLAP_HOST_GUI: ClapHostGui = ClapHostGui {
    resize_hints_changed: Some(host_gui_resize_hints_changed),
    request_resize: Some(host_gui_request_resize),
    request_show: Some(host_gui_request_show),
    request_hide: Some(host_gui_request_hide),
    closed: Some(host_gui_closed),
};

static CLAP_HOST_THREAD_CHECK: ClapHostThreadCheck = ClapHostThreadCheck {
    is_main_thread: Some(host_thread_check_is_main_thread),
    is_audio_thread: Some(host_thread_check_is_audio_thread),
};

static CLAP_HOST_LOG: ClapHostLog = ClapHostLog {
    log: Some(host_log_log),
};

static CLAP_HOST_TIMER_SUPPORT: ClapHostTimerSupport = ClapHostTimerSupport {
    register_timer: Some(host_timer_support_register_timer),
    unregister_timer: Some(host_timer_support_unregister_timer),
};

static CLAP_HOST_POSIX_FD_SUPPORT: ClapHostPosixFdSupport = ClapHostPosixFdSupport {
    register_fd: Some(host_posix_fd_support_register_fd),
    modify_fd: Some(host_posix_fd_support_modify_fd),
    unregister_fd: Some(host_posix_fd_support_unregister_fd),
};

static CLAP_HOST_STATE: ClapHostState = ClapHostState {
    mark_dirty: Some(host_state_mark_dirty),
};

static CLAP_HOST_FILE_REFERENCE: ClapHostFileReference = ClapHostFileReference {
    changed: Some(host_file_reference_changed),
    set_path: Some(host_file_reference_set_path),
    set_copy_path: Some(host_file_reference_set_copy_path),
};

unsafe extern "C" fn host_params_resize(_host: *const ClapHost, _capacity: u32) -> bool {
    true
}
unsafe extern "C" fn host_params_clear(_host: *const ClapHost, _begin: u32, _end: u32) {}
unsafe extern "C" fn host_params_request_flush(_host: *const ClapHost) {
    crate::host::request_params_flush();
}
unsafe extern "C" fn host_audio_ports_is_rescan_flag_supported(
    _host: *const ClapHost,
    _flag: u32,
) -> bool {
    false
}
unsafe extern "C" fn host_audio_ports_rescan(_host: *const ClapHost, _flag: u32) {
    crate::host::request_audio_ports_rescan();
}
unsafe extern "C" fn host_latency_changed(_host: *const ClapHost) {}
unsafe extern "C" fn host_thread_pool_request_exec(host: *const ClapHost, num_tasks: u32) -> bool {
    if host.is_null() {
        return false;
    }
    let host_data = unsafe { &*((*host).host_data as *const HostData) };
    let plugin = host_data.plugin;
    if plugin.is_null() {
        return false;
    }

    let ext = unsafe {
        (*plugin)
            .get_extension
            .map(|f| f(plugin, CLAP_EXT_THREAD_POOL.as_ptr()))
    };
    let ext = match ext {
        Some(p) if !p.is_null() => p,
        _ => return false,
    };
    let tp = unsafe { &*(ext as *const ClapPluginThreadPool) };
    let Some(exec) = tp.exec else {
        return false;
    };

    for task_index in 0..num_tasks {
        unsafe { exec(plugin, task_index) };
    }
    true
}

unsafe extern "C" fn host_gui_resize_hints_changed(_host: *const ClapHost) {}
unsafe extern "C" fn host_gui_request_resize(
    _host: *const ClapHost,
    _width: u32,
    _height: u32,
) -> bool {
    false
}
unsafe extern "C" fn host_gui_request_show(_host: *const ClapHost) -> bool {
    false
}
unsafe extern "C" fn host_gui_request_hide(_host: *const ClapHost) -> bool {
    false
}
unsafe extern "C" fn host_gui_closed(_host: *const ClapHost, was_destroyed: bool) {
    let _ = was_destroyed;
    HOST_GUI_CLOSED_REQUESTED.store(true, Ordering::Release);
}

unsafe extern "C" fn host_state_mark_dirty(host: *const ClapHost) {
    tracing::info!("Plugin called clap_host_state.mark_dirty()");
    if host.is_null() {
        tracing::warn!("host_state_mark_dirty: host is null");
        return;
    }
    let host_data = unsafe { (*host).host_data as *mut HostData };
    if host_data.is_null() {
        tracing::warn!("host_state_mark_dirty: host_data is null");
        return;
    }
    let header = unsafe { (*host_data).header };
    if header.is_null() {
        tracing::warn!("host_state_mark_dirty: header is null");
        return;
    }
    unsafe {
        (*header)
            .state_dirty
            .store(1, std::sync::atomic::Ordering::Release);
    }
    tracing::info!("host_state_mark_dirty: set state_dirty=1");
}

unsafe extern "C" fn host_file_reference_changed(_host: *const ClapHost) {
    tracing::info!("Plugin called clap_host_file_reference.changed()");
}

unsafe extern "C" fn host_file_reference_set_path(
    _host: *const ClapHost,
    _index: u32,
    _path: *const c_char,
) {
    if _path.is_null() {
        return;
    }
    let path = unsafe { CStr::from_ptr(_path) }.to_string_lossy();
    tracing::info!(index = _index, path = %path, "Plugin called clap_host_file_reference.set_path()");
}

unsafe extern "C" fn host_file_reference_set_copy_path(
    _host: *const ClapHost,
    _index: u32,
    _path: *const c_char,
) {
    if _path.is_null() {
        return;
    }
    let path = unsafe { CStr::from_ptr(_path) }.to_string_lossy();
    tracing::info!(index = _index, path = %path, "Plugin called clap_host_file_reference.set_copy_path()");
}

unsafe extern "C" fn host_thread_check_is_main_thread(_host: *const ClapHost) -> bool {
    current_thread_type() == ThreadType::MainThread
}

unsafe extern "C" fn host_thread_check_is_audio_thread(_host: *const ClapHost) -> bool {
    matches!(
        current_thread_type(),
        ThreadType::AudioThread | ThreadType::AudioThreadPool
    )
}

unsafe extern "C" fn host_log_log(_host: *const ClapHost, severity: u32, msg: *const c_char) {
    if msg.is_null() {
        return;
    }
    let msg = unsafe { CStr::from_ptr(msg) }.to_string_lossy();
    match severity {
        0 => tracing::debug!(target: "clap_plugin", "{msg}"),
        1 => tracing::info!(target: "clap_plugin", "{msg}"),
        2 => tracing::warn!(target: "clap_plugin", "{msg}"),
        3..=5 => tracing::error!(target: "clap_plugin", "{msg}"),
        _ => tracing::info!(target: "clap_plugin", "{msg}"),
    }
}

unsafe extern "C" fn host_timer_support_register_timer(
    _host: *const ClapHost,
    period_ms: u32,
    timer_id: *mut u32,
) -> bool {
    if timer_id.is_null() {
        return false;
    }
    let id = next_timer_id();
    unsafe { *timer_id = id };
    let mut timers = host_timers().lock().unwrap();
    timers.push(HostTimer {
        id,
        period_ms,
        deadline: Instant::now() + Duration::from_millis(period_ms as u64),
    });
    true
}

unsafe extern "C" fn host_timer_support_unregister_timer(
    _host: *const ClapHost,
    timer_id: u32,
) -> bool {
    let mut timers = host_timers().lock().unwrap();
    if let Some(pos) = timers.iter().position(|t| t.id == timer_id) {
        timers.swap_remove(pos);
        true
    } else {
        false
    }
}

unsafe extern "C" fn host_posix_fd_support_register_fd(
    _host: *const ClapHost,
    fd: i32,
    flags: u32,
) -> bool {
    let mut fds = host_fds().lock().unwrap();
    if fds.iter().any(|f| f.fd == fd) {
        return false;
    }
    fds.push(HostFd { fd, flags });
    true
}

unsafe extern "C" fn host_posix_fd_support_modify_fd(
    _host: *const ClapHost,
    fd: i32,
    flags: u32,
) -> bool {
    let mut fds = host_fds().lock().unwrap();
    if let Some(f) = fds.iter_mut().find(|f| f.fd == fd) {
        f.flags = flags;
        true
    } else {
        false
    }
}

unsafe extern "C" fn host_posix_fd_support_unregister_fd(_host: *const ClapHost, fd: i32) -> bool {
    let mut fds = host_fds().lock().unwrap();
    if let Some(pos) = fds.iter().position(|f| f.fd == fd) {
        fds.swap_remove(pos);
        true
    } else {
        false
    }
}

unsafe extern "C" fn empty_input_events_size(_: *const ClapInputEvents) -> u32 {
    0
}

unsafe extern "C" fn empty_input_events_get(
    _: *const ClapInputEvents,
    _index: u32,
) -> *const ClapEventHeader {
    ptr::null()
}

unsafe extern "C" fn empty_output_events_try_push(
    _: *const ClapOutputEvents,
    _: *const ClapEventHeader,
) -> bool {
    false
}

pub fn empty_input_events() -> ClapInputEvents {
    ClapInputEvents {
        ctx: ptr::null(),
        size: Some(empty_input_events_size),
        get: Some(empty_input_events_get),
    }
}

pub fn empty_output_events() -> ClapOutputEvents {
    ClapOutputEvents {
        ctx: ptr::null_mut(),
        try_push: Some(empty_output_events_try_push),
    }
}

pub struct EventBuffer {
    events: Vec<Vec<u8>>,
}

impl Default for EventBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBuffer {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    pub fn push_param_value(&mut self, param_id: u32, value: f64, sample_offset: u32) {
        let ev = ClapEventParamValue {
            header: ClapEventHeader {
                size: std::mem::size_of::<ClapEventParamValue>() as u32,
                time: sample_offset,
                space_id: CLAP_CORE_EVENT_SPACE_ID,
                type_: CLAP_EVENT_PARAM_VALUE,
                flags: 0,
            },
            param_id,
            cookie: ptr::null_mut(),
            note_id: -1,
            port_index: -1,
            channel: -1,
            key: -1,
            value,
        };
        self.events.push(unsafe {
            std::slice::from_raw_parts(
                &ev as *const _ as *const u8,
                std::mem::size_of::<ClapEventParamValue>(),
            )
            .to_vec()
        });
    }

    pub fn push_param_mod(&mut self, param_id: u32, amount: f64, sample_offset: u32) {
        let ev = ClapEventParamMod {
            header: ClapEventHeader {
                size: std::mem::size_of::<ClapEventParamMod>() as u32,
                time: sample_offset,
                space_id: CLAP_CORE_EVENT_SPACE_ID,
                type_: CLAP_EVENT_PARAM_MOD,
                flags: 0,
            },
            param_id,
            cookie: ptr::null_mut(),
            note_id: -1,
            port_index: -1,
            channel: -1,
            key: -1,
            amount,
        };
        self.events.push(unsafe {
            std::slice::from_raw_parts(
                &ev as *const _ as *const u8,
                std::mem::size_of::<ClapEventParamMod>(),
            )
            .to_vec()
        });
    }

    pub fn push_param_gesture_begin(&mut self, param_id: u32, sample_offset: u32) {
        let ev = ClapEventParamGesture {
            header: ClapEventHeader {
                size: std::mem::size_of::<ClapEventParamGesture>() as u32,
                time: sample_offset,
                space_id: CLAP_CORE_EVENT_SPACE_ID,
                type_: CLAP_EVENT_PARAM_GESTURE_BEGIN,
                flags: 0,
            },
            param_id,
        };
        self.events.push(unsafe {
            std::slice::from_raw_parts(
                &ev as *const _ as *const u8,
                std::mem::size_of::<ClapEventParamGesture>(),
            )
            .to_vec()
        });
    }

    pub fn push_param_gesture_end(&mut self, param_id: u32, sample_offset: u32) {
        let ev = ClapEventParamGesture {
            header: ClapEventHeader {
                size: std::mem::size_of::<ClapEventParamGesture>() as u32,
                time: sample_offset,
                space_id: CLAP_CORE_EVENT_SPACE_ID,
                type_: CLAP_EVENT_PARAM_GESTURE_END,
                flags: 0,
            },
            param_id,
        };
        self.events.push(unsafe {
            std::slice::from_raw_parts(
                &ev as *const _ as *const u8,
                std::mem::size_of::<ClapEventParamGesture>(),
            )
            .to_vec()
        });
    }

    pub fn push_note_on(
        &mut self,
        note_id: i32,
        port_index: i16,
        channel: i16,
        key: i16,
        velocity: f64,
        sample_offset: u32,
    ) {
        let ev = ClapEventNote {
            header: ClapEventHeader {
                size: std::mem::size_of::<ClapEventNote>() as u32,
                time: sample_offset,
                space_id: CLAP_CORE_EVENT_SPACE_ID,
                type_: CLAP_EVENT_NOTE_ON,
                flags: 0,
            },
            note_id,
            port_index,
            channel,
            key,
            velocity,
        };
        self.events.push(unsafe {
            std::slice::from_raw_parts(
                &ev as *const _ as *const u8,
                std::mem::size_of::<ClapEventNote>(),
            )
            .to_vec()
        });
    }

    pub fn push_note_off(
        &mut self,
        note_id: i32,
        port_index: i16,
        channel: i16,
        key: i16,
        velocity: f64,
        sample_offset: u32,
    ) {
        let ev = ClapEventNote {
            header: ClapEventHeader {
                size: std::mem::size_of::<ClapEventNote>() as u32,
                time: sample_offset,
                space_id: CLAP_CORE_EVENT_SPACE_ID,
                type_: CLAP_EVENT_NOTE_OFF,
                flags: 0,
            },
            note_id,
            port_index,
            channel,
            key,
            velocity,
        };
        self.events.push(unsafe {
            std::slice::from_raw_parts(
                &ev as *const _ as *const u8,
                std::mem::size_of::<ClapEventNote>(),
            )
            .to_vec()
        });
    }

    pub fn push_midi(&mut self, data: [u8; 3], port_index: u16, sample_offset: u32) {
        let ev = ClapEventMidi {
            header: ClapEventHeader {
                size: std::mem::size_of::<ClapEventMidi>() as u32,
                time: sample_offset,
                space_id: CLAP_CORE_EVENT_SPACE_ID,
                type_: CLAP_EVENT_MIDI,
                flags: 0,
            },
            port_index,
            data,
        };
        self.events.push(unsafe {
            std::slice::from_raw_parts(
                &ev as *const _ as *const u8,
                std::mem::size_of::<ClapEventMidi>(),
            )
            .to_vec()
        });
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn as_input_events(&self) -> ClapInputEvents {
        ClapInputEvents {
            ctx: self as *const _ as *const c_void,
            size: Some(event_buffer_size),
            get: Some(event_buffer_get),
        }
    }
}

unsafe extern "C" fn event_buffer_size(events: *const ClapInputEvents) -> u32 {
    let buf = unsafe { &*((*events).ctx as *const EventBuffer) };
    buf.events.len() as u32
}

unsafe extern "C" fn event_buffer_get(
    events: *const ClapInputEvents,
    index: u32,
) -> *const ClapEventHeader {
    let buf = unsafe { &*((*events).ctx as *const EventBuffer) };
    buf.events
        .get(index as usize)
        .map(|bytes| bytes.as_ptr() as *const ClapEventHeader)
        .unwrap_or(ptr::null())
}

pub struct EventCapture {
    events: Vec<Vec<u8>>,
}

impl Default for EventCapture {
    fn default() -> Self {
        Self::new()
    }
}

impl EventCapture {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    pub fn drain(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.events)
    }

    pub fn as_output_events(&self) -> ClapOutputEvents {
        ClapOutputEvents {
            ctx: self as *const _ as *mut c_void,
            try_push: Some(event_capture_try_push),
        }
    }
}

unsafe extern "C" fn event_capture_try_push(
    events: *const ClapOutputEvents,
    header: *const ClapEventHeader,
) -> bool {
    if header.is_null() {
        return false;
    }
    let header = unsafe { &*header };
    let size = header.size as usize;
    if size == 0 || size > 128 {
        return false;
    }
    let capture = unsafe { &mut *((*events).ctx as *mut EventCapture) };
    let bytes =
        unsafe { std::slice::from_raw_parts(header as *const _ as *const u8, size).to_vec() };
    capture.events.push(bytes);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clap_state_streams_round_trip_bytes() {
        let mut written = Vec::new();
        let ostream = ClapOStream {
            ctx: (&mut written as *mut Vec<u8>).cast(),
            write: Some(clap_ostream_write),
        };
        let input = [1_u8, 2, 3, 4];

        let count = unsafe {
            clap_ostream_write(
                &ostream,
                input.as_ptr().cast::<c_void>(),
                input.len() as u64,
            )
        };

        assert_eq!(count, input.len() as i64);
        assert_eq!(written, input);

        let mut reader = ClapIStreamReader {
            bytes: &written,
            offset: 0,
        };
        let istream = ClapIStream {
            ctx: (&mut reader as *mut ClapIStreamReader).cast(),
            read: Some(clap_istream_read),
        };
        let mut first = [0_u8; 2];
        let mut second = [0_u8; 4];

        let first_count = unsafe {
            clap_istream_read(
                &istream,
                first.as_mut_ptr().cast::<c_void>(),
                first.len() as u64,
            )
        };
        let second_count = unsafe {
            clap_istream_read(
                &istream,
                second.as_mut_ptr().cast::<c_void>(),
                second.len() as u64,
            )
        };

        assert_eq!(first_count, 2);
        assert_eq!(second_count, 2);
        assert_eq!(first, [1, 2]);
        assert_eq!(&second[..2], &[3, 4]);
    }
}
