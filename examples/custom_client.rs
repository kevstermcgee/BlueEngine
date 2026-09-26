//! Minimal application-owned Macroquad presentation over BlueEngine simulation.
//! See docs/CUSTOM_CLIENT.md for the ownership boundary and external Cargo setup.
use macroquad::prelude as mq;
use vesper3d::prelude::*;

#[macroquad::main("BlueEngine custom client")]
async fn main() -> Result<()> {
    let mut world = SceneBuilder::new("Custom client")
        .box_body("floor", V(0., -0.1, 0.), V(8., 0.1, 8.), V(0.25, 0.3, 0.4))
        .prop("ball", "apple", V(2., 1., 0.))
        .world()?;
    world.join(1);

    let mut yaw = 0.;
    let mut pitch = 0.;
    let mut accumulator = 0.;
    loop {
        let mouse = mq::mouse_delta_position();
        yaw -= mouse.x * 0.0025;
        pitch = (pitch - mouse.y * 0.0025).clamp(-1.4, 1.4);
        let input = Movement {
            forward: axis(mq::KeyCode::W, mq::KeyCode::S),
            right: axis(mq::KeyCode::D, mq::KeyCode::A),
            sprint: mq::is_key_down(mq::KeyCode::LeftShift),
            jump: mq::is_key_pressed(mq::KeyCode::Space),
            crouch: mq::is_key_down(mq::KeyCode::LeftControl),
        };
        world.input(1, input, yaw, pitch);

        accumulator = (accumulator + mq::get_frame_time()).min(TICK_SECONDS * 8.);
        while accumulator >= TICK_SECONDS {
            world.step();
            accumulator -= TICK_SECONDS;
        }

        let player = world.player(1).expect("joined player");
        mq::clear_background(mq::Color::new(0.48, 0.7, 0.86, 1.));
        mq::set_camera(&mq::Camera3D {
            position: mq::vec3(
                player.position.0,
                player.position.1 + 2.5,
                player.position.2 + 6.,
            ),
            target: mq::vec3(player.position.0, player.position.1, player.position.2),
            up: mq::Vec3::Y,
            ..Default::default()
        });
        mq::draw_plane(
            mq::Vec3::ZERO,
            mq::vec2(16., 16.),
            None,
            mq::Color::new(0.25, 0.3, 0.4, 1.),
        );
        mq::draw_cube(
            mq::vec3(
                player.position.0,
                player.position.1 - 0.9,
                player.position.2,
            ),
            mq::vec3(0.46, 1.8, 0.46),
            None,
            mq::BLUE,
        );
        if let Some(ball) = world.prop_position("ball") {
            mq::draw_sphere(mq::vec3(ball.0, ball.1, ball.2), 0.2, None, mq::RED);
        }
        mq::set_default_camera();
        mq::draw_text("WASD + mouse, Space, Shift, Ctrl", 20., 32., 24., mq::WHITE);
        mq::next_frame().await;
    }
}

fn axis(positive: mq::KeyCode, negative: mq::KeyCode) -> f32 {
    mq::is_key_down(positive) as u8 as f32 - mq::is_key_down(negative) as u8 as f32
}
