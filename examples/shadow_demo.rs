//! Spike: packed-depth shadow map, raw pieces, no helper.
use macroquad::prelude::*;
use vesper3d::viewer::{
    devkit::{flag_value, Lifecycle},
    game_client,
    kit::{self, Batch, Look, Materials, ShadowMap, Template, Tint},
};

fn window() -> macroquad::conf::Conf {
    let mut conf = game_client::window_config("shadow spike");
    let args: Vec<String> = std::env::args().collect();
    if let Some((w, h)) = flag_value(&args, "--size").and_then(vesper3d::viewer::devkit::parse_size)
    {
        conf.miniquad_conf.window_width = w as i32;
        conf.miniquad_conf.window_height = h as i32;
    }
    conf
}

#[macroquad::main(window)]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut life = Lifecycle::<()>::start_or_exit(&args, &[]);
    let mode = flag_value(&args, "--mode").unwrap_or("full").to_owned();
    let materials = Materials::load().expect("materials");
    let look = match flag_value(&args, "--look") {
        Some("dusk") => Look::dusk(),
        Some("night") => Look::night(),
        _ => Look::daylight(),
    };
    let map = ShadowMap::new(2048).expect("map");
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
    let mut world = Batch::new();
    let mut stat = Batch::new();
    stat.add(&ground, Mat4::IDENTITY, Tint::NONE);
    let mut cube = Template::new();
    cube.box_(vec3(0., 1., 0.), vec3(1., 1., 1.), [0.8, 0.25, 0.2], 0.);
    cube.box_(vec3(-6., 2., -4.), vec3(0.4, 2., 0.4), [0.7, 0.7, 0.75], 0.);
    world.add(&cube, Mat4::IDENTITY, Tint::NONE);
    let mut ball = Template::new();
    ball.ball(
        vec3(4., 2.5, 2.),
        Vec3::splat(0.8),
        [0.9, 0.8, 0.2],
        0.,
        20,
        12,
    );
    world.add(&ball, Mat4::IDENTITY, Tint::NONE);
    loop {
        let dt = life.begin_frame(get_frame_time());
        clear_background(look.clear_color());
        let focus = vec3(0., 0., 0.);
        let cam = map.camera(&look, focus, 20., 80.);
        if mode != "off" {
            map.pass(&cam, || {
                stat.draw();
                world.draw();
            });
        }
        let eye = vec3(9., 7., 11.);
        set_camera(&Camera3D {
            position: eye,
            target: vec3(1., 0., 0.),
            up: Vec3::Y,
            fovy: 0.9,
            z_near: 0.3,
            z_far: 700.,
            ..Default::default()
        });
        materials.set_scene(&look, eye, 0., 0.);
        match mode.as_str() {
            "full" => materials.set_shadow(&map, &cam, 0.85),
            "unset" => {
                materials
                    .world
                    .set_uniform("LightVP", cam.light().view_proj);
                materials
                    .world
                    .set_uniform("Shadow", vec4(1., 1. / 2048., 0.05, 0.001));
            }
            _ => materials.clear_shadow(),
        }
        gl_use_material(&materials.world);
        stat.draw();
        world.draw();
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
