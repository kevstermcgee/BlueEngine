use vesper3d::{
    math::V,
    viewer::{builder::SceneBuilder, controller::Movement, simulation::HeadlessWorld},
};

#[test]
fn static_game_preserves_authored_collision_without_promoting_props() {
    let map = SceneBuilder::new("Static game")
        .spawn(V(0., 0., 4.), 0.)
        .structural_box("floor", V(0., -0.1, 0.), V(8., 0.1, 8.), V(0.1, 0.3, 0.4))
        .prop("apple", "apple", V(3., 1., 0.))
        .build()
        .unwrap();
    let room = map.build().unwrap();
    let count = room.colliders.len();
    let mut world = HeadlessWorld::with_static_room(room);
    assert!(world.prop_physics.is_none());
    assert!(world.join(1));
    world.input(
        1,
        Movement {
            forward: 1.,
            ..Default::default()
        },
        0.,
        0.,
    );
    for _ in 0..60 {
        world.step();
    }
    assert_eq!(world.room.colliders.len(), count);
    assert!(world.prop_position("apple").is_none());
    assert!(world.player(1).unwrap().position.2 < 2.);
}
