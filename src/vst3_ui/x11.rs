use maolan_engine::plugins::vst3::interfaces::{PluginFactory, pump_host_run_loop};
use std::ffi::{CString, c_char, c_int, c_long, c_uchar, c_uint, c_ulong, c_void};
use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};
use vst3::Interface;
use vst3::Steinberg::IPlugViewTrait;
use vst3::Steinberg::Vst::{IEditControllerTrait, ViewType};
use vst3::Steinberg::kResultTrue;

const CLIENT_MESSAGE: i32 = 33;
const DESTROY_NOTIFY: i32 = 17;
const STRUCTURE_NOTIFY_MASK: i64 = 1 << 17;
const EXPOSURE_MASK: i64 = 1 << 15;
const XEMBED_EMBEDDED_NOTIFY: c_long = 0;
const XEMBED_WINDOW_ACTIVATE: c_long = 1;
const XEMBED_FOCUS_IN: c_long = 4;
const XEMBED_FOCUS_CURRENT: c_long = 0;

static X11_THREADS_INIT: OnceLock<bool> = OnceLock::new();

#[repr(C)]
struct HostPlugFrame {
    iface: vst3::Steinberg::IPlugFrame,
    ref_count: AtomicU32,
    display: *mut c_void,
    window: c_ulong,
    embed_window: c_ulong,
}

impl HostPlugFrame {
    fn new(display: *mut c_void, window: c_ulong, embed_window: c_ulong) -> Self {
        Self {
            iface: vst3::Steinberg::IPlugFrame {
                vtbl: &HOST_PLUG_FRAME_VTBL,
            },
            ref_count: AtomicU32::new(1),
            display,
            window,
            embed_window,
        }
    }
}

fn tuid_matches(iid_ptr: *const vst3::Steinberg::TUID, guid: &[u8; 16]) -> bool {
    if iid_ptr.is_null() {
        return false;
    }
    let iid = unsafe { &*iid_ptr };
    iid.iter()
        .zip(guid.iter())
        .all(|(lhs, rhs)| (*lhs as u8) == *rhs)
}

unsafe extern "system" fn frame_query_interface(
    this: *mut vst3::Steinberg::FUnknown,
    iid: *const vst3::Steinberg::TUID,
    obj: *mut *mut c_void,
) -> vst3::Steinberg::tresult {
    if this.is_null() || iid.is_null() || obj.is_null() {
        return vst3::Steinberg::kInvalidArgument;
    }
    let requested_frame = tuid_matches(iid, &vst3::Steinberg::IPlugFrame::IID);
    let requested_unknown = tuid_matches(iid, &vst3::Steinberg::FUnknown::IID);
    if !(requested_frame || requested_unknown) {
        unsafe { *obj = std::ptr::null_mut() };
        return vst3::Steinberg::kNoInterface;
    }
    let frame = this as *mut HostPlugFrame;
    unsafe {
        (*frame).ref_count.fetch_add(1, Ordering::Relaxed);
        *obj = this.cast::<c_void>();
    }
    vst3::Steinberg::kResultOk
}

unsafe extern "system" fn frame_add_ref(this: *mut vst3::Steinberg::FUnknown) -> u32 {
    if this.is_null() {
        return 0;
    }
    let frame = this as *mut HostPlugFrame;
    unsafe { (*frame).ref_count.fetch_add(1, Ordering::Relaxed) + 1 }
}

unsafe extern "system" fn frame_release(this: *mut vst3::Steinberg::FUnknown) -> u32 {
    if this.is_null() {
        return 0;
    }
    let frame = this as *mut HostPlugFrame;
    unsafe { (*frame).ref_count.fetch_sub(1, Ordering::Relaxed) - 1 }
}

