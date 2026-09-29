//! Standard keyboard/mouse + native-controller adapter for standalone clients.
//! Own this alongside GameShell (never in thread-local storage): native device
//! workers must shut down before Windows TLS teardown. No OS calls live here.
use super::{
    controller::Movement,
    devkit::FrameClock,
    game_client::{GameShell, ShellActions},
    game_ui::NavigationInput,
    gamepad::{Button, GamepadFrame, Gamepads},
};
use macroquad::prelude::*;

/// Hand motion since the last frame in screen pixels, `[right, down]`, the way an operating system
/// reports it. Feed it to `devkit::MouseLook::look`. This is the only place that knows macroquad's
/// `mouse_delta_position()` is previous-minus-current in half-screens; do not read that function
/// anywhere else (see `devkit::look` for the whole convention).
pub fn mouse_pixels() -> [f32; 2] {
    let d = mouse_delta_position();
    pixels_from_macroquad_delta([d.x, d.y], [screen_width(), screen_height()])
}
/// The pure half of [`mouse_pixels`]: macroquad's delta (`last - current`, `-1..1` across the window)
/// and the window size in pixels in, `[right, down]` pixels out.
pub fn pixels_from_macroquad_delta(delta: [f32; 2], window: [f32; 2]) -> [f32; 2] {
    [-delta[0] * window[0] * 0.5, -delta[1] * window[1] * 0.5]
}

pub struct ClientInput {
    backend: Option<Gamepads>,
    frame: GamepadFrame,
    status: String,
    focused: bool,
    keyboard: Option<KeyboardFrame>,
    clock: FrameClock,
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
            clock: FrameClock::new(),
        }
    }
    /// Length of the current frame in seconds: the wall-clock interval between `begin_frame` calls,
    /// clamped to 0.1 s. Prefer it to macroquad's `get_frame_time()`, which is stamped after the GL
    /// flush and reads 33 ms then 1 ms when one frame stalls although frames were presented evenly.
    /// Feed it to `GameSession::advance`, a `FixedStepper` or your camera.
    pub fn frame_seconds(&self) -> f32 {
        self.clock.dt()
    }
    /// The clock behind [`ClientInput::frame_seconds`], with its frame and hitch counters.
    pub fn frame_clock(&self) -> &FrameClock {
        &self.clock
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
    /// The standard frame entry point for new games: call it once at the top of every frame, including
    /// while paused. It starts the frame clock, polls devices and updates the shell.
    ///
    /// `capture_cursor` is whether the game wants the mouse captured while unpaused and focused. Pass
    /// `true` while playing and `false` on a title, menu or game-over screen so the cursor is released
    /// and clickable (an online session passes "connected"). Advanced hosts may poll and call
    /// `shell_actions` separately to route their own modal screens.
    pub fn begin_frame(&mut self, shell: &mut GameShell, capture_cursor: bool, focused: bool) {
        self.begin_frame_with_keyboard(shell, capture_cursor, focused, None);
    }
    /// Optional application-owned Windows key-state reader. A single edge source
    /// avoids mixing delayed window events with native/accessibility input.
    pub fn begin_frame_with_keyboard(
        &mut self,
        shell: &mut GameShell,
        capture_cursor: bool,
        focused: bool,
        reader: Option<fn(i32) -> i16>,
    ) {
        self.clock.tick();
        self.poll(focused);
        if let Some(read) = reader {
            self.keyboard
                .get_or_insert_with(KeyboardFrame::default)
                .poll(focused, read);
        } else {
            self.keyboard = None;
        }
        shell.begin_frame_with_actions(
            capture_cursor,
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
    /// Radian deltas for Controller::look(..., 1.0, false): mouse plus right stick, the stick scaled by
    /// [`ClientInput::frame_seconds`]. A game that wants its own stick sensitivity uses
    /// [`ClientInput::mouse_look`] and [`ClientInput::stick_look`] separately.
    pub fn look_delta(&self, shell: &GameShell) -> [f32; 2] {
        self.look_delta_with(shell, self.frame_seconds())
    }
    /// As [`ClientInput::look_delta`] for a frame of `seconds`: fixed-step captures and playback pass
    /// 1/60 so scripted runs do not depend on the wall clock.
    pub fn look_delta_with(&self, shell: &GameShell, seconds: f32) -> [f32; 2] {
        let (mouse, stick) = (self.mouse_look(shell), self.stick_look(shell, seconds));
        [mouse[0] + stick[0], mouse[1] + stick[1]]
    }
    /// Mouse motion this frame as radians for `Controller::look(.., 1.0, ..)`; zero unless playing.
    pub fn mouse_look(&self, shell: &GameShell) -> [f32; 2] {
        if !self.focused || !shell.playing() {
            return [0.; 2];
        }
        let mouse = mouse_delta_position();
        [-mouse.x * 2.5, -mouse.y * 2.5]
    }
    /// Right-stick look for a frame of `seconds` (radians; 2.5 rad/s at full deflection after the
    /// engine's dead zone); zero unless playing.
    pub fn stick_look(&self, shell: &GameShell, seconds: f32) -> [f32; 2] {
        if !self.focused || !shell.playing() {
            return [0.; 2];
        }
        self.frame.look_delta(seconds)
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
            clock: FrameClock::new(),
        }
    }
    #[test]
    fn macroquad_delta_becomes_right_down_pixels() {
        // Moving the hand 10 px right on a 1000x500 window: macroquad reports last - current, so the
        // x delta is negative (10 px = 0.02 of the half-width-normalised range).
        let px = pixels_from_macroquad_delta([-0.02, 0.], [1000., 500.]);
        assert!((px[0] - 10.).abs() < 1e-4 && px[1] == 0.);
        // Moving the hand 10 px up: y decreases on screen, so last - current is positive.
        let px = pixels_from_macroquad_delta([0., 0.04], [1000., 500.]);
        assert!((px[1] + 10.).abs() < 1e-4, "up is negative down-pixels");
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
    fn frame_seconds_come_from_the_engine_clock() {
        let mut input = input();
        assert_eq!(input.frame_seconds(), 1. / 60., "before the first frame");
        input.clock.tick_after(std::time::Duration::from_millis(20));
        assert!((input.frame_seconds() - 0.02).abs() < 1e-4);
        input.clock.tick_after(std::time::Duration::from_secs(3));
        assert_eq!(input.frame_seconds(), 0.1, "a stall is clamped to 100 ms");
        assert_eq!(input.frame_clock().frames(), 2);
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
            (KeyCode::F5, 0x74),
            (KeyCode::F9, 0x78),
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
