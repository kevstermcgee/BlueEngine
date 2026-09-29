//! Discrete menu steps from a D-pad and an analog stick: flick once, hold to repeat.
//!
//! A menu needs *events* ("move the highlight down one row"), not a continuous axis. Turning a stick into
//! them takes a threshold, hysteresis so a wobbling thumb does not re-trigger, an initial delay before
//! repeating and a repeat rate; every game that hand-wrote this also hand-wrote its bugs. [`MenuNav`] is
//! that state machine, with no device or window dependency: feed it the stick, the D-pad and the frame
//! length, get a [`MenuStep`] back. `ClientInput` runs one for the shared menus
//! (`ClientInput::menu_step`); a custom menu can own its own.
//!
//! ```
//! use vesper3d::viewer::devkit::{MenuNav, MenuStep};
//! let mut nav = MenuNav::default();
//! // Stick Y is positive up, like `GamepadFrame::right_stick`. A flick down steps once...
//! let flick = nav.update(1. / 60., [0., -0.9], [false; 4]);
//! assert_eq!(flick, MenuStep { down: true, ..Default::default() });
//! assert!(!nav.update(1. / 60., [0., -0.9], [false; 4]).any());
//! // ...and holding it repeats after a delay, about every 0.12 s.
//! let repeats = (0..60).filter(|_| nav.update(1. / 60., [0., -0.9], [false; 4]).down).count();
//! assert!((5..=8).contains(&repeats), "{repeats}");
//! ```
/// Stick magnitude along an axis that starts a step.
pub const FLICK: f32 = 0.55;
/// Magnitude below which a held direction is considered let go (lower than [`FLICK`] on purpose).
pub const RELEASE: f32 = 0.35;
/// Seconds a direction is held before it starts repeating.
pub const REPEAT_DELAY: f32 = 0.40;
/// Seconds between repeats once repeating.
pub const REPEAT_INTERVAL: f32 = 0.12;

/// The steps taken this frame; at most one is true.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MenuStep {
    /// Move the highlight up.
    pub up: bool,
    /// Move the highlight down.
    pub down: bool,
    /// Move left (or decrease a slider).
    pub left: bool,
    /// Move right (or increase a slider).
    pub right: bool,
}

