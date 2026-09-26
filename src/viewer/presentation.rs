//! Bounded snapshot presentation, independent of graphics and authoritative physics.
use crate::math::V;
use std::collections::{BTreeMap, VecDeque};

#[derive(Default)]
pub struct PoseStream {
    samples: BTreeMap<u64, VecDeque<(f64, V)>>,
    tick: Option<u64>,
}
impl PoseStream {
    /// Reject old/duplicate snapshots. A discontinuity clears history (respawn/teleport).
    pub fn push(&mut self, tick: u64, now: f64, players: &[(u64, V)]) {
        if !now.is_finite() || self.tick.is_some_and(|old| tick <= old) {
            return;
        }
        self.tick = Some(tick);
        self.samples
            .retain(|id, _| players.iter().any(|(next, _)| next == id));
        for &(id, position) in players {
            if !position.finite() {
                continue;
            }
            let history = self.samples.entry(id).or_default();
            if history
                .back()
                .is_some_and(|&(t, p)| now <= t || now - t > 0.25 || (p - position).length() > 3.0)
            {
                history.clear();
            }
            history.push_back((now, position));
            while history.len() > 4 {
                history.pop_front();
            }
        }
    }
    /// Local extrapolation swept against the full player hull. Presentation must
    /// never push a valid authoritative pose through walls, ceilings or stair edges.
    pub fn collision_safe_position(
        &self,
        id: u64,
        now: f64,
        colliders: &[super::controller::Collider],
        eye_height: f32,
        height: f32,
        radius: f32,
    ) -> Option<V> {
        let &(_, origin) = self.samples.get(&id)?.back()?;
        let target = self.position(id, now, true)?;
        Some(sweep_eye(
            origin, target, colliders, eye_height, height, radius,
        ))
    }
    /// Remote actors interpolate 50 ms behind receipt. Local camera extrapolation is
    /// capped at 50 ms and freezes on packet loss; look angles remain client-owned.
    /// This is presentation smoothing, not authoritative input prediction.
    pub fn position(&self, id: u64, now: f64, local: bool) -> Option<V> {
        let history = self.samples.get(&id)?;
        let &(last_time, last) = history.back()?;
        if history.len() < 2 || !now.is_finite() {
            return Some(last);
        }
        let target = now - if local { 0.0 } else { 0.05 };
        for i in 1..history.len() {
            let (a_time, a) = history[i - 1];
            let (b_time, b) = history[i];
            if target <= b_time {
                let alpha = ((target - a_time) / (b_time - a_time)).clamp(0.0, 1.0) as f32;
                return Some(a.lerp(b, alpha));
            }
        }
        let (previous_time, previous) = history[history.len() - 2];
        let extra = ((target - last_time).clamp(0.0, 0.05) / (last_time - previous_time)) as f32;
        Some(last + (last - previous) * extra)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interpolation_extrapolation_ordering_and_discontinuities() {
        let mut stream = PoseStream::default();
        stream.push(3, 1.0, &[(1, V::ZERO)]);
        stream.push(6, 1.05, &[(1, V(1., 0., 0.))]);
        assert!((stream.position(1, 1.075, false).unwrap().0 - 0.5).abs() < 0.001);
        assert!((stream.position(1, 1.075, true).unwrap().0 - 1.5).abs() < 0.001);
        assert!((stream.position(1, 3., true).unwrap().0 - 2.).abs() < 0.001);
        stream.push(5, 1.06, &[(1, V(100., 0., 0.))]);
        assert!(stream.position(1, 1.075, true).unwrap().0 < 2.);
        stream.push(9, 1.1, &[(1, V(50., 0., 0.))]);
        assert_eq!(stream.position(1, 1.1, true), Some(V(50., 0., 0.)));
        stream.push(12, 1.15, &[]);
        assert!(stream.position(1, 1.2, true).is_none());
    }
}

/// Sweep a cylindrical character's conservative box from an authoritative eye pose.
/// Shrinking vertical contact planes slightly permits motion along a supporting floor.
pub fn sweep_eye(
    origin: V,
    target: V,
    colliders: &[super::controller::Collider],
    eye: f32,
    height: f32,
    radius: f32,
) -> V {
    let delta = target - origin;
    let mut fraction = 1.0_f32;
    for c in colliders {
        let lo = c.min - V(radius, height - eye - 0.002, radius);
        let hi = c.max + V(radius, eye - 0.002, radius);
        let mut enter = 0.0_f32;
        let mut leave = 1.0_f32;
        let mut hit = true;
        for (o, d, l, h) in [
            (origin.0, delta.0, lo.0, hi.0),
            (origin.1, delta.1, lo.1, hi.1),
            (origin.2, delta.2, lo.2, hi.2),
        ] {
            if d.abs() < 0.000001 {
                if o <= l || o >= h {
                    hit = false;
                    break;
                }
            } else {
                let a = (l - o) / d;
                let b = (h - o) / d;
                enter = enter.max(a.min(b));
                leave = leave.min(a.max(b));
                if enter > leave {
                    hit = false;
                    break;
                }
            }
        }
        if hit && leave > 0. && enter >= 0. {
            fraction = fraction.min((enter - 0.002).max(0.));
        }
    }
    origin + delta * fraction
}
#[cfg(test)]
mod collision_tests {
    use super::super::controller::Collider;
    use super::*;
    #[test]
    fn camera_stops_before_wall_and_floor() {
        let wall = Collider {
            min: V(1., 0., -2.),
            max: V(2., 4., 2.),
        };
        let p = sweep_eye(V(0., 1.56, 0.), V(2., 1.56, 0.), &[wall], 1.56, 1.72, 0.34);
        assert!(p.0 < 0.66 && p.0 > 0.6);
        let floor = Collider {
            min: V(-10., -1., -10.),
            max: V(10., 0., 10.),
        };
        let p = sweep_eye(
            V(0., 2., 0.),
            V(0., 1., 0.),
            std::slice::from_ref(&floor),
            1.56,
            1.72,
            0.34,
        );
        assert!(p.1 >= 1.55);
        let p = sweep_eye(V(0., 1.56, 0.), V(1., 1.56, 0.), &[floor], 1.56, 1.72, 0.34);
        assert_eq!(p.0, 1.);
    }
}
