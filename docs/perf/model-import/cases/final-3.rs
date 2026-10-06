//! Isolated benchmark: preprocessed Kenney Furniture Kit 2.0 art, CC0-1.0.
//! For a relocatable game, copy the selected packs (including provenance and
//! upstream licenses) into game/art/{table,chair,floor-lamp}, then replace the
//! absolute include_bytes! paths with paths relative to this Rust source.
//! The same embedded data supports native/browser portable presentation.

use vesper3d::asset_model::draw::{Mat4, Model, Vec3};
use vesper3d::portable::draw::World;

// Upload once per rendering thread and retain the models across frames.
// Initialize this only while the client's graphics context is active.
std::thread_local! {
    static FURNITURE: [Model; 3] = [
        Model::from_json(include_bytes!("../../../../assets/models/cc0/table/model.json"))
            .expect("embedded CC0 table must decode"),
        Model::from_json(include_bytes!("../../../../assets/models/cc0/chair/model.json"))
            .expect("embedded CC0 chair must decode"),
        Model::from_json(include_bytes!("../../../../assets/models/cc0/floor-lamp/model.json"))
            .expect("embedded CC0 floor lamp must decode"),
    ];
}

/// Add the furniture to the caller's world; the caller supplies floor/camera/HUD.
pub fn furnish(world: &mut World) {
    FURNITURE.with(|models| {
        place(world, &models[0], [0.0, 0.0, 0.0], 0.0);

        // Catalog chair forward is +Z, with its back at -Z. Aim +Z at the
        // table center so each chair's back points away from the table.
        for x in [-0.9_f32, 0.9_f32] {
            for z in [-0.95_f32, 0.95_f32] {
                let yaw = (-x).atan2(-z);
                place(world, &models[1], [x, 0.0, z], yaw);
            }
        }

        place(world, &models[2], [1.65, 0.0, -0.65], 0.0);
    });
}

fn place(world: &mut World, model: &Model, position: [f32; 3], yaw: f32) {
    // Each imported pack has a bottom-center origin and already matches the
    // requested dimensions in meters, so no scaling or vertical offset is needed.
    let transform = Mat4::from_translation(Vec3::from_array(position)) * Mat4::from_rotation_y(yaw);
    for mesh in model
        .meshes(transform)
        .expect("furniture transform must be valid")
    {
        world.mesh(mesh);
    }
}
