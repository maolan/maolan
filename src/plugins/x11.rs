use crate::consts::plugins_x11::{
    CLIENT_MESSAGE, DESTROY_NOTIFY, EXPOSURE_MASK, STRUCTURE_NOTIFY_MASK, XEMBED_EMBEDDED_NOTIFY,
    XEMBED_FOCUS_CURRENT, XEMBED_FOCUS_IN, XEMBED_WINDOW_ACTIVATE,
};
use std::ffi::{CString, c_char, c_int, c_long, c_uchar, c_uint, c_ulong, c_void};
use std::sync::OnceLock;

static X11_THREADS_INIT: OnceLock<bool> = OnceLock::new();

#[repr(C)]
#[derive(Copy, Clone)]
pub union XEvent {
    pub type_: c_int,
    pub xclient: XClientMessageEvent,
    pub xconfigure: XConfigureEvent,
    pub xdestroywindow: XDestroyWindowEvent,
    pub xunmap: XUnmapEvent,
    pad: [c_long; 24],
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct XClientMessageData {
    pub longs: [c_long; 5],
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct XClientMessageEvent {
    pub type_: c_int,
    pub serial: c_ulong,
    pub send_event: c_int,
    pub display: *mut c_void,
    pub window: c_ulong,
    pub message_type: c_ulong,
    pub format: c_int,
    pub data: XClientMessageData,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct XConfigureEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: c_int,
    display: *mut c_void,
    event: c_ulong,
    window: c_ulong,
    x: c_int,
    y: c_int,
    width: c_int,
    height: c_int,
    border_width: c_int,
    above: c_ulong,
    override_redirect: c_int,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct XDestroyWindowEvent {
    pub type_: c_int,
    pub serial: c_ulong,
    pub send_event: c_int,
    pub display: *mut c_void,
    pub event: c_ulong,
    pub window: c_ulong,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct XUnmapEvent {
    pub type_: c_int,
    pub serial: c_ulong,
    pub send_event: c_int,
    pub display: *mut c_void,
    pub event: c_ulong,
    pub window: c_ulong,
    pub from_configure: c_int,
}

#[repr(C)]
pub struct XErrorEvent {
    type_: c_int,
    display: *mut c_void,
    resourceid: c_ulong,
    serial: c_ulong,
    error_code: c_uchar,
    request_code: c_uchar,
    minor_code: c_uchar,
}

type XErrorHandler = unsafe extern "C" fn(*mut c_void, *mut XErrorEvent) -> c_int;

unsafe extern "C" fn vst3_x11_error_handler(
    _display: *mut c_void,
    event: *mut XErrorEvent,
) -> c_int {
    if event.is_null() {
        return 0;
    }
    let ev = unsafe { &*event };
    if ev.error_code == 3 {
        return 0;
    }
    0
}

struct X11ErrorHandlerGuard {
    previous: Option<XErrorHandler>,
}

impl X11ErrorHandlerGuard {
    fn install() -> Self {
        let previous = unsafe { XSetErrorHandler(Some(vst3_x11_error_handler)) };
        Self { previous }
    }
}

impl Drop for X11ErrorHandlerGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = XSetErrorHandler(self.previous);
        }
    }
}

#[link(name = "X11")]
unsafe extern "C" {
    pub fn XInitThreads() -> c_int;
    pub fn XOpenDisplay(display_name: *const c_char) -> *mut c_void;
    pub fn XCloseDisplay(display: *mut c_void) -> c_int;
    pub fn XDefaultScreen(display: *mut c_void) -> c_int;
    pub fn XRootWindow(display: *mut c_void, screen_number: c_int) -> c_ulong;
    pub fn XBlackPixel(display: *mut c_void, screen_number: c_int) -> c_ulong;
    pub fn XWhitePixel(display: *mut c_void, screen_number: c_int) -> c_ulong;
    pub fn XCreateSimpleWindow(
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
    pub fn XStoreName(display: *mut c_void, window: c_ulong, window_name: *const c_char) -> c_int;
    pub fn XSelectInput(display: *mut c_void, window: c_ulong, event_mask: c_long) -> c_int;
    pub fn XInternAtom(
        display: *mut c_void,
        atom_name: *const c_char,
        only_if_exists: c_int,
    ) -> c_ulong;
    pub fn XSetWMProtocols(
        display: *mut c_void,
        window: c_ulong,
        protocols: *mut c_ulong,
        count: c_int,
    ) -> c_int;
    pub fn XChangeProperty(
        display: *mut c_void,
        window: c_ulong,
        property: c_ulong,
        type_: c_ulong,
        format: c_int,
        mode: c_int,
        data: *const c_uchar,
        nelements: c_int,
    ) -> c_int;
    pub fn XMapRaised(display: *mut c_void, window: c_ulong) -> c_int;
    pub fn XMapSubwindows(display: *mut c_void, window: c_ulong) -> c_int;
    pub fn XResizeWindow(
        display: *mut c_void,
        window: c_ulong,
        width: c_uint,
        height: c_uint,
    ) -> c_int;
    fn XMoveResizeWindow(
        display: *mut c_void,
        window: c_ulong,
        x: c_int,
        y: c_int,
        width: c_uint,
        height: c_uint,
    ) -> c_int;
    pub fn XDestroyWindow(display: *mut c_void, window: c_ulong) -> c_int;
    pub fn XQueryTree(
        display: *mut c_void,
        window: c_ulong,
        root_return: *mut c_ulong,
        parent_return: *mut c_ulong,
        children_return: *mut *mut c_ulong,
        nchildren_return: *mut c_uint,
    ) -> c_int;
    pub fn XFree(data: *mut c_void) -> c_int;
    pub fn XSync(display: *mut c_void, discard: c_int) -> c_int;
    pub fn XPending(display: *mut c_void) -> c_int;
    pub fn XNextEvent(display: *mut c_void, event_return: *mut XEvent) -> c_int;
    pub fn XFlush(display: *mut c_void) -> c_int;
    pub fn XSendEvent(
        display: *mut c_void,
        w: c_ulong,
        propagate: c_int,
        event_mask: c_long,
        event_send: *mut XEvent,
    ) -> c_int;
    pub fn XSetErrorHandler(handler: Option<XErrorHandler>) -> Option<XErrorHandler>;
}

fn ensure_x11_threads() -> bool {
    *X11_THREADS_INIT.get_or_init(|| unsafe { XInitThreads() != 0 })
}

fn first_child(display: *mut c_void, window: c_ulong) -> Option<c_ulong> {
    let mut root: c_ulong = 0;
    let mut parent: c_ulong = 0;
    let mut children_ptr: *mut c_ulong = std::ptr::null_mut();
    let mut nchildren: c_uint = 0;
    let ok = unsafe {
        XQueryTree(
            display,
            window,
            &mut root,
            &mut parent,
            &mut children_ptr,
            &mut nchildren,
        )
    };
    if ok == 0 || children_ptr.is_null() || nchildren == 0 {
        return None;
    }
    let child = unsafe { *children_ptr };
    unsafe {
        let _ = XFree(children_ptr.cast::<c_void>());
    }
    Some(child)
}

pub fn set_dialog_window_type(display: *mut c_void, window: c_ulong) {
    if display.is_null() || window == 0 {
        return;
    }
    let wm_type_name = CString::new("_NET_WM_WINDOW_TYPE").unwrap_or_default();
    let dialog_name = CString::new("_NET_WM_WINDOW_TYPE_DIALOG").unwrap_or_default();
    if wm_type_name.as_bytes().is_empty() || dialog_name.as_bytes().is_empty() {
        return;
    }
    unsafe {
        let wm_type = XInternAtom(display, wm_type_name.as_ptr(), 0);
        let dialog = XInternAtom(display, dialog_name.as_ptr(), 0);
        if wm_type == 0 || dialog == 0 {
            return;
        }
        XChangeProperty(
            display,
            window,
            wm_type,
            4,
            32,
            0,
            &dialog as *const c_ulong as *const c_uchar,
            1,
        );
    }
}

fn map_and_resize_children(display: *mut c_void, parent: c_ulong, width: i32, height: i32) {
    let mut root: c_ulong = 0;
    let mut parent_ret: c_ulong = 0;
    let mut children_ptr: *mut c_ulong = std::ptr::null_mut();
    let mut nchildren: c_uint = 0;
    let ok = unsafe {
        XQueryTree(
            display,
            parent,
            &mut root,
            &mut parent_ret,
            &mut children_ptr,
            &mut nchildren,
        )
    };
    if ok != 0 && !children_ptr.is_null() && nchildren > 0 {
        for i in 0..nchildren as usize {
            let child = unsafe { *children_ptr.add(i) };
            unsafe {
                let _ = XMoveResizeWindow(display, child, 0, 0, width as c_uint, height as c_uint);
                let _ = XMapRaised(display, child);
            }
        }
        unsafe {
            let _ = XFree(children_ptr.cast::<c_void>());
        }
    }
    unsafe {
        let _ = XFlush(display);
    }
}

fn send_xembed_message(
    display: *mut c_void,
    child: c_ulong,
    message: c_long,
    detail: c_long,
    data2: c_long,
) {
    let atom_name = match CString::new("_XEMBED") {
        Ok(v) => v,
        Err(_) => return,
    };
    let xembed_atom = unsafe { XInternAtom(display, atom_name.as_ptr(), 0) };
    if xembed_atom == 0 {
        return;
    }
    let mut event = XEvent { pad: [0; 24] };
    event.xclient = XClientMessageEvent {
        type_: CLIENT_MESSAGE,
        serial: 0,
        send_event: 1,
        display,
        window: child,
        message_type: xembed_atom,
        format: 32,
        data: XClientMessageData {
            longs: [0, message, detail, data2, 0],
        },
    };
    let _ = unsafe { XSendEvent(display, child, 0, 0, &mut event) };
    unsafe {
        let _ = XFlush(display);
    }
}

// Processor-based GUI entry point
pub fn open_editor_with_processor(
    processor: std::sync::Arc<maolan_engine::vst3::Vst3Processor>,
    title: String,
) -> Result<Option<maolan_engine::vst3::Vst3PluginState>, String> {
    let result = run_vst3_x11_editor_with_processor(processor, title);
    result.map(|_| None)
}

fn run_vst3_x11_editor_with_processor(
    processor: std::sync::Arc<maolan_engine::vst3::Vst3Processor>,
    title: String,
) -> Result<(), String> {
    let _ = ensure_x11_threads();

    processor.ui_begin_session();

    let display = unsafe { XOpenDisplay(std::ptr::null()) };
    if display.is_null() {
        processor.ui_end_session();
        return Err("Failed to open X display for VST3 editor".to_string());
    }
    let _error_handler_guard = X11ErrorHandlerGuard::install();
    let screen = unsafe { XDefaultScreen(display) };
    let root = unsafe { XRootWindow(display, screen) };
    let black = unsafe { XBlackPixel(display, screen) };
    let white = unsafe { XWhitePixel(display, screen) };
    let window = unsafe { XCreateSimpleWindow(display, root, 120, 120, 900, 600, 1, black, white) };
    if window == 0 {
        unsafe {
            let _ = XCloseDisplay(display);
        }
        processor.ui_end_session();
        return Err("Failed to create X11 window for VST3 editor".to_string());
    }
    let embed_window =
        unsafe { XCreateSimpleWindow(display, window, 0, 0, 900, 600, 0, black, white) };
    if embed_window == 0 {
        unsafe {
            let _ = XDestroyWindow(display, window);
            let _ = XCloseDisplay(display);
        }
        processor.ui_end_session();
        return Err("Failed to create X11 embed window for VST3 editor".to_string());
    }

    set_dialog_window_type(display, window);

    let platform_type = "X11EmbedWindowID";
    if let Err(e) = processor.gui_create(platform_type) {
        unsafe {
            let _ = XDestroyWindow(display, window);
            let _ = XCloseDisplay(display);
        }
        processor.ui_end_session();
        return Err(e);
    }

    let (width, height) = match processor.gui_get_size() {
        Ok((w, h)) => (w.max(320), h.max(240)),
        Err(_) => (900, 600),
    };

    let title_c = CString::new(title).map_err(|e| e.to_string())?;
    let (wm_delete, wm_protocols) = unsafe {
        let _ = XStoreName(display, window, title_c.as_ptr());
        let _ = XResizeWindow(display, window, width as c_uint, height as c_uint);
        let _ = XResizeWindow(display, embed_window, width as c_uint, height as c_uint);
        let _ = XSelectInput(
            display,
            window,
            (STRUCTURE_NOTIFY_MASK | EXPOSURE_MASK) as c_long,
        );
        let _ = XSelectInput(
            display,
            embed_window,
            (STRUCTURE_NOTIFY_MASK | EXPOSURE_MASK) as c_long,
        );
        let wm_delete_atom_name = CString::new("WM_DELETE_WINDOW").map_err(|e| e.to_string())?;
        let wm_delete = XInternAtom(display, wm_delete_atom_name.as_ptr(), 0);
        let wm_protocols_atom_name = CString::new("WM_PROTOCOLS").map_err(|e| e.to_string())?;
        let wm_protocols = XInternAtom(display, wm_protocols_atom_name.as_ptr(), 0);
        if wm_delete != 0 {
            let mut protocols = [wm_delete];
            let _ = XSetWMProtocols(display, window, protocols.as_mut_ptr(), 1);
        }
        (wm_delete, wm_protocols)
    };

    if let Err(e) = processor.gui_set_parent(embed_window as usize, platform_type) {
        unsafe {
            let _ = XDestroyWindow(display, window);
            let _ = XCloseDisplay(display);
        }
        processor.gui_destroy();
        processor.ui_end_session();
        return Err(e);
    }

    let _ = processor.gui_on_size(width, height);

    if let Err(e) = processor.gui_show() {
        unsafe {
            let _ = XDestroyWindow(display, window);
            let _ = XCloseDisplay(display);
        }
        processor.gui_destroy();
        processor.ui_end_session();
        return Err(e);
    }

    unsafe {
        let _ = XMapSubwindows(display, embed_window);
        let _ = XMapRaised(display, embed_window);
        let _ = XMapRaised(display, window);
        let _ = XFlush(display);
    }
    map_and_resize_children(display, embed_window, width, height);
    if let Some(child) = first_child(display, embed_window) {
        send_xembed_message(
            display,
            child,
            XEMBED_EMBEDDED_NOTIFY,
            0,
            embed_window as c_long,
        );
        send_xembed_message(display, child, XEMBED_WINDOW_ACTIVATE, 0, 0);
        send_xembed_message(display, child, XEMBED_FOCUS_IN, XEMBED_FOCUS_CURRENT, 0);
    }

    loop {
        processor.gui_on_main_thread();
        if processor.ui_should_close() {
            break;
        }
        let pending = unsafe { XPending(display) };
        if pending <= 0 {
            std::thread::sleep(std::time::Duration::from_millis(16));
            continue;
        }
        let mut event = XEvent { pad: [0; 24] };
        unsafe {
            let _ = XNextEvent(display, &mut event);
        }
        let event_type = unsafe { event.type_ };
        if event_type == DESTROY_NOTIFY {
            break;
        }
        if event_type == 22 {
            let cfg = unsafe { event.xconfigure };
            let w = cfg.width.max(1);
            let h = cfg.height.max(1);
            if cfg.window == window {
                let _ = processor.gui_on_size(w, h);
                map_and_resize_children(display, embed_window, w, h);
            } else if cfg.window == embed_window {
                map_and_resize_children(display, embed_window, w, h);
            }
        }
        if event_type == CLIENT_MESSAGE {
            let msg = unsafe { event.xclient };
            if wm_delete != 0
                && wm_protocols != 0
                && msg.window == window
                && msg.message_type == wm_protocols
                && msg.format == 32
                && (msg.data.longs[0] as c_ulong) == wm_delete
            {
                unsafe {
                    let _ = XDestroyWindow(display, window);
                    let _ = XSync(display, 0);
                }
            }
        }
    }

    processor.gui_hide();
    processor.gui_destroy();
    unsafe {
        let _ = XSync(display, 0);
        let _ = XDestroyWindow(display, window);
        let _ = XSync(display, 0);
        let _ = XCloseDisplay(display);
    }
    processor.ui_end_session();
    Ok(())
}
