//! Reusable static shelf and tabletop decor. No runtime light or clock behavior.
use super::{Builder, PropKind, Shape, V};
pub(super) fn palette(b: &mut Builder) {
    for (id, c) in [
        ("decor-paper", V(0.92, 0.9, 0.8)),
        ("decor-wine", V(0.35, 0.055, 0.08)),
        ("decor-wicker", V(0.58, 0.36, 0.16)),
    ] {
        b.material(id, c, 0., 0.);
        b.scene.materials.get_mut(id).unwrap().roughness = 1.;
    }
}
fn cylinder(b: &mut Builder, m: &str, p: V, s: V) {
    b.add(Shape::Cylinder, m, p, s, V::ZERO);
}
fn sphere(b: &mut Builder, m: &str, p: V, s: V) {
    b.add(Shape::Sphere, m, p, s, V::ZERO);
}
pub(super) fn build(b: &mut Builder, kind: PropKind, p: V) {
    match kind {
        PropKind::TableLamp => {
            cylinder(b, "decor-ink", p + V(0., 0.025, 0.), V(0.16, 0.025, 0.16));
            cylinder(b, "decor-gold", p + V(0., 0.25, 0.), V(0.025, 0.20, 0.025));
            cylinder(b, "decor-cream", p + V(0., 0.54, 0.), V(0.23, 0.14, 0.23));
            cylinder(b, "decor-gold", p + V(0., 0.69, 0.), V(0.035, 0.01, 0.035));
        }
        PropKind::BookStack => {
            for (i, m) in ["decor-wine", "decor-sage", "prop-blue"].iter().enumerate() {
                let y = i as f32 * 0.07;
                let x = if i == 1 { 0.02 } else { -0.01 };
                for h in [0.006, 0.064] {
                    b.cube(m, p + V(x, y + h, 0.), V(0.23, 0.006, 0.17));
                }
                b.cube(
                    "decor-paper",
                    p + V(x, y + 0.035, 0.008),
                    V(0.22, 0.023, 0.155),
                );
                b.cube(m, p + V(x, y + 0.035, -0.163), V(0.23, 0.023, 0.007));
            }
        }
        PropKind::CandleTrio => {
            b.cube("decor-ink", p + V(0., 0.018, 0.), V(0.25, 0.018, 0.18));
            for (x, z, h) in [
                (-0.14, -0.04, 0.27),
                (0.02, -0.06, 0.18),
                (0.13, 0.08, 0.12),
            ] {
                cylinder(
                    b,
                    "decor-cream",
                    p + V(x, 0.036 + h * 0.5, z),
                    V(0.055, h * 0.5, 0.055),
                );
                cylinder(
                    b,
                    "decor-ink",
                    p + V(x, 0.047 + h, z),
                    V(0.006, 0.011, 0.006),
                );
            }
        }
        PropKind::PottedCactus => {
            cylinder(b, "decor-clay", p + V(0., 0.11, 0.), V(0.15, 0.11, 0.15));
            cylinder(b, "prop-stem", p + V(0., 0.223, 0.), V(0.128, 0.003, 0.128));
            sphere(b, "decor-leaf", p + V(0., 0.48, 0.), V(0.085, 0.26, 0.075));
            for (x, y) in [(-0.13, 0.43), (0.14, 0.52)] {
                sphere(
                    b,
                    "decor-sage",
                    p + V(x * 0.5, y - 0.06, 0.),
                    V(0.10, 0.045, 0.05),
                );
                sphere(b, "decor-leaf", p + V(x, y, 0.), V(0.045, 0.10, 0.045));
            }
        }
        PropKind::FlowerVase => {
            sphere(b, "prop-blue", p + V(0., 0.14, 0.), V(0.13, 0.14, 0.13));
            cylinder(b, "prop-blue", p + V(0., 0.27, 0.), V(0.07, 0.06, 0.07));
            for (x, z, y) in [(-0.14, 0., 0.62), (0.13, 0.04, 0.68), (0., -0.11, 0.74)] {
                b.add(
                    Shape::Cylinder,
                    "decor-leaf",
                    p + V(x * 0.5, (y + 0.26) * 0.5, z * 0.5),
                    V(0.008, (y - 0.26) * 0.53, 0.008),
                    V(z * 100., 0., -x * 100.),
                );
                for i in 0..6 {
                    let a = i as f32 * std::f32::consts::TAU / 6.;
                    sphere(
                        b,
                        "decor-paper",
                        p + V(x + 0.06 * a.cos(), y, z + 0.06 * a.sin()),
                        V(0.045, 0.016, 0.045),
                    );
                }
                sphere(
                    b,
                    "decor-gold",
                    p + V(x, y + 0.019, z),
                    V(0.033, 0.016, 0.033),
                );
            }
        }
        PropKind::TallVase => {
            cylinder(
                b,
                "decor-clay",
                p + V(0., 0.055, 0.),
                V(0.115, 0.055, 0.115),
            );
            sphere(b, "decor-clay", p + V(0., 0.28, 0.), V(0.20, 0.24, 0.20));
            cylinder(b, "decor-clay", p + V(0., 0.54, 0.), V(0.09, 0.12, 0.09));
            cylinder(
                b,
                "decor-cream",
                p + V(0., 0.645, 0.),
                V(0.097, 0.015, 0.097),
            );
            cylinder(b, "decor-ink", p + V(0., 0.662, 0.), V(0.072, 0.002, 0.072));
        }
        PropKind::MantelClock => {
            b.cube("prop-wood", p + V(0., 0.03, 0.), V(0.29, 0.03, 0.12));
            b.add(
                Shape::Cylinder,
                "prop-wood",
                p + V(0., 0.24, 0.),
                V(0.20, 0.085, 0.20),
                V(90., 0., 0.),
            );
            b.add(
                Shape::Cylinder,
                "decor-paper",
                p + V(0., 0.24, 0.09),
                V(0.174, 0.006, 0.174),
                V(90., 0., 0.),
            );
            for i in 0..12 {
                let a = i as f32 * std::f32::consts::TAU / 12.;
                b.add(
                    Shape::Box,
                    "decor-ink",
                    p + V(0.146 * a.sin(), 0.24 + 0.146 * a.cos(), 0.100),
                    V(0.006, 0.012, 0.003),
                    V(0., 0., -a.to_degrees()),
                );
            }
            b.cube("decor-ink", p + V(0., 0.295, 0.108), V(0.008, 0.055, 0.004));
            b.cube(
                "decor-ink",
                p + V(0.038, 0.24, 0.108),
                V(0.038, 0.008, 0.004),
            );
        }
        PropKind::WovenBasket => {
            // Five abutting panels. Raised weave strips shared the wall's outer
            // planes and flickered; the plain silhouette needs no overlays.
            b.cube("decor-wicker", p + V(0., 0.018, 0.), V(0.3, 0.018, 0.23));
            for z in [-0.21, 0.21] {
                b.cube("decor-wicker", p + V(0., 0.207, z), V(0.30, 0.171, 0.02));
            }
            for x in [-0.28, 0.28] {
                b.cube("decor-wicker", p + V(x, 0.207, 0.), V(0.02, 0.171, 0.19));
            }
        }
        _ => unreachable!("library decor only"),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn basket_panels_abut_without_overlapping_volumes_or_trim() {
        let scene = super::super::scene(super::PropKind::WovenBasket);
        let compiled = crate::geometry::Compiled::new(scene, std::path::Path::new(".")).unwrap();
        let world = compiled.at(0.);
        assert_eq!(world.instances.len(), 5);
        for (i, a) in world.instances.iter().enumerate() {
            for b in world.instances.iter().skip(i + 1) {
                assert!((0..3).any(|axis| {
                    a.bounds.hi.axis(axis).min(b.bounds.hi.axis(axis))
                        - a.bounds.lo.axis(axis).max(b.bounds.lo.axis(axis))
                        <= 0.001
                }));
            }
        }
    }
}
