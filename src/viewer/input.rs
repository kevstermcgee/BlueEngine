#![allow(unsafe_code)]
//! Desktop input, focus tracking, and Windows native key handling.
//!
//! Provides:
//! - Physical key down tracking with repeat filter
//! - Single-frame pressed edge detection
//! - Windows `GetAsyncKeyState` fallback for accessibility and lost key-up recovery
//! - Foreground window focus detection (PID verification)
//! - Focus loss protection: losing window focus immediately clears movement and releases cursor
//! - Arrow / WASD axis normalization
//! - Pointer capture / mouse lock toggling
use macroquad::prelude::*;
use std::collections::HashSet;

/// Robust keyboard state tracker with Windows native fallback and focus loss protection.
#[derive(Default)]
pub struct Keys {
    pub down: HashSet<KeyCode>,
    pressed: HashSet<KeyCode>,
    #[cfg(windows)]
    native_down: HashSet<KeyCode>,
}

impl Keys {
    pub fn new() -> Self {
        Self::default()
    }

    /// True if the key was pressed this frame or is reported pressed by macroquad.
    pub fn pressed(&self, key: KeyCode) -> bool {
        self.pressed.contains(&key) || is_key_pressed(key)
    }

    /// True if the key is currently held down.
    pub fn down(&self, key: KeyCode) -> bool {
        self.down.contains(&key) || is_key_down(key)
    }

    /// Poll keyboard state against focus status.
    ///
    /// CRITICAL: When the window is unfocused (`focused == false`), all held movement keys
    /// are immediately cleared so the character never continues moving while tabbed out.
    pub fn poll(&mut self, focused: bool) {
        self.pressed.clear();
        #[cfg(windows)]
        {
            // Read only the engine's bound keys. Polling also accepts accessibility-injected
            // keys without hardware scan codes and prevents a missed key-up sticking.
            for (key, vk) in [
                (KeyCode::W, 0x57),
                (KeyCode::A, 0x41),
                (KeyCode::S, 0x53),
                (KeyCode::D, 0x44),
                (KeyCode::Up, 0x26),
                (KeyCode::Down, 0x28),
                (KeyCode::Left, 0x25),
                (KeyCode::Right, 0x27),
                (KeyCode::LeftShift, 0xA0),
                (KeyCode::RightShift, 0xA1),
                (KeyCode::Space, 0x20),
                (KeyCode::LeftControl, 0xA2),
                (KeyCode::RightControl, 0xA3),
                (KeyCode::C, 0x43),
                (KeyCode::Enter, 0x0D),
                (KeyCode::Escape, 0x1B),
                (KeyCode::Tab, 0x09),
                (KeyCode::F, 0x46),
                (KeyCode::F3, 0x72),
                (KeyCode::F11, 0x7A),
                (KeyCode::H, 0x48),
                (KeyCode::Q, 0x51),
                (KeyCode::E, 0x45),
            ] {
                let state = unsafe {
                    windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(vk)
                } as u16;
                let held = state & 0x8000 != 0;
                if focused && !self.native_down.contains(&key) && (held || state & 1 != 0) {
                    self.pressed.insert(key);
                }
                if held {
                    self.native_down.insert(key);
                } else {
                    self.native_down.remove(&key);
                }
                if focused && held {
                    self.down.insert(key);
                } else {
                    self.down.remove(&key);
                }
            }
        }
        if !focused {
            self.down.clear();
        }
    }
}

impl miniquad::EventHandler for Keys {
    fn update(&mut self) {}
    fn draw(&mut self) {}
    fn key_down_event(&mut self, key: KeyCode, _: miniquad::KeyMods, repeat: bool) {
        if !repeat {
            self.down.insert(key);
            self.pressed.insert(key);
        }
    }
    fn key_up_event(&mut self, key: KeyCode, _: miniquad::KeyMods) {
        self.down.remove(&key);
    }
}

/// Calculate normalized forward/right axes from held WASD and Arrow keys.
pub fn axes(keys: &HashSet<KeyCode>) -> (f32, f32) {
    let held = |a, b| {
        if keys.contains(&a) || keys.contains(&b) {
            1.
        } else {
            0.
        }
    };
    (
        held(KeyCode::W, KeyCode::Up) - held(KeyCode::S, KeyCode::Down),
        held(KeyCode::D, KeyCode::Right) - held(KeyCode::A, KeyCode::Left),
    )
}

/// Returns true if this process's window is currently the active foreground window.
pub fn foreground() -> bool {
    #[cfg(windows)]
    {
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
    {
        true
    }
}

/// Capture or release the mouse cursor and visibility.
pub fn capture(active: bool) {
    set_cursor_grab(active);
    show_mouse(!active);
}
