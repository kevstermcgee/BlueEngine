//! The smallest complete game on the kit: window, first-person camera, coloured meshes, a sound.
//!
//! Walk with WASD, look with the mouse (Esc frees the cursor, a click grabs it again), touch the glowing
//! orbs to collect them, Q quits. `cargo run --example minimal_game [-- --mute] [-- --shot out.png]` (`--shot`
//! saves the frame after 10 frames and exits, for an agent that cannot watch the window).
//! Copy this file, then read docs/CUSTOM_CLIENT.md for the fixed-step loop, saves and effects.
use macroquad::prelude::*;
use vesper3d::viewer::devkit::synth::{self, Preset};
use vesper3d::viewer::devkit::{Bounds, FpsCamera, MouseLook, EYE_HEIGHT};
use vesper3d::viewer::game_client::{exit_requested, request_exit, window_config};
use vesper3d::viewer::game_input::mouse_pixels;
use vesper3d::viewer::kit::{
    gizmo, hud, Batch, Look, Materials, Rendered, SoundBank, Template, Tint, View,
};

fn window() -> macroquad::conf::Conf {
    window_config("Minimal game")
}

#[macroquad::main(window)]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str| args.iter().any(|a| a == name);
    let shot = args
        .iter()
        .position(|a| a == "--shot")
        .and_then(|i| args.get(i + 1))
        .cloned();

    // Build meshes once (metres, +Y up). A Template is a small mesh; a Batch draws many in one call.
    let (mut floor, mut orb, mut pillar) = (Template::new(), Template::new(), Template::new());
    floor.box_top(
        vec3(0., -0.5, 0.),
        vec3(12., 0.5, 12.),
        [0.18, 0.2, 0.26],
        [0.3, 0.34, 0.42],
        0.,
    );
    orb.ball(
        vec3(0., 0., 0.),
        vec3(0.25, 0.25, 0.25),
        [1., 0.8, 0.3],
        0.8,
        16,
        8,
    );
    pillar.box_(vec3(0., 1.5, 0.), vec3(0.4, 1.5, 0.4), [0.5, 0.25, 0.3], 0.);
    // Scale check with no window needed: a pillar should be a person-ish 3 m tall, an orb hand-sized.
    Bounds::of(orb.verts.iter().map(|v| [v.p.x, v.p.y, v.p.z]))
        .unwrap()
        .expect_longest("orb", 0.2..=0.8)
        .expect("orb is out of scale");

    let (materials, look) = (Materials::load().expect("shaders"), Look::dusk());
    let mut sky = Template::new();
    sky.sky_dome(
        200.,
        |e| {
            let t = e.clamp(0., 1.).sqrt();
            [
                0.1 + 0.3 * (1. - t),
                0.1 + 0.15 * (1. - t),
                0.25 + 0.15 * (1. - t),
            ]
        },
        32,
        16,
    );
    let sky_meshes = sky.to_meshes();
    let render = || Rendered {
        // sfx[sound][variant] = WAV bytes; here one sound (0) with all of Coin's pitch variants.
        sfx: vec![(0..Preset::Coin.variants())
            .map(|v| synth::wav_bytes(&synth::render(Preset::Coin, v, 1), synth::RATE))
            .collect()],
        stems: Vec::new(),
    };
    let mut sounds = SoundBank::start(flag("--mute"), 0.9, 0.0, render).await;

    // State: where I stand, where I look (FpsCamera owns the mouse convention), what is left to collect.
    let (mut pos, mut cam, mouse) = (vec3(0., 0., 6.), FpsCamera::default(), MouseLook::default());
    let mut orbs = vec![vec3(-4., 0.9, -2.), vec3(3., 0.9, -5.), vec3(0., 0.9, -9.)];
    let (mut grabbed, mut skip_look, mut world, mut frame) = (false, false, Batch::new(), 0u32);
    let ui = hud::ui_scale();

    loop {
        if exit_requested() {
            break; // the loop ends, main returns, destructors run
        }
        if is_key_pressed(KeyCode::Q) {
            request_exit();
        }
        let dt = get_frame_time().min(0.05);
        sounds.poll().await;

        // Input: a click grabs the cursor, Esc frees it. The frame a grab starts has a bogus mouse jump: skip it.
        let want_grab = (grabbed || is_mouse_button_pressed(MouseButton::Left))
            && !is_key_pressed(KeyCode::Escape);
        if want_grab != grabbed {
            grabbed = want_grab;
            set_cursor_grab(grabbed);
            show_mouse(!grabbed);
            skip_look = true;
        }
        if grabbed && !std::mem::take(&mut skip_look) {
            let [dx, dy] = mouse_pixels(); // pixels: +x right, +y down, whatever the platform reports
            cam.turn_pixels(&mouse, dx, dy);
        }
        let key = |k| if is_key_down(k) { 1. } else { 0. };
        let (fwd, right) = (
            key(KeyCode::W) - key(KeyCode::S),
            key(KeyCode::D) - key(KeyCode::A),
        );
        let (f, r) = (cam.walk_forward(), cam.walk_right());
        pos += vec3(f.0 * fwd + r.0 * right, 0., f.2 * fwd + r.2 * right).normalize_or_zero()
            * 4.
            * dt;
        pos = pos.clamp(vec3(-11., 0., -11.), vec3(11., 0., 11.));
        let before = orbs.len();
        orbs.retain(|o| (*o - vec3(pos.x, 0.9, pos.z)).length() > 1.0);
        if orbs.len() < before {
            sounds.play(0, 1.);
        }

        // Draw: sky, lit world, then the 2D layer.
        let view = View::first_person(pos + vec3(0., EYE_HEIGHT, 0.), cam.yaw, cam.pitch);
        clear_background(look.clear_color());
        set_camera(&view.sky_camera());
        gl_use_material(&materials.sky);
        sky_meshes.iter().for_each(draw_mesh);
        set_camera(&view.camera(0.05, 400.));
        materials.set_scene(&look, view.eye, get_time() as f32, 0.5);
        world.clear();
        world.add(&floor, Mat4::IDENTITY, Tint::NONE);
        world.add(
            &pillar,
            Mat4::from_translation(vec3(0., 0., -4.)),
            Tint::NONE,
        );
        for o in &orbs {
            world.add(&orb, Mat4::from_translation(*o), Tint::NONE);
        }
        gl_use_material(&materials.world);
        world.draw();
        gl_use_default_material();
        gizmo::human_scale(vec3(2., 0., -3.), 0., WHITE); // a 1.75 m person for scale; delete when happy
        set_default_camera();
        hud::crosshair(ui, 0., WHITE);
        hud::text_outlined(
            &format!("Orbs left: {}", orbs.len()),
            20. * ui,
            36. * ui,
            24. * ui,
            WHITE,
        );

        frame += 1;
        if let (Some(path), true) = (&shot, frame == 10) {
            let size = vesper3d::viewer::kit::capture::save_frame(std::path::Path::new(path))
                .expect("save frame");
            println!("saved {path} at {}x{}", size.0, size.1);
            break;
        }
        next_frame().await;
    }
}
