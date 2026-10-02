//! Shadows in the kit, end to end: a ground plane, a wall, a cube and a pillar, with a ball rolling in a
//! circle, a hopping ball and a box sliding behind the wall. It is the reference for adopting
//! `kit::Shadows` (about ten lines in a game) and what the visual checks run.
//!
//!   cargo run --example shadow_demo -- --shadows off|simple|full [--look daylight|dusk|night] [--cam high|low]
//!   python tools/xcapture.py target/.../examples/shadow_demo --frames 5 -- --shadows full
//!
//! Also takes the lifecycle flags (`--capture DIR --frames N`, `--perf`, `--size WxH`).
use macroquad::prelude::*;
use vesper3d::viewer::{
    devkit::{flag_value, parse_size, Lifecycle, ShadowQuality},
    game_client,
    kit::{self, Batch, Look, Materials, Shadows, Template, Tint},
};

fn window() -> macroquad::conf::Conf {
    let mut conf = game_client::window_config("BlueEngine shadow demo");
    let args: Vec<String> = std::env::args().collect();
    if let Some((w, h)) = flag_value(&args, "--size").and_then(parse_size) {
        conf.miniquad_conf.window_width = w as i32;
        conf.miniquad_conf.window_height = h as i32;
    }
    conf
}

#[macroquad::main(window)]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut life = Lifecycle::<()>::start_or_exit(&args, &[]);
    let quality = match ShadowQuality::from_flag(&args) {
        Ok(q) => q.unwrap_or_default(),
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };
    let look = match flag_value(&args, "--look") {
        Some("dusk") => Look::dusk(),
        Some("night") => Look::night(),
        _ => Look::daylight(),
    };
    let (eye, target) = match flag_value(&args, "--cam") {
        Some("low") => (vec3(8., 2.2, 12.), vec3(0., 1., -2.)),
        _ => (vec3(9., 7., 11.), vec3(1., 0., 0.)),
    };
    let materials = Materials::load().expect("the materials failed to compile");
    // One line to opt in: the helper owns the tier, the map and the blobs.
    let mut shadows = Shadows::new(quality).with_range(20., 80.);

    let mut ground = Template::new();
    ground.quad_facing(
        [
            vec3(-30., 0., -30.),
            vec3(30., 0., -30.),
            vec3(30., 0., 30.),
            vec3(-30., 0., 30.),
        ],
        Vec3::Y,
        [0.45, 0.55, 0.35],
        0.,
    );
    ground.box_(
        vec3(0., 1.75, -3.),
        vec3(6., 1.75, 0.2),
        [0.75, 0.7, 0.6],
        0.,
    ); // the wall
    ground.box_(vec3(0., 1., 4.), vec3(1., 1., 1.), [0.8, 0.25, 0.2], 0.); // the cube
    ground.box_(vec3(-7., 2., 2.), vec3(0.4, 2., 0.4), [0.7, 0.7, 0.75], 0.); // the pillar
    let mut statics = Batch::new();
    statics.add(&ground, Mat4::IDENTITY, Tint::NONE);
    let mut ball = Template::new();
    ball.ball(Vec3::ZERO, Vec3::splat(0.8), [0.9, 0.8, 0.2], 0., 20, 12);
    let mut crate_ = Template::new();
    crate_.box_(Vec3::ZERO, vec3(0.8, 0.5, 0.8), [0.3, 0.5, 0.9], 0.);
    let mut actors = Batch::new();

    loop {
        let dt = life.begin_frame(get_frame_time());
        let t = life.time();
        clear_background(look.clear_color());

        // Where the actors are this frame (blobs sit at their feet).
        let roller = vec3(5. * (t * 0.8).cos(), 0.8, 2. + 3. * (t * 0.8).sin());
        let hop = (t * 2.).sin().abs() * 2.5;
        let hopper = vec3(-3., 0.8 + hop, 6.);
        let slider = vec3(4. * (t * 0.5).sin(), 0.5, -6.);
        actors.clear();
        actors.add(&ball, Mat4::from_translation(roller), Tint::NONE);
        actors.add(&ball, Mat4::from_translation(hopper), Tint::NONE);
        actors.add(&crate_, Mat4::from_translation(slider), Tint::NONE);

        // The shadow code a game adds: begin, blobs for the actors, the pass, apply.
        shadows.begin_frame(&look, vec3(0., 0., 0.));
        shadows.blob(roller - vec3(0., 0.8, 0.), 1.1);
        shadows.blob(hopper - vec3(0., 0.8, 0.), 1.1);
        shadows.blob(slider - vec3(0., 0.5, 0.), 1.4);
        shadows.cast(|| {
            statics.draw();
            actors.draw();
        });

        set_camera(&Camera3D {
            position: eye,
            target,
            up: Vec3::Y,
            fovy: 0.9,
            z_near: 0.3,
            z_far: 700.,
            ..Default::default()
        });
        materials.set_scene(&look, eye, t, 0.);
        shadows.apply(&materials);
        materials.draw_static(&statics.meshes);
        shadows.draw_decals(&materials); // after the static world, before the actors
        gl_use_material(&materials.world);
        actors.draw();
        gl_use_default_material();
        set_default_camera();

        if let Some(path) = life.capture_path() {
            life.captured(
                &path,
                kit::capture::save_frame(&path).map_err(|e| e.to_string()),
            );
        }
        if life.end_frame(dt) {
            break;
        }
        next_frame().await;
    }
    if let Some(report) = life.report() {
        println!("{report}");
    }
}
