#[cfg(all(unix, not(target_os = "macos")))]
use lilv::{World, plugin::Plugin};
#[cfg(all(unix, not(target_os = "macos")))]
use maolan_engine::client::Client;
#[cfg(all(unix, not(target_os = "macos")))]
use maolan_engine::message::{Action, Lv2ControlPortInfo, Message as EngineMessage};
#[cfg(all(unix, not(target_os = "macos")))]
use std::collections::HashMap;
#[cfg(all(unix, not(target_os = "macos")))]
use std::ffi::{CStr, CString, c_char, c_uint, c_ulong, c_void};
#[cfg(all(unix, not(target_os = "macos")))]
use std::sync::mpsc;
#[cfg(all(unix, not(target_os = "macos")))]
use std::thread;

#[cfg(all(unix, not(target_os = "macos")))]
const GTK_WINDOW_TOPLEVEL: i32 = 0;
#[cfg(all(unix, not(target_os = "macos")))]
const LV2_URID_MAP: &str = "http://lv2plug.in/ns/ext/urid#map";
#[cfg(all(unix, not(target_os = "macos")))]
const LV2_URID_MAP_TYPO_COMPAT: &str = "http://lv2plug.in/ns//ext/urid#map";
#[cfg(all(unix, not(target_os = "macos")))]
const LV2_URID_UNMAP: &str = "http://lv2plug.in/ns/ext/urid#unmap";
#[cfg(all(unix, not(target_os = "macos")))]
const LV2_UI_GTK3: &str = "http://lv2plug.in/ns/extensions/ui#Gtk3UI";
#[cfg(all(unix, not(target_os = "macos")))]
const LV2_UI_GTK: &str = "http://lv2plug.in/ns/extensions/ui#GtkUI";
#[cfg(all(unix, not(target_os = "macos")))]
const LV2_UI_X11: &str = "http://lv2plug.in/ns/extensions/ui#X11UI";
#[cfg(all(unix, not(target_os = "macos")))]
const LV2_UI_QT4: &str = "http://lv2plug.in/ns/extensions/ui#Qt4UI";
#[cfg(all(unix, not(target_os = "macos")))]
const LV2_UI_QT5: &str = "http://lv2plug.in/ns/extensions/ui#Qt5UI";
#[cfg(all(unix, not(target_os = "macos")))]
const LV2_UI_QT6: &str = "http://lv2plug.in/ns/extensions/ui#Qt6UI";
#[cfg(all(unix, not(target_os = "macos")))]
const LV2_UI_PARENT: &str = "http://lv2plug.in/ns/extensions/ui#parent";
#[cfg(all(unix, not(target_os = "macos")))]
const LV2_UI_RESIZE: &str = "http://lv2plug.in/ns/extensions/ui#resize";
#[cfg(all(unix, not(target_os = "macos")))]
const LV2_UI_IDLE_INTERFACE: &str = "http://lv2plug.in/ns/extensions/ui#idleInterface";
#[cfg(all(unix, not(target_os = "macos")))]
const LV2_UI_SHOW_INTERFACE: &str = "http://lv2plug.in/ns/extensions/ui#showInterface";
#[cfg(all(unix, not(target_os = "macos")))]
const LV2_UI_HIDE_INTERFACE: &str = "http://lv2plug.in/ns/extensions/ui#hideInterface";
#[cfg(all(unix, not(target_os = "macos")))]
const LV2_INSTANCE_ACCESS: &str = "http://lv2plug.in/ns/ext/instance-access";

#[cfg(all(unix, not(target_os = "macos")))]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct WindowKey {
    track_name: String,
    instance_id: usize,
}

#[cfg(all(unix, not(target_os = "macos")))]
struct NativeWindowEntry {
    _window: *mut c_void,
    _widget: *mut c_void,
    suil_instance: *mut SuilInstance,
    suil_host: *mut SuilHost,
    controller: *mut NativeUiController,
    idle_source: c_uint,
    idle_data_ptr: *mut UiIdleData,
    hide_fn: Option<extern "C" fn(*mut c_void) -> i32>,
    ui_handle: *mut c_void,
    _urid_feature: UridMapFeature,
}

#[cfg(all(unix, not(target_os = "macos")))]
struct GenericWindowEntry {
    _window: *mut c_void,
}

#[cfg(all(unix, not(target_os = "macos")))]
enum WindowEntry {
    Native(NativeWindowEntry),
    Generic(GenericWindowEntry),
}

