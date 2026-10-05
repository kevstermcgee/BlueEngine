//! Presentation only. Rules are in lib.rs; --playback uses the same Input as devices.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod platform;
use macroquad::prelude::*;
use signal_garden::{audio, Cell, Event, Input, Outcome, Sim, BEACON, HEIGHT, LIMIT, PADS, RELAYS, WIDTH};
use std::sync::OnceLock;
use vesper3d::viewer::{
    devkit::{
        beside_exe, downloads_dir, flag_value, has_flag, parse_size, sanitize_filename, unique_path, Lifecycle, Notice,
        Playback, Settings, Simulation,
    },
    game_client::{self, AudioMenu, GameShell, ShellActions},
    game_input::ClientInput,
    identity::Identity,
    kit::{self, hud, Batch, Fx, Look, Materials, Rendered, SoundBank, Template, Tint},
};

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

const CUES: [&str; 10] = ["fwd", "back", "left", "right", "interact", "menu", "select", "down", "fullscreen", "shield"];

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
        Ok(()) => Notice { title: "MUSIC SAVED", detail: path.display().to_string(), color: [0.4, 0.9, 0.6], ok: true },
        Err(error) => Notice { title: "SAVE FAILED", detail: error.to_string(), color: [1., 0.5, 0.3], ok: false },
    }
}

