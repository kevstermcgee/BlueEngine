//! Native Windows path-only drop handling; avoids the renderer's eager file-byte copies.
#[cfg(not(target_os = "windows"))]
pub fn install() -> Result<(), String> {
    Ok(())
}
#[cfg(not(target_os = "windows"))]
pub fn take() -> Vec<std::path::PathBuf> {
    macroquad::input::get_dropped_files()
        .into_iter()
        .filter_map(|f| f.path)
        .collect()
}
#[cfg(target_os = "windows")]
mod native {
    use std::{
        ffi::OsString,
        os::windows::ffi::OsStringExt,
        path::PathBuf,
        sync::{
            atomic::{AtomicIsize, Ordering},
            Mutex, OnceLock,
        },
    };
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        UI::{
            Input::KeyboardAndMouse::GetActiveWindow,
            Shell::{DragAcceptFiles, DragFinish, DragQueryFileW},
            WindowsAndMessaging::{
                CallWindowProcW, GetWindowThreadProcessId, SetWindowLongPtrW, GWLP_WNDPROC,
                WM_DROPFILES,
            },
        },
    };
    static ORIGINAL: AtomicIsize = AtomicIsize::new(0);
    static QUEUE: OnceLock<Mutex<Vec<PathBuf>>> = OnceLock::new();
    unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        if msg == WM_DROPFILES {
            let drop = w as isize;
            let count = DragQueryFileW(drop, u32::MAX, std::ptr::null_mut(), 0);
            let mut paths = Vec::new();
            for i in 0..count {
                let len = DragQueryFileW(drop, i, std::ptr::null_mut(), 0);
                let mut text = vec![0_u16; len as usize + 1];
                let read = DragQueryFileW(drop, i, text.as_mut_ptr(), len + 1);
                if read > 0 {
                    paths.push(PathBuf::from(OsString::from_wide(&text[..read as usize])));
                }
            }
            DragFinish(drop);
            if let Some(queue) = QUEUE.get() {
                if let Ok(mut q) = queue.lock() {
                    q.extend(paths);
                }
            }
            return 0;
        }
        let original = ORIGINAL.load(Ordering::Acquire);
        // SetWindowLongPtrW installed us only after a valid original procedure was obtained.
        CallWindowProcW(
            Some(std::mem::transmute::<
                isize,
                unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
            >(original)),
            hwnd,
            msg,
            w,
            l,
        )
    }
    pub fn install() -> Result<(), String> {
        QUEUE.get_or_init(|| Mutex::new(Vec::new()));
        unsafe {
            let hwnd = GetActiveWindow();
            let mut pid = 0;
            if hwnd == 0
                || GetWindowThreadProcessId(hwnd, &mut pid) == 0
                || pid != std::process::id()
            {
                return Err("Could not enable native file drops for this window".into());
            }
            let old = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, window_proc as *const () as isize);
            if old == 0 {
                return Err("Could not enable file drops".into());
            }
            ORIGINAL.store(old, Ordering::Release);
            DragAcceptFiles(hwnd, 1);
        }
        Ok(())
    }
    pub fn take() -> Vec<PathBuf> {
        QUEUE
            .get()
            .and_then(|q| q.lock().ok().map(|mut q| std::mem::take(&mut *q)))
            .unwrap_or_default()
    }
}
#[cfg(target_os = "windows")]
pub use native::{install, take};
