#[cfg(unix)]
use crate::lv2::{Lv2PluginState, Lv2StatePortValue, Lv2StateProperty};
use maolan_plugin_protocol::events::EventPair;
use maolan_plugin_protocol::protocol::*;
use maolan_plugin_protocol::ringbuf::RingBuffer;
use maolan_plugin_protocol::shm::ShmMapping;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::time::Duration;

#[cfg(all(unix, not(target_os = "macos")))]
use maolan_lv2::raw::{
    LV2Feature, LV2UIControllerRaw, LV2UIDescriptorRaw, LV2UIHandle, LV2UIIdleInterface,
    LV2UIShowInterface, LV2UIWidget,
};

#[cfg(all(unix, not(target_os = "macos")))]
const LV2_UI_X11_UI: &str = "http://lv2plug.in/ns/extensions/ui#X11UI";
#[cfg(all(unix, not(target_os = "macos")))]
const LV2_UI_PARENT: &str = "http://lv2plug.in/ns/extensions/ui#parent";
#[cfg(all(unix, not(target_os = "macos")))]
const LV2_UI_IDLE_INTERFACE: &str = "http://lv2plug.in/ns/extensions/ui#idleInterface";
#[cfg(all(unix, not(target_os = "macos")))]
const LV2_UI_SHOW_INTERFACE: &str = "http://lv2plug.in/ns/extensions/ui#showInterface";

const SHM_LATENCY_SAMPLES_OFFSET: usize = 84;

unsafe fn latency_samples_atomic(ptr: *mut u8) -> &'static std::sync::atomic::AtomicU32 {
    unsafe { &*(ptr.add(SHM_LATENCY_SAMPLES_OFFSET) as *const std::sync::atomic::AtomicU32) }
}

#[cfg(windows)]
struct ComInitGuard {
    initialized: bool,
}

#[cfg(windows)]
impl ComInitGuard {
    fn new() -> Self {
        use windows_sys::Win32::Foundation::S_OK;
        use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        let hr = unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) };
        Self {
            initialized: hr == S_OK,
        }
    }
}

