use libloading::Library;
use std::ffi::{CStr, CString, c_char, c_void};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::thread;
use std::time::Duration;

pub struct GuiClapUiHost;

impl GuiClapUiHost {
    pub fn new() -> Self {
        Self
    }

    pub fn open_editor(&mut self, plugin_spec: &str) -> Result<(), String> {
        let plugin_spec = plugin_spec.to_string();
        thread::Builder::new()
            .name("clap-ui".to_string())
            .spawn(move || {
                if let Err(err) = open_editor_blocking(&plugin_spec) {
                    eprintln!("CLAP UI error: {err}");
                }
            })
            .map_err(|e| format!("Failed to spawn CLAP UI thread: {e}"))?;
        Ok(())
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
    process: Option<unsafe extern "C" fn(*const ClapPlugin, *const c_void) -> i32>,
    get_extension: Option<unsafe extern "C" fn(*const ClapPlugin, *const c_char) -> *const c_void>,
    on_main_thread: Option<unsafe extern "C" fn(*const ClapPlugin)>,
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

struct UiHostState {
    should_close: AtomicBool,
    callback_requested: AtomicBool,
}

static NEXT_TIMER_ID: AtomicU32 = AtomicU32::new(1);
static HOST_THREAD_CHECK_EXT: ClapHostThreadCheck = ClapHostThreadCheck {
    is_main_thread: Some(host_is_main_thread),
    is_audio_thread: Some(host_is_audio_thread),
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

unsafe extern "C" fn host_get_extension(
    _host: *const ClapHost,
    extension_id: *const c_char,
) -> *const c_void {
    if extension_id.is_null() {
        return std::ptr::null();
    }
    let id = unsafe { CStr::from_ptr(extension_id) }.to_string_lossy();
    match id.as_ref() {
        "clap.host.thread-check" => {
            (&HOST_THREAD_CHECK_EXT as *const ClapHostThreadCheck).cast::<c_void>()
        }
        "clap.host.timer-support" => {
            (&HOST_TIMER_EXT as *const ClapHostTimerSupport).cast::<c_void>()
        }
        "clap.host.gui" => (&HOST_GUI_EXT as *const ClapHostGui).cast::<c_void>(),
        _ => std::ptr::null(),
    }
}

unsafe extern "C" fn host_request_restart(_host: *const ClapHost) {}

unsafe extern "C" fn host_request_process(_host: *const ClapHost) {}

unsafe extern "C" fn host_request_callback(host: *const ClapHost) {
    let state = host_state(host);
    state.callback_requested.store(true, Ordering::SeqCst);
}

unsafe extern "C" fn host_is_main_thread(_host: *const ClapHost) -> bool {
    true
}

unsafe extern "C" fn host_is_audio_thread(_host: *const ClapHost) -> bool {
    false
}

unsafe extern "C" fn host_timer_register(
    _host: *const ClapHost,
    _period_ms: u32,
    timer_id: *mut u32,
) -> bool {
    if timer_id.is_null() {
        return false;
    }
    let id = NEXT_TIMER_ID.fetch_add(1, Ordering::Relaxed);
    unsafe { *timer_id = id };
    true
}

unsafe extern "C" fn host_timer_unregister(_host: *const ClapHost, _timer_id: u32) -> bool {
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

unsafe extern "C" fn host_gui_request_hide(host: *const ClapHost) -> bool {
    let state = host_state(host);
    state.should_close.store(true, Ordering::SeqCst);
    true
}

unsafe extern "C" fn host_gui_closed(host: *const ClapHost, _was_destroyed: bool) {
    let state = host_state(host);
    state.should_close.store(true, Ordering::SeqCst);
}

fn host_state(host: *const ClapHost) -> &'static UiHostState {
    if host.is_null() {
        panic!("CLAP host pointer is null");
    }
    let state_ptr = unsafe { (*host).host_data as *const UiHostState };
    if state_ptr.is_null() {
        panic!("CLAP host_data is null");
    }
    unsafe { &*state_ptr }
}

fn split_plugin_spec(spec: &str) -> (&str, Option<&str>) {
    if let Some((path, id)) = spec.split_once("::") {
        (path, Some(id))
    } else {
        (spec, None)
    }
}

fn open_editor_blocking(plugin_spec: &str) -> Result<(), String> {
    let (plugin_path, plugin_id) = split_plugin_spec(plugin_spec);
    let mut host_state = Box::new(UiHostState {
        should_close: AtomicBool::new(false),
        callback_requested: AtomicBool::new(false),
    });
    let host = ClapHost {
        clap_version: CLAP_VERSION,
        host_data: (&mut *host_state as *mut UiHostState).cast::<c_void>(),
        name: c"Maolan".as_ptr(),
        vendor: c"Maolan".as_ptr(),
        url: c"https://example.invalid".as_ptr(),
        version: c"0.0.1".as_ptr(),
        get_extension: Some(host_get_extension),
        request_restart: Some(host_request_restart),
        request_process: Some(host_request_process),
        request_callback: Some(host_request_callback),
    };
    let factory_id = c"clap.plugin-factory";
    let gui_ext_id = c"clap.gui";

    let library = unsafe { Library::new(plugin_path) }.map_err(|e| e.to_string())?;
    let entry_ptr = unsafe {
        let sym = library
            .get::<*const ClapPluginEntry>(b"clap_entry\0")
            .map_err(|e| e.to_string())?;
        *sym
    };
    if entry_ptr.is_null() {
        return Err("CLAP entry symbol is null".to_string());
    }
    let entry = unsafe { &*entry_ptr };
    let init = entry
        .init
        .ok_or_else(|| "CLAP entry missing init()".to_string())?;
    if unsafe { !init(&host as *const ClapHost) } {
        return Err(format!("CLAP entry init failed for {plugin_path}"));
    }

    let get_factory = entry
        .get_factory
        .ok_or_else(|| "CLAP entry missing get_factory()".to_string())?;
    let factory = unsafe { get_factory(factory_id.as_ptr()) } as *const ClapPluginFactory;
    if factory.is_null() {
        return Err("CLAP plugin factory not found".to_string());
    }
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

    let count = unsafe { get_count(factory) };
    if count == 0 {
        return Err("CLAP factory returned zero plugins".to_string());
    }
    let mut selected_id = None::<CString>;
    for i in 0..count {
        let desc = unsafe { get_desc(factory, i) };
        if desc.is_null() {
            continue;
        }
        let desc = unsafe { &*desc };
        if desc.id.is_null() {
            continue;
        }
        let id = unsafe { CStr::from_ptr(desc.id) };
        let id_str = id.to_string_lossy();
        if plugin_id.is_none() || plugin_id == Some(id_str.as_ref()) {
            selected_id = Some(CString::new(id_str.as_ref()).map_err(|e| e.to_string())?);
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

    let plugin = unsafe { create(factory, &host, selected_id.as_ptr()) };
    if plugin.is_null() {
        return Err("CLAP factory create_plugin failed".to_string());
    }
    let plugin_ref = unsafe { &*plugin };
    let plugin_init = plugin_ref
        .init
        .ok_or_else(|| "CLAP plugin missing init()".to_string())?;
    if unsafe { !plugin_init(plugin) } {
        return Err("CLAP plugin init() failed".to_string());
    }
    if let Some(activate) = plugin_ref.activate {
        let _ = unsafe { activate(plugin, 48_000.0, 256, 256) };
    }
    if let Some(start_processing) = plugin_ref.start_processing {
        let _ = unsafe { start_processing(plugin) };
    }

    let get_extension = plugin_ref
        .get_extension
        .ok_or_else(|| "CLAP plugin missing get_extension()".to_string())?;
    let gui_ptr = unsafe { get_extension(plugin, gui_ext_id.as_ptr()) };
    if gui_ptr.is_null() {
        return Err("CLAP plugin does not expose clap.gui".to_string());
    }
    let gui = unsafe { &*(gui_ptr as *const ClapPluginGui) };

    let create = gui
        .create
        .ok_or_else(|| "CLAP gui.create is unavailable".to_string())?;
    let show = gui
        .show
        .ok_or_else(|| "CLAP gui.show is unavailable".to_string())?;
    let mut chosen: Option<CString> = None;
    if let Some(get_preferred_api) = gui.get_preferred_api {
        let mut api_ptr: *const c_char = std::ptr::null();
        let mut floating = true;
        let ok = unsafe { get_preferred_api(plugin, &mut api_ptr, &mut floating) };
        if ok && floating && !api_ptr.is_null() {
            let pref = unsafe { CStr::from_ptr(api_ptr) }
                .to_string_lossy()
                .to_string();
            chosen = CString::new(pref).ok();
        }
    }
    if chosen.is_none()
        && let Some(is_api_supported) = gui.is_api_supported
    {
        for candidate in ["x11", "cocoa", "win32"] {
            let c = CString::new(candidate).map_err(|e| e.to_string())?;
            if unsafe { is_api_supported(plugin, c.as_ptr(), true) } {
                chosen = Some(c);
                break;
            }
        }
    }
    let Some(api) = chosen else {
        return Err("No supported floating CLAP GUI API found".to_string());
    };

    if unsafe { !create(plugin, api.as_ptr(), true) } {
        return Err("CLAP gui.create failed".to_string());
    }
    if unsafe { !show(plugin) } {
        return Err("CLAP gui.show failed".to_string());
    }

    while !host_state.should_close.load(Ordering::SeqCst) {
        if host_state.callback_requested.swap(false, Ordering::SeqCst)
            && let Some(on_main_thread) = plugin_ref.on_main_thread
        {
            unsafe { on_main_thread(plugin) };
        }
        thread::sleep(Duration::from_millis(16));
    }

    if let Some(hide) = gui.hide {
        let _ = unsafe { hide(plugin) };
    }
    if let Some(destroy_gui) = gui.destroy {
        unsafe { destroy_gui(plugin) };
    }
    if let Some(stop_processing) = plugin_ref.stop_processing {
        unsafe { stop_processing(plugin) };
    }
    if let Some(deactivate) = plugin_ref.deactivate {
        unsafe { deactivate(plugin) };
    }
    if let Some(destroy_plugin) = plugin_ref.destroy {
        unsafe { destroy_plugin(plugin) };
    }
    if let Some(deinit) = entry.deinit {
        unsafe { deinit() };
    }

    Ok(())
}
