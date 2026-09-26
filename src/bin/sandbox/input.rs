//! One key-edge source on Windows, including accessibility-injected keys.
use macroquad::prelude::KeyCode;
#[cfg(windows)]
use std::{cell::RefCell, collections::HashSet};
#[cfg(windows)]
#[derive(Default)]
struct Keys {
    previous: HashSet<KeyCode>,
    down: HashSet<KeyCode>,
    pressed: HashSet<KeyCode>,
}
#[cfg(windows)]
thread_local! {static KEYS:RefCell<Keys> = RefCell::new(Keys::default());}

pub fn poll(focused: bool) {
    #[cfg(windows)]
    KEYS.with(|cell| {
        let mut keys = cell.borrow_mut();
        keys.down.clear();
        keys.pressed.clear();
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
            (KeyCode::LeftControl, 0xA2),
            (KeyCode::C, 0x43),
            (KeyCode::Space, 0x20),
            (KeyCode::Enter, 0x0D),
            (KeyCode::Escape, 0x1B),
            (KeyCode::Tab, 0x09),
            (KeyCode::F, 0x46),
            (KeyCode::F11, 0x7A),
            (KeyCode::F3, 0x72),
            (KeyCode::Q, 0x51),
            (KeyCode::E, 0x45),
            (KeyCode::R, 0x52),
            (KeyCode::B, 0x42),
            (KeyCode::V, 0x56),
            (KeyCode::G, 0x47),
            (KeyCode::Z, 0x5A),
            (KeyCode::Delete, 0x2E),
            (KeyCode::Home, 0x24),
            (KeyCode::Backspace, 0x08),
            (KeyCode::LeftAlt, 0xA4),
        ] {
            // Read only our bound virtual keys; no memory pointers or external mutations.
            let state =
                unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(vk) }
                    as u16;
            let held = state & 0x8000 != 0;
            let edge = !keys.previous.contains(&key) && (held || state & 1 != 0);
            if focused && edge {
                keys.pressed.insert(key);
            }
            if focused && (held || edge) {
                keys.down.insert(key);
            }
            if held {
                keys.previous.insert(key);
            } else {
                keys.previous.remove(&key);
            }
        }
    });
    #[cfg(not(windows))]
    let _ = focused;
}
pub fn pressed(key: KeyCode) -> bool {
    #[cfg(windows)]
    {
        KEYS.with(|k| k.borrow().pressed.contains(&key))
    }
    #[cfg(not(windows))]
    {
        macroquad::prelude::is_key_pressed(key)
    }
}
pub fn down(key: KeyCode) -> bool {
    #[cfg(windows)]
    {
        KEYS.with(|k| k.borrow().down.contains(&key))
    }
    #[cfg(not(windows))]
    {
        macroquad::prelude::is_key_down(key)
    }
}
pub fn axes() -> (f32, f32) {
    let held = |a, b| u8::from(down(a) || down(b)) as f32;
    (
        held(KeyCode::W, KeyCode::Up) - held(KeyCode::S, KeyCode::Down),
        held(KeyCode::D, KeyCode::Right) - held(KeyCode::A, KeyCode::Left),
    )
}
