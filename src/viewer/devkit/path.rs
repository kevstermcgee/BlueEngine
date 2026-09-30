//! A closed loop on the ground plane: race tracks, patrol routes, rails, a circular arena edge.
//!
//! [`ClosedPath`] turns a handful of control points into a smooth loop (closed Catmull-Rom, sampled) and
//! answers the questions a game keeps asking about it: how long is it, where is the point `s` metres along, which
//! way does it run there, and, for any position, how far along the loop is it and how far to the right of the
//! centreline ([`PathPoint`]). Progress along a loop is what makes lap counting, race positions, "am I on the
//! road", bot steering and wrong-way detection short. All of it is graphics-free and deterministic.
//!
//! Angles follow the engine's convention (see [`Controller`](crate::viewer::controller::Controller)): yaw 0
//! faces -Z and positive yaw turns towards +X, so the forward vector is `(sin yaw, -cos yaw)` in (x, z) and right
//! is `(cos yaw, sin yaw)`.
//!
//! ```
//! use vesper3d::viewer::devkit::path::{arc_delta_on, ClosedPath};
//! // A square-ish loop, 100 m on a side, driven anticlockwise seen from above (+Y).
//! let path = ClosedPath::from_control_points(&[(0., 0.), (100., 0.), (100., 100.), (0., 100.)], 12);
//! assert!((path.length() - 400.).abs() < 40., "about 400 m around: {}", path.length());
//! let start = path.point_at(0.);
//! assert!((start.0 - 0.).abs() < 1. && (start.2 - 0.).abs() < 1.);
//! let here = path.nearest_global(vesper3d::math::V(50., 0., 3.));
//! assert!(here.lateral.abs() < 5., "three metres off the centreline");
//! assert!((arc_delta_on(path.length(), 390., 10.) - 20.).abs() < 1e-3, "wraps through the start");
//! ```
use crate::math::V;
use std::f32::consts::{PI, TAU};

/// Forward direction for a yaw (yaw 0 faces -Z).
pub fn forward(yaw: f32) -> V {
    V(yaw.sin(), 0., -yaw.cos())
}

/// Right-hand direction for a yaw.
pub fn right(yaw: f32) -> V {
    V(yaw.cos(), 0., yaw.sin())
}

/// The yaw that faces along a horizontal `direction`.
pub fn yaw_of(direction: V) -> f32 {
    direction.0.atan2(-direction.2)
}

/// Wrap an angle to -PI..PI.
pub fn wrap_angle(a: f32) -> f32 {
    let mut a = a % TAU;
    if a > PI {
        a -= TAU;
    } else if a < -PI {
        a += TAU;
    }
    a
}

/// Signed arc distance from `from` to `to` around a loop of `length`, wrapped to (-length/2, length/2].
pub fn arc_delta_on(length: f32, from: f32, to: f32) -> f32 {
    let mut d = (to - from) % length;
    if d > length * 0.5 {
        d -= length;
    } else if d < -length * 0.5 {
        d += length;
    }
    d
}

/// Where a position sits relative to a [`ClosedPath`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathPoint {
    /// Index of the centreline segment.
    pub index: usize,
    /// Distance along the loop from its start, `0..length`.
    pub s: f32,
    /// Signed distance from the centreline, positive to the right of the direction of travel.
    pub lateral: f32,
    /// The closest point on the centreline.
    pub center: V,
    /// The direction of travel there.
    pub tangent: V,
}

/// A smooth closed loop on the ground plane (y = 0), sampled for fast queries.
#[derive(Clone, Debug)]
pub struct ClosedPath {
    samples: Vec<V>,
    /// Cumulative arc length at each sample; the last entry is the loop length.
    cum: Vec<f32>,
    length: f32,
}

fn catmull(p0: V, p1: V, p2: V, p3: V, t: f32) -> V {
    let (t2, t3) = (t * t, t * t * t);
    (p1 * 2.
        + (p2 - p0) * t
        + (p0 * 2. - p1 * 5. + p2 * 4. - p3) * t2
        + (p1 * 3. - p0 - p2 * 3. + p3) * t3)
        * 0.5
}

