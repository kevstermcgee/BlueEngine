//! Rendering-free collision protection for third-person camera booms.
use crate::math::{Ray, V};
use crate::viewer::controller::Collider;
pub(crate) fn entry(ray: Ray, c: &Collider, radius: f32) -> Option<f32> {
    let mut near = 0_f32;
    let mut far = f32::INFINITY;
    for axis in 0..3 {
        let lo = c.min.axis(axis) - radius;
        let hi = c.max.axis(axis) + radius;
        let o = ray.o.axis(axis);
        let d = ray.d.axis(axis);
        if d.abs() < 1e-6 {
            if o < lo || o > hi {
                return None;
            }
        } else {
            let a = (lo - o) / d;
            let b = (hi - o) / d;
            near = near.max(a.min(b));
            far = far.min(a.max(b));
            if near > far {
                return None;
            }
        }
    }
    (far >= 0.).then_some(near)
}
/// Sphere-sweep a custom camera boom against current colliders, without constructing a Room.
/// This only places a presentation camera; it never changes movement or gameplay rays.
pub fn sweep_boom(anchor: V, desired: V, colliders: &[Collider], radius: f32) -> crate::Result<V> {
    if !anchor.finite() || !desired.finite() || !radius.is_finite() || !(0. ..=1.).contains(&radius)
    {
        return Err("camera boom needs finite positions and radius 0..1".into());
    }
    let delta = desired - anchor;
    let distance = delta.length();
    if !distance.is_finite() {
        return Err("camera boom distance is not finite".into());
    }
    if distance < 1e-6 {
        return Ok(anchor);
    }
    let ray = Ray {
        o: anchor,
        d: delta / distance,
    };
    let mut limit = distance;
    for collider in colliders {
        if let Some(t) = entry(ray, collider, radius) {
            limit = limit.min((t - 0.025).max(0.));
        }
    }
    Ok(ray.at(limit))
}
