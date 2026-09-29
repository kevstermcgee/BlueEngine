//! Debug wireframes for judging scale by eye: bounding boxes, axes and a person-sized silhouette.
//!
//! One unit is one metre. Draw a [`human_scale`] next to a model you generated and a mistake of a
//! factor of ten is obvious; [`bbox`] shows where a mesh really is. All of it is immediate-mode line
//! drawing to be called between `set_camera(..)` and the 2D pass, and costs nothing when not called.
//! The numeric twin, for an agent without a window, is `devkit::Bounds` (`describe`, `expect_longest`).
use super::batch::Template;
use crate::viewer::devkit::{Bounds, EYE_HEIGHT, HUMAN_HEIGHT};
use macroquad::prelude::*;

/// The box around a template's vertices after `transform`, in world metres; `None` for an empty one.
pub fn template_bounds(template: &Template, transform: Mat4) -> Option<Bounds> {
    Bounds::of(template.verts.iter().map(|v| {
        let p = transform.transform_point3(v.p);
        [p.x, p.y, p.z]
    }))
}

/// Draw the twelve edges of the box between `min` and `max`.
pub fn bbox(min: Vec3, max: Vec3, color: Color) {
    let c = |x: bool, y: bool, z: bool| {
        vec3(
            if x { max.x } else { min.x },
            if y { max.y } else { min.y },
            if z { max.z } else { min.z },
        )
    };
    for (a, b) in [
        (c(false, false, false), c(true, false, false)),
        (c(false, false, true), c(true, false, true)),
        (c(false, true, false), c(true, true, false)),
        (c(false, true, true), c(true, true, true)),
        (c(false, false, false), c(false, true, false)),
        (c(true, false, false), c(true, true, false)),
        (c(false, false, true), c(false, true, true)),
        (c(true, false, true), c(true, true, true)),
        (c(false, false, false), c(false, false, true)),
        (c(true, false, false), c(true, false, true)),
        (c(false, true, false), c(false, true, true)),
        (c(true, true, false), c(true, true, true)),
    ] {
        draw_line_3d(a, b, color);
    }
}

/// [`bbox`] around a template placed with `transform`. Returns the numeric bounds too, so the caller
/// can log or assert them.
pub fn template_bbox(template: &Template, transform: Mat4, color: Color) -> Option<Bounds> {
    let bounds = template_bounds(template, transform)?;
    bbox(
        vec3(bounds.min[0], bounds.min[1], bounds.min[2]),
        vec3(bounds.max[0], bounds.max[1], bounds.max[2]),
        color,
    );
    Some(bounds)
}

/// Red, green, blue lines along +X, +Y, +Z from `at`, each `length` metres.
pub fn axes(at: Vec3, length: f32) {
    draw_line_3d(at, at + vec3(length, 0., 0.), RED);
    draw_line_3d(at, at + vec3(0., length, 0.), GREEN);
    draw_line_3d(at, at + vec3(0., 0., length), BLUE);
}

/// A wire silhouette of an adult (1.75 m, eyes at 1.6 m) standing with its feet at `feet` and facing
/// `yaw` (0 faces -Z): legs, torso, arms, head. Put it beside a model to judge its size.
pub fn human_scale(feet: Vec3, yaw: f32, color: Color) {
    let turn = Mat3::from_rotation_y(-yaw);
    let at = |x: f32, y: f32, z: f32| feet + turn * vec3(x, y, z);
    // (centre, half extents): legs, hips-to-shoulders torso, arms.
    for (c, h) in [
        (vec3(-0.09, 0.44, 0.), vec3(0.07, 0.44, 0.07)),
        (vec3(0.09, 0.44, 0.), vec3(0.07, 0.44, 0.07)),
        (vec3(0., 1.16, 0.), vec3(0.19, 0.28, 0.10)),
        (vec3(-0.26, 1.14, 0.), vec3(0.05, 0.30, 0.05)),
        (vec3(0.26, 1.14, 0.), vec3(0.05, 0.30, 0.05)),
    ] {
        let corner = at(c.x, c.y, c.z);
        // The boxes are small; axis-aligned around the turned centre is close enough for a scale cue.
        bbox(corner - h, corner + h, color);
    }
    draw_sphere_wires(at(0., HUMAN_HEIGHT - 0.11, 0.), 0.11, None, color);
    // A short tick at eye height on the front of the face.
    draw_line_3d(
        at(-0.04, EYE_HEIGHT, -0.10),
        at(0.04, EYE_HEIGHT, -0.10),
        color,
    );
}
