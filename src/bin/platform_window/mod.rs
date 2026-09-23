//! Window lifecycle operations are restricted to this executable's own GUI thread.
#[cfg(windows)]
fn own_window() -> windows_sys::Win32::Foundation::HWND {
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM},
        System::Threading::GetCurrentThreadId,
        UI::WindowsAndMessaging::*,
    };
    unsafe extern "system" fn find(hwnd: HWND, data: LPARAM) -> i32 {
        // EnumThreadWindows supplies live handles owned by our thread; the callback
        // runs synchronously while the caller's output pointer remains valid.
        unsafe {
            if IsWindowVisible(hwnd) != 0 && GetWindow(hwnd, GW_OWNER).is_null() {
                *(data as *mut HWND) = hwnd;
                return 0;
            }
        }
        1
    }
    let mut hwnd = std::ptr::null_mut();
    // The callback cannot outlive this stack variable.
    unsafe {
        EnumThreadWindows(
            GetCurrentThreadId(),
            Some(find),
            &mut hwnd as *mut HWND as LPARAM,
        );
    }
    hwnd
}

pub fn maximize() {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        let hwnd = own_window();
        if !hwnd.is_null() {
            // Queue rather than synchronously reenter the graphics event handler.
            // Windows chooses the current monitor's work area and keeps the caption.
            unsafe {
                PostMessageW(hwnd, WM_SYSCOMMAND, SC_MAXIMIZE as usize, 0);
            }
        }
    }
}

pub fn report() -> String {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        let hwnd = own_window();
        if !hwnd.is_null() {
            // Both queries read this process's live window; no pointers are retained.
            unsafe {
                return format!(
                    "maximized={}\ncaption={}\n",
                    IsZoomed(hwnd) != 0,
                    GetWindowLongPtrW(hwnd, GWL_STYLE) as u32 & WS_CAPTION == WS_CAPTION
                );
            }
        }
    }
    "window_state=unavailable\n".into()
}
