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
