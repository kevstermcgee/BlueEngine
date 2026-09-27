//! Standard keyboard/mouse + native-controller adapter for standalone clients.
//! Own this alongside GameShell (never in thread-local storage): native device
//! workers must shut down before Windows TLS teardown. No OS calls live here.
use super::{
    controller::Movement,
    game_client::{GameShell, ShellActions},
    game_ui::NavigationInput,
    gamepad::{Button, GamepadFrame, Gamepads},
};
use macroquad::prelude::*;

pub struct ClientInput {
    backend: Option<Gamepads>,
    frame: GamepadFrame,
    status: String,
    focused: bool,
    keyboard: Option<KeyboardFrame>,
}
impl Default for ClientInput {
    fn default() -> Self {
        Self::new()
    }
}
impl ClientInput {
    /// Device failure is reported by status(); keyboard/mouse remain usable.
    pub fn new() -> Self {
        let (backend, status) = match Gamepads::new() {
            Ok(p) => (Some(p), "Controller: disconnected".into()),
            Err(e) => (None, format!("Controller unavailable: {e}")),
        };
        Self {
            backend,
            status,
            frame: GamepadFrame::default(),
            focused: false,
            keyboard: None,
        }
    }
    /// Poll once per frame, including during pause. Native focus is supplied by the host.
    pub fn poll(&mut self, focused: bool) {
        self.focused = focused;
        if let Some(p) = &mut self.backend {
            self.frame = p.poll(focused);
            self.status = p
                .connected()
                .find(|(id, _)| Some(*id) == p.active())
                .map(|(_, name)| format!("Controller: {name}"))
                .unwrap_or_else(|| "Controller: disconnected".into());
        }
    }
    pub fn status(&self) -> &str {
        &self.status
    }
    pub fn gamepad(&self) -> &GamepadFrame {
        &self.frame
    }
    pub fn navigation(&self) -> NavigationInput {
        NavigationInput {
            next: self.frame.pressed(Button::DPadDown) || self.frame.pressed(Button::DPadRight),
            previous: self.frame.pressed(Button::DPadUp) || self.frame.pressed(Button::DPadLeft),
            accept: self.frame.pressed(Button::South),
        }
    }
    pub fn shell_actions(&self, paused: bool, keys: impl Fn(KeyCode) -> bool) -> ShellActions {
        if !self.focused {
            return ShellActions::default();
        }
        let mut a = ShellActions::from_keys(keys);
        a.pause |=
            self.frame.pressed(Button::Start) || (paused && self.frame.pressed(Button::East));
        if paused {
            a.next |= self.frame.pressed(Button::DPadDown);
            a.previous |= self.frame.pressed(Button::DPadUp);
            a.accept |= self.frame.pressed(Button::South);
        }
        a
    }
    /// The standard frame entry point for new games. Advanced hosts may poll and
    /// call shell_actions separately to route their own modal screens.
    pub fn begin_frame(&mut self, shell: &mut GameShell, playing: bool, focused: bool) {
        self.begin_frame_with_keyboard(shell, playing, focused, None);
    }
    /// Optional application-owned Windows key-state reader. A single edge source
    /// avoids mixing delayed window events with native/accessibility input.
    pub fn begin_frame_with_keyboard(
        &mut self,
        shell: &mut GameShell,
        playing: bool,
        focused: bool,
        reader: Option<fn(i32) -> i16>,
    ) {
        self.poll(focused);
        if let Some(read) = reader {
            self.keyboard
                .get_or_insert_with(KeyboardFrame::default)
                .poll(focused, read);
        } else {
            self.keyboard = None;
        }
        shell.begin_frame_with_actions(
            playing,
            focused,
            self.shell_actions(shell.paused, |key| self.pressed(key)),
        );
    }
    pub fn pressed(&self, key: KeyCode) -> bool {
        self.focused
            && self
                .keyboard
                .as_ref()
                .map_or_else(|| is_key_pressed(key), |k| k.pressed.contains(&key))
    }
    pub fn down(&self, key: KeyCode) -> bool {
        self.focused
            && self
                .keyboard
                .as_ref()
                .map_or_else(|| is_key_down(key), |k| k.down.contains(&key))
    }