unsafe extern "system" fn frame_resize_view(
    this: *mut vst3::Steinberg::IPlugFrame,
    _view: *mut vst3::Steinberg::IPlugView,
    new_size: *mut vst3::Steinberg::ViewRect,
) -> vst3::Steinberg::tresult {
    if this.is_null() || new_size.is_null() {
        return vst3::Steinberg::kInvalidArgument;
    }
    let frame = this as *mut HostPlugFrame;
    let width = unsafe { ((*new_size).right - (*new_size).left).max(1) };
    let height = unsafe { ((*new_size).bottom - (*new_size).top).max(1) };
    unsafe {
        if !(*frame).display.is_null() && (*frame).window != 0 {
            XResizeWindow(
                (*frame).display,
                (*frame).window,
                width as c_uint,
                height as c_uint,
            );
            if (*frame).embed_window != 0 {
                XResizeWindow(
                    (*frame).display,
                    (*frame).embed_window,
                    width as c_uint,
                    height as c_uint,
                );
            }
            XFlush((*frame).display);
        }
    }
    vst3::Steinberg::kResultOk
}

static HOST_PLUG_FRAME_VTBL: vst3::Steinberg::IPlugFrameVtbl = vst3::Steinberg::IPlugFrameVtbl {
    base: vst3::Steinberg::FUnknownVtbl {
        queryInterface: frame_query_interface,
        addRef: frame_add_ref,
        release: frame_release,
    },
    resizeView: frame_resize_view,
};

