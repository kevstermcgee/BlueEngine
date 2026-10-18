//! Application console/key hooks; shared presentation owns native focus.
/// Compatibility hook: the shared GameShell owns native focus and neutral-input handling.
pub fn focused() -> bool {
    true
}

/// A windowed release build has no console of its own (see `windows_subsystem` in main.rs), so a game
/// started from Explorer or a desktop shortcut opens no stray console window. Reattach to the terminal
/// that launched it, if any, so `--capture`/`--perf` output stays visible.
#[allow(dead_code)] // The engine's own custom_client example does not call it; the custom-sim template does.
pub fn attach_console() {
    #[cfg(windows)]
    // Attaching only changes where this process's stdout goes; failure (no parent console) is harmless.
    unsafe {
        windows_sys::Win32::System::Console::AttachConsole(
            windows_sys::Win32::System::Console::ATTACH_PARENT_PROCESS,
        );
    }
}

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
