//! Shared X11 container-window helpers for hosting plugin GUIs.

#[cfg(unix)]
pub mod x11 {
    #[cfg(all(unix, not(target_os = "macos")))]
    use std::os::raw::{c_char, c_uint};
    use std::os::raw::{c_int, c_ulong};
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

    #[cfg(all(unix, not(target_os = "macos")))]
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

    // macOS has no X11/XQuartz, so plugin GUI container windows are unsupported
    // there until a Cocoa parent-window implementation exists. These stubs keep
    // the crate linking; XOpenDisplay always fails, so callers take their
    // existing "failed to open X11 display" error path.
    #[cfg(target_os = "macos")]
    mod imp {
        //! No-op stubs: see the comment on the parent module.
        use super::{Display, Window, XErrorHandler};
        use std::os::raw::{c_char, c_int, c_uint, c_ulong};

        /// # Safety
        /// No-op stub; always returns a null display pointer.
        pub unsafe fn open_display(_display_name: *const c_char) -> *mut Display {
            std::ptr::null_mut()
        }
        /// # Safety
        /// No-op stub; all arguments are ignored.
        pub unsafe fn close_display(_display: *mut Display) -> c_int {
            0
        }
        /// # Safety
        /// No-op stub; all arguments are ignored.
        pub unsafe fn default_screen(_display: *mut Display) -> c_int {
            0
        }
        /// # Safety
        /// No-op stub; all arguments are ignored.
        pub unsafe fn root_window(_display: *mut Display, _screen: c_int) -> Window {
            0
        }
        /// # Safety
        /// No-op stub; all arguments are ignored.
        pub unsafe fn black_pixel(_display: *mut Display, _screen: c_int) -> c_ulong {
            0
        }
        /// # Safety
        /// No-op stub; all arguments are ignored.
        pub unsafe fn white_pixel(_display: *mut Display, _screen: c_int) -> c_ulong {
            0
        }
        /// # Safety
        /// No-op stub; all arguments are ignored.
        pub unsafe fn create_simple_window(
            _display: *mut Display,
            _parent: Window,
            _x: c_int,
            _y: c_int,
            _width: c_uint,
            _height: c_uint,
            _border_width: c_uint,
            _border: c_ulong,
            _background: c_ulong,
        ) -> Window {
            0
        }
        /// # Safety
        /// No-op stub; all arguments are ignored.
        pub unsafe fn store_name(
            _display: *mut Display,
            _w: Window,
            _name: *const c_char,
        ) -> c_int {
            0
        }
        /// # Safety
        /// No-op stub; all arguments are ignored.
        pub unsafe fn map_window(_display: *mut Display, _w: Window) -> c_int {
            0
        }
        /// # Safety
        /// No-op stub; all arguments are ignored.
        pub unsafe fn unmap_window(_display: *mut Display, _w: Window) -> c_int {
            0
        }
        /// # Safety
        /// No-op stub; all arguments are ignored.
        pub unsafe fn destroy_window(_display: *mut Display, _w: Window) -> c_int {
            0
        }
        /// # Safety
        /// No-op stub; all arguments are ignored.
        pub unsafe fn resize_window(
            _display: *mut Display,
            _w: Window,
            _width: c_uint,
            _height: c_uint,
        ) -> c_int {
            0
        }
        /// # Safety
        /// No-op stub; all arguments are ignored.
        pub unsafe fn flush(_display: *mut Display) -> c_int {
            0
        }
        /// # Safety
        /// No-op stub; all arguments are ignored.
        pub unsafe fn set_error_handler(_handler: XErrorHandler) -> XErrorHandler {
            None
        }
    }

    #[cfg(target_os = "macos")]
    pub use imp::black_pixel as XBlackPixel;
    #[cfg(target_os = "macos")]
    pub use imp::close_display as XCloseDisplay;
    #[cfg(target_os = "macos")]
    pub use imp::create_simple_window as XCreateSimpleWindow;
    #[cfg(target_os = "macos")]
    pub use imp::default_screen as XDefaultScreen;
    #[cfg(target_os = "macos")]
    pub use imp::destroy_window as XDestroyWindow;
    #[cfg(target_os = "macos")]
    pub use imp::flush as XFlush;
    #[cfg(target_os = "macos")]
    pub use imp::map_window as XMapWindow;
    #[cfg(target_os = "macos")]
    pub use imp::open_display as XOpenDisplay;
    #[cfg(target_os = "macos")]
    pub use imp::resize_window as XResizeWindow;
    #[cfg(target_os = "macos")]
    pub use imp::root_window as XRootWindow;
    #[cfg(target_os = "macos")]
    pub use imp::set_error_handler as XSetErrorHandler;
    #[cfg(target_os = "macos")]
    pub use imp::store_name as XStoreName;
    #[cfg(target_os = "macos")]
    pub use imp::unmap_window as XUnmapWindow;
    #[cfg(target_os = "macos")]
    pub use imp::white_pixel as XWhitePixel;

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
