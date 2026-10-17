//! Native focus and physical key state, isolated from the rendering-free rules.
pub struct Sample {
    pub focused: bool,
    pub lost: bool,
    pub neutral: bool,
    pub space: bool,
}
pub use platform::WindowFocus;

#[cfg(target_os = "windows")]
mod platform {
    use super::Sample;
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, Ordering};
    static LOST: AtomicBool = AtomicBool::new(false);
    #[link(name = "user32")]
    extern "system" {
        fn GetForegroundWindow() -> *mut c_void;
        fn GetWindowThreadProcessId(window: *mut c_void, pid: *mut u32) -> u32;
        fn GetAsyncKeyState(key: i32) -> i16;
        fn SetWinEventHook(
            min: u32,
            max: u32,
            module: *mut c_void,
            callback: Option<
                unsafe extern "system" fn(*mut c_void, u32, *mut c_void, i32, i32, u32, u32),
            >,
            process: u32,
            thread: u32,
            flags: u32,
        ) -> *mut c_void;
        fn UnhookWinEvent(hook: *mut c_void) -> i32;
    }
    unsafe fn own_window(window: *mut c_void) -> bool {
        let mut pid = 0;
        GetWindowThreadProcessId(window, &mut pid);
        !window.is_null() && pid == std::process::id()
    }
    unsafe extern "system" fn foreground(
        _: *mut c_void,
        _: u32,
        window: *mut c_void,
        _: i32,
        _: i32,
        _: u32,
        _: u32,
    ) {
        if !own_window(window) {
            LOST.store(true, Ordering::Relaxed);
        }
    }
    pub struct WindowFocus(*mut c_void);
    impl WindowFocus {
        pub fn new() -> Self {
            // EVENT_SYSTEM_FOREGROUND, out-of-context: callbacks use this thread's message pump.
            let hook =
                unsafe { SetWinEventHook(3, 3, std::ptr::null_mut(), Some(foreground), 0, 0, 0) };
            assert!(!hook.is_null(), "Cannot subscribe to Windows focus events");
            Self(hook)
        }
        pub fn sample(&mut self) -> Sample {
            // All calls run on the native client thread; no state enters a Snapshot.
            unsafe {
                Sample {
                    focused: own_window(GetForegroundWindow()),
                    lost: LOST.swap(false, Ordering::Relaxed),
                    neutral: (1..256).all(|key| GetAsyncKeyState(key) >= 0),
                    space: GetAsyncKeyState(0x20) < 0,
                }
            }
        }
    }
    impl Drop for WindowFocus {
        fn drop(&mut self) {
            unsafe {
                UnhookWinEvent(self.0);
            }
        }
    }
}

#[cfg(target_os = "linux")]
#[allow(non_snake_case)] // Exact Xlib symbol names in the existing dynamic-loader macro.
mod platform {
    use super::Sample;
    use std::ffi::{c_char, c_int, c_long, c_uint, c_ulong, c_void, CStr, CString};
    type ErrorHandler = Option<unsafe extern "C" fn(*mut c_void, *mut c_void) -> c_int>;
    macroquad::miniquad::declare_module!(X11,
        "libX11.so.6", ... ...
        fn XOpenDisplay(*const c_char) -> *mut c_void,
        fn XCloseDisplay(*mut c_void) -> c_int,
        fn XGetInputFocus(*mut c_void, *mut c_ulong, *mut c_int) -> c_int,
        fn XDefaultRootWindow(*mut c_void) -> c_ulong,
        fn XQueryTree(*mut c_void, c_ulong, *mut c_ulong, *mut c_ulong, *mut *mut c_ulong, *mut c_uint) -> c_int,
        fn XFetchName(*mut c_void, c_ulong, *mut *mut c_char) -> c_int,
        fn XFree(*mut c_void) -> c_int,
        fn XStoreName(*mut c_void, c_ulong, *const c_char) -> c_int,
        fn XInternAtom(*mut c_void, *const c_char, c_int) -> c_ulong,
        fn XChangeProperty(*mut c_void, c_ulong, c_ulong, c_ulong, c_int, c_int, *const u8, c_int) -> c_int,
        fn XSetErrorHandler(ErrorHandler) -> ErrorHandler,
        fn XSync(*mut c_void, c_int) -> c_int,
        fn XSelectInput(*mut c_void, c_ulong, c_long) -> c_int,
        fn XCheckTypedWindowEvent(*mut c_void, c_ulong, c_int, *mut c_long) -> c_int,
        fn XQueryKeymap(*mut c_void, *mut c_char) -> c_int,
        fn XKeysymToKeycode(*mut c_void, c_ulong) -> u8,
        fn XFlush(*mut c_void) -> c_int,
        ...
        ...
    );
    pub struct WindowFocus {
        api: X11,
        display: *mut c_void,
        window: c_ulong,
        space: u8,
    }

