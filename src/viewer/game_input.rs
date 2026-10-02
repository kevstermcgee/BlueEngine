//! Standard keyboard/mouse + native-controller adapter for standalone clients.
//! Own this alongside GameShell (never in thread-local storage): native device
//! workers must shut down before Windows TLS teardown. No OS calls live here.
use super::{
    controller::Movement,
    devkit::{EditKey, FrameClock, MenuNav, MenuStep, TextField},
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
    menu_nav: MenuNav,
    menu: MenuStep,
    /// Keys already reported as missing from the native table, so each is announced once.
    reported_unsupported: std::sync::Mutex<std::collections::HashSet<KeyCode>>,
    /// A text field was fed during the previous frame (see [`ClientInput::text_input_active`]); latched into `typing`
    /// at the start of the next frame, because the shell reads its hotkeys before the game draws its screen.
    typing_next: std::sync::atomic::AtomicBool,
    /// While true the plain letter hotkey (`F`, fullscreen) is ignored so typing a name does not toggle the window.
    typing: bool,
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
            menu_nav: MenuNav::default(),
            menu: MenuStep::default(),
            reported_unsupported: Default::default(),
            typing_next: Default::default(),
            typing: false,
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
        // Menu steps come from the D-pad and a flicked stick, with repeat; unfocused, nothing steps.
        self.menu = if focused {
            self.frame.menu_step(&mut self.menu_nav, self.clock.dt())
        } else {
            self.menu_nav.reset();
            MenuStep::default()
        };
    }
    /// The discrete menu step of this frame (D-pad or stick flick, with auto-repeat), for custom menus
    /// and sliders (`left`/`right`). Valid after [`ClientInput::poll`] / `begin_frame`.
    pub fn menu_step(&self) -> MenuStep {
        self.menu
    }
    /// True on the frame the controller's confirm button (South: A / Cross) goes down.
    pub fn menu_select(&self) -> bool {
        self.focused && self.frame.menu_select()
    }
    /// True on the frame the controller's back button (East: B / Circle) goes down.
    pub fn menu_back(&self) -> bool {
        self.focused && self.frame.menu_back()
    }
    pub fn status(&self) -> &str {
        &self.status
    }
    pub fn gamepad(&self) -> &GamepadFrame {
        &self.frame
    }
    pub fn navigation(&self) -> NavigationInput {
        NavigationInput {
            next: self.menu.down || self.menu.right,
            previous: self.menu.up || self.menu.left,
            accept: self.frame.pressed(Button::South),
        }
    }
    pub fn shell_actions(&self, paused: bool, keys: impl Fn(KeyCode) -> bool) -> ShellActions {
        if !self.focused {
            return ShellActions::default();
        }
        // The letter F toggles fullscreen, so it must not fire while the player is typing into a text field.
        let typing = self.typing;
        let mut a = ShellActions::from_keys(|key| keys(key) && !(typing && key == KeyCode::F));
        a.pause |=
            self.frame.pressed(Button::Start) || (paused && self.frame.pressed(Button::East));
        if paused {
            a.next |= self.frame.pressed(Button::DPadDown) || self.menu.down;
            a.previous |= self.frame.pressed(Button::DPadUp) || self.menu.up;
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
        let source = reader.as_ref().map(|r| r as &dyn Fn(i32) -> i16);
        self.begin_frame_with_key_source(shell, capture_cursor, focused, source);
    }
    /// As [`ClientInput::begin_frame_with_keyboard`] for any key-state source, not only a plain
    /// function: `source(virtual_key)` follows `GetAsyncKeyState` (bit 15 held, bit 0 pressed since the
    /// last query). A host or test that has no OS to ask passes a closure over its own key state.
    pub fn begin_frame_with_key_source(
        &mut self,
        shell: &mut GameShell,
        capture_cursor: bool,
        focused: bool,
        source: Option<&dyn Fn(i32) -> i16>,
    ) {
        self.poll_devices(focused, source);
        self.latch_typing();
        shell.begin_frame_with_actions(
            capture_cursor,
            focused,
            self.shell_actions(shell.paused, |key| self.pressed(key)),
        );
    }
    /// Start of a frame: whether a text field was fed last frame becomes this frame's shell rule.
    fn latch_typing(&mut self) {
        self.typing = self
            .typing_next
            .swap(false, std::sync::atomic::Ordering::Relaxed);
    }
    /// Say that a text field has the keyboard this frame, for games that draw their own text box and do not use
    /// [`ClientInput::feed_text`] (which calls this itself). The shell then ignores the plain `F` fullscreen hotkey
    /// on the next frame, so typing a name that contains an F does not flip the window. `F11`, `Esc` and the rest
    /// still work. Call it each frame the field is focused; it lapses by itself on the first frame you do not.
    pub fn text_input_active(&self) {
        self.typing_next
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
    /// The device half of a frame (clock, controller, native keys): everything but the window shell.
    fn poll_devices(&mut self, focused: bool, source: Option<&dyn Fn(i32) -> i16>) {
        self.clock.tick();
        self.poll(focused);
        if let Some(read) = source {
            self.keyboard
                .get_or_insert_with(KeyboardFrame::default)
                .poll(focused, read);
        } else {
            self.keyboard = None;
        }
    }
    /// True on the frame a key goes down. With a native reader installed (Windows), a key outside
    /// [`native_key_supported`] is announced once on stderr and trips a `debug_assert!`: it could never
    /// read as pressed, and that must not fail silently.
    pub fn pressed(&self, key: KeyCode) -> bool {
        self.check_native_key(key);
        self.focused
            && self
                .keyboard
                .as_ref()
                .map_or_else(|| is_key_pressed(key), |k| k.pressed.contains(&key))
    }
    /// True while a key is held; the same native-table rules as [`ClientInput::pressed`].
    pub fn down(&self, key: KeyCode) -> bool {
        self.check_native_key(key);
        self.focused
            && self
                .keyboard
                .as_ref()
                .map_or_else(|| is_key_down(key), |k| k.down.contains(&key))
    }
    /// True once per press while a game is over, for the shared "play again" convention: `R` (always a
    /// hotkey), Enter, or the controller's South (A / Cross) or Start button. False while the game is
    /// running or the window is unfocused. Call it every frame with the game's own over flag; a held
    /// key does not repeat, because each source is an edge.
    pub fn restart_requested(&self, game_over: bool) -> bool {
        game_over
            && self.focused
            && (self.pressed(KeyCode::R)
                || self.pressed(KeyCode::Enter)
                || self.frame.pressed(Button::South)
                || self.frame.pressed(Button::Start))
    }
    /// Records and announces (once per key) a query the native table cannot answer.
    fn check_native_key(&self, key: KeyCode) {
        if self.keyboard.is_none() || native_key_supported(key) {
            return;
        }
        if self.note_unsupported(key) {
            eprintln!("vesper3d: {}", unsupported_message(key));
        }
        debug_assert!(false, "{}", unsupported_message(key));
    }
    /// True the first time `key` is reported; later calls are quiet.
    fn note_unsupported(&self, key: KeyCode) -> bool {
        self.reported_unsupported
            .lock()
            .is_ok_and(|mut seen| seen.insert(key))
    }

    /// Standard WASD/arrows, sprint, jump and crouch merged with analog input. Read whenever the game should be taking
    /// input (window focused, menu closed): it does not need a captured mouse, so a game with no mouse look
    /// (`capture_cursor = false`) still moves. See [`GameShell::accepting_input`].
    pub fn movement(&self, shell: &GameShell) -> Movement {
        if !self.focused || !shell.accepting_input() {
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
    /// engine's dead zone); zero unless the game is accepting input. A stick needs no mouse capture.
    pub fn stick_look(&self, shell: &GameShell, seconds: f32) -> [f32; 2] {
        if !self.focused || !shell.accepting_input() {
            return [0.; 2];
        }
        self.frame.look_delta(seconds)
    }
    /// Feed this frame's typing, paste (Ctrl+V, Shift+Insert, Cmd+V) and editing keys into `field`. Key state
    /// goes through [`ClientInput::pressed`]/[`ClientInput::down`], so it follows the same focus rule as the
    /// rest of the input and works when the native Windows key reader is installed (macroquad's own key state
    /// is bypassed then). Call it only while the field has the keyboard, once per frame, and use
    /// [`ClientInput::frame_seconds`] for the repeat timing. Characters typed while the window is unfocused
    /// are discarded, not queued.
    pub fn feed_text(&self, field: &mut TextField) {
        self.text_input_active();
        let mut typed = Vec::new();
        while let Some(c) = get_char_pressed() {
            typed.push(c);
        }
        if !self.focused {
            return;
        }
        feed_text_with(
            field,
            &typed,
            self.frame_seconds(),
            |key| self.down(key),
            |key| self.pressed(key),
            paste_from_clipboard,
        );
    }
}

/// The text on the system clipboard, if there is any.
pub fn paste_from_clipboard() -> Option<String> {
    macroquad::miniquad::window::clipboard_get()
}

/// Put `text` on the system clipboard (a "copy invite" button).
pub fn copy_to_clipboard(text: &str) {
    macroquad::miniquad::window::clipboard_set(text);
}

impl TextField {
    /// [`ClientInput::feed_text`] reading macroquad's keyboard directly, for a game that does not use
    /// `ClientInput`. Assumes the window is focused.
    pub fn feed_frame(&mut self) {
        let mut typed = Vec::new();
        while let Some(c) = get_char_pressed() {
            typed.push(c);
        }
        feed_text_with(
            self,
            &typed,
            get_frame_time(),
            is_key_down,
            is_key_pressed,
            paste_from_clipboard,
        );
    }
}

/// The key handling behind both entry points, with every device read passed in so it is testable.
fn feed_text_with(
    field: &mut TextField,
    typed: &[char],
    dt: f32,
    down: impl Fn(KeyCode) -> bool,
    pressed: impl Fn(KeyCode) -> bool,
    clipboard: impl FnOnce() -> Option<String>,
) {
    let ctrl = down(KeyCode::LeftControl) || down(KeyCode::RightControl);
    let alt = down(KeyCode::LeftAlt) || down(KeyCode::RightAlt);
    let command = down(KeyCode::LeftSuper) || down(KeyCode::RightSuper);
    let shift = down(KeyCode::LeftShift) || down(KeyCode::RightShift);
    // AltGr arrives as Ctrl+Alt and types real characters (@ on many layouts), so only a bare Ctrl or Cmd
    // chord is a shortcut whose character must not be typed.
    let shortcut = (ctrl && !alt) || command;
    if !shortcut {
        for &c in typed {
            field.insert_char(c);
        }
    }
    let paste = (shortcut && pressed(KeyCode::V)) || (shift && pressed(KeyCode::Insert));
    if paste {
        if let Some(text) = clipboard() {
            field.insert_str(&text);
        }
    }
    if pressed(KeyCode::Home) {
        field.home();
    }
    if pressed(KeyCode::End) {
        field.end();
    }
    let held = [
        EditKey::Backspace,
        EditKey::Delete,
        EditKey::Left,
        EditKey::Right,
    ]
    .into_iter()
    .find(|key| {
        down(match key {
            EditKey::Backspace => KeyCode::Backspace,
            EditKey::Delete => KeyCode::Delete,
            EditKey::Left => KeyCode::Left,
            _ => KeyCode::Right,
        })
    });
    field.step_held(held, dt);
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
            menu_nav: MenuNav::default(),
            menu: MenuStep::default(),
            reported_unsupported: Default::default(),
            typing_next: Default::default(),
            typing: false,
        }
    }
    /// A keyboard that holds `down` (and reports `pressed` as this frame's edges), as the normal per-frame poll would.
    fn with_keys(down: &[KeyCode], pressed: &[KeyCode]) -> ClientInput {
        let mut input = input();
        input.keyboard = Some(KeyboardFrame {
            down: down.iter().copied().collect(),
            pressed: pressed.iter().copied().collect(),
            ..Default::default()
        });
        input
    }
    #[test]
    fn a_game_that_never_captures_the_mouse_still_receives_keys_and_sticks() {
        // `capture_cursor = false`: playing() is false for ever, accepting_input() is the right gate.
        let shell = GameShell::in_state(false, false, false);
        assert!(!shell.playing() && shell.accepting_input());
        let mut input = with_keys(&[KeyCode::W, KeyCode::D], &[KeyCode::Space]);
        let m = input.movement(&shell);
        assert!(m.forward > 0. && m.right > 0. && m.jump, "{m:?}");
        input.frame.right_stick = [1., 0.];
        assert!(input.stick_look(&shell, 1. / 60.)[0] > 0.);
        assert!(input.look_delta_with(&shell, 1. / 60.)[0] > 0.);
        // The mouse half does need capture, so an uncaptured cursor must not turn the camera.
        assert_eq!(input.mouse_look(&shell), [0., 0.]);
        // A controller alone works too.
        let mut pad = input_with_left_stick();
        assert!(pad.movement(&shell).forward > 0.);
        pad.focused = false;
        assert_eq!(pad.movement(&shell).forward, 0.);
    }
    fn input_with_left_stick() -> ClientInput {
        let mut input = with_keys(&[], &[]);
        input.frame.left_stick = [0., 1.];
        input
    }
    #[test]
    fn input_is_withheld_while_paused_unfocused_or_right_after_a_shell_key() {
        let keys = [KeyCode::W];
        for (label, shell) in [
            ("menu open", GameShell::in_state(true, true, false)),
            (
                "shell key just handled",
                GameShell::in_state(true, false, true),
            ),
        ] {
            let mut input = with_keys(&keys, &[]);
            input.frame.right_stick = [1., 0.];
            assert_eq!(input.movement(&shell).forward, 0., "{label}");
            assert_eq!(input.stick_look(&shell, 1. / 60.), [0., 0.], "{label}");
        }
        let shell = GameShell::in_state(true, false, false);
        let mut unfocused = with_keys(&keys, &[]);
        unfocused.focused = false;
        assert_eq!(unfocused.movement(&shell).forward, 0.);
        // A first-person game (mouse captured) moves as before.
        assert!(shell.playing() && with_keys(&keys, &[]).movement(&shell).forward > 0.);
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
    fn a_flicked_stick_moves_the_shared_menus_like_the_dpad() {
        let mut input = input();
        input.menu = MenuStep {
            down: true,
            ..Default::default()
        };
        assert!(input.navigation().next && !input.navigation().previous);
        assert!(input.shell_actions(true, |_| false).next);
        input.menu = MenuStep {
            left: true,
            ..Default::default()
        };
        assert!(input.navigation().previous);
    }
    #[test]
    fn typing_in_a_text_field_masks_the_f_hotkey_for_the_next_frame_only() {
        let mut input = input();
        let f = |key: KeyCode| key == KeyCode::F;
        assert!(
            input.shell_actions(false, f).fullscreen,
            "F is fullscreen when nobody is typing"
        );
        input.text_input_active();
        assert!(
            input.shell_actions(false, f).fullscreen,
            "the rule starts on the next frame, not this one"
        );
        input.latch_typing();
        assert!(
            !input.shell_actions(false, f).fullscreen,
            "typing an F must not toggle the window"
        );
        assert!(
            input
                .shell_actions(false, |key| key == KeyCode::F11)
                .fullscreen,
            "F11 still toggles"
        );
        assert!(
            input
                .shell_actions(false, |key| key == KeyCode::Escape)
                .pause,
            "Esc still pauses"
        );
        input.latch_typing();
        assert!(
            input.shell_actions(false, f).fullscreen,
            "the rule lapses on the first frame the field is not fed"
        );
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
    /// A fake OS key state: `GetAsyncKeyState` semantics (bit 15 held, bit 0 pressed since last query).
    fn fake_os(held: &std::cell::Cell<Option<i32>>) -> impl Fn(i32) -> i16 + '_ {
        move |vk| {
            if held.get() == Some(vk) {
                0x8000u16 as i16
            } else {
                0
            }
        }
    }
    #[test]
    fn a_native_frame_reports_a_restart_key_edge_exactly_once() {
        // End to end through ClientInput: source -> table -> edges -> pressed(), the path that
        // silently dropped R on Windows when it was missing from the table.
        let (mut input, held) = (input(), std::cell::Cell::new(None));
        let os = fake_os(&held);
        input.poll_devices(true, Some(&os));
        assert!(!input.pressed(KeyCode::R) && !input.down(KeyCode::R));
        held.set(Some(0x52));
        input.poll_devices(true, Some(&os));
        assert!(input.pressed(KeyCode::R) && input.down(KeyCode::R));
        input.poll_devices(true, Some(&os));
        assert!(!input.pressed(KeyCode::R), "a held key is one edge");
        assert!(input.down(KeyCode::R));
        held.set(None);
        input.poll_devices(true, Some(&os));
        assert!(!input.down(KeyCode::R));
        held.set(Some(0x52));
        input.poll_devices(true, Some(&os));
        assert!(input.pressed(KeyCode::R), "a second press is a second edge");
        // Shell keys flow through the same source.
        held.set(Some(0x1B));
        input.poll_devices(true, Some(&os));
        assert!(input.shell_actions(false, |k| input.pressed(k)).pause);
    }
    #[test]
    fn restart_is_only_requested_while_the_game_is_over() {
        let (mut input, held) = (input(), std::cell::Cell::new(None));
        let os = fake_os(&held);
        held.set(Some(0x52));
        input.poll_devices(true, Some(&os));
        assert!(
            !input.restart_requested(false),
            "R while playing is not a restart"
        );
        assert!(input.restart_requested(true));
        input.poll_devices(true, Some(&os));
        assert!(
            !input.restart_requested(true),
            "holding R restarts once, not every frame"
        );
        held.set(None);
        input.poll_devices(true, Some(&os));
        held.set(Some(0x0D));
        input.poll_devices(true, Some(&os));
        assert!(input.restart_requested(true), "Enter also restarts");
        assert!(!input.restart_requested(false));
        held.set(Some(0x52));
        input.poll_devices(false, Some(&os));
        assert!(
            !input.restart_requested(true),
            "an unfocused window never restarts"
        );
    }
    #[test]
    fn restart_accepts_the_controller() {
        let mut input = with_keys(&[], &[]);
        assert!(!input.restart_requested(true));
        input.frame.button(Button::South, true);
        assert!(input.restart_requested(true));
        assert!(!input.restart_requested(false));
        let mut start = with_keys(&[], &[]);
        start.frame.button(Button::Start, true);
        assert!(start.restart_requested(true));
        // A button that is not a confirm or Start does not restart.
        let mut other = with_keys(&[], &[]);
        other.frame.button(Button::West, true);
        assert!(!other.restart_requested(true));
        input.focused = false;
        assert!(!input.restart_requested(true));
    }
    #[test]
    fn a_missing_native_key_is_announced_once_per_key() {
        let input = with_keys(&[], &[]);
        assert!(input.note_unsupported(KeyCode::F20));
        assert!(!input.note_unsupported(KeyCode::F20));
        assert!(
            input.note_unsupported(KeyCode::KpEnter),
            "another key reports again"
        );
        assert!(!native_key_supported(KeyCode::F20) && native_key_supported(KeyCode::R));
    }
    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "not in the native key table")]
    fn asking_for_a_key_the_native_table_lacks_fails_loudly_in_debug() {
        with_keys(&[], &[]).pressed(KeyCode::F20);
    }
    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "not in the native key table")]
    fn held_queries_are_checked_too() {
        with_keys(&[], &[]).down(KeyCode::LeftSuper);
    }
}

