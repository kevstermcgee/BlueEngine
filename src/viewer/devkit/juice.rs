//! "Game feel" state: screen shake, hit-stop, FOV kick, screen flash and a landing dip.
//!
//! It is fed by simulation events and read by the renderer; the simulation never sees it. All
//! timers run in real seconds, so effects look the same at any frame rate, and nothing here depends
//! on a graphics library: [`Juice::camera_shake`] returns plain numbers a renderer applies to its camera.
use crate::math::V;

/// A value that jumps to a level and decays linearly back to zero: recoil, hurt vignette, hit marker.
///
/// ```
/// use vesper3d::viewer::devkit::Pulse;
/// let mut recoil = Pulse::default();
/// recoil.fire(1.0);
/// recoil.update(0.1, 5.0); // decays 5 units per second
/// assert!((recoil.value() - 0.5).abs() < 1e-6);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pulse(f32);

impl Pulse {
    /// Raise the pulse to at least `level` (it never lowers an active, stronger pulse).
    pub fn fire(&mut self, level: f32) {
        if level.is_finite() {
            self.0 = self.0.max(level);
        }
    }
    /// Decay by `rate` units per second.
    pub fn update(&mut self, dt: f32, rate: f32) {
        self.0 = (self.0 - dt * rate).max(0.);
    }
    /// Current level.
    pub fn value(&self) -> f32 {
        self.0
    }
}

/// Screen-space feel effects. Call [`Juice::update`] once per rendered frame with real seconds.
#[derive(Clone, Debug, Default)]
pub struct Juice {
    /// Screen-shake energy, 0-1 (the visible amount is trauma squared).
    pub trauma: f32,
    /// Extra field of view in degrees, decaying.
    pub fov_kick: f32,
    /// Real seconds of freeze-frame remaining.
    pub hitstop: f32,
    /// Full-screen flash colour (rgb 0-1) and remaining strength.
    pub flash_color: [f32; 3],
    pub flash: f32,
    /// Camera dip after a landing (metres, negative = down) and its spring velocity.
    pub dip: f32,
    dip_velocity: f32,
    /// Seconds since the effects started (drives the shake noise).
    pub time: f32,
}

/// Longest hit-stop one event can request, in seconds: longer freezes feel like a crash.
pub const MAX_HITSTOP: f32 = 0.12;

