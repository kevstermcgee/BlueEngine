//! The one mouse-look convention: which way the camera turns for which way the hand moves.
//!
//! A first-person game gets this wrong easily because three conventions meet: an operating system
//! reports mouse motion as +x right, +y down; macroquad's `mouse_delta_position()` reports the
//! *previous position minus the current one* in half-screens (so +x is left and +y is up); and the
//! engine's angles are yaw 0 = facing -Z, positive yaw turning towards +X, pitch up-positive. An agent
//! cannot feel a mouse, so it cannot tell which of the eight sign combinations is right by trying.
//!
//! Everything here is expressed in one direction-of-the-hand vocabulary and is unit-tested against
//! [`Controller::look`](crate::viewer::controller::Controller::look):
//!
//! * a **look delta** is `[right, down]` in radians: how far the view turns right and how far it
//!   tilts down. This is what [`Tick::look`](super::Tick::look), `ClientInput::mouse_look` and
//!   `Controller::look(dx, dy, 1.0, false)` already carry;
//! * moving the mouse right turns the view right, moving it up looks up (`invert_y` reverses only the
//!   vertical axis);
//! * pitch is clamped to [`PITCH_LIMIT`], the same limit the controller and the arena body use, so a
//!   camera built from this type never disagrees with the simulation at the extremes.
//!
//! A gamepad's right stick uses the same vocabulary through [`stick_look`]: stick right turns right,
//! stick up looks up (stick Y is positive up, as `GamepadFrame::right_stick` reports it), scaled by
//! [`STICK_RADIANS_PER_SECOND`] and the frame length. `GamepadFrame::look_delta` is that function, so
//! mouse and stick add into one `[right, down]` delta. If a stick feels mirrored, the camera's forward
//! vector is not `(sin(yaw), .., -cos(yaw))`: use [`FpsCamera::forward`] instead of your own.
//!
//! Getting pixels from the window is the one part that needs macroquad: use
//! `vesper3d::viewer::game_input::mouse_pixels()`, which undoes macroquad's sign and unit quirks, and
//! never read `mouse_delta_position()` directly.
//!
//! ```
//! use vesper3d::viewer::devkit::{FpsCamera, MouseLook};
//! let mouse = MouseLook::default();
//! let mut cam = FpsCamera::default(); // yaw 0 faces -Z
//! cam.turn(mouse.look(200., 0.));     // the hand moved 200 px right
//! assert!(cam.forward().0 > 0.);      // the view now points towards +X
//! cam.turn(mouse.look(0., -100.));    // the hand moved 100 px up
//! assert!(cam.forward().1 > 0.);      // the view tilts up
//! cam.turn_stick([1., 0.], 0.1);      // the right stick fully right for one 0.1 s frame
//! assert!(cam.forward().0 > 0.);
//! ```
use crate::math::V;

/// The largest pitch magnitude in radians (about 86 degrees). Matches `Controller::look` and
/// `ArenaBody::look`, so the camera and the simulation clamp identically.
pub const PITCH_LIMIT: f32 = 1.5;
/// Default turn rate: radians per pixel of hand motion (about 0.14 degrees, a 1600-pixel sweep is
/// roughly a half turn).
pub const DEFAULT_RADIANS_PER_PIXEL: f32 = 0.0025;
/// Turn rate of a right stick held fully over, in radians per second.
pub const STICK_RADIANS_PER_SECOND: f32 = 2.5;
/// Sensitivity is clamped to this range so a corrupt settings file cannot freeze or spin the view.
pub const SENSITIVITY_RANGE: (f32, f32) = (0.0002, 0.02);

/// Turns hand motion in screen pixels into a look delta.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MouseLook {
    radians_per_pixel: f32,
    /// True when moving the mouse up looks down (flight-stick style). The horizontal axis is never
    /// inverted.
    pub invert_y: bool,
}

impl Default for MouseLook {
    fn default() -> Self {
        Self {
            radians_per_pixel: DEFAULT_RADIANS_PER_PIXEL,
            invert_y: false,
        }
    }
}