impl ClosedPath {
    /// A loop through `points` (x, z), closing from the last back to the first, with `subdivisions` samples
    /// per span. Needs at least three points. The path leaves the first point towards the second.
    pub fn from_control_points(points: &[(f32, f32)], subdivisions: usize) -> Self {
        assert!(
            points.len() >= 3,
            "a closed path needs at least three control points"
        );
        let pts: Vec<V> = points.iter().map(|&(x, z)| V(x, 0., z)).collect();
        let n = pts.len();
        let per = subdivisions.max(1);
        let mut samples = Vec::with_capacity(n * per);
        for i in 0..n {
            let (p0, p1, p2, p3) = (
                pts[(i + n - 1) % n],
                pts[i],
                pts[(i + 1) % n],
                pts[(i + 2) % n],
            );
            for k in 0..per {
                samples.push(catmull(p0, p1, p2, p3, k as f32 / per as f32));
            }
        }
        let mut cum = Vec::with_capacity(samples.len() + 1);
        let mut total = 0.;
        for i in 0..samples.len() {
            cum.push(total);
            total += (samples[(i + 1) % samples.len()] - samples[i]).length();
        }
        cum.push(total);
        Self {
            samples,
            cum,
            length: total,
        }
    }

    /// Loop length in metres.
    pub fn length(&self) -> f32 {
        self.length
    }

    /// The sampled centreline (the last sample joins the first).
    pub fn samples(&self) -> &[V] {
        &self.samples
    }

    fn segment(&self, index: usize) -> (V, V) {
        (
            self.samples[index],
            self.samples[(index + 1) % self.samples.len()],
        )
    }

    fn project(&self, pos: V, j: usize) -> (f32, PathPoint) {
        let (a, b) = self.segment(j);
        let ab = b - a;
        let len2 = ab.dot(ab).max(1e-6);
        let t = ((pos - a).dot(ab) / len2).clamp(0., 1.);
        let center = a + ab * t;
        let tangent = ab.norm();
        let off = pos - center;
        let off = V(off.0, 0., off.2);
        let lateral = off.dot(V(-tangent.2, 0., tangent.0));
        let s = self.cum[j] + t * (self.cum[j + 1] - self.cum[j]);
        (
            off.length(),
            PathPoint {
                index: j,
                s,
                lateral,
                center,
                tangent,
            },
        )
    }

    /// The closest point near segment `hint` (pass the previous answer's `index`): a cheap local search of
    /// `window` segments either side, right for something that moves a little each tick.
    pub fn nearest(&self, pos: V, hint: usize, window: usize) -> PathPoint {
        let n = self.samples.len();
        let window = window.min(n / 2);
        let mut best: Option<(f32, PathPoint)> = None;
        for step in 0..=(2 * window) {
            let j = (hint + n + step - window) % n;
            let candidate = self.project(pos, j);
            if best.as_ref().is_none_or(|b| candidate.0 < b.0) {
                best = Some(candidate);
            }
        }
        best.map(|b| b.1).expect("the window is never empty")
    }

    /// The closest point on the whole loop (for placing something, not for every tick).
    pub fn nearest_global(&self, pos: V) -> PathPoint {
        let mut best: Option<(f32, PathPoint)> = None;
        for j in 0..self.samples.len() {
            let candidate = self.project(pos, j);
            if best.as_ref().is_none_or(|b| candidate.0 < b.0) {
                best = Some(candidate);
            }
        }
        best.map(|b| b.1).expect("a path has samples")
    }

    fn locate(&self, s: f32) -> (usize, f32) {
        let s = s.rem_euclid(self.length);
        let j = match self
            .cum
            .binary_search_by(|c| c.partial_cmp(&s).unwrap_or(std::cmp::Ordering::Equal))
        {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        }
        .min(self.samples.len() - 1);
        let span = (self.cum[j + 1] - self.cum[j]).max(1e-6);
        (j, ((s - self.cum[j]) / span).clamp(0., 1.))
    }

    /// The centreline point `s` metres along the loop (wraps, negative counts back from the start).
    pub fn point_at(&self, s: f32) -> V {
        let (j, t) = self.locate(s);
        let (a, b) = self.segment(j);
        a + (b - a) * t
    }

    /// The direction of travel at `s`.
    pub fn tangent_at(&self, s: f32) -> V {
        let (j, _) = self.locate(s);
        let (a, b) = self.segment(j);
        (b - a).norm()
    }