#[cfg(all(unix, not(target_os = "macos")))]
impl WindowEntry {
    fn cleanup(self) {
        match self {
            Self::Native(native) => unsafe {
                if let Some(hide) = native.hide_fn {
                    let _ = hide(native.ui_handle);
                }
                if native.idle_source != 0 {
                    g_source_remove(native.idle_source);
                }
                if !native.idle_data_ptr.is_null() {
                    drop(Box::from_raw(native.idle_data_ptr));
                }
                suil_instance_free(native.suil_instance);
                suil_host_free(native.suil_host);
                drop(Box::from_raw(native.controller));
            },
            Self::Generic(generic) => {
                let _ = generic;
            }
        }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
struct SliderCallbackData {
    track_name: String,
    instance_id: usize,
    port_index: u32,
    tx: mpsc::Sender<(String, usize, u32, f32)>,
}

#[cfg(all(unix, not(target_os = "macos")))]
unsafe impl Send for SliderCallbackData {}

#[cfg(all(unix, not(target_os = "macos")))]
struct NativeUiController {
    track_name: String,
    instance_id: usize,
    port_symbol_to_index: HashMap<String, u32>,
    tx: mpsc::Sender<(String, usize, u32, f32)>,
}

#[cfg(all(unix, not(target_os = "macos")))]
unsafe impl Send for NativeUiController {}

#[cfg(all(unix, not(target_os = "macos")))]
struct WindowCloseData {
    key: WindowKey,
    tx: mpsc::Sender<WindowKey>,
}

#[cfg(all(unix, not(target_os = "macos")))]
unsafe impl Send for WindowCloseData {}

#[cfg(all(unix, not(target_os = "macos")))]
#[derive(Default)]
struct UridMapState {
    next_urid: u32,
    by_uri: HashMap<String, u32>,
    by_urid: HashMap<u32, CString>,
}

#[cfg(all(unix, not(target_os = "macos")))]
#[repr(C)]
struct LV2FeatureRaw {
    uri: *const c_char,
    data: *mut c_void,
}

#[cfg(all(unix, not(target_os = "macos")))]
#[repr(C)]
struct LV2UridMap {
    handle: *mut c_void,
    map: extern "C" fn(handle: *mut c_void, uri: *const c_char) -> u32,
}

#[cfg(all(unix, not(target_os = "macos")))]
#[repr(C)]
struct LV2UridUnmap {
    handle: *mut c_void,
    unmap: extern "C" fn(handle: *mut c_void, urid: u32) -> *const c_char,
}

#[cfg(all(unix, not(target_os = "macos")))]
struct UridMapFeature {
    _map_uri: CString,
    _map_typo_uri: CString,
    _unmap_uri: CString,
    _map: Box<LV2UridMap>,
    _unmap: Box<LV2UridUnmap>,
    map_feature: LV2FeatureRaw,
    map_typo_feature: LV2FeatureRaw,
    unmap_feature: LV2FeatureRaw,
    _state: Box<std::sync::Mutex<UridMapState>>,
}

#[cfg(all(unix, not(target_os = "macos")))]
unsafe impl Send for UridMapFeature {}

#[cfg(all(unix, not(target_os = "macos")))]
impl UridMapFeature {
    fn new() -> Result<Self, String> {
        let map_uri = CString::new(LV2_URID_MAP).map_err(|e| e.to_string())?;
        let map_typo_uri = CString::new(LV2_URID_MAP_TYPO_COMPAT).map_err(|e| e.to_string())?;
        let unmap_uri = CString::new(LV2_URID_UNMAP).map_err(|e| e.to_string())?;
        let mut state = Box::new(std::sync::Mutex::new(UridMapState::default()));

        let map = Box::new(LV2UridMap {
            handle: (&mut *state as *mut std::sync::Mutex<UridMapState>).cast::<c_void>(),
            map: lv2_urid_map_callback,
        });
        let unmap = Box::new(LV2UridUnmap {
            handle: (&mut *state as *mut std::sync::Mutex<UridMapState>).cast::<c_void>(),
            unmap: lv2_urid_unmap_callback,
        });

        let map_feature = LV2FeatureRaw {
            uri: map_uri.as_ptr(),
            data: (&*map as *const LV2UridMap).cast::<c_void>() as *mut c_void,
        };
        let map_typo_feature = LV2FeatureRaw {
            uri: map_typo_uri.as_ptr(),
            data: (&*map as *const LV2UridMap).cast::<c_void>() as *mut c_void,
        };
        let unmap_feature = LV2FeatureRaw {
            uri: unmap_uri.as_ptr(),
            data: (&*unmap as *const LV2UridUnmap).cast::<c_void>() as *mut c_void,
        };

        Ok(Self {
            _map_uri: map_uri,
            _map_typo_uri: map_typo_uri,
            _unmap_uri: unmap_uri,
            _map: map,
            _unmap: unmap,
            map_feature,
            map_typo_feature,
            unmap_feature,
            _state: state,
        })
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
extern "C" fn lv2_urid_map_callback(handle: *mut c_void, uri: *const c_char) -> u32 {
    if handle.is_null() || uri.is_null() {
        return 0;
    }
    let Some(uri_str) = (unsafe { CStr::from_ptr(uri) }).to_str().ok() else {
        return 0;
    };
    let state = unsafe { &*(handle as *const std::sync::Mutex<UridMapState>) };
    let Ok(mut state) = state.lock() else {
        return 0;
    };
    if let Some(existing) = state.by_uri.get(uri_str) {
        return *existing;
    }

    let next = state.next_urid.saturating_add(1).max(1);
    state.next_urid = next;
    let Ok(c_uri) = CString::new(uri_str) else {
        return 0;
    };
    state.by_uri.insert(uri_str.to_string(), next);
    state.by_urid.insert(next, c_uri);
    next
}

#[cfg(all(unix, not(target_os = "macos")))]
extern "C" fn lv2_urid_unmap_callback(handle: *mut c_void, urid: u32) -> *const c_char {
    if handle.is_null() || urid == 0 {
        return std::ptr::null();
    }
    let state = unsafe { &*(handle as *const std::sync::Mutex<UridMapState>) };
    let Ok(state) = state.lock() else {
        return std::ptr::null();
    };
    state
        .by_urid
        .get(&urid)
        .map(|s| s.as_ptr())
        .unwrap_or(std::ptr::null())
}

#[cfg(all(unix, not(target_os = "macos")))]
#[derive(Debug, Clone)]
struct NativeUiSpec {
    plugin_uri: String,
    ui_uri: String,
    container_type_uri: String,
    ui_type_uri: String,
    ui_bundle_path: String,
    ui_binary_path: String,
    port_symbol_to_index: HashMap<String, u32>,
}

#[cfg(all(unix, not(target_os = "macos")))]
#[repr(C)]
struct LV2UiResize {
    handle: *mut c_void,
    ui_resize: Option<extern "C" fn(*mut c_void, i32, i32) -> i32>,
}

#[cfg(all(unix, not(target_os = "macos")))]
#[repr(C)]
struct LV2UiIdleInterface {
    idle: Option<extern "C" fn(*mut c_void) -> i32>,
}

#[cfg(all(unix, not(target_os = "macos")))]
#[repr(C)]
struct LV2UiShowInterface {
    show: Option<extern "C" fn(*mut c_void) -> i32>,
}

#[cfg(all(unix, not(target_os = "macos")))]
#[repr(C)]
struct LV2UiHideInterface {
    hide: Option<extern "C" fn(*mut c_void) -> i32>,
}

#[cfg(all(unix, not(target_os = "macos")))]
struct UiIdleData {
    interface: *const LV2UiIdleInterface,
    handle: *mut c_void,
}

#[cfg(all(unix, not(target_os = "macos")))]
unsafe impl Send for UiIdleData {}

#[cfg(all(unix, not(target_os = "macos")))]
extern "C" fn host_ui_resize(handle: *mut c_void, width: i32, height: i32) -> i32 {
    if handle.is_null() || width <= 0 || height <= 0 {
        return 1;
    }
    unsafe {
        gtk_window_resize(handle, width, height);
    }
    0
}

#[cfg(all(unix, not(target_os = "macos")))]
extern "C" fn ui_idle_tick(data: *mut c_void) -> i32 {
    if data.is_null() {
        return 0;
    }
    let idle_data = unsafe { &*(data as *const UiIdleData) };
    if idle_data.interface.is_null() {
        return 0;
    }
    let interface = unsafe { &*idle_data.interface };
    let Some(idle_fn) = interface.idle else {
        return 0;
    };
    idle_fn(idle_data.handle)
}

#[cfg(all(unix, not(target_os = "macos")))]
#[allow(non_camel_case_types)]
type SuilHost = c_void;
#[cfg(all(unix, not(target_os = "macos")))]
#[allow(non_camel_case_types)]
type SuilInstance = c_void;
#[cfg(all(unix, not(target_os = "macos")))]
type SuilController = *mut c_void;
#[cfg(all(unix, not(target_os = "macos")))]
type SuilPortWriteFunc = extern "C" fn(SuilController, u32, u32, u32, *const c_void);
#[cfg(all(unix, not(target_os = "macos")))]
type SuilPortIndexFunc = extern "C" fn(SuilController, *const c_char) -> u32;

#[cfg(all(unix, not(target_os = "macos")))]
extern "C" fn suil_write_port(
    controller: SuilController,
    port_index: u32,
    buffer_size: u32,
    protocol: u32,
    buffer: *const c_void,
) {
    if controller.is_null() || buffer.is_null() || protocol != 0 || buffer_size != 4 {
        return;
    }
    let controller = unsafe { &*(controller as *const NativeUiController) };
    let value = unsafe { *(buffer as *const f32) };
    let _ = controller.tx.send((
        controller.track_name.clone(),
        controller.instance_id,
        port_index,
        value,
    ));
}

#[cfg(all(unix, not(target_os = "macos")))]
extern "C" fn suil_port_index(controller: SuilController, port_symbol: *const c_char) -> u32 {
    if controller.is_null() || port_symbol.is_null() {
        return u32::MAX;
    }
    let controller = unsafe { &*(controller as *const NativeUiController) };
    let Some(symbol) = unsafe { CStr::from_ptr(port_symbol) }.to_str().ok() else {
        return u32::MAX;
    };
    controller
        .port_symbol_to_index
        .get(symbol)
        .copied()
        .unwrap_or(u32::MAX)
}

#[cfg(all(unix, not(target_os = "macos")))]
extern "C" fn suil_subscribe_port(
    _controller: SuilController,
    _port_index: u32,
    _protocol: u32,
    _features: *const *const LV2FeatureRaw,
) -> u32 {
    0
}

#[cfg(all(unix, not(target_os = "macos")))]
extern "C" fn suil_unsubscribe_port(
    _controller: SuilController,
    _port_index: u32,
    _protocol: u32,
    _features: *const *const LV2FeatureRaw,
) -> u32 {
    0
}

#[cfg(all(unix, not(target_os = "macos")))]
unsafe extern "C" fn on_window_close_data_destroy(data: *mut c_void, _closure: *mut c_void) {
    if data.is_null() {
        return;
    }
    unsafe {
        drop(Box::<WindowCloseData>::from_raw(data as *mut WindowCloseData));
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
unsafe extern "C" fn on_slider_data_destroy(data: *mut c_void, _closure: *mut c_void) {
    if data.is_null() {
        return;
    }
    unsafe {
        drop(Box::<SliderCallbackData>::from_raw(
            data as *mut SliderCallbackData,
        ));
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
unsafe extern "C" fn on_window_destroy(_widget: *mut c_void, data: *mut c_void) {
    if data.is_null() {
        return;
    }
    let data = unsafe { &*(data as *const WindowCloseData) };
    let _ = data.tx.send(data.key.clone());
}

#[cfg(all(unix, not(target_os = "macos")))]
unsafe extern "C" fn on_slider_changed(range: *mut c_void, data: *mut c_void) {
    if range.is_null() || data.is_null() {
        return;
    }
    let data = unsafe { &*(data as *const SliderCallbackData) };
    let value = unsafe { gtk_range_get_value(range) as f32 };
    let _ = data
        .tx
        .send((data.track_name.clone(), data.instance_id, data.port_index, value));
}

#[cfg(all(unix, not(target_os = "macos")))]
unsafe extern "C" fn on_generic_slider_changed(range: *mut c_void, data: *mut c_void) {
    unsafe { on_slider_changed(range, data) };
}

#[cfg(all(unix, not(target_os = "macos")))]
unsafe extern "C" fn on_gtk_destroy(_widget: *mut c_void, _data: *mut c_void) {
    unsafe { gtk_main_quit() };
}

#[cfg(all(unix, not(target_os = "macos")))]
pub struct GuiLv2UiHost {
    gtk_initialized: bool,
    windows: HashMap<WindowKey, WindowEntry>,
    closed_tx: mpsc::Sender<WindowKey>,
    closed_rx: mpsc::Receiver<WindowKey>,
}

#[cfg(all(unix, not(target_os = "macos")))]
impl GuiLv2UiHost {
    pub fn new() -> Self {
        let (closed_tx, closed_rx) = mpsc::channel();
        Self {
            gtk_initialized: false,
            windows: HashMap::new(),
            closed_tx,
            closed_rx,
        }
    }

    pub fn has_open_windows(&self) -> bool {
        !self.windows.is_empty()
    }

    pub fn pump(&mut self) {
        while let Ok(key) = self.closed_rx.try_recv() {
            if let Some(entry) = self.windows.remove(&key) {
                entry.cleanup();
            }
        }

        unsafe {
            while gtk_events_pending() != 0 {
                gtk_main_iteration_do(0);
            }
        }
    }

    pub fn open_editor(
        &mut self,
        track_name: String,
        instance_id: usize,
        plugin_name: String,
        plugin_uri: String,
        controls: Vec<Lv2ControlPortInfo>,
        instance_access_handle: Option<usize>,
        client: Client,
    ) -> Result<(), String> {
        let key = WindowKey {
            track_name: track_name.clone(),
            instance_id,
        };
        if self.windows.contains_key(&key) {
            return Ok(());
        }

        // Mark that a window is open for this key (before spawning thread)
        self.windows.insert(key.clone(), WindowEntry::Generic(GenericWindowEntry { _window: std::ptr::null_mut() }));

        let closed_tx = self.closed_tx.clone();

        // Spawn a thread that will run gtk_main()
        thread::spawn(move || {
            let result = run_lv2_ui_window(
                track_name.clone(),
                instance_id,
                plugin_name,
                plugin_uri,
                controls,
                instance_access_handle,
                client,
            );

            if let Err(e) = result {
                eprintln!("LV2 UI error: {}", e);
            }

            // Notify that window was closed
            let _ = closed_tx.send(WindowKey {
                track_name,
                instance_id,
            });
        });

        Ok(())
    }

    fn open_native_editor(
        &self,
        key: &WindowKey,
        plugin_name: &str,
        plugin_uri: &str,
        controls: &[Lv2ControlPortInfo],
        instance_access_handle: Option<usize>,
        tx: &mpsc::Sender<(String, usize, u32, f32)>,
    ) -> Result<WindowEntry, String> {
        let ui_spec = resolve_preferred_ui(plugin_uri)?;
        let window = unsafe { gtk_window_new(GTK_WINDOW_TOPLEVEL) };
        if window.is_null() {
            return Err("Failed to create LV2 UI window".to_string());
        }

        let title = match CString::new(format!("LV2 UI - {plugin_name}")) {
            Ok(value) => value,
            Err(err) => {
                unsafe { gtk_widget_destroy(window) };
                return Err(err.to_string());
            }
        };
        let close_data = Box::new(WindowCloseData {
            key: key.clone(),
            tx: self.closed_tx.clone(),
        });
        let close_data_ptr = Box::into_raw(close_data).cast::<c_void>();
        unsafe {
            gtk_window_set_title(window, title.as_ptr());
            gtk_window_set_default_size(window, 780, 520);
            g_signal_connect_data(
                window,
                b"destroy\0".as_ptr() as *const c_char,
                Some(on_window_destroy),
                close_data_ptr,
                Some(on_window_close_data_destroy),
                0,
            );
        }

        let use_x11_parent = ui_spec.container_type_uri == LV2_UI_X11;
        let parent_data: *mut c_void = if use_x11_parent {
            let socket = unsafe { gtk_socket_new() };
            if socket.is_null() {
                unsafe { gtk_widget_destroy(window) };
                return Err("Failed to create GtkSocket for X11 UI embedding".to_string());
            }
            unsafe {
                gtk_container_add(window, socket);
                gtk_widget_show_all(window);
                gtk_widget_realize(socket);
            }
            let xid = unsafe { gtk_socket_get_id(socket) };
            xid as *mut c_void
        } else {
            window
        };

        let controller = Box::new(NativeUiController {
            track_name: key.track_name.clone(),
            instance_id: key.instance_id,
            port_symbol_to_index: ui_spec.port_symbol_to_index,
            tx: tx.clone(),
        });
        let controller_ptr = Box::into_raw(controller);

        let suil_host = unsafe {
            suil_host_new(
                Some(suil_write_port),
                Some(suil_port_index),
                Some(suil_subscribe_port),
                Some(suil_unsubscribe_port),
            )
        };
        if suil_host.is_null() {
            unsafe { drop(Box::from_raw(controller_ptr)); }
            unsafe { gtk_widget_destroy(window) };
            return Err("Failed to create suil host".to_string());
        }

        let container_type_uri = match CString::new(ui_spec.container_type_uri) {
            Ok(value) => value,
            Err(err) => {
                unsafe {
                    suil_host_free(suil_host);
                    drop(Box::from_raw(controller_ptr));
                    gtk_widget_destroy(window);
                }
                return Err(err.to_string());
            }
        };
        let plugin_uri = match CString::new(ui_spec.plugin_uri) {
            Ok(value) => value,
            Err(err) => {
                unsafe {
                    suil_host_free(suil_host);
                    drop(Box::from_raw(controller_ptr));
                    gtk_widget_destroy(window);
                }
                return Err(err.to_string());
            }
        };
        let ui_uri = match CString::new(ui_spec.ui_uri) {
            Ok(value) => value,
            Err(err) => {
                unsafe {
                    suil_host_free(suil_host);
                    drop(Box::from_raw(controller_ptr));
                    gtk_widget_destroy(window);
                }
                return Err(err.to_string());
            }
        };
        let ui_type_uri = match CString::new(ui_spec.ui_type_uri) {
            Ok(value) => value,
            Err(err) => {
                unsafe {
                    suil_host_free(suil_host);
                    drop(Box::from_raw(controller_ptr));
                    gtk_widget_destroy(window);
                }
                return Err(err.to_string());
            }
        };
        let ui_bundle_path = match CString::new(ui_spec.ui_bundle_path) {
            Ok(value) => value,
            Err(err) => {
                unsafe {
                    suil_host_free(suil_host);
                    drop(Box::from_raw(controller_ptr));
                    gtk_widget_destroy(window);
                }
                return Err(err.to_string());
            }
        };
        let ui_binary_path = match CString::new(ui_spec.ui_binary_path) {
            Ok(value) => value,
            Err(err) => {
                unsafe {
                    suil_host_free(suil_host);
                    drop(Box::from_raw(controller_ptr));
                    gtk_widget_destroy(window);
                }
                return Err(err.to_string());
            }
        };

        let urid_feature = match UridMapFeature::new() {
            Ok(value) => value,
            Err(err) => {
                unsafe {
                    suil_host_free(suil_host);
                    drop(Box::from_raw(controller_ptr));
                    gtk_widget_destroy(window);
                }
                return Err(err);
            }
        };
        let parent_uri = match CString::new(LV2_UI_PARENT) {
            Ok(value) => value,
            Err(err) => {
                unsafe {
                    suil_host_free(suil_host);
                    drop(Box::from_raw(controller_ptr));
                    gtk_widget_destroy(window);
                }
                return Err(err.to_string());
            }
        };
        let resize_uri = match CString::new(LV2_UI_RESIZE) {
            Ok(value) => value,
            Err(err) => {
                unsafe {
                    suil_host_free(suil_host);
                    drop(Box::from_raw(controller_ptr));
                    gtk_widget_destroy(window);
                }
                return Err(err.to_string());
            }
        };
        let instance_access_uri = match CString::new(LV2_INSTANCE_ACCESS) {
            Ok(value) => value,
            Err(err) => {
                unsafe {
                    suil_host_free(suil_host);
                    drop(Box::from_raw(controller_ptr));
                    gtk_widget_destroy(window);
                }
                return Err(err.to_string());
            }
        };
        let mut resize_feature = LV2UiResize {
            handle: window,
            ui_resize: Some(host_ui_resize),
        };
        let parent_raw = LV2FeatureRaw {
            uri: parent_uri.as_ptr(),
            data: parent_data,
        };
        let resize_raw = LV2FeatureRaw {
            uri: resize_uri.as_ptr(),
            data: (&mut resize_feature as *mut LV2UiResize).cast::<c_void>(),
        };
        let instance_access_raw = instance_access_handle.map(|handle| LV2FeatureRaw {
            uri: instance_access_uri.as_ptr(),
            data: handle as *mut c_void,
        });

        let mut feature_ptrs: Vec<*const LV2FeatureRaw> = vec![
            &urid_feature.map_feature,
            &urid_feature.map_typo_feature,
            &urid_feature.unmap_feature,
            &parent_raw,
            &resize_raw,
        ];
        if let Some(ref raw) = instance_access_raw {
            feature_ptrs.push(raw as *const LV2FeatureRaw);
        }
        feature_ptrs.push(std::ptr::null());

        let suil_instance = unsafe {
            suil_instance_new(
                suil_host,
                controller_ptr.cast::<c_void>(),
                container_type_uri.as_ptr(),
                plugin_uri.as_ptr(),
                ui_uri.as_ptr(),
                ui_type_uri.as_ptr(),
                ui_bundle_path.as_ptr(),
                ui_binary_path.as_ptr(),
                feature_ptrs.as_ptr(),
            )
        };
        if suil_instance.is_null() {
            unsafe {
                suil_host_free(suil_host);
                drop(Box::from_raw(controller_ptr));
                gtk_widget_destroy(window);
            }
            return Err("Failed to instantiate suil UI".to_string());
        }

        let widget = unsafe { suil_instance_get_widget(suil_instance) };
        if widget.is_null() {
            unsafe {
                suil_instance_free(suil_instance);
                suil_host_free(suil_host);
                drop(Box::from_raw(controller_ptr));
                gtk_widget_destroy(window);
            }
            return Err("Suil returned null UI widget".to_string());
        }

        let ui_handle = unsafe { suil_instance_get_handle(suil_instance) };
        let idle_iface_uri = CString::new(LV2_UI_IDLE_INTERFACE).map_err(|e| e.to_string())?;
        let show_iface_uri = CString::new(LV2_UI_SHOW_INTERFACE).map_err(|e| e.to_string())?;
        let hide_iface_uri = CString::new(LV2_UI_HIDE_INTERFACE).map_err(|e| e.to_string())?;
        let idle_iface_ptr = unsafe {
            suil_instance_extension_data(suil_instance, idle_iface_uri.as_ptr())
                as *const LV2UiIdleInterface
        };
        let show_iface_ptr = unsafe {
            suil_instance_extension_data(suil_instance, show_iface_uri.as_ptr())
                as *const LV2UiShowInterface
        };
        let hide_iface_ptr = unsafe {
            suil_instance_extension_data(suil_instance, hide_iface_uri.as_ptr())
                as *const LV2UiHideInterface
        };
        let mut idle_source: c_uint = 0;
        let mut idle_data_ptr: *mut UiIdleData = std::ptr::null_mut();
        if !idle_iface_ptr.is_null() {
            let idle_data = Box::new(UiIdleData {
                interface: idle_iface_ptr,
                handle: ui_handle,
            });
            idle_data_ptr = Box::into_raw(idle_data);
            unsafe {
                idle_source = g_timeout_add(16, Some(ui_idle_tick), idle_data_ptr.cast::<c_void>());
            }
        }

        let mut hide_fn: Option<extern "C" fn(*mut c_void) -> i32> = None;
        if !show_iface_ptr.is_null()
            && let Some(show) = unsafe { (*show_iface_ptr).show }
        {
            let _ = show(ui_handle);
        }
        if !hide_iface_ptr.is_null() {
            hide_fn = unsafe { (*hide_iface_ptr).hide };
        }

        for port in controls {
            let value = port.value;
            unsafe {
                suil_instance_port_event(
                    suil_instance,
                    port.index,
                    std::mem::size_of::<f32>() as u32,
                    0,
                    (&value as *const f32).cast::<c_void>(),
                );
            }
        }

        if !use_x11_parent {
            unsafe {
                gtk_container_add(window, widget);
                gtk_widget_show_all(window);
            }
        }

        Ok(WindowEntry::Native(NativeWindowEntry {
            _window: window,
            _widget: widget,
            suil_instance,
            suil_host,
            controller: controller_ptr,
            idle_source,
            idle_data_ptr,
            hide_fn,
            ui_handle,
            _urid_feature: urid_feature,
        }))
    }

    fn open_generic_editor(
        &self,
        key: &WindowKey,
        plugin_name: &str,
        controls: Vec<Lv2ControlPortInfo>,
        tx: &mpsc::Sender<(String, usize, u32, f32)>,
    ) -> Result<WindowEntry, String> {
        let window = unsafe { gtk_window_new(GTK_WINDOW_TOPLEVEL) };
        if window.is_null() {
            return Err("Failed to create LV2 UI window".to_string());
        }

        let title = CString::new(format!("LV2 UI - {plugin_name}")).map_err(|e| e.to_string())?;
        let close_data = Box::new(WindowCloseData {
            key: key.clone(),
            tx: self.closed_tx.clone(),
        });
        let close_data_ptr = Box::into_raw(close_data).cast::<c_void>();

        unsafe {
            gtk_window_set_title(window, title.as_ptr());
            gtk_window_set_default_size(window, 720, 480);
            g_signal_connect_data(
                window,
                b"destroy\0".as_ptr() as *const c_char,
                Some(on_window_destroy),
                close_data_ptr,
                Some(on_window_close_data_destroy),
                0,
            );
        }

        let root = unsafe { gtk_vbox_new(0, 8) };
        if root.is_null() {
            return Err("Failed to create LV2 UI root".to_string());
        }

        for port in controls {
            let row = unsafe { gtk_hbox_new(0, 8) };
            if row.is_null() {
                continue;
            }

            let label_txt = CString::new(port.name).map_err(|e| e.to_string())?;
            let label = unsafe { gtk_label_new(label_txt.as_ptr()) };
            let step = ((port.max - port.min).abs() / 200.0).max(0.0001) as f64;
            let slider = unsafe { gtk_hscale_new_with_range(port.min as f64, port.max as f64, step) };
            if slider.is_null() {
                continue;
            }

            unsafe {
                gtk_widget_set_size_request(slider, 420, -1);
                gtk_range_set_value(slider, port.value as f64);
            }

            let slider_data = Box::new(SliderCallbackData {
                track_name: key.track_name.clone(),
                instance_id: key.instance_id,
                port_index: port.index,
                tx: tx.clone(),
            });
            let slider_data_ptr = Box::into_raw(slider_data).cast::<c_void>();
            unsafe {
                g_signal_connect_data(
                    slider,
                    b"value-changed\0".as_ptr() as *const c_char,
                    Some(on_slider_changed),
                    slider_data_ptr,
                    Some(on_slider_data_destroy),
                    0,
                );
                gtk_box_pack_start(row, label, 0, 0, 0);
                gtk_box_pack_start(row, slider, 1, 1, 0);
                gtk_box_pack_start(root, row, 0, 0, 0);
            }
        }

        unsafe {
            gtk_container_add(window, root);
            gtk_widget_show_all(window);
        }

        Ok(WindowEntry::Generic(GenericWindowEntry { _window: window }))
    }

    fn ensure_gtk_initialized(&mut self) -> Result<(), String> {
        if self.gtk_initialized {
            return Ok(());
        }

        let mut argc = 0;
        let mut argv: *mut *mut c_char = std::ptr::null_mut();
        if unsafe { gtk_init_check(&mut argc, &mut argv) } == 0 {
            return Err("gtk_init_check failed; cannot open LV2 UI".to_string());
        }
        self.gtk_initialized = true;
        Ok(())
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn run_lv2_ui_window(
    track_name: String,
    instance_id: usize,
    plugin_name: String,
    plugin_uri: String,
    controls: Vec<Lv2ControlPortInfo>,
    instance_access_handle: Option<usize>,
    client: Client,
) -> Result<(), String> {
    // Initialize GTK
    let mut argc = 0;
    let mut argv: *mut *mut c_char = std::ptr::null_mut();
    if unsafe { gtk_init_check(&mut argc, &mut argv) } == 0 {
        return Err("Failed to initialize GTK".to_string());
    }

    let tx = spawn_control_sender(client);

    // Try to open native UI first, fall back to generic
    let ui_spec = resolve_preferred_ui(&plugin_uri);

    if let Ok(spec) = ui_spec {
        run_native_ui_with_gtk_main(
            track_name,
            instance_id,
            plugin_name,
            spec,
            controls,
            instance_access_handle,
            &tx,
        )
    } else {
        run_generic_ui_with_gtk_main(plugin_name, controls, &tx)
    }
}

fn run_native_ui_with_gtk_main(
    track_name: String,
    instance_id: usize,
    plugin_name: String,
    ui_spec: NativeUiSpec,
    controls: Vec<Lv2ControlPortInfo>,
    instance_access_handle: Option<usize>,
    tx: &mpsc::Sender<(String, usize, u32, f32)>,
) -> Result<(), String> {
    let window = unsafe { gtk_window_new(GTK_WINDOW_TOPLEVEL) };
    if window.is_null() {
        return Err("Failed to create LV2 UI window".to_string());
    }

    let title = CString::new(format!("LV2 UI - {}", plugin_name)).map_err(|e| e.to_string())?;
    unsafe {
        gtk_window_set_title(window, title.as_ptr());
        gtk_window_set_default_size(window, 780, 520);
        g_signal_connect_data(
            window,
            b"destroy\0".as_ptr() as *const c_char,
            Some(on_gtk_destroy),
            std::ptr::null_mut(),
            None,
            0,
        );
    }

    let use_x11_parent = ui_spec.container_type_uri == LV2_UI_X11;
    let parent_data: *mut c_void = if use_x11_parent {
        let socket = unsafe { gtk_socket_new() };
        if socket.is_null() {
            unsafe { gtk_widget_destroy(window) };
            return Err("Failed to create GtkSocket for X11 UI embedding".to_string());
        }
        unsafe {
            gtk_container_add(window, socket);
            gtk_widget_show_all(window);
            gtk_widget_realize(socket);
        }
        let xid = unsafe { gtk_socket_get_id(socket) };
        xid as *mut c_void
    } else {
        window
    };

    let controller = Box::new(NativeUiController {
        track_name,
        instance_id,
        port_symbol_to_index: ui_spec.port_symbol_to_index,
        tx: tx.clone(),
    });
    let controller_ptr = Box::into_raw(controller);

    let suil_host = unsafe {
        suil_host_new(
            Some(suil_write_port),
            Some(suil_port_index),
            Some(suil_subscribe_port),
            Some(suil_unsubscribe_port),
        )
    };
    if suil_host.is_null() {
        unsafe {
            drop(Box::from_raw(controller_ptr));
            gtk_widget_destroy(window);
        }
        return Err("Failed to create suil host".to_string());
    }

    let container_type_uri = CString::new(ui_spec.container_type_uri).map_err(|e| e.to_string())?;
    let plugin_uri = CString::new(ui_spec.plugin_uri).map_err(|e| e.to_string())?;
    let ui_uri = CString::new(ui_spec.ui_uri).map_err(|e| e.to_string())?;
    let ui_type_uri = CString::new(ui_spec.ui_type_uri).map_err(|e| e.to_string())?;
    let ui_bundle_path = CString::new(ui_spec.ui_bundle_path).map_err(|e| e.to_string())?;
    let ui_binary_path = CString::new(ui_spec.ui_binary_path).map_err(|e| e.to_string())?;

    let urid_feature = UridMapFeature::new().map_err(|e| {
        unsafe {
            suil_host_free(suil_host);
            drop(Box::from_raw(controller_ptr));
            gtk_widget_destroy(window);
        }
        e
    })?;

    let parent_uri = CString::new(LV2_UI_PARENT).map_err(|e| e.to_string())?;
    let resize_uri = CString::new(LV2_UI_RESIZE).map_err(|e| e.to_string())?;
    let instance_access_uri = CString::new(LV2_INSTANCE_ACCESS).map_err(|e| e.to_string())?;

    let mut resize_feature = LV2UiResize {
        handle: window,
        ui_resize: Some(host_ui_resize),
    };
    let parent_raw = LV2FeatureRaw {
        uri: parent_uri.as_ptr(),
        data: parent_data,
    };
    let resize_raw = LV2FeatureRaw {
        uri: resize_uri.as_ptr(),
        data: (&mut resize_feature as *mut LV2UiResize).cast::<c_void>(),
    };
    let instance_access_raw = instance_access_handle.map(|handle| LV2FeatureRaw {
        uri: instance_access_uri.as_ptr(),
        data: handle as *mut c_void,
    });

    let mut feature_ptrs: Vec<*const LV2FeatureRaw> = vec![
        &urid_feature.map_feature,
        &urid_feature.map_typo_feature,
        &urid_feature.unmap_feature,
        &parent_raw,
        &resize_raw,
    ];
    if let Some(ref raw) = instance_access_raw {
        feature_ptrs.push(raw as *const LV2FeatureRaw);
    }
    feature_ptrs.push(std::ptr::null());

    let suil_instance = unsafe {
        suil_instance_new(
            suil_host,
            controller_ptr.cast::<c_void>(),
            container_type_uri.as_ptr(),
            plugin_uri.as_ptr(),
            ui_uri.as_ptr(),
            ui_type_uri.as_ptr(),
            ui_bundle_path.as_ptr(),
            ui_binary_path.as_ptr(),
            feature_ptrs.as_ptr(),
        )
    };
    if suil_instance.is_null() {
        unsafe {
            suil_host_free(suil_host);
            drop(Box::from_raw(controller_ptr));
            gtk_widget_destroy(window);
        }
        return Err("Failed to instantiate suil UI".to_string());
    }

    let widget = unsafe { suil_instance_get_widget(suil_instance) };
    if widget.is_null() {
        unsafe {
            suil_instance_free(suil_instance);
            suil_host_free(suil_host);
            drop(Box::from_raw(controller_ptr));
            gtk_widget_destroy(window);
        }
        return Err("Suil returned null UI widget".to_string());
    }

    let ui_handle = unsafe { suil_instance_get_handle(suil_instance) };
    let idle_iface_uri = CString::new(LV2_UI_IDLE_INTERFACE).map_err(|e| e.to_string())?;
    let show_iface_uri = CString::new(LV2_UI_SHOW_INTERFACE).map_err(|e| e.to_string())?;
    let hide_iface_uri = CString::new(LV2_UI_HIDE_INTERFACE).map_err(|e| e.to_string())?;

    let idle_iface_ptr = unsafe {
        suil_instance_extension_data(suil_instance, idle_iface_uri.as_ptr())
            as *const LV2UiIdleInterface
    };
    let show_iface_ptr = unsafe {
        suil_instance_extension_data(suil_instance, show_iface_uri.as_ptr())
            as *const LV2UiShowInterface
    };
    let hide_iface_ptr = unsafe {
        suil_instance_extension_data(suil_instance, hide_iface_uri.as_ptr())
            as *const LV2UiHideInterface
    };

    let mut idle_source: c_uint = 0;
    let mut idle_data_ptr: *mut UiIdleData = std::ptr::null_mut();
    if !idle_iface_ptr.is_null() {
        let idle_data = Box::new(UiIdleData {
            interface: idle_iface_ptr,
            handle: ui_handle,
        });
        idle_data_ptr = Box::into_raw(idle_data);
        unsafe {
            idle_source = g_timeout_add(16, Some(ui_idle_tick), idle_data_ptr.cast::<c_void>());
        }
    }

    if !show_iface_ptr.is_null()
        && let Some(show) = unsafe { (*show_iface_ptr).show }
    {
        let _ = show(ui_handle);
    }

    // Set initial port values
    for port in controls {
        let value = port.value;
        unsafe {
            suil_instance_port_event(
                suil_instance,
                port.index,
                std::mem::size_of::<f32>() as u32,
                0,
                (&value as *const f32).cast::<c_void>(),
            );
        }
    }

    unsafe {
        if !use_x11_parent {
            gtk_container_add(window, widget);
            gtk_widget_show_all(window);
        }
        // This blocks until the window is closed
        gtk_main();
    }

    // Cleanup after gtk_main returns
    if !hide_iface_ptr.is_null()
        && let Some(hide) = unsafe { (*hide_iface_ptr).hide }
    {
        let _ = hide(ui_handle);
    }
    if idle_source != 0 {
        unsafe {
            g_source_remove(idle_source);
        }
    }
    if !idle_data_ptr.is_null() {
        unsafe {
            drop(Box::from_raw(idle_data_ptr));
        }
    }
    unsafe {
        suil_instance_free(suil_instance);
        suil_host_free(suil_host);
        drop(Box::from_raw(controller_ptr));
    }

    let _ = &urid_feature; // Keep feature alive

    Ok(())
}

fn run_generic_ui_with_gtk_main(
    plugin_name: String,
    controls: Vec<Lv2ControlPortInfo>,
    tx: &mpsc::Sender<(String, usize, u32, f32)>,
) -> Result<(), String> {
    let window = unsafe { gtk_window_new(GTK_WINDOW_TOPLEVEL) };
    if window.is_null() {
        return Err("Failed to create generic parameter UI window".to_string());
    }

    let title =
        CString::new(format!("LV2 Generic UI - {}", plugin_name)).map_err(|e| e.to_string())?;
    unsafe {
        gtk_window_set_title(window, title.as_ptr());
        gtk_window_set_default_size(window, 720, 480);
        g_signal_connect_data(
            window,
            b"destroy\0".as_ptr() as *const c_char,
            Some(on_gtk_destroy),
            std::ptr::null_mut(),
            None,
            0,
        );
    }

    let root = unsafe { gtk_vbox_new(0, 8) };
    if root.is_null() {
        unsafe { gtk_widget_destroy(window) };
        return Err("Failed to create generic parameter UI root".to_string());
    }

    let mut slider_data = vec![];
    for port in controls {
        let row = unsafe { gtk_hbox_new(0, 8) };
        if row.is_null() {
            continue;
        }
        let label_txt = CString::new(port.name).map_err(|e| e.to_string())?;
        let label = unsafe { gtk_label_new(label_txt.as_ptr()) };
        let step = ((port.max - port.min).abs() / 200.0).max(0.0001) as f64;
        let slider = unsafe { gtk_hscale_new_with_range(port.min as f64, port.max as f64, step) };
        if slider.is_null() {
            continue;
        }
        unsafe {
            gtk_widget_set_size_request(slider, 420, -1);
            gtk_range_set_value(slider, port.value as f64);
        }

        let data = Box::new(SliderCallbackData {
            track_name: "".to_string(), // Not used for now
            instance_id: 0,
            port_index: port.index,
            tx: tx.clone(),
        });
        let data_ptr = Box::into_raw(data);
        slider_data.push(data_ptr);
        unsafe {
            g_signal_connect_data(
                slider,
                b"value-changed\0".as_ptr() as *const c_char,
                Some(on_generic_slider_changed),
                data_ptr.cast::<c_void>(),
                None,
                0,
            );
            gtk_box_pack_start(row, label, 0, 0, 4);
            gtk_box_pack_start(row, slider, 1, 1, 4);
            gtk_box_pack_start(root, row, 0, 0, 2);
        }
    }

    unsafe {
        gtk_container_add(window, root);
        gtk_widget_show_all(window);
        // This blocks until the window is closed
        gtk_main();
    }

    // Cleanup after gtk_main returns
    for data_ptr in slider_data {
        unsafe {
            drop(Box::from_raw(data_ptr));
        }
    }

    Ok(())
}

fn spawn_control_sender(client: Client) -> mpsc::Sender<(String, usize, u32, f32)> {
    let (tx, rx) = mpsc::channel::<(String, usize, u32, f32)>();
    thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(err) => {
                eprintln!("Failed to create LV2 UI runtime: {err}");
                return;
            }
        };
        for (track_name, instance_id, index, value) in rx {
            let _ = runtime.block_on(client.send(EngineMessage::Request(Action::TrackSetLv2ControlValue {
                track_name,
                instance_id,
                index,
                value,
            })));
        }
    });
    tx
}

#[cfg(all(unix, not(target_os = "macos")))]
fn port_symbol_map(plugin: &Plugin) -> HashMap<String, u32> {
    let mut map = HashMap::new();
    for port in plugin.iter_ports() {
        if let Some(symbol) = port.symbol().and_then(|n| n.as_str().map(str::to_string)) {
            map.insert(symbol, port.index() as u32);
        }
    }
    map
}

#[cfg(all(unix, not(target_os = "macos")))]
fn resolve_preferred_ui(plugin_uri: &str) -> Result<NativeUiSpec, String> {
    let world = World::new();
    world.load_all();

    let uri_node = world.new_uri(plugin_uri);
    let plugin = world
        .plugins()
        .plugin(&uri_node)
        .ok_or_else(|| format!("Plugin not found for URI: {plugin_uri}"))?;
    let port_symbol_to_index = port_symbol_map(&plugin);

    let uis = plugin
        .uis()
        .ok_or_else(|| format!("Plugin has no UI: {plugin_uri}"))?;

    let gtk3_uri = world.new_uri(LV2_UI_GTK3);
    let gtk_uri = world.new_uri(LV2_UI_GTK);
    let x11_uri = world.new_uri(LV2_UI_X11);
    let qt4_uri = world.new_uri(LV2_UI_QT4);
    let qt5_uri = world.new_uri(LV2_UI_QT5);
    let qt6_uri = world.new_uri(LV2_UI_QT6);

    let ui_classes = [
        (&gtk3_uri, LV2_UI_GTK3),
        (&gtk_uri, LV2_UI_GTK),
        (&x11_uri, LV2_UI_X11),
        (&qt6_uri, LV2_UI_QT6),
        (&qt5_uri, LV2_UI_QT5),
        (&qt4_uri, LV2_UI_QT4),
    ];

    let mut best: Option<(usize, usize, u32, NativeUiSpec)> = None;
    for ui in uis.iter() {
        let ui_uri = ui
            .uri()
            .as_uri()
            .ok_or_else(|| "UI URI is invalid".to_string())?
            .to_string();
        let bundle_uri = ui
            .bundle_uri()
            .ok_or_else(|| "UI bundle URI missing".to_string())?;
        let binary_uri = ui
            .binary_uri()
            .ok_or_else(|| "UI binary URI missing".to_string())?;
        let (_, ui_bundle_path) = bundle_uri
            .path()
            .ok_or_else(|| "Failed to resolve UI bundle path".to_string())?;
        let (_, ui_binary_path) = binary_uri
            .path()
            .ok_or_else(|| "Failed to resolve UI binary path".to_string())?;

        for (class_rank, (class_node, class_uri)) in ui_classes.iter().enumerate() {
            if !ui.is_a(class_node) {
                continue;
            }
            let class_c = CString::new(*class_uri).map_err(|e| e.to_string())?;
            let host_containers: &[&str] = if *class_uri == LV2_UI_X11 {
                &[LV2_UI_X11]
            } else {
                &[LV2_UI_GTK, LV2_UI_X11]
            };
            for container_uri in host_containers.iter().copied() {
                let host_type = CString::new(container_uri).map_err(|e| e.to_string())?;
                let quality = unsafe { suil_ui_supported(host_type.as_ptr(), class_c.as_ptr()) };
                if quality == 0 {
                    continue;
                }

                let container_rank = usize::from(container_uri != LV2_UI_GTK);
                let spec = NativeUiSpec {
                    plugin_uri: plugin_uri.to_string(),
                    ui_uri: ui_uri.clone(),
                    container_type_uri: container_uri.to_string(),
                    ui_type_uri: (*class_uri).to_string(),
                    ui_bundle_path: ui_bundle_path.clone(),
                    ui_binary_path: ui_binary_path.clone(),
                    port_symbol_to_index: port_symbol_to_index.clone(),
                };
                let is_better = match &best {
                    None => true,
                    Some((best_class_rank, best_container_rank, best_quality, _)) => {
                        class_rank < *best_class_rank
                            || (class_rank == *best_class_rank
                                && (container_rank < *best_container_rank
                                    || (container_rank == *best_container_rank
                                        && quality > *best_quality)))
                    }
                };
                if is_better {
                    best = Some((class_rank, container_rank, quality, spec));
                }
            }
        }
    }

    best.map(|(_, _, _, spec)| spec)
        .ok_or_else(|| format!("No supported native UI found for plugin: {plugin_uri}"))
}

#[cfg(all(unix, not(target_os = "macos")))]
#[link(name = "suil-0")]
unsafe extern "C" {
    fn suil_host_new(
        write_func: Option<SuilPortWriteFunc>,
        index_func: Option<SuilPortIndexFunc>,
        subscribe_func: Option<
            extern "C" fn(SuilController, u32, u32, *const *const LV2FeatureRaw) -> u32,
        >,
        unsubscribe_func: Option<
            extern "C" fn(SuilController, u32, u32, *const *const LV2FeatureRaw) -> u32,
        >,
    ) -> *mut SuilHost;
    fn suil_host_free(host: *mut SuilHost);
    fn suil_ui_supported(host_type_uri: *const c_char, ui_type_uri: *const c_char) -> u32;
    fn suil_instance_new(
        host: *mut SuilHost,
        controller: SuilController,
        container_type_uri: *const c_char,
        plugin_uri: *const c_char,
        ui_uri: *const c_char,
        ui_type_uri: *const c_char,
        ui_bundle_path: *const c_char,
        ui_binary_path: *const c_char,
        features: *const *const LV2FeatureRaw,
    ) -> *mut SuilInstance;
    fn suil_instance_free(instance: *mut SuilInstance);
    fn suil_instance_get_widget(instance: *mut SuilInstance) -> *mut c_void;
    fn suil_instance_get_handle(instance: *mut SuilInstance) -> *mut c_void;
    fn suil_instance_extension_data(
        instance: *mut SuilInstance,
        uri: *const c_char,
    ) -> *const c_void;
    fn suil_instance_port_event(
        instance: *mut SuilInstance,
        port_index: u32,
        buffer_size: u32,
        protocol: u32,
        buffer: *const c_void,
    );
}

#[cfg(all(unix, not(target_os = "macos")))]
#[link(name = "gtk-x11-2.0")]
unsafe extern "C" {
    fn gtk_init_check(argc: *mut i32, argv: *mut *mut *mut c_char) -> i32;
    fn gtk_window_new(window_type: i32) -> *mut c_void;
    fn gtk_window_set_title(window: *mut c_void, title: *const c_char);
    fn gtk_window_set_default_size(window: *mut c_void, width: i32, height: i32);
    fn gtk_window_resize(window: *mut c_void, width: i32, height: i32);
    fn gtk_container_add(container: *mut c_void, widget: *mut c_void);
    fn gtk_vbox_new(homogeneous: i32, spacing: i32) -> *mut c_void;
    fn gtk_hbox_new(homogeneous: i32, spacing: i32) -> *mut c_void;
    fn gtk_label_new(text: *const c_char) -> *mut c_void;
    fn gtk_hscale_new_with_range(min: f64, max: f64, step: f64) -> *mut c_void;
    fn gtk_range_set_value(range: *mut c_void, value: f64);
    fn gtk_range_get_value(range: *mut c_void) -> f64;
    fn gtk_widget_set_size_request(widget: *mut c_void, width: i32, height: i32);
    fn gtk_box_pack_start(
        boxed: *mut c_void,
        child: *mut c_void,
        expand: i32,
        fill: i32,
        padding: u32,
    );
    fn gtk_widget_destroy(widget: *mut c_void);
    fn gtk_widget_show_all(widget: *mut c_void);
    fn gtk_widget_realize(widget: *mut c_void);
    fn gtk_socket_new() -> *mut c_void;
    fn gtk_socket_get_id(socket: *mut c_void) -> c_ulong;
    fn gtk_events_pending() -> i32;
    fn gtk_main_iteration_do(blocking: i32);
    fn gtk_main();
    fn gtk_main_quit();
}

#[cfg(all(unix, not(target_os = "macos")))]
#[link(name = "gobject-2.0")]
unsafe extern "C" {
    fn g_signal_connect_data(
        instance: *mut c_void,
        detailed_signal: *const c_char,
        c_handler: Option<unsafe extern "C" fn(*mut c_void, *mut c_void)>,
        data: *mut c_void,
        destroy_data: Option<unsafe extern "C" fn(*mut c_void, *mut c_void)>,
        connect_flags: u32,
    ) -> u64;
}

#[cfg(all(unix, not(target_os = "macos")))]
#[link(name = "glib-2.0")]
unsafe extern "C" {
    fn g_timeout_add(
        interval: c_uint,
        function: Option<extern "C" fn(*mut c_void) -> i32>,
        data: *mut c_void,
    ) -> c_uint;
    fn g_source_remove(tag: c_uint) -> i32;
}
