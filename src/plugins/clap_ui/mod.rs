use libloading::Library;
use std::ffi::{CStr, CString, c_char, c_void};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(target_os = "windows")]
use std::ffi::c_int;

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use std::ffi::{c_int, c_long, c_uint, c_ulong};

// Platform-specific window creation and event handling for Win32
#[cfg(target_os = "windows")]
use std::ffi::c_ushort;

#[cfg(target_os = "windows")]
#[repr(C)]
struct MSG {
    hwnd: *mut c_void,
    message: u32,
    w_param: usize,
    l_param: isize,
    time: u32,
    pt: POINT,
}

#[cfg(target_os = "windows")]
#[repr(C)]
struct POINT {
    x: i32,
    y: i32,
}

#[cfg(target_os = "windows")]
#[repr(C)]
struct WNDCLASSEXW {
    cb_size: u32,
    style: u32,
    lpfn_wnd_proc: *const c_void,
    cb_cls_extra: c_int,
    cb_wnd_extra: c_int,
    h_instance: *mut c_void,
    h_icon: *mut c_void,
    h_cursor: *mut c_void,
    hbr_background: *mut c_void,
    lpsz_menu_name: *const c_ushort,
    lpsz_class_name: *const c_ushort,
    h_icon_sm: *mut c_void,
}

