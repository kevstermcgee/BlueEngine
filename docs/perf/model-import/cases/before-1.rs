use vesper3d::portable::draw::{Color, World};

/// Furniture only. The caller supplies floor, camera, lighting, and HUD.
/// All placements use ground-plane centers; every prop begins at y = 0.
pub fn furnish(world: &mut World) {
    let wood = Color::new(0.8962264, 0.6015712, 0.3931559, 1.0);
    let warm = Color::new(1.0, 0.9137255, 0.5882353, 1.0);
    let metal = Color::new(0.74061054, 0.822_866_74, 0.8396226, 1.0);

    // Table: exact outer size, thin solid top, four inset legs, four aprons.
    let width = 2.10372;
    let height = 0.816835;
    let depth = 1.118433;
    let top_thickness = 0.075;
    let leg_height = height - top_thickness;
    let leg_x = width * 0.5 - 0.08;
    let leg_z = depth * 0.5 - 0.08;
    world.cube(
        [0.0, height - top_thickness * 0.5, 0.0],
        [width, top_thickness, depth],
        wood,
    );
    for x in [-leg_x, leg_x] {
        for z in [-leg_z, leg_z] {
            world.cube([x, leg_height * 0.5, z], [0.09, leg_height, 0.09], wood);
        }
    }
    for z in [-leg_z, leg_z] {
        world.cube(
            [0.0, leg_height - 0.045, z],
            [leg_x * 2.0, 0.09, 0.055],
            wood,
        );
    }
    for x in [-leg_x, leg_x] {
        world.cube(
            [x, leg_height - 0.045, 0.0],
            [0.055, 0.09, leg_z * 2.0],
            wood,
        );
    }

    // Chairs on the long sides face inward: their backs are on the outer z side.
    // Each is 0.5 wide, 0.5 deep, and 1.175 high, with open space below the seat
    // and between the back slats. Rear legs continue upward as back posts.
    for [x, z] in [[-0.9, 0.95], [0.9, 0.95], [-0.9, -0.95], [0.9, -0.95]] {
        let outward = if z > 0.0 { 1.0 } else { -1.0 };
        world.cube([x, 0.47, z], [0.5, 0.06, 0.5], wood);
        for dx in [-0.215, 0.215] {
            world.cube(
                [x + dx, 0.22, z - outward * 0.215],
                [0.055, 0.44, 0.055],
                wood,
            );
            world.cube(
                [x + dx, 0.5875, z + outward * 0.225],
                [0.055, 1.175, 0.05],
                wood,
            );
        }
        let back_z = z + outward * 0.225;
        world.cube([x, 1.14, back_z], [0.485, 0.07, 0.05], wood);
        world.cube([x, 0.78, back_z], [0.485, 0.065, 0.05], wood);
        for dx in [-0.12, 0.0, 0.12] {
            world.cube([x + dx, 0.96375, back_z], [0.045, 0.3025, 0.035], wood);
        }
        // Side stretchers add a recognizable wooden frame below the seat.
        for dx in [-0.215, 0.215] {
            world.cube([x + dx, 0.18, z], [0.045, 0.045, 0.43], wood);
        }
    }

    // Floor lamp: square metal foot, slender stem, open-bottom square shade.
    // Shade walls and top form a shell rather than filling the whole lamp box.
    let lamp_x = 1.65;
    let lamp_z = -0.65;
    world.cube([lamp_x, 0.0225, lamp_z], [0.3, 0.045, 0.3], metal);
    world.cube([lamp_x, 0.055, lamp_z], [0.22, 0.02, 0.22], metal);
    world.cube([lamp_x, 1.005, lamp_z], [0.035, 1.92, 0.035], metal);
    for dx in [-0.1425, 0.1425] {
        world.cube([lamp_x + dx, 1.99, lamp_z], [0.015, 0.32, 0.3], warm);
    }
    for dz in [-0.1425, 0.1425] {
        world.cube([lamp_x, 1.99, lamp_z + dz], [0.27, 0.32, 0.015], warm);
    }
    world.cube([lamp_x, 2.144, lamp_z], [0.3, 0.012, 0.3], warm);
}
