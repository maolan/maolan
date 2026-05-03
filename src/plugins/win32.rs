use maolan_engine::plugins::vst3::interfaces::{PluginFactory, pump_host_run_loop};
use maolan_engine::plugins::vst3::{MemoryStream, ibstream_ptr};
use std::ffi::{OsStr, c_void};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicU32, Ordering};
use vst3::Interface;
use vst3::Steinberg::IPlugViewTrait;
use vst3::Steinberg::Vst::IComponentTrait;
use vst3::Steinberg::Vst::{IEditControllerTrait, ViewType};
use vst3::Steinberg::kResultTrue;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::UpdateWindow;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, CreateWindowExW,
    DefWindowProcW, DestroyWindow, DispatchMessageW, GWLP_USERDATA, GetClientRect,
    GetWindowLongPtrW, IDC_ARROW, LoadCursorW, MSG, MoveWindow, PM_REMOVE, PeekMessageW,
    PostQuitMessage, RegisterClassW, SW_SHOW, SWP_NOZORDER, SetWindowLongPtrW, SetWindowPos,
    ShowWindow, TranslateMessage, WINDOW_EX_STYLE, WM_CLOSE, WM_DESTROY, WM_NCCREATE, WM_SETFOCUS,
    WM_SIZE, WNDCLASSW, WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_OVERLAPPEDWINDOW,
    WS_VISIBLE,
};

const HOST_WINDOW_CLASS: &str = "MaolanVst3HostWindow";
const EMBED_WINDOW_CLASS: &str = "MaolanVst3EmbedWindow";

#[repr(C)]
struct HostPlugFrame {
    iface: vst3::Steinberg::IPlugFrame,
    ref_count: AtomicU32,
    window: HWND,
    embed_window: HWND,
}

impl HostPlugFrame {
    fn new(window: HWND, embed_window: HWND) -> Self {
        Self {
            iface: vst3::Steinberg::IPlugFrame {
                vtbl: &HOST_PLUG_FRAME_VTBL,
            },
            ref_count: AtomicU32::new(1),
            window,
            embed_window,
        }
    }
}

