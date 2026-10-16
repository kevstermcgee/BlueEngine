//! Macroquad exposes no desktop focus getter. Native focus must still pause rules.
#[cfg(target_os = "windows")]
pub fn focused() -> bool {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetForegroundWindow() -> *mut core::ffi::c_void;
        fn GetWindowThreadProcessId(window: *mut core::ffi::c_void, process: *mut u32) -> u32;
    }
    unsafe {
        let mut process = 0;
        GetWindowThreadProcessId(GetForegroundWindow(), &mut process);
        process == std::process::id()
    }
}
#[cfg(target_os = "linux")]
pub fn focused() -> bool {
    use std::ffi::{c_char, c_int, c_ulong, c_void, CStr};
    #[link(name = "libX11.so.6", kind = "dylib", modifiers = "+verbatim")]
    unsafe extern "C" {
        fn XOpenDisplay(name: *const c_char) -> *mut c_void;
        fn XCloseDisplay(display: *mut c_void) -> c_int;
        fn XGetInputFocus(display: *mut c_void, focus: *mut c_ulong, revert: *mut c_int) -> c_int;
        fn XFetchName(display: *mut c_void, window: c_ulong, name: *mut *mut c_char) -> c_int;
        fn XQueryTree(
            display: *mut c_void,
            window: c_ulong,
            root: *mut c_ulong,
            parent: *mut c_ulong,
            children: *mut *mut c_ulong,
            count: *mut u32,
        ) -> c_int;
        fn XFree(pointer: *mut c_void) -> c_int;
    }
    struct DisplayHandle(*mut c_void);
    impl Drop for DisplayHandle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    XCloseDisplay(self.0);
                }
            }
        }
    }
    thread_local! {
        static DISPLAY: DisplayHandle = DisplayHandle(unsafe { XOpenDisplay(core::ptr::null()) });
    }
    DISPLAY.with(|connection| unsafe {
        let display = connection.0;
        if display.is_null() {
            return true;
        }
        let mut window = 0;
        let mut revert = 0;
        XGetInputFocus(display, &mut window, &mut revert);
        // PointerRoot is used by bare Xvfb without a window manager.
        if window <= 1 {
            return true;
        }
        let mut result = false;
        for _ in 0..8 {
            let mut name = core::ptr::null_mut();
            if XFetchName(display, window, &mut name) != 0 && !name.is_null() {
                result = CStr::from_ptr(name).to_bytes() == b"Embertrail";
                XFree(name.cast());
                if result {
                    break;
                }
            }
            let (mut root, mut parent, mut count) = (0, 0, 0);
            let mut children = core::ptr::null_mut();
            if XQueryTree(
                display,
                window,
                &mut root,
                &mut parent,
                &mut children,
                &mut count,
            ) == 0
            {
                break;
            }
            if !children.is_null() {
                XFree(children.cast());
            }
            if parent == 0 || window == root {
                break;
            }
            window = parent;
        }
        result
    })
}
#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn focused() -> bool {
    true
}
