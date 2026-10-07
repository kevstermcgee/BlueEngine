//! Leo's window: shared fixed-step movement, cached art, checked audio and engine-owned saves.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod platform;
mod scene;
use leo::{Input, Sim, DAY_TICKS};
use macroquad::prelude::*;
use std::path::{Path, PathBuf};
use vesper3d::viewer::{
    devkit::{
        beside_exe, flag_value, has_flag, parse_size, runtime_assets, snapshot, Lifecycle, Settings, ShadowQuality,
        Simulation,
    },
    game_client::{self, AudioMenu, GameShell},
    game_input::ClientInput,
    identity::Identity,
    kit::{self, hud, AudioBank, AudioState, Materials, Shadows},
};
const IDENTITY: &str = include_str!("../assets/identity.json");
fn identity() -> Identity {
    Identity::parse(IDENTITY).expect("invalid Leo identity")
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
#[derive(Clone, Copy, Default)]
struct Held {
    forward: f32,
    right: f32,
    sprint: bool,
}
const JUMP: u32 = 1;
const CUES: [&str; 9] = ["fwd", "back", "left", "right", "sprint", "jump", "look", "menu", "music"];
fn fail(message: impl std::fmt::Display) -> ! {
    eprintln!("Leo: {message}");
    std::process::exit(1)
}
fn asset_root(args: &[String]) -> PathBuf {
    if let Some(path) = flag_value(args, "--assets") {
        return path.into();
    }
    runtime_assets("assets", Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap_or_else(|error| fail(error))
}
fn toggle_music(settings: &mut Settings, path: &Path) -> Result<(), String> {
    settings.toggle_music();
    if settings.store(path) {
        Ok(())
    } else {
        Err("could not save music setting".into())
    }
}
async fn audio(root: &Path, muted: bool, settings: &Settings) -> Result<Option<(AudioBank, AudioBank)>, String> {
    if muted {
        return Ok(None);
    }
    let mut nature = AudioBank::load(root.join("audio/nature"), false, 0., settings.sfx_level()).await?;
    let mut score = AudioBank::load(root.join("audio/music"), false, 0., settings.music_level()).await?;
    let began = std::time::Instant::now();
    loop {
        nature.sounds.poll().await;
        score.sounds.poll().await;
        for bank in [&nature, &score] {
            if matches!(bank.sounds.state(), AudioState::Failed | AudioState::Empty) {
                return Err(format!("audio {:?}: {:?}", bank.sounds.state(), bank.sounds.errors()));
            }
        }
        if nature.sounds.ready() && score.sounds.ready() {
            return Ok(Some((nature, score)));
        }
        if began.elapsed().as_secs() > 30 {
            return Err("audio decoding timed out".into());
        }
        next_frame().await;
    }
}
#[macroquad::main(window)]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut life = Lifecycle::<Held>::start_or_exit(&args, &CUES);
    let unattended = life.options.unattended();
    // Capture is silent by default. Actual audio submission is checked only on remote CI's virtual display.
    let verify_audio = has_flag(&args, "--verify-audio");
    if verify_audio
        && !(cfg!(target_os = "linux")
            && std::env::var("CI").as_deref() == Ok("true")
            && std::env::var("BE2_AUDIO_OFFSCREEN").as_deref() == Ok("1")
            && std::env::var_os("DISPLAY").is_some()
            && std::env::var_os("ALSA_CONFIG_PATH").is_some())
    {
        fail("--verify-audio requires remote Linux CI, a virtual display and explicit null ALSA configuration");
    }
    let muted = life.options.mute || (unattended && !verify_audio);
    let day_ticks = match flag_value(&args, "--day-seconds") {
        Some(v) => match v.parse::<u32>() {
            Ok(s) if (4..=3600).contains(&s) => s * 60,
            _ => fail("--day-seconds must be 4..3600"),
        },
        None => DAY_TICKS,
    };
    let settings_path =
        flag_value(&args, "--settings").map(PathBuf::from).unwrap_or_else(|| beside_exe("settings.json"));
    let mut settings = Settings::load(&settings_path);
    let mut sim = Sim::with_day(life.seed(), day_ticks).unwrap_or_else(|e| fail(e));
    let mut notice = String::new();
    if life.options.load.is_some() {
        life.load_flag_or_exit(&mut sim);
    } else if !unattended && life.slots.path("auto").is_ok_and(|p| p.exists()) {
        if let Err(e) = snapshot::load_from_slot(&mut sim, &life.slots, "auto") {
            notice = format!("Could not resume saved days: {e}");
        }
    }
    let materials = Materials::load().unwrap_or_else(|e| fail(e));
    // Reuse the engine's texel-snapped directional map. Simple contact shadows remain available
    // if this device cannot create the map; report the actual tier in capture evidence.
    let mut shadows = Shadows::new(ShadowQuality::Full).with_range(32., 120.);
    let mut scene = scene::Scene::new();
    let assets = asset_root(&args);
    let mut banks = audio(&assets, muted, &settings).await.unwrap_or_else(|e| fail(e));
    let mut shell = GameShell::new();
    shell.paused = !unattended;
    let mut devices = ClientInput::new();
    let mut evidence = Vec::new();
    let mut submissions = 0u64;
    let mut auto_tick = sim.tick;
    let mut auto_day = sim.time().days;
    let portrait = has_flag(&args, "--portrait");
    let controls = [
        "WASD / left stick: wander",
        "Shift: run   Space: hop",
        "Mouse / right stick: look",
        "M: music on/off",
        "F5 / F9: save / load",
        "Esc: menu   F: fullscreen",
    ];
    loop {
        devices.begin_frame_with_keyboard(&mut shell, true, unattended || platform::focused(), platform::keyboard());
        let dt = life.begin_frame(devices.frame_seconds());
        let (held, jump, look, menu, music) = match life.script() {
            Some(s) => (
                Held { forward: s.axis("fwd", "back"), right: s.axis("right", "left"), sprint: s.held("sprint") },
                s.starts("jump"),
                [s.value("look", 0), s.value("look", 1)],
                s.starts("menu"),
                s.starts("music"),
            ),
            None => {
                let m = devices.movement(&shell);
                (
                    Held { forward: m.forward, right: m.right, sprint: m.sprint },
                    m.jump,
                    devices.look_delta_with(&shell, dt),
                    false,
                    devices.pressed(KeyCode::M),
                )
            }
        };
        if menu {
            shell.paused = !shell.paused;
            life.reset_input();
        }
        if music {
            if let Err(e) = toggle_music(&mut settings, &settings_path) {
                notice = e;
            }
        }
        life.feed(held, if jump { JUMP } else { 0 }, look);
        let (save, load) = life.save_load_requested(devices.pressed(KeyCode::F5), devices.pressed(KeyCode::F9));
        if save {
            let n = life.quick_save(&sim, &format!("Leo - Day {}", sim.time().days + 1));
            notice = format!("{}: {}", n.title, n.detail);
            if unattended && !n.ok {
                fail(&notice);
            }
        }
        if load {
            let n = life.quick_load(&mut sim);
            notice = format!("{}: {}", n.title, n.detail);
            if unattended && !n.ok {
                fail(&notice);
            }
        }
        for _ in 0..life.ticks(dt, 1., !shell.paused) {
            let tick = life.take_tick();
            sim.step(&Input {
                forward: tick.held.forward,
                right: tick.held.right,
                sprint: tick.held.sprint,
                jump: tick.pressed(JUMP),
                look: tick.look,
            })
            .unwrap_or_else(|e| fail(e));
        }
        if !unattended && (sim.tick.saturating_sub(auto_tick) >= 3600 || sim.time().days != auto_day) {
            if let Err(e) = snapshot::autosave(&sim, &life.slots, 3, &format!("Leo - Day {}", sim.time().days + 1)) {
                notice = format!("Autosave failed: {e}");
            }
            auto_tick = sim.tick;
            auto_day = sim.time().days;
        }
        let day = scene::daylight(sim.time().phase);
        if let Some((nature, score)) = &mut banks {
            nature.sounds.music_volume = settings.sfx_level();
            score.sounds.music_volume = settings.music_level();
            nature
                .music(dt, &[("birds", day), ("wind", 0.28), ("night", 1. - day), ("leaves", sim.forest() * 0.6)])
                .unwrap_or_else(|e| fail(e));
            score
                .music(dt, &[("home", 0.65), ("wander", day * 0.7), ("starlight", (1. - day) * 0.8)])
                .unwrap_or_else(|e| fail(e));
            submissions += 1;
        }
        let view = scene
            .draw(
                &sim,
                life.alpha(),
                &materials,
                &mut shadows,
                scene::ViewOptions { pending_look: life.pending_look(), portrait },
            )
            .unwrap_or_else(|e| fail(e));
        let ui = hud::ui_scale();
        let phase = sim.time().phase;
        if !notice.is_empty() {
            hud::text_outlined(&notice, 24. * ui, screen_height() - 24. * ui, 15. * ui, WHITE);
        }
        let title = format!("Leo - Day {}", sim.time().days + 1);
        let outcome = shell.menu_with_audio_settings(
            &title,
            &controls,
            AudioMenu { music_on: settings.music_on, sfx_on: settings.sfx_on, has_music: true },
            false,
        );
        if outcome.toggle_music {
            if let Err(e) = toggle_music(&mut settings, &settings_path) {
                notice = e;
            }
        }
        if outcome.toggle_sfx {
            settings.toggle_sfx();
            if !settings.store(&settings_path) {
                notice = "Could not save sound setting".into();
            }
        }
        if let Some(path) = life.capture_path() {
            let result = kit::capture::save_frame(&path).map_err(|e| e.to_string());
            if let Err(e) = &result {
                fail(e);
            }
            life.captured(&path, result);
            evidence.push(serde_json::json!({"frame":life.frame(),"tick":sim.tick,"days":sim.time().days,"phase":phase,"origin":sim.origin,"local":sim.point().local,"chunks":sim.chunks.chunks().len(),"hash":format!("{:016x}",sim.state_hash()),"paused":shell.paused,"music_on":settings.music_on,"music_volume":settings.music_level(),"audio_ready":banks.is_some(),"audio_submissions":submissions,"first_person":!portrait,"camera_eye":[view.eye.x,view.eye.y,view.eye.z],"player_eye":[sim.player.position.0,sim.player.position.1,sim.player.position.2],"world_rebuilds":scene.world_rebuilds()}));
        }
        if outcome.quit || game_client::exit_requested() || life.end_frame(dt) {
            break;
        }
        next_frame().await;
    }
    if !unattended {
        if let Err(e) = snapshot::autosave(&sim, &life.slots, 3, &format!("Leo - Day {}", sim.time().days + 1)) {
            eprintln!("Leo: autosave failed: {e}");
        }
    }
    if let Some(capture) = &life.options.capture {
        let result = serde_json::json!({"schema":1,"game":"Leo","state":evidence,"asset_root":assets,"muted":muted,"audio_submissions":submissions,"shadows":shadows.quality(),"gameplay_day_overlay":false,"audibility_verified":false});
        std::fs::write(capture.dir.join("run.json"), serde_json::to_vec_pretty(&result).unwrap())
            .unwrap_or_else(|e| fail(e));
    }
    if let Some(report) = life.report() {
        println!("{report}");
    }
}