#[repr(C)]
#[derive(Copy, Clone)]
union XEvent {
    type_: c_int,
    xclient: XClientMessageEvent,
    xconfigure: XConfigureEvent,
    pad: [c_long; 24],
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XClientMessageData {
    longs: [c_long; 5],
}

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

#[repr(C)]
#[derive(Copy, Clone)]
struct XConfigureEvent {
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
struct XErrorEvent {
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
    eprintln!(
        "X11 error during VST3 UI hosting: error_code={} request_code={} minor_code={} resource=0x{:x}",
        ev.error_code, ev.request_code, ev.minor_code, ev.resourceid
    );
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
    fn XInitThreads() -> c_int;
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
    fn XSelectInput(display: *mut c_void, window: c_ulong, event_mask: c_long) -> c_int;
    fn XInternAtom(
        display: *mut c_void,
        atom_name: *const c_char,
        only_if_exists: c_int,
    ) -> c_ulong;
    fn XSetWMProtocols(
        display: *mut c_void,
        window: c_ulong,
        protocols: *mut c_ulong,
        count: c_int,
    ) -> c_int;
    fn XMapRaised(display: *mut c_void, window: c_ulong) -> c_int;
    fn XMapSubwindows(display: *mut c_void, window: c_ulong) -> c_int;
    fn XResizeWindow(display: *mut c_void, window: c_ulong, width: c_uint, height: c_uint)
    -> c_int;
    fn XMoveResizeWindow(
        display: *mut c_void,
        window: c_ulong,
        x: c_int,
        y: c_int,
        width: c_uint,
        height: c_uint,
    ) -> c_int;
    fn XDestroyWindow(display: *mut c_void, window: c_ulong) -> c_int;
    fn XQueryTree(
        display: *mut c_void,
        window: c_ulong,
        root_return: *mut c_ulong,
        parent_return: *mut c_ulong,
        children_return: *mut *mut c_ulong,
        nchildren_return: *mut c_uint,
    ) -> c_int;
    fn XFree(data: *mut c_void) -> c_int;
    fn XSync(display: *mut c_void, discard: c_int) -> c_int;
    fn XPending(display: *mut c_void) -> c_int;
    fn XNextEvent(display: *mut c_void, event_return: *mut XEvent) -> c_int;
    fn XFlush(display: *mut c_void) -> c_int;
    fn XSendEvent(
        display: *mut c_void,
        w: c_ulong,
        propagate: c_int,
        event_mask: c_long,
        event_send: *mut XEvent,
    ) -> c_int;
    fn XSetErrorHandler(handler: Option<XErrorHandler>) -> Option<XErrorHandler>;
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

pub fn open_editor_blocking(
    plugin_path: &str,
    plugin_name: &str,
    plugin_id: &str,
) -> Result<(), String> {
    let path = Path::new(plugin_path);
    let factory = PluginFactory::from_module(path)?;
    let class_count = factory.count_classes();
    if class_count <= 0 {
        return Err("No VST3 classes found".to_string());
    }
    let class_info = if !plugin_id.is_empty() {
        let mut found = None;
        for i in 0..class_count {
            if let Some(ci) = factory.get_class_info(i) {
                let cid = format!("{:02X?}", ci.cid);
                if cid == plugin_id {
                    found = Some(ci);
                    break;
                }
            }
        }
        found.ok_or_else(|| format!("Failed to find VST3 class for plugin id: {plugin_id}"))?
    } else {
        factory
            .get_class_info(0)
            .ok_or("Failed to get VST3 class info")?
    };
    let mut instance = factory.create_instance(&class_info.cid)?;
    instance.initialize(&factory)?;
    let controller = instance
        .edit_controller
        .clone()
        .ok_or("VST3 plugin has no edit controller")?;
    let title = if plugin_name.is_empty() {
        class_info.name
    } else {
        plugin_name.to_string()
    };
    let result = run_vst3_x11_editor(controller, title);
    let _ = instance.terminate();
    result
}

fn run_vst3_x11_editor(
    controller: vst3::ComPtr<vst3::Steinberg::Vst::IEditController>,
    title: String,
) -> Result<(), String> {
    let _ = ensure_x11_threads();

    let display = unsafe { XOpenDisplay(std::ptr::null()) };
    if display.is_null() {
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
        return Err("Failed to create X11 window for VST3 editor".to_string());
    }
    let embed_window =
        unsafe { XCreateSimpleWindow(display, window, 0, 0, 900, 600, 0, black, white) };
    if embed_window == 0 {
        unsafe {
            let _ = XDestroyWindow(display, window);
            let _ = XCloseDisplay(display);
        }
        return Err("Failed to create X11 embed window for VST3 editor".to_string());
    }

    let view_ptr = unsafe { controller.createView(ViewType::kEditor as *const i8) };
    if view_ptr.is_null() {
        unsafe {
            let _ = XDestroyWindow(display, window);
            let _ = XCloseDisplay(display);
        }
        return Err("VST3 plugin does not expose an editor view".to_string());
    }
    let view = unsafe { vst3::ComPtr::from_raw(view_ptr) }
        .ok_or("Failed to manage VST3 editor view pointer")?;
    let x11 =
        unsafe { view.isPlatformTypeSupported(vst3::Steinberg::kPlatformTypeX11EmbedWindowID) };
    if x11 != kResultTrue && x11 != vst3::Steinberg::kResultOk {
        unsafe {
            let _ = XDestroyWindow(display, window);
            let _ = XCloseDisplay(display);
        }
        return Err("VST3 editor does not support X11 embedding".to_string());
    }

    let mut frame = Box::new(HostPlugFrame::new(display, window, embed_window));
    let frame_ptr = &mut frame.iface as *mut vst3::Steinberg::IPlugFrame;

    let mut rect = vst3::Steinberg::ViewRect {
        left: 0,
        top: 0,
        right: 900,
        bottom: 600,
    };
    let _ = unsafe { view.getSize(&mut rect) };
    let width = (rect.right - rect.left).max(320);
    let height = (rect.bottom - rect.top).max(240);

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

    rect.left = 0;
    rect.top = 0;
    rect.right = width;
    rect.bottom = height;
    let _ = unsafe { view.setFrame(frame_ptr) };
    let attached = unsafe {
        view.attached(
            embed_window as usize as *mut c_void,
            vst3::Steinberg::kPlatformTypeX11EmbedWindowID as *const i8,
        )
    };
    if attached != vst3::Steinberg::kResultOk && attached != vst3::Steinberg::kResultTrue {
        unsafe {
            let _ = XDestroyWindow(display, window);
            let _ = XCloseDisplay(display);
        }
        return Err(format!("VST3 editor attach failed (result: {attached})"));
    }
    let _ = unsafe { view.onSize(&mut rect) };
    let _ = unsafe { view.onFocus(1) };
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
        pump_host_run_loop();
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
                rect.left = 0;
                rect.top = 0;
                rect.right = w;
                rect.bottom = h;
                let _ = unsafe { view.onSize(&mut rect) };
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

    let _ = unsafe { view.onFocus(0) };
    let _ = unsafe { view.setFrame(std::ptr::null_mut()) };
    let _ = unsafe { view.removed() };
    unsafe {
        let _ = XSync(display, 0);
        let _ = XDestroyWindow(display, window);
        let _ = XSync(display, 0);
        let _ = XCloseDisplay(display);
    }
    Ok(())
}
