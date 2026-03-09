use maolan_engine::plugins::vst3::interfaces::PluginFactory;
use maolan_engine::vst3::{MemoryStream, ibstream_ptr};
use std::ffi::c_void;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use vst3::Interface;
use vst3::Steinberg::IPlugViewTrait;
use vst3::Steinberg::IPlugView;
use vst3::Steinberg::Vst::IComponentTrait;
use vst3::Steinberg::Vst::{IEditControllerTrait, ViewType};
use vst3::Steinberg::kResultTrue;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW,
    DestroyWindow, DispatchMessageW, GetClientRect, GetParent, GetWindowRect, IDC_ARROW, IsWindow,
    LoadCursorW, MSG, MoveWindow, PM_REMOVE, PeekMessageW, PostQuitMessage, RegisterClassW,
    SW_SHOWDEFAULT, SetForegroundWindow, ShowWindow, TranslateMessage, WM_CLOSE, WM_DESTROY,
    WM_QUIT, WNDCLASSW, WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_OVERLAPPEDWINDOW,
    WS_VISIBLE,
};

#[repr(C)]
struct HostPlugFrame {
    iface: vst3::Steinberg::IPlugFrame,
    ref_count: AtomicU32,
    embed_window: HWND,
}

impl HostPlugFrame {
    fn new(embed_window: HWND) -> Self {
        Self {
            iface: vst3::Steinberg::IPlugFrame {
                vtbl: &HOST_PLUG_FRAME_VTBL,
            },
            ref_count: AtomicU32::new(1),
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
        if !(*frame).embed_window.is_null() {
            let _ = MoveWindow((*frame).embed_window, 0, 0, width, height, 1);

            let host_window = GetParent((*frame).embed_window);
            if !host_window.is_null() {
                let mut wr: RECT = std::mem::zeroed();
                let mut cr: RECT = std::mem::zeroed();
                let _ = GetWindowRect(host_window, &mut wr);
                let _ = GetClientRect(host_window, &mut cr);
                let outer_w = (wr.right - wr.left).max(1);
                let outer_h = (wr.bottom - wr.top).max(1);
                let client_w = (cr.right - cr.left).max(1);
                let client_h = (cr.bottom - cr.top).max(1);
                let non_client_w = (outer_w - client_w).max(0);
                let non_client_h = (outer_h - client_h).max(0);
                let target_outer_w = width + non_client_w;
                let target_outer_h = height + non_client_h;
                let _ = MoveWindow(
                    host_window,
                    wr.left,
                    wr.top,
                    target_outer_w.max(1),
                    target_outer_h.max(1),
                    1,
                );
            }
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
    plugin_path: &str,
    plugin_name: &str,
    plugin_id: &str,
    sample_rate_hz: f64,
    block_size: usize,
    audio_inputs: usize,
    audio_outputs: usize,
    state: Option<maolan_engine::vst3::Vst3PluginState>,
) -> Result<(), String> {
    let coinit_hr = unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) };
    let did_init_com = coinit_hr == 0 || coinit_hr == 1;

    let result = (|| {
        eprintln!("[vst3-ui] open_editor_blocking: begin");
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
        eprintln!("[vst3-ui] instance created");
        instance.initialize(&factory)?;
        eprintln!("[vst3-ui] instance initialized");
        let (input_buses, output_buses) = instance.audio_bus_counts();
        // Keep editor-host setup conservative: some plugins are sensitive to
        // unrealistic process settings and crash during UI bring-up.
        let setup_sample_rate = if sample_rate_hz.is_finite() && sample_rate_hz > 1.0 {
            sample_rate_hz
        } else {
            48_000.0
        };
        let setup_block_size = block_size.clamp(64, 8192);
        let ui_audio_inputs = if input_buses == 0 {
            0
        } else {
            audio_inputs.max(1)
        };
        let ui_audio_outputs = if output_buses == 0 {
            0
        } else {
            audio_outputs.max(1)
        };
        instance.set_active(true)?;
        instance.setup_processing(
            setup_sample_rate,
            setup_block_size.min(i32::MAX as usize) as i32,
            ui_audio_inputs.min(i32::MAX as usize) as i32,
            ui_audio_outputs.min(i32::MAX as usize) as i32,
        )?;
        if let Some(snapshot) = state.as_ref() {
            if !snapshot.component_state.is_empty() {
                let comp_stream =
                    vst3::ComWrapper::new(MemoryStream::from_bytes(&snapshot.component_state));
                let _ = unsafe {
                    instance
                        .component
                        .setState(ibstream_ptr(&comp_stream) as *mut _)
                };
            }
            if !snapshot.controller_state.is_empty()
                && let Some(controller) = instance.edit_controller.as_ref()
            {
                let ctrl_stream =
                    vst3::ComWrapper::new(MemoryStream::from_bytes(&snapshot.controller_state));
                let _ = unsafe {
                    controller.setState(ibstream_ptr(&ctrl_stream) as *mut _)
                };
            }
        }
        eprintln!(
            "[vst3-ui] processing setup sr={} block={} in={} out={} (requested in={} out={})",
            setup_sample_rate,
            setup_block_size,
            ui_audio_inputs,
            ui_audio_outputs,
            audio_inputs,
            audio_outputs
        );

        let controller = instance
            .edit_controller
            .clone()
            .ok_or("VST3 plugin has no edit controller")?;
        let title = if plugin_name.is_empty() {
            class_info.name
        } else {
            plugin_name.to_string()
        };
        eprintln!("[vst3-ui] opening editor window");
        let result = run_vst3_win32_editor(controller, title);
        eprintln!("[vst3-ui] editor closed, terminating instance");
        instance.stop_processing();
        let _ = instance.set_active(false);
        let _ = instance.terminate();
        result
    })();

    if did_init_com {
        unsafe {
            CoUninitialize();
        }
    }
    result
}

pub fn open_editor_from_handle_blocking(view_handle: usize, title: &str) -> Result<(), String> {
    if view_handle == 0 {
        return Err("VST3 editor view handle is null".to_string());
    }
    let coinit_hr = unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) };
    let did_init_com = coinit_hr == 0 || coinit_hr == 1;

    let result = (|| {
        let view_ptr = view_handle as *mut vst3::Steinberg::IPlugView;
        let view = unsafe { vst3::ComPtr::from_raw(view_ptr) }
            .ok_or("Failed to adopt VST3 editor view handle")?;
        run_vst3_win32_editor_view(view, title.to_string())
    })();

    if did_init_com {
        unsafe {
            CoUninitialize();
        }
    }
    result
}

