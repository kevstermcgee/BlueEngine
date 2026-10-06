use vesper3d::asset_model::draw::{Mat4, Model, Vec3};
use vesper3d::portable::draw::World;

// CC0: Kenney, Furniture Kit 2.0, https://kenney.nl/assets/furniture-kit.
// Isolated benchmark paths below embed the preprocessed catalog art directly.
// For a relocatable game, copy the complete chair/table/floor-lamp directories
// from assets/models/cc0 into the game's art/ directory, preserve provenance and
// license files, and replace these paths with ../art/<name>/model.json (src/lib.rs).
// Model loading happens once, on the graphics thread at the first furnish call.
pub fn furnish(world: &mut World) {
    std::thread_local! {
        static ART: [Model; 3] = [
            Model::from_json(include_bytes!("../../../../assets/models/cc0/table/model.json"))
                .expect("embedded CC0 table is valid"),
            Model::from_json(include_bytes!("../../../../assets/models/cc0/chair/model.json"))
                .expect("embedded CC0 chair is valid"),
            Model::from_json(include_bytes!("../../../../assets/models/cc0/floor-lamp/model.json"))
                .expect("embedded CC0 floor lamp is valid"),
        ];
    }

    ART.with(|art| {
        // Catalog assets use a bottom-center pivot and already match all sizes.
        append(world, &art[0], Mat4::IDENTITY);
        // The imported chair's back is on local -Z: north backs stay outward,
        // while south chairs turn by pi to face the center of the table.
        for x in [-0.9, 0.9] {
            for (z, yaw) in [(-0.95, 0.0), (0.95, std::f32::consts::PI)] {
                append(
                    world,
                    &art[1],
                    Mat4::from_translation(Vec3::new(x, 0.0, z)) * Mat4::from_rotation_y(yaw),
                );
            }
        }
        append(
            world,
            &art[2],
            Mat4::from_translation(Vec3::new(1.65, 0.0, -0.65)),
        );
    });
}

fn append(world: &mut World, model: &Model, transform: Mat4) {
    for mesh in model.meshes(transform).expect("valid furniture transform") {
        world.mesh(mesh);
    }
}
