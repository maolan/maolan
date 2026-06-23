//! Shared X11 container-window helpers for hosting plugin GUIs.

#[cfg(all(unix, not(target_os = "macos")))]
pub mod x11 {
    use std::os::raw::{c_char, c_int, c_uint, c_ulong};
    use std::sync::Once;

    pub type Display = std::ffi::c_void;
    pub type Window = c_ulong;

    #[repr(C)]
    pub struct XErrorEvent {
        _private: [u8; 0],
    }

    pub type XErrorHandler =
        Option<unsafe extern "C" fn(display: *mut Display, event: *mut XErrorEvent) -> c_int>;

    unsafe extern "C" fn ignore_x_error(_display: *mut Display, _event: *mut XErrorEvent) -> c_int {
        0
    }

    fn install_x_error_handler() {
        static INSTALL: Once = Once::new();
        INSTALL.call_once(|| unsafe {
            XSetErrorHandler(Some(ignore_x_error));
        });
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
        pub fn XResizeWindow(
            display: *mut Display,
            w: Window,
            width: c_uint,
            height: c_uint,
        ) -> c_int;
        pub fn XFlush(display: *mut Display) -> c_int;
        pub fn XSetErrorHandler(handler: XErrorHandler) -> XErrorHandler;
    }

    pub struct ContainerWindow {
        display: *mut Display,
        window: Window,
    }

    unsafe impl Send for ContainerWindow {}

    impl ContainerWindow {
        pub fn window(&self) -> Window {
            self.window
        }

        pub fn map(&self) {
            unsafe {
                XMapWindow(self.display, self.window);
                XFlush(self.display);
            }
        }

        pub fn unmap(&self) {
            unsafe {
                XUnmapWindow(self.display, self.window);
                XFlush(self.display);
            }
        }

        pub fn resize(&self, width: u32, height: u32) {
            unsafe {
                XResizeWindow(self.display, self.window, width, height);
                XFlush(self.display);
            }
        }
    }

    impl Drop for ContainerWindow {
        fn drop(&mut self) {
            unsafe {
                XDestroyWindow(self.display, self.window);
                XFlush(self.display);
                XCloseDisplay(self.display);
            }
        }
    }

    pub fn create_container_window(
        display_name: Option<&str>,
        parent: Option<Window>,
        title: &str,
        width: u32,
        height: u32,
    ) -> Result<ContainerWindow, String> {
        install_x_error_handler();

        let display_name_c = display_name.and_then(|s| std::ffi::CString::new(s).ok());
        let display_name_ptr = display_name_c
            .as_ref()
            .map(|s| s.as_ptr())
            .unwrap_or(std::ptr::null());

        let display = unsafe { XOpenDisplay(display_name_ptr) };
        if display.is_null() {
            return Err("failed to open X11 display".to_string());
        }

        let screen = unsafe { XDefaultScreen(display) };
        let root = unsafe { XRootWindow(display, screen) };
        let black = unsafe { XBlackPixel(display, screen) };
        let white = unsafe { XWhitePixel(display, screen) };

        let parent = parent.unwrap_or(root);

        let window =
            unsafe { XCreateSimpleWindow(display, parent, 0, 0, width, height, 1, black, white) };
        if window == 0 {
            unsafe { XCloseDisplay(display) };
            return Err("failed to create X11 container window".to_string());
        }

        if let Ok(cstr) = std::ffi::CString::new(title) {
            unsafe {
                XStoreName(display, window, cstr.as_ptr());
            }
        }

        Ok(ContainerWindow { display, window })
    }
}
