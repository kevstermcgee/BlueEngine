//! {{title}}: the window, renderer, sound and input around the simulation in the library.
//!
//! Play it, or drive it without a human (an agent cannot watch a window):
//!   --capture DIR [--frames 30,90] [--exit-after N]   save screenshots (DIR must be new), then exit
//!   --script "fwd:0-200,look:0.01@0-100,jump@60"      drive the human input path from a cue script
//!   --seed N   --size WxH   --mute   --perf           reproducible run, window size, silence, frame times
//!   --load SLOT_OR_FILE   --save-dir DIR                resume a saved game / where F5 saves (default: next to the exe)
//! F5 saves the run to the `quick` slot and F9 loads it. `--script` accepts `save@N` and `load@N` cues too.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod platform;

use macroquad::prelude::*;
use std::time::Instant;
use vesper3d::viewer::{
    devkit::{
        flag_value, has_flag, parse_size, snapshot, synth, CapturePlan, FixedStepper, InputAccumulator, Juice,
        PerfReport, SaveSlots, Source, Timeline, QUICK_SLOT, TICK,
    },
    game_client::{self, GameShell},
    game_input::ClientInput,
    identity::Identity,
    kit::{self, hud, Batch, Fx, Look, Materials, Rendered, SoundBank, Template, Tint, View},
};
use {{lib}}::{Event, Input, Sim, PLATFORM_HALF};

/// Title, tagline and controls live in one file, shared with the build script and `scripts/ship.py`.
const IDENTITY: &str = include_str!("../assets/identity.json");

fn identity() -> Identity {
    Identity::parse(IDENTITY).expect("assets/identity.json is invalid; run: python scripts/ship.py verify")
}

fn window() -> macroquad::conf::Conf {
    platform::attach_console();
    let mut conf = game_client::window_config_with_icon(
        &identity().title,
        game_client::icon_from_rgba(
            include_bytes!("../assets/icon_16.rgba"),
            include_bytes!("../assets/icon_32.rgba"),
            include_bytes!("../assets/icon_64.rgba"),
        ),
    );
    let args: Vec<String> = std::env::args().collect();
    if let Some((w, h)) = flag_value(&args, "--size").and_then(parse_size) {
        conf.miniquad_conf.window_width = w.clamp(320, 7680) as i32;
        conf.miniquad_conf.window_height = h.clamp(240, 4320) as i32;
    }
    if has_flag(&args, "--novsync") {
        conf.miniquad_conf.platform.swap_interval = Some(0);
    }
    conf
}

/// Held device state; press edges travel separately as bits (`JUMP`).
#[derive(Clone, Copy, Default)]
struct Held {
    forward: f32,
    right: f32,
}
const JUMP: u32 = 1;

/// Sounds by index: the engine's synthesised presets, rendered on a worker thread.
const SOUNDS: [synth::Preset; 5] =
    [synth::Preset::Coin, synth::Preset::Hit, synth::Preset::Jump, synth::Preset::Land, synth::Preset::GameOver];
fn sound(preset: synth::Preset) -> usize {
    SOUNDS.iter().position(|p| *p == preset).unwrap_or(0)
}
fn render_audio() -> Rendered {
    Rendered {
        sfx: SOUNDS
            .iter()
            .map(|p| (0..p.variants()).map(|v| synth::wav_bytes(&synth::render(*p, v, 7), synth::RATE)).collect())
            .collect(),
        stems: Vec::new(),
    }
}

/// The meshes built once at startup.
struct Scene {
    sky: Vec<Mesh>,
    platform: Vec<Mesh>,
    orb: Template,
    bumper: Template,
}