impl MouseLook {
    /// A converter with `radians_per_pixel` clamped to [`SENSITIVITY_RANGE`]; a non-finite value
    /// becomes the default.
    pub fn new(radians_per_pixel: f32, invert_y: bool) -> Self {
        let mut look = Self {
            radians_per_pixel: DEFAULT_RADIANS_PER_PIXEL,
            invert_y,
        };
        look.set_sensitivity(radians_per_pixel);
        look
    }
    /// Radians the view turns per pixel of hand motion.
    pub fn sensitivity(&self) -> f32 {
        self.radians_per_pixel
    }
    /// Set the turn rate, clamped to [`SENSITIVITY_RANGE`]; a non-finite value is ignored.
    pub fn set_sensitivity(&mut self, radians_per_pixel: f32) {
        if radians_per_pixel.is_finite() {
            self.radians_per_pixel =
                radians_per_pixel.clamp(SENSITIVITY_RANGE.0, SENSITIVITY_RANGE.1);
        }
    }
    /// A look delta `[right, down]` (radians) for hand motion of `right_px` pixels right and
    /// `down_px` pixels down, the way an operating system reports it. Non-finite input is zero.
    pub fn look(&self, right_px: f32, down_px: f32) -> [f32; 2] {
        let flip = if self.invert_y { -1. } else { 1. };
        [
            finite_or_zero(right_px) * self.radians_per_pixel,
            finite_or_zero(down_px) * self.radians_per_pixel * flip,
        ]
    }
}

/// A look delta `[right, down]` (radians) for a right stick held at `stick` (`[x, y]`, Y positive up)
/// for `seconds`. The frame length is clamped to 0.1 s so a hitch does not snap the camera; non-finite
/// input is zero.
pub fn stick_look(stick: [f32; 2], seconds: f32) -> [f32; 2] {
    let dt = if seconds.is_finite() {
        seconds.clamp(0., 0.1)
    } else {
        0.
    };
    let axis = |v: f32| if v.is_finite() { v } else { 0. };
    [
        axis(stick[0]) * STICK_RADIANS_PER_SECOND * dt,
        -axis(stick[1]) * STICK_RADIANS_PER_SECOND * dt,
    ]
}

fn finite_or_zero(v: f32) -> f32 {
    if v.is_finite() {
        v
    } else {
        0.
    }
}

/// A first-person orientation: yaw (0 faces -Z, positive turns towards +X) and pitch (up-positive,
/// within [`PITCH_LIMIT`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FpsCamera {
    /// Yaw in radians, kept in `0..TAU`.
    pub yaw: f32,
    /// Pitch in radians, kept within `-PITCH_LIMIT..=PITCH_LIMIT`.
    pub pitch: f32,
}