#[cfg(target_os = "windows")]
#[repr(C)]
struct RECT {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[cfg(target_os = "windows")]
const WM_DESTROY: u32 = 0x0002;
#[cfg(target_os = "windows")]
const WM_CLOSE: u32 = 0x0010;
#[cfg(target_os = "windows")]
const WM_QUIT: u32 = 0x0012;
#[cfg(target_os = "windows")]
const PM_REMOVE: u32 = 0x0001;
#[cfg(target_os = "windows")]
const WS_OVERLAPPEDWINDOW: u32 = 0x00CF0000;
#[cfg(target_os = "windows")]
const WS_VISIBLE: u32 = 0x10000000;
#[cfg(target_os = "windows")]
const CW_USEDEFAULT: c_int = -2147483648i32;
#[cfg(target_os = "windows")]
const SW_SHOW: c_int = 5;

#[cfg(target_os = "windows")]
#[link(name = "user32")]
extern "system" {
    fn DefWindowProcW(hWnd: *mut c_void, Msg: u32, wParam: usize, lParam: isize) -> isize;
    fn RegisterClassExW(lpwcx: *const WNDCLASSEXW) -> u16;
    fn CreateWindowExW(
        dwExStyle: u32,
        lpClassName: *const c_ushort,
        lpWindowName: *const c_ushort,
        dwStyle: u32,
        x: c_int,
        y: c_int,
        nWidth: c_int,
        nHeight: c_int,
        hWndParent: *mut c_void,
        hMenu: *mut c_void,
        hInstance: *mut c_void,
        lpParam: *mut c_void,
    ) -> *mut c_void;
    fn DestroyWindow(hWnd: *mut c_void) -> c_int;
    fn ShowWindow(hWnd: *mut c_void, nCmdShow: c_int) -> c_int;
    fn PeekMessageW(
        lpMsg: *mut MSG,
        hWnd: *mut c_void,
        wMsgFilterMin: u32,
        wMsgFilterMax: u32,
        wRemoveMsg: u32,
    ) -> c_int;
    fn TranslateMessage(lpMsg: *const MSG) -> c_int;
    fn DispatchMessageW(lpMsg: *const MSG) -> isize;
    fn PostQuitMessage(nExitCode: c_int);
    fn IsWindow(hWnd: *mut c_void) -> c_int;
    fn GetModuleHandleW(lpModuleName: *const c_ushort) -> *mut c_void;
    fn GetClientRect(hWnd: *mut c_void, lpRect: *mut RECT) -> c_int;
    fn MoveWindow(hWnd: *mut c_void, x: c_int, y: c_int, nWidth: c_int, nHeight: c_int, bRepaint: c_int) -> c_int;
}

// Platform-specific event handling for X11
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
#[repr(C)]
#[derive(Copy, Clone)]
struct XClientMessageEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: c_int,
    display: *mut c_void,
    window: c_ulong,
    message_type: c_ulong,
    format: c_int,
    data: XClientMessageData,
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
#[repr(C)]
#[derive(Copy, Clone)]
union XClientMessageData {
    bytes: [c_char; 20],
    shorts: [c_int; 10],
    longs: [c_long; 5],
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
#[repr(C)]
#[derive(Copy, Clone)]
union XEvent {
    type_: c_int,
    xclient: XClientMessageEvent,
    pad: [c_long; 24],
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
const DESTROY_NOTIFY: c_int = 17;
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
const CLIENT_MESSAGE: c_int = 33;
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
const STRUCTURE_NOTIFY_MASK: c_long = 1 << 17;

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
#[link(name = "X11")]
unsafe extern "C" {
    fn XOpenDisplay(display_name: *const c_char) -> *mut c_void;
    fn XCloseDisplay(display: *mut c_void) -> c_int;
    fn XDefaultScreen(display: *mut c_void) -> c_int;
    fn XRootWindow(display: *mut c_void, screen_number: c_int) -> c_ulong;
    fn XBlackPixel(display: *mut c_void, screen_number: c_int) -> c_ulong;
    fn XWhitePixel(display: *mut c_void, screen_number: c_int) -> c_ulong;
    fn XCreateSimpleWindow(
        display: *mut c_void,
        parent: c_ulong,
        x: c_int,
        y: c_int,
        width: c_uint,
        height: c_uint,
        border_width: c_uint,
        border: c_ulong,
        background: c_ulong,
    ) -> c_ulong;
    fn XStoreName(display: *mut c_void, window: c_ulong, window_name: *const c_char) -> c_int;
    fn XSelectInput(display: *mut c_void, w: c_ulong, event_mask: c_long) -> c_int;
    fn XInternAtom(display: *mut c_void, atom_name: *const c_char, only_if_exists: c_int) -> c_ulong;
    fn XSetWMProtocols(display: *mut c_void, w: c_ulong, protocols: *mut c_ulong, count: c_int) -> c_int;
    fn XMapRaised(display: *mut c_void, window: c_ulong) -> c_int;
    fn XResizeWindow(display: *mut c_void, window: c_ulong, width: c_uint, height: c_uint) -> c_int;
    fn XDestroyWindow(display: *mut c_void, window: c_ulong) -> c_int;
    fn XPending(display: *mut c_void) -> c_int;
    fn XNextEvent(display: *mut c_void, event: *mut XEvent) -> c_int;
    fn XFlush(display: *mut c_void) -> c_int;
    fn XSync(display: *mut c_void, discard: c_int) -> c_int;
}

// Platform-specific event handling for macOS
#[cfg(target_os = "macos")]
#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {
    fn NSApplicationLoad() -> bool;
}

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
    set_parent: Option<unsafe extern "C" fn(*const ClapPlugin, *const ClapWindow) -> bool>,
    set_transient: Option<unsafe extern "C" fn(*const ClapPlugin, *const ClapWindow) -> bool>,
    suggest_title: Option<unsafe extern "C" fn(*const ClapPlugin, *const c_char)>,
    show: Option<unsafe extern "C" fn(*const ClapPlugin) -> bool>,
    hide: Option<unsafe extern "C" fn(*const ClapPlugin) -> bool>,
}

#[repr(C)]
union ClapWindowHandle {
    x11: c_ulong,
    win32: *mut c_void,
    cocoa: *mut c_void,
}

#[repr(C)]
struct ClapWindow {
    api: *const c_char,
    handle: ClapWindowHandle,
}

#[repr(C)]
struct ClapPluginTimerSupport {
    on_timer: Option<unsafe extern "C" fn(*const ClapPlugin, u32)>,
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
    timers: Mutex<Vec<HostTimer>>,
}

struct HostTimer {
    id: u32,
    period: Duration,
    next_tick: Instant,
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
    host: *const ClapHost,
    period_ms: u32,
    timer_id: *mut u32,
) -> bool {
    if timer_id.is_null() {
        return false;
    }
    let id = NEXT_TIMER_ID.fetch_add(1, Ordering::Relaxed);
    let period_ms = period_ms.max(1);
    let period = Duration::from_millis(period_ms as u64);
    let state = host_state(host);
    {
        let mut timers = state.timers.lock().unwrap();
        timers.push(HostTimer {
            id,
            period,
            next_tick: Instant::now() + period,
        });
    }
    unsafe { *timer_id = id };
    true
}

unsafe extern "C" fn host_timer_unregister(host: *const ClapHost, timer_id: u32) -> bool {
    let state = host_state(host);
    let mut timers = state.timers.lock().unwrap();
    timers.retain(|timer| timer.id != timer_id);
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
    eprintln!("[clap-ui] Plugin requested hide");
    let state = host_state(host);
    state.should_close.store(true, Ordering::SeqCst);
    true
}

unsafe extern "C" fn host_gui_closed(host: *const ClapHost, was_destroyed: bool) {
    eprintln!("[clap-ui] Plugin reported GUI closed (was_destroyed: {})", was_destroyed);
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

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn create_x11_window_for_clap(
    gui: &ClapPluginGui,
    plugin: *const ClapPlugin,
) -> Result<(*mut c_void, c_ulong, c_ulong, c_ulong, c_ulong), String> {
    eprintln!("[clap-ui] Opening X11 display...");
    let display = unsafe { XOpenDisplay(std::ptr::null()) };
    if display.is_null() {
        return Err("Failed to open X display for CLAP UI".to_string());
    }
    eprintln!("[clap-ui] Display opened: {:p}", display);

    eprintln!("[clap-ui] Getting screen info...");
    let screen = unsafe { XDefaultScreen(display) };
    let root = unsafe { XRootWindow(display, screen) };
    let black = unsafe { XBlackPixel(display, screen) };
    let white = unsafe { XWhitePixel(display, screen) };
    eprintln!("[clap-ui] Screen: {}, Root: {}", screen, root);

    // Start with default size - we'll get actual size after creating GUI
    let mut width = 900u32;
    let mut height = 600u32;

    // Create main window with default size
    eprintln!("[clap-ui] Creating main window...");
    let window = unsafe {
        XCreateSimpleWindow(
            display,
            root,
            120,
            120,
            width,
            height,
            1,
            black,
            white,
        )
    };
    if window == 0 {
        unsafe { XCloseDisplay(display) };
        return Err("Failed to create X11 window for CLAP UI".to_string());
    }
    eprintln!("[clap-ui] Main window created: {}", window);

    // Create embed window (child window for plugin)
    eprintln!("[clap-ui] Creating embed window...");
    let embed_window = unsafe {
        XCreateSimpleWindow(display, window, 0, 0, width, height, 0, black, white)
    };
    if embed_window == 0 {
        unsafe {
            XDestroyWindow(display, window);
            XCloseDisplay(display);
        }
        return Err("Failed to create X11 embed window for CLAP UI".to_string());
    }
    eprintln!("[clap-ui] Embed window created: {}", embed_window);

    // Set window title
    eprintln!("[clap-ui] Setting window title...");
    let title = CString::new("CLAP Plugin").unwrap();
    unsafe {
        XStoreName(display, window, title.as_ptr());
    }
    eprintln!("[clap-ui] Title set");

    // Set up event masks
    unsafe {
        XSelectInput(display, window, STRUCTURE_NOTIFY_MASK);
        XSelectInput(display, embed_window, STRUCTURE_NOTIFY_MASK);
    }

    // Set up WM_DELETE_WINDOW protocol
    let wm_delete_atom_name = CString::new("WM_DELETE_WINDOW").map_err(|e| e.to_string())?;
    let wm_delete = unsafe { XInternAtom(display, wm_delete_atom_name.as_ptr(), 0) };
    let wm_protocols_atom_name = CString::new("WM_PROTOCOLS").map_err(|e| e.to_string())?;
    let wm_protocols = unsafe { XInternAtom(display, wm_protocols_atom_name.as_ptr(), 0) };
    if wm_delete != 0 {
        let mut protocols = [wm_delete];
        unsafe {
            XSetWMProtocols(display, window, protocols.as_mut_ptr(), 1);
        }
    }

    // Get GUI callbacks
    eprintln!("[clap-ui] Getting GUI callbacks...");
    let create = gui
        .create
        .ok_or_else(|| "CLAP gui.create is unavailable".to_string())?;
    let set_parent = gui
        .set_parent
        .ok_or_else(|| "CLAP gui.set_parent is unavailable".to_string())?;
    let show = gui
        .show
        .ok_or_else(|| "CLAP gui.show is unavailable".to_string())?;
    eprintln!("[clap-ui] Got create, set_parent, show callbacks");

    // Create the GUI (must be done BEFORE get_size)
    eprintln!("[clap-ui] Calling gui.create...");
    let api_x11 = CString::new("x11").map_err(|e| e.to_string())?;
    if unsafe { !create(plugin, api_x11.as_ptr(), false) } {
        unsafe {
            XDestroyWindow(display, embed_window);
            XDestroyWindow(display, window);
            XCloseDisplay(display);
        }
        return Err("CLAP gui.create failed".to_string());
    }
    eprintln!("[clap-ui] gui.create succeeded");

    // Now get the actual size from the plugin (AFTER creating GUI)
    eprintln!("[clap-ui] Getting actual plugin size...");
    if let Some(get_size) = gui.get_size {
        unsafe {
            if get_size(plugin, &mut width, &mut height) {
                width = width.max(320);
                height = height.max(240);
                eprintln!("[clap-ui] Plugin size: {}x{}", width, height);

                // Resize windows to match plugin's preferred size
                XResizeWindow(display, window, width, height);
                XResizeWindow(display, embed_window, width, height);
            }
        }
    }

    // Set the parent window for the plugin UI
    // For X11, CLAP expects a ClapWindow structure with api="x11" and the Window ID
    eprintln!("[clap-ui] Creating ClapWindow structure with window {}...", embed_window);
    let clap_window = ClapWindow {
        api: c"x11".as_ptr(),
        handle: ClapWindowHandle { x11: embed_window },
    };

    eprintln!("[clap-ui] Calling gui.set_parent...");
    let set_parent_result = unsafe { set_parent(plugin, &clap_window) };
    eprintln!("[clap-ui] gui.set_parent returned: {}", set_parent_result);

    if !set_parent_result {
        if let Some(destroy_gui) = gui.destroy {
            unsafe { destroy_gui(plugin) };
        }
        unsafe {
            XDestroyWindow(display, embed_window);
            XDestroyWindow(display, window);
            XCloseDisplay(display);
        }
        return Err("CLAP gui.set_parent failed".to_string());
    }
    eprintln!("[clap-ui] gui.set_parent succeeded");

    // Show the GUI
    eprintln!("[clap-ui] Calling gui.show...");
    if unsafe { !show(plugin) } {
        if let Some(destroy_gui) = gui.destroy {
            unsafe { destroy_gui(plugin) };
        }
        unsafe {
            XDestroyWindow(display, embed_window);
            XDestroyWindow(display, window);
            XCloseDisplay(display);
        }
        return Err("CLAP gui.show failed".to_string());
    }

    // Map windows
    unsafe {
        XMapRaised(display, embed_window);
        XMapRaised(display, window);
        XFlush(display);
    }

    eprintln!("[clap-ui] Created X11 window: {} with embed window: {}", window, embed_window);

    Ok((display, window, embed_window, wm_delete, wm_protocols))
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn get_all_windows(display: *mut c_void, parent: c_ulong, windows: &mut Vec<c_ulong>) {
    #[link(name = "X11")]
    unsafe extern "C" {
        fn XQueryTree(
            display: *mut c_void,
            w: c_ulong,
            root_return: *mut c_ulong,
            parent_return: *mut c_ulong,
            children_return: *mut *mut c_ulong,
            nchildren_return: *mut c_uint,
        ) -> c_int;
        fn XFree(data: *mut c_void) -> c_int;
    }

    unsafe {
        let mut root_return: c_ulong = 0;
        let mut parent_return: c_ulong = 0;
        let mut children: *mut c_ulong = std::ptr::null_mut();
        let mut nchildren: c_uint = 0;

        if XQueryTree(
            display,
            parent,
            &mut root_return,
            &mut parent_return,
            &mut children,
            &mut nchildren,
        ) != 0
            && !children.is_null()
        {
            for i in 0..nchildren {
                let child = *children.add(i as usize);
                windows.push(child);
                get_all_windows(display, child, windows);
            }
            XFree(children as *mut c_void);
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn create_x11_wrapper_for_floating_clap(
    gui: &ClapPluginGui,
    plugin: *const ClapPlugin,
) -> Result<(*mut c_void, c_ulong, c_ulong, c_ulong, c_ulong), String> {
    let display = unsafe { XOpenDisplay(std::ptr::null()) };
    if display.is_null() {
        return Err("Failed to open X display for CLAP UI".to_string());
    }

    let screen = unsafe { XDefaultScreen(display) };
    let root = unsafe { XRootWindow(display, screen) };

    // Get list of windows before creating plugin UI
    let mut windows_before = Vec::new();
    get_all_windows(display, root, &mut windows_before);

    // Create the plugin's floating UI
    let create = gui
        .create
        .ok_or_else(|| "CLAP gui.create is unavailable".to_string())?;
    let show = gui
        .show
        .ok_or_else(|| "CLAP gui.show is unavailable".to_string())?;

    let api_x11 = CString::new("x11").map_err(|e| e.to_string())?;
    if unsafe { !create(plugin, api_x11.as_ptr(), true) } {
        unsafe { XCloseDisplay(display) };
        return Err("CLAP gui.create failed (floating mode)".to_string());
    }

    if unsafe { !show(plugin) } {
        if let Some(destroy_gui) = gui.destroy {
            unsafe { destroy_gui(plugin) };
        }
        unsafe { XCloseDisplay(display) };
        return Err("CLAP gui.show failed".to_string());
    }

    // Give the plugin time to create its window
    std::thread::sleep(Duration::from_millis(100));
    unsafe { XSync(display, 0) };

    // Get list of windows after creating plugin UI
    let mut windows_after = Vec::new();
    get_all_windows(display, root, &mut windows_after);

    // Find the new window (plugin's window)
    let mut plugin_window = 0;
    for &window in &windows_after {
        if !windows_before.contains(&window) {
            plugin_window = window;
            eprintln!("[clap-ui] Found plugin's floating window: {}", window);
            break;
        }
    }

    if plugin_window == 0 {
        eprintln!("[clap-ui] Warning: Could not find plugin's window, will rely on plugin callbacks");
        // Return display but no window to monitor
        return Ok((display, 0, 0, 0, 0));
    }

    // Set up event monitoring on the plugin's window
    unsafe {
        XSelectInput(display, plugin_window, STRUCTURE_NOTIFY_MASK);
    }

    // Set up WM_DELETE_WINDOW protocol (in case we can intercept it)
    let wm_delete_atom_name = CString::new("WM_DELETE_WINDOW").map_err(|e| e.to_string())?;
    let wm_delete = unsafe { XInternAtom(display, wm_delete_atom_name.as_ptr(), 0) };
    let wm_protocols_atom_name = CString::new("WM_PROTOCOLS").map_err(|e| e.to_string())?;
    let wm_protocols = unsafe { XInternAtom(display, wm_protocols_atom_name.as_ptr(), 0) };

    eprintln!("[clap-ui] Monitoring plugin's floating window for close events");

    Ok((display, plugin_window, 0, wm_delete, wm_protocols))
}

// Platform-specific event pumping to detect window close
#[cfg(target_os = "windows")]
fn pump_platform_events() -> bool {
    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            if msg.message == WM_QUIT {
                return true; // Window was closed
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    false // Continue running
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn pump_platform_events_x11(
    display: *mut c_void,
    window: c_ulong,
    wm_delete: c_ulong,
    wm_protocols: c_ulong,
) -> bool {
    if display.is_null() || window == 0 {
        return false;
    }
    unsafe {
        let pending = XPending(display);
        if pending > 0 {
            let mut event: XEvent = std::mem::zeroed();
            for _ in 0..pending {
                XNextEvent(display, &mut event);
                let event_type = event.type_;

                // Check for window destruction
                if event_type == DESTROY_NOTIFY {
                    eprintln!("[clap-ui] X11 DESTROY_NOTIFY received");
                    return true;
                }

                // Check for WM_DELETE_WINDOW (user clicked close button)
                if event_type == CLIENT_MESSAGE {
                    let msg = event.xclient;
                    if wm_delete != 0
                        && wm_protocols != 0
                        && msg.window == window
                        && msg.message_type == wm_protocols
                        && msg.format == 32
                        && (msg.data.longs[0] as c_ulong) == wm_delete
                    {
                        eprintln!("[clap-ui] X11 WM_DELETE_WINDOW received");
                        return true;
                    }
                }
            }
        }
    }
    false // Continue running
}

#[cfg(target_os = "macos")]
fn pump_platform_events_cocoa() -> bool {
    // For macOS, we would need to integrate with NSRunLoop
    // This is a placeholder for now
    false
}

fn open_editor_blocking(plugin_spec: &str) -> Result<(), String> {
    let (plugin_path, plugin_id) = split_plugin_spec(plugin_spec);
    let mut host_state = Box::new(UiHostState {
        should_close: AtomicBool::new(false),
        callback_requested: AtomicBool::new(false),
        timers: Mutex::new(Vec::new()),
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
    let timer_ext_id = c"clap.timer-support";

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
    let timer_ext_ptr = unsafe { get_extension(plugin, timer_ext_id.as_ptr()) };
    let timer_ext = if timer_ext_ptr.is_null() {
        None
    } else {
        Some(unsafe { &*(timer_ext_ptr as *const ClapPluginTimerSupport) })
    };
    let on_timer = timer_ext.and_then(|ext| ext.on_timer);

    // Determine which GUI API to use and whether embedded mode is supported
    let mut chosen: Option<CString> = None;
    let mut supports_embedded = false;

    if let Some(is_api_supported) = gui.is_api_supported {
        // Try embedded (non-floating) mode first
        for candidate in ["x11", "cocoa", "win32"] {
            let c = CString::new(candidate).map_err(|e| e.to_string())?;
            if unsafe { is_api_supported(plugin, c.as_ptr(), false) } {
                eprintln!("[clap-ui] Plugin supports {} in embedded mode", candidate);
                chosen = Some(c);
                supports_embedded = true;
                break;
            }
        }

        // Fall back to floating mode if embedded not supported
        if chosen.is_none() {
            for candidate in ["x11", "cocoa", "win32"] {
                let c = CString::new(candidate).map_err(|e| e.to_string())?;
                if unsafe { is_api_supported(plugin, c.as_ptr(), true) } {
                    eprintln!("[clap-ui] Plugin supports {} in floating mode only", candidate);
                    chosen = Some(c);
                    supports_embedded = false;
                    break;
                }
            }
        }
    }

    let Some(api) = chosen else {
        return Err("No supported CLAP GUI API found".to_string());
    };

    // Platform-specific window creation
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    let (x11_display, x11_window, x11_embed_window, wm_delete, wm_protocols) =
        if api.to_str().map(|s| s == "x11").unwrap_or(false) && supports_embedded {
            eprintln!("[clap-ui] Creating embedded X11 window");
            create_x11_window_for_clap(gui, plugin)?
        } else if api.to_str().map(|s| s == "x11").unwrap_or(false) {
            eprintln!("[clap-ui] Plugin only supports floating mode, creating wrapper window");
            create_x11_wrapper_for_floating_clap(gui, plugin)?
        } else {
            (std::ptr::null_mut(), 0, 0, 0, 0)
        };

    #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
    {
        let create = gui
            .create
            .ok_or_else(|| "CLAP gui.create is unavailable".to_string())?;
        let show = gui
            .show
            .ok_or_else(|| "CLAP gui.show is unavailable".to_string())?;

        let floating = !supports_embedded;
        if unsafe { !create(plugin, api.as_ptr(), floating) } {
            return Err("CLAP gui.create failed".to_string());
        }
        if unsafe { !show(plugin) } {
            return Err("CLAP gui.show failed".to_string());
        }
    }

    eprintln!("[clap-ui] Event loop started, API: {:?}", api);
    let mut iteration_count = 0u32;
    while !host_state.should_close.load(Ordering::SeqCst) {
        iteration_count += 1;
        // Pump platform-specific events to detect window close
        #[cfg(target_os = "windows")]
        if pump_platform_events() {
            eprintln!("[clap-ui] Window close detected via Windows events");
            host_state.should_close.store(true, Ordering::SeqCst);
            break;
        }

        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        if !x11_display.is_null()
            && x11_window != 0
            && pump_platform_events_x11(x11_display, x11_window, wm_delete, wm_protocols)
        {
            host_state.should_close.store(true, Ordering::SeqCst);
            break;
        }

        #[cfg(target_os = "macos")]
        if pump_platform_events_cocoa() {
            eprintln!("[clap-ui] Window close detected via Cocoa events");
            host_state.should_close.store(true, Ordering::SeqCst);
            break;
        }
        if host_state.callback_requested.swap(false, Ordering::SeqCst)
            && let Some(on_main_thread) = plugin_ref.on_main_thread
        {
            unsafe { on_main_thread(plugin) };
        }
        if let Some(on_timer) = on_timer {
            let now = Instant::now();
            let mut due_ids = Vec::new();
            {
                let mut timers = host_state.timers.lock().unwrap();
                for timer in timers.iter_mut() {
                    if now >= timer.next_tick {
                        due_ids.push(timer.id);
                        timer.next_tick = now + timer.period;
                    }
                }
            }
            for timer_id in due_ids {
                unsafe { on_timer(plugin, timer_id) };
            }
        }

        // Debug: log periodically to show we're still running
        if iteration_count % 600 == 0 {
            eprintln!("[clap-ui] Still running (iteration {}), should_close = {}",
                iteration_count, host_state.should_close.load(Ordering::SeqCst));
        }

        thread::sleep(Duration::from_millis(16));
    }
    eprintln!("[clap-ui] Event loop exited, cleaning up...");

    // Clean up GUI
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

    // Clean up platform-specific resources
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    if !x11_display.is_null() {
        unsafe {
            if x11_embed_window != 0 {
                XDestroyWindow(x11_display, x11_embed_window);
            }
            if x11_window != 0 {
                XDestroyWindow(x11_display, x11_window);
            }
            XSync(x11_display, 0);
            XCloseDisplay(x11_display);
        }
        eprintln!("[clap-ui] X11 resources cleaned up");
    }

    Ok(())
}