fn build_scene() -> Scene {
    let mut sky = Template::new();
    sky.sky_dome(
        200.,
        |e| {
            let t = e.clamp(0., 1.).sqrt();
            [0.10 + (0.40 - 0.10) * (1. - t), 0.08 + (0.22 - 0.08) * (1. - t), 0.22 + (0.42 - 0.22) * (1. - t)]
        },
        32,
        16,
    );
    let mut platform = Template::new();
    let half = vec3(PLATFORM_HALF, 0.5, PLATFORM_HALF);
    platform.box_top(vec3(0., -0.5, 0.), half, [0.10, 0.09, 0.16], [0.20, 0.19, 0.28], 0.);
    // A glowing rim so the edge of the world reads in the dark.
    for (c, h) in [
        (vec3(0., 0.02, PLATFORM_HALF - 0.1), vec3(PLATFORM_HALF, 0.02, 0.1)),
        (vec3(0., 0.02, -PLATFORM_HALF + 0.1), vec3(PLATFORM_HALF, 0.02, 0.1)),
        (vec3(PLATFORM_HALF - 0.1, 0.02, 0.), vec3(0.1, 0.02, PLATFORM_HALF)),
        (vec3(-PLATFORM_HALF + 0.1, 0.02, 0.), vec3(0.1, 0.02, PLATFORM_HALF)),
    ] {
        platform.box_(c, h, [0.2, 0.9, 1.0], 1.);
    }
    let mut orb = Template::new();
    orb.ball(Vec3::ZERO, Vec3::splat(0.3), [1.0, 0.85, 0.25], 0.9, 14, 9);
    let mut bumper = Template::new();
    bumper.box_(vec3(0., 0., 0.), Vec3::splat(0.4), [0.9, 0.2, 0.3], 0.25);
    Scene { sky: sky.to_meshes(), platform: platform.to_meshes(), orb, bumper }
}

/// Turn one simulation event into sound, particles, shake and text. The `match` is exhaustive on
/// purpose: a new `Event` variant must be handled (or explicitly ignored) here.
fn react(event: &Event, sounds: &mut SoundBank, fx: &mut Fx, juice: &mut Juice) {
    let v = |p: vesper3d::math::V| vec3(p.0, p.1, p.2);
    match event {
        Event::Collected { at, score } => {
            sounds.play(sound(synth::Preset::Coin), 0.8);
            fx.sparks(v(*at), 24, 6., [1., 0.85, 0.25]);
            fx.ring(v(*at), Vec3::Y, 0.2, 1.6, 0.5, [1., 0.9, 0.4]);
            fx.popup(v(*at) + vec3(0., 0.6, 0.), format!("+1  ({score})"), [1., 0.95, 0.5], 34.);
            juice.kick(2.);
        }
        Event::Bumped { at } => {
            sounds.play(sound(synth::Preset::Hit), 1.);
            fx.sparks(v(*at), 30, 8., [1., 0.4, 0.4]);
            fx.dust(v(*at) - vec3(0., 0.3, 0.), 8, 2., [0.6, 0.5, 0.6]);
            juice.shake(0.6);
            juice.stop(0.06);
            juice.flash([1., 0.2, 0.2], 0.5);
        }
        Event::Jumped => sounds.play(sound(synth::Preset::Jump), 0.5),
        Event::Landed => {
            sounds.play(sound(synth::Preset::Land), 0.6);
            juice.land(0.5);
        }
        Event::Fell => {
            sounds.play(sound(synth::Preset::GameOver), 1.);
            fx.banner("YOU FELL", "press R to try again", [1., 0.4, 0.5]);
            juice.shake(1.);
        }
    }
}

/// F5: write the run to the quick slot. Returns the banner to show.
fn quick_save(sim: &Sim, slots: &SaveSlots) -> (&'static str, String, [f32; 3]) {
    if sim.over {
        return ("NOT SAVED", "the run is over".into(), [1., 0.6, 0.3]);
    }
    match snapshot::save_to_slot(sim, slots, QUICK_SLOT, &format!("Quick save, {} orbs", sim.score)) {
        Ok(()) => ("GAME SAVED", "F9 loads it".into(), [0.4, 1., 0.6]),
        Err(e) => ("SAVE FAILED", e.to_string(), [1., 0.4, 0.4]),
    }
}

/// F9: resume the quick slot. A damaged save falls back to the previous good one, and the banner says so.
fn quick_load(sim: &mut Sim, slots: &SaveSlots) -> (&'static str, String, [f32; 3]) {
    match snapshot::load_from_slot(sim, slots, QUICK_SLOT) {
        Ok((header, Source::Primary)) => ("GAME LOADED", header.label, [0.4, 0.9, 1.]),
        Ok((header, Source::Backup(_))) => ("LOADED THE PREVIOUS SAVE", header.label, [1., 0.8, 0.4]),
        Err(e) => ("LOAD FAILED", e.to_string(), [1., 0.4, 0.4]),
    }
}