    // A GLXWindow is a rendering resource, not the X Window it wraps. Looking
    // up our process-specific startup title works with both GLX and EGL, and
    // distinguishes two concurrent copies of Afterpull under a reparenting WM.
    unsafe fn find_window(api: &X11, display: *mut c_void, title: &CStr) -> Option<c_ulong> {
        let mut pending = vec![(api.XDefaultRootWindow)(display)];
        while let Some(window) = pending.pop() {
            let mut name = std::ptr::null_mut();
            (api.XFetchName)(display, window, &mut name);
            if !name.is_null() {
                let matches = CStr::from_ptr(name) == title;
                (api.XFree)(name.cast());
                if matches {
                    return Some(window);
                }
            }
            let (mut root, mut parent, mut count) = (0, 0, 0);
            let mut children = std::ptr::null_mut();
            (api.XQueryTree)(
                display,
                window,
                &mut root,
                &mut parent,
                &mut children,
                &mut count,
            );
            if !children.is_null() {
                pending.extend_from_slice(std::slice::from_raw_parts(children, count as usize));
                (api.XFree)(children.cast());
            }
        }
        None
    }

    unsafe extern "C" fn ignore_disappearing_window(_: *mut c_void, _: *mut c_void) -> c_int {
        // Other applications may close windows while the startup tree is read.
        0
    }

