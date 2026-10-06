use std::cell::RefCell;
use std::f32::consts::PI;
use vesper3d::asset_model::draw::{Mat4, Model, Vec3};

struct Furniture {
    table: Model,
    chair: Model,
    lamp: Model,
}

thread_local! {
    // Upload once on the rendering thread; retain textures across frames.
    static FURNITURE: RefCell<Option<Furniture>> = const { RefCell::new(None) };
}

// Kenney Furniture Kit 2.0, CC0-1.0, via the reviewed cc0/furniture catalog.
// For a relocatable game, copy these processed packs and their provenance/license
// into game-local art/table, art/chair and art/floor-lamp, then replace each
// absolute include_bytes! path with a path relative to this Rust source file.
// Native and browser clients embed the same model.json files; no runtime import.
pub fn furnish(world: &mut vesper3d::portable::draw::World) {
    FURNITURE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let furniture = cache.get_or_insert_with(|| Furniture {
            table: Model::from_json(include_bytes!(
                "../../../../assets/models/cc0/table/model.json"
            ))
            .expect("reviewed CC0 table model"),
            chair: Model::from_json(include_bytes!(
                "../../../../assets/models/cc0/chair/model.json"
            ))
            .expect("reviewed CC0 chair model"),
            lamp: Model::from_json(include_bytes!(
                "../../../../assets/models/cc0/floor-lamp/model.json"
            ))
            .expect("reviewed CC0 floor-lamp model"),
        });

        // Catalog models already have the requested meter dimensions and y=0 bottoms.
        append(world, &furniture.table, Mat4::IDENTITY);
        for x in [-0.9_f32, 0.9] {
            for (z, yaw) in [(-0.95_f32, 0.0), (0.95, PI)] {
                // Chair forward is +Z, back is -Z: backs point away from the table.
                append(
                    world,
                    &furniture.chair,
                    Mat4::from_translation(Vec3::new(x, 0.0, z)) * Mat4::from_rotation_y(yaw),
                );
            }
        }
        append(
            world,
            &furniture.lamp,
            Mat4::from_translation(Vec3::new(1.65, 0.0, -0.65)),
        );
    });
}

fn append(world: &mut vesper3d::portable::draw::World, model: &Model, transform: Mat4) {
    for mesh in model.meshes(transform).expect("valid furniture transform") {
        world.mesh(mesh);
    }
}
