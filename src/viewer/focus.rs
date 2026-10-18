//! Native focus adapter shared by presentation clients. No state enters simulation or saves.
//! X11 is the supported Linux window backend; Windows retains foreground events between frames.
#![allow(unsafe_code)] // Narrow OS adapter; all calls stay on the window thread.
use std::cell::RefCell;
static TITLE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
#[cfg(target_os = "linux")]
fn window_title_for(title: &str) -> String {
    format!("{title} [{}]", std::process::id())
}
/// Register the native window identity before creation. The adapter restores the display title.
pub fn window_title(title: &str) -> String {
    *TITLE.lock().unwrap() = Some(title.into());
    #[cfg(target_os = "linux")]
    {
        window_title_for(title)
    }
    #[cfg(not(target_os = "linux"))]
    {
        title.into()
    }
}
thread_local! { static WINDOW: RefCell<Option<WindowFocus>> = const { RefCell::new(None) }; }
/// Poll once per frame. Custom hosts may instead supply their own native focus observation.
pub fn sample() -> Option<Sample> {
    WINDOW.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            let title = TITLE.lock().unwrap().clone()?;
            *slot = Some(WindowFocus::new(&title));
        }
        Some(slot.as_mut().unwrap().sample())
    })
}
/// Physical held state after `sample`, avoiding stale window key-up events after focus loss.
/// Unregistered custom hosts retain Macroquad's own input source.
pub fn key_down(key: macroquad::prelude::KeyCode) -> bool {
    WINDOW.with(|slot| {
        slot.borrow()
            .as_ref()
            .map_or_else(|| macroquad::prelude::is_key_down(key), |w| w.key_down(key))
    })
}
/// Physical mouse-button state; a release outside the window must not leave drag/look held.
pub fn mouse_down(button: macroquad::prelude::MouseButton) -> bool {
    WINDOW.with(|slot| {
        slot.borrow().as_ref().map_or_else(
            || macroquad::prelude::is_mouse_button_down(button),
            |w| w.mouse_down(button),
        )
    })
}
/// A loss discards this frame; returning requires a complete neutral frame before fresh input.
#[derive(Default)]
pub struct NeutralGate {
    blocked: bool,
}
impl NeutralGate {
    pub fn observe(&mut self, focused: bool, lost: bool, neutral: bool) -> bool {
        if !focused || lost {
            self.blocked = true;
            return false;
        }
        if self.blocked {
            self.blocked = !neutral;
            return false;
        }
        true
    }
}
// Native focus and physical key state, isolated from the rendering-free rules.
pub struct Sample {
    pub focused: bool,
    pub lost: bool,
    pub neutral: bool,
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
        pub fn new(_title: &str) -> Self {
            // EVENT_SYSTEM_FOREGROUND, out-of-context: callbacks use this thread's message pump.
            let hook =
                unsafe { SetWinEventHook(3, 3, std::ptr::null_mut(), Some(foreground), 0, 0, 0) };
            assert!(!hook.is_null(), "Cannot subscribe to Windows focus events");
            Self(hook)
        }
        pub fn key_down(&self, key: macroquad::prelude::KeyCode) -> bool {
            let code = key as u32;
            let vk = match code {
                32 | 48..=57 | 65..=90 => code,
                256 => 0x1b,
                257 => 0x0d,
                258 => 0x09,
                259 => 0x08,
                260 => 0x2d,
                261 => 0x2e,
                262 => 0x27,
                263 => 0x25,
                264 => 0x28,
                265 => 0x26,
                266 => 0x21,
                267 => 0x22,
                268 => 0x24,
                269 => 0x23,
                290..=313 => 0x70 + code - 290,
                320..=329 => 0x60 + code - 320,
                340 => 0xa0,
                344 => 0xa1,
                341 => 0xa2,
                345 => 0xa3,
                342 => 0xa4,
                346 => 0xa5,
                343 => 0x5b,
                347 => 0x5c,
                39 => 0xde,
                44 => 0xbc,
                45 => 0xbd,
                46 => 0xbe,
                47 => 0xbf,
                59 => 0xba,
                61 => 0xbb,
                91 => 0xdb,
                92 => 0xdc,
                93 => 0xdd,
                96 => 0xc0,
                _ => return false,
            };
            unsafe { GetAsyncKeyState(vk as i32) < 0 }
        }
        pub fn mouse_down(&self, button: macroquad::prelude::MouseButton) -> bool {
            let vk = match button {
                macroquad::prelude::MouseButton::Left => 1,
                macroquad::prelude::MouseButton::Right => 2,
                macroquad::prelude::MouseButton::Middle => 4,
                _ => return false,
            };
            unsafe { GetAsyncKeyState(vk) < 0 }
        }
        pub fn sample(&mut self) -> Sample {
            // All calls run on the native client thread; no state enters a Snapshot.
            unsafe {
                Sample {
                    focused: own_window(GetForegroundWindow()),
                    lost: LOST.swap(false, Ordering::Relaxed),
                    neutral: (1..256).all(|key| GetAsyncKeyState(key) >= 0),
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
    use super::{window_title_for, Sample};
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
        fn XQueryPointer(*mut c_void, c_ulong, *mut c_ulong, *mut c_ulong, *mut c_int, *mut c_int, *mut c_int, *mut c_int, *mut c_uint) -> c_int,
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
        keys: [u8; 32],
        mouse: u32,
    }

    // A GLXWindow is a rendering resource, not the X Window it wraps. Looking
    // up our process-specific startup title works with both GLX and EGL, and
    // distinguishes two concurrent copies of BlueEngine under a reparenting WM.
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
        pub fn new(title: &str) -> Self {
            let api = X11::try_load().expect("Xlib is required by the native X11 client");
            unsafe {
                let display = (api.XOpenDisplay)(std::ptr::null());
                assert!(
                    !display.is_null(),
                    "BlueEngine needs an X11 display for focus protection"
                );
                let title = CString::new(window_title_for(title)).unwrap();
                // Scope error suppression to read-only discovery on the client
                // thread, draining its replies before restoring Xlib's handler.
                let previous = (api.XSetErrorHandler)(Some(ignore_disappearing_window));
                let found = find_window(&api, display, &title);
                (api.XSync)(display, 0);
                (api.XSetErrorHandler)(previous);
                let window = found.expect("Cannot find BlueEngine's native X11 window");
                let title = CString::new(title).unwrap();
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
                Self {
                    api,
                    display,
                    window,
                    keys: [0; 32],
                    mouse: 0,
                }
            }
        }
        pub fn key_down(&self, key: macroquad::prelude::KeyCode) -> bool {
            let code = key as u32;
            let symbol = match code {
                32..=126 => code,
                256 => 0xff1b,
                257 => 0xff0d,
                258 => 0xff09,
                259 => 0xff08,
                260 => 0xff63,
                261 => 0xffff,
                262 => 0xff53,
                263 => 0xff51,
                264 => 0xff54,
                265 => 0xff52,
                266 => 0xff55,
                267 => 0xff56,
                268 => 0xff50,
                269 => 0xff57,
                290..=314 => 0xffbe + code - 290,
                320..=329 => 0xffb0 + code - 320,
                340 => 0xffe1,
                344 => 0xffe2,
                341 => 0xffe3,
                345 => 0xffe4,
                342 => 0xffe9,
                346 => 0xffea,
                343 => 0xffeb,
                347 => 0xffec,
                _ => return false,
            };
            let key = unsafe { (self.api.XKeysymToKeycode)(self.display, symbol.into()) } as usize;
            key != 0 && self.keys[key / 8] & (1 << (key % 8)) != 0
        }
        pub fn mouse_down(&self, button: macroquad::prelude::MouseButton) -> bool {
            let mask = match button {
                macroquad::prelude::MouseButton::Left => 0x100,
                macroquad::prelude::MouseButton::Middle => 0x200,
                macroquad::prelude::MouseButton::Right => 0x400,
                _ => return false,
            };
            self.mouse & mask != 0
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
                let (mut root, mut child, mut rx, mut ry, mut wx, mut wy, mut mask) =
                    (0, 0, 0, 0, 0, 0, 0);
                (self.api.XQueryPointer)(
                    self.display,
                    self.window,
                    &mut root,
                    &mut child,
                    &mut rx,
                    &mut ry,
                    &mut wx,
                    &mut wy,
                    &mut mask,
                );
                self.keys = keys;
                self.mouse = mask;
                Sample {
                    focused: focus == self.window,
                    lost,
                    neutral: keys.iter().all(|k| *k == 0) && mask & 0x1f00 == 0,
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

        macroquad::miniquad::declare_module!(TestXInput,
            "libXtst.so.6", ... ...
            fn XTestFakeKeyEvent(*mut c_void, c_uint, c_int, c_ulong) -> c_int,
            fn XTestFakeButtonEvent(*mut c_void, c_uint, c_int, c_ulong) -> c_int,
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
                // have the ordinary BlueEngine title; neither changes ownership.
                let parent = (test.XCreateSimpleWindow)(display, root, 0, 0, 300, 300, 0, 0, 0);
                let own = (test.XCreateSimpleWindow)(display, parent, 0, 0, 100, 100, 0, 0, 0);
                let other = (test.XCreateSimpleWindow)(display, root, 320, 0, 100, 100, 0, 0, 0);
                let title = CString::new(window_title_for("Focus fixture")).unwrap();
                (api.XStoreName)(display, own, title.as_ptr());
                (api.XStoreName)(display, other, c"Focus fixture".as_ptr());
                for window in [parent, own, other] {
                    (test.XMapWindow)(display, window);
                }
                (test.XSetInputFocus)(display, own, 0, 0);
                (api.XSync)(display, 0);

                let mut focus = WindowFocus::new("Focus fixture");
                assert_eq!(
                    focus.window, own,
                    "subscribe to the X window, never a GLX drawable"
                );
                assert!(focus.sample().focused);
                let mut name = std::ptr::null_mut();
                (api.XFetchName)(display, own, &mut name);
                assert!(!name.is_null());
                assert_eq!(CStr::from_ptr(name), c"Focus fixture");
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
                let input = TestXInput::try_load().expect("XTest for real native input regression");
                let key = (api.XKeysymToKeycode)(display, b'W'.into());
                (input.XTestFakeKeyEvent)(display, key.into(), 1, 0);
                (input.XTestFakeButtonEvent)(display, 1, 1, 0);
                (api.XSync)(display, 0);
                assert!(focus.sample().focused);
                assert!(focus.key_down(macroquad::prelude::KeyCode::W));
                assert!(focus.mouse_down(macroquad::prelude::MouseButton::Left));
                (test.XSetInputFocus)(display, other, 0, 0);
                (api.XSync)(display, 0);
                assert!(focus.sample().lost);
                // Releases go to another window. Shared input must not retain the old down state.
                (input.XTestFakeKeyEvent)(display, key.into(), 0, 0);
                (input.XTestFakeButtonEvent)(display, 1, 0, 0);
                (test.XSetInputFocus)(display, own, 0, 0);
                (api.XSync)(display, 0);
                let returned = focus.sample();
                assert!(returned.focused && returned.neutral);
                assert!(!focus.key_down(macroquad::prelude::KeyCode::W));
                assert!(!focus.mouse_down(macroquad::prelude::MouseButton::Left));
                drop(focus);
                (api.XCloseDisplay)(display); // Also destroys the fixture windows.
            }
        }
    }
}

#[cfg(test)]
mod gate_tests {
    use super::NeutralGate;
    #[test]
    fn focus_between_frames_discards_edges_and_requires_release_frame() {
        let mut gate = NeutralGate::default();
        assert!(gate.observe(true, false, false));
        assert!(!gate.observe(false, true, false));
        assert!(!gate.observe(true, false, false)); // held keyboard/mouse/controller
        assert!(!gate.observe(true, false, true)); // drain neutral frame
        assert!(gate.observe(true, false, false)); // new press
        assert!(!gate.observe(true, true, true)); // full away/back between frames
        assert!(!gate.observe(true, false, true));
        assert!(gate.observe(true, false, true));
    }
}