    impl WindowFocus {
        pub fn new() -> Self {
            let api = X11::try_load().expect("Xlib is required by the native X11 client");
            unsafe {
                let display = (api.XOpenDisplay)(std::ptr::null());
                assert!(
                    !display.is_null(),
                    "Afterpull needs an X11 display for focus protection"
                );
                let title = CString::new(super::super::window_title()).unwrap();
                // Scope error suppression to read-only discovery on the client
                // thread, draining its replies before restoring Xlib's handler.
                let previous = (api.XSetErrorHandler)(Some(ignore_disappearing_window));
                let found = find_window(&api, display, &title);
                (api.XSync)(display, 0);
                (api.XSetErrorHandler)(previous);
                let window = found.expect("Cannot find Afterpull's native X11 window");
                let title =
                    CString::new(<afterpull::Afterpull as vesper3d::two_d::GameLogic>::TITLE)
                        .unwrap();
                (api.XStoreName)(display, window, title.as_ptr());
                let property = (api.XInternAtom)(display, c"_NET_WM_NAME".as_ptr(), 0);
                let utf8 = (api.XInternAtom)(display, c"UTF8_STRING".as_ptr(), 0);
                (api.XChangeProperty)(
                    display,
                    window,
                    property,
                    utf8,
                    8,
                    0,
                    title.as_bytes().as_ptr(),
                    title.as_bytes().len() as c_int,
                );
                (api.XSelectInput)(display, window, 1 << 21); // FocusChangeMask, separate connection.
                (api.XFlush)(display);
                let space = (api.XKeysymToKeycode)(display, 0x20);
                Self {
                    api,
                    display,
                    window,
                    space,
                }
            }
        }
        pub fn sample(&mut self) -> Sample {
            unsafe {
                let mut focus = 0;
                let mut revert = 0;
                (self.api.XGetInputFocus)(self.display, &mut focus, &mut revert);
                // XEvent is a union padded to 24 native longs. Retain loss even
                // when focus-out and focus-in both happened between native frames.
                let mut event = [0 as c_long; 24];
                let mut lost = false;
                while (self.api.XCheckTypedWindowEvent)(
                    self.display,
                    self.window,
                    10,
                    event.as_mut_ptr(),
                ) != 0
                {
                    lost = true;
                }
                while (self.api.XCheckTypedWindowEvent)(
                    self.display,
                    self.window,
                    9,
                    event.as_mut_ptr(),
                ) != 0
                {}
                let mut keys = [0u8; 32];
                (self.api.XQueryKeymap)(self.display, keys.as_mut_ptr().cast());
                Sample {
                    focused: focus == self.window,
                    lost,
                    neutral: keys.iter().all(|k| *k == 0),
                    space: keys[self.space as usize / 8] & (1 << (self.space % 8)) != 0,
                }
            }
        }
    }
    impl Drop for WindowFocus {
        fn drop(&mut self) {
            unsafe {
                (self.api.XCloseDisplay)(self.display);
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        macroquad::miniquad::declare_module!(TestX11,
            "libX11.so.6", ... ...
            fn XCreateSimpleWindow(*mut c_void, c_ulong, c_int, c_int, c_uint, c_uint, c_uint, c_ulong, c_ulong) -> c_ulong,
            fn XMapWindow(*mut c_void, c_ulong) -> c_int,
            fn XSetInputFocus(*mut c_void, c_ulong, c_int, c_ulong) -> c_int,
            ... ...
        );

        // This is intentionally a real X11 integration regression, runnable
        // under Xvfb. It needs no OpenGL context and does not simulate focus.
        #[test]
        #[ignore = "requires an X11 display; run under xvfb-run with --ignored"]
        fn native_focus_uses_real_window_without_a_gl_context() {
            unsafe {
                let api = X11::try_load().unwrap();
                let test = TestX11::try_load().unwrap();
                let display = (api.XOpenDisplay)(std::ptr::null());
                assert!(
                    !display.is_null(),
                    "No X11 display for native focus regression"
                );
                let root = (api.XDefaultRootWindow)(display);
                // A WM may reparent our window. Another instance may already
                // have the ordinary Afterpull title; neither changes ownership.
                let parent = (test.XCreateSimpleWindow)(display, root, 0, 0, 300, 300, 0, 0, 0);
                let own = (test.XCreateSimpleWindow)(display, parent, 0, 0, 100, 100, 0, 0, 0);
                let other = (test.XCreateSimpleWindow)(display, root, 320, 0, 100, 100, 0, 0, 0);
                let title = CString::new(super::super::super::window_title()).unwrap();
                (api.XStoreName)(display, own, title.as_ptr());
                (api.XStoreName)(display, other, c"Afterpull".as_ptr());
                for window in [parent, own, other] {
                    (test.XMapWindow)(display, window);
                }
                (test.XSetInputFocus)(display, own, 0, 0);
                (api.XSync)(display, 0);

                let mut focus = WindowFocus::new();
                assert_eq!(
                    focus.window, own,
                    "subscribe to the X window, never a GLX drawable"
                );
                assert!(focus.sample().focused);
                let mut name = std::ptr::null_mut();
                (api.XFetchName)(display, own, &mut name);
                assert!(!name.is_null());
                assert_eq!(CStr::from_ptr(name), c"Afterpull");
                (api.XFree)(name.cast());

                (test.XSetInputFocus)(display, other, 0, 0);
                (api.XSync)(display, 0);
                let away = focus.sample();
                assert!(!away.focused && away.lost);
                (test.XSetInputFocus)(display, own, 0, 0);
                (api.XSync)(display, 0);
                assert!(focus.sample().focused);
                // A full application switch between frames must retain loss.
                (test.XSetInputFocus)(display, other, 0, 0);
                (test.XSetInputFocus)(display, own, 0, 0);
                (api.XSync)(display, 0);
                let returned = focus.sample();
                assert!(returned.focused && returned.lost);
                drop(focus);
                (api.XCloseDisplay)(display); // Also destroys the fixture windows.
            }
        }
    }
}
