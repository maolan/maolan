use std::ffi::{OsStr, c_void};
use std::os::windows::ffi::OsStrExt;
use std::ptr::null_mut;
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

struct WindowState {
    view: *mut vst3::Steinberg::IPlugView,
    embed_window: HWND,
}

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