#[cfg(windows)]
impl Drop for ComInitGuard {
    fn drop(&mut self) {
        if self.initialized {
            unsafe { windows_sys::Win32::System::Com::CoUninitialize() };
        }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod x11_ffi {
    use std::os::raw::{c_char, c_int, c_long, c_uint, c_ulong};
    pub type Atom = c_ulong;
    pub type Display = std::ffi::c_void;
    pub type Window = c_ulong;

    pub const CLIENT_MESSAGE: c_int = 33;
    pub const DESTROY_NOTIFY: c_int = 17;
    pub const STRUCTURE_NOTIFY_MASK: c_long = 1 << 17;

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub union ClientMessageData {
        pub b: [c_char; 20],
        pub s: [i16; 10],
        pub l: [c_long; 5],
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct XClientMessageEvent {
        pub type_: c_int,
        pub serial: c_ulong,
        pub send_event: c_int,
        pub display: *mut Display,
        pub window: Window,
        pub message_type: Atom,
        pub format: c_int,
        pub data: ClientMessageData,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct XDestroyWindowEvent {
        pub type_: c_int,
        pub serial: c_ulong,
        pub send_event: c_int,
        pub display: *mut Display,
        pub event: Window,
        pub window: Window,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub union XEvent {
        pub type_: c_int,
        pub client_message: XClientMessageEvent,
        pub destroy_window: XDestroyWindowEvent,
        pub pad: [c_long; 24],
    }

    #[link(name = "X11")]
    unsafe extern "C" {
        pub fn XOpenDisplay(display_name: *const c_char) -> *mut Display;
        pub fn XCloseDisplay(display: *mut Display) -> c_int;
        pub fn XDefaultScreen(display: *mut Display) -> c_int;
        pub fn XRootWindow(display: *mut Display, screen: c_int) -> Window;
        pub fn XBlackPixel(display: *mut Display, screen: c_int) -> c_ulong;
        pub fn XWhitePixel(display: *mut Display, screen: c_int) -> c_ulong;
        pub fn XCreateSimpleWindow(
            display: *mut Display,
            parent: Window,
            x: c_int,
            y: c_int,
            width: c_uint,
            height: c_uint,
            border_width: c_uint,
            border: c_ulong,
            background: c_ulong,
        ) -> Window;
        pub fn XStoreName(display: *mut Display, w: Window, name: *const c_char) -> c_int;
        pub fn XMapWindow(display: *mut Display, w: Window) -> c_int;
        pub fn XUnmapWindow(display: *mut Display, w: Window) -> c_int;
        pub fn XDestroyWindow(display: *mut Display, w: Window) -> c_int;
        pub fn XSelectInput(display: *mut Display, w: Window, event_mask: c_long) -> c_int;
        pub fn XInternAtom(
            display: *mut Display,
            atom_name: *const c_char,
            only_if_exists: c_int,
        ) -> Atom;
        pub fn XSetWMProtocols(
            display: *mut Display,
            w: Window,
            protocols: *mut Atom,
            count: c_int,
        ) -> c_int;
        pub fn XPending(display: *mut Display) -> c_int;
        pub fn XNextEvent(display: *mut Display, event_return: *mut XEvent) -> c_int;
        pub fn XReparentWindow(
            display: *mut Display,
            w: Window,
            parent: Window,
            x: c_int,
            y: c_int,
        ) -> c_int;
        pub fn XResizeWindow(
            display: *mut Display,
            w: Window,
            width: c_uint,
            height: c_uint,
        ) -> c_int;
        pub fn XFlush(display: *mut Display) -> c_int;
        pub fn XSync(display: *mut Display, discard: c_int) -> c_int;
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
struct Vst3GuiWindow {
    display: *mut x11_ffi::Display,
    window: x11_ffi::Window,
}

#[cfg(all(unix, not(target_os = "macos")))]
unsafe impl Send for Vst3GuiWindow {}

#[cfg(all(unix, not(target_os = "macos")))]
impl Drop for Vst3GuiWindow {
    fn drop(&mut self) {
        unsafe {
            x11_ffi::XDestroyWindow(self.display, self.window);
            x11_ffi::XFlush(self.display);
            x11_ffi::XCloseDisplay(self.display);
        }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn create_vst3_gui(
    processor: &crate::vst3::Vst3Processor,
    plugin_path: &str,
    ptr: *mut u8,
) -> Result<Vst3GuiWindow, String> {
    use std::os::raw::c_uint;

    let header = unsafe { header_ref(ptr) };
    let gui_mode = header.gui_mode();
    let requested_api = match gui_mode {
        GuiMode::Embedded => header.gui_parent_api(),
        GuiMode::Floating => floating_gui_parent_api(),
    };
    if gui_mode == GuiMode::Embedded && requested_api == GuiParentApi::Wayland {
        return Err(
            "VST3 GUI: embedded Wayland parent requested, but VST3 on Unix supports X11EmbedWindowID"
                .to_string(),
        );
    }
    if gui_mode == GuiMode::Floating && requested_api == GuiParentApi::Wayland {
        tracing::warn!("VST3 GUI: no standard Wayland platform type; falling back to X11");
    }

    let display = unsafe { x11_ffi::XOpenDisplay(std::ptr::null()) };
    if display.is_null() {
        return Err("VST3 GUI: failed to open X11 display".to_string());
    }

    let screen = unsafe { x11_ffi::XDefaultScreen(display) };
    let root = unsafe { x11_ffi::XRootWindow(display, screen) };
    let black = unsafe { x11_ffi::XBlackPixel(display, screen) };
    let white = unsafe { x11_ffi::XWhitePixel(display, screen) };

    let parent_window = if gui_mode == GuiMode::Embedded
        && requested_api == GuiParentApi::X11
        && header.parent_window_usize() != 0
    {
        header.parent_window_usize() as x11_ffi::Window
    } else {
        root
    };

    let window = unsafe {
        x11_ffi::XCreateSimpleWindow(display, parent_window, 0, 0, 800, 600, 1, black, white)
    };
    if window == 0 {
        unsafe {
            x11_ffi::XCloseDisplay(display);
        }
        return Err("VST3 GUI: failed to create X11 container window".to_string());
    }

    let title = std::path::Path::new(plugin_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Plugin");
    if let Ok(cstr) = std::ffi::CString::new(title) {
        unsafe {
            x11_ffi::XStoreName(display, window, cstr.as_ptr());
        }
    }

    processor.gui_create("X11EmbedWindowID").map_err(|e| {
        unsafe {
            x11_ffi::XDestroyWindow(display, window);
            x11_ffi::XCloseDisplay(display);
        }
        format!("VST3 GUI: gui_create failed: {e}")
    })?;

    processor
        .gui_set_parent(window as usize, "X11EmbedWindowID")
        .map_err(|e| {
            unsafe {
                x11_ffi::XDestroyWindow(display, window);
                x11_ffi::XCloseDisplay(display);
            }
            format!("VST3 GUI: gui_set_parent failed: {e}")
        })?;

    if let Ok((w, h)) = processor.gui_get_size()
        && w > 0
        && h > 0
    {
        unsafe {
            x11_ffi::XResizeWindow(display, window, w as c_uint, h as c_uint);
        }
    }

    unsafe {
        x11_ffi::XMapWindow(display, window);
        x11_ffi::XFlush(display);
    }

    Ok(Vst3GuiWindow { display, window })
}

#[cfg(windows)]
mod win32_gui {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static CLASS_ATOM: AtomicUsize = AtomicUsize::new(0);

    unsafe extern "system" fn wnd_proc(
        hwnd: windows_sys::Win32::Foundation::HWND,
        msg: u32,
        wparam: windows_sys::Win32::Foundation::WPARAM,
        lparam: windows_sys::Win32::Foundation::LPARAM,
    ) -> windows_sys::Win32::Foundation::LRESULT {
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::DefWindowProcW(hwnd, msg, wparam, lparam)
        }
    }

    pub fn ensure_class_registered() -> u16 {
        let atom = CLASS_ATOM.load(Ordering::Acquire);
        if atom != 0 {
            return atom as u16;
        }

        let class_name: Vec<u16> = "MaolanVst3Container\0".encode_utf16().collect();
        let wndclass = windows_sys::Win32::UI::WindowsAndMessaging::WNDCLASSEXW {
            cbSize: std::mem::size_of::<windows_sys::Win32::UI::WindowsAndMessaging::WNDCLASSEXW>()
                as u32,
            style: 0,
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: unsafe {
                windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null())
            } as *mut _,
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: (5 + 1) as *mut _,
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };

        let atom =
            unsafe { windows_sys::Win32::UI::WindowsAndMessaging::RegisterClassExW(&wndclass) };
        if atom == 0 {
            return 0;
        }
        CLASS_ATOM.store(atom as usize, Ordering::Release);
        atom as u16
    }
}

#[cfg(windows)]
struct Vst3GuiWindow {
    hwnd: windows_sys::Win32::Foundation::HWND,
}

#[cfg(windows)]
impl Drop for Vst3GuiWindow {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow(self.hwnd);
        }
    }
}

#[cfg(windows)]
fn create_vst3_gui(
    processor: &crate::vst3::Vst3Processor,
    plugin_path: &str,
    ptr: *mut u8,
) -> Result<Vst3GuiWindow, String> {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    let atom = win32_gui::ensure_class_registered();
    if atom == 0 {
        return Err("VST3 GUI: failed to register window class".to_string());
    }

    let parent_hwnd: HWND = {
        let header = unsafe { header_ref(ptr) };
        if header.gui_mode() == GuiMode::Floating {
            std::ptr::null_mut()
        } else {
            let parent = header.parent_window_usize();
            if parent != 0 {
                parent as HWND
            } else {
                std::ptr::null_mut()
            }
        }
    };

    let title: Vec<u16> = std::path::Path::new(plugin_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Plugin")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    let hwnd = unsafe {
        CreateWindowExW(
            0,
            atom as *const u16,
            title.as_ptr(),
            if parent_hwnd.is_null() {
                WS_OVERLAPPEDWINDOW
            } else {
                WS_CHILD | WS_VISIBLE
            },
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            800,
            600,
            parent_hwnd,
            std::ptr::null_mut(),
            windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        )
    };

    if hwnd.is_null() {
        return Err("VST3 GUI: failed to create window".to_string());
    }

    processor.gui_create("HWND").map_err(|e| {
        unsafe { DestroyWindow(hwnd) };
        format!("VST3 GUI: gui_create failed: {e}")
    })?;

    processor
        .gui_set_parent(hwnd as usize, "HWND")
        .map_err(|e| {
            unsafe { DestroyWindow(hwnd) };
            format!("VST3 GUI: gui_set_parent failed: {e}")
        })?;

    if let Ok((w, h)) = processor.gui_get_size()
        && w > 0
        && h > 0
    {
        unsafe {
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                0,
                0,
                w,
                h,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }

    unsafe {
        ShowWindow(hwnd, SW_SHOW);
    }

    Ok(Vst3GuiWindow { hwnd })
}

fn apply_vst3_param_ring(processor: &crate::vst3::Vst3Processor, ptr: *mut u8) {
    let ring = unsafe {
        let buf = param_ring_ptr(ptr);
        let (w, r) = param_indices(ptr);
        RingBuffer::new(buf, w, r, RING_CAPACITY)
    };
    while let Some(ev) = ring.pop() {
        if let Err(_e) =
            processor.set_parameter_value_at(ev.param_index, ev.value, ev.sample_offset)
        {}
    }
}

fn drain_midi_ring(ptr: *mut u8, port_idx: usize) -> Vec<crate::util::MidiEvent> {
    let ring = unsafe {
        let buf = midi_in_ring_ptr(ptr, port_idx);
        let (w, r) = midi_in_indices(ptr, port_idx);
        RingBuffer::new(buf, w, r, RING_CAPACITY)
    };
    let mut events = Vec::new();
    while let Some(ev) = ring.pop() {
        events.push(crate::util::MidiEvent {
            frame: ev.sample_offset,
            data: ev.data.to_vec(),
        });
    }
    events
}

fn write_midi_out_ring(ptr: *mut u8, port_idx: usize, events: &[crate::util::MidiEvent]) {
    let ring = unsafe {
        let buf = midi_out_ring_ptr(ptr, port_idx);
        let (w, r) = midi_out_indices(ptr, port_idx);
        RingBuffer::new(buf, w, r, RING_CAPACITY)
    };
    for ev in events {
        let midi_ev = maolan_plugin_protocol::MidiEvent {
            sample_offset: ev.frame,
            data: {
                let mut d = [0u8; 3];
                for (i, b) in ev.data.iter().enumerate().take(3) {
                    d[i] = *b;
                }
                d
            },
            channel: ev.data.first().copied().unwrap_or(0) & 0x0F,
            flags: 0,
            _pad: 0,
        };
        if !ring.push(midi_ev) {
            break;
        }
    }
}

fn apply_vst3_transport(processor: &crate::vst3::Vst3Processor, ptr: *mut u8) {
    let transport = unsafe { transport_ref(ptr) };
    let info = crate::vst3::processor::Vst3TransportInfo {
        playhead_sample: transport.playhead_sample as i64,
        playing: transport.flags & 0x1 != 0,
        tempo: transport.tempo,
        tsig_num: transport.numerator as i32,
        tsig_denom: transport.denominator as i32,
    };
    processor.set_transport_info(info);
}

fn refresh_vst3_param_cache(processor: &crate::vst3::Vst3Processor, cache: &mut HashMap<u32, f32>) {
    cache.clear();
    for param in processor.parameters() {
        let current = processor.get_parameter_value(param.id).unwrap_or(0.0);
        cache.insert(param.id, current);
    }
}

fn write_vst3_echo_ring(
    processor: &crate::vst3::Vst3Processor,
    ptr: *mut u8,
    cache: &mut HashMap<u32, f32>,
) {
    let ring = unsafe {
        let buf = echo_ring_ptr(ptr);
        let (w, r) = echo_indices(ptr);
        RingBuffer::new(buf, w, r, RING_CAPACITY)
    };

    for (param_id, value) in processor.ui_take_param_updates() {
        let ev = ParameterEvent {
            param_index: param_id,
            value: value as f32,
            sample_offset: 0,
            event_kind: PARAM_EVENT_VALUE,
        };
        if !ring.push(ev) {
            break;
        }

        cache.insert(param_id, value as f32);
    }

    for param in processor.parameters() {
        let current = processor.get_parameter_value(param.id).unwrap_or(0.0);
        if cache.get(&param.id) != Some(&current) {
            let ev = ParameterEvent {
                param_index: param.id,
                value: current,
                sample_offset: 0,
                event_kind: PARAM_EVENT_VALUE,
            };
            if !ring.push(ev) {
                break;
            }
            cache.insert(param.id, current);
        }
    }
}

fn serialize_vst3_state(
    scratch: *mut u8,
    state: &crate::vst3::state::Vst3PluginState,
) -> Result<usize, String> {
    let max_len = SCRATCH_SIZE;
    let mut offset = 0usize;

    let plugin_id_bytes = state.plugin_id.as_bytes();
    if offset + 4 > max_len {
        return Err("scratch overflow".to_string());
    }
    unsafe {
        std::ptr::write_unaligned(
            scratch.add(offset) as *mut u32,
            plugin_id_bytes.len() as u32,
        );
    }
    offset += 4;
    if offset + plugin_id_bytes.len() > max_len {
        return Err("scratch overflow".to_string());
    }
    unsafe {
        std::ptr::copy_nonoverlapping(
            plugin_id_bytes.as_ptr(),
            scratch.add(offset),
            plugin_id_bytes.len(),
        );
    }
    offset += plugin_id_bytes.len();

    if offset + 4 > max_len {
        return Err("scratch overflow".to_string());
    }
    unsafe {
        std::ptr::write_unaligned(
            scratch.add(offset) as *mut u32,
            state.component_state.len() as u32,
        );
    }
    offset += 4;
    if offset + state.component_state.len() > max_len {
        return Err("scratch overflow".to_string());
    }
    unsafe {
        std::ptr::copy_nonoverlapping(
            state.component_state.as_ptr(),
            scratch.add(offset),
            state.component_state.len(),
        );
    }
    offset += state.component_state.len();

    if offset + 4 > max_len {
        return Err("scratch overflow".to_string());
    }
    unsafe {
        std::ptr::write_unaligned(
            scratch.add(offset) as *mut u32,
            state.controller_state.len() as u32,
        );
    }
    offset += 4;
    if offset + state.controller_state.len() > max_len {
        return Err("scratch overflow".to_string());
    }
    unsafe {
        std::ptr::copy_nonoverlapping(
            state.controller_state.as_ptr(),
            scratch.add(offset),
            state.controller_state.len(),
        );
    }
    offset += state.controller_state.len();

    Ok(offset)
}

fn deserialize_vst3_state(
    scratch: *const u8,
    size: usize,
) -> Result<crate::vst3::state::Vst3PluginState, String> {
    if size < 12 {
        return Err("scratch too small for VST3 state".to_string());
    }
    let mut offset = 0usize;

    let plugin_id_len =
        unsafe { std::ptr::read_unaligned(scratch.add(offset) as *const u32) } as usize;
    offset += 4;
    if offset + plugin_id_len > size {
        return Err("scratch underflow".to_string());
    }
    let mut plugin_id_bytes = vec![0u8; plugin_id_len];
    unsafe {
        std::ptr::copy_nonoverlapping(
            scratch.add(offset),
            plugin_id_bytes.as_mut_ptr(),
            plugin_id_len,
        );
    }
    offset += plugin_id_len;
    let plugin_id = String::from_utf8(plugin_id_bytes).map_err(|e| e.to_string())?;

    let component_state_len =
        unsafe { std::ptr::read_unaligned(scratch.add(offset) as *const u32) } as usize;
    offset += 4;
    if offset + component_state_len > size {
        return Err("scratch underflow".to_string());
    }
    let mut component_state = vec![0u8; component_state_len];
    unsafe {
        std::ptr::copy_nonoverlapping(
            scratch.add(offset),
            component_state.as_mut_ptr(),
            component_state_len,
        );
    }
    offset += component_state_len;

    let controller_state_len =
        unsafe { std::ptr::read_unaligned(scratch.add(offset) as *const u32) } as usize;
    offset += 4;
    if offset + controller_state_len > size {
        return Err("scratch underflow".to_string());
    }
    let mut controller_state = vec![0u8; controller_state_len];
    unsafe {
        std::ptr::copy_nonoverlapping(
            scratch.add(offset),
            controller_state.as_mut_ptr(),
            controller_state_len,
        );
    }

    Ok(crate::vst3::state::Vst3PluginState {
        plugin_id,
        component_state,
        controller_state,
    })
}

pub struct Vst3RunArgs<'a> {
    pub plugin_path: &'a str,
    pub mapping: ShmMapping,
    pub events: EventPair,
    pub instance_id: &'a str,
    pub sample_rate: f64,
    pub buffer_size: usize,
    pub num_inputs: usize,
    pub num_outputs: usize,
}

pub fn run_vst3(args: Vst3RunArgs) {
    #[cfg(windows)]
    let _com = ComInitGuard::new();

    let Vst3RunArgs {
        plugin_path,
        mapping,
        events,
        instance_id: _,
        sample_rate,
        buffer_size,
        num_inputs,
        num_outputs,
    } = args;

    let header = unsafe { header_ref(mapping.as_ptr()) };
    let ptr = mapping.as_ptr();

    match plugin_path {
        "__test__" => {
            let scratch = unsafe { scratch_ptr(mapping.as_ptr()) };
            unsafe {
                std::ptr::write_unaligned(scratch as *mut u32, 0xDEADBEEF);
            }
            header.ready.store(1, Ordering::Release);
            return;
        }
        "__crash__" => {
            header.ready.store(1, Ordering::Release);
            std::process::exit(1);
        }
        "__hang__" => {
            header.ready.store(1, Ordering::Release);
            loop {
                std::thread::sleep(Duration::from_secs(60));
            }
        }
        _ => {}
    }

    let processor = match crate::vst3::Vst3Processor::new_with_sample_rate(
        sample_rate,
        buffer_size,
        plugin_path,
        num_inputs,
        num_outputs,
    ) {
        Ok(p) => p,
        Err(_e) => {
            return;
        }
    };

    processor.setup_audio_ports();

    unsafe {
        maolan_plugin_protocol::protocol::write_plugin_name_to_scratch(
            mapping.as_ptr(),
            processor.name(),
        );
    }

    header
        .midi_in_port_count
        .store(processor.midi_input_count() as u32, Ordering::Release);
    header
        .midi_out_port_count
        .store(processor.midi_output_count() as u32, Ordering::Release);
    unsafe {
        latency_samples_atomic(ptr).store(processor.latency_samples(), Ordering::Release);
    }
    header.ready.store(1, Ordering::Release);

    let mut vst3_param_cache = HashMap::new();
    refresh_vst3_param_cache(&processor, &mut vst3_param_cache);

    #[cfg(any(windows, all(unix, not(target_os = "macos"))))]
    let mut vst3_gui_window: Option<Vst3GuiWindow> = None;

    loop {
        if header.shutdown_request.load(Ordering::Acquire) != 0 {
            break;
        }

        let req = header.request_type.load(Ordering::Acquire);
        if req != 0 {
            let scratch = unsafe { scratch_ptr(ptr) };
            let result = match req {
                1 => match processor.snapshot_state() {
                    Ok(state) => match serialize_vst3_state(scratch, &state) {
                        Ok(size) => {
                            header.scratch_size.store(size as u32, Ordering::Release);
                            Ok(())
                        }
                        Err(e) => Err(e),
                    },
                    Err(e) => Err(e),
                },
                2 => {
                    let size = header.scratch_size.load(Ordering::Acquire) as usize;
                    match deserialize_vst3_state(scratch, size) {
                        Ok(state) => {
                            let result = processor.restore_state(&state);
                            if result.is_ok() {
                                refresh_vst3_param_cache(&processor, &mut vst3_param_cache);
                            }
                            result
                        }
                        Err(e) => Err(e),
                    }
                }
                3 => {
                    #[cfg(any(windows, all(unix, not(target_os = "macos"))))]
                    {
                        if vst3_gui_window.is_none() {
                            match create_vst3_gui(&processor, plugin_path, ptr) {
                                Ok(gw) => {
                                    vst3_gui_window = Some(gw);
                                    processor.gui_show()
                                }
                                Err(e) => Err(e),
                            }
                        } else {
                            #[cfg(all(unix, not(target_os = "macos")))]
                            if let Some(ref gw) = vst3_gui_window {
                                unsafe {
                                    x11_ffi::XMapWindow(gw.display, gw.window);
                                    x11_ffi::XFlush(gw.display);
                                }
                            }
                            #[cfg(windows)]
                            if let Some(ref gw) = vst3_gui_window {
                                unsafe {
                                    windows_sys::Win32::UI::WindowsAndMessaging::ShowWindow(
                                        gw.hwnd,
                                        windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOW,
                                    );
                                }
                            }
                            processor.gui_show()
                        }
                    }
                    #[cfg(not(any(windows, all(unix, not(target_os = "macos")))))]
                    Err("VST3 GUI not supported on this platform".to_string())
                }
                4 => {
                    #[cfg(all(unix, not(target_os = "macos")))]
                    if let Some(ref gw) = vst3_gui_window {
                        unsafe {
                            x11_ffi::XUnmapWindow(gw.display, gw.window);
                            x11_ffi::XFlush(gw.display);
                        }
                    }
                    #[cfg(windows)]
                    if let Some(ref gw) = vst3_gui_window {
                        unsafe {
                            windows_sys::Win32::UI::WindowsAndMessaging::ShowWindow(
                                gw.hwnd,
                                windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE,
                            );
                        }
                    }
                    processor.gui_hide();
                    Ok(())
                }
                _ => Err(format!("Unknown request type: {}", req)),
            };
            header
                .request_status
                .store(if result.is_ok() { 1 } else { 2 }, Ordering::Release);

            if req == 1 || req == 2 {
                let _ = events.signal_daw();
            }
            header.request_type.store(0, Ordering::Release);
            continue;
        }

        #[cfg(windows)]
        let wait_result = events.wait_daw_with_message_pump(Duration::from_millis(100));
        #[cfg(not(windows))]
        let wait_result = events.wait_daw(Duration::from_millis(100));
        match wait_result {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
            Err(_e) => {
                break;
            }
        }

        let block_size = header.block_size.load(Ordering::Acquire) as usize;
        let num_in = header.num_input_channels.load(Ordering::Acquire) as usize;
        let num_out = header.num_output_channels.load(Ordering::Acquire) as usize;

        if block_size == 0 || block_size > MAX_BLOCK_SIZE {
            let _ = events.signal_daw();
            continue;
        }

        apply_vst3_param_ring(&processor, ptr);

        apply_vst3_transport(&processor, ptr);

        let inputs = processor.audio_inputs();
        for (ch, input) in inputs.iter().enumerate().take(num_in) {
            let src = unsafe { audio_channel_ptr(ptr, ch, 0) };
            let dst = input.buffer.lock();
            let len = block_size.min(dst.len());
            unsafe {
                std::ptr::copy_nonoverlapping(src, dst.as_mut_ptr(), len);
            }
            *input.finished.lock() = true;
        }

        let mut midi_input = Vec::new();
        for port in 0..processor.midi_input_count() {
            midi_input.extend(drain_midi_ring(ptr, port));
        }
        midi_input.sort_by_key(|ev| ev.frame);

        let _midi_output = if processor.midi_input_count() > 0 || processor.midi_output_count() > 0
        {
            processor.process_with_midi(block_size, &midi_input)
        } else {
            processor.process_with_audio_io(block_size);
            Vec::new()
        };
        unsafe {
            latency_samples_atomic(ptr).store(processor.latency_samples(), Ordering::Release);
        }

        write_vst3_echo_ring(&processor, ptr, &mut vst3_param_cache);

        for port in 0..processor.midi_output_count().max(1) {
            write_midi_out_ring(ptr, port, &_midi_output);
        }

        let outputs = processor.audio_outputs();
        for (ch, output) in outputs.iter().enumerate().take(num_out) {
            let src = output.buffer.lock();
            let dst = unsafe { audio_channel_ptr(ptr, ch, 1) };
            let len = block_size.min(src.len());
            unsafe {
                std::ptr::copy_nonoverlapping(src.as_ptr(), dst, len);
            }
        }

        if let Err(_e) = events.signal_daw() {
            break;
        }
    }
}

#[cfg(unix)]
fn apply_lv2_param_ring(processor: &mut crate::lv2::Lv2Processor, ptr: *mut u8) {
    let ring = unsafe {
        let buf = param_ring_ptr(ptr);
        let (w, r) = param_indices(ptr);
        RingBuffer::new(buf, w, r, RING_CAPACITY)
    };
    while let Some(ev) = ring.pop() {
        if let Err(_e) = processor.set_control_value(ev.param_index, ev.value) {}
    }
}

#[cfg(unix)]
fn read_lv2_transport(ptr: *mut u8) -> crate::lv2::Lv2TransportInfo {
    let transport = unsafe { transport_ref(ptr) };
    crate::lv2::Lv2TransportInfo {
        transport_sample: transport.playhead_sample as usize,
        playing: transport.flags & 0x1 != 0,
        bpm: transport.tempo,
        tsig_num: transport.numerator,
        tsig_denom: transport.denominator,
    }
}

#[cfg(unix)]
fn refresh_lv2_param_cache(processor: &crate::lv2::Lv2Processor, cache: &mut HashMap<u32, f32>) {
    cache.clear();
    for port in processor.control_ports_with_values() {
        cache.insert(port.index, port.value);
    }
}

#[cfg(unix)]
fn write_lv2_echo_ring(
    processor: &crate::lv2::Lv2Processor,
    ptr: *mut u8,
    cache: &mut HashMap<u32, f32>,
) {
    let ring = unsafe {
        let buf = echo_ring_ptr(ptr);
        let (w, r) = echo_indices(ptr);
        RingBuffer::new(buf, w, r, RING_CAPACITY)
    };
    for port in processor.control_ports_with_values() {
        let current = port.value;
        if cache.get(&port.index) != Some(&current) {
            let ev = ParameterEvent {
                param_index: port.index,
                value: current,
                sample_offset: 0,
                event_kind: PARAM_EVENT_VALUE,
            };
            if !ring.push(ev) {
                break;
            }
            cache.insert(port.index, current);
        }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
type Lv2UiDescriptorFn = unsafe extern "C" fn(index: u32) -> *const LV2UIDescriptorRaw;

#[cfg(all(unix, not(target_os = "macos")))]
#[derive(Clone, Copy)]
struct Lv2UiWrite {
    port_index: u32,
    value: f32,
}

#[cfg(all(unix, not(target_os = "macos")))]
struct Lv2UiController {
    writes: std::sync::Mutex<Vec<Lv2UiWrite>>,
}

#[cfg(all(unix, not(target_os = "macos")))]
extern "C" fn lv2_ui_write_function(
    controller: LV2UIControllerRaw,
    port_index: libc::c_uint,
    buffer_size: libc::c_uint,
    port_protocol: libc::c_uint,
    buffer: *const libc::c_void,
) {
    if controller.is_null()
        || buffer.is_null()
        || port_protocol != 0
        || buffer_size as usize != std::mem::size_of::<f32>()
    {
        return;
    }
    let controller = unsafe { &*(controller as *const Lv2UiController) };
    let value = unsafe { *(buffer as *const f32) };
    controller
        .writes
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(Lv2UiWrite { port_index, value });
}

#[cfg(all(unix, not(target_os = "macos")))]
struct Lv2UiFeatureSet {
    _uris: Vec<std::ffi::CString>,
    _features: Vec<LV2Feature>,
    ptrs: Vec<*const LV2Feature>,
}

#[cfg(all(unix, not(target_os = "macos")))]
impl Lv2UiFeatureSet {
    fn new(parent: Option<x11_ffi::Window>, show_interface: bool) -> Result<Self, String> {
        let mut uris =
            vec![std::ffi::CString::new(LV2_UI_IDLE_INTERFACE).map_err(|e| e.to_string())?];
        if parent.is_some() {
            uris.push(std::ffi::CString::new(LV2_UI_PARENT).map_err(|e| e.to_string())?);
        }
        if show_interface {
            uris.push(std::ffi::CString::new(LV2_UI_SHOW_INTERFACE).map_err(|e| e.to_string())?);
        }

        let mut features = Vec::with_capacity(uris.len());
        features.push(LV2Feature {
            uri: uris[0].as_ptr(),
            data: std::ptr::null_mut(),
        });
        if let Some(parent) = parent {
            features.push(LV2Feature {
                uri: uris[1].as_ptr(),
                data: parent as usize as *mut libc::c_void,
            });
        }
        if show_interface {
            features.push(LV2Feature {
                uri: uris.last().expect("show interface URI exists").as_ptr(),
                data: std::ptr::null_mut(),
            });
        }
        let mut ptrs: Vec<*const LV2Feature> =
            features.iter().map(|feature| feature as *const _).collect();
        ptrs.push(std::ptr::null());
        Ok(Self {
            _uris: uris,
            _features: features,
            ptrs,
        })
    }

    fn as_ptr(&self) -> *const *const LV2Feature {
        self.ptrs.as_ptr()
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
#[derive(Clone, Copy)]
enum Lv2GuiSurface {
    X11 {
        display: *mut x11_ffi::Display,
        container: x11_ffi::Window,
        child: x11_ffi::Window,
        wm_delete_window: x11_ffi::Atom,
    },
    External {
        show_interface: *const LV2UIShowInterface,
    },
}

#[cfg(all(unix, not(target_os = "macos")))]
struct Lv2GuiWindow {
    surface: Lv2GuiSurface,
    descriptor: *const LV2UIDescriptorRaw,
    handle: LV2UIHandle,
    idle_interface: Option<*const LV2UIIdleInterface>,
    _lib: libloading::Library,
    _features: Lv2UiFeatureSet,
    controller: Box<Lv2UiController>,
}

#[cfg(all(unix, not(target_os = "macos")))]
unsafe impl Send for Lv2GuiWindow {}

#[cfg(all(unix, not(target_os = "macos")))]
impl Lv2GuiWindow {
    fn show(&self) {
        match self.surface {
            Lv2GuiSurface::X11 {
                display,
                container,
                child,
                ..
            } => unsafe {
                x11_ffi::XMapWindow(display, child);
                x11_ffi::XMapWindow(display, container);
                x11_ffi::XFlush(display);
            },
            Lv2GuiSurface::External { show_interface } => {
                let show = unsafe { (*show_interface).show };
                let _ = show(self.handle);
            }
        }
    }

    fn hide(&self) {
        match self.surface {
            Lv2GuiSurface::X11 {
                display, container, ..
            } => unsafe {
                x11_ffi::XUnmapWindow(display, container);
                x11_ffi::XFlush(display);
            },
            Lv2GuiSurface::External { show_interface } => {
                let hide = unsafe { (*show_interface).hide };
                let _ = hide(self.handle);
            }
        }
    }

    fn poll_x11_close_request(&self) -> bool {
        let Lv2GuiSurface::X11 {
            display,
            container,
            wm_delete_window,
            ..
        } = self.surface
        else {
            return false;
        };
        if display.is_null() || container == 0 {
            return false;
        }

        let mut destroyed = false;
        unsafe {
            while x11_ffi::XPending(display) > 0 {
                let mut event = x11_ffi::XEvent { pad: [0; 24] };
                x11_ffi::XNextEvent(display, &mut event);
                match event.type_ {
                    x11_ffi::CLIENT_MESSAGE => {
                        let client = event.client_message;
                        if client.window == container
                            && client.format == 32
                            && wm_delete_window != 0
                            && client.data.l[0] as x11_ffi::Atom == wm_delete_window
                        {
                            x11_ffi::XUnmapWindow(display, container);
                            x11_ffi::XFlush(display);
                        }
                    }
                    x11_ffi::DESTROY_NOTIFY => {
                        let destroy = event.destroy_window;
                        if destroy.window == container {
                            destroyed = true;
                        }
                    }
                    _ => {}
                }
            }
        }
        destroyed
    }

    fn idle(&self) -> bool {
        if self.poll_x11_close_request() {
            return true;
        }
        let Some(idle_interface) = self.idle_interface else {
            return false;
        };
        let idle = unsafe { (*idle_interface).idle };
        idle(self.handle) != 0
    }

    fn drain_writes(&self) -> Vec<Lv2UiWrite> {
        self.controller
            .writes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .drain(..)
            .collect()
    }

    fn send_control_ports(&self, processor: &crate::lv2::Lv2Processor) {
        let port_event = unsafe { (*self.descriptor).port_event };
        if (port_event as *const ()) as usize == 0 {
            return;
        }
        for port in processor.ui_control_port_values() {
            let value = port.value;
            port_event(
                self.handle,
                port.index,
                std::mem::size_of::<f32>() as u32,
                0,
                (&value as *const f32).cast(),
            );
        }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
impl Drop for Lv2GuiWindow {
    fn drop(&mut self) {
        unsafe {
            if let Lv2GuiSurface::External { show_interface } = self.surface {
                ((*show_interface).hide)(self.handle);
            }
            let cleanup = (*self.descriptor).cleanup;
            if (cleanup as *const ()) as usize != 0 && !self.handle.is_null() {
                cleanup(self.handle);
            }
            if let Lv2GuiSurface::X11 {
                display, container, ..
            } = self.surface
                && !display.is_null()
            {
                x11_ffi::XDestroyWindow(display, container);
                x11_ffi::XFlush(display);
                x11_ffi::XCloseDisplay(display);
            }
        }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn file_uri_to_path(uri: &str) -> Option<String> {
    let rest = uri.strip_prefix("file://")?;
    let path_start = rest.find('/')?;
    let authority = &rest[..path_start];
    if !authority.is_empty() && !authority.eq_ignore_ascii_case("localhost") {
        return None;
    }
    let path = &rest[path_start..];
    let mut out = Vec::with_capacity(path.len());
    let bytes = path.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return None;
            }
            let hex = std::str::from_utf8(&bytes[i + 1..=i + 2]).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(all(unix, not(target_os = "macos")))]
fn load_lv2_ui_descriptor(
    ui: &crate::lv2::Lv2UiInfo,
) -> Result<(libloading::Library, *const LV2UIDescriptorRaw, String), String> {
    let ui_binary = file_uri_to_path(&ui.binary_uri)
        .ok_or_else(|| format!("LV2 GUI: unsupported UI binary URI '{}'", ui.binary_uri))?;
    let lib = unsafe {
        libloading::os::unix::Library::open(
            Some(&ui_binary),
            libloading::os::unix::RTLD_NOW | libloading::os::unix::RTLD_LOCAL,
        )
        .map(libloading::Library::from)
    }
    .map_err(|e| format!("LV2 GUI: failed to open UI library '{ui_binary}': {e}"))?;
    let descriptor_fn = unsafe {
        *lib.get::<Lv2UiDescriptorFn>(b"lv2ui_descriptor\0")
            .map_err(|e| format!("LV2 GUI: no lv2ui_descriptor in '{ui_binary}': {e}"))?
    };
    let descriptor = {
        let mut index = 0_u32;
        loop {
            let descriptor = unsafe { descriptor_fn(index) };
            if descriptor.is_null() {
                return Err(format!(
                    "LV2 GUI: UI descriptor '{}' not found in '{}'",
                    ui.uri, ui_binary
                ));
            }
            let descriptor_uri = unsafe { std::ffi::CStr::from_ptr((*descriptor).uri) };
            if descriptor_uri.to_bytes() == ui.uri.as_bytes() {
                break descriptor;
            }
            index += 1;
        }
    };
    Ok((lib, descriptor, ui_binary))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn lv2_ui_interface<T>(descriptor: *const LV2UIDescriptorRaw, uri: &str) -> Option<*const T> {
    unsafe {
        (*descriptor).extension_data.and_then(|extension_data| {
            let uri = std::ffi::CString::new(uri).ok()?;
            let ptr = extension_data(uri.as_ptr());
            (!ptr.is_null()).then_some(ptr as *const T)
        })
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn instantiate_lv2_ui(
    ui: &crate::lv2::Lv2UiInfo,
    processor: &crate::lv2::Lv2Processor,
    plugin_uri: &str,
    parent: Option<x11_ffi::Window>,
    show_interface: bool,
) -> Result<Lv2GuiWindow, String> {
    let mut ui_bundle = file_uri_to_path(&ui.bundle_uri)
        .ok_or_else(|| format!("LV2 GUI: unsupported UI bundle URI '{}'", ui.bundle_uri))?;
    if !ui_bundle.ends_with(std::path::MAIN_SEPARATOR) {
        ui_bundle.push(std::path::MAIN_SEPARATOR);
    }

    let (lib, descriptor, ui_binary) = load_lv2_ui_descriptor(ui)?;
    let features = Lv2UiFeatureSet::new(parent, show_interface)?;
    let controller = Box::new(Lv2UiController {
        writes: std::sync::Mutex::new(Vec::new()),
    });
    let plugin_uri = std::ffi::CString::new(plugin_uri).map_err(|e| e.to_string())?;
    let bundle_path = std::ffi::CString::new(ui_bundle).map_err(|e| e.to_string())?;
    let mut widget: LV2UIWidget = std::ptr::null_mut();
    let handle = unsafe {
        ((*descriptor).instantiate_raw)(
            descriptor,
            plugin_uri.as_ptr(),
            bundle_path.as_ptr(),
            Some(lv2_ui_write_function),
            (&*controller as *const Lv2UiController).cast(),
            &mut widget,
            features.as_ptr(),
        )
    };
    if handle.is_null() {
        return Err(format!(
            "LV2 GUI: failed to instantiate UI '{}' from '{}'",
            ui.uri, ui_binary
        ));
    }

    let idle_interface = lv2_ui_interface::<LV2UIIdleInterface>(descriptor, LV2_UI_IDLE_INTERFACE);
    let show_interface = lv2_ui_interface::<LV2UIShowInterface>(descriptor, LV2_UI_SHOW_INTERFACE);

    if parent.is_none() {
        let Some(show_interface) = show_interface else {
            unsafe {
                ((*descriptor).cleanup)(handle);
            }
            return Err(format!(
                "LV2 GUI: UI '{}' does not provide ui:showInterface",
                ui.uri
            ));
        };
        let show = unsafe { (*show_interface).show };
        if show(handle) != 0 {
            unsafe {
                ((*descriptor).cleanup)(handle);
            }
            return Err(format!("LV2 GUI: UI '{}' refused to show", ui.uri));
        }
        let window = Lv2GuiWindow {
            surface: Lv2GuiSurface::External { show_interface },
            descriptor,
            handle,
            idle_interface,
            _lib: lib,
            _features: features,
            controller,
        };
        window.send_control_ports(processor);
        return Ok(window);
    }

    if widget.is_null() {
        unsafe {
            ((*descriptor).cleanup)(handle);
        }
        return Err(format!("LV2 GUI: UI '{}' returned a null widget", ui.uri));
    }

    let window = Lv2GuiWindow {
        surface: Lv2GuiSurface::X11 {
            display: std::ptr::null_mut(),
            container: 0,
            child: widget as usize as x11_ffi::Window,
            wm_delete_window: 0,
        },
        descriptor,
        handle,
        idle_interface,
        _lib: lib,
        _features: features,
        controller,
    };
    window.send_control_ports(processor);
    Ok(window)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn create_lv2_x11_gui(
    processor: &crate::lv2::Lv2Processor,
    plugin_uri: &str,
    ptr: *mut u8,
    ui: &crate::lv2::Lv2UiInfo,
) -> Result<Lv2GuiWindow, String> {
    let display = unsafe { x11_ffi::XOpenDisplay(std::ptr::null()) };
    if display.is_null() {
        return Err("LV2 GUI: failed to open X11 display".to_string());
    }

    let screen = unsafe { x11_ffi::XDefaultScreen(display) };
    let root = unsafe { x11_ffi::XRootWindow(display, screen) };
    let black = unsafe { x11_ffi::XBlackPixel(display, screen) };
    let white = unsafe { x11_ffi::XWhitePixel(display, screen) };
    let parent_window = {
        let header = unsafe { header_ref(ptr) };
        if header.gui_mode() == GuiMode::Floating {
            root
        } else {
            match header.gui_parent_api() {
                GuiParentApi::X11 => {
                    let parent = header.parent_window_usize();
                    if parent != 0 {
                        parent as x11_ffi::Window
                    } else {
                        root
                    }
                }
                GuiParentApi::Wayland => root,
                GuiParentApi::None => root,
            }
        }
    };
    let container = unsafe {
        x11_ffi::XCreateSimpleWindow(display, parent_window, 0, 0, 800, 600, 1, black, white)
    };
    if container == 0 {
        unsafe {
            x11_ffi::XCloseDisplay(display);
        }
        return Err("LV2 GUI: failed to create X11 container window".to_string());
    }
    if let Ok(title) = std::ffi::CString::new(processor.name()) {
        unsafe {
            x11_ffi::XStoreName(display, container, title.as_ptr());
        }
    }
    let wm_delete_window = if let Ok(atom_name) = std::ffi::CString::new("WM_DELETE_WINDOW") {
        let mut atom = unsafe { x11_ffi::XInternAtom(display, atom_name.as_ptr(), 0) };
        if atom != 0 {
            unsafe {
                x11_ffi::XSetWMProtocols(display, container, &mut atom, 1);
            }
        }
        atom
    } else {
        0
    };
    unsafe {
        x11_ffi::XSelectInput(display, container, x11_ffi::STRUCTURE_NOTIFY_MASK);
        x11_ffi::XSync(display, 0);
    }

    let result = (|| {
        let mut window = instantiate_lv2_ui(ui, processor, plugin_uri, Some(container), false)?;
        let widget = match window.surface {
            Lv2GuiSurface::X11 { child, .. } => child,
            Lv2GuiSurface::External { .. } => return Ok(window),
        };
        unsafe {
            x11_ffi::XReparentWindow(display, widget, container, 0, 0);
            x11_ffi::XMapWindow(display, widget);
            x11_ffi::XMapWindow(display, container);
            x11_ffi::XFlush(display);
        }
        window.surface = Lv2GuiSurface::X11 {
            display,
            container,
            child: widget,
            wm_delete_window,
        };
        Ok(window)
    })();

    if result.is_err() {
        unsafe {
            x11_ffi::XDestroyWindow(display, container);
            x11_ffi::XFlush(display);
            x11_ffi::XCloseDisplay(display);
        }
    }
    result
}

#[cfg(all(unix, not(target_os = "macos")))]
fn create_lv2_external_gui(
    processor: &crate::lv2::Lv2Processor,
    plugin_uri: &str,
    ui: &crate::lv2::Lv2UiInfo,
) -> Result<Lv2GuiWindow, String> {
    instantiate_lv2_ui(ui, processor, plugin_uri, None, true)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn lv2_ui_has_x11_class(ui: &crate::lv2::Lv2UiInfo) -> bool {
    ui.class_uris.iter().any(|class| class == LV2_UI_X11_UI)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn lv2_ui_has_wayland_class(ui: &crate::lv2::Lv2UiInfo) -> bool {
    ui.class_uris
        .iter()
        .any(|class| class.to_ascii_lowercase().contains("wayland"))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn lv2_ui_supports_show_interface(ui: &crate::lv2::Lv2UiInfo) -> bool {
    ui.extension_data_uris
        .iter()
        .any(|uri| uri == LV2_UI_SHOW_INTERFACE)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn floating_gui_parent_api() -> GuiParentApi {
    let forced_x11 = ["WINIT_UNIX_BACKEND", "GDK_BACKEND", "QT_QPA_PLATFORM"]
        .iter()
        .filter_map(std::env::var_os)
        .filter_map(|value| value.into_string().ok())
        .any(|value| {
            let value = value.to_ascii_lowercase();
            value.contains("x11") || value.contains("xcb")
        });
    if forced_x11 {
        GuiParentApi::X11
    } else if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        GuiParentApi::Wayland
    } else if std::env::var_os("DISPLAY").is_some() {
        GuiParentApi::X11
    } else {
        GuiParentApi::None
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn create_lv2_embedded_wayland_gui(
    processor: &crate::lv2::Lv2Processor,
    plugin_uri: &str,
    parent: usize,
    ui: &crate::lv2::Lv2UiInfo,
) -> Result<Lv2GuiWindow, String> {
    if parent == 0 {
        return Err("LV2 GUI: embedded Wayland requested without a parent surface".to_string());
    }
    instantiate_lv2_ui(
        ui,
        processor,
        plugin_uri,
        Some(parent as x11_ffi::Window),
        false,
    )
}

#[cfg(all(unix, not(target_os = "macos")))]
fn try_lv2_external_gui_by_backend(
    processor: &crate::lv2::Lv2Processor,
    plugin_uri: &str,
    ui_infos: &[crate::lv2::Lv2UiInfo],
    backend: GuiParentApi,
) -> Result<Lv2GuiWindow, Vec<String>> {
    let mut errors = Vec::new();
    for ui in ui_infos.iter().filter(|ui| {
        lv2_ui_supports_show_interface(ui)
            && match backend {
                GuiParentApi::Wayland => lv2_ui_has_wayland_class(ui),
                GuiParentApi::X11 => lv2_ui_has_x11_class(ui),
                GuiParentApi::None => false,
            }
    }) {
        match create_lv2_external_gui(processor, plugin_uri, ui) {
            Ok(window) => return Ok(window),
            Err(error) => errors.push(error),
        }
    }
    Err(errors)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn create_lv2_gui(
    processor: &crate::lv2::Lv2Processor,
    plugin_uri: &str,
    ptr: *mut u8,
) -> Result<Lv2GuiWindow, String> {
    let ui_infos = processor.ui_infos();
    if ui_infos.is_empty() {
        return Err("LV2 GUI: plugin does not advertise any UI".to_string());
    }

    let header = unsafe { header_ref(ptr) };
    let gui_mode = header.gui_mode();
    let requested_api = match gui_mode {
        GuiMode::Embedded => header.gui_parent_api(),
        GuiMode::Floating => floating_gui_parent_api(),
    };
    let parent = header.parent_window_usize();

    match requested_api {
        GuiParentApi::Wayland => {
            if gui_mode == GuiMode::Floating {
                match try_lv2_external_gui_by_backend(
                    processor,
                    plugin_uri,
                    &ui_infos,
                    GuiParentApi::Wayland,
                ) {
                    Ok(window) => return Ok(window),
                    Err(errors) if !errors.is_empty() => tracing::warn!(
                        errors = %errors.join(" | "),
                        "LV2 GUI: floating Wayland UI failed; checking X11 fallback"
                    ),
                    Err(_) => {}
                }
            } else {
                for ui in ui_infos.iter().filter(|ui| lv2_ui_has_wayland_class(ui)) {
                    match create_lv2_embedded_wayland_gui(processor, plugin_uri, parent, ui) {
                        Ok(window) => return Ok(window),
                        Err(error) => tracing::warn!(%error, "LV2 GUI: embedded Wayland UI failed"),
                    }
                }
            }
        }
        GuiParentApi::X11 => {
            if gui_mode == GuiMode::Floating {
                match try_lv2_external_gui_by_backend(
                    processor,
                    plugin_uri,
                    &ui_infos,
                    GuiParentApi::X11,
                ) {
                    Ok(window) => return Ok(window),
                    Err(errors) if !errors.is_empty() => tracing::warn!(
                        errors = %errors.join(" | "),
                        "LV2 GUI: floating X11 UI failed; checking embedded X11 fallback"
                    ),
                    Err(_) => {}
                }
            }
        }
        GuiParentApi::None => {}
    }

    let Some(ui) = ui_infos.iter().find(|ui| lv2_ui_has_x11_class(ui)) else {
        let supported = ui_infos
            .into_iter()
            .flat_map(|ui| ui.class_uris)
            .collect::<Vec<_>>();
        return Err(format!(
            "LV2 GUI: no supported UI for {:?} {:?}; plugin UI types: {}",
            gui_mode,
            requested_api,
            supported.join(", ")
        ));
    };

    create_lv2_x11_gui(processor, plugin_uri, ptr, ui)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn pump_lv2_gui(
    window: &mut Option<Lv2GuiWindow>,
    processor: &mut crate::lv2::Lv2Processor,
    ptr: *mut u8,
    param_cache: &mut HashMap<u32, f32>,
) {
    let Some(gui_window) = window else {
        return;
    };
    for write in gui_window.drain_writes() {
        let _ = processor.set_control_value(write.port_index, write.value);
    }
    write_lv2_echo_ring(processor, ptr, param_cache);
    gui_window.send_control_ports(processor);
    if gui_window.idle() {
        *window = None;
    }
}

#[cfg(unix)]
fn serialize_lv2_state(scratch: *mut u8, state: &Lv2PluginState) -> Result<usize, String> {
    let max_len = SCRATCH_SIZE;
    let mut offset = 0usize;

    if offset + 4 > max_len {
        return Err("scratch overflow".to_string());
    }
    unsafe {
        std::ptr::write_unaligned(
            scratch.add(offset) as *mut u32,
            state.port_values.len() as u32,
        );
    }
    offset += 4;
    for v in &state.port_values {
        if offset + 8 > max_len {
            return Err("scratch overflow".to_string());
        }
        unsafe {
            std::ptr::write_unaligned(scratch.add(offset) as *mut u32, v.index);
        }
        offset += 4;
        unsafe {
            std::ptr::write_unaligned(scratch.add(offset) as *mut u32, v.value.to_bits());
        }
        offset += 4;
    }

    if offset + 4 > max_len {
        return Err("scratch overflow".to_string());
    }
    unsafe {
        std::ptr::write_unaligned(
            scratch.add(offset) as *mut u32,
            state.properties.len() as u32,
        );
    }
    offset += 4;
    for prop in &state.properties {
        let key_bytes = prop.key_uri.as_bytes();
        if offset + 4 > max_len {
            return Err("scratch overflow".to_string());
        }
        unsafe {
            std::ptr::write_unaligned(scratch.add(offset) as *mut u32, key_bytes.len() as u32);
        }
        offset += 4;
        if offset + key_bytes.len() > max_len {
            return Err("scratch overflow".to_string());
        }
        unsafe {
            std::ptr::copy_nonoverlapping(key_bytes.as_ptr(), scratch.add(offset), key_bytes.len());
        }
        offset += key_bytes.len();

        let type_bytes = prop.type_uri.as_bytes();
        if offset + 4 > max_len {
            return Err("scratch overflow".to_string());
        }
        unsafe {
            std::ptr::write_unaligned(scratch.add(offset) as *mut u32, type_bytes.len() as u32);
        }
        offset += 4;
        if offset + type_bytes.len() > max_len {
            return Err("scratch overflow".to_string());
        }
        unsafe {
            std::ptr::copy_nonoverlapping(
                type_bytes.as_ptr(),
                scratch.add(offset),
                type_bytes.len(),
            );
        }
        offset += type_bytes.len();

        if offset + 4 > max_len {
            return Err("scratch overflow".to_string());
        }
        unsafe {
            std::ptr::write_unaligned(scratch.add(offset) as *mut u32, prop.flags);
        }
        offset += 4;
        if offset + 4 > max_len {
            return Err("scratch overflow".to_string());
        }
        unsafe {
            std::ptr::write_unaligned(scratch.add(offset) as *mut u32, prop.value.len() as u32);
        }
        offset += 4;
        if offset + prop.value.len() > max_len {
            return Err("scratch overflow".to_string());
        }
        unsafe {
            std::ptr::copy_nonoverlapping(
                prop.value.as_ptr(),
                scratch.add(offset),
                prop.value.len(),
            );
        }
        offset += prop.value.len();
    }

    Ok(offset)
}

#[cfg(unix)]
fn deserialize_lv2_state(scratch: *const u8, size: usize) -> Result<Lv2PluginState, String> {
    if size < 8 {
        return Err("scratch too small for LV2 state".to_string());
    }
    let mut offset = 0usize;

    let port_count =
        unsafe { std::ptr::read_unaligned(scratch.add(offset) as *const u32) } as usize;
    offset += 4;
    let mut port_values = Vec::with_capacity(port_count);
    for _ in 0..port_count {
        if offset + 8 > size {
            return Err("scratch underflow".to_string());
        }
        let index = unsafe { std::ptr::read_unaligned(scratch.add(offset) as *const u32) };
        offset += 4;
        let bits = unsafe { std::ptr::read_unaligned(scratch.add(offset) as *const u32) };
        offset += 4;
        port_values.push(Lv2StatePortValue {
            index,
            value: f32::from_bits(bits),
        });
    }

    let prop_count =
        unsafe { std::ptr::read_unaligned(scratch.add(offset) as *const u32) } as usize;
    offset += 4;
    let mut properties = Vec::with_capacity(prop_count);
    for _ in 0..prop_count {
        let key_len =
            unsafe { std::ptr::read_unaligned(scratch.add(offset) as *const u32) } as usize;
        offset += 4;
        if offset + key_len > size {
            return Err("scratch underflow".to_string());
        }
        let mut key_bytes = vec![0u8; key_len];
        unsafe {
            std::ptr::copy_nonoverlapping(scratch.add(offset), key_bytes.as_mut_ptr(), key_len);
        }
        offset += key_len;
        let key_uri = String::from_utf8(key_bytes).map_err(|e| e.to_string())?;

        let type_len =
            unsafe { std::ptr::read_unaligned(scratch.add(offset) as *const u32) } as usize;
        offset += 4;
        if offset + type_len > size {
            return Err("scratch underflow".to_string());
        }
        let mut type_bytes = vec![0u8; type_len];
        unsafe {
            std::ptr::copy_nonoverlapping(scratch.add(offset), type_bytes.as_mut_ptr(), type_len);
        }
        offset += type_len;
        let type_uri = String::from_utf8(type_bytes).map_err(|e| e.to_string())?;

        let flags = unsafe { std::ptr::read_unaligned(scratch.add(offset) as *const u32) };
        offset += 4;
        let value_len =
            unsafe { std::ptr::read_unaligned(scratch.add(offset) as *const u32) } as usize;
        offset += 4;
        if offset + value_len > size {
            return Err("scratch underflow".to_string());
        }
        let mut value = vec![0u8; value_len];
        unsafe {
            std::ptr::copy_nonoverlapping(scratch.add(offset), value.as_mut_ptr(), value_len);
        }
        offset += value_len;

        properties.push(Lv2StateProperty {
            key_uri,
            type_uri,
            flags,
            value,
        });
    }

    Ok(Lv2PluginState {
        port_values,
        properties,
    })
}

#[cfg(unix)]
fn serialize_lv2_control_ports(
    scratch: *mut u8,
    ports: &[crate::lv2::Lv2ControlPortInfo],
) -> Result<usize, String> {
    let max_len = SCRATCH_SIZE;
    let mut offset = 0usize;

    if offset + 4 > max_len {
        return Err("scratch overflow".to_string());
    }
    unsafe {
        std::ptr::write_unaligned(scratch.add(offset) as *mut u32, ports.len() as u32);
    }
    offset += 4;

    for port in ports {
        if offset + 4 > max_len {
            return Err("scratch overflow".to_string());
        }
        unsafe {
            std::ptr::write_unaligned(scratch.add(offset) as *mut u32, port.index);
        }
        offset += 4;

        let name_bytes = port.name.as_bytes();
        if offset + 4 > max_len {
            return Err("scratch overflow".to_string());
        }
        unsafe {
            std::ptr::write_unaligned(scratch.add(offset) as *mut u32, name_bytes.len() as u32);
        }
        offset += 4;
        if offset + name_bytes.len() > max_len {
            return Err("scratch overflow".to_string());
        }
        unsafe {
            std::ptr::copy_nonoverlapping(
                name_bytes.as_ptr(),
                scratch.add(offset),
                name_bytes.len(),
            );
        }
        offset += name_bytes.len();

        if offset + 12 > max_len {
            return Err("scratch overflow".to_string());
        }
        unsafe {
            std::ptr::write_unaligned(scratch.add(offset) as *mut u32, port.min.to_bits());
            std::ptr::write_unaligned(scratch.add(offset + 4) as *mut u32, port.max.to_bits());
            std::ptr::write_unaligned(scratch.add(offset + 8) as *mut u32, port.value.to_bits());
        }
        offset += 12;
    }

    Ok(offset)
}

#[cfg(unix)]
fn serialize_lv2_note_names(
    scratch: *mut u8,
    note_names: &std::collections::HashMap<u8, String>,
) -> Result<usize, String> {
    let max_len = SCRATCH_SIZE;
    let mut offset = 0usize;

    if offset + 4 > max_len {
        return Err("scratch overflow".to_string());
    }
    unsafe {
        std::ptr::write_unaligned(scratch.add(offset) as *mut u32, note_names.len() as u32);
    }
    offset += 4;

    for (note, name) in note_names {
        if offset + 4 > max_len {
            return Err("scratch overflow".to_string());
        }
        unsafe {
            std::ptr::write_unaligned(scratch.add(offset) as *mut u32, u32::from(*note));
        }
        offset += 4;

        let name_bytes = name.as_bytes();
        if offset + 4 > max_len {
            return Err("scratch overflow".to_string());
        }
        unsafe {
            std::ptr::write_unaligned(scratch.add(offset) as *mut u32, name_bytes.len() as u32);
        }
        offset += 4;
        if offset + name_bytes.len() > max_len {
            return Err("scratch overflow".to_string());
        }
        unsafe {
            std::ptr::copy_nonoverlapping(
                name_bytes.as_ptr(),
                scratch.add(offset),
                name_bytes.len(),
            );
        }
        offset += name_bytes.len();
    }

    Ok(offset)
}

#[cfg(all(unix, test))]
fn deserialize_lv2_note_names(
    scratch: *const u8,
    size: usize,
) -> Result<std::collections::HashMap<u8, String>, String> {
    if size < 4 {
        return Err("scratch too small for LV2 note names".to_string());
    }
    let mut offset = 0usize;
    let count = unsafe { std::ptr::read_unaligned(scratch.add(offset) as *const u32) } as usize;
    offset += 4;

    let mut note_names = std::collections::HashMap::with_capacity(count);
    for _ in 0..count {
        if offset + 4 > size {
            return Err("scratch underflow".to_string());
        }
        let note = unsafe { std::ptr::read_unaligned(scratch.add(offset) as *const u32) } as u8;
        offset += 4;

        if offset + 4 > size {
            return Err("scratch underflow".to_string());
        }
        let name_len =
            unsafe { std::ptr::read_unaligned(scratch.add(offset) as *const u32) } as usize;
        offset += 4;
        if offset + name_len > size {
            return Err("scratch underflow".to_string());
        }
        let mut name_bytes = vec![0u8; name_len];
        unsafe {
            std::ptr::copy_nonoverlapping(scratch.add(offset), name_bytes.as_mut_ptr(), name_len);
        }
        offset += name_len;
        let name = String::from_utf8(name_bytes).map_err(|e| e.to_string())?;
        note_names.insert(note, name);
    }

    Ok(note_names)
}

#[cfg(all(unix, test))]
fn deserialize_lv2_control_ports(
    scratch: *const u8,
    size: usize,
) -> Result<Vec<crate::lv2::Lv2ControlPortInfo>, String> {
    if size < 4 {
        return Err("scratch too small for LV2 control ports".to_string());
    }
    let mut offset = 0usize;

    let count = unsafe { std::ptr::read_unaligned(scratch.add(offset) as *const u32) } as usize;
    offset += 4;

    let mut ports = Vec::with_capacity(count);
    for _ in 0..count {
        if offset + 4 > size {
            return Err("scratch underflow".to_string());
        }
        let index = unsafe { std::ptr::read_unaligned(scratch.add(offset) as *const u32) };
        offset += 4;

        if offset + 4 > size {
            return Err("scratch underflow".to_string());
        }
        let name_len =
            unsafe { std::ptr::read_unaligned(scratch.add(offset) as *const u32) } as usize;
        offset += 4;
        if offset + name_len > size {
            return Err("scratch underflow".to_string());
        }
        let mut name_bytes = vec![0u8; name_len];
        unsafe {
            std::ptr::copy_nonoverlapping(scratch.add(offset), name_bytes.as_mut_ptr(), name_len);
        }
        offset += name_len;
        let name = String::from_utf8(name_bytes).map_err(|e| e.to_string())?;

        if offset + 12 > size {
            return Err("scratch underflow".to_string());
        }
        let min =
            f32::from_bits(unsafe { std::ptr::read_unaligned(scratch.add(offset) as *const u32) });
        let max = f32::from_bits(unsafe {
            std::ptr::read_unaligned(scratch.add(offset + 4) as *const u32)
        });
        let value = f32::from_bits(unsafe {
            std::ptr::read_unaligned(scratch.add(offset + 8) as *const u32)
        });
        offset += 12;

        ports.push(crate::lv2::Lv2ControlPortInfo {
            index,
            name,
            min,
            max,
            value,
        });
    }

    Ok(ports)
}

#[cfg(unix)]
pub fn run_lv2(
    plugin_uri: &str,
    mapping: ShmMapping,
    events: EventPair,
    _instance_id: &str,
    sample_rate: f64,
    buffer_size: usize,
) {
    let header = unsafe { header_ref(mapping.as_ptr()) };
    let ptr = mapping.as_ptr();

    match plugin_uri {
        "__test__" => {
            let scratch = unsafe { scratch_ptr(mapping.as_ptr()) };
            unsafe {
                std::ptr::write_unaligned(scratch as *mut u32, 0xDEADBEEF);
            }
            header.ready.store(1, Ordering::Release);
            return;
        }
        "__crash__" => {
            header.ready.store(1, Ordering::Release);
            std::process::exit(1);
        }
        "__hang__" => {
            header.ready.store(1, Ordering::Release);
            loop {
                std::thread::sleep(Duration::from_secs(60));
            }
        }
        _ => {}
    }

    let mut processor = match crate::lv2::Lv2Processor::new(sample_rate, buffer_size, plugin_uri) {
        Ok(p) => p,
        Err(_e) => {
            return;
        }
    };

    unsafe {
        maolan_plugin_protocol::protocol::write_plugin_name_to_scratch(
            mapping.as_ptr(),
            processor.name(),
        );
    }

    header
        .midi_in_port_count
        .store(processor.midi_input_count() as u32, Ordering::Release);
    header
        .midi_out_port_count
        .store(processor.midi_output_count() as u32, Ordering::Release);
    unsafe {
        latency_samples_atomic(ptr).store(processor.latency_samples(), Ordering::Release);
    }
    header.ready.store(1, Ordering::Release);

    let mut lv2_param_cache = HashMap::new();
    refresh_lv2_param_cache(&processor, &mut lv2_param_cache);
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut lv2_gui_window: Option<Lv2GuiWindow> = None;

    loop {
        if header.shutdown_request.load(Ordering::Acquire) != 0 {
            break;
        }

        let req = header.request_type.load(Ordering::Acquire);
        if req != 0 {
            let scratch = unsafe { scratch_ptr(ptr) };
            let result = match req {
                1 => {
                    tracing::info!("LV2 host: received state save request");
                    let state = processor.snapshot_state();
                    match serialize_lv2_state(scratch, &state) {
                        Ok(size) => {
                            header.scratch_size.store(size as u32, Ordering::Release);
                            Ok(())
                        }
                        Err(e) => Err(e),
                    }
                }
                2 => {
                    let size = header.scratch_size.load(Ordering::Acquire) as usize;
                    match deserialize_lv2_state(scratch, size) {
                        Ok(state) => {
                            let result = processor.restore_state(&state);
                            if result.is_ok() {
                                refresh_lv2_param_cache(&processor, &mut lv2_param_cache);
                            }
                            result
                        }
                        Err(e) => Err(e),
                    }
                }
                3 => {
                    #[cfg(all(unix, not(target_os = "macos")))]
                    {
                        if let Some(window) = &lv2_gui_window {
                            window.show();
                            window.send_control_ports(&processor);
                            Ok(())
                        } else {
                            create_lv2_gui(&processor, plugin_uri, ptr).map(|window| {
                                lv2_gui_window = Some(window);
                            })
                        }
                    }
                    #[cfg(any(not(unix), target_os = "macos"))]
                    {
                        Err("LV2 GUI is only supported on X11 hosts".to_string())
                    }
                }
                4 => {
                    #[cfg(all(unix, not(target_os = "macos")))]
                    {
                        if let Some(window) = &lv2_gui_window {
                            window.hide();
                        }
                    }
                    Ok(())
                }
                5 => {
                    std::sync::atomic::fence(Ordering::SeqCst);
                    let dir = unsafe { read_resource_directory_from_scratch(ptr) };
                    match dir {
                        Some(dir) => {
                            processor.set_state_base_dir(std::path::PathBuf::from(dir));
                            Ok(())
                        }
                        None => Err("Invalid resource directory in scratch".to_string()),
                    }
                }
                maolan_plugin_protocol::protocol::REQUEST_LV2_CONTROL_PORTS => {
                    tracing::info!("LV2 host: received control port request");
                    let ports = processor.control_ports_with_values();
                    tracing::info!(count = ports.len(), "LV2 host: got control ports");
                    match serialize_lv2_control_ports(scratch, &ports) {
                        Ok(size) => {
                            header.scratch_size.store(size as u32, Ordering::Release);
                            Ok(())
                        }
                        Err(e) => Err(e),
                    }
                }
                maolan_plugin_protocol::protocol::REQUEST_LV2_MIDNAM => {
                    tracing::info!("LV2 host: received midnam request");
                    let note_names = processor.midnam_note_names();
                    tracing::info!(count = note_names.len(), "LV2 host: got midnam note names");
                    match serialize_lv2_note_names(scratch, &note_names) {
                        Ok(size) => {
                            header.scratch_size.store(size as u32, Ordering::Release);
                            Ok(())
                        }
                        Err(e) => Err(e),
                    }
                }
                _ => Err(format!("Unknown request type: {}", req)),
            };
            header
                .request_status
                .store(if result.is_ok() { 1 } else { 2 }, Ordering::Release);

            if matches!(
                req,
                1 | 2
                    | 5
                    | maolan_plugin_protocol::protocol::REQUEST_LV2_CONTROL_PORTS
                    | maolan_plugin_protocol::protocol::REQUEST_LV2_MIDNAM
            ) {
                let _ = events.signal_daw();
            }
            header.request_type.store(0, Ordering::Release);
            continue;
        }

        #[cfg(windows)]
        let wait_result = events.wait_daw_with_message_pump(Duration::from_millis(100));
        #[cfg(not(windows))]
        let wait_result = events.wait_daw(Duration::from_millis(100));
        match wait_result {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                #[cfg(all(unix, not(target_os = "macos")))]
                pump_lv2_gui(
                    &mut lv2_gui_window,
                    &mut processor,
                    ptr,
                    &mut lv2_param_cache,
                );
                continue;
            }
            Err(_e) => {
                break;
            }
        }

        let block_size = header.block_size.load(Ordering::Acquire) as usize;
        let num_in = header.num_input_channels.load(Ordering::Acquire) as usize;
        let num_out = header.num_output_channels.load(Ordering::Acquire) as usize;

        if block_size == 0 || block_size > MAX_BLOCK_SIZE {
            let _ = events.signal_daw();
            continue;
        }

        apply_lv2_param_ring(&mut processor, ptr);
        #[cfg(all(unix, not(target_os = "macos")))]
        pump_lv2_gui(
            &mut lv2_gui_window,
            &mut processor,
            ptr,
            &mut lv2_param_cache,
        );

        let transport = read_lv2_transport(ptr);

        let inputs = processor.audio_inputs();
        for (ch, input) in inputs.iter().enumerate().take(num_in) {
            let src = unsafe { audio_channel_ptr(ptr, ch, 0) };
            let dst = input.buffer.lock();
            let len = block_size.min(dst.len());
            unsafe {
                std::ptr::copy_nonoverlapping(src, dst.as_mut_ptr(), len);
            }
            *input.finished.lock() = true;
        }

        let midi_per_port: Vec<Vec<crate::util::MidiEvent>> = if processor.midi_input_count() > 0 {
            (0..processor.midi_input_count())
                .map(|port| drain_midi_ring(ptr, port))
                .collect()
        } else {
            vec![]
        };

        let midi_out = processor.process_with_audio_io(block_size, &midi_per_port, transport);
        unsafe {
            latency_samples_atomic(ptr).store(processor.latency_samples(), Ordering::Release);
        }

        write_lv2_echo_ring(&processor, ptr, &mut lv2_param_cache);

        for (port, events) in midi_out.iter().enumerate() {
            write_midi_out_ring(ptr, port, events);
        }

        let outputs = processor.audio_outputs();
        for (ch, output) in outputs.iter().enumerate().take(num_out) {
            let src = output.buffer.lock();
            let dst = unsafe { audio_channel_ptr(ptr, ch, 1) };
            let len = block_size.min(src.len());
            unsafe {
                std::ptr::copy_nonoverlapping(src.as_ptr(), dst, len);
            }
        }

        if let Err(_e) = events.signal_daw() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use super::*;

    #[test]
    #[cfg(unix)]
    fn lv2_state_serialization_roundtrip() {
        let state = Lv2PluginState {
            port_values: vec![
                Lv2StatePortValue {
                    index: 0,
                    value: 0.5,
                },
                Lv2StatePortValue {
                    index: 1,
                    value: 1.0,
                },
            ],
            properties: vec![Lv2StateProperty {
                key_uri: "http://example.com/key".to_string(),
                type_uri: "http://example.com/type".to_string(),
                flags: 0,
                value: vec![1, 2, 3],
            }],
        };
        let mut scratch = vec![0u8; SCRATCH_SIZE];
        let size =
            serialize_lv2_state(scratch.as_mut_ptr(), &state).expect("serialize should succeed");
        assert!(size > 0);
        assert!(size < SCRATCH_SIZE);

        let decoded =
            deserialize_lv2_state(scratch.as_ptr(), size).expect("deserialize should succeed");
        assert_eq!(decoded.port_values.len(), state.port_values.len());
        assert_eq!(decoded.port_values[0].index, state.port_values[0].index);
        assert_eq!(decoded.port_values[0].value, state.port_values[0].value);
        assert_eq!(decoded.properties.len(), state.properties.len());
        assert_eq!(decoded.properties[0].key_uri, state.properties[0].key_uri);
        assert_eq!(decoded.properties[0].type_uri, state.properties[0].type_uri);
        assert_eq!(decoded.properties[0].flags, state.properties[0].flags);
        assert_eq!(decoded.properties[0].value, state.properties[0].value);
    }

    #[test]
    #[cfg(unix)]
    fn lv2_control_ports_serialization_roundtrip() {
        let ports = vec![
            crate::lv2::Lv2ControlPortInfo {
                index: 0,
                name: "Gain".to_string(),
                min: 0.0,
                max: 1.0,
                value: 0.75,
            },
            crate::lv2::Lv2ControlPortInfo {
                index: 1,
                name: "Frequency".to_string(),
                min: 20.0,
                max: 20000.0,
                value: 1000.0,
            },
        ];
        let mut scratch = vec![0u8; SCRATCH_SIZE];
        let size = serialize_lv2_control_ports(scratch.as_mut_ptr(), &ports)
            .expect("serialize should succeed");
        assert!(size > 0);
        assert!(size < SCRATCH_SIZE);

        let decoded = deserialize_lv2_control_ports(scratch.as_ptr(), size)
            .expect("deserialize should succeed");
        assert_eq!(decoded.len(), ports.len());
        assert_eq!(decoded[0].index, ports[0].index);
        assert_eq!(decoded[0].name, ports[0].name);
        assert_eq!(decoded[0].min, ports[0].min);
        assert_eq!(decoded[0].max, ports[0].max);
        assert_eq!(decoded[0].value, ports[0].value);
        assert_eq!(decoded[1].index, ports[1].index);
        assert_eq!(decoded[1].name, ports[1].name);
        assert_eq!(decoded[1].min, ports[1].min);
        assert_eq!(decoded[1].max, ports[1].max);
        assert_eq!(decoded[1].value, ports[1].value);
    }

    #[test]
    #[cfg(unix)]
    fn lv2_note_names_serialization_roundtrip() {
        let note_names = std::collections::HashMap::from([
            (36_u8, "Kick".to_string()),
            (38_u8, "Snare".to_string()),
            (42_u8, "Closed Hi-Hat".to_string()),
        ]);
        let mut scratch = vec![0u8; SCRATCH_SIZE];
        let size = serialize_lv2_note_names(scratch.as_mut_ptr(), &note_names)
            .expect("serialize should succeed");
        assert!(size > 0);
        assert!(size < SCRATCH_SIZE);

        let decoded =
            deserialize_lv2_note_names(scratch.as_ptr(), size).expect("deserialize should succeed");
        assert_eq!(decoded.len(), note_names.len());
        for (note, name) in &note_names {
            assert_eq!(decoded.get(note), Some(name));
        }
    }
}