/// Pure key-edge tracking; the host owns the platform call.
#[derive(Default)]
struct KeyboardFrame {
    previous: std::collections::HashSet<KeyCode>,
    down: std::collections::HashSet<KeyCode>,
    pressed: std::collections::HashSet<KeyCode>,
}
/// Every key the native reader reports. A key absent from this table reads as
/// silently never pressed on Windows (the macroquad fallback is bypassed once a
/// native reader is installed), so the table covers the full set a game might
/// bind — polling ~90 virtual keys per frame costs nothing.
pub const KEY_TABLE: &[(KeyCode, i32)] = &[
    (KeyCode::A, 0x41),
    (KeyCode::B, 0x42),
    (KeyCode::C, 0x43),
    (KeyCode::D, 0x44),
    (KeyCode::E, 0x45),
    (KeyCode::F, 0x46),
    (KeyCode::G, 0x47),
    (KeyCode::H, 0x48),
    (KeyCode::I, 0x49),
    (KeyCode::J, 0x4A),
    (KeyCode::K, 0x4B),
    (KeyCode::L, 0x4C),
    (KeyCode::M, 0x4D),
    (KeyCode::N, 0x4E),
    (KeyCode::O, 0x4F),
    (KeyCode::P, 0x50),
    (KeyCode::Q, 0x51),
    (KeyCode::R, 0x52),
    (KeyCode::S, 0x53),
    (KeyCode::T, 0x54),
    (KeyCode::U, 0x55),
    (KeyCode::V, 0x56),
    (KeyCode::W, 0x57),
    (KeyCode::X, 0x58),
    (KeyCode::Y, 0x59),
    (KeyCode::Z, 0x5A),
    (KeyCode::Key0, 0x30),
    (KeyCode::Key1, 0x31),
    (KeyCode::Key2, 0x32),
    (KeyCode::Key3, 0x33),
    (KeyCode::Key4, 0x34),
    (KeyCode::Key5, 0x35),
    (KeyCode::Key6, 0x36),
    (KeyCode::Key7, 0x37),
    (KeyCode::Key8, 0x38),
    (KeyCode::Key9, 0x39),
    (KeyCode::Up, 0x26),
    (KeyCode::Down, 0x28),
    (KeyCode::Left, 0x25),
    (KeyCode::Right, 0x27),
    (KeyCode::LeftShift, 0xA0),
    (KeyCode::RightShift, 0xA1),
    (KeyCode::LeftControl, 0xA2),
    (KeyCode::RightControl, 0xA3),
    (KeyCode::LeftAlt, 0xA4),
    (KeyCode::RightAlt, 0xA5),
    (KeyCode::Space, 0x20),
    (KeyCode::Enter, 0x0D),
    (KeyCode::Escape, 0x1B),
    (KeyCode::Tab, 0x09),
    (KeyCode::Backspace, 0x08),
    (KeyCode::Delete, 0x2E),
    (KeyCode::Insert, 0x2D),
    (KeyCode::Home, 0x24),
    (KeyCode::End, 0x23),
    (KeyCode::PageUp, 0x21),
    (KeyCode::PageDown, 0x22),
    (KeyCode::CapsLock, 0x14),
    (KeyCode::Minus, 0xBD),
    (KeyCode::Equal, 0xBB),
    (KeyCode::LeftBracket, 0xDB),
    (KeyCode::RightBracket, 0xDD),
    (KeyCode::Backslash, 0xDC),
    (KeyCode::Semicolon, 0xBA),
    (KeyCode::Apostrophe, 0xDE),
    (KeyCode::Comma, 0xBC),
    (KeyCode::Period, 0xBE),
    (KeyCode::Slash, 0xBF),
    (KeyCode::GraveAccent, 0xC0),
    (KeyCode::Pause, 0x13),
    (KeyCode::PrintScreen, 0x2C),
    (KeyCode::NumLock, 0x90),
    (KeyCode::ScrollLock, 0x91),
    (KeyCode::Kp0, 0x60),
    (KeyCode::Kp1, 0x61),
    (KeyCode::Kp2, 0x62),
    (KeyCode::Kp3, 0x63),
    (KeyCode::Kp4, 0x64),
    (KeyCode::Kp5, 0x65),
    (KeyCode::Kp6, 0x66),
    (KeyCode::Kp7, 0x67),
    (KeyCode::Kp8, 0x68),
    (KeyCode::Kp9, 0x69),
    (KeyCode::KpMultiply, 0x6A),
    (KeyCode::KpAdd, 0x6B),
    (KeyCode::KpSubtract, 0x6D),
    (KeyCode::KpDecimal, 0x6E),
    (KeyCode::KpDivide, 0x6F),
    (KeyCode::F1, 0x70),
    (KeyCode::F2, 0x71),
    (KeyCode::F3, 0x72),
    (KeyCode::F4, 0x73),
    (KeyCode::F5, 0x74),
    (KeyCode::F6, 0x75),
    (KeyCode::F7, 0x76),
    (KeyCode::F8, 0x77),
    (KeyCode::F9, 0x78),
    (KeyCode::F10, 0x79),
    (KeyCode::F11, 0x7A),
    (KeyCode::F12, 0x7B),
];
/// Keys a game could bind that the native reader deliberately does not report. Each one needs its own
/// virtual-key decision (a shared code, or a platform quirk) before it can join [`KEY_TABLE`]; until
/// then asking [`ClientInput::pressed`] about it is a bug the engine flags loudly.
pub const UNSUPPORTED_NATIVE_KEYS: &[KeyCode] = &[
    KeyCode::KpEnter, // Windows reports it as VK_RETURN (Enter) with an extended flag, not its own code.
    KeyCode::KpEqual,
    KeyCode::LeftSuper,
    KeyCode::RightSuper,
    KeyCode::Menu,
    KeyCode::Back,
    KeyCode::World1,
    KeyCode::World2,
    KeyCode::F13,
    KeyCode::F14,
    KeyCode::F15,
    KeyCode::F16,
    KeyCode::F17,
    KeyCode::F18,
    KeyCode::F19,
    KeyCode::F20,
    KeyCode::F21,
    KeyCode::F22,
    KeyCode::F23,
    KeyCode::F24,
    KeyCode::F25,
    KeyCode::Unknown,
];
/// Whether the native Windows reader reports `key` (it is in [`KEY_TABLE`]). A key outside it reads as
/// never pressed once a native reader is installed.
pub fn native_key_supported(key: KeyCode) -> bool {
    KEY_TABLE.iter().any(|&(k, _)| k == key)
}
fn unsupported_message(key: KeyCode) -> String {
    format!(
        "KeyCode::{key:?} is not in the native key table (game_input::KEY_TABLE), so it reads as \
         never pressed while a native key reader is installed (Windows). Bind another key or add it \
         to the table."
    )
}
impl KeyboardFrame {
    fn poll(&mut self, focused: bool, read: impl Fn(i32) -> i16) {
        self.down.clear();
        self.pressed.clear();
        for &(key, vk) in KEY_TABLE {
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
mod text_tests {
    use super::*;
    use crate::viewer::devkit::CharFilter;
    use std::collections::HashSet;

    fn feed(
        field: &mut TextField,
        typed: &str,
        keys: &[KeyCode],
        pressed: &[KeyCode],
        clip: Option<&str>,
    ) {
        let down: HashSet<KeyCode> = keys.iter().chain(pressed).copied().collect();
        let edge: HashSet<KeyCode> = pressed.iter().copied().collect();
        let typed: Vec<char> = typed.chars().collect();
        feed_text_with(
            field,
            &typed,
            1. / 60.,
            |k| down.contains(&k),
            |k| edge.contains(&k),
            || clip.map(str::to_string),
        );
    }

    #[test]
    fn ctrl_v_and_shift_insert_paste_and_the_control_character_is_not_typed() {
        let mut f = TextField::new(40, CharFilter::Address);
        feed(&mut f, "ab", &[], &[], None);
        feed(
            &mut f,
            "\u{16}",
            &[KeyCode::LeftControl],
            &[KeyCode::V],
            Some("play.example.com:27015\r\n"),
        );
        assert_eq!(f.as_str(), "abplay.example.com:27015");
        feed(
            &mut f,
            "",
            &[KeyCode::LeftShift],
            &[KeyCode::Insert],
            Some("-x"),
        );
        assert_eq!(f.as_str(), "abplay.example.com:27015-x");
        feed(&mut f, "", &[], &[KeyCode::V], Some("nope"));
        assert_eq!(f.len(), 26, "V alone is only a letter, and no char arrived");
        feed(&mut f, "", &[KeyCode::LeftControl], &[KeyCode::V], None);
        assert_eq!(f.len(), 26, "an empty clipboard pastes nothing");
    }

    #[test]
    fn ctrl_chords_do_not_type_but_altgr_does() {
        let mut f = TextField::new(40, CharFilter::Any);
        feed(&mut f, "a", &[KeyCode::LeftControl], &[KeyCode::A], None);
        assert!(f.is_empty(), "Ctrl+A is a shortcut");
        feed(
            &mut f,
            "@",
            &[KeyCode::LeftControl, KeyCode::RightAlt],
            &[],
            None,
        );
        assert_eq!(f.as_str(), "@", "AltGr (Ctrl+Alt) types");
    }

    #[test]
    fn editing_keys_move_and_delete() {
        let mut f = TextField::with_text(40, CharFilter::Any, "hello");
        feed(&mut f, "", &[], &[KeyCode::Home], None);
        assert_eq!(f.caret, 0);
        feed(&mut f, "", &[KeyCode::Delete], &[], None);
        assert_eq!(f.as_str(), "ello");
        feed(&mut f, "", &[], &[KeyCode::End], None);
        feed(&mut f, "", &[], &[], None);
        feed(&mut f, "", &[KeyCode::Backspace], &[], None);
        assert_eq!(f.as_str(), "ell");
        feed(&mut f, "", &[], &[], None);
        feed(&mut f, "", &[KeyCode::Left], &[], None);
        assert_eq!(f.caret, 2);
    }

    #[test]
    fn the_keys_the_glue_reads_are_in_the_native_table() {
        for key in [
            KeyCode::V,
            KeyCode::Insert,
            KeyCode::LeftControl,
            KeyCode::RightControl,
            KeyCode::LeftAlt,
            KeyCode::RightAlt,
            KeyCode::LeftShift,
            KeyCode::Backspace,
            KeyCode::Delete,
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::Home,
            KeyCode::End,
        ] {
            assert!(
                KEY_TABLE.iter().any(|&(k, _)| k == key),
                "{key:?} would never read on Windows"
            );
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
    #[test]
    fn native_table_covers_every_letter_and_digit() {
        // Games bind restart to R, tools to digits and letters; a key missing
        // here is silently never pressed on Windows (the bug behind "R doesn't
        // restart"), so the full alphanumeric set must stay covered.
        let keys: std::collections::HashSet<KeyCode> =
            KEY_TABLE.iter().map(|&(key, _)| key).collect();
        for key in [
            KeyCode::A,
            KeyCode::B,
            KeyCode::C,
            KeyCode::D,
            KeyCode::E,
            KeyCode::F,
            KeyCode::G,
            KeyCode::H,
            KeyCode::I,
            KeyCode::J,
            KeyCode::K,
            KeyCode::L,
            KeyCode::M,
            KeyCode::N,
            KeyCode::O,
            KeyCode::P,
            KeyCode::Q,
            KeyCode::R,
            KeyCode::S,
            KeyCode::T,
            KeyCode::U,
            KeyCode::V,
            KeyCode::W,
            KeyCode::X,
            KeyCode::Y,
            KeyCode::Z,
            KeyCode::Key0,
            KeyCode::Key1,
            KeyCode::Key2,
            KeyCode::Key3,
            KeyCode::Key4,
            KeyCode::Key5,
            KeyCode::Key6,
            KeyCode::Key7,
            KeyCode::Key8,
            KeyCode::Key9,
            KeyCode::F1,
            KeyCode::F2,
            KeyCode::F3,
            KeyCode::F4,
            KeyCode::F5,
            KeyCode::F6,
            KeyCode::F7,
            KeyCode::F8,
            KeyCode::F9,
            KeyCode::F10,
            KeyCode::F11,
            KeyCode::F12,
            KeyCode::Up,
            KeyCode::Down,
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::LeftShift,
            KeyCode::RightShift,
            KeyCode::LeftControl,
            KeyCode::RightControl,
            KeyCode::LeftAlt,
            KeyCode::RightAlt,
            KeyCode::Space,
            KeyCode::Enter,
            KeyCode::Escape,
            KeyCode::Tab,
        ] {
            assert!(keys.contains(&key), "KEY_TABLE is missing {key:?}");
            assert!(native_key_supported(key));
        }
        let vks: Vec<i32> = KEY_TABLE.iter().map(|&(_, vk)| vk).collect();
        let unique: std::collections::HashSet<i32> = vks.iter().copied().collect();
        assert_eq!(vks.len(), unique.len(), "duplicate virtual-key code");
        assert_eq!(keys.len(), vks.len(), "duplicate KeyCode entry");
    }
    #[test]
    fn every_key_is_in_the_table_or_explicitly_unsupported() {
        // All of macroquad's KeyCode variants. A new one (or a dropped table row) makes this fail
        // until it is classified, so a key can never be missing without anyone having decided so.
        use KeyCode::*;
        let all = [
            Space,
            Apostrophe,
            Comma,
            Minus,
            Period,
            Slash,
            Key0,
            Key1,
            Key2,
            Key3,
            Key4,
            Key5,
            Key6,
            Key7,
            Key8,
            Key9,
            Semicolon,
            Equal,
            A,
            B,
            C,
            D,
            E,
            F,
            G,
            H,
            I,
            J,
            K,
            L,
            M,
            N,
            O,
            P,
            Q,
            R,
            S,
            T,
            U,
            V,
            W,
            X,
            Y,
            Z,
            LeftBracket,
            Backslash,
            RightBracket,
            GraveAccent,
            World1,
            World2,
            Escape,
            Enter,
            Tab,
            Backspace,
            Insert,
            Delete,
            Right,
            Left,
            Down,
            Up,
            PageUp,
            PageDown,
            Home,
            End,
            CapsLock,
            ScrollLock,
            NumLock,
            PrintScreen,
            Pause,
            F1,
            F2,
            F3,
            F4,
            F5,
            F6,
            F7,
            F8,
            F9,
            F10,
            F11,
            F12,
            F13,
            F14,
            F15,
            F16,
            F17,
            F18,
            F19,
            F20,
            F21,
            F22,
            F23,
            F24,
            F25,
            Kp0,
            Kp1,
            Kp2,
            Kp3,
            Kp4,
            Kp5,
            Kp6,
            Kp7,
            Kp8,
            Kp9,
            KpDecimal,
            KpDivide,
            KpMultiply,
            KpSubtract,
            KpAdd,
            KpEnter,
            KpEqual,
            LeftShift,
            LeftControl,
            LeftAlt,
            LeftSuper,
            RightShift,
            RightControl,
            RightAlt,
            RightSuper,
            Menu,
            Back,
            Unknown,
        ];
        for key in all {
            assert_ne!(
                native_key_supported(key),
                UNSUPPORTED_NATIVE_KEYS.contains(&key),
                "{key:?} must be in exactly one of KEY_TABLE and UNSUPPORTED_NATIVE_KEYS"
            );
        }
        assert_eq!(KEY_TABLE.len() + UNSUPPORTED_NATIVE_KEYS.len(), all.len());
    }
}
