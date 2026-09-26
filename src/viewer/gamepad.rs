//! Native gamepads without a window/rendering dependency (`gamepad` feature).
//!
//! Poll once per rendered frame, even while paused/unfocused. The first connected
//! device owns the stock player's input until disconnected. Custom clients can
//! select another connected ID. Buttons are positional (South = Xbox A / PS Cross).
use super::controller::Movement;
pub use gilrs::{Axis, Button};
/// Session-local device identifier; it is not a persistent hardware ID.
pub type GamepadId = usize;
use gilrs::{EventType, Gilrs};
use std::collections::{HashMap, HashSet};

/// A single frame's normalized state. Stick Y is positive upwards.
#[derive(Clone, Debug, Default)]
pub struct GamepadFrame {
    pub left_stick: [f32; 2],
    pub right_stick: [f32; 2],
    /// Left and right trigger pressure in 0..=1.
    pub triggers: [f32; 2],
    down: HashSet<Button>,
    pressed: HashSet<Button>,
    released: HashSet<Button>,
}
impl GamepadFrame {
    pub fn down(&self, button: Button) -> bool {
        self.down.contains(&button)
    }
    /// A press survives a release in the same poll, and never repeats while held.
    pub fn pressed(&self, button: Button) -> bool {
        self.pressed.contains(&button)
    }
    pub fn released(&self, button: Button) -> bool {
        self.released.contains(&button)
    }
    /// Combine stock bindings with keyboard input; preserve analog magnitude and
    /// cap diagonals. South jumps, East crouches, left stick click sprints.
    pub fn movement(&self, mut keyboard: Movement) -> Movement {
        keyboard.forward += self.left_stick[1];
        keyboard.right += self.left_stick[0];
        let length = keyboard.forward.hypot(keyboard.right).max(1.);
        keyboard.forward /= length;
        keyboard.right /= length;
        keyboard.jump |= self.pressed(Button::South);
        keyboard.crouch |= self.down(Button::East);
        keyboard.sprint |= self.down(Button::LeftThumb);
        keyboard
    }
    /// Look deltas in radians for Controller::look with sensitivity 1.0.
    /// Positive stick Y looks up. Clamp long frames to avoid catch-up camera snaps.
    pub fn look_delta(&self, seconds: f32) -> [f32; 2] {
        let dt = if seconds.is_finite() {
            seconds.clamp(0., 0.1)
        } else {
            0.
        };
        [
            self.right_stick[0] * 2.5 * dt,
            -self.right_stick[1] * 2.5 * dt,
        ]
    }
    pub(super) fn button(&mut self, button: Button, down: bool) {
        if down {
            if self.down.insert(button) {
                self.pressed.insert(button);
            }
        } else if self.down.remove(&button) {
            self.released.insert(button);
        }
    }
    fn begin_frame(&mut self) {
        self.pressed.clear();
        self.released.clear();
    }
}

/// Continuous radial dead zone, rescaled to a unit circle; invalid samples are neutral.
fn stick(x: f32, y: f32) -> [f32; 2] {
    if !x.is_finite() || !y.is_finite() {
        return [0.; 2];
    }
    let x = x.clamp(-1., 1.);
    let y = y.clamp(-1., 1.);
    let length = x.hypot(y);
    const DEAD_ZONE: f32 = 0.18;
    if length <= DEAD_ZONE {
        return [0.; 2];
    }
    let scale = (length.min(1.) - DEAD_ZONE) / (1. - DEAD_ZONE) / length;
    [x * scale, y * scale]
}

/// Fallible native backend. Initialization failure should leave keyboard/mouse usable.
pub struct Gamepads {
    backend: Gilrs,
    input: GamepadInput,
}

