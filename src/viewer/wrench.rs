//! Frame-rate independent melee timing and closest-surface contact.
use super::room::Room;
use crate::math::{Ray, V};
pub const REACH: f32 = 1.65;
pub const CONTACT_TIME: f32 = 0.18;
pub const SWING_TIME: f32 = 0.52;
#[derive(Clone, Debug)]
pub struct Impact {
    pub point: V,
    pub normal: V,
    pub label: &'static str,
    pub age: f32,
}
#[derive(Default)]
pub struct Wrench {
    elapsed: Option<f32>,
    pub impact: Option<Impact>,
    pub hits: u32,
}
impl Wrench {
    pub fn start(&mut self, active: bool, settled: bool) -> bool {
        if !active || !settled || self.elapsed.is_some() {
            return false;
        }
        self.elapsed = Some(0.);
        true
    }
    pub fn cancel(&mut self) {
        self.elapsed = None;
    }
    pub fn phase(&self) -> Option<f32> {
        self.elapsed.map(|t| t / SWING_TIME)
    }
    pub fn tick(&mut self, dt: f32, room: &Room, ray: Ray) {
        if !dt.is_finite() || dt <= 0. {
            return;
        }
        if let Some(hit) = &mut self.impact {
            hit.age += dt;
        }
        if self.impact.as_ref().is_some_and(|h| h.age > 0.65) {
            self.impact = None;
        }
        let Some(old) = self.elapsed else {
            return;
        };
        let next = old + dt;
        // Crossing the contact instant also works when a frame spans the whole swing.
        if old < CONTACT_TIME && next >= CONTACT_TIME {
            if let Some(hit) = room.world.hit(ray, REACH, false) {
                self.hits += 1;
                self.impact = Some(Impact {
                    point: hit.p,
                    normal: hit.n,
                    label: room
                        .entities
                        .iter()
                        .find(|e| e.bounds.contains(hit.p))
                        .map_or("Surface", |e| e.label),
                    age: (next - CONTACT_TIME).min(0.65),
                });
            }
        }
        self.elapsed = (next < SWING_TIME).then_some(next);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::viewer::room;
    fn ray(x: f32) -> Ray {
        Ray {
            o: V(x, 1.34, -1.6),
            d: V(-1., 0., 0.),
        }
    }
    #[test]
    fn windup_single_contact_and_recovery() {
        let room = room::build().unwrap();
        let mut w = Wrench::default();
        assert!(!w.start(false, true));
        assert!(!w.start(true, false));
        assert!(w.start(true, true));
        assert!(!w.start(true, true));
        w.tick(0.1, &room, ray(-3.3));
        assert_eq!(w.hits, 0);
        w.tick(0.1, &room, ray(-3.3));
        assert_eq!(w.hits, 1);
        assert_eq!(
            w.impact.as_ref().unwrap().label,
            room.focus(ray(-3.3)).unwrap().label
        );
        w.tick(0.4, &room, ray(-3.3));
        assert_eq!(w.hits, 1);
        assert!(w.start(true, true));
    }
    #[test]
    fn range_occlusion_cancel_and_large_frames() {
        let room = room::build().unwrap();
        let mut w = Wrench::default();
        w.start(true, true);
        w.tick(0.6, &room, ray(0.));
        assert_eq!(w.hits, 0);
        w.start(true, true);
        w.cancel();
        w.tick(0.6, &room, ray(-3.3));
        assert_eq!(w.hits, 0);
        w.start(true, true);
        w.tick(0.6, &room, ray(-3.3));
        assert_eq!(w.hits, 1);
        w.start(true, true);
        w.tick(
            0.2,
            &room,
            Ray {
                o: V(-5.7, 1.34, -1.6),
                d: V(-1., 0., 0.),
            },
        );
        assert_eq!(w.impact.as_ref().unwrap().label, "Surface");
        assert!(w.impact.as_ref().unwrap().point.0 < -5.7);
    }
    #[test]
    fn contact_uses_current_aim_and_timing_is_frame_independent() {
        let room = room::build().unwrap();
        for fps in [30, 60, 144] {
            let mut w = Wrench::default();
            w.start(true, true);
            for _ in 0..fps {
                w.tick(1. / fps as f32, &room, ray(-3.3));
            }
            assert_eq!(w.hits, 1);
            assert!(w.phase().is_none());
        }
        let mut w = Wrench::default();
        w.start(true, true);
        w.tick(0.1, &room, ray(-3.3));
        w.tick(0.1, &room, ray(0.));
        assert_eq!(w.hits, 0);
    }
}
