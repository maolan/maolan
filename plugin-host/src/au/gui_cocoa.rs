//! Minimal Cocoa glue for AUv2 plugin views. The process main thread runs
//! NSApplication (`run_gui_main`, called from `au::run_au`); AU Cocoa views
//! dispatch to the main thread during creation and require a servicing AppKit
//! runloop. Window work is forwarded from the audio/block thread through a
//! runtime-registered dispatcher object and serviced synchronously via
//! `performSelector:onThread:withObject:waitUntilDone:`. All Objective-C
//! calls go through hand-rolled runtime FFI; the unsafe surface is kept to
//! this file.

use super::AuUnit;
use std::ffi::{CString, c_char, c_void};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

type Sel = *const c_char;
type Class = *mut c_void;
type Id = *mut c_void;

#[repr(C)]
struct NSSize {
    width: f64,
    height: f64,
}

#[link(name = "objc")]
unsafe extern "C" {
    fn objc_msgSend();
    fn sel_registerName(name: *const c_char) -> Sel;
    fn objc_getClass(name: *const c_char) -> Class;
    fn objc_allocateClassPair(superclass: Class, name: *const c_char, extra_bytes: usize) -> Class;
    fn objc_registerClassPair(cls: Class);
    fn class_addMethod(cls: Class, name: Sel, imp: *mut c_void, types: *const c_char) -> bool;
    fn class_getInstanceMethod(cls: Class, sel: Sel) -> *mut c_void;
}

// Linking AppKit makes its classes visible to `objc_getClass`; without this
// the NS* class lookups return null.
#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {}

fn msg_send_addr() -> usize {
    objc_msgSend as *const () as usize
}

fn sel(name: &str) -> Sel {
    let c = CString::new(name).expect("selector has no nul");
    unsafe { sel_registerName(c.as_ptr()) }
}

fn class(name: &str) -> Class {
    let c = CString::new(name).expect("class name has no nul");
    unsafe { objc_getClass(c.as_ptr()) }
}

/// Typed `objc_msgSend` shims. Transmuting the symbol to a signature whose
/// argument/return types exactly match the callee is the standard way to
/// call ObjC methods from Rust; the macros exist so call sites cannot pick
/// the wrong arity.
macro_rules! msg0 {
    ($obj:expr, $sel:expr) => {{
        let f: unsafe extern "C" fn(Id, Sel) -> Id =
            unsafe { std::mem::transmute(msg_send_addr()) };
        unsafe { f($obj, $sel) }
    }};
}

macro_rules! msg1 {
    ($obj:expr, $sel:expr, $a:expr) => {{
        let f: unsafe extern "C" fn(Id, Sel, usize) -> Id =
            unsafe { std::mem::transmute(msg_send_addr()) };
        unsafe { f($obj, $sel, $a as usize) }
    }};
}

macro_rules! msg4 {
    ($obj:expr, $sel:expr, $a:expr, $b:expr, $c:expr, $d:expr) => {{
        let f: unsafe extern "C" fn(Id, Sel, usize, usize, usize, usize) -> Id =
            unsafe { std::mem::transmute(msg_send_addr()) };
        unsafe {
            f(
                $obj,
                $sel,
                $a as usize,
                $b as usize,
                $c as usize,
                $d as usize,
            )
        }
    }};
}

fn ns_string(s: &str) -> Id {
    let c = CString::new(s).unwrap_or_else(|_| CString::new("plugin").expect("cstr"));
    msg1!(
        class("NSString"),
        sel("stringWithUTF8String:"),
        c.as_ptr() as usize
    )
}

/// IMP for the dispatcher's `dispatch:` method. The argument is an NSNumber
/// created with `numberWithUnsignedLongLong:` whose value is the address of
/// a `Box<dyn FnOnce() + Send>`; ownership transfers to this call. Runs on
/// the GUI (main) thread only, once per NSNumber.
extern "C" fn dispatch_imp(_self: Id, _cmd: Sel, number: Id) {
    let value: u64 = {
        let f: unsafe extern "C" fn(Id, Sel) -> u64 =
            unsafe { std::mem::transmute(msg_send_addr()) };
        unsafe { f(number, sel("unsignedLongLongValue")) }
    };
    if value == 0 {
        return;
    }
    let boxed = value as *mut Box<dyn FnOnce() + Send>;
    // SAFETY: the pointer was leaked by `GuiThread::dispatch` with exactly
    // one Box; ownership is consumed here on the GUI thread.
    let closure = unsafe { Box::from_raw(boxed) };
    closure();
}

