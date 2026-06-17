//! Shared Win32 container-window helpers for hosting plugin GUIs.

#[cfg(windows)]
pub mod win32 {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    static CLASS_ATOM: AtomicUsize = AtomicUsize::new(0);

    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    pub fn ensure_class_registered() -> u16 {
        let atom = CLASS_ATOM.load(Ordering::Acquire);
        if atom != 0 {
            return atom as u16;
        }

        let class_name: Vec<u16> = "MaolanPluginContainer\0".encode_utf16().collect();
        let wndclass = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: unsafe { GetModuleHandleW(std::ptr::null()) } as *mut _,
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: (5 + 1) as *mut _,
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };

        let atom = unsafe { RegisterClassExW(&wndclass) };
        if atom == 0 {
            return 0;
        }
        CLASS_ATOM.store(atom as usize, Ordering::Release);
        atom as u16
    }

    pub fn create_container_window(
        parent: HWND,
        title: &str,
        width: i32,
        height: i32,
    ) -> Result<ContainerWindow, String> {
        let atom = ensure_class_registered();
        if atom == 0 {
            return Err("failed to register container window class".to_string());
        }

        let title: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        let style = if parent.is_null() {
            WS_OVERLAPPEDWINDOW
        } else {
            WS_CHILD | WS_VISIBLE
        };

        let hwnd = unsafe {
            CreateWindowExW(
                0,
                atom as *const u16,
                title.as_ptr(),
                style,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                width,
                height,
                parent,
                std::ptr::null_mut(),
                GetModuleHandleW(std::ptr::null()),
                std::ptr::null(),
            )
        };

        if hwnd.is_null() {
            return Err("failed to create container window".to_string());
        }

        Ok(ContainerWindow { hwnd })
    }

    pub struct ContainerWindow {
        pub hwnd: HWND,
    }

    impl Drop for ContainerWindow {
        fn drop(&mut self) {
            unsafe {
                DestroyWindow(self.hwnd);
            }
        }
    }
}
