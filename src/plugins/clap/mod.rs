use crate::consts::plugins_clap::{
    CLIENT_MESSAGE, DESTROY_NOTIFY, STRUCTURE_NOTIFY_MASK, UNMAP_NOTIFY,
};
#[cfg(all(unix, not(target_os = "macos")))]
use crate::plugins::x11::{
    XBlackPixel, XCloseDisplay, XCreateSimpleWindow, XDefaultScreen, XDestroyWindow, XEvent,
    XFlush, XInternAtom, XMapRaised, XNextEvent, XOpenDisplay, XPending, XResizeWindow,
    XRootWindow, XSelectInput, XSetWMProtocols, XStoreName, XSync, XWhitePixel,
    set_dialog_window_type,
};
use maolan_engine::clap::ClapProcessor;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

#[cfg(all(unix, not(target_os = "macos")))]
use std::ffi::{CString, c_int, c_uint, c_ulong, c_void};

#[derive(Debug, Clone)]
pub(crate) struct ClapUiClosedState {
    pub track_name: String,
    pub clip_idx: Option<usize>,
    pub instance_id: usize,
    pub plugin_path: String,
    pub state: maolan_engine::clap::ClapPluginState,
}

#[derive(Debug, Clone)]
pub(crate) struct ClapUiParamUpdate {
    pub track_name: String,
    pub clip_idx: Option<usize>,
    pub instance_id: usize,
    pub param_id: u32,
    pub value: f64,
}

#[derive(Debug, Clone)]
pub(crate) struct ClapUiStateUpdate {
    pub track_name: String,
    pub clip_idx: Option<usize>,
    pub instance_id: usize,
    pub plugin_path: String,
    pub state: maolan_engine::clap::ClapPluginState,
}

pub struct GuiClapUiHost {
    closed_tx: mpsc::Sender<ClapUiClosedState>,
    closed_rx: mpsc::Receiver<ClapUiClosedState>,
    param_tx: mpsc::Sender<ClapUiParamUpdate>,
    param_rx: mpsc::Receiver<ClapUiParamUpdate>,
    state_tx: mpsc::Sender<ClapUiStateUpdate>,
    state_rx: mpsc::Receiver<ClapUiStateUpdate>,
    active_ui_sessions: Arc<AtomicUsize>,
    pending_closed_states: Arc<AtomicUsize>,
    pending_param_updates: Arc<AtomicUsize>,
    pending_state_updates: Arc<AtomicUsize>,
    active_session_keys: Arc<Mutex<HashSet<ClapUiSessionKey>>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ClapUiSessionKey {
    track_name: String,
    clip_idx: Option<usize>,
    instance_id: usize,
}

impl GuiClapUiHost {
    pub fn new() -> Self {
        let (closed_tx, closed_rx) = mpsc::channel();
        let (param_tx, param_rx) = mpsc::channel();
        let (state_tx, state_rx) = mpsc::channel();
        let active_ui_sessions = Arc::new(AtomicUsize::new(0));
        let pending_closed_states = Arc::new(AtomicUsize::new(0));
        let pending_param_updates = Arc::new(AtomicUsize::new(0));
        let pending_state_updates = Arc::new(AtomicUsize::new(0));
        let active_session_keys = Arc::new(Mutex::new(HashSet::new()));
        Self {
            closed_tx,
            closed_rx,
            param_tx,
            param_rx,
            state_tx,
            state_rx,
            active_ui_sessions,
            pending_closed_states,
            pending_param_updates,
            pending_state_updates,
            active_session_keys,
        }
    }

    pub fn drain_closed_states(&mut self) -> Vec<ClapUiClosedState> {
        let mut states = Vec::new();
        while let Ok(state) = self.closed_rx.try_recv() {
            self.pending_closed_states.fetch_sub(1, Ordering::Relaxed);
            states.push(state);
        }
        states
    }

    pub fn pop_param_update(&mut self) -> Option<ClapUiParamUpdate> {
        if let Ok(update) = self.param_rx.try_recv() {
            self.pending_param_updates.fetch_sub(1, Ordering::Relaxed);
            Some(update)
        } else {
            None
        }
    }

    pub fn pop_state_update(&mut self) -> Option<ClapUiStateUpdate> {
        if let Ok(update) = self.state_rx.try_recv() {
            self.pending_state_updates.fetch_sub(1, Ordering::Relaxed);
            Some(update)
        } else {
            None
        }
    }

    pub fn has_pending_ui_work(&self) -> bool {
        self.active_ui_sessions.load(Ordering::Acquire) != 0
            || self.pending_closed_states.load(Ordering::Acquire) != 0
            || self.pending_param_updates.load(Ordering::Acquire) != 0
            || self.pending_state_updates.load(Ordering::Acquire) != 0
    }