impl Juice {
    /// Add shake energy (saturates at 1).
    pub fn shake(&mut self, amount: f32) {
        if amount.is_finite() {
            self.trauma = (self.trauma + amount).clamp(0., 1.);
        }
    }
    /// Freeze the simulation clock briefly (clamped to [`MAX_HITSTOP`]).
    pub fn stop(&mut self, seconds: f32) {
        if seconds.is_finite() {
            self.hitstop = self.hitstop.max(seconds.clamp(0., MAX_HITSTOP));
        }
    }
    /// Kick the field of view outwards by `degrees`.
    pub fn kick(&mut self, degrees: f32) {
        if degrees.is_finite() {
            self.fov_kick = (self.fov_kick + degrees).clamp(0., 30.);
        }
    }
    /// Flash the whole screen with a colour at `strength` (0-1).
    pub fn flash(&mut self, color: [f32; 3], strength: f32) {
        if strength.is_finite() {
            self.flash_color = color;
            self.flash = self.flash.max(strength.clamp(0., 1.));
        }
    }
    /// Push the camera down as if landing from a fall; `amount` about 0.5 is a normal jump.
    pub fn land(&mut self, amount: f32) {
        if amount.is_finite() {
            self.dip_velocity -= amount.clamp(0., 3.) * 9.;
        }
    }
    /// Advance every timer by `dt` real seconds.
    pub fn update(&mut self, dt: f32) {
        let dt = if dt.is_finite() {
            dt.clamp(0., 0.1)
        } else {
            0.
        };
        self.time += dt;
        self.trauma = (self.trauma - dt * 1.7).max(0.);
        self.fov_kick *= (-dt * 7.).exp();
        self.hitstop = (self.hitstop - dt).max(0.);
        self.flash = (self.flash - dt * 4.).max(0.);
        // A damped spring pulls the landing dip back to zero.
        const STIFFNESS: f32 = 110.;
        const DAMPING: f32 = 13.;
        self.dip_velocity += (-STIFFNESS * self.dip - DAMPING * self.dip_velocity) * dt;
        self.dip += self.dip_velocity * dt;
    }
    /// Camera position offset (metres) and roll (radians) for the shake this instant. Zero once the
    /// trauma has decayed. Full trauma stays under 0.2 m / 0.05 rad, comfortable rather than nauseating.
    pub fn camera_shake(&self) -> (V, f32) {
        let amp = self.trauma * self.trauma;
        if amp < 1e-4 {
            return (V::ZERO, 0.);
        }
        let t = self.time;
        let noise = |f: f32, o: f32| (t * f + o).sin() * 0.6 + (t * f * 2.31 + o * 1.7).sin() * 0.4;
        (
            V(
                noise(27., 0.) * 0.07,
                noise(31., 2.) * 0.07,
                noise(23., 4.) * 0.05,
            ) * amp,
            noise(19., 6.) * 0.045 * amp,
        )
    }
    /// Time scale for the simulation clock: near zero during a hit-stop, `slow` (if given) during a
    /// slow-motion moment, otherwise 1.
    pub fn time_scale(&self, slow: Option<f32>) -> f32 {
        if self.hitstop > 0. {
            0.04
        } else {
            slow.map_or(1., |s| s.clamp(0.05, 1.))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trauma_saturates_and_decays_to_zero_shake() {
        let mut j = Juice::default();
        j.shake(0.7);
        j.shake(0.7);
        assert_eq!(j.trauma, 1.);
        j.time = 0.37;
        let (offset, roll) = j.camera_shake();
        assert!(offset.length() > 0. || roll != 0.);
        for _ in 0..120 {
            j.update(1. / 60.);
        }
        assert_eq!(j.trauma, 0.);
        assert_eq!(j.camera_shake(), (V::ZERO, 0.));
    }

    #[test]
    fn shake_amount_is_bounded() {
        let mut j = Juice::default();
        j.shake(1.);
        let mut worst = 0f32;
        for i in 0..600 {
            j.time = i as f32 * 0.01;
            let (offset, roll) = j.camera_shake();
            worst = worst.max(offset.length()).max(roll.abs());
        }
        assert!(worst < 0.2, "full trauma stays comfortable: {worst}");
    }

    #[test]
    fn hitstop_is_short_clamped_and_slows_time() {
        let mut j = Juice::default();
        j.stop(5.);
        assert!(j.hitstop <= MAX_HITSTOP);
        assert!(j.time_scale(None) < 0.1);
        for _ in 0..20 {
            j.update(1. / 60.);
        }
        assert_eq!(j.time_scale(None), 1.);
        assert!(j.time_scale(Some(0.4)) < 0.5);
        assert_eq!(
            j.time_scale(Some(0.0)),
            0.05,
            "slow motion never fully stops the clock"
        );
    }

    #[test]
    fn landing_dip_springs_back() {
        let mut j = Juice::default();
        j.land(1.);
        let mut deepest = 0f32;
        for _ in 0..30 {
            j.update(1. / 60.);
            deepest = deepest.min(j.dip);
        }
        assert!(deepest < -0.03, "dips: {deepest}");
        for _ in 0..120 {
            j.update(1. / 60.);
        }
        assert!(j.dip.abs() < 0.005, "settles: {}", j.dip);
    }

    #[test]
    fn kick_flash_and_pulses_decay_and_bad_input_is_ignored() {
        let mut j = Juice::default();
        j.kick(8.);
        j.flash([1., 0., 0.], 0.8);
        j.shake(f32::NAN);
        j.stop(f32::NAN);
        j.land(f32::INFINITY);
        j.update(f32::NAN);
        assert_eq!((j.trauma, j.hitstop, j.time), (0., 0., 0.));
        for _ in 0..180 {
            j.update(1. / 60.);
        }
        assert!(j.fov_kick < 0.01 && j.flash == 0.);
        let mut p = Pulse::default();
        p.fire(0.4);
        p.fire(0.2);
        assert_eq!(p.value(), 0.4, "a weaker pulse never lowers a stronger one");
        p.fire(f32::NAN);
        p.update(1., 1.);
        assert_eq!(p.value(), 0.);
    }
}
