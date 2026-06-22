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
mod x11_ffi {
    use std::os::raw::{c_char, c_int, c_uint, c_ulong};
    pub type Display = std::ffi::c_void;
    pub type Window = c_ulong;

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
        pub fn XResizeWindow(
            display: *mut Display,
            w: Window,
            width: c_uint,
            height: c_uint,
        ) -> c_int;
        pub fn XFlush(display: *mut Display) -> c_int;
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

    let display = unsafe { x11_ffi::XOpenDisplay(std::ptr::null()) };
    if display.is_null() {
        return Err("VST3 GUI: failed to open X11 display".to_string());
    }

    let screen = unsafe { x11_ffi::XDefaultScreen(display) };
    let root = unsafe { x11_ffi::XRootWindow(display, screen) };
    let black = unsafe { x11_ffi::XBlackPixel(display, screen) };
    let white = unsafe { x11_ffi::XWhitePixel(display, screen) };

    let parent_window = {
        let header = unsafe { header_ref(ptr) };
        let parent = header.parent_window_usize();
        if parent != 0 {
            parent as x11_ffi::Window
        } else {
            root
        }
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
        let parent = header.parent_window_usize();
        if parent != 0 {
            parent as HWND
        } else {
            std::ptr::null_mut()
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

fn drain_midi_ring(ptr: *mut u8) -> Vec<crate::util::MidiEvent> {
    let ring = unsafe {
        let buf = midi_ring_ptr(ptr);
        let (w, r) = midi_indices(ptr);
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

fn write_midi_out_ring(ptr: *mut u8, events: &[crate::util::MidiEvent]) {
    let ring = unsafe {
        let buf = midi_out_ring_ptr(ptr);
        let (w, r) = midi_out_indices(ptr);
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

    match plugin_path {
        "__test__" => {
            let scratch = unsafe { scratch_ptr(mapping.as_ptr()) };
            unsafe {
                std::ptr::write_unaligned(scratch as *mut u32, 0xDEADBEEF);
            }
            return;
        }
        "__crash__" => {
            std::process::exit(1);
        }
        "__hang__" => loop {
            std::thread::sleep(Duration::from_secs(60));
        },
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

    let header = unsafe { header_ref(mapping.as_ptr()) };
    let ptr = mapping.as_ptr();
    let mut vst3_param_cache = HashMap::new();

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
                        Ok(state) => processor.restore_state(&state),
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

        let midi_input = drain_midi_ring(ptr);

        let _midi_output = if processor.midi_input_count() > 0 || processor.midi_output_count() > 0
        {
            processor.process_with_midi(block_size, &midi_input)
        } else {
            processor.process_with_audio_io(block_size);
            Vec::new()
        };

        write_vst3_echo_ring(&processor, ptr, &mut vst3_param_cache);

        write_midi_out_ring(ptr, &_midi_output);

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
pub fn run_lv2(
    plugin_uri: &str,
    mapping: ShmMapping,
    events: EventPair,
    _instance_id: &str,
    sample_rate: f64,
    buffer_size: usize,
) {
    match plugin_uri {
        "__test__" => {
            let scratch = unsafe { scratch_ptr(mapping.as_ptr()) };
            unsafe {
                std::ptr::write_unaligned(scratch as *mut u32, 0xDEADBEEF);
            }
            return;
        }
        "__crash__" => {
            std::process::exit(1);
        }
        "__hang__" => loop {
            std::thread::sleep(Duration::from_secs(60));
        },
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

    let header = unsafe { header_ref(mapping.as_ptr()) };
    let ptr = mapping.as_ptr();
    let mut lv2_param_cache = HashMap::new();

    loop {
        if header.shutdown_request.load(Ordering::Acquire) != 0 {
            break;
        }

        let req = header.request_type.load(Ordering::Acquire);
        if req != 0 {
            let scratch = unsafe { scratch_ptr(ptr) };
            let result = match req {
                1 => {
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
                        Ok(state) => processor.restore_state(&state),
                        Err(e) => Err(e),
                    }
                }
                3 => Err("LV2 GUI not yet supported".to_string()),
                4 => Ok(()),
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
                _ => Err(format!("Unknown request type: {}", req)),
            };
            header
                .request_status
                .store(if result.is_ok() { 1 } else { 2 }, Ordering::Release);

            if matches!(req, 1 | 2 | 5) {
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

        apply_lv2_param_ring(&mut processor, ptr);

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

        let midi_input = drain_midi_ring(ptr);
        let midi_per_port: Vec<Vec<crate::util::MidiEvent>> = if processor.midi_input_count() > 0 {
            vec![midi_input]
        } else {
            vec![]
        };

        let midi_out = processor.process_with_audio_io(block_size, &midi_per_port, transport);

        write_lv2_echo_ring(&processor, ptr, &mut lv2_param_cache);

        let mut all_midi_out = Vec::new();
        for port_events in midi_out {
            all_midi_out.extend(port_events);
        }
        write_midi_out_ring(ptr, &all_midi_out);

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
}
