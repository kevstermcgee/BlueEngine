use std::cell::RefCell;
use vesper3d::asset_model::draw::{Mat4, Model, Vec3};

// The benchmark embeds the catalog fixtures by absolute path. For a relocatable
// game, keep the processed packs under art/{table,chair,floor-lamp}, including
// provenance and the CC0 license, and use include_bytes!("../art/.../model.json").
// Models upload once on the graphics thread and are retained for later frames.
thread_local! {
    static FURNITURE: RefCell<Option<(Model, Model, Model)>> = const { RefCell::new(None) };
}

pub fn furnish(world: &mut vesper3d::portable::draw::World) {
    FURNITURE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let (table, chair, lamp) = cache.get_or_insert_with(|| {
            (
                Model::from_json(include_bytes!(
                    "../../../../assets/models/cc0/table/model.json"
                ))
                .expect("valid CC0 table model"),
                Model::from_json(include_bytes!(
                    "../../../../assets/models/cc0/chair/model.json"
                ))
                .expect("valid CC0 chair model"),
                Model::from_json(include_bytes!(
                    "../../../../assets/models/cc0/floor-lamp/model.json"
                ))
                .expect("valid CC0 floor-lamp model"),
            )
        });

        // Imported origins are bottom-center: identity scale places bottoms at y=0.
        for mesh in table.meshes(Mat4::IDENTITY).expect("table instance") {
            world.mesh(mesh);
        }
        for x in [-0.9_f32, 0.9] {
            for z in [-0.95_f32, 0.95] {
                // The catalog chair faces +Z with its back at -Z. Each row faces
                // the table along Z; its chair backs point away from the table.
                let yaw = if z > 0.0 { std::f32::consts::PI } else { 0.0 };
                let transform =
                    Mat4::from_translation(Vec3::new(x, 0.0, z)) * Mat4::from_rotation_y(yaw);
                for mesh in chair.meshes(transform).expect("chair instance") {
                    world.mesh(mesh);
                }
            }
        }
        let transform = Mat4::from_translation(Vec3::new(1.65, 0.0, -0.65));
        for mesh in lamp.meshes(transform).expect("floor-lamp instance") {
            world.mesh(mesh);
        }
    });
}