fn run_vst3_win32_editor(
    controller: vst3::ComPtr<vst3::Steinberg::Vst::IEditController>,
    title: String,
) -> Result<(), String> {
    eprintln!("[vst3-ui] createView");
    let view_ptr = unsafe { controller.createView(ViewType::kEditor) };
    if view_ptr.is_null() {
        return Err("VST3 plugin does not expose an editor view".to_string());
    }
    let view = unsafe { vst3::ComPtr::from_raw(view_ptr) }
        .ok_or("Failed to manage VST3 editor view pointer")?;
    run_vst3_win32_editor_view(view, title)
}

fn run_vst3_win32_editor_view(
    view: vst3::ComPtr<IPlugView>,
    title: String,
) -> Result<(), String> {
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

    let class_name = to_wide("MaolanVst3HostWindow");
    let title_w = to_wide(&title);
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
            WS_OVERLAPPEDWINDOW | WS_VISIBLE | WS_CLIPCHILDREN,
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
            WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS,
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
        let _ = BringWindowToTop(window);
        let _ = SetForegroundWindow(window);
        let _ = MoveWindow(embed_window, 0, 0, width, height, 1);
    }
    for _ in 0..4 {
        let mut msg: MSG = unsafe { std::mem::zeroed() };
        while unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) } != 0 {
            unsafe {
                let _ = TranslateMessage(&msg);
                let _ = DispatchMessageW(&msg);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let mut embed_client: RECT = unsafe { std::mem::zeroed() };
    unsafe {
        let _ = GetClientRect(embed_window, &mut embed_client);
    }
    let embed_w = (embed_client.right - embed_client.left).max(1);
    let embed_h = (embed_client.bottom - embed_client.top).max(1);
    if embed_w != width || embed_h != height {
        unsafe {
            let _ = MoveWindow(embed_window, 0, 0, embed_w, embed_h, 1);
        }
    }

    let attach_parent = embed_window;
    let mut frame = Box::new(HostPlugFrame::new(attach_parent));
    let frame_ptr = &mut frame.iface as *mut vst3::Steinberg::IPlugFrame;
    eprintln!("[vst3-ui] setFrame (pre-attach)");
    let set_frame = unsafe { view.setFrame(frame_ptr) };
    if set_frame != vst3::Steinberg::kResultOk && set_frame != vst3::Steinberg::kResultTrue {
        unsafe {
            let _ = DestroyWindow(window);
        }
        return Err(format!("VST3 editor setFrame failed (result: {set_frame})"));
    }
    eprintln!("[vst3-ui] attached");
    let attached = unsafe {
        view.attached(
            attach_parent.cast::<c_void>(),
            vst3::Steinberg::kPlatformTypeHWND,
        )
    };
    eprintln!("[vst3-ui] attached returned {attached}");
    if attached != vst3::Steinberg::kResultOk && attached != vst3::Steinberg::kResultTrue {
        let _ = unsafe { view.setFrame(std::ptr::null_mut()) };
        unsafe {
            let _ = DestroyWindow(window);
        }
        return Err(format!("VST3 editor attach failed (result: {attached})"));
    }
    let mut initial_rect = vst3::Steinberg::ViewRect {
        left: 0,
        top: 0,
        right: width,
        bottom: height,
    };
    let _ = unsafe { view.onSize(&mut initial_rect) };

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

    eprintln!("[vst3-ui] detaching");
    let _ = unsafe { view.setFrame(std::ptr::null_mut()) };
    let _ = unsafe { view.removed() };
    unsafe {
        if IsWindow(window) != 0 {
            let _ = DestroyWindow(window);
        }
    }
    Ok(())
}