static DISPATCHER_CLASS: OnceLock<usize> = OnceLock::new();

fn dispatcher_class() -> Result<Class, String> {
    let ptr = *DISPATCHER_CLASS.get_or_init(|| {
        let cls =
            unsafe { objc_allocateClassPair(class("NSObject"), c"MaolanGuiDispatch".as_ptr(), 0) };
        if cls.is_null() {
            return 0;
        }
        let ok = unsafe {
            class_addMethod(
                cls,
                sel("dispatch:"),
                dispatch_imp as *mut c_void,
                c"v@:@".as_ptr(),
            )
        };
        if !ok {
            return 0;
        }
        unsafe { objc_registerClassPair(cls) };
        cls as usize
    });
    if ptr == 0 {
        Err("failed to register MaolanGuiDispatch Objective-C class".to_string())
    } else {
        Ok(ptr as Class)
    }
}

struct GuiThreadInner {
    // Main-thread object pointers, valid for the process lifetime.
    ns_thread: usize,
    dispatcher: usize,
}

static GUI_STATE: OnceLock<(Mutex<Option<Arc<GuiThreadInner>>>, Condvar)> = OnceLock::new();

fn gui_state() -> &'static (Mutex<Option<Arc<GuiThreadInner>>>, Condvar) {
    GUI_STATE.get_or_init(|| (Mutex::new(None), Condvar::new()))
}

/// Handle to the GUI (main) thread. All window operations dispatch to it
/// synchronously, so calls from the audio/block thread are safe.
pub struct GuiThread {
    inner: Arc<GuiThreadInner>,
}

unsafe impl Send for GuiThread {}
unsafe impl Sync for GuiThread {}

/// Run NSApplication on the current thread, which must be the process main
/// thread. Never returns; the process exits from the block loop.
pub fn run_gui_main() -> ! {
    let result = (|| -> Result<GuiThreadInner, String> {
        let cls = dispatcher_class()?;
        let dispatcher = msg0!(cls, sel("alloc"));
        let dispatcher = msg0!(dispatcher, sel("init"));
        if dispatcher.is_null() {
            return Err("failed to create dispatcher instance".to_string());
        }
        let app = msg0!(class("NSApplication"), sel("sharedApplication"));
        if app.is_null() {
            return Err("NSApplication sharedApplication returned nil".to_string());
        }
        // 0 = NSApplicationActivationPolicyRegular.
        msg1!(app, sel("setActivationPolicy:"), 0usize);
        msg0!(app, sel("finishLaunching"));
        msg1!(app, sel("activateIgnoringOtherApps:"), 1usize);
        let ns_thread = msg0!(class("NSThread"), sel("currentThread"));
        Ok(GuiThreadInner {
            ns_thread: ns_thread as usize,
            dispatcher: dispatcher as usize,
        })
    })();
    match result {
        Ok(inner) => {
            let (mutex, condvar) = gui_state();
            if let Ok(mut g) = mutex.lock() {
                *g = Some(Arc::new(inner));
            }
            condvar.notify_all();
            let app = msg0!(class("NSApplication"), sel("sharedApplication"));
            // Services `performSelector:onThread:waitUntilDone:` and main-
            // queue dispatches from AU view code until the process exits.
            msg1!(app, sel("run"), 0usize);
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("maolan-plugin-host: AU GUI init failed: {e}");
            std::process::exit(1);
        }
    }
}

