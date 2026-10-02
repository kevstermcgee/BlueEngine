//! {{title}}: the window, renderer, sound and input around the simulation in the library.
//!
//! Play it, or drive it without a human (an agent cannot watch a window); `devkit::Lifecycle` handles these:
//!   --capture DIR [--frames 30,90] [--exit-after N]   save screenshots (DIR must be new), then exit
//!   --script "fwd:0-200,look:0.01@0-100,jump@60"      drive the human input path from a cue script
//!   --seed N   --size WxH   --mute   --perf           reproducible run, window size, silence, frame times
//!   --shadows off|simple|full                          shadow tier for this run (Esc > Settings changes and remembers it)
//!   --load SLOT_OR_FILE   --save-dir DIR                resume a saved game / where F5 saves (default: next to the exe)
//! F5 saves the run to the `quick` slot and F9 loads it. `--script` accepts `save@N` and `load@N` cues too.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod platform;

use macroquad::prelude::*;
use std::sync::{Arc, OnceLock};
use vesper3d::viewer::{
    devkit::{
        beside_exe, downloads_dir, flag_value, has_flag, parse_size, sanitize_filename, synth,
        unique_path, Juice, Lifecycle, Notice, Settings, ShadowQuality,
    },
    game_client::{self, AudioMenu, GameShell},
    game_input::ClientInput,
    identity::Identity,
    kit::{self, hud, Batch, Fx, Look, Materials, Rendered, Shadows, SoundBank, Template, Tint, View},
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
/// The cue names a `--script` may use (`save` and `load` are always understood).
const CUES: [&str; 6] = ["fwd", "back", "left", "right", "jump", "look"];

/// Sounds by index: the engine's synthesised presets, rendered on a worker thread.
const SOUNDS: [synth::Preset; 5] =
    [synth::Preset::Coin, synth::Preset::Hit, synth::Preset::Jump, synth::Preset::Land, synth::Preset::GameOver];
fn sound(preset: synth::Preset) -> usize {
    SOUNDS.iter().position(|p| *p == preset).unwrap_or(0)
}
/// Not every game needs music: generated ambient music can fight a gameplay mechanic that depends on
/// precise or diegetic audio (rhythm timing, sound-based detection, a soundtrack the game itself is
/// about), or just not suit the game's feel. Set this to `false` to ship with sound effects only; the
/// Settings screen adapts on its own (it drops the music toggle and "Save music" button, not just hides
/// them silently as broken). Default on because most games benefit from it, not because every game must
/// have it.
const HAS_MUSIC: bool = true;

/// Renders on the worker thread; `music_wav` is filled once so the Settings-screen "Save music" button
/// can hand the player the exact bytes the stem below plays, without regenerating or hitching.
/// `ambient_spec_for` keys the track to this game's own title and tagline (see assets/identity.json),
/// so a fresh game does not sound identical to every other game made from this template; replace the
/// spec with your own if your theme calls for something that heuristic cannot read from those words, or
/// turn [`HAS_MUSIC`] off if this game should not have music at all.
fn render_audio(music_wav: Arc<OnceLock<Vec<u8>>>, title: &str, tagline: &str) -> Rendered {
    let stems = if HAS_MUSIC {
        let spec = synth::ambient_spec_for(title, tagline);
        let ambient = synth::wav_bytes(&synth::ambient_loop(&spec), synth::RATE);
        let _ = music_wav.set(ambient.clone());
        vec![ambient]
    } else {
        Vec::new()
    };
    Rendered {
        sfx: SOUNDS
            .iter()
            .map(|p| (0..p.variants()).map(|v| synth::wav_bytes(&synth::render(*p, v, 7), synth::RATE)).collect())
            .collect(),
        stems,
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

/// Show the outcome of a quick save or load.
fn announce(fx: &mut Fx, notice: &Notice) {
    fx.banners.clear();
    fx.banner(notice.title, notice.detail.clone(), notice.color);
}

/// Write the currently playing ambient track to the player's Downloads folder as a `.wav`, named after
/// the game, never overwriting an earlier save of it.
fn save_music(music_wav: &OnceLock<Vec<u8>>, title: &str) -> Notice {
    let Some(bytes) = music_wav.get() else {
        return Notice {
            title: "NOT READY",
            detail: "the music is still rendering; try again in a moment".into(),
            color: [1., 0.7, 0.3],
            ok: false,
        };
    };
    let Some(dir) = downloads_dir() else {
        return Notice {
            title: "SAVE FAILED",
            detail: "could not find your Downloads folder".into(),
            color: [1., 0.5, 0.3],
            ok: false,
        };
    };
    let path = unique_path(&dir, &format!("{} - Ambient Music", sanitize_filename(title)), "wav");
    match std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&path, bytes)) {
        Ok(()) => Notice {
            title: "MUSIC SAVED",
            detail: path.display().to_string(),
            color: [0.4, 0.9, 0.6],
            ok: true,
        },
        Err(error) => Notice {
            title: "SAVE FAILED",
            detail: error.to_string(),
            color: [1., 0.5, 0.3],
            ok: false,
        },
    }
}

#[macroquad::main(window)]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let identity = identity();
    // The run flags, the fixed-step loop, quick save/load and evidence for a caller that cannot watch.
    let mut life = Lifecycle::<Held>::start_or_exit(&args, &CUES);
    let seed = life.seed();
    let unattended = life.options.unattended();

    let materials = Materials::load().expect("the materials failed to compile");
    let look = Look::dusk();
    let scene = build_scene();
    // Audio settings survive a relaunch next to the exe (devkit::save, ADR 0017's packaging rule
    // already preserves this file). `music_wav` is filled on the worker thread, once, with exactly the
    // bytes the music stem below plays, so the Settings screen can save them without regenerating.
    let settings_path = beside_exe("settings.json");
    let mut settings = Settings::load(&settings_path);
    let music_wav: Arc<OnceLock<Vec<u8>>> = Arc::new(OnceLock::new());
    let mut sounds = SoundBank::start(life.options.silent(), settings.sfx_level(), settings.music_level(), {
        let music_wav = music_wav.clone();
        let (title, tagline) = (identity.title.clone(), identity.tagline.clone());
        move || render_audio(music_wav, &title, &tagline)
    })
    .await;
    // Shadows: Off, Simple (contact blobs, the default) or Full (a shadow map). `--shadows` overrides the
    // remembered setting for this run only. Delete this block, the `shadows.` lines in the frame and the
    // `cycle_shadows` handling if the game wants no shadows.
    let quality = match ShadowQuality::from_flag(&args) {
        Ok(flag) => flag.unwrap_or(settings.shadow_quality),
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };
    let mut shadows = Shadows::new(quality);
    let mut shell = GameShell::new();
    let mut input = ClientInput::new();
    let (mut fx, mut juice) = (Fx::new(seed), Juice::default());
    let mut sim = Sim::new(seed);
    life.load_flag_or_exit(&mut sim);
    let vignette = hud::make_vignette();
    let (mut world, mut alpha, mut add) = (Batch::new(), Batch::new(), Batch::new());

    loop {
        // The cursor is captured while a run is in progress; the shell's menu releases it.
        input.begin_frame_with_keyboard(&mut shell, !sim.over, unattended || platform::focused(), platform::keyboard());
        let dt = life.begin_frame(input.frame_seconds());
        let time = life.time();
        sounds.poll().await;
        if sounds.ready() {
            sounds.start_music();
        }
        sounds.update_music(dt, &[1.]);

        // 1. Devices in: one frame of held state, press edges and look motion (from the script when there is one).
        let (held, jump, look_delta) = match life.script() {
            Some(s) => (
                Held { forward: s.axis("fwd", "back"), right: s.axis("right", "left") },
                s.starts("jump"),
                [s.value("look", 0), s.value("look", 1)],
            ),
            None => {
                let m = input.movement(&shell);
                (Held { forward: m.forward, right: m.right }, m.jump, input.look_delta_with(&shell, dt))
            }
        };
        life.feed(held, if jump { JUMP } else { 0 }, look_delta);
        let (save, load) = life.save_load_requested(input.pressed(KeyCode::F5), input.pressed(KeyCode::F9));
        if save {
            let notice = if sim.over {
                Notice::refused("the run is over")
            } else {
                life.quick_save(&sim, &format!("Quick save, {} orbs", sim.score))
            };
            announce(&mut fx, &notice);
        }
        if load {
            let notice = life.quick_load(&mut sim);
            announce(&mut fx, &notice);
            if notice.ok {
                // The saved moment replaces everything in flight (the lifecycle dropped pending input already).
                juice = Juice::default();
            }
        }
        if input.restart_requested(sim.over) {
            sim = Sim::new(life.restart_seed());
            fx.clear();
            life.reset_input();
        }

        // 2. Simulation: whole fixed ticks, each with exactly one Input.
        let playing = !shell.paused;
        for _ in 0..life.ticks(dt, juice.time_scale(None), playing) {
            let tick = life.take_tick();
            sim.step(&Input { forward: tick.held.forward, right: tick.held.right, look: tick.look, jump: tick.pressed(JUMP) });
            for event in sim.drain_events() {
                react(&event, &mut sounds, &mut fx, &mut juice);
            }
        }
        if playing {
            juice.update(dt);
            fx.update(dt);
        }

        // 3. Camera: the simulation's pose plus any look motion no tick has consumed yet.
        let pending = life.pending_look();
        let (shake, roll) = juice.camera_shake();
        let eye = vec3(sim.player.position.0, sim.player.position.1 + juice.dip, sim.player.position.2);
        let mut view = View::first_person(
            eye + vec3(shake.0, shake.1, shake.2),
            sim.player.yaw + pending[0],
            (sim.player.pitch - pending[1]).clamp(-1.5, 1.5),
        );
        view.roll = roll;
        view.fov = (72. + juice.fov_kick).to_radians();

        // 4. Draw: the dynamic geometry is filled first (CPU only), so the shadow pass can reuse it; then sky,
        // world, blobs, translucent effects, additive effects, then the 2D layer.
        clear_background(look.clear_color());
        world.clear();
        alpha.clear();
        add.clear();
        shadows.begin_frame(&look, eye);
        for (i, orb) in sim.orbs.iter().enumerate() {
            let bob = 0.08 * (time * 3. + i as f32 * 2.).sin();
            let at = vec3(orb.0, orb.1 + bob, orb.2);
            world.add(&scene.orb, Mat4::from_translation(at), Tint::NONE);
            shadows.blob(vec3(orb.0, 0., orb.2), 0.45);
        }
        for bumper in &sim.bumpers {
            world.add(&scene.bumper, Mat4::from_translation(vec3(bumper.pos.0, bumper.pos.1, bumper.pos.2)), Tint::NONE);
            shadows.blob(vec3(bumper.pos.0, 0., bumper.pos.2), 0.7);
        }
        fx.draw(&mut add, &mut alpha, view.eye, view.right(), view.up());
        shadows.cast(|| world.draw()); // Full only: the same meshes, seen from the sun
        set_camera(&view.sky_camera());
        gl_use_material(&materials.sky);
        for mesh in &scene.sky {
            draw_mesh(mesh);
        }
        set_camera(&view.camera(0.05, 400.));
        materials.set_scene(&look, view.eye, time, 0.5 + 0.5 * (time * 3.).sin());
        shadows.apply(&materials);
        materials.draw_static(&scene.platform);
        shadows.draw_decals(&materials); // after the static world, before the actors
        gl_use_material(&materials.world);
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
        let audio_menu = AudioMenu {
            music_on: settings.music_on,
            sfx_on: settings.sfx_on,
            has_music: HAS_MUSIC,
        };
        let outcome = shell.local_menu_with_options(&identity.title, &controls, audio_menu, shadows.quality());
        if outcome.toggle_music {
            settings.toggle_music();
            sounds.music_volume = settings.music_level();
            settings.store(&settings_path);
        }
        if outcome.toggle_sfx {
            settings.toggle_sfx();
            sounds.sfx_volume = settings.sfx_level();
            settings.store(&settings_path);
        }
        if outcome.cycle_shadows {
            shadows.set_quality(shadows.quality().next());
            settings.shadow_quality = shadows.quality();
            settings.store(&settings_path);
        }
        if outcome.download_music {
            announce(&mut fx, &save_music(&music_wav, &identity.title));
        }
        if outcome.quit {
            break;
        }

        // 5. Evidence for a caller that cannot watch: screenshots and frame times, then exit.
        if let Some(path) = life.capture_path() {
            life.captured(&path, kit::capture::save_frame(&path).map_err(|e| e.to_string()));
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