    pub fn open_editor(
        &mut self,
        track_name: &str,
        clip_idx: Option<usize>,
        instance_id: usize,
        plugin_spec: &str,
        processor: Arc<ClapProcessor>,
    ) -> Result<(), String> {
        let session_key = ClapUiSessionKey {
            track_name: track_name.to_string(),
            clip_idx,
            instance_id,
        };
        if let Ok(mut active) = self.active_session_keys.lock()
            && !active.insert(session_key.clone())
        {
            return Ok(());
        }
        let track_name = track_name.to_string();
        let plugin_path = plugin_spec.to_string();
        let closed_tx = self.closed_tx.clone();
        let param_tx = self.param_tx.clone();
        let state_tx = self.state_tx.clone();
        let active_ui_sessions = self.active_ui_sessions.clone();
        let active_ui_sessions_spawn = active_ui_sessions.clone();
        let active_session_keys = self.active_session_keys.clone();
        let session_key_for_thread = session_key.clone();
        let pending_closed_states = self.pending_closed_states.clone();
        let pending_param_updates = self.pending_param_updates.clone();
        let pending_state_updates = self.pending_state_updates.clone();
        active_ui_sessions.fetch_add(1, Ordering::AcqRel);
        thread::Builder::new()
            .name("clap-ui".to_string())
            .spawn(move || {
                match open_editor_blocking(
                    &plugin_path,
                    processor,
                    |param_id, value| {
                        let update = ClapUiParamUpdate {
                            track_name: track_name.clone(),
                            clip_idx,
                            instance_id,
                            param_id,
                            value,
                        };
                        if param_tx.send(update).is_ok() {
                            pending_param_updates.fetch_add(1, Ordering::AcqRel);
                        }
                    },
                    |state| {
                        let update = ClapUiStateUpdate {
                            track_name: track_name.clone(),
                            clip_idx,
                            instance_id,
                            plugin_path: plugin_path.clone(),
                            state,
                        };
                        if state_tx.send(update).is_ok() {
                            pending_state_updates.fetch_add(1, Ordering::AcqRel);
                        }
                    },
                ) {
                    Ok(Some(saved_state)) => {
                        let closed = ClapUiClosedState {
                            track_name,
                            clip_idx,
                            instance_id,
                            plugin_path,
                            state: saved_state,
                        };
                        if closed_tx.send(closed).is_ok() {
                            pending_closed_states.fetch_add(1, Ordering::AcqRel);
                        }
                    }
                    Ok(None) => {}
                    Err(err) => {
                        tracing::error!("Failed to open CLAP editor: {err}");
                    }
                }
                if let Ok(mut active) = active_session_keys.lock() {
                    active.remove(&session_key_for_thread);
                }
                active_ui_sessions_spawn.fetch_sub(1, Ordering::AcqRel);
            })
            .map_err(|e| {
                if let Ok(mut active) = self.active_session_keys.lock() {
                    active.remove(&session_key);
                }
                active_ui_sessions.fetch_sub(1, Ordering::AcqRel);
                format!("Failed to spawn CLAP UI thread: {e}")
            })?;
        Ok(())
    }
}

fn open_editor_blocking(
    _plugin_spec: &str,
    processor: Arc<ClapProcessor>,
    mut on_param: impl FnMut(u32, f64),
    mut on_state: impl FnMut(maolan_engine::clap::ClapPluginState),
) -> Result<Option<maolan_engine::clap::ClapPluginState>, String> {
    let gui_info = processor.gui_info()?;
    processor.ui_begin_session();
    let result = if cfg!(all(unix, not(target_os = "macos"))) && gui_info.api == "x11" {
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            if gui_info.supports_embedded {
                run_x11_embedded(processor.clone(), &mut on_param, &mut on_state)
            } else {
                run_x11_floating(processor.clone(), &mut on_param, &mut on_state)
            }
        }
        #[cfg(not(all(unix, not(target_os = "macos"))))]
        {
            Err("X11 CLAP UI is unavailable on this platform".to_string())
        }
    } else {
        processor.gui_create(&gui_info.api, !gui_info.supports_embedded)?;
        let result = (|| {
            processor.gui_show()?;
            run_ui_loop(&processor, || false, &mut on_param, &mut on_state);
            Ok(())
        })();
        processor.gui_hide();
        processor.gui_destroy();
        result
    };
    processor.ui_end_session();
    result?;
    Ok(processor.snapshot_state().ok())
}

