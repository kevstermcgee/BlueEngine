# BlueEngine prototype API
`use vesper3d::prelude::*;` Metres, +Y up; yaw 0 faces -Z; angles radians.
Boxes: center/half extents. `structural_box` omits semantic reachability. Props: bottom origin; catalog physics rules.
`build()->Result<MapDocument>`; `world()->Result<HeadlessWorld>`.
`input(id:u64,Movement,yaw:f32,pitch:f32)->bool`; `step()` = 1/60s.
`impulse(id:&str,V)->bool`; `prop_position(id:&str)->Option<V>` (body origin; `prop_center_of_mass` is the physical centre).
By ID too: `prop_mass` (kg), `prop_half_extents`, `prop_rotation`, `prop_linear_velocity`, `prop_angular_velocity`, `is_prop_sleeping`,
`prop_holder`, `held_prop(player)`, `hit_prop(Ray,max)`, `prop_index(id)` (its stable index for the `prop_physics` API).
`throw(player,velocity,angvel)->bool` releases a carried prop at an exact velocity (a plain drop keeps at most 4 m/s).
`set_prop_floor(None)` removes the implicit y = 0 ground under props (pits, voids; `Some(y)` moves it), as `Controller::set_floor` does for players.
`SceneBuilder::without_spawn()` builds a props-only world (no player can join). `vesper3d::rapier` is the engine's `rapier3d`, re-exported.
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

Shared standalone-game infrastructure: [gameplay kit](SHARED_GAMEPLAY.md). Generated games call `playable::run_game_with_options` for shared local/online gameplay. `MapPlayer` remains a static viewer; custom presentation can use the graphics-free `GameSession`.

## Making a first-person game with your own window
Copy `examples/minimal_game.rs` (`cargo run --example minimal_game -- --shot out.png`, ~100 lines: window, camera, meshes, sound, HUD). Then `be2-tools new-game NAME DIR ENGINE_PATH custom-sim` for the full loop, saves and shipping ([CUSTOM_CLIENT.md](CUSTOM_CLIENT.md)).
- **Mouse look: one convention, never hand-written.** `MouseLook::default().look(px_right, px_down)` -> `FpsCamera::turn` (or `Controller::look(dx, dy, 1.0, false)`); pixels come from `game_input::mouse_pixels()`. Hand right turns right, hand up looks up, `MouseLook::new(sens, invert_y)`. Do not read macroquad's `mouse_delta_position()` (previous minus current: both signs flipped). Pitch limit `devkit::PITCH_LIMIT`.
- **Scale: 1 unit = 1 m.** `Bounds::of(points)?.expect_longest("shell", 0.05..=0.30)?` fails with the factor to apply; `Bounds::describe` prints "0.11 x 0.05 x 0.07 m (about 0.7 x a phone)". By eye: `kit::gizmo::{human_scale, bbox, template_bbox, axes}`.
- **Sound:** `SoundBank::start(muted, sfx, music, || Rendered{..})` with `devkit::synth::render(Preset::Coin, variant, seed)`; no audio files.
- **Iterate:** `scripts/blue dev [-- game args]` closes a still-open copy of the game first (a running .exe cannot be relinked on Windows: os error 5) and builds in the fast profile; `--release` only to ship.
- **Gamepad look:** `GamepadFrame::look_delta(dt)` (= `devkit::stick_look`) is the same `[right, down]` delta as the mouse: add them and call `FpsCamera::turn`. Stick right/up = look right/up. If it feels mirrored your own forward vector is: use `FpsCamera::forward()` = `(sin yaw·cos pitch, sin pitch, -cos yaw·cos pitch)`.
- **Menus:** `ClientInput::menu_step()` (`up/down/left/right`, stick flick + D-pad, hold-to-repeat), `menu_select()` (A/Cross), `menu_back()` (B/Circle); without `ClientInput`, `GamepadFrame::menu_step(&mut MenuNav, dt)`. Do not write your own debounce.
- **Quit:** `game_client::request_exit()`, and `if exit_requested() { break }` at the top of the loop, so `main` returns and destructors/network goodbyes run. Not `std::process::exit`.
