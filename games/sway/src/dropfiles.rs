//! Native Windows path-only drop handling; avoids the renderer's eager file-byte copies.
#[cfg(not(target_os = "windows"))]
pub fn install(_verify: bool) -> Result<(), String> {
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
            atomic::{AtomicBool, AtomicIsize, Ordering},
            Mutex, OnceLock,
        },
    };
    use windows_sys::Win32::{
        Foundation::{GlobalFree, HWND, LPARAM, LRESULT, POINT, WPARAM},
        System::{
            DataExchange::COPYDATASTRUCT,
            Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE, GMEM_ZEROINIT},
        },
        UI::{
            Input::KeyboardAndMouse::GetActiveWindow,
            Shell::{DragAcceptFiles, DragFinish, DragQueryFileW, DROPFILES},
            WindowsAndMessaging::{
                CallWindowProcW, GetWindowThreadProcessId, SendMessageW, SetWindowLongPtrW,
                GWLP_WNDPROC, WM_COPYDATA, WM_DROPFILES,
            },
        },
    };
    static ORIGINAL: AtomicIsize = AtomicIsize::new(0);
    static VERIFY: AtomicBool = AtomicBool::new(false);
    static QUEUE: OnceLock<Mutex<Vec<PathBuf>>> = OnceLock::new();
    unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        // QA-only IPC bridge: GlobalAlloc handles belong to their allocating process.
        // Marshal test paths, then allocate a real HDROP in this receiver and exercise
        // the ordinary WM_DROPFILES procedure. Normal launches never enable this.
        if msg == WM_COPYDATA && VERIFY.load(Ordering::Acquire) && l != 0 {
            let data = &*(l as *const COPYDATASTRUCT);
            if data.dwData == 0x53574159
                && (4..=65536).contains(&data.cbData)
                && data.cbData.is_multiple_of(2)
                && !data.lpData.is_null()
            {
                let header = std::mem::size_of::<DROPFILES>();
                let handle =
                    GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, header + data.cbData as usize);
                if handle == 0 {
                    return 0;
                }
                let pointer = GlobalLock(handle);
                if pointer.is_null() {
                    GlobalFree(handle);
                    return 0;
                }
                std::ptr::write(
                    pointer as *mut DROPFILES,
                    DROPFILES {
                        pFiles: header as u32,
                        pt: POINT { x: 0, y: 0 },
                        fNC: 0,
                        fWide: 1,
                    },
                );
                std::ptr::copy_nonoverlapping(
                    data.lpData as *const u8,
                    (pointer as *mut u8).add(header),
                    data.cbData as usize,
                );
                GlobalUnlock(handle);
                SendMessageW(hwnd, WM_DROPFILES, handle as usize, 0);
                return 1;
            }
        }
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
    pub fn install(verify: bool) -> Result<(), String> {
        QUEUE.get_or_init(|| Mutex::new(Vec::new()));
        VERIFY.store(verify, Ordering::Release);
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