fn position(c: Cell) -> Vec3 {
    vec3(c.0 as f32 - WIDTH as f32 / 2., 0., c.1 as f32 - HEIGHT as f32 / 2.)
}
fn models() -> (Vec<Mesh>, Template, Template, Template, Template) {
    let mut ground = Template::new();
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let c = Cell(x, y);
            let p = position(c);
            if c.walkable() {
                let color = if (x + y) % 2 == 0 { [0.11, 0.23, 0.19] } else { [0.13, 0.27, 0.22] };
                ground.box_(p - vec3(0., 0.15, 0.), vec3(0.48, 0.15, 0.48), color, 0.);
            } else {
                ground.rounded_box(p + vec3(0., 0.22, 0.), vec3(0.45, 0.4, 0.45), 0.1, [0.10, 0.16, 0.21], 0.);
                ground.ball(p + vec3(0., 0.7, 0.), vec3(0.40, 0.45, 0.40), [0.20, 0.38, 0.29], 0., 8, 6);
            }
        }
    }
    let mut player = Template::new();
    player.cylinder(vec3(0., 0.15, 0.), 0.28, 0.55, [0.45, 0.84, 0.91], 0.1, 12);
    player.ball(vec3(0., 0.85, 0.), vec3(0.28, 0.28, 0.28), [0.93, 0.90, 0.75], 0.2, 12, 8);
    player.cone(vec3(0., 1.05, 0.), 0.4, 0.0, 0.35, [0.20, 0.55, 0.60], 0., 12);
    let mut spark = Template::new();
    spark.ball(vec3(0., 0.65, 0.), Vec3::splat(0.15), [1., 0.78, 0.23], 1., 10, 7);
    let mut patrol = Template::new();
    patrol.rounded_box(vec3(0., 0.4, 0.), Vec3::splat(0.28), 0.10, [0.90, 0.24, 0.36], 0.2);
    patrol.cone(vec3(0., 0.7, 0.), 0.2, 0.02, 0.3, [1., 0.5, 0.4], 0.6, 8);
    let mut relay = Template::new();
    relay.cylinder(vec3(0., 0., 0.), 0.32, 0.65, [0.35, 0.49, 0.45], 0., 12);
    relay.ball(vec3(0., 0.95, 0.), vec3(0.32, 0.4, 0.32), [0.85, 0.9, 0.75], 0.5, 12, 8);
    (ground.to_meshes(), player, spark, patrol, relay)
}
fn react(event: Event, sounds: &mut SoundBank, fx: &mut Fx) {
    let (sound, at, color, label) = match event {
        Event::Pickup(c) => (0, Some(c), [1., 0.8, 0.3], "SPARK +1"),
        Event::Charged(c) => (1, Some(c), [0.4, 1., 0.7], "CHARGE +1"),
        Event::Relay(c) => (2, Some(c), [0.3, 1., 0.8], "RELAY RESTORED"),
        Event::Hurt(c) => (3, Some(c), [1., 0.3, 0.4], "HIT -1 SPARK"),
        Event::Shield(c) => (7, Some(c), [0.3, 0.8, 1.], "SHIELD: 3 SECONDS"),
        Event::Heal => (4, Some(BEACON), [0.5, 1., 0.6], "HEALED"),
        Event::Won => (5, None, [0.6, 1., 0.8], "GARDEN RESTORED"),
        Event::Lost => (6, None, [1., 0.5, 0.5], "SIGNAL LOST"),
    };
    sounds.play(sound, 0.8);
    if let Some(at) = at {
        fx.sparks(position(at) + vec3(0., 0.7, 0.), 20, 3., color);
        fx.popup(position(at) + vec3(0., 1.5, 0.), label, color, 22.);
    }
}
fn centered(text: &str, y: f32, size: f32, color: Color) {
    hud::text_centered(text, screen_width() / 2., y, size, color);
}
#[macroquad::main(window)]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let expected = flag_value(&args, "--expect");
    if (has_flag(&args, "--expect") && expected.is_none())
        || expected.is_some_and(|v| !["won", "lost", "playing"].contains(&v))
    {
        eprintln!("--expect requires won, lost or playing");
        std::process::exit(2);
    }
    if has_flag(&args, "--playback") && flag_value(&args, "--playback").is_none_or(|v| v.starts_with("--")) {
        eprintln!("--playback requires a JSON Input file; generate it with the documented replay test");
        std::process::exit(2);
    }
    let identity = identity();
    let mut life = Lifecycle::<Input>::start_or_exit(&args, &CUES);
    // Playback is game input, never a privileged simulation shortcut. An empty script makes the
    // lifecycle fixed-step even when only --playback was specified by the caller.
    let mut playback = flag_value(&args, "--playback").map(|p| {
        Playback::<Input>::load(std::path::Path::new(p)).expect("--playback must name a valid Input JSON file")
    });
    if playback.is_some() && life.options.script.is_none() {
        life.options.script = Some(vesper3d::viewer::devkit::Timeline::parse("", &CUES).unwrap());
    }
    let mut sim = Sim::new(life.seed());
    life.load_flag_or_exit(&mut sim);
    let settings_path = beside_exe("settings.json");
    let mut settings = Settings::load(&settings_path);
    let mut sounds = SoundBank::start(life.options.silent(), settings.sfx_level(), settings.music_level(), || {
        Rendered { sfx: audio::effects(), stems: vec![audio::music()] }
    })
    .await;
    let music_wav = OnceLock::new();
    let materials = Materials::load().expect("shared materials compile");
    let look = Look::dusk();
    let (ground, player, spark, patrol, relay) = models();
    let mut world = Batch::new();
    let mut add = Batch::new();
    let mut alpha = Batch::new();
    let mut fx = Fx::new(life.seed());
    let mut shell = GameShell::new();
    let mut input = ClientInput::new();
    let mut visual_player = position(sim.state.player);
    let camera = Camera3D {
        position: vec3(0., 18., 12.),
        target: vec3(0., 0., 0.),
        up: Vec3::Y,
        projection: Projection::Orthographics,
        fovy: 16.,
        ..Default::default()
    };
    let mut intro = !life.options.unattended();
    let mut audio_tested = false;
    loop {
        input.begin_frame_with_keyboard(
            &mut shell,
            false,
            life.options.unattended() || platform::focused(),
            platform::keyboard(),
        );
        let dt = life.begin_frame(input.frame_seconds());
        sounds.poll().await;
        if sounds.ready() {
            sounds.start_music();
        }
        sounds.update_music(dt, &[0.65]);
        // --audible proves every effect can be submitted, including outcome cues. This does not
        // claim a human heard them; status distinguishes silent/default captures from audio evidence.
        if life.options.audible && sounds.ready() && !audio_tested {
            for i in 0..audio::SOUNDS.len() {
                sounds.play(i, 0.15);
            }
            audio_tested = true;
        }
        let script = life.script();
        if let Some(s) = script.as_ref() {
            shell.begin_frame_with_actions(
                false,
                true,
                ShellActions {
                    pause: s.starts("menu"),
                    next: s.starts("down"),
                    accept: s.starts("select"),
                    fullscreen: s.starts("fullscreen"),
                    ..Default::default()
                },
            );
        }
        let start = input.pressed(KeyCode::Enter) || input.pressed(KeyCode::Space) || input.menu_select();
        let intent = if let Some(p) = &mut playback {
            p.next_input()
        } else if let Some(s) = script {
            Input {
                dx: s.axis("right", "left") as i32,
                dy: s.axis("back", "fwd") as i32,
                interact: s.held("interact"),
                shield: s.starts("shield"),
            }
        } else {
            let m = input.movement(&shell);
            Input {
                dx: if m.right.abs() > 0.4 { m.right.signum() as i32 } else { 0 },
                dy: if m.forward.abs() > 0.4 { -m.forward.signum() as i32 } else { 0 },
                interact: shell.accepting_input()
                    && (input.down(KeyCode::E) || input.gamepad().down(vesper3d::viewer::gamepad::Button::West)),
                shield: shell.accepting_input()
                    && (input.pressed(KeyCode::Q)
                        || input.gamepad().pressed(vesper3d::viewer::gamepad::Button::LeftTrigger)),
            }
        };
        if intro && start {
            intro = false;
            life.reset_input();
        }
        life.feed(intent, 0, [0.; 2]);
        let (save, load) = life.save_load_requested(input.pressed(KeyCode::F5), input.pressed(KeyCode::F9));
        if save {
            let n = life.quick_save(&sim, "Signal Garden");
            fx.banner(n.title, n.detail, n.color);
        }
        if load {
            let n = life.quick_load(&mut sim);
            if n.ok {
                fx.clear();
                visual_player = position(sim.state.player);
            }
            fx.banner(n.title, n.detail, n.color);
        }
        if input.restart_requested(sim.state.outcome != Outcome::Playing) {
            sim = Sim::new(life.restart_seed());
            life.reset_input();
            fx.clear();
            intro = true;
        }
        for _ in 0..life.ticks(dt, 1., !shell.paused && !intro) {
            let i = life.take_tick().held;
            sim.step(&i);
            for event in sim.drain_events() {
                react(event, &mut sounds, &mut fx);
            }
        }
        let next = position(sim.state.player);
        visual_player = visual_player.lerp(next, 1. - (-dt * 18.).exp());
        if !shell.paused {
            fx.update(dt);
        }
        clear_background(Color::new(0.06, 0.11, 0.14, 1.));
        set_camera(&camera);
        materials.set_scene(&look, camera.position, life.time(), 0.5);
        materials.draw_static(&ground);
        world.clear();
        add.clear();
        alpha.clear();
        world.add(&player, Mat4::from_translation(visual_player), Tint::NONE);
        for (i, c) in RELAYS.iter().enumerate() {
            let tint = if i < sim.state.relay {
                [0.3, 1., 0.7]
            } else if i == sim.state.relay {
                [1., 0.8, 0.2]
            } else {
                [0.3, 0.4, 0.45]
            };
            world.add(&relay, Mat4::from_translation(position(*c)), Tint { mul: tint, ..Tint::NONE });
        }
        for (i, c) in PADS.iter().enumerate() {
            if sim.state.pads[i] == 0 {
                world.add(&spark, Mat4::from_translation(position(*c)), Tint::NONE);
            }
        }
        for c in sim.patrols() {
            world.add(&patrol, Mat4::from_translation(position(c)), Tint::NONE);
        }
        let mut beacon = Template::new();
        beacon.ring(position(BEACON) + vec3(0., 0.02, 0.), 0.3, 0.8, [0.2, 0.8, 0.9], 0.8, 20);
        world.add(&beacon, Mat4::IDENTITY, Tint::NONE);
        if sim.state.shield_ticks > 0 {
            let mut shield = Template::new();
            shield.ring(visual_player + vec3(0., 0.1, 0.), 0.4, 0.55, [0.3, 0.8, 1.], 0.8, 16);
            world.add(&shield, Mat4::IDENTITY, Tint::NONE);
        }
        gl_use_material(&materials.world);
        world.draw();
        fx.draw(&mut add, &mut alpha, camera.position, Vec3::X, Vec3::Y);
        gl_use_material(&materials.fx_add);
        add.draw();
        gl_use_material(&materials.fx_alpha);
        alpha.draw();
        gl_use_default_material();
        set_default_camera();
        let ui = hud::ui_scale();
        // Labels are projected from the actual camera and stay aligned at every window size.
        for (i, c) in RELAYS.iter().enumerate() {
            let point = camera.matrix().project_point3(position(*c) + vec3(0., 1.4, 0.));
            hud::text_centered(
                &format!("{}  {}/3", i + 1, sim.state.charges[i]),
                (point.x + 1.) * screen_width() / 2.,
                (1. - point.y) * screen_height() / 2.,
                18. * ui,
                if i == sim.state.relay { YELLOW } else { WHITE },
            );
        }
        hud::panel(
            12. * ui,
            12. * ui,
            screen_width() - 24. * ui,
            64. * ui,
            8. * ui,
            Color::new(0.02, 0.06, 0.07, 0.90),
        );
        hud::text_outlined(
            &format!("SIGNAL GARDEN   Relay {}/6", sim.state.relay.min(5) + 1),
            24. * ui,
            36. * ui,
            22. * ui,
            WHITE,
        );
        let secs = (LIMIT - sim.state.tick) / 60;
        hud::text_right(
            &format!("{} hearts   {} / 3 sparks   {}:{:02}", sim.state.hearts, sim.state.cargo, secs / 60, secs % 60),
            screen_width() - 24. * ui,
            36. * ui,
            20. * ui,
            WHITE,
        );
        let hint = if sim.state.relay == 6 {
            "All relays restored! Return to the cyan beacon and hold E."
        } else if sim.state.player.distance(RELAYS[sim.state.relay]) <= 1 {
            "Hold E to charge this relay. Each spark takes 1.5 seconds."
        } else if sim.state.player.distance(BEACON) <= 1 && sim.state.hearts < 5 {
            "Hold E at the beacon to heal. Recharge: 30 seconds."
        } else {
            "Collect gold sparks. Carry three to the numbered gold relay. Avoid pink patrols."
        };
        hud::text_outlined(hint, 24. * ui, 60. * ui, 16. * ui, hud::col([0.6, 0.9, 0.85], 1.));
        if sim.charging() > 0. {
            hud::bar(
                screen_width() / 2. - 80. * ui,
                screen_height() - 34. * ui,
                160. * ui,
                12. * ui,
                sim.charging(),
                GREEN,
            );
        }
        hud::draw_banners(&fx.banners, ui);
        if intro || sim.state.outcome != Outcome::Playing {
            draw_rectangle(0., 0., screen_width(), screen_height(), Color::new(0.02, 0.05, 0.06, 0.80));
            let (title, subtitle) = match sim.state.outcome {
                Outcome::Won => ("GARDEN RESTORED", "Every lantern is lit. The night has a way home."),
                Outcome::Lost => ("SIGNAL LOST", "The garden needs its keeper. Try another route."),
                Outcome::Playing => ("SIGNAL GARDEN", "A small night garden. Six lanterns. Eight minutes."),
            };
            centered(title, screen_height() * 0.34, 44. * ui, WHITE);
            centered(subtitle, screen_height() * 0.43, 20. * ui, hud::col([0.6, 0.9, 0.85], 1.));
            centered("WASD / arrows: move     Hold E / pad X: charge & heal", screen_height() * 0.53, 19. * ui, WHITE);
            centered(
                "Gold sparks refill every 15s. Q / pad LB: spend a spark for a 3s shield.",
                screen_height() * 0.59,
                17. * ui,
                WHITE,
            );
            centered(
                "F5 / F9: save & resume     Esc: settings     F: fullscreen",
                screen_height() * 0.65,
                17. * ui,
                WHITE,
            );
            centered(
                if intro { "Enter / Space / pad A to begin" } else { "R / Enter / pad A to play again" },
                screen_height() * 0.76,
                24. * ui,
                YELLOW,
            );
        }
        let controls: Vec<&str> = identity.controls.split(", ").collect();
        let outcome = shell.local_menu_with_audio(
            &identity.title,
            &controls,
            AudioMenu { music_on: settings.music_on, sfx_on: settings.sfx_on, has_music: true },
        );
        if outcome.toggle_music {
            settings.toggle_music();
            sounds.music_volume = settings.music_level();
            if !settings.store(&settings_path) {
                fx.banner("SETTINGS FAILED", "Could not write settings beside the executable", [1., 0.5, 0.3]);
            }
        }
        if outcome.toggle_sfx {
            settings.toggle_sfx();
            sounds.sfx_volume = settings.sfx_level();
            if !settings.store(&settings_path) {
                fx.banner("SETTINGS FAILED", "Could not write settings beside the executable", [1., 0.5, 0.3]);
            }
        }
        if outcome.download_music {
            if music_wav.get().is_none() {
                let _ = music_wav.set(audio::music());
            }
            let n = save_music(&music_wav, &identity.title);
            fx.banner(n.title, n.detail, n.color);
        }
        if outcome.quit {
            break;
        }
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
    let actual = match sim.state.outcome {
        Outcome::Won => "won",
        Outcome::Lost => "lost",
        Outcome::Playing => "playing",
    };
    if expected.is_some_and(|v| v != actual) {
        eprintln!(
            "Game verification failed: expected {}, reached {actual} at tick {}",
            expected.unwrap(),
            sim.state.tick
        );
        std::process::exit(1);
    }
    let status = sounds.status();
    println!(
        "{}",
        serde_json::json!({"garden":{"tick":sim.state.tick,"outcome":format!("{:?}",sim.state.outcome),"hash":format!("{:016x}",sim.state_hash())},"audio":status})
    );
    if life.options.audible {
        let expected_effects = audio::SOUNDS.iter().map(|p| p.variants()).sum();
        if let Err(error) = status.verify_playback(expected_effects, 1) {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
