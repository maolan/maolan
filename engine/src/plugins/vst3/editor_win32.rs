use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};
use vst3::ComPtr;
use vst3::Interface;
use vst3::Steinberg::IPlugViewTrait;
use vst3::Steinberg::Vst::{IEditController, IEditControllerTrait, ViewType};
use vst3::Steinberg::kResultTrue;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DestroyWindow,
    DispatchMessageW, GetClientRect, IDC_ARROW, IsWindow, LoadCursorW, MSG, MoveWindow, PM_REMOVE,
    PeekMessageW, PostQuitMessage, RegisterClassW, SW_SHOWDEFAULT, ShowWindow, TranslateMessage,
    WM_CLOSE, WM_DESTROY, WM_QUIT, WNDCLASSW, WS_CHILD, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};

#[repr(C)]
struct HostPlugFrame {
    iface: vst3::Steinberg::IPlugFrame,
    ref_count: AtomicU32,
    window: HWND,
}

impl HostPlugFrame {
    fn new(window: HWND) -> Self {
        Self {
            iface: vst3::Steinberg::IPlugFrame {
                vtbl: &HOST_PLUG_FRAME_VTBL,
            },
            ref_count: AtomicU32::new(1),
            window,
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
        if !(*frame).window.is_null() {
            let _ = MoveWindow((*frame).window, 0, 0, width, height, 1);
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

unsafe extern "system" fn host_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CLOSE => {
            unsafe {
                DestroyWindow(hwnd);
            }
            0
        }
        WM_DESTROY => {
            unsafe {
                PostQuitMessage(0);
            }
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn open_editor_blocking(
    controller: ComPtr<IEditController>,
    title: &str,
) -> Result<(), String> {
    let coinit_hr = unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) };
    let did_init_com = coinit_hr == 0 || coinit_hr == 1;

    let result = (|| {
        let view_ptr = unsafe { controller.createView(ViewType::kEditor) };
        if view_ptr.is_null() {
            return Err("VST3 plugin does not expose an editor view".to_string());
        }
        let view = unsafe { ComPtr::from_raw(view_ptr) }
            .ok_or("Failed to manage VST3 editor view pointer")?;
        let hwnd_supported =
            unsafe { view.isPlatformTypeSupported(vst3::Steinberg::kPlatformTypeHWND) };
        if hwnd_supported != kResultTrue && hwnd_supported != vst3::Steinberg::kResultOk {
            return Err("VST3 editor does not support Win32 embedding".to_string());
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

        let class_name = to_wide("MaolanVst3SingleInstanceHostWindow");
        let title_w = to_wide(title);
        let hinstance = unsafe { GetModuleHandleW(std::ptr::null()) };
        let wnd_class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(host_wnd_proc),
            hInstance: hinstance,
            lpszClassName: class_name.as_ptr(),
            hCursor: unsafe { LoadCursorW(std::ptr::null_mut(), IDC_ARROW) },
            ..unsafe { std::mem::zeroed() }
        };
        unsafe {
            let _ = RegisterClassW(&wnd_class);
        }

        let window = unsafe {
            CreateWindowExW(
                0,
                class_name.as_ptr(),
                title_w.as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                width + 20,
                height + 40,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                hinstance,
                std::ptr::null(),
            )
        };
        if window.is_null() {
            return Err("Failed to create Win32 host window for VST3 editor".to_string());
        }
        let static_class = to_wide("STATIC");
        let embed_window = unsafe {
            CreateWindowExW(
                0,
                static_class.as_ptr(),
                std::ptr::null(),
                WS_CHILD | WS_VISIBLE,
                0,
                0,
                width,
                height,
                window,
                std::ptr::null_mut(),
                hinstance,
                std::ptr::null(),
            )
        };
        if embed_window.is_null() {
            unsafe {
                let _ = DestroyWindow(window);
            }
            return Err("Failed to create Win32 embed window for VST3 editor".to_string());
        }
        unsafe {
            let _ = ShowWindow(window, SW_SHOWDEFAULT);
            let _ = MoveWindow(embed_window, 0, 0, width, height, 1);
        }

        let attached = unsafe {
            view.attached(
                embed_window.cast::<c_void>(),
                vst3::Steinberg::kPlatformTypeHWND,
            )
        };
        if attached != vst3::Steinberg::kResultOk && attached != vst3::Steinberg::kResultTrue {
            unsafe {
                let _ = DestroyWindow(window);
            }
            return Err(format!("VST3 editor attach failed (result: {attached})"));
        }

        let mut frame = Box::new(HostPlugFrame::new(embed_window));
        let frame_ptr = &mut frame.iface as *mut vst3::Steinberg::IPlugFrame;
        let _ = unsafe { view.setFrame(frame_ptr) };

        let mut last_w = width;
        let mut last_h = height;
        let mut running = true;
        while running {
            let mut msg: MSG = unsafe { std::mem::zeroed() };
            while unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) } != 0 {
                if msg.message == WM_QUIT {
                    running = false;
                    break;
                }
                unsafe {
                    let _ = TranslateMessage(&msg);
                    let _ = DispatchMessageW(&msg);
                }
            }
            if !running || unsafe { IsWindow(window) } == 0 {
                break;
            }
            let mut client: RECT = unsafe { std::mem::zeroed() };
            unsafe {
                let _ = GetClientRect(window, &mut client);
            }
            let w = (client.right - client.left).max(1);
            let h = (client.bottom - client.top).max(1);
            unsafe {
                let _ = MoveWindow(embed_window, 0, 0, w, h, 1);
            }
            if w != last_w || h != last_h {
                last_w = w;
                last_h = h;
            }
            std::thread::sleep(std::time::Duration::from_millis(16));
        }

        let _ = unsafe { view.setFrame(std::ptr::null_mut()) };
        let _ = unsafe { view.removed() };
        unsafe {
            if IsWindow(window) != 0 {
                let _ = DestroyWindow(window);
            }
        }
        Ok(())
    })();

    if did_init_com {
        unsafe { CoUninitialize() };
    }
    result
}
