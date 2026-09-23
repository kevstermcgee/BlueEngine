use crate::math::{Ray, V};

pub const EYE_HEIGHT: f32 = 1.68;
pub const RADIUS: f32 = 0.23;

#[derive(Clone, Debug)]
pub struct Collider {
    pub min: V,
    pub max: V,
}
impl Collider {
    pub fn blocks(&self, p: V) -> bool {
        if self.max.1 <= 0.12 || self.min.1 >= 1.85 {
            return false;
        }
        let x = p.0.clamp(self.min.0, self.max.0);
        let z = p.2.clamp(self.min.2, self.max.2);
        (p.0 - x).powi(2) + (p.2 - z).powi(2) < RADIUS * RADIUS
    }
    pub fn contains(&self, p: V) -> bool {
        p.0 >= self.min.0 - 0.01
            && p.0 <= self.max.0 + 0.01
            && p.1 >= self.min.1 - 0.01
            && p.1 <= self.max.1 + 0.01
            && p.2 >= self.min.2 - 0.01
            && p.2 <= self.max.2 + 0.01
    }
}

#[derive(Clone, Debug)]
pub struct Controller {
    pub position: V,
    pub yaw: f32,
    pub pitch: f32,
    velocity: V,
}
impl Default for Controller {
    fn default() -> Self {
        Self {
            position: V(0., EYE_HEIGHT, 4.6),
            yaw: -0.10,
            pitch: -0.035,
            velocity: V::ZERO,
        }
    }
}
impl Controller {
    pub fn direction(&self) -> V {
        V(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            -self.yaw.cos() * self.pitch.cos(),
        )
    }
    pub fn ray(&self) -> Ray {
        Ray {
            o: self.position,
            d: self.direction(),
        }
    }
    pub fn stop(&mut self) {
        self.velocity = V::ZERO;
    }
    pub fn look(&mut self, dx: f32, dy: f32, sensitivity: f32, invert: bool) {
        if !dx.is_finite() || !dy.is_finite() {
            return;
        }
        self.yaw = (self.yaw + dx * sensitivity).rem_euclid(std::f32::consts::TAU);
        self.pitch =
            (self.pitch - dy * sensitivity * if invert { -1. } else { 1. }).clamp(-1.50, 1.50);
    }
    pub fn step(&mut self, forward: f32, right: f32, fast: bool, dt: f32, colliders: &[Collider]) {
        if !dt.is_finite() || dt <= 0. || !forward.is_finite() || !right.is_finite() {
            return;
        }
        let dt = dt.min(0.1);
        let length = (forward * forward + right * right).sqrt().max(1.);
        let desired = V(
            self.yaw.sin() * forward + self.yaw.cos() * right,
            0.,
            -self.yaw.cos() * forward + self.yaw.sin() * right,
        ) * (if fast { 4.2 } else { 2.6 } / length);
        let steps = (dt / 0.008).ceil() as usize;
        let h = dt / steps as f32;
        for _ in 0..steps {
            self.velocity = self.velocity.lerp(desired, 1. - (-18. * h).exp());
            let d = self.velocity * h;
            let px = self.position + V(d.0, 0., 0.);
            if !colliders.iter().any(|c| c.blocks(px)) {
                self.position = px;
            } else {
                self.velocity.0 = 0.;
            }
            let pz = self.position + V(0., 0., d.2);
            if !colliders.iter().any(|c| c.blocks(pz)) {
                self.position = pz;
            } else {
                self.velocity.2 = 0.;
            }
        }
        self.position.1 = EYE_HEIGHT;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn walk(f: f32, r: f32, hz: usize) -> Controller {
        let mut c = Controller {
            yaw: 0.,
            ..Default::default()
        };
        for _ in 0..hz {
            c.step(f, r, false, 1. / hz as f32, &[]);
        }
        c
    }
    #[test]
    fn diagonal_and_frame_rate_independent() {
        let start = Controller::default().position;
        let a = (walk(1., 0., 60).position - start).length();
        let b = (walk(1., 1., 60).position - start).length();
        assert!((a - b).abs() < 0.001);
        assert!((a - (walk(1., 0., 144).position - start).length()).abs() < 0.02);
    }
    #[test]
    fn forward_tracks_yaw_and_ignores_pitch() {
        let mut c = Controller {
            yaw: std::f32::consts::FRAC_PI_2,
            pitch: 1.4,
            ..Default::default()
        };
        let p = c.position;
        for _ in 0..60 {
            c.step(1., 0., false, 1. / 60., &[]);
        }
        assert!(c.position.0 > p.0 + 2.);
        assert!((c.position.2 - p.2).abs() < 0.001);
        assert_eq!(c.position.1, EYE_HEIGHT);
    }
    #[test]
    fn collision_slides_and_long_frames_do_not_tunnel() {
        let wall = Collider {
            min: V(-100., 0., 2.),
            max: V(100., 3., 2.1),
        };
        let mut c = Controller {
            yaw: 0.,
            ..Default::default()
        };
        for _ in 0..100 {
            c.step(1., 0.5, true, 0.1, std::slice::from_ref(&wall));
        }
        assert!(c.position.2 >= 2.1 + RADIUS - 0.001);
        assert!(c.position.0 > 3.);
    }
    #[test]
    fn mouse_is_bounded_and_reversible() {
        let mut c = Controller::default();
        c.look(40., 20., 0.002, false);
        assert!(c.pitch < -0.035);
        c.look(0., 100000., 0.002, false);
        assert_eq!(c.pitch, -1.5);
        c.look(0., 100000., 0.002, true);
        assert_eq!(c.pitch, 1.5);
        assert!((c.direction().length() - 1.).abs() < 0.001);
    }
    #[test]
    fn stop_discards_momentum() {
        let mut c = walk(1., 0., 60);
        c.stop();
        let p = c.position;
        c.step(0., 0., false, 0.1, &[]);
        assert_eq!(c.position, p);
    }
    #[test]
    fn invalid_delta_is_ignored() {
        let mut c = Controller::default();
        let p = c.position;
        c.step(1., 0., false, f32::NAN, &[]);
        assert_eq!(p, c.position);
    }
}
