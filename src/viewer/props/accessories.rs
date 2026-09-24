//! Original static design accessories, assembled from native matte primitives.
use super::{Builder, PropKind, Shape, V};
pub(super) fn palette(b: &mut Builder) {
    for (id, color) in [
        ("decor-cream", V(0.88, 0.81, 0.66)),
        ("decor-ink", V(0.045, 0.065, 0.07)),
        ("decor-clay", V(0.68, 0.24, 0.12)),
        ("decor-gold", V(0.95, 0.57, 0.15)),
        ("decor-sage", V(0.27, 0.43, 0.29)),
        ("decor-leaf", V(0.075, 0.24, 0.13)),
    ] {
        b.material(id, color, 0., 0.);
        b.scene.materials.get_mut(id).unwrap().roughness = 1.;
    }
}
pub(super) fn build(b: &mut Builder, kind: PropKind, p: V) {
    match kind {
        PropKind::FramedArt | PropKind::FramedBotanical => {
            // Front is +Z. Raised pigment is separated from the backing to avoid z fighting.
            b.cube("decor-cream", p + V(0., 0.4, 0.), V(0.515, 0.365, 0.015));
            for x in [-0.5325, 0.5325] {
                b.cube("prop-wood", p + V(x, 0.4, 0.), V(0.0175, 0.4, 0.055));
            }
            for y in [0.0175, 0.7825] {
                b.cube("prop-wood", p + V(0., y, 0.), V(0.515, 0.0175, 0.055));
            }
            if kind == PropKind::FramedArt {
                b.add(
                    Shape::Sphere,
                    "decor-gold",
                    p + V(0.18, 0.53, 0.023),
                    V(0.12, 0.12, 0.005),
                    V::ZERO,
                );
                b.cube("decor-sage", p + V(0., 0.20, 0.024), V(0.43, 0.085, 0.006));
                b.add(
                    Shape::Sphere,
                    "decor-clay",
                    p + V(-0.12, 0.29, 0.035),
                    V(0.28, 0.115, 0.005),
                    V::ZERO,
                );
            } else {
                b.add(
                    Shape::Box,
                    "decor-ink",
                    p + V(0., 0.4, 0.024),
                    V(0.009, 0.27, 0.006),
                    V(0., 0., -12.),
                );
                for (x, y, a) in [
                    (-0.09, 0.28, -45.),
                    (0.09, 0.38, 45.),
                    (-0.045, 0.49, -45.),
                    (0.12, 0.60, 45.),
                ] {
                    b.add(
                        Shape::Sphere,
                        "decor-leaf",
                        p + V(x, y, 0.035),
                        V(0.055, 0.11, 0.007),
                        V(0., 0., a),
                    );
                }
            }
        }
        PropKind::Sculpture => {
            b.cube("decor-ink", p + V(0., 0.035, 0.), V(0.25, 0.035, 0.19));
            // Continuous faceted oval, turned toward the seating area.
            for i in 0..16 {
                let a = i as f32 * std::f32::consts::TAU / 16.;
                let next = (i + 1) as f32 * std::f32::consts::TAU / 16.;
                let x = 0.155 * (a.cos() + next.cos()) * 0.5;
                let y = 0.435 + 0.285 * (a.sin() + next.sin()) * 0.5;
                let dx = 0.155 * (next.cos() - a.cos());
                let dy = 0.285 * (next.sin() - a.sin());
                b.add(
                    Shape::Cylinder,
                    "decor-clay",
                    p + V(
                        x * std::f32::consts::FRAC_1_SQRT_2,
                        y,
                        -x * std::f32::consts::FRAC_1_SQRT_2,
                    ),
                    V(0.052, (dx * dx + dy * dy).sqrt() * 0.56, 0.052),
                    V(0., 45., (-dx).atan2(dy).to_degrees()),
                );
            }
            b.cube("decor-clay", p + V(0., 0.11, 0.), V(0.065, 0.04, 0.065));
        }
        PropKind::VasePlant => {
            b.add(
                Shape::Sphere,
                "decor-cream",
                p + V(0., 0.16, 0.),
                V(0.15, 0.16, 0.15),
                V::ZERO,
            );
            b.add(
                Shape::Cylinder,
                "decor-cream",
                p + V(0., 0.30, 0.),
                V(0.075, 0.07, 0.075),
                V::ZERO,
            );
            b.add(
                Shape::Cylinder,
                "decor-ink",
                p + V(0., 0.372, 0.),
                V(0.055, 0.003, 0.055),
                V::ZERO,
            );
            for (x, z, h) in [(-0.15, 0., 0.69), (0.13, 0.08, 0.77), (0., -0.12, 0.84)] {
                b.add(
                    Shape::Cylinder,
                    "decor-leaf",
                    p + V(x * 0.5, (0.34 + h) * 0.5, z * 0.5),
                    V(0.009, (h - 0.34) * 0.53, 0.009),
                    V(z * 100., 0., -x * 100.),
                );
                for (side, y) in [(-1., h - 0.15), (1., h - 0.03)] {
                    b.add(
                        Shape::Sphere,
                        if side < 0. {
                            "decor-leaf"
                        } else {
                            "decor-sage"
                        },
                        p + V(x + side * 0.075, y, z),
                        V(0.125, 0.037, 0.065),
                        V(0., side * 25., side * 28.),
                    );
                }
            }
        }
        PropKind::Bowl => {
            b.add(
                Shape::Cylinder,
                "decor-sage",
                p + V(0., 0.018, 0.),
                V(0.20, 0.018, 0.20),
                V::ZERO,
            );
            for i in 0..16 {
                let a = i as f32 * std::f32::consts::TAU / 16.;
                b.add(
                    Shape::Box,
                    "decor-cream",
                    p + V(0.205 * a.cos(), 0.08, 0.205 * a.sin()),
                    V(0.043, 0.08, 0.022),
                    V(0., -a.to_degrees() - 90., 0.),
                );
            }
        }
        _ => unreachable!("accessory builder only"),
    }
}
