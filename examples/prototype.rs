use vesper3d::prelude::*;

fn main() -> Result<()> {
    let mut world = SceneBuilder::new("Prototype")
        .box_body("floor", V(0., -0.1, 0.), V(8., 0.1, 8.), V::ONE)
        .prop("ball", "apple", V(2., 1., 0.))
        .world()?;
    world.join(1);
    world.impulse("ball", V(0., 1., 0.));
    let mut input = Movement {
        forward: 1.,
        ..Default::default()
    };
    for tick in 0..120 {
        if tick == 60 {
            input.forward = 0.;
            input.jump = true;
        }
        world.input(1, input, 0., 0.);
        world.step();
        input.jump = false;
    }
    let player = world.player(1).unwrap();
    println!("Player: {:?}", player.position);
    println!("Ball: {:?}", world.prop_position("ball"));
    assert!(player.position.2 < 4.6);
    world.neutralize_input(1);
    world.leave(1);
    Ok(())
}