struct WindowState {
    view: *mut vst3::Steinberg::IPlugView,
    embed_window: HWND,
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
        resize_host_windows((*frame).window, (*frame).embed_window, width, height);
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

fn wide_null(s: &str) -> Vec<u16> {
    OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

unsafe fn resize_host_windows(window: HWND, embed_window: HWND, width: i32, height: i32) {
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: width,
        bottom: height,
    };
    let _ = unsafe { AdjustWindowRectEx(&mut rect, WS_OVERLAPPEDWINDOW, 0, 0) };
    let outer_width = rect.right - rect.left;
    let outer_height = rect.bottom - rect.top;
    let _ = unsafe {
        SetWindowPos(
            window,
            null_mut(),
            0,
            0,
            outer_width,
            outer_height,
            SWP_NOZORDER,
        )
    };
    let _ = unsafe { MoveWindow(embed_window, 0, 0, width, height, 1) };
}

unsafe extern "system" fn host_window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_NCCREATE {
        let create = lparam as *const CREATESTRUCTW;
        if !create.is_null() {
            let state_ptr = unsafe { (*create).lpCreateParams } as *mut WindowState;
            let _ = unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize) };
        }
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    }

    let state_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut WindowState;

    match msg {
        WM_SETFOCUS => {
            if !state_ptr.is_null() {
                let view = unsafe { (*state_ptr).view };
                if !view.is_null() {
                    let _ = unsafe { ((*(*view).vtbl).onFocus)(view, 1) };
                }
            }
            0
        }
        WM_SIZE => {
            if !state_ptr.is_null() {
                let mut rect = RECT::default();
                let _ = unsafe { GetClientRect(hwnd, &mut rect) };
                let width = (rect.right - rect.left).max(1);
                let height = (rect.bottom - rect.top).max(1);
                let embed = unsafe { (*state_ptr).embed_window };
                let _ = unsafe { MoveWindow(embed, 0, 0, width, height, 1) };
                let view = unsafe { (*state_ptr).view };
                if !view.is_null() {
                    let mut view_rect = vst3::Steinberg::ViewRect {
                        left: 0,
                        top: 0,
                        right: width,
                        bottom: height,
                    };
                    let _ = unsafe { ((*(*view).vtbl).onSize)(view, &mut view_rect) };
                }
            }
            0
        }
        WM_CLOSE => {
            unsafe { DestroyWindow(hwnd) };
            0
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn register_window_class(
    name: &str,
    proc: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
) {
    let class_name = wide_null(name);
    let hinstance = unsafe { GetModuleHandleW(null_mut()) };
    let wnd_class = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(proc),
        hInstance: hinstance,
        lpszClassName: class_name.as_ptr(),
        hCursor: unsafe { LoadCursorW(null_mut(), IDC_ARROW) },
        ..Default::default()
    };
    unsafe {
        let _ = RegisterClassW(&wnd_class);
    }
}

unsafe fn create_host_windows(
    title: &str,
    width: i32,
    height: i32,
    state: *mut WindowState,
) -> Result<(HWND, HWND), String> {
    register_window_class(HOST_WINDOW_CLASS, host_window_proc);
    register_window_class(EMBED_WINDOW_CLASS, DefWindowProcW);

    let hinstance = unsafe { GetModuleHandleW(null_mut()) };
    let title_w = wide_null(title);
    let host_class = wide_null(HOST_WINDOW_CLASS);
    let embed_class = wide_null(EMBED_WINDOW_CLASS);

    let mut rect = RECT {
        left: 0,
        top: 0,
        right: width,
        bottom: height,
    };
    let _ = unsafe { AdjustWindowRectEx(&mut rect, WS_OVERLAPPEDWINDOW, 0, 0 as WINDOW_EX_STYLE) };

    let window = unsafe {
        CreateWindowExW(
            0,
            host_class.as_ptr(),
            title_w.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            rect.right - rect.left,
            rect.bottom - rect.top,
            null_mut(),
            null_mut(),
            hinstance,
            state.cast::<c_void>(),
        )
    };
    if window.is_null() {
        return Err("Failed to create Win32 host window for VST3 editor".to_string());
    }

    let embed_window = unsafe {
        CreateWindowExW(
            0,
            embed_class.as_ptr(),
            wide_null("").as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
            0,
            0,
            width,
            height,
            window,
            null_mut(),
            hinstance,
            null_mut(),
        )
    };
    if embed_window.is_null() {
        unsafe {
            let _ = DestroyWindow(window);
        }
        return Err("Failed to create Win32 embed window for VST3 editor".to_string());
    }

    Ok((window, embed_window))
}

#[allow(clippy::too_many_arguments)]
pub fn open_vst3_editor_blocking(
    plugin_path: &str,
    plugin_name: &str,
    plugin_id: &str,
    _sample_rate_hz: f64,
    _block_size: usize,
    _audio_inputs: usize,
    _audio_outputs: usize,
    state: Option<maolan_engine::vst3::Vst3PluginState>,
) -> Result<Option<maolan_engine::vst3::Vst3PluginState>, String> {
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

    if let Some(snapshot) = state.as_ref()
        && !snapshot.component_state.is_empty()
    {
        let comp_stream =
            vst3::ComWrapper::new(MemoryStream::from_bytes(&snapshot.component_state));
        let _ = unsafe {
            instance
                .component
                .setState(ibstream_ptr(&comp_stream) as *mut _)
        };
    }
    if let Some(snapshot) = state.as_ref()
        && !snapshot.controller_state.is_empty()
        && let Some(controller) = instance.edit_controller.as_ref()
    {
        let ctrl_stream =
            vst3::ComWrapper::new(MemoryStream::from_bytes(&snapshot.controller_state));
        let _ = unsafe { controller.setState(ibstream_ptr(&ctrl_stream) as *mut _) };
    }

    let controller = instance
        .edit_controller
        .clone()
        .ok_or("VST3 plugin has no edit controller")?;
    let title = if plugin_name.is_empty() {
        class_info.name
    } else {
        plugin_name.to_string()
    };
    let result = run_vst3_win32_editor(controller, title);
    let state = if result.is_ok() {
        let comp_stream = vst3::ComWrapper::new(MemoryStream::new());
        unsafe {
            let snapshot_result = instance
                .component
                .getState(ibstream_ptr(&comp_stream) as *mut _);
            if snapshot_result != vst3::Steinberg::kResultOk {
                return Err("Failed to get component state".to_string());
            }
        }

        let ctrl_stream = vst3::ComWrapper::new(MemoryStream::new());
        if let Some(controller) = &instance.edit_controller {
            unsafe {
                controller.getState(ibstream_ptr(&ctrl_stream) as *mut _);
            }
        }

        Some(maolan_engine::vst3::Vst3PluginState {
            plugin_id: plugin_id.to_string(),
            component_state: comp_stream.bytes(),
            controller_state: ctrl_stream.bytes(),
        })
    } else {
        None
    };
    let _ = instance.terminate();
    result?;
    Ok(state)
}

fn run_vst3_win32_editor(
    controller: vst3::ComPtr<vst3::Steinberg::Vst::IEditController>,
    title: String,
) -> Result<(), String> {
    let view_ptr = unsafe { controller.createView(ViewType::kEditor) };
    if view_ptr.is_null() {
        return Err("VST3 plugin does not expose an editor view".to_string());
    }
    let view = unsafe { vst3::ComPtr::from_raw(view_ptr) }
        .ok_or("Failed to manage VST3 editor view pointer")?;
    let hwnd_support = unsafe { view.isPlatformTypeSupported(vst3::Steinberg::kPlatformTypeHWND) };
    if hwnd_support != kResultTrue && hwnd_support != vst3::Steinberg::kResultOk {
        return Err("VST3 editor does not support Win32 HWND embedding".to_string());
    }

    let mut rect = vst3::Steinberg::ViewRect {
        left: 0,
        top: 0,
        right: 900,
        bottom: 600,
    };
    let _ = unsafe { view.getSize(&mut rect) };
    let width = (rect.right - rect.left).max(320);
    let height = (rect.bottom - rect.top).max(240);

    let mut window_state = Box::new(WindowState {
        view: view.as_ptr(),
        embed_window: null_mut(),
    });
    let state_ptr = &mut *window_state as *mut WindowState;
    let (window, embed_window) = unsafe { create_host_windows(&title, width, height, state_ptr)? };
    window_state.embed_window = embed_window;

    let mut frame = Box::new(HostPlugFrame::new(window, embed_window));
    let frame_ptr = &mut frame.iface as *mut vst3::Steinberg::IPlugFrame;

    let _ = unsafe { view.setFrame(frame_ptr) };
    let attached = unsafe {
        view.attached(
            embed_window as usize as *mut c_void,
            vst3::Steinberg::kPlatformTypeHWND,
        )
    };
    if attached != vst3::Steinberg::kResultOk && attached != vst3::Steinberg::kResultTrue {
        unsafe {
            let _ = DestroyWindow(window);
        }
        return Err(format!("VST3 editor attach failed (result: {attached})"));
    }

    rect.left = 0;
    rect.top = 0;
    rect.right = width;
    rect.bottom = height;
    let _ = unsafe { view.onSize(&mut rect) };
    let _ = unsafe { view.onFocus(1) };
    unsafe {
        resize_host_windows(window, embed_window, width, height);
        ShowWindow(window, SW_SHOW);
        UpdateWindow(window);
    }

    let mut msg = MSG::default();
    loop {
        pump_host_run_loop();
        let has_message = unsafe { PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) };
        if has_message == 0 {
            std::thread::sleep(std::time::Duration::from_millis(16));
            continue;
        }
        if msg.message == windows_sys::Win32::UI::WindowsAndMessaging::WM_QUIT {
            break;
        }
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    let _ = unsafe { view.onFocus(0) };
    let _ = unsafe { view.setFrame(std::ptr::null_mut()) };
    let _ = unsafe { view.removed() };
    unsafe {
        let _ = SetWindowLongPtrW(window, GWLP_USERDATA, 0isize);
        let _ = DestroyWindow(window);
    }
    drop(frame);
    drop(window_state);
    Ok(())
}

pub fn open_editor_with_processor(
    processor: std::sync::Arc<maolan_engine::vst3::Vst3Processor>,
    title: String,
) -> Result<Option<maolan_engine::vst3::Vst3PluginState>, String> {
    let result = run_vst3_win32_editor_with_processor(processor, title);
    result.map(|_| None)
}

fn run_vst3_win32_editor_with_processor(
    processor: std::sync::Arc<maolan_engine::vst3::Vst3Processor>,
    title: String,
) -> Result<(), String> {
    processor.ui_begin_session();

    let platform_type = "HWND";
    if let Err(e) = processor.gui_create(platform_type) {
        processor.ui_end_session();
        return Err(e);
    }

    let (width, height) = match processor.gui_get_size() {
        Ok((w, h)) => (w.max(320), h.max(240)),
        Err(_) => (900, 600),
    };

    let mut window_state = Box::new(WindowState {
        view: std::ptr::null_mut(),
        embed_window: null_mut(),
    });
    let state_ptr = &mut *window_state as *mut WindowState;
    let (window, embed_window) = unsafe { create_host_windows(&title, width, height, state_ptr)? };
    window_state.embed_window = embed_window;

    if let Err(e) = processor.gui_set_parent(embed_window as usize, platform_type) {
        unsafe {
            let _ = DestroyWindow(window);
        }
        processor.gui_destroy();
        processor.ui_end_session();
        return Err(e);
    }

    let _ = processor.gui_on_size(width, height);

    if let Err(e) = processor.gui_show() {
        unsafe {
            let _ = DestroyWindow(window);
        }
        processor.gui_destroy();
        processor.ui_end_session();
        return Err(e);
    }

    unsafe {
        resize_host_windows(window, embed_window, width, height);
        ShowWindow(window, SW_SHOW);
        UpdateWindow(window);
    }

    let mut msg = MSG::default();
    loop {
        processor.gui_on_main_thread();
        if processor.ui_should_close() {
            break;
        }
        let has_message = unsafe { PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) };
        if has_message == 0 {
            std::thread::sleep(std::time::Duration::from_millis(16));
            continue;
        }
        if msg.message == windows_sys::Win32::UI::WindowsAndMessaging::WM_QUIT {
            break;
        }
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    processor.gui_hide();
    processor.gui_destroy();
    unsafe {
        let _ = SetWindowLongPtrW(window, GWLP_USERDATA, 0isize);
        let _ = DestroyWindow(window);
    }
    drop(window_state);
    processor.ui_end_session();
    Ok(())
}