fn run_ui_loop<F>(
    processor: &Arc<ClapProcessor>,
    mut pump_platform_close: F,
    on_param: &mut impl FnMut(u32, f64),
    on_state: &mut impl FnMut(maolan_engine::clap::ClapPluginState),
) where
    F: FnMut() -> bool,
{
    loop {
        if processor.ui_should_close() || pump_platform_close() {
            break;
        }
        processor.gui_on_main_thread();
        for timer_id in processor.ui_take_due_timers() {
            processor.gui_on_timer(timer_id);
        }
        emit_param_updates(processor, on_param);
        emit_state_updates(processor, on_state);
        thread::sleep(Duration::from_millis(16));
    }
}

fn emit_param_updates(processor: &Arc<ClapProcessor>, on_param: &mut impl FnMut(u32, f64)) {
    for update in processor.ui_take_param_updates() {
        if update.value.is_finite() {
            on_param(update.param_id, update.value);
        }
    }
}

fn emit_state_updates(
    processor: &Arc<ClapProcessor>,
    on_state: &mut impl FnMut(maolan_engine::clap::ClapPluginState),
) {
    if let Some(state) = processor.ui_take_state_update() {
        on_state(state);
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn run_x11_embedded(
    processor: Arc<ClapProcessor>,
    on_param: &mut impl FnMut(u32, f64),
    on_state: &mut impl FnMut(maolan_engine::clap::ClapPluginState),
) -> Result<(), String> {
    let display = unsafe { XOpenDisplay(std::ptr::null()) };
    if display.is_null() {
        return Err("Failed to open X display for CLAP UI".to_string());
    }

    let screen = unsafe { XDefaultScreen(display) };
    let root = unsafe { XRootWindow(display, screen) };
    let black = unsafe { XBlackPixel(display, screen) };
    let white = unsafe { XWhitePixel(display, screen) };

    let mut width = 900u32;
    let mut height = 600u32;

    let window =
        unsafe { XCreateSimpleWindow(display, root, 120, 120, width, height, 1, black, white) };
    if window == 0 {
        unsafe { XCloseDisplay(display) };
        return Err("Failed to create X11 window for CLAP UI".to_string());
    }

    let embed_window =
        unsafe { XCreateSimpleWindow(display, window, 0, 0, width, height, 0, black, white) };
    if embed_window == 0 {
        unsafe {
            XDestroyWindow(display, window);
            XCloseDisplay(display);
        }
        return Err("Failed to create X11 embed window for CLAP UI".to_string());
    }

    let title = CString::new("CLAP Plugin").map_err(|e| e.to_string())?;
    unsafe {
        XStoreName(display, window, title.as_ptr());
        XSelectInput(display, window, STRUCTURE_NOTIFY_MASK);
        XSelectInput(display, embed_window, STRUCTURE_NOTIFY_MASK);
    }

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
    set_dialog_window_type(display, window);

    let result = (|| {
        processor.gui_create("x11", false)?;
        processor.gui_create("x11", false)?;
        if let Ok((new_width, new_height)) = processor.gui_get_size() {
            width = new_width.max(320);
            height = new_height.max(240);
            unsafe {
                XResizeWindow(display, window, width, height);
                XResizeWindow(display, embed_window, width, height);
            }
        }
        processor.gui_set_parent_x11(embed_window as usize)?;

        processor.gui_show()?;
        unsafe {
            XMapRaised(display, embed_window);
            XMapRaised(display, window);
            XFlush(display);
        }
        run_ui_loop(
            &processor,
            || pump_platform_events_x11(display, window, wm_delete, wm_protocols),
            on_param,
            on_state,
        );
        Ok(())
    })();

    processor.gui_hide();
    processor.gui_destroy();
    unsafe {
        XDestroyWindow(display, embed_window);
        XDestroyWindow(display, window);
        XSync(display, 0);
        XCloseDisplay(display);
    }
    result
}

#[cfg(all(unix, not(target_os = "macos")))]
fn run_x11_floating(
    processor: Arc<ClapProcessor>,
    on_param: &mut impl FnMut(u32, f64),
    on_state: &mut impl FnMut(maolan_engine::clap::ClapPluginState),
) -> Result<(), String> {
    let display = unsafe { XOpenDisplay(std::ptr::null()) };
    if display.is_null() {
        return Err("Failed to open X display for CLAP UI".to_string());
    }

    let screen = unsafe { XDefaultScreen(display) };
    let root = unsafe { XRootWindow(display, screen) };
    let windows_before = get_top_level_windows(display, root);

    let result = (|| {
        processor.gui_create("x11", true)?;
        processor.gui_show()?;
        thread::sleep(Duration::from_millis(200));
        unsafe { XSync(display, 0) };

        let windows_after = get_top_level_windows(display, root);
        let new_top_levels: Vec<c_ulong> = windows_after
            .into_iter()
            .filter(|window| !windows_before.contains(window))
            .collect();
        let mut tracked_windows = new_top_levels;

        let wm_delete_atom_name = CString::new("WM_DELETE_WINDOW").map_err(|e| e.to_string())?;
        let wm_delete = unsafe { XInternAtom(display, wm_delete_atom_name.as_ptr(), 0) };
        let wm_protocols_atom_name = CString::new("WM_PROTOCOLS").map_err(|e| e.to_string())?;
        let wm_protocols = unsafe { XInternAtom(display, wm_protocols_atom_name.as_ptr(), 0) };

        if !tracked_windows.is_empty() {
            for window in &tracked_windows {
                unsafe {
                    XSelectInput(display, *window, STRUCTURE_NOTIFY_MASK);
                }
                set_dialog_window_type(display, *window);
            }
        }

        run_ui_loop(
            &processor,
            || {
                if tracked_windows.is_empty() {
                    let windows_after = get_top_level_windows(display, root);
                    let new_top_levels: Vec<c_ulong> = windows_after
                        .into_iter()
                        .filter(|window| !windows_before.contains(window))
                        .collect();
                    if !new_top_levels.is_empty() {
                        for window in &new_top_levels {
                            unsafe {
                                XSelectInput(display, *window, STRUCTURE_NOTIFY_MASK);
                            }
                            set_dialog_window_type(display, *window);
                        }
                        tracked_windows.extend(new_top_levels);
                    }
                    false
                } else {
                    pump_platform_events_x11_any(display, &tracked_windows, wm_delete, wm_protocols)
                        || tracked_windows_gone(display, root, &tracked_windows)
                }
            },
            on_param,
            on_state,
        );
        Ok(())
    })();

    processor.gui_hide();
    processor.gui_destroy();
    unsafe {
        XSync(display, 0);
        XCloseDisplay(display);
    }
    result
}

#[cfg(all(unix, not(target_os = "macos")))]
fn get_top_level_windows(display: *mut c_void, root: c_ulong) -> Vec<c_ulong> {
    let mut windows = Vec::new();
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
            root,
            &mut root_return,
            &mut parent_return,
            &mut children,
            &mut nchildren,
        ) != 0
            && !children.is_null()
        {
            windows.reserve(nchildren as usize);
            for i in 0..nchildren {
                windows.push(*children.add(i as usize));
            }
            XFree(children.cast::<c_void>());
        }
    }
    windows
}

