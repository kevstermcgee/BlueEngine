# BlueEngine prototype API
`use vesper3d::prelude::*;` Metres, +Y up; yaw 0 faces -Z; angles radians.
Boxes: center/half extents. `structural_box` omits semantic reachability. Props: bottom origin; catalog physics rules.
`build()->Result<MapDocument>`; `world()->Result<HeadlessWorld>`.
`input(id:u64,Movement,yaw:f32,pitch:f32)->bool`; `step()` = 1/60s.
`impulse(id:&str,V)->bool`; `prop_position(id:&str)->Option<V>`.
IDs are owned; unknown IDs fail. Jump is an edge; movement persists.
Run: `cargo run --no-default-features --example prototype`.
For a nonstandard window or renderer, continue with [CUSTOM_CLIENT.md](CUSTOM_CLIENT.md).
For input-driven acceptance tests, use the existing scenario driver in
[BEHAVIORAL_TESTING.md](BEHAVIORAL_TESTING.md) instead of building a shell harness.
```rust
use vesper3d::prelude::*;

fn main() -> Result<()> {
    let mut world = SceneBuilder::new("Prototype")
        .spawn(V(0., 0., 4.6), -0.10)
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
```
