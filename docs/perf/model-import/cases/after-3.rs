//! CC0 furniture from the catalog, retaining model uploads across draw frames.
//! Relocatable game route: copy assets/models/cc0/{chair,table,floor-lamp} into
//! your game's art/ directory, retaining pack.json/provenance.json/collider.json
//! and the upstream CC0 license; use include_bytes!("../art/chair/model.json")
//! (and corresponding table/lamp paths) from src/, then package that art directory.

use std::cell::OnceCell;
use vesper3d::asset_model::draw::{Mat4, Model, Vec3};
use vesper3d::portable::draw::World;

thread_local! {
    // First furnish call must run on the graphics thread with an active context.
    static FURNITURE: OnceCell<[Model; 3]> = const { OnceCell::new() };
}

pub fn furnish(world: &mut World) {
    FURNITURE.with(|cache| {
        let models = cache.get_or_init(|| {
            [
                Model::from_json(include_bytes!(
                    "../../../../assets/models/cc0/table/model.json"
                ))
                .expect("valid catalog table model"),
                Model::from_json(include_bytes!(
                    "../../../../assets/models/cc0/chair/model.json"
                ))
                .expect("valid catalog chair model"),
                Model::from_json(include_bytes!(
                    "../../../../assets/models/cc0/floor-lamp/model.json"
                ))
                .expect("valid catalog floor-lamp model"),
            ]
        });

        // Imported bounds: 2.103720 x 0.816835 x 1.118433, bottom center at zero.
        for mesh in models[0].meshes(Mat4::IDENTITY).expect("table transform") {
            world.mesh(mesh);
        }

        // The imported chair's back is at local -Z. Near chairs face +Z;
        // far chairs face -Z, putting all backs outside the table.
        for z in [-0.95_f32, 0.95_f32] {
            let yaw = if z < 0.0 { 0.0 } else { std::f32::consts::PI };
            for x in [-0.9_f32, 0.9_f32] {
                let transform =
                    Mat4::from_translation(Vec3::new(x, 0.0, z)) * Mat4::from_rotation_y(yaw);
                for mesh in models[1].meshes(transform).expect("chair transform") {
                    world.mesh(mesh);
                }
            }
        }

        // The original model supplies its square warm shade and metal base/pole.
        let transform = Mat4::from_translation(Vec3::new(1.65, 0.0, -0.65));
        for mesh in models[2].meshes(transform).expect("lamp transform") {
            world.mesh(mesh);
        }
    });
}