impl GuiThread {
    /// Wait for the GUI (main) thread to finish its NSApplication setup.
    pub fn start() -> Result<GuiThread, String> {
        let (mutex, condvar) = gui_state();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut guard = mutex
            .lock()
            .map_err(|e| format!("GUI state lock failed: {e}"))?;
        while guard.is_none() {
            let now = std::time::Instant::now();
            if now >= deadline {
                return Err("GUI thread did not start within 10s".to_string());
            }
            let (g, _timeout) = condvar
                .wait_timeout(guard, deadline - now)
                .map_err(|e| format!("GUI state wait failed: {e}"))?;
            guard = g;
        }
        let inner = guard.as_ref().expect("checked non-none").clone();
        Ok(GuiThread { inner })
    }

    /// Run `f` on the GUI thread, blocking until it completes.
    pub fn dispatch<F>(&self, f: F) -> Result<(), String>
    where
        F: FnOnce() + Send + 'static,
    {
        let boxed: Box<dyn FnOnce() + Send> = Box::new(f);
        let raw = Box::into_raw(Box::new(boxed)) as u64;
        let number = msg1!(
            class("NSNumber"),
            sel("numberWithUnsignedLongLong:"),
            raw as usize
        );
        if number.is_null() {
            // Recover the leaked box so it is not lost.
            drop(unsafe { Box::from_raw(raw as *mut Box<dyn FnOnce() + Send>) });
            return Err("NSNumber creation failed".to_string());
        }
        // SAFETY: the dispatcher IMP on the main thread takes ownership of
        // the boxed closure and consumes it exactly once. waitUntilDone=YES
        // keeps the lifetime of `number` valid for the whole call; the main
        // thread is inside -[NSApplication run] and services the selector.
        msg4!(
            self.inner.dispatcher as Id,
            sel("performSelector:onThread:withObject:waitUntilDone:"),
            sel("dispatch:") as usize,
            self.inner.ns_thread,
            number as usize,
            1usize
        );
        Ok(())
    }
}

/// Owned NSWindow hosting an AU Cocoa view. Every method dispatches to the
/// GUI thread, so the raw pointers are only touched there.
pub struct CocoaWindow {
    thread: GuiThread,
    window: Id,
    view: Id,
}

unsafe impl Send for CocoaWindow {}

impl CocoaWindow {
    /// Create a window hosting the plugin's Cocoa view. `parent` is an
    /// `NSView *` to embed into, or null for a floating window.
    pub fn create(
        thread: &GuiThread,
        title: &str,
        unit: &dyn AuUnit,
        parent: *mut c_void,
    ) -> Result<CocoaWindow, String> {
        let (bundle_path, class_name) = unit
            .cocoa_ui()
            .ok_or_else(|| "AU component does not publish a Cocoa UI".to_string())?;
        let title = title.to_string();
        let bundle_path = bundle_path.to_string();
        let class_name = class_name.to_string();
        let au_unit = unit.raw_unit() as usize;
        let (tx, rx) = std::sync::mpsc::channel::<Result<(usize, usize), String>>();
        thread.dispatch(move || {
            let result =
                create_view_on_gui_thread(&title, &bundle_path, &class_name, au_unit as Id);
            let _ = tx.send(result.map(|(w, v)| (w as usize, v as usize)));
        })?;
        let (window, view) = rx
            .recv()
            .map_err(|e| format!("GUI thread response lost: {e}"))??;
        let window = CocoaWindow {
            thread: GuiThread::start()?,
            window: window as Id,
            view: view as Id,
        };
        if !parent.is_null() {
            let parent = parent as usize;
            let view = window.view as usize;
            window.thread.dispatch(move || {
                msg1!(parent as Id, sel("addSubview:"), view);
            })?;
        }
        Ok(window)
    }

    pub fn show(&self) {
        let window = self.window as usize;
        let _ = self.thread.dispatch(move || {
            msg1!(window as Id, sel("makeKeyAndOrderFront:"), 0usize);
            let app = msg0!(class("NSApplication"), sel("sharedApplication"));
            msg1!(app, sel("activateIgnoringOtherApps:"), 1usize);
        });
    }

    pub fn hide(&self) {
        let window = self.window as usize;
        let _ = self.thread.dispatch(move || {
            msg1!(window as Id, sel("orderOut:"), 0usize);
        });
    }