impl FpsCamera {
    /// A camera with the given orientation (pitch is clamped, yaw wrapped).
    pub fn new(yaw: f32, pitch: f32) -> Self {
        let mut cam = Self::default();
        cam.turn([yaw, -pitch]);
        cam
    }
    /// Apply a look delta `[right, down]` in radians. Non-finite components are ignored.
    pub fn turn(&mut self, look: [f32; 2]) {
        if look[0].is_finite() {
            self.yaw = (self.yaw + look[0]).rem_euclid(std::f32::consts::TAU);
        }
        if look[1].is_finite() {
            self.pitch = (self.pitch - look[1]).clamp(-PITCH_LIMIT, PITCH_LIMIT);
        }
    }
    /// Turn by hand motion in pixels (`+x` right, `+y` down); shorthand for
    /// `turn(mouse.look(right_px, down_px))`.
    pub fn turn_pixels(&mut self, mouse: &MouseLook, right_px: f32, down_px: f32) {
        self.turn(mouse.look(right_px, down_px));
    }
    /// Turn by a right stick held at `stick` (`[x, y]`, Y positive up) for `seconds`; shorthand for
    /// `turn(stick_look(stick, seconds))`.
    pub fn turn_stick(&mut self, stick: [f32; 2], seconds: f32) {
        self.turn(stick_look(stick, seconds));
    }
    /// Unit vector the camera looks along.
    pub fn forward(&self) -> V {
        V(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            -self.yaw.cos() * self.pitch.cos(),
        )
    }
    /// Horizontal unit vector to walk along for "forward" (ignores pitch).
    pub fn walk_forward(&self) -> V {
        V(self.yaw.sin(), 0., -self.yaw.cos())
    }
    /// Horizontal unit vector to the camera's right.
    pub fn walk_right(&self) -> V {
        V(self.yaw.cos(), 0., self.yaw.sin())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viewer::controller::Controller;

    #[test]
    fn right_turns_right_and_up_looks_up() {
        let mouse = MouseLook::default();
        let mut cam = FpsCamera::default();
        assert!(cam.forward().2 < -0.99, "yaw 0 faces -Z");
        cam.turn(mouse.look(400., 0.));
        assert!(
            cam.forward().0 > 0.5,
            "a hand moving right turns towards +X"
        );
        assert!(cam.walk_right().2 > 0., "and the right-hand side follows");
        let level = cam.pitch;
        cam.turn(mouse.look(0., -100.));
        assert!(
            cam.pitch > level && cam.forward().1 > 0.,
            "a hand moving up looks up"
        );
        cam.turn(mouse.look(0., 300.));
        assert!(cam.forward().1 < 0., "a hand moving down looks down");
    }

    #[test]
    fn the_stick_turns_the_way_the_hand_does() {
        let mut cam = FpsCamera::default();
        for _ in 0..5 {
            cam.turn_stick([1., 0.], 0.1); // half a second of held stick, in frames
        }
        assert!(
            cam.forward().0 > 0.5,
            "stick right turns towards +X, like a mouse moving right"
        );
        assert!(cam.walk_right().2 > 0.);
        let level = cam.pitch;
        cam.turn_stick([0., 1.], 0.1);
        assert!(
            cam.pitch > level && cam.forward().1 > 0.,
            "stick up looks up"
        );
        cam.turn_stick([0., -1.], 0.1);
        cam.turn_stick([0., -1.], 0.1);
        assert!(cam.pitch < level, "stick down looks down");
        // Frame-rate independent, and a hitch cannot snap the view.
        for hz in [30., 60., 144.] {
            let d = stick_look([1., 0.], 1. / hz);
            assert!((d[0] * hz - STICK_RADIANS_PER_SECOND).abs() < 1e-3);
        }
        assert_eq!(stick_look([1., 0.], 5.)[0], STICK_RADIANS_PER_SECOND * 0.1);
        assert_eq!(stick_look([f32::NAN, f32::INFINITY], f32::NAN), [0., 0.]);
    }

    #[test]
    fn invert_flips_only_the_vertical_axis() {
        let (normal, inverted) = (MouseLook::new(0.003, false), MouseLook::new(0.003, true));
        let (a, b) = (normal.look(10., 10.), inverted.look(10., 10.));
        assert_eq!(a[0], b[0]);
        assert_eq!(a[1], -b[1]);
    }

    #[test]
    fn pitch_is_limited_and_yaw_wraps() {
        let mut cam = FpsCamera::default();
        cam.turn([0., -1000.]);
        assert_eq!(cam.pitch, PITCH_LIMIT);
        cam.turn([0., 1000.]);
        assert_eq!(cam.pitch, -PITCH_LIMIT);
        cam.turn([-0.1, 0.]);
        assert!(
            (0. ..std::f32::consts::TAU).contains(&cam.yaw),
            "yaw stays in 0..TAU"
        );
        assert!((cam.forward().length() - 1.).abs() < 1e-5);
    }

    #[test]
    fn hostile_input_changes_nothing() {
        let mouse = MouseLook::default();
        let mut cam = FpsCamera::new(1., 0.2);
        let before = cam;
        cam.turn([f32::NAN, f32::INFINITY]);
        cam.turn_pixels(&mouse, f32::NAN, f32::NEG_INFINITY);
        assert_eq!(cam, before);
        let mut m = MouseLook::default();
        m.set_sensitivity(f32::NAN);
        assert_eq!(m.sensitivity(), DEFAULT_RADIANS_PER_PIXEL);
        m.set_sensitivity(100.);
        assert_eq!(m.sensitivity(), SENSITIVITY_RANGE.1);
        m.set_sensitivity(0.);
        assert_eq!(m.sensitivity(), SENSITIVITY_RANGE.0);
    }

    /// The camera and the simulation's own controller must turn identically for the same delta, or a
    /// game that aims with one and steers with the other drifts apart.
    #[test]
    fn agrees_with_the_controller_it_feeds() {
        let mouse = MouseLook::default();
        let mut controller = Controller::default();
        let mut cam = FpsCamera::new(controller.yaw, controller.pitch);
        for (x, y) in [
            (120., -40.),
            (-300., 15.),
            (5., -900.),
            (0., 2000.),
            (2500., 0.),
        ] {
            let look = mouse.look(x, y);
            cam.turn(look);
            controller.look(look[0], look[1], 1., false);
            assert!(
                (cam.yaw - controller.yaw).abs() < 1e-4,
                "yaw {} vs {}",
                cam.yaw,
                controller.yaw
            );
            assert!((cam.pitch - controller.pitch).abs() < 1e-6, "pitch");
        }
    }
}