#[derive(Default)]
struct GamepadInput {
    states: HashMap<GamepadId, GamepadFrame>,
    active: Option<GamepadId>,
    focused: bool,
}
impl Gamepads {
    pub fn new() -> Result<Self, Box<gilrs::Error>> {
        let backend = Gilrs::new().map_err(Box::new)?;
        let states = backend
            .gamepads()
            .map(|(id, _)| (usize::from(id), GamepadFrame::default()))
            .collect();
        Ok(Self {
            backend,
            input: GamepadInput {
                states,
                ..Default::default()
            },
        })
    }
    pub fn active(&self) -> Option<GamepadId> {
        self.input.active
    }
    /// Connected IDs and device names. Enumeration order is unspecified.
    pub fn connected(&self) -> impl Iterator<Item = (GamepadId, String)> + '_ {
        self.backend
            .gamepads()
            .map(|(id, pad)| (usize::from(id), pad.name().to_owned()))
    }
    /// Select a connected device; returns false for a disconnected/unknown ID.
    pub fn select(&mut self, id: GamepadId) -> bool {
        if self.input.states.contains_key(&id) {
            self.input.active = Some(id);
            self.input.focused = false;
            true
        } else {
            false
        }
    }
    /// Drain native events (including hot-plug) and return the active device state.
    /// Unfocused frames are neutral. Refocusing suppresses stale button edges.
    /// Disconnect clears all held state; a replacement device starts without edges.
    pub fn poll(&mut self, focused: bool) -> GamepadFrame {
        for state in self.input.states.values_mut() {
            state.begin_frame();
        }
        while let Some(event) = self.backend.next_event() {
            let id = usize::from(event.id);
            match event.event {
                EventType::Connected => {
                    self.input.states.insert(id, GamepadFrame::default());
                }
                EventType::Disconnected => {
                    self.input.states.remove(&id);
                    if self.input.active == Some(id) {
                        self.input.active = None;
                    }
                }
                EventType::ButtonPressed(button, _) => {
                    if let Some(state) = self.input.states.get_mut(&id) {
                        state.button(button, true);
                    }
                }
                EventType::ButtonReleased(button, _) => {
                    if let Some(state) = self.input.states.get_mut(&id) {
                        state.button(button, false);
                    }
                }
                _ => {}
            }
        }
        for (id, pad) in self.backend.gamepads() {
            if let Some(state) = self.input.states.get_mut(&usize::from(id)) {
                state.triggers = [Button::LeftTrigger2, Button::RightTrigger2].map(|button| {
                    pad.button_data(button).map_or(0., |data| {
                        let value = data.value();
                        if value.is_finite() {
                            value.clamp(0., 1.)
                        } else {
                            0.
                        }
                    })
                });
                state.left_stick = stick(pad.value(Axis::LeftStickX), pad.value(Axis::LeftStickY));
                state.right_stick =
                    stick(pad.value(Axis::RightStickX), pad.value(Axis::RightStickY));
            }
        }
        self.backend.inc();
        self.input.frame(focused)
    }
}
impl GamepadInput {
    fn frame(&mut self, focused: bool) -> GamepadFrame {
        let previous = self.active;
        if !self.active.is_some_and(|id| self.states.contains_key(&id)) {
            self.active = self.states.keys().copied().min();
        }
        let mut frame = self
            .active
            .and_then(|id| self.states.get(&id))
            .cloned()
            .unwrap_or_default();
        if !focused {
            frame = GamepadFrame::default();
        } else if !self.focused || previous != self.active {
            frame.begin_frame();
        }
        self.focused = focused;
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn focus_and_disconnect_neutralize_input_without_stale_edges() {
        let mut input = GamepadInput::default();
        let mut held = GamepadFrame {
            left_stick: [0., 1.],
            ..Default::default()
        };
        held.button(Button::South, true);
        input.states.insert(2, held.clone());
        assert!(!input.frame(true).pressed(Button::South)); // initial attachment
        assert_eq!(
            input.frame(false).movement(Movement::default()),
            Movement::default()
        );
        assert!(!input.frame(true).pressed(Button::South)); // focus restored
        input.states.insert(1, held);
        input.frame(true);
        assert_eq!(input.active, Some(2)); // another pad cannot steal control
        input.states.remove(&2);
        assert!(!input.frame(true).pressed(Button::South)); // replacement
        assert_eq!(input.active, Some(1));
        input.states.clear();
        assert_eq!(
            input.frame(true).movement(Movement::default()),
            Movement::default()
        );
        assert_eq!(input.active, None);
    }
    #[test]
    fn radial_deadzone_preserves_analog_speed_and_bounds_diagonals() {
        assert_eq!(stick(0.1, -0.1), [0.; 2]);
        assert_eq!(stick(f32::NAN, 1.), [0.; 2]);
        let half = stick(0.59, 0.);
        assert!((half[0] - 0.5).abs() < 0.0001);
        let diagonal = stick(1., 1.);
        assert!((diagonal[0].hypot(diagonal[1]) - 1.).abs() < 0.0001);
    }
    #[test]
    fn taps_survive_one_frame_and_held_buttons_do_not_repeat() {
        let mut frame = GamepadFrame::default();
        frame.button(Button::South, true);
        frame.button(Button::South, false);
        assert!(frame.pressed(Button::South) && frame.released(Button::South));
        assert!(!frame.down(Button::South));
        assert!(frame.movement(Movement::default()).jump);
        frame.begin_frame();
        assert!(!frame.pressed(Button::South));
        frame.button(Button::South, true);
        frame.begin_frame();
        frame.button(Button::South, true);
        assert!(frame.down(Button::South));
        assert!(!frame.pressed(Button::South));
    }
    #[test]
    fn mixed_movement_is_bounded_and_keyboard_actions_survive() {
        let frame = GamepadFrame {
            left_stick: [0.5, 0.5],
            ..Default::default()
        };
        let movement = frame.movement(Movement {
            forward: 1.,
            right: 1.,
            jump: true,
            ..Default::default()
        });
        assert!((movement.forward.hypot(movement.right) - 1.).abs() < 0.0001);
        assert!(movement.jump);
        assert_eq!(
            GamepadFrame::default().movement(Movement::default()),
            Movement::default()
        );
    }
    #[test]
    fn look_rate_is_independent_of_render_frequency() {
        let frame = GamepadFrame {
            right_stick: [1., 1.],
            ..Default::default()
        };
        for hz in [30., 60., 144.] {
            let delta = frame.look_delta(1. / hz);
            assert!((delta[0] * hz - 2.5).abs() < 0.0001);
            assert!((delta[1] * hz + 2.5).abs() < 0.0001);
        }
        assert_eq!(frame.look_delta(f32::NAN), [0., -0.]);
    }
}