    /// The point `lateral` metres to the right of the centreline at `s` (negative is left).
    pub fn offset_point(&self, s: f32, lateral: f32) -> V {
        let tangent = self.tangent_at(s);
        self.point_at(s) + V(-tangent.2, 0., tangent.0) * lateral
    }

    /// Signed arc distance from `from` to `to`, wrapped to (-length/2, length/2].
    pub fn arc_delta(&self, from: f32, to: f32) -> f32 {
        arc_delta_on(self.length, from, to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn circle(radius: f32, n: usize) -> ClosedPath {
        // Clockwise seen from above with +Y up and -Z forward: start at the top and go towards +X.
        let pts: Vec<(f32, f32)> = (0..n)
            .map(|i| {
                let a = i as f32 / n as f32 * TAU;
                (radius * a.sin(), -radius * a.cos())
            })
            .collect();
        ClosedPath::from_control_points(&pts, 16)
    }

    #[test]
    fn a_circle_has_the_right_length_and_a_constant_radius() {
        let p = circle(50., 12);
        assert!(
            (p.length() - TAU * 50.).abs() < 1.5,
            "length {}",
            p.length()
        );
        for s in [0., 40., 123., 250., p.length() - 1.] {
            let r = p.point_at(s).length();
            assert!((r - 50.).abs() < 0.5, "radius {r} at {s}");
        }
    }

    #[test]
    fn nearest_reports_progress_and_which_side_of_the_road_you_are_on() {
        let p = circle(50., 12);
        let start = p.point_at(0.);
        let on = p.nearest_global(start);
        assert!(on.s < 1. || on.s > p.length() - 1.);
        assert!(on.lateral.abs() < 0.1);
        // The loop runs clockwise from above, so its right-hand side is the inside.
        let inside = p.nearest_global(p.offset_point(100., 4.));
        assert!(
            inside.lateral > 3.5 && inside.lateral < 4.5,
            "{}",
            inside.lateral
        );
        assert!(inside.center.length() > 49. && inside.center.length() < 51.);
        let outside = p.nearest_global(p.offset_point(100., -4.));
        assert!(outside.lateral < -3.5);
        assert!((inside.s - 100.).abs() < 1.5);
    }

    #[test]
    fn the_local_search_follows_a_moving_point_and_agrees_with_the_global_one() {
        let p = circle(50., 12);
        let mut hint = p.nearest_global(p.point_at(0.)).index;
        for step in 0..300 {
            let s = step as f32 * 1.0;
            let pos = p.offset_point(s, 2.);
            let local = p.nearest(pos, hint, 10);
            let global = p.nearest_global(pos);
            assert!(
                (local.s - global.s).abs() < 0.5,
                "step {step}: {} vs {}",
                local.s,
                global.s
            );
            hint = local.index;
        }
    }

    #[test]
    fn arc_delta_wraps_through_the_start_in_both_directions() {
        assert!((arc_delta_on(400., 390., 10.) - 20.).abs() < 1e-4);
        assert!((arc_delta_on(400., 10., 390.) + 20.).abs() < 1e-4);
        assert!((arc_delta_on(400., 100., 150.) - 50.).abs() < 1e-4);
    }

    #[test]
    fn point_at_wraps_and_tangents_are_unit_and_follow_the_path() {
        let p = circle(50., 12);
        let l = p.length();
        assert!((p.point_at(0.) - p.point_at(l)).length() < 0.01);
        assert!((p.point_at(-10.) - p.point_at(l - 10.)).length() < 0.01);
        for s in [0., 30., 200.] {
            let t = p.tangent_at(s);
            assert!((t.length() - 1.).abs() < 1e-3);
            let ahead = p.point_at(s + 1.) - p.point_at(s);
            assert!(ahead.norm().dot(t) > 0.99);
        }
    }

    #[test]
    fn yaw_helpers_agree_with_each_other() {
        for yaw in [-3., -1., 0., 0.5, 2., 3.] {
            let f = forward(yaw);
            assert!((yaw_of(f) - yaw).abs() < 1e-4, "yaw {yaw}");
            assert!(f.dot(right(yaw)).abs() < 1e-5);
            assert!((wrap_angle(yaw + TAU) - yaw).abs() < 1e-4);
        }
        assert_eq!(forward(0.), V(0., 0., -1.));
    }
}