    pub fn close(&self) {
        let window = self.window as usize;
        let view = self.view as usize;
        let _ = self.thread.dispatch(move || {
            if view != 0 {
                msg0!(view as Id, sel("removeFromSuperview"));
            }
            msg0!(window as Id, sel("close"));
        });
    }
}

impl Drop for CocoaWindow {
    fn drop(&mut self) {
        self.close();
    }
}

fn create_view_on_gui_thread(
    title: &str,
    bundle_path: &str,
    class_name: &str,
    au_unit: *mut c_void,
) -> Result<(Id, Id), String> {
    let mut view_class: Class = std::ptr::null_mut();
    if !bundle_path.is_empty() {
        let path = ns_string(bundle_path);
        let bundle = msg1!(class("NSBundle"), sel("bundleWithPath:"), path as usize);
        if !bundle.is_null() {
            let name = ns_string(class_name);
            view_class = msg1!(bundle, sel("classNamed:"), name as usize);
        }
    }
    if view_class.is_null() {
        let name = ns_string(class_name);
        view_class = msg0!(name, sel("class"));
    }
    if view_class.is_null() {
        return Err(format!("AU view class '{class_name}' not found"));
    }

    // Two published contracts exist: plain NSView subclasses implement
    // -initWithAudioUnit:size:, while Apple's CoreAudioAUUI factory classes
    // (e.g. AUDelayFactory) implement -viewWithAudioUnit:size: on an
    // allocated factory instance. Probe both.
    let size = NSSize {
        width: 800.0,
        height: 600.0,
    };
    let make_view = |selector: Sel| -> Id {
        let alloc = msg0!(view_class, sel("alloc"));
        let f: unsafe extern "C" fn(Id, Sel, usize, NSSize) -> Id =
            unsafe { std::mem::transmute(msg_send_addr()) };
        unsafe { f(alloc, selector, au_unit as usize, size) }
    };
    // `class_getInstanceMethod` walks the superclass chain, which matters
    // for Apple's factory classes (e.g. AUDelayFactory) that inherit the
    // view-construction methods from a bundle-internal base class.
    let view =
        if !unsafe { class_getInstanceMethod(view_class, sel("uiViewForAudioUnit:withSize:")) }
            .is_null()
        {
            // Apple's CoreAudioAUUI factory classes (e.g. AUDelayFactory) vend
            // views through -uiViewForAudioUnit:withSize:.
            make_view(sel("uiViewForAudioUnit:withSize:"))
        } else if !unsafe { class_getInstanceMethod(view_class, sel("viewWithAudioUnit:size:")) }
            .is_null()
        {
            make_view(sel("viewWithAudioUnit:size:"))
        } else if !unsafe { class_getInstanceMethod(view_class, sel("initWithAudioUnit:size:")) }
            .is_null()
        {
            make_view(sel("initWithAudioUnit:size:"))
        } else {
            return Err(format!(
                "AU view class '{class_name}' implements no known view factory selector"
            ));
        };
    if view.is_null() {
        return Err(format!("AU view '{class_name}' failed to create a view"));
    }

    // Plain init gives a zero-rect window; style and size are set below so
    // no NSRect argument (32-byte struct) ever crosses the FFI boundary.
    let window = {
        let alloc = msg0!(class("NSWindow"), sel("alloc"));
        msg0!(alloc, sel("init"))
    };
    if window.is_null() {
        return Err("failed to create NSWindow".to_string());
    }
    // NSWindowStyleMask titled|closable|miniaturizable|resizable = 15.
    msg1!(window, sel("setStyleMask:"), 15usize);
    {
        let f: unsafe extern "C" fn(Id, Sel, NSSize) =
            unsafe { std::mem::transmute(msg_send_addr()) };
        unsafe {
            f(
                window,
                sel("setContentSize:"),
                NSSize {
                    width: 800.0,
                    height: 600.0,
                },
            )
        };
    }
    msg1!(window, sel("setContentView:"), view as usize);
    let title_ns = ns_string(title);
    msg1!(window, sel("setTitle:"), title_ns as usize);
    msg1!(view, sel("setAutoresizingMask:"), 18usize); // width+height sizable
    Ok((window, view))
}
