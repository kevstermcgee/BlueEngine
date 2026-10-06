use std::cell::RefCell;
use vesper3d::asset_model::draw::{Mat4, Model, Vec3};
use vesper3d::portable::draw::World;

// Relocatable game route: copy assets/models/cc0 into game/art/furniture,
// including KENNEY_LICENSE.txt and pack.json, then replace the absolute
// include_bytes! paths below with ../art/furniture/<prop>/model.json relative
// to this module. Retain the accompanying pack/provenance/collider metadata.
// Model::from_json uploads graphics resources once on the rendering thread.
thread_local! {
    static ART: RefCell<Option<(Model, Model, Model)>> = const { RefCell::new(None) };
}

pub fn furnish(world: &mut World) {
    ART.with(|art| {
        let mut art = art.borrow_mut();
        let (table, chair, lamp) = art.get_or_insert_with(|| {
            (
                Model::from_json(include_bytes!(
                    "../../../../assets/models/cc0/table/model.json"
                ))
                .expect("checked-in CC0 table must decode"),
                Model::from_json(include_bytes!(
                    "../../../../assets/models/cc0/chair/model.json"
                ))
                .expect("checked-in CC0 chair must decode"),
                Model::from_json(include_bytes!(
                    "../../../../assets/models/cc0/floor-lamp/model.json"
                ))
                .expect("checked-in CC0 floor lamp must decode"),
            )
        });

        // Catalog models are already bottom-centered and scaled in meters.
        // table: 2.10372 x .816835 x 1.118433; chair: .5 x 1.175 x .5;
        // lamp: .3 x 2.15 x .3. Translation at y=0 leaves all bottoms at y=0.
        place(world, table, Vec3::ZERO, 0.0);
        // The chair's original back is at -Z; each pair faces inward.
        for x in [-0.9, 0.9] {
            place(world, chair, Vec3::new(x, 0.0, -0.95), 0.0);
            place(world, chair, Vec3::new(x, 0.0, 0.95), std::f32::consts::PI);
        }
        place(world, lamp, Vec3::new(1.65, 0.0, -0.65), 0.0);
    });
}

fn place(world: &mut World, model: &Model, bottom_center: Vec3, yaw: f32) {
    let transform = Mat4::from_translation(bottom_center) * Mat4::from_rotation_y(yaw);
    for mesh in model.meshes(transform).expect("finite furniture transform") {
        world.mesh(mesh);
    }
}