impl MenuStep {
    /// True when any direction stepped.
    pub fn any(&self) -> bool {
        self.up || self.down || self.left || self.right
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dir {
    Up,
    Down,
    Left,
    Right,
}

/// Turns stick and D-pad state, frame by frame, into [`MenuStep`]s.
#[derive(Clone, Copy, Debug, Default)]
pub struct MenuNav {
    held: Option<Dir>,
    timer: f32,
}

impl MenuNav {
    /// Forget any held direction (a menu opened or closed, focus was lost).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// One frame of `seconds`. `stick` is `[x, y]` with **Y positive up**; `dpad` is
    /// `[up, down, left, right]` held state. The D-pad wins over the stick. A direction steps on the
    /// frame it starts, again after [`REPEAT_DELAY`], then every [`REPEAT_INTERVAL`]. Non-finite
    /// input counts as centred.
    pub fn update(&mut self, seconds: f32, stick: [f32; 2], dpad: [bool; 4]) -> MenuStep {
        let dt = if seconds.is_finite() {
            seconds.clamp(0., 0.1)
        } else {
            0.
        };
        let Some(dir) = self.resolve(stick, dpad) else {
            self.reset();
            return MenuStep::default();
        };
        let fire = if self.held != Some(dir) {
            self.held = Some(dir);
            self.timer = REPEAT_DELAY;
            true
        } else {
            self.timer -= dt;
            if self.timer <= 0. {
                self.timer += REPEAT_INTERVAL;
                true
            } else {
                false
            }
        };
        let mut step = MenuStep::default();
        if fire {
            match dir {
                Dir::Up => step.up = true,
                Dir::Down => step.down = true,
                Dir::Left => step.left = true,
                Dir::Right => step.right = true,
            }
        }
        step
    }

    fn resolve(&self, stick: [f32; 2], dpad: [bool; 4]) -> Option<Dir> {
        for (pressed, dir) in dpad
            .into_iter()
            .zip([Dir::Up, Dir::Down, Dir::Left, Dir::Right])
        {
            if pressed {
                return Some(dir);
            }
        }
        let clean = |v: f32| if v.is_finite() { v.clamp(-1., 1.) } else { 0. };
        let (x, y) = (clean(stick[0]), clean(stick[1]));
        // Hysteresis: a direction already held stays while it is above RELEASE on its own axis and
        // still the dominant axis; a new one needs a real flick.
        if let Some(held) = self.held {
            let (along, across) = match held {
                Dir::Up => (y, x),
                Dir::Down => (-y, x),
                Dir::Left => (-x, y),
                Dir::Right => (x, y),
            };
            if along >= RELEASE && along >= across.abs() {
                return Some(held);
            }
        }
        if x.abs().max(y.abs()) < FLICK {
            return None;
        }
        Some(if y.abs() >= x.abs() {
            if y > 0. {
                Dir::Up
            } else {
                Dir::Down
            }
        } else if x > 0. {
            Dir::Right
        } else {
            Dir::Left
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const DT: f32 = 1. / 60.;
    const NONE: [bool; 4] = [false; 4];

    fn steps(nav: &mut MenuNav, seconds: f32, stick: [f32; 2]) -> Vec<f32> {
        let mut at = Vec::new();
        let mut t = 0.;
        while t < seconds {
            if nav.update(DT, stick, NONE).any() {
                at.push(t);
            }
            t += DT;
        }
        at
    }

    #[test]
    fn a_flick_steps_once_then_waits_for_the_delay_then_repeats() {
        let mut nav = MenuNav::default();
        let at = steps(&mut nav, 1., [0., -1.]);
        assert_eq!(at[0], 0., "immediately");
        assert!(
            at[1] >= REPEAT_DELAY - DT && at[1] <= REPEAT_DELAY + DT,
            "{at:?}"
        );
        assert!(
            (at[2] - at[1] - REPEAT_INTERVAL).abs() <= DT + 1e-4,
            "{at:?}"
        );
    }

    #[test]
    fn stick_up_is_up_and_the_dpad_wins() {
        let mut nav = MenuNav::default();
        assert!(nav.update(DT, [0., 1.], NONE).up);
        nav.reset();
        assert!(
            nav.update(DT, [0., 1.], [false, true, false, false]).down,
            "the D-pad beats the stick"
        );
        nav.reset();
        assert!(nav.update(DT, [-1., 0.], NONE).left);
        nav.reset();
        assert!(nav.update(DT, [1., 0.], NONE).right);
    }

    #[test]
    fn a_wobbling_thumb_does_not_retrigger() {
        let mut nav = MenuNav::default();
        assert!(nav.update(DT, [0., -0.9], NONE).down);
        // Drifting between the release and flick thresholds neither fires nor releases.
        for y in [-0.5, -0.6, -0.45, -0.7, -0.4] {
            assert!(!nav.update(DT, [0., y], NONE).any(), "y={y}");
        }
        // Let go, then a new flick steps immediately.
        assert!(!nav.update(DT, [0., 0.], NONE).any());
        assert!(nav.update(DT, [0., -0.9], NONE).down);
    }

    #[test]
    fn below_the_flick_threshold_nothing_happens() {
        let mut nav = MenuNav::default();
        assert!(steps(&mut nav, 1., [0., -0.5]).is_empty());
    }

    #[test]
    fn a_diagonal_takes_the_dominant_axis_and_a_change_steps_at_once() {
        let mut nav = MenuNav::default();
        assert!(nav.update(DT, [0.6, -0.9], NONE).down);
        assert!(
            nav.update(DT, [0.95, -0.4], NONE).right,
            "the new direction does not wait for the delay"
        );
    }

    #[test]
    fn hostile_input_is_centred() {
        let mut nav = MenuNav::default();
        assert!(!nav.update(f32::NAN, [f32::NAN, f32::INFINITY], NONE).any());
        assert!(!nav.update(DT, [f32::NAN, 0.], NONE).any());
    }
}
