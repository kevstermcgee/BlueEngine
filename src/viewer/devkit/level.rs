//! Straight walls with doorway gaps cut out, for a custom-sim game that builds its own indoor level in
//! code instead of authoring a `MapDocument` map. Pairs with [`super::WaypointGraph`] for AI that needs
//! to path between the rooms these walls make.
use crate::math::V;
use crate::viewer::controller::Collider;

/// Solid sub-ranges of `[lo, hi]` after removing every range in `gaps`: a 0..10 run with one gap at
/// 4..6 becomes two segments, 0..4 and 6..10. The building block both wall functions use; exposed
/// because a game may want the same cut on something that is not a [`Collider`] (a floor seam, a UI
/// layout).
pub fn solid_ranges(lo: f32, hi: f32, gaps: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let mut points = vec![lo, hi];
    for &(a, b) in gaps {
        points.push(a.clamp(lo, hi));
        points.push(b.clamp(lo, hi));
    }
    points.sort_by(|a, b| a.partial_cmp(b).unwrap());
    points.dedup_by(|a, b| (*a - *b).abs() < 1e-4);
    let mut out = Vec::new();
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if b - a < 1e-3 {
            continue;
        }
        let mid = (a + b) / 2.;
        if !gaps.iter().any(|&(g0, g1)| mid > g0 && mid < g1) {
            out.push((a, b));
        }
    }
    out
}

/// A wall along Z at fixed `x`, from `z0` to `z1` (either order), `height` and `thickness` metres, with
/// doorway `gaps` (each a `(z_start, z_end)` range to leave open). Example:
/// `wall_along_z(-1.2, -16., 2., 2.6, 0.2, &[(0., 2.)])` is a corridor wall with one 2 m doorway.
pub fn wall_along_z(
    x: f32,
    z0: f32,
    z1: f32,
    height: f32,
    thickness: f32,
    gaps: &[(f32, f32)],
) -> Vec<Collider> {
    let (lo, hi) = (z0.min(z1), z0.max(z1));
    solid_ranges(lo, hi, gaps)
        .into_iter()
        .map(|(a, b)| Collider {
            min: V(x - thickness / 2., 0., a),
            max: V(x + thickness / 2., height, b),
        })
        .collect()
}

/// A wall along X at fixed `z`, from `x0` to `x1` (either order), with doorway `gaps` (each a
/// `(x_start, x_end)` range). See [`wall_along_z`].
pub fn wall_along_x(
    z: f32,
    x0: f32,
    x1: f32,
    height: f32,
    thickness: f32,
    gaps: &[(f32, f32)],
) -> Vec<Collider> {
    let (lo, hi) = (x0.min(x1), x0.max(x1));
    solid_ranges(lo, hi, gaps)
        .into_iter()
        .map(|(a, b)| Collider {
            min: V(a, 0., z - thickness / 2.),
            max: V(b, height, z + thickness / 2.),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_ranges_cuts_a_doorway_out_of_the_middle_start_end_or_everything() {
        assert_eq!(
            solid_ranges(0., 10., &[(4., 6.)]),
            vec![(0., 4.), (6., 10.)]
        );
        assert_eq!(solid_ranges(0., 10., &[]), vec![(0., 10.)]);
        assert_eq!(solid_ranges(0., 10., &[(0., 2.)]), vec![(2., 10.)]);
        assert_eq!(solid_ranges(0., 10., &[(8., 10.)]), vec![(0., 8.)]);
        assert_eq!(
            solid_ranges(0., 10., &[(-5., 15.)]),
            Vec::<(f32, f32)>::new()
        );
    }

    #[test]
    fn solid_ranges_handles_several_non_overlapping_gaps() {
        assert_eq!(
            solid_ranges(0., 10., &[(6., 8.), (2., 3.)]),
            vec![(0., 2.), (3., 6.), (8., 10.)]
        );
    }

    fn bounds(colliders: &[Collider]) -> Vec<(V, V)> {
        colliders.iter().map(|c| (c.min, c.max)).collect()
    }

    #[test]
    fn wall_builders_accept_either_endpoint_order() {
        assert_eq!(
            bounds(&wall_along_z(0., -16., 2., 2.6, 0.2, &[])),
            bounds(&wall_along_z(0., 2., -16., 2.6, 0.2, &[]))
        );
        assert_eq!(
            bounds(&wall_along_x(0., -3., 3., 2.6, 0.2, &[])),
            bounds(&wall_along_x(0., 3., -3., 2.6, 0.2, &[]))
        );
    }

    #[test]
    fn a_wall_has_no_collider_covering_the_doorway_centre() {
        let wall = wall_along_z(1.2, -16., 2., 2.6, 0.2, &[(0., 2.), (-9., -7.)]);
        for midpoint in [V(1.2, 1., 1.), V(1.2, 1., -8.)] {
            assert!(
                !wall.iter().any(|c| c.contains(midpoint)),
                "{midpoint:?} should be an open doorway"
            );
        }
        // Either side of a doorway is still solid.
        assert!(wall.iter().any(|c| c.contains(V(1.2, 1., -3.))));
    }

    #[test]
    fn walls_are_centred_on_the_fixed_coordinate_with_the_given_thickness_and_height() {
        let wall = wall_along_x(5., -3., 3., 2.6, 0.4, &[]);
        assert_eq!(wall.len(), 1);
        let c = &wall[0];
        assert_eq!((c.min.0, c.max.0), (-3., 3.));
        assert_eq!((c.min.2, c.max.2), (4.8, 5.2));
        assert_eq!((c.min.1, c.max.1), (0., 2.6));
    }
}
