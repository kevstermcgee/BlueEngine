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
        self.poll(focused);
        shell.begin_frame_with_actions(
            playing,
            focused,
            self.shell_actions(shell.paused, is_key_pressed),
        );
    }
    /// Standard WASD/arrows, sprint, jump and crouch merged with analog input.
    pub fn movement(&self, shell: &GameShell) -> Movement {
        if !self.focused || !shell.playing() {
            return Movement::default();
        }
        let (forward, right) = super::game_client::movement_axes();
        self.frame.movement(Movement {
            forward,
            right,
            sprint: is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift),
            jump: is_key_pressed(KeyCode::Space),
            crouch: is_key_down(KeyCode::C) || is_key_down(KeyCode::LeftControl),
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
