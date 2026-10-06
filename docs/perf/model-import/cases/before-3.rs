use vesper3d::portable::draw::{Color, World};

/// Fixed furniture only. Floor, camera, and HUD are owned by the caller.
pub fn furnish(world: &mut vesper3d::portable::draw::World) {
    let wood = Color::new(0.8962264, 0.6015712, 0.3931559, 1.0);
    let warm = Color::new(1.0, 0.9137255, 0.5882353, 1.0);
    let metal = Color::new(0.74061054, 0.822_866_74, 0.8396226, 1.0);

    // Full-width tabletop establishes the exact [2.10372, 0.816835, 1.118433] bounds.
    // All remaining table pieces are inset beneath it.
    world.cube([0.0, 0.7659175, 0.0], [2.10372, 0.101835, 1.118433], wood);
    for x in [-0.92186, 0.92186] {
        for z in [-0.4292165, 0.4292165] {
            world.cube([x, 0.3575, z], [0.14, 0.715, 0.14], wood);
        }
    }
    for z in [-0.4292165, 0.4292165] {
        world.cube([0.0, 0.65, z], [1.98372, 0.13, 0.07], wood);
    }
    for x in [-0.92186, 0.92186] {
        world.cube([x, 0.65, 0.0], [0.07, 0.13, 0.998433], wood);
    }

    // Back lies away from the table; open front faces its center.
    for x in [-0.9, 0.9] {
        chair(world, [x, 0.0, 0.95], 1.0, wood);
        chair(world, [x, 0.0, -0.95], -1.0, wood);
    }

    // Thin metal upright and square warm shade; total bounds [0.3, 2.15, 0.3].
    let lamp_x = 1.65;
    let lamp_z = -0.65;
    world.cube([lamp_x, 0.0325, lamp_z], [0.3, 0.065, 0.3], metal);
    world.cube([lamp_x, 0.9475, lamp_z], [0.036, 1.765, 0.036], metal);
    world.cube([lamp_x, 0.09, lamp_z], [0.085, 0.05, 0.085], metal);
    world.cube([lamp_x, 1.79, lamp_z], [0.08, 0.04, 0.08], metal);
    world.cube([lamp_x, 1.98, lamp_z], [0.3, 0.34, 0.3], warm);
}

fn chair(world: &mut World, origin: [f32; 3], back_sign: f32, wood: Color) {
    let [x, y, z] = origin;
    // Seat establishes exact horizontal bounds; its underside meets all four legs.
    world.cube([x, y + 0.48, z], [0.5, 0.09, 0.5], wood);
    for dx in [-0.205, 0.205] {
        for dz in [-0.205, 0.205] {
            world.cube([x + dx, y + 0.2175, z + dz], [0.07, 0.435, 0.07], wood);
        }
    }
    let back_z = z + back_sign * 0.215;
    for dx in [-0.215, 0.215] {
        world.cube([x + dx, y + 0.84, back_z], [0.07, 0.67, 0.07], wood);
    }
    world.cube([x, y + 1.12, back_z], [0.5, 0.11, 0.07], wood);
    world.cube([x, y + 0.66, back_z], [0.5, 0.07, 0.07], wood);
    for dx in [-0.125, 0.0, 0.125] {
        world.cube([x + dx, y + 0.8725, back_z], [0.045, 0.385, 0.05], wood);
    }
}