#[cfg(all(unix, not(target_os = "macos")))]
fn tracked_windows_gone(display: *mut c_void, root: c_ulong, tracked_windows: &[c_ulong]) -> bool {
    if display.is_null() || root == 0 || tracked_windows.is_empty() {
        return false;
    }
    let visible = get_top_level_windows(display, root);
    tracked_windows
        .iter()
        .all(|window| !visible.contains(window))
}

#[cfg(all(unix, not(target_os = "macos")))]
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
                if event_type == DESTROY_NOTIFY {
                    return true;
                }
                if event_type == UNMAP_NOTIFY && event.xunmap.window == window {
                    return true;
                }
                if event_type == CLIENT_MESSAGE {
                    let msg = event.xclient;
                    if wm_delete != 0
                        && wm_protocols != 0
                        && msg.window == window
                        && msg.message_type == wm_protocols
                        && msg.format == 32
                        && (msg.data.longs[0] as c_ulong) == wm_delete
                    {
                        return true;
                    }
                }
            }
        }
    }
    false
}

#[cfg(all(unix, not(target_os = "macos")))]
fn pump_platform_events_x11_any(
    display: *mut c_void,
    windows: &[c_ulong],
    wm_delete: c_ulong,
    wm_protocols: c_ulong,
) -> bool {
    if display.is_null() || windows.is_empty() {
        return false;
    }
    unsafe {
        let pending = XPending(display);
        if pending <= 0 {
            return false;
        }
        let mut event: XEvent = std::mem::zeroed();
        for _ in 0..pending {
            XNextEvent(display, &mut event);
            let event_type = event.type_;
            if event_type == DESTROY_NOTIFY && windows.contains(&event.xdestroywindow.window) {
                return true;
            }
            if event_type == UNMAP_NOTIFY && windows.contains(&event.xunmap.window) {
                return true;
            }
            if event_type == CLIENT_MESSAGE {
                let msg = event.xclient;
                if wm_delete != 0
                    && wm_protocols != 0
                    && windows.contains(&msg.window)
                    && msg.message_type == wm_protocols
                    && msg.format == 32
                    && (msg.data.longs[0] as c_ulong) == wm_delete
                {
                    return true;
                }
            }
        }
    }
    false
}
