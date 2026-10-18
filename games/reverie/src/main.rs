//! Native first-person presentation, device input and composed adaptive music.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod gallery_ui;
mod platform;
mod visual;
use gallery_ui::{Page, Preferences, Ui};
use macroquad::prelude::*;
use reverie::{
    collection::{MOTTO, THEMES},
    Event, Input, Sim,
};
use vesper3d::viewer::{
    devkit::{beside_exe, flag_value, has_flag, parse_size, runtime_assets, Lifecycle, Snapshot},
    game_client::{self, GameShell},
    game_input::ClientInput,
    gamepad::Button,
    identity::Identity,
    kit::{self, AudioBank, AudioState, Batch, Materials, View},
};
const IDENTITY: &str = include_str!("../assets/identity.json");
const MOVEMENTS: [&str; 8] = ["wonder", "longing", "grief", "joy", "unease", "anger", "tenderness", "acceptance"];
const CUES: [&str; 12] =
    ["fwd", "back", "left", "right", "look", "interact", "inspect", "leave", "sprint", "map", "hint", "pause"];
#[derive(Clone, Copy, Default)]
struct Held {
    forward: f32,
    right: f32,
    sprint: bool,
}
const INTERACT: u32 = 1;
const INSPECT: u32 = 2;
const LEAVE: u32 = 4;
fn window() -> macroquad::conf::Conf {
    platform::attach_console();
    let id = Identity::parse(IDENTITY).expect("Reverie identity");
    let mut c = game_client::window_config_with_icon(
        &id.title,
        game_client::icon_from_rgba(
            include_bytes!("../assets/icon_16.rgba"),
            include_bytes!("../assets/icon_32.rgba"),
            include_bytes!("../assets/icon_64.rgba"),
        ),
    );
    let args: Vec<String> = std::env::args().collect();
    if let Some((w, h)) = flag_value(&args, "--size").and_then(parse_size) {
        c.miniquad_conf.window_width = w.clamp(640, 3840) as i32;
        c.miniquad_conf.window_height = h.clamp(480, 2160) as i32;
    }
    if has_flag(&args, "--novsync") {
        c.miniquad_conf.platform.swap_interval = Some(0);
    }
    c
}
fn mood(sim: &Sim) -> usize {
    if sim.depth() == 0 {
        sim.emotion()
    } else {
        sim.world_emotion()
    }
}
fn notice(ui: &mut Ui, n: vesper3d::viewer::devkit::Notice) {
    ui.announce(format!("{}: {}", n.title, n.detail));
}
#[macroquad::main(window)]
async fn main() {
    vesper3d::viewer::startup::show().await;
    let mut args: Vec<String> = std::env::args().collect();
    // A designed first visit; --seed still allows completely different museums.
    if flag_value(&args, "--seed").is_none() {
        args.extend(["--seed".into(), "3".into()]);
    }
    let mut life = Lifecycle::<Held>::start_or_exit(&args, &CUES);
    let unattended = life.options.unattended();
    let root = runtime_assets("assets", std::path::Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2)
    });
    let settings_path = beside_exe("settings.json");
    let mut prefs = Preferences::load(&settings_path);
    let mut sim = Sim::new(life.seed());
    life.load_flag_or_exit(&mut sim);
    let materials = Materials::load().expect("BlueEngine materials");
    let canvas = visual::canvas_material();
    let mut scene = visual::Scene::build(&sim);
    let mut world = Batch::new();
    let mut audio_mood = mood(&sim);
    let mut audio = AudioBank::load(
        root.join("audio").join(MOVEMENTS[audio_mood]),
        life.options.silent(),
        prefs.audio.sfx_level(),
        prefs.audio.music_level(),
    )
    .await
    .unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2)
    });
    let mut fading: Option<(AudioBank, f32)> = None;
    let mut ui = Ui::new();
    let mut shell = GameShell::new();
    let mut input = ClientInput::new();
    if !unattended && !has_flag(&args, "--windowed") && !has_flag(&args, "--size") {
        shell.fullscreen = prefs.audio.fullscreen;
        set_fullscreen(shell.fullscreen);
    }
    shell.paused = !unattended;
    if has_flag(&args, "--menu") {
        shell.paused = true;
    }
    let mut transition = 0f32;
    let mut has_save = life.options.load.is_some() || beside_exe("saves/quick.be2save").is_file();
    let mut failures = false;
    loop {
        let was_paused = shell.paused;
        input.begin_frame_with_keyboard(&mut shell, true, unattended || platform::focused(), platform::keyboard());
        let dt = life.begin_frame(input.frame_seconds());
        let time = life.time();
        // Escape backs out of a subpage before resuming. The shared shell still owns cursor/focus.
        if was_paused && !shell.paused && matches!(ui.page, Page::Map | Page::Settings) {
            ui.open(Page::Pause);
            shell.paused = true;
        }
        if !was_paused && shell.paused {
            ui.open(Page::Pause);
        }
        let scripted = life.script();
        let open_map = input.pressed(KeyCode::M) || scripted.is_some_and(|s| s.starts("map"));
        if open_map && !shell.paused {
            ui.open(Page::Map);
            shell.paused = true;
        }
        if scripted.is_some_and(|s| s.starts("pause")) {
            ui.open(Page::Pause);
            shell.paused = true;
        }
        if input.pressed(KeyCode::H) || scripted.is_some_and(|s| s.starts("hint")) {
            ui.hint_time = 9.;
        }
        audio.sounds.poll().await;
        if audio.sounds.state() == AudioState::Failed && !failures {
            eprintln!("Audio failed: {:?}", audio.sounds.errors());
            ui.announce("The soundtrack could not load.");
            failures = true;
        }
        let mut bits = 0;
        let (held, look) = if shell.paused {
            (Held::default(), [0., 0.])
        } else if let Some(s) = scripted {
            if s.starts("interact") {
                bits |= INTERACT;
            }
            if s.starts("inspect") {
                bits |= INSPECT;
            }
            if s.starts("leave") {
                bits |= LEAVE;
            }
            (
                Held { forward: s.axis("fwd", "back"), right: s.axis("right", "left"), sprint: s.held("sprint") },
                [s.value("look", 0), s.value("look", 1)],
            )
        } else {
            let pad = input.gamepad();
            if shell.accepting_input() {
                if input.pressed(KeyCode::E) || pad.pressed(Button::South) {
                    bits |= INTERACT;
                }
                if input.pressed(KeyCode::Q) || pad.pressed(Button::West) {
                    bits |= INSPECT;
                }
                if input.pressed(KeyCode::B) || pad.pressed(Button::East) {
                    bits |= LEAVE;
                }
            }
            let movement = input.movement(&shell);
            let d = input.look_delta_with(&shell, dt);
            (
                Held { forward: movement.forward, right: movement.right, sprint: movement.sprint },
                [d[0] * prefs.audio.sensitivity, d[1] * prefs.audio.sensitivity],
            )
        };
        life.feed(held, bits, look);
        let (save, load) = life.save_load_requested(input.pressed(KeyCode::F5), input.pressed(KeyCode::F9));
        if save {
            let n = life
                .quick_save(&sim, &format!("{} galleries, {} keys", sim.archive.visited.len(), sim.archive.keys.len()));
            has_save |= n.ok;
            notice(&mut ui, n);
        }
        if load {
            let n = life.quick_load(&mut sim);
            if n.ok {
                transition = 1.;
            }
            notice(&mut ui, n);
        }
        for _ in 0..life.ticks(dt, 1., !shell.paused) {
            let tick = life.take_tick();
            sim.step(&Input {
                forward: tick.held.forward,
                right: tick.held.right,
                look: tick.look,
                sprint: tick.held.sprint,
                interact: tick.pressed(INTERACT),
                inspect: tick.pressed(INSPECT),
                leave: tick.pressed(LEAVE),
            });
            for event in sim.drain_events() {
                let cue = match event {
                    Event::Footstep => {
                        if prefs.footsteps {
                            Some(("step", 0.22))
                        } else {
                            None
                        }
                    }
                    Event::Entered => {
                        transition = 1.;
                        ui.announce(format!("{}  /  {}", sim.entered_art().name, MOTTO[mood(&sim)]));
                        Some(("enter", 0.5))
                    }
                    Event::Returned => {
                        transition = 0.8;
                        ui.announce("The frame remembers where you stood.");
                        Some(("enter", 0.3))
                    }
                    Event::DoorOpened => {
                        transition = 1.;
                        ui.announce(format!("The {} gallery. {}", THEMES[sim.emotion()], MOTTO[sim.emotion()]));
                        Some(("enter", 0.4))
                    }
                    Event::KeyFound => {
                        ui.announce("A memory made golden. Both gallery doors are open.");
                        Some(("key", 0.65))
                    }
                    Event::Locked => {
                        ui.announce(sim.hint());
                        Some(("locked", 0.25))
                    }
                    Event::Awakened(slot) => {
                        ui.announce(if sim.key_host() == slot && !sim.has_key() {
                            "Something golden is waking inside this work."
                        } else {
                            MOTTO[sim.emotion()]
                        });
                        Some(("wake", 0.55))
                    }
                    Event::Inspected(art) => {
                        ui.open(Page::Inspect(art));
                        shell.paused = true;
                        None
                    }
                };
                if let Some((name, level)) = cue {
                    if audio.sounds.state() == AudioState::Ready {
                        if let Err(e) = audio.play(name, level) {
                            eprintln!("{e}");
                            failures = true;
                        }
                    }
                }
            }
            if shell.paused {
                break;
            }
        }
        let new_mood = mood(&sim);
        if new_mood != audio_mood {
            let next = AudioBank::load(
                root.join("audio").join(MOVEMENTS[new_mood]),
                life.options.silent(),
                prefs.audio.sfx_level(),
                0.,
            )
            .await
            .unwrap_or_else(|e| {
                eprintln!("{e}");
                std::process::exit(2)
            });
            if let Some((mut previous, _)) = fading.take() {
                previous.sounds.stop_music();
            }
            fading = Some((audio, 3.5));
            audio = next;
            audio_mood = new_mood;
        }
        let age = if let Some((_, remaining)) = &fading { (1. - remaining / 3.5).clamp(0., 1.) } else { 1. };
        audio.sounds.music_volume = prefs.audio.music_level() * age;
        audio.sounds.sfx_volume = prefs.audio.sfx_level();
        if audio.sounds.state() == AudioState::Ready {
            let depth = sim.depth() as f32;
            if let Err(e) = audio.music(
                dt,
                &[
                    ("piano", if shell.paused { 0.62 } else { 0.9 }),
                    ("strings", (0.48 + depth * 0.12).min(0.9)),
                    ("glints", if depth > 0. { 0.75 } else { 0.38 }),
                ],
            ) {
                eprintln!("{e}");
                failures = true;
            }
        }
        if let Some((old, remaining)) = &mut fading {
            *remaining -= dt;
            old.sounds.music_volume = prefs.audio.music_level() * (*remaining / 3.5).max(0.);
            old.sounds.poll().await;
            if old.sounds.state() == AudioState::Ready {
                let _ = old.music(dt, &[("piano", 0.7), ("strings", 0.5), ("glints", 0.5)]);
            }
            if *remaining <= 0. {
                old.sounds.stop_music();
                fading = None;
            }
        }
        let stamp = (if sim.depth() == 0 { sim.seed() } else { sim.world_seed() }, sim.depth());
        if scene.stamp != stamp {
            scene = visual::Scene::build(&sim);
        }
        transition = (transition - dt * 0.75).max(0.);
        let pending = life.pending_look();
        let p = sim.player.position;
        let bob = if prefs.gentle || shell.paused { 0. } else { (time * 5.8).sin() * 0.012 * held.forward.abs() };
        let mut view = View::first_person(
            vec3(p.0, p.1 + bob, p.2),
            sim.player.yaw + pending[0],
            (sim.player.pitch - pending[1]).clamp(-1.5, 1.5),
        );
        view.fov = (68. + if prefs.gentle { 0. } else { transition * 9. }).to_radians();
        scene.draw(&sim, &materials, &canvas, &view, time, &mut world);
        ui.hud(&sim, dt, transition, prefs.gentle);
        if shell.paused {
            let response = ui.menu(&sim, &input, &mut prefs, has_save);
            if response.fullscreen {
                shell.fullscreen = !shell.fullscreen;
                set_fullscreen(shell.fullscreen);
                prefs.audio.fullscreen = shell.fullscreen;
            }
            if response.changed_settings {
                prefs.store(&settings_path);
            }
            if response.save {
                let n = life.quick_save(
                    &sim,
                    &format!("{} galleries, {} keys", sim.archive.visited.len(), sim.archive.keys.len()),
                );
                has_save |= n.ok;
                notice(&mut ui, n);
            }
            if response.load {
                let n = life.quick_load(&mut sim);
                transition = if n.ok { 1. } else { 0. };
                notice(&mut ui, n);
            }
            if response.resume {
                shell.paused = false;
                ui.open(Page::Pause);
                life.reset_input();
            }
            if response.quit {
                break;
            }
        }
        if let Some(path) = life.capture_path() {
            life.captured(&path, kit::capture::save_frame(&path).map_err(|e| e.to_string()));
        }
        if life.end_frame(dt) || game_client::exit_requested() {
            break;
        }
        next_frame().await;
    }
    if let Some(report) = life.report() {
        println!("{report}");
    }
    println!(
        "museum: {}",
        serde_json::json!({"rooms":sim.archive.visited.len(),"keys":sim.archive.keys.len(),"worlds":sim.archive.worlds.len(),"depth":sim.depth(),"room":sim.archive.room,"position":sim.player.position,"snapshot_tick":sim.save_tick()})
    );
    if life.options.audible {
        let status = audio.sounds.status();
        println!("audio: {status:?}");
        let effects = vesper3d::viewer::devkit::synth::Preset::Footstep.variants()
            + vesper3d::viewer::devkit::synth::Preset::Whoosh.variants()
            + vesper3d::viewer::devkit::synth::Preset::Chime.variants()
            + vesper3d::viewer::devkit::synth::Preset::Success.variants()
            + vesper3d::viewer::devkit::synth::Preset::Back.variants();
        if let Err(e) = status.verify_playback(effects, 3) {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
    if failures {
        std::process::exit(1);
    }
}
