use vesper3d::portable::draw::{Color, World};

/// Presentation-only furniture; all dimensions are world units, with feet on y=0.
pub fn furnish(world: &mut World) {
    let wood = Color::new(0.8962264, 0.6015712, 0.3931559, 1.0);
    let lamp = Color::new(1.0, 0.9137255, 0.5882353, 1.0);
    let metal = Color::new(0.74061054, 0.822_866_74, 0.8396226, 1.0);

    // Table: a full-width top, four inset legs, and a supporting apron.
    const TABLE_W: f32 = 2.10372;
    const TABLE_H: f32 = 0.816835;
    const TABLE_D: f32 = 1.118433;
    const TOP_T: f32 = 0.09;
    let underside = TABLE_H - TOP_T;
    world.cube(
        [0.0, TABLE_H - TOP_T * 0.5, 0.0],
        [TABLE_W, TOP_T, TABLE_D],
        wood,
    );
    for x in [-0.93, 0.93] {
        for z in [-0.44, 0.44] {
            world.cube([x, underside * 0.5, z], [0.10, underside, 0.10], wood);
        }
    }
    for z in [-0.44, 0.44] {
        world.cube([0.0, underside - 0.065, z], [1.86, 0.13, 0.06], wood);
    }
    for x in [-0.93, 0.93] {
        world.cube([x, underside - 0.065, 0.0], [0.06, 0.13, 0.88], wood);
    }

    // Chairs face the table: the back sits on each chair's outward edge.
    // The 0.5 x 0.5 seat fixes the horizontal bounds; posts reach y=1.175.
    for x in [-0.9, 0.9] {
        for z in [-0.95, 0.95] {
            let outward = if z > 0.0 { 1.0 } else { -1.0 };
            world.cube([x, 0.435, z], [0.5, 0.07, 0.5], wood);
            for dx in [-0.2225, 0.2225] {
                for dz in [-0.2225, 0.2225] {
                    world.cube([x + dx, 0.20, z + dz], [0.055, 0.40, 0.055], wood);
                }
                world.cube(
                    [x + dx, 0.8225, z + outward * 0.2225],
                    [0.055, 0.705, 0.055],
                    wood,
                );
            }
            // Open slatted backs preserve visible gaps above the seat.
            for y in [0.74, 0.925, 1.11] {
                world.cube([x, y, z + outward * 0.2225], [0.445, 0.09, 0.055], wood);
            }
            // Front and side seat rails give the legs a coherent frame.
            world.cube([x, 0.365, z - outward * 0.2225], [0.445, 0.07, 0.055], wood);
            for dx in [-0.2225, 0.2225] {
                world.cube([x + dx, 0.365, z], [0.055, 0.07, 0.445], wood);
            }
        }
    }

    // Square-shaded floor lamp: broad low base, thin metal stem, hollow shade.
    const LX: f32 = 1.65;
    const LZ: f32 = -0.65;
    world.cube([LX, 0.0175, LZ], [0.30, 0.035, 0.30], metal);
    world.cube([LX, 0.95, LZ], [0.035, 1.83, 0.035], metal);
    world.cube([LX, 1.86, LZ], [0.16, 0.02, 0.16], metal);
    // Four thin walls and a top form a shade with an open underside.
    for dz in [-0.141, 0.141] {
        world.cube([LX, 2.0, LZ + dz], [0.30, 0.30, 0.018], lamp);
    }
    for dx in [-0.141, 0.141] {
        world.cube([LX + dx, 2.0, LZ], [0.018, 0.30, 0.264], lamp);
    }
    world.cube([LX, 2.141, LZ], [0.264, 0.018, 0.264], lamp);
}
