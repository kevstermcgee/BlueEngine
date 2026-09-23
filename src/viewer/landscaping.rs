//! Reusable, static low-poly planting. No textures, transparency or wind animation.
use super::room::Builder;
use crate::{math::V, scene::Shape};
pub(super) fn palette(b: &mut Builder) {
    for (name, color) in [
        ("bark", V(0.22, 0.105, 0.045)),
        ("bark-light", V(0.34, 0.18, 0.075)),
        ("foliage-dark", V(0.045, 0.17, 0.045)),
        ("foliage-mid", V(0.11, 0.29, 0.055)),
        ("foliage-light", V(0.22, 0.39, 0.075)),
        ("pine-dark", V(0.035, 0.16, 0.10)),
        ("pine-light", V(0.07, 0.27, 0.14)),
        ("flower-cream", V(0.96, 0.89, 0.68)),
        ("flower-pink", V(0.79, 0.10, 0.28)),
        ("flower-gold", V(0.99, 0.56, 0.055)),
        ("flower-center", V(0.36, 0.17, 0.025)),
    ] {
        b.material(name, color, 0., 0.);
    }
}
fn crown(b: &mut Builder, mat: &str, p: V, s: V) {
    b.add(Shape::Sphere, mat, p, s, V::ZERO);
}
pub(super) fn broadleaf(b: &mut Builder, p: V) {
    // A tapered trunk, visible fork, spreading branches and irregular leaf clusters.
    b.add(
        Shape::Cone,
        "bark",
        p + V(0., 1.55, 0.),
        V(0.25, 1.55, 0.24),
        V::ZERO,
    );
    b.obstacle(p + V(0., 1.1, 0.), V(0.20, 1.1, 0.20));
    for (offset, scale, rotation) in [
        (V(-0.45, 2.20, 0.), V(0.095, 0.86, 0.095), V(0., 0., 34.)),
        (V(0.46, 2.30, 0.), V(0.09, 0.96, 0.09), V(0., 0., -33.)),
        (V(0., 2.33, -0.48), V(0.08, 0.82, 0.08), V(-35., 0., 0.)),
        (V(0., 2.55, 0.42), V(0.07, 0.70, 0.07), V(32., 0., 0.)),
    ] {
        b.add(Shape::Cylinder, "bark-light", p + offset, scale, rotation);
    }
    for (offset, scale, mat) in [
        (V(-0.92, 3.05, 0.0), V(0.83, 0.66, 0.78), "foliage-dark"),
        (V(0.94, 3.21, 0.12), V(0.81, 0.75, 0.74), "foliage-mid"),
        (V(-0.20, 3.24, -0.82), V(0.95, 0.66, 0.72), "foliage-mid"),
        (V(0.04, 3.12, 0.79), V(0.92, 0.69, 0.74), "foliage-dark"),
        (V(-0.49, 3.72, 0.12), V(0.85, 0.74, 0.77), "foliage-mid"),
        (V(0.49, 3.80, -0.19), V(0.78, 0.68, 0.81), "foliage-light"),
        (V(-0.75, 3.53, -0.53), V(0.64, 0.60, 0.62), "foliage-light"),
    ] {
        crown(b, mat, p + offset, scale);
    }
    // Root flare meets the ground instead of a square pole planted in it.
    for angle in [0., 120., 240.] {
        b.add(
            Shape::Box,
            "bark",
            p + V(0., 0.065, 0.),
            V(0.34, 0.065, 0.075),
            V(0., angle, 0.),
        );
    }
}
pub(super) fn pine(b: &mut Builder, p: V) {
    b.add(
        Shape::Cylinder,
        "bark",
        p + V(0., 0.80, 0.),
        V(0.15, 0.80, 0.15),
        V::ZERO,
    );
    b.obstacle(p + V(0., 0.80, 0.), V(0.18, 0.80, 0.18));
    for (y, r, h, mat) in [
        (1.65, 1.0, 0.95, "pine-dark"),
        (2.4, 0.83, 0.92, "pine-light"),
        (3.1, 0.61, 0.84, "pine-dark"),
        (3.72, 0.39, 0.73, "pine-light"),
    ] {
        b.add(Shape::Cone, mat, p + V(0., y, 0.), V(r, h, r), V::ZERO);
    }
}
pub(super) fn shrub(b: &mut Builder, p: V, wide: bool, variant: usize) {
    let sx = if wide { 1.1 } else { 0.65 };
    b.cube("soil", p + V(0., 0.035, 0.), V(sx + 0.1, 0.035, 0.65));
    // Preserve established hiding-space collision cores and leave routes unchanged.
    b.obstacle(p + V(0., 0.65, 0.), V(sx * 0.78, 0.65, 0.45));
    for (x, y, z, size) in [
        (-0.50, 0.61, -0.09, 0.65),
        (0.47, 0.69, 0.08, 0.65),
        (0., 0.98, -0.10, 0.61),
        (-0.32, 1.04, 0.17, 0.46),
        (0.35, 1.18, -0.03, 0.42),
        (0., 0.62, 0.29, 0.52),
    ] {
        let mat = match (variant + (y * 10.) as usize) % 3 {
            0 => "foliage-dark",
            1 => "foliage-mid",
            _ => "foliage-light",
        };
        crown(
            b,
            mat,
            p + V(x * sx, y, z),
            V(size * sx, size * 0.76, size * 0.60),
        );
    }
    for x in [-0.3, 0.22] {
        b.add(
            Shape::Cylinder,
            "bark",
            p + V(x * sx, 0.26, 0.),
            V(0.035, 0.26, 0.035),
            V(0., 0., x * 30.),
        );
    }
    // Flowering edges add colour without blocking feet or changing collision routes.
    for i in 0..3 {
        flower(
            b,
            p + V((i as f32 - 1.) * sx * 0.65, 0.07, 0.54),
            variant + i,
        );
    }
}
pub(super) fn flower(b: &mut Builder, p: V, variant: usize) {
    let height = 0.26 + (variant % 3) as f32 * 0.065;
    b.cube(
        "foliage-dark",
        p + V(0., height * 0.5, 0.),
        V(0.009, height * 0.5, 0.009),
    );
    for side in [-1., 1.] {
        b.add(
            Shape::Box,
            "foliage-mid",
            p + V(side * 0.045, height * 0.45, 0.),
            V(0.055, 0.009, 0.021),
            V(0., side * 25., side * 22.),
        );
    }
    let mat = match variant % 3 {
        0 => "flower-cream",
        1 => "flower-pink",
        _ => "flower-gold",
    };
    for i in 0..5 {
        let angle = i as f32 * std::f32::consts::TAU / 5.;
        b.add(
            Shape::Box,
            mat,
            p + V(angle.cos() * 0.07, height, angle.sin() * 0.07),
            V(0.061, 0.015, 0.035),
            V(0., -angle.to_degrees(), 0.),
        );
    }
    b.add(
        Shape::Box,
        if variant.is_multiple_of(3) {
            "flower-gold"
        } else {
            "flower-center"
        },
        p + V(0., height + 0.019, 0.),
        V(0.035, 0.019, 0.035),
        V(0., 45., 0.),
    );
}
pub(super) fn flower_patch(b: &mut Builder, p: V, variant: usize) {
    b.cube("soil", p + V(0., 0.025, 0.), V(0.38, 0.025, 0.32));
    for (i, (x, z)) in [(-0.20, -0.12), (0.15, -0.08), (0., 0.16)]
        .into_iter()
        .enumerate()
    {
        flower(b, p + V(x, 0.05, z), variant + i);
    }
}
