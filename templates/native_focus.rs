//! Application-owned foreground check. Keep OS unsafe calls out of the engine library.
#[cfg(windows)]
pub fn focused() -> bool {
    // Read only the foreground process ID; no external window changes.
    unsafe {
        let mut pid = 0;
        windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(
            windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow(),
            &mut pid,
        );
        pid == windows_sys::Win32::System::Threading::GetCurrentProcessId()
    }
}
#[cfg(not(windows))]
pub fn focused() -> bool {
    true
} // GameShell additionally handles minimization.

/// Native key-state hook for the shared runner; no engine source is copied.
#[allow(dead_code)] // Older static-viewer hosts only use focused().
pub fn keyboard() -> Option<fn(i32) -> i16> {
    #[cfg(windows)]
    {
        Some(read_key)
    }
    #[cfg(not(windows))]
    {
        None
    }
}
#[cfg(windows)]
fn read_key(vk: i32) -> i16 {
    // Read-only bound virtual-key query; the engine tracks edges and focus.
    unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(vk) }
}