/// Held device state and the press edge for this frame from a `--script`.
fn scripted(script: &Timeline, frame: u32) -> (Held, bool, [f32; 2]) {
    let mut held = Held::default();
    let mut look = [0.; 2];
    for cue in script.active(frame) {
        match cue.name.as_str() {
            "fwd" => held.forward += 1.,
            "back" => held.forward -= 1.,
            "right" => held.right += 1.,
            "left" => held.right -= 1.,
            "look" => {
                look[0] += cue.value(0);
                look[1] += cue.value(1);
            }
            _ => {}
        }
    }
    (held, script.starting(frame).any(|c| c.name == "jump"), look)
}

#[macroquad::main(window)]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let identity = identity();
    let plan = CapturePlan::from_args(&args).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2)
    });
    if let Some(plan) = &plan {
        if let Err(e) = plan.create_dir() {
            eprintln!("{e}");
            std::process::exit(2);
        }
    }
    let script = flag_value(&args, "--script").map(|text| {
        Timeline::parse(text, &["fwd", "back", "left", "right", "jump", "look", "save", "load"]).unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(2)
        })
    });
    let seed = flag_value(&args, "--seed").and_then(|s| s.parse().ok()).unwrap_or_else(|| {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64)
    });
    // Captures and scripted runs are fixed-step and silent, whatever the display does.
    let unattended = plan.is_some() || script.is_some();

    let materials = Materials::load().expect("the materials failed to compile");
    let look = Look::dusk();
    let scene = build_scene();
    let mut sounds = SoundBank::start(has_flag(&args, "--mute") || unattended, 0.9, 0.6, render_audio).await;
    let mut shell = GameShell::new();
    let mut input = ClientInput::new();
    let (mut acc, mut stepper) = (InputAccumulator::<Held>::new(), FixedStepper::new());
    let (mut fx, mut juice) = (Fx::new(seed), Juice::default());
    let mut sim = Sim::new(seed);
    let slots = flag_value(&args, "--save-dir").map_or_else(SaveSlots::beside_exe, SaveSlots::new);
    if let Some(target) = flag_value(&args, "--load") {
        match snapshot::load_target(&mut sim, &slots, target) {
            Ok((header, _)) => println!("loaded: {}", header.label),
            Err(e) => {
                eprintln!("--load {target}: {e}");
                std::process::exit(2);
            }
        }
    }
    let mut perf = PerfReport::new(30);
    let vignette = hud::make_vignette();
    let (mut world, mut alpha, mut add) = (Batch::new(), Batch::new(), Batch::new());
    let mut frame: u32 = 0;
    let mut time = 0.;

    loop {
        let began = Instant::now();
        // The cursor is captured while a run is in progress; the shell's menu releases it.
        input.begin_frame_with_keyboard(&mut shell, !sim.over, unattended || platform::focused(), platform::keyboard());
        let dt = if unattended { TICK } else { input.frame_seconds() };
        sounds.poll().await;
        if sounds.ready() {
            sounds.start_music();
        }
        time += dt;

        // 1. Devices in: one frame of held state, press edges and look motion.
        let (held, jump, look_delta) = match &script {
            Some(script) => scripted(script, frame),
            None => {
                let m = input.movement(&shell);
                (Held { forward: m.forward, right: m.right }, m.jump, input.look_delta_with(&shell, dt))
            }
        };
        acc.feed(held, if jump { JUMP } else { 0 }, look_delta);
        let (save, load) = match &script {
            Some(script) => (
                script.starting(frame).any(|c| c.name == "save"),
                script.starting(frame).any(|c| c.name == "load"),
            ),
            None => (input.pressed(KeyCode::F5), input.pressed(KeyCode::F9)),
        };
        if save {
            let (title, detail, color) = quick_save(&sim, &slots);
            fx.banners.clear();
            fx.banner(title, &detail, color);
        }
        if load {
            let (title, detail, color) = quick_load(&mut sim, &slots);
            fx.banners.clear();
            fx.banner(title, &detail, color);
            // The saved moment replaces everything in flight: inputs, leftover time, effects.
            acc.clear();
            stepper = FixedStepper::new();
            juice = Juice::default();
        }
        if sim.over && (input.pressed(KeyCode::R) || input.pressed(KeyCode::Enter)) {
            sim = Sim::new(seed.wrapping_add(u64::from(frame)));
            fx.clear();
            acc.clear();
        }

        // 2. Simulation: whole fixed ticks, each with exactly one Input.
        let playing = !shell.paused;
        let scale = juice.time_scale(None);
        if playing {
            for _ in 0..stepper.advance(dt * scale) {
                let tick = acc.take_tick();
                sim.step(&Input { forward: tick.held.forward, right: tick.held.right, look: tick.look, jump: tick.pressed(JUMP) });
                for event in sim.drain_events() {
                    react(&event, &mut sounds, &mut fx, &mut juice);
                }
            }
            juice.update(dt);
            fx.update(dt);
        } else {
            acc.clear();
        }

        // 3. Camera: the simulation's pose plus any look motion no tick has consumed yet.
        let pending = acc.pending_look();
        let (shake, roll) = juice.camera_shake();
        let eye = vec3(sim.player.position.0, sim.player.position.1 + juice.dip, sim.player.position.2);
        let mut view = View::first_person(
            eye + vec3(shake.0, shake.1, shake.2),
            sim.player.yaw + pending[0],
            (sim.player.pitch - pending[1]).clamp(-1.5, 1.5),
        );
        view.roll = roll;
        view.fov = (72. + juice.fov_kick).to_radians();

        // 4. Draw: sky, world, translucent effects, additive effects, then the 2D layer.
        clear_background(look.clear_color());
        set_camera(&view.sky_camera());
        gl_use_material(&materials.sky);
        for mesh in &scene.sky {
            draw_mesh(mesh);
        }
        set_camera(&view.camera(0.05, 400.));
        materials.set_scene(&look, view.eye, time, 0.5 + 0.5 * (time * 3.).sin());
        world.clear();
        alpha.clear();
        add.clear();
        for (i, orb) in sim.orbs.iter().enumerate() {
            let bob = 0.08 * (time * 3. + i as f32 * 2.).sin();
            let at = vec3(orb.0, orb.1 + bob, orb.2);
            world.add(&scene.orb, Mat4::from_translation(at), Tint::NONE);
        }
        for bumper in &sim.bumpers {
            world.add(&scene.bumper, Mat4::from_translation(vec3(bumper.pos.0, bumper.pos.1, bumper.pos.2)), Tint::NONE);
        }
        fx.draw(&mut add, &mut alpha, view.eye, view.right(), view.up());
        gl_use_material(&materials.world);
        for mesh in &scene.platform {
            draw_mesh(mesh);
        }
        world.draw();
        gl_use_material(&materials.fx_alpha);
        alpha.draw();
        gl_use_material(&materials.fx_add);
        add.draw();
        gl_use_default_material();
        set_default_camera();

        let ui = hud::ui_scale();
        hud::overlay(&vignette, Color::new(0.05, 0., 0.1, 0.5));
        if juice.flash > 0.01 {
            draw_rectangle(0., 0., screen_width(), screen_height(), hud::col(juice.flash_color, juice.flash * 0.4));
        }
        hud::draw_popups(&fx.popups, &view, ui);
        hud::draw_banners(&fx.banners, ui);
        hud::panel(20. * ui, 16. * ui, 190. * ui, 62. * ui, 12. * ui, Color::new(0.03, 0.01, 0.1, 0.6));
        hud::text_outlined("ORBS", 34. * ui, 38. * ui, 16. * ui, hud::col([0.3, 0.9, 1.], 1.));
        hud::text_outlined(&sim.score.to_string(), 34. * ui, 70. * ui, 36. * ui, WHITE);
        hud::crosshair(ui, 0., Color::new(1., 1., 1., 0.85));
        let controls: Vec<&str> = identity.controls.split(", ").collect();
        if shell.local_menu(&identity.title, &controls) {
            break;
        }

        // 5. Evidence for a caller that cannot watch: screenshots, then exit.
        if let Some(plan) = &plan {
            if plan.wants(frame) {
                match kit::capture::save_frame(&plan.path_for(frame)) {
                    Ok((w, h)) => println!(
                        "{{\"frame\":{frame},\"width\":{w},\"height\":{h},\"path\":{:?}}}",
                        plan.path_for(frame).display().to_string()
                    ),
                    Err(e) => eprintln!("capture failed: {e}"),
                }
            }
            if plan.finished(frame + 1) {
                break;
            }
        }
        if has_flag(&args, "--perf") {
            perf.frame(dt);
            perf.work(began.elapsed().as_secs_f32());
        }
        frame += 1;
        next_frame().await;
    }
    if has_flag(&args, "--perf") {
        println!("{}", perf.text(25.));
    }
}