    /// Standard WASD/arrows, sprint, jump and crouch merged with analog input.
    pub fn movement(&self, shell: &GameShell) -> Movement {
        if !self.focused || !shell.playing() {
            return Movement::default();
        }
        let held = |a, b| f32::from(self.down(a) || self.down(b));
        let forward = held(KeyCode::W, KeyCode::Up) - held(KeyCode::S, KeyCode::Down);
        let right = held(KeyCode::D, KeyCode::Right) - held(KeyCode::A, KeyCode::Left);
        self.frame.movement(Movement {
            forward,
            right,
            sprint: self.down(KeyCode::LeftShift) || self.down(KeyCode::RightShift),
            jump: self.pressed(KeyCode::Space),
            crouch: self.down(KeyCode::C) || self.down(KeyCode::LeftControl),
        })
    }
    /// Radian deltas for Controller::look(..., 1.0, false).
    pub fn look_delta(&self, shell: &GameShell) -> [f32; 2] {
        if !self.focused || !shell.playing() {
            return [0.; 2];
        }
        let mouse = mouse_delta_position();
        let pad = self.frame.look_delta(get_frame_time());
        [-mouse.x * 2.5 + pad[0], -mouse.y * 2.5 + pad[1]]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input() -> ClientInput {
        ClientInput {
            backend: None,
            frame: GamepadFrame::default(),
            status: String::new(),
            focused: true,
            keyboard: None,
        }
    }
    #[test]
    fn controller_confirmation_does_not_leak_from_gameplay_into_menu() {
        let mut input = input();
        input.frame.button(Button::South, true);
        assert!(!input.shell_actions(false, |_| false).accept);
        assert!(input.shell_actions(true, |_| false).accept);
        assert!(input.navigation().accept);
        input.focused = false;
        assert!(!input.shell_actions(true, |_| true).accept);
    }
    #[test]
    fn start_back_and_dpad_route_to_shared_menus() {
        let mut input = input();
        input.frame.button(Button::East, true);
        input.frame.button(Button::DPadDown, true);
        assert!(!input.shell_actions(false, |_| false).pause);
        assert!(input.shell_actions(true, |_| false).pause);
        assert!(input.shell_actions(true, |_| false).next);
        input.frame.button(Button::Start, true);
        assert!(input.shell_actions(false, |_| false).pause);
    }
}

/// Pure key-edge tracking; the host owns the platform call.
#[derive(Default)]
struct KeyboardFrame {
    previous: std::collections::HashSet<KeyCode>,
    down: std::collections::HashSet<KeyCode>,
    pressed: std::collections::HashSet<KeyCode>,
}
impl KeyboardFrame {
    fn poll(&mut self, focused: bool, read: impl Fn(i32) -> i16) {
        self.down.clear();
        self.pressed.clear();
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
            (KeyCode::F, 0x46),
            (KeyCode::F11, 0x7A),
            (KeyCode::F3, 0x72),
            (KeyCode::Q, 0x51),
            (KeyCode::E, 0x45),
            (KeyCode::LeftAlt, 0xA4),
        ] {
            let value = read(vk) as u16;
            let held = value & 0x8000 != 0;
            let edge = !self.previous.contains(&key) && (held || value & 1 != 0);
            if focused && edge {
                self.pressed.insert(key);
            }
            if focused && (held || edge) {
                self.down.insert(key);
            }
            if held {
                self.previous.insert(key);
            } else {
                self.previous.remove(&key);
            }
        }
    }
}
#[cfg(test)]
mod keyboard_tests {
    use super::*;
    #[test]
    fn native_edges_are_consumed_once_and_focus_loss_does_not_replay_them() {
        let mut keys = KeyboardFrame::default();
        keys.poll(true, |vk| if vk == 0x45 { 0x8001u16 as i16 } else { 0 });
        assert!(keys.pressed.contains(&KeyCode::E));
        keys.poll(true, |vk| if vk == 0x45 { 0x8000u16 as i16 } else { 0 });
        assert!(!keys.pressed.contains(&KeyCode::E));
        keys.poll(false, |vk| if vk == 0x45 { 0x8001u16 as i16 } else { 0 });
        assert!(keys.down.is_empty());
        keys.poll(true, |vk| if vk == 0x45 { 0x8000u16 as i16 } else { 0 });
        assert!(!keys.pressed.contains(&KeyCode::E));
        keys.poll(true, |_| 0);
        keys.poll(true, |vk| i16::from(vk == 0x45));
        assert!(
            keys.pressed.contains(&KeyCode::E),
            "a complete tap between frames survives"
        );
    }
}
